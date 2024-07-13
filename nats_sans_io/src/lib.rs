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
use nats_codec::{ClientCommand, ServerCommand};
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
    /// How long should the elapsed period between receiving `PING` and enqueueing `PONG` be.
    pub pong_delay: Duration,
    /// How often should a `PONG` be received from the server.
    pub pong_interval: Duration,
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

    /// What happens when [Self::poll_recv_pong_timeout]'s timestamp is exceeded.
    pub fn handle_recv_pong_timeout(&mut self, now: Instant) {
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
            Some(received_at) if now.duration_since(received_at) >= timeouts.pong_interval => {
                log::error!(
                    "Connection lost! It has been over {}s since the NATS server sent a PONG",
                    timeouts.pong_interval.as_secs_f64()
                );
                *conn_state = ConnState::ConnectionLost
            }
            _ => {}
        };
    }

    /// Returns the timestamp when we next expect [Self::handle_recv_ping_timeout] to be called.
    /// i.e. When should the server have sent its next PONG by.
    pub fn poll_recv_pong_timeout(&self) -> Option<Instant> {
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
            .map(|i| i + timeouts.pong_interval)
    }
}

#[cfg(test)]
fn info() -> Box<nats_codec::Info> {
    serde_json::from_str(r#"{"server_id":"NC5WKM2NEXZZYVBSLD24PDKRCMRXZXSMBIYC3VLG7YS5RSD7ERST3OS4","server_name":"us-south-nats-demo","version":"2.10.17","proto":1,"git_commit":"b91de03","go":"go1.22.4","host":"0.0.0.0","port":4222,"headers":true,"tls_available":true,"max_payload":1048576,"jetstream":true,"client_id":710058,"client_ip":"176.199.209.34","nonce":"WnZZsP2OjHY8YwU","xkey":"XAHQDFJMDUWCMLSZC6U5REONIGLFHANVWQLZRSFLVBMC5RSUSGHSF5EC"}"#).unwrap()
}

#[test]
fn timeouts() {
    let ping_interval = Duration::from_secs(2);
    let pong_interval = Duration::from_secs(3);
    let pong_delay = Duration::from_secs(5);

    let mut binding = NatsBinding::new(Timeouts {
        ping_interval: ping_interval.clone(),
        pong_interval: pong_interval.clone(),
        pong_delay: pong_delay.clone(),
    });
    assert!(matches!(
        binding.state.conn_state,
        ConnState::AwaitingInfo(_)
    ));

    // Tick 0 - setup
    let tick = Instant::now();
    binding.handle_server_input(ServerCommand::Info(info()), tick);
    assert!(matches!(
        binding.state.conn_state,
        ConnState::InfoReceived(_)
    ));
    assert!(matches!(
        binding.poll_transmit(),
        Some(ClientCommand::Connect(_))
    ));
    assert_eq!(binding.poll_transmit(), None);

    let now = Instant::now();

    // Tick 1 - Enqueue PING, nothing else
    {
        let tick = now + Duration::from_secs(1);

        // No PING sent so far, but first one is always sent
        assert_eq!(binding.poll_send_ping_timeout(tick), Some(tick));

        binding.handle_send_ping_timeout(tick); // <-- Enqueues

        // No modifications should happen here
        binding.handle_send_pong_timeout(tick);
        binding.handle_recv_pong_timeout(tick);

        // Tick 1 - Commands
        assert_eq!(binding.poll_transmit(), Some(ClientCommand::Ping));
        assert_eq!(binding.poll_transmit(), None);
    }

    // Tick 2 - Receive PING, Enqueue PONG within given interval
    {
        let tick = now + Duration::from_secs(2);
        binding.handle_server_input(ServerCommand::Ping, tick);

        // Expect to send PING in upcoming tick
        assert_eq!(
            binding.poll_send_ping_timeout(tick),
            Some(tick + Duration::from_secs(1))
        );

        // PING received, expect in upcoming
        assert_eq!(binding.poll_send_pong_timeout(), Some(tick + pong_delay));

        binding.handle_send_ping_timeout(tick);
        binding.handle_send_pong_timeout(tick);
        binding.handle_recv_pong_timeout(tick);

        // Enqueued Commands
        assert_eq!(binding.poll_transmit(), None);
    }

    // Tick 3 - Enqueue PING
    {
        let tick = now + Duration::from_secs(3);

        // Expect to send PING now
        assert_eq!(binding.poll_send_ping_timeout(tick), Some(tick));
        binding.handle_send_ping_timeout(tick);

        // Do nothing
        binding.handle_send_ping_timeout(tick);
        binding.handle_send_pong_timeout(tick);
        binding.handle_recv_pong_timeout(tick);

        // Enqueued Commands
        assert_eq!(binding.poll_transmit(), Some(ClientCommand::Ping));
        assert_eq!(binding.poll_transmit(), None);
    }

    // Tick 7
    // 1. PING was last sent in tick 3, interval is 2 => capture delayed PING!
    // 2. PING was received in tick 2, delay is 5, send PONG now
    {
        let tick = now + Duration::from_secs(7);

        // Detect delayed PING
        assert_eq!(
            binding.poll_send_ping_timeout(tick),
            Some(tick - Duration::from_secs(2))
        );
        // Detect punctual PONG
        assert_eq!(binding.poll_send_pong_timeout(), Some(tick));

        binding.handle_send_ping_timeout(tick);
        binding.handle_recv_pong_timeout(tick);
        binding.handle_send_pong_timeout(tick);

        // Enqueued Commands
        assert_eq!(binding.poll_transmit(), Some(ClientCommand::Ping));
        assert_eq!(binding.poll_transmit(), Some(ClientCommand::Pong));
        assert_eq!(binding.poll_transmit(), None);
    }

    // Capture Connection Lost due to not receiving PONG soon enough.
    /*
    {
        let tick = now + (Duration::from_secs(7 + 1) + pong_interval);

        assert_eq!(binding.poll_recv_pong_timeout(), Some(tick - Duration::from_secs(1)));

        binding.handle_send_ping_timeout(tick);
        binding.handle_recv_pong_timeout(tick);
        binding.handle_send_pong_timeout(tick);

        assert!(matches!(
            binding.state.conn_state,
            ConnState::ConnectionLost
        ))
    }
    */
}

#[cfg(test)]
struct StepExpectations {}

#[cfg(test)]
fn ok_step(binding: &mut NatsBinding, start: Instant, tick: Duration) {
    let now = start + tick;
}
