mod state;
mod subscription;

pub use state::ConnState;
pub use subscription::{SubscribeResponse, SubscriptionOptions};

use state::{AwaitingInfo, InfoReceived, Step};

use std::{
    num::NonZeroUsize,
    time::{Duration, Instant},
};

use bytes::Bytes;
use nats_codec::{ClientCommand, Publish, ServerCommand};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug)]
struct State {
    conn_state: ConnState,
    timeouts: Timeouts,
}

#[derive(Clone, Copy, Debug)]
pub struct Timeouts {
    /// How often should a `PING` be sent.
    pub ping_interval: Duration,
    /// How long should the minimum elapsed period between receiving `PING` and enqueueing `PONG` be.
    pub pong_delay: Duration,
    /// How long should the maximum elapsed period between receiving `PONG`s from the server.
    /// Should be larger than [Self::ping_interval], as each `PONG` stems from a `PING`, i.e. you cannot receive
    /// `PONG` more often / sooner than a `PING`
    pub keep_alive: Duration,
}

#[derive(Debug)]
pub enum ConnectionCommand {
    Subscribe {
        subject: String,
        options: SubscriptionOptions,

        // `oneshot::Sender::send` is a synchronous function, which makes this ok :)
        sender: oneshot::Sender<SubscribeResponse>,
    },
    Unsubscribe {
        sid: String,
        max_msgs: Option<NonZeroUsize>,
    },
    Publish {
        subject: String,
        payload: Bytes,
    },
}

#[derive(Debug)]
pub struct NatsBinding {
    state: State,
}

#[derive(Debug)]
pub struct ConnectionHandle {
    pub chan: mpsc::Sender<ConnectionCommand>,
}

impl NatsBinding {
    pub fn new(timeouts: Timeouts) -> Self {
        let state = State {
            conn_state: ConnState::AwaitingInfo(AwaitingInfo {
                preliminary: vec![],
            }),
            timeouts,
        };

        Self { state }
    }

    pub fn handle_server_input(&mut self, command: ServerCommand, now: Instant) {
        if let Some(change) = self.state.conn_state.step(command, now) {
            self.state.conn_state = change;
        }
    }

    pub fn handle_client_input(&mut self, command: ConnectionCommand, now: Instant) {
        if let Some(change) = self.state.conn_state.step(command, now) {
            self.state.conn_state = change;
        }
    }

    pub fn poll_transmit(&mut self) -> Option<ClientCommand> {
        let ConnState::InfoReceived(InfoReceived {
            buffered_transmits, ..
        }) = &mut self.state.conn_state
        else {
            return None;
        };
        let command = buffered_transmits.pop_front();
        log::trace!("Polled {command:?}");
        command
    }

    /// What happens when [Self::poll_send_ping_timeout]'s timestamp is exceeded
    pub fn handle_send_ping_timeout(&mut self, now: Instant) {
        let State {
            timeouts,
            conn_state,
            ..
        } = &mut self.state;

        let ConnState::InfoReceived(InfoReceived {
            buffered_transmits,
            keep_alive,
            ..
        }) = conn_state
        else {
            return;
        };

        let command = match keep_alive.last_ping_sent_at {
            Some(received_at) if now.duration_since(received_at) >= timeouts.ping_interval => {
                Some(ClientCommand::Ping)
            }
            None => Some(ClientCommand::Ping),
            _ => None,
        };

        if let Some(command) = command {
            buffered_transmits.push_back(command);
            keep_alive.last_ping_sent_at = Some(now);
            log::trace!("Enqueued `PING`");
        }
    }

    /// Returns the timestamp when we next expect [Self::handle_send_ping_timeout] to be called.
    /// i.e. When should the next `PING` be sent.
    pub fn poll_send_ping_timeout(&self, now: Instant) -> Option<Instant> {
        let State {
            timeouts,
            conn_state,
            ..
        } = &self.state;

        let ConnState::InfoReceived(InfoReceived { keep_alive, .. }) = conn_state else {
            return None;
        };

        match keep_alive.last_ping_sent_at {
            // Ping has been previously sent
            Some(sent_at) => Some(sent_at + timeouts.ping_interval),
            // Ping has never been sent; now is the time!
            None => Some(now),
        }
    }

    /// What happens when [Self::poll_recv_ping_timeout]'s timestamp is exceeded.
    pub fn handle_send_pong_timeout(&mut self, now: Instant) {
        let State {
            timeouts,
            conn_state,
            ..
        } = &mut self.state;

        let ConnState::InfoReceived(InfoReceived {
            buffered_transmits,
            keep_alive,
            ..
        }) = conn_state
        else {
            return;
        };

        let command = match keep_alive.last_ping_received_at {
            Some(received_at) if now.duration_since(received_at) >= timeouts.pong_delay => {
                Some(ClientCommand::Pong)
            }
            _ => None,
        };

        if let Some(pong) = command {
            buffered_transmits.push_back(pong);
            keep_alive.last_ping_received_at = None;
            log::trace!("Enqueued `PONG`");
        }
    }

    /// Returns the timestamp when we next expect [Self::handle_recv_ping_timeout] to be called.
    /// i.e. When should the next `PONG` be sent.
    pub fn poll_send_pong_timeout(&self) -> Option<Instant> {
        let State {
            timeouts,
            conn_state,
            ..
        } = &self.state;

        let ConnState::InfoReceived(InfoReceived { keep_alive, .. }) = conn_state else {
            return None;
        };
        keep_alive
            .last_ping_received_at
            .map(|i| i + timeouts.pong_delay)
    }

    /// What happens when [Self::poll_keep_alive_timeout]'s timestamp is exceeded.
    pub fn handle_keep_alive_timeout(&mut self, now: Instant) {
        let State {
            conn_state,
            timeouts,
            ..
        } = &mut self.state;

        let ConnState::InfoReceived(InfoReceived { keep_alive, .. }) = conn_state else {
            return;
        };

        match keep_alive.last_pong_received_at {
            // Please stabilise if-let chains soon :)
            Some(received_at) if now.duration_since(received_at) >= timeouts.keep_alive => {
                log::error!(
                    "Connection lost! It has been over {}s since the NATS server sent a PONG",
                    timeouts.keep_alive.as_secs_f64()
                );
                *conn_state = ConnState::ConnectionLost
            }
            _ => {}
        };
    }

    /// Returns the timestamp when we next expect [Self::handle_keep_alive_timeout] to be called.
    /// i.e. When should the server have sent its next PONG by.
    pub fn poll_keep_alive_timeout(&self) -> Option<Instant> {
        let State {
            timeouts,
            conn_state,
            ..
        } = &self.state;

        let ConnState::InfoReceived(InfoReceived { keep_alive, .. }) = conn_state else {
            return None;
        };

        keep_alive
            .last_pong_received_at
            .map(|i| i + timeouts.keep_alive)
    }
}

#[cfg(test)]
fn info() -> Box<nats_codec::Info> {
    serde_json::from_str(r#"{"server_id":"NC5WKM2NEXZZYVBSLD24PDKRCMRXZXSMBIYC3VLG7YS5RSD7ERST3OS4","server_name":"us-south-nats-demo","version":"2.10.17","proto":1,"git_commit":"b91de03","go":"go1.22.4","host":"0.0.0.0","port":4222,"headers":true,"tls_available":true,"max_payload":1048576,"jetstream":true,"client_id":710058,"client_ip":"176.199.209.34","nonce":"WnZZsP2OjHY8YwU","xkey":"XAHQDFJMDUWCMLSZC6U5REONIGLFHANVWQLZRSFLVBMC5RSUSGHSF5EC"}"#).unwrap()
}

#[test]
fn timeouts() {
    // How often do we send PING
    let ping_interval = Duration::from_secs(2);
    // How long do we allow to wait for sending PONG in response to PING
    let pong_delay = Duration::from_secs(0);
    // How long do we wait for receiving a PONG
    let keep_alive = Duration::from_secs(3);

    // Tick 0 - setup
    let now = Instant::now();

    let mut binding = NatsBinding::new(Timeouts {
        ping_interval: ping_interval.clone(),
        pong_delay: pong_delay.clone(),
        keep_alive: keep_alive.clone(),
    });
    assert!(matches!(
        binding.state.conn_state,
        ConnState::AwaitingInfo(_)
    ));

    {
        let tick = now + Duration::from_secs(0);
        step(
            &mut binding,
            tick,
            StepExpectations {
                poll_send_ping: None,
                poll_send_pong: None,
                poll_keep_alive: None,
                //enqueued: vec![ClientCommand::Connect(_)],
            },
        );

        assert_eq!(binding.poll_transmit(), None);
    }

    // Tick 1 - Receive INFO, PING tick is also triggered
    {
        let tick = now + Duration::from_secs(1);
        binding.handle_server_input(ServerCommand::Info(info()), tick);

        step(
            &mut binding,
            tick,
            StepExpectations {
                poll_send_ping: Some(tick),
                poll_send_pong: None,
                poll_keep_alive: None,
            },
        );

        assert!(matches!(
            binding.poll_transmit(),
            Some(ClientCommand::Connect(_))
        ));
        assert_eq!(binding.poll_transmit(), Some(ClientCommand::Ping));
        assert_eq!(binding.poll_transmit(), None);
    }

    // Tick 2 - Receive PONG
    {
        let tick = now + Duration::from_secs(2);
        binding.handle_server_input(ServerCommand::Pong, tick);

        step(
            &mut binding,
            tick,
            StepExpectations {
                // Send PING based on last tick + interval
                poll_send_ping: Some(tick + ping_interval - Duration::from_secs(1)),
                poll_send_pong: None,
                // Expect next PONG within given interval
                poll_keep_alive: Some(tick + keep_alive),
            },
        );

        assert_eq!(binding.poll_transmit(), None);
    }

    // Tick 3 - PING was sent in tick 1, so we expect it now.
    // Also, receive PING from server at this time to later send PONG
    {
        let tick = now + Duration::from_secs(3);
        binding.handle_server_input(ServerCommand::Ping, tick);

        step(
            &mut binding,
            tick,
            StepExpectations {
                poll_send_ping: Some(tick),
                poll_send_pong: Some(tick), // Receiving PING immediately starts send PONG
                poll_keep_alive: Some(tick + keep_alive - Duration::from_secs(1)),
            },
        );

        assert_eq!(binding.poll_transmit(), Some(ClientCommand::Ping));
        assert_eq!(binding.poll_transmit(), Some(ClientCommand::Pong));
        assert_eq!(binding.poll_transmit(), None);
    }

    // Tick 7 - PING was sent in tick 3, and we do not receive a PONG for > 3 ticks, therefore the connection is "lost"
    {
        let tick = now + Duration::from_secs(7);

        step(
            &mut binding,
            tick,
            StepExpectations {
                poll_send_ping: Some(tick - Duration::from_secs(2)), // <-- overdue PING
                poll_send_pong: None,                                // <--- No PING has been sent
                poll_keep_alive: Some(tick + keep_alive - Duration::from_secs(5)), // <-- no PONG has been received!
            },
        );

        assert_eq!(binding.poll_transmit(), None);
        assert!(matches!(
            binding.state.conn_state,
            ConnState::ConnectionLost
        ));
    }
}

#[test]
fn retain_preemptive_messages() {
    let tick = Instant::now();

    let mut binding = NatsBinding::new(Timeouts {
        ping_interval: Duration::from_secs(10),
        pong_delay: Duration::from_secs(10),
        keep_alive: Duration::from_secs(10),
    });
    assert!(matches!(
        binding.state.conn_state,
        ConnState::AwaitingInfo(_)
    ));

    binding.handle_client_input(
        ConnectionCommand::Publish {
            subject: "preemptive".into(),
            payload: Bytes::from_static(b"Hello World!"),
        },
        tick,
    );

    let (sender, mut receiver) = oneshot::channel();
    binding.handle_client_input(
        ConnectionCommand::Subscribe {
            subject: "preemptive".into(),
            options: SubscriptionOptions {
                max_msgs: NonZeroUsize::new(5),
                queue_group: None,
            },
            sender,
        },
        tick,
    );

    binding.handle_server_input(ServerCommand::Info(info()), tick);
    assert!(matches!(
        binding.state.conn_state,
        ConnState::InfoReceived(_)
    ));

    assert!(matches!(
        binding.poll_transmit(),
        Some(ClientCommand::Connect(_))
    ));
    assert_eq!(
        binding.poll_transmit(),
        Some(ClientCommand::Publish(Publish {
            subject: "preemptive".into(),
            reply_to: None,
            bytes: "Hello World!".len(),
            payload: Bytes::from_static(b"Hello World!")
        }))
    );
    assert!(matches!(
        binding.poll_transmit(),
        Some(ClientCommand::Subscribe(_))
    ));
    assert!(matches!(
        binding.poll_transmit(),
        Some(ClientCommand::Unsubscribe(_))
    ));
    assert!(matches!(binding.poll_transmit(), None));

    let _response = receiver.try_recv().unwrap();
}

#[cfg(test)]
struct StepExpectations {
    poll_send_ping: Option<Instant>,
    poll_send_pong: Option<Instant>,
    poll_keep_alive: Option<Instant>,
    // enqueued: Vec<ClientCommand>,
}

#[cfg(test)]
fn step(binding: &mut NatsBinding, tick: Instant, expectations: StepExpectations) {
    assert_eq!(
        binding.poll_send_ping_timeout(tick),
        expectations.poll_send_ping,
        "Wrong PING timeout"
    );
    assert_eq!(
        binding.poll_send_pong_timeout(),
        expectations.poll_send_pong,
        "Wrong PONG timeout"
    );
    assert_eq!(
        binding.poll_keep_alive_timeout(),
        expectations.poll_keep_alive,
        "Wrong Keep-Alive timeout!"
    );

    binding.handle_send_ping_timeout(tick);
    binding.handle_send_pong_timeout(tick);
    binding.handle_keep_alive_timeout(tick);

    /*
    let mut msgs = expectations.enqueued.into_iter();

    while let Some((enqueued, expected)) = binding.poll_transmit().zip(msgs.next()) {
        assert_eq!(enqueued, expected);
    }

    assert_eq!(binding.poll_transmit(), None);
    assert_eq!(msgs.next(), None);
    */
}
