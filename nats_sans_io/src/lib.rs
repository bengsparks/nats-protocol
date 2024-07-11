use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use logcall::logcall;
use nats_codec::{ClientCommand, Connect, ServerCommand};

#[derive(Debug)]

struct State {
    conn_state: ConnState,
    keep_alive: KeepAliveState,
    timeouts: Timeouts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnState {
    /// First message sent from a server to the client must be [INFO](nats_codec::Info).
    AwaitingInfo,
    /// The first message received was [INFO](nats_codec::Info).
    InfoReceived,
    /// The first message received was not [INFO](nats_codec::Info),
    NotInfoReceived,
    // Server is not responding to `PINGs`
    ConnectionLost,
}

#[derive(Debug)]
struct KeepAliveState {
    /// Keep-Alive Client: when did the client last check that the server is still alive?
    last_ping_sent_at: Option<Instant>,

    /// Keep-Alive Server: when did the server last check that the client is still alive?
    last_ping_received_at: Option<Instant>,

    /// Keep-Alive Server: when did the server last indicate that it is still alive?
    last_pong_received_at: Option<Instant>,
}

#[derive(Debug)]
pub struct Timeouts {
    /// How often should a `PING` be sent.
    pub ping_interval: Duration,
    /// How long should the elapsed period between receiving `PING` and enqueueing `PONG` be.
    pub pong_delay: Duration,
    /// How often should a `PONG` be received from the server.
    pub pong_interval: Duration,
}

#[derive(Debug)]
pub struct NatsBinding {
    state: State,
    buffered_transmits: VecDeque<ClientCommand>,
}

impl NatsBinding {
    pub fn new(timeouts: Timeouts) -> Self {
        Self {
            state: State {
                conn_state: ConnState::AwaitingInfo,
                keep_alive: KeepAliveState {
                    last_ping_sent_at: None,
                    last_pong_received_at: None,
                    last_ping_received_at: None,
                },
                timeouts,
            },
            buffered_transmits: VecDeque::new(),
        }
    }

    pub fn handle_input(&mut self, command: ServerCommand, now: Instant) {
        let State {
            conn_state: state,
            keep_alive,
            ..
        } = &mut self.state;

        match (&state, command) {
            // Awaiting first command from server
            (ConnState::AwaitingInfo, ServerCommand::Info(_info)) => {
                self.buffered_transmits
                    .push_back(ClientCommand::Connect(Connect {
                        // From INFO
                        sig: None,
                        nkey: None,
                        // Hardcoded
                        verbose: true,
                        pedantic: true,
                        tls_required: false,
                        auth_token: None,
                        user: None,
                        lang: "Rust".into(),
                        name: None,
                        pass: None,
                        version: "1.0".into(),
                        protocol: None,
                        echo: None,
                        jwt: None,
                        no_responders: None,
                        headers: Some(true),
                    }));
                *state = ConnState::InfoReceived;
            }
            (ConnState::AwaitingInfo, _otherwise) => {
                *state = ConnState::NotInfoReceived;
            }

            // Connection upheld
            (ConnState::InfoReceived, ServerCommand::Ok) => {
                log::trace!("Received OK");
            }
            (ConnState::InfoReceived, ServerCommand::Err(error)) => {
                log::error!("Received error!: {error}");
            }
            (ConnState::InfoReceived, ServerCommand::Info(_info)) => {
                log::warn!("Received new `INFO` during already established connection")
            }
            (ConnState::InfoReceived, ServerCommand::Msg(message)) => {
                log::trace!("Received message: {message:?}");
            }
            (ConnState::InfoReceived, ServerCommand::HMsg(message)) => {
                log::trace!("Received message with headers: {message:?}");
            }
            (ConnState::InfoReceived, ServerCommand::Ping) => {
                log::trace!("Received `PING`");
                keep_alive.last_ping_received_at = Some(now);
            }
            (ConnState::InfoReceived, ServerCommand::Pong) => {
                log::trace!("Received `PONG`");
                keep_alive.last_pong_received_at = Some(now);
            }
            (ConnState::NotInfoReceived, otherwise) => {
                log::error!("Received {otherwise:?} despite not having connected!");
            }
            (ConnState::ConnectionLost, otherwise) => {
                log::error!("Received {otherwise:?} despite having lost the connection!");
            }
        };
    }

    pub fn poll_transmit(&mut self) -> Option<ClientCommand> {
        self.buffered_transmits.pop_front()
    }

    /// What happens when [Self::poll_send_ping_timeout]'s timestamp is exceeded
    #[logcall(err = "error")]
    pub fn handle_send_ping_timeout(&mut self, now: Instant) -> Result<(), ConnState> {
        let State {
            keep_alive,
            timeouts,
            conn_state,
        } = &mut self.state;

        let ConnState::InfoReceived = conn_state else {
            return Err(conn_state.clone());
        };

        let command = match keep_alive.last_ping_sent_at {
            Some(received_at) if now.duration_since(received_at) >= timeouts.ping_interval => {
                Some(ClientCommand::Ping)
            }
            None => Some(ClientCommand::Ping),
            _ => None,
        };

        if let Some(command) = command {
            self.buffered_transmits.push_back(command);
            keep_alive.last_ping_sent_at = Some(now);
            log::trace!("Enqueued `PING`");
        }

        Ok(())
    }

    /// Returns the timestamp when we next expect [Self::handle_send_ping_timeout] to be called.
    /// i.e. When should the next `PING` be sent.
    pub fn poll_send_ping_timeout(&self, now: Instant) -> Option<Instant> {
        let State {
            keep_alive,
            timeouts,
            conn_state,
        } = &self.state;

        let ConnState::InfoReceived = conn_state else {
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
    #[logcall(err = "error")]
    pub fn handle_send_pong_timeout(&mut self, now: Instant) -> Result<(), ConnState> {
        let State {
            keep_alive,
            timeouts,
            conn_state,
        } = &mut self.state;

        let ConnState::InfoReceived = conn_state else {
            return Err(conn_state.clone());
        };

        let command = match keep_alive.last_ping_received_at {
            Some(received_at) if now.duration_since(received_at) >= timeouts.pong_delay => {
                Some(ClientCommand::Pong)
            }
            _ => None,
        };

        if let Some(pong) = command {
            self.buffered_transmits.push_back(pong);
            keep_alive.last_ping_received_at = None;
            log::trace!("Enqueued `PONG`");
        }

        Ok(())
    }

    /// Returns the timestamp when we next expect [Self::handle_recv_ping_timeout] to be called.
    /// i.e. When should the next `PONG` be sent.
    pub fn poll_send_pong_timeout(&self) -> Option<Instant> {
        let State {
            keep_alive,
            timeouts,
            conn_state,
        } = &self.state;

        let ConnState::InfoReceived = conn_state else {
            return None;
        };
        keep_alive
            .last_ping_received_at
            .map(|i| i + timeouts.pong_delay)
    }

    /// What happens when [Self::poll_recv_pong_timeout]'s timestamp is exceeded.
    #[logcall(err = "error")]
    pub fn handle_recv_pong_timeout(&mut self, now: Instant) -> Result<(), ConnState> {
        let State {
            conn_state,
            keep_alive,
            timeouts,
        } = &mut self.state;

        let ConnState::InfoReceived = conn_state else {
            return Err(conn_state.clone());
        };

        let state_change = match keep_alive.last_pong_received_at {
            Some(received_at) if now.duration_since(received_at) >= timeouts.pong_interval => {
                Some(ConnState::ConnectionLost)
            }
            _ => None,
        };

        if let Some(change) = state_change {
            *conn_state = change;
        }
        Ok(())
    }

    /// Returns the timestamp when we next expect [Self::handle_recv_ping_timeout] to be called.
    /// i.e. When should the server have sent its next PONG by.
    pub fn poll_recv_pong_timeout(&self) -> Option<Instant> {
        let State {
            keep_alive,
            timeouts,
            conn_state,
        } = &self.state;

        let ConnState::InfoReceived = conn_state else {
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
    assert_eq!(binding.state.conn_state, ConnState::AwaitingInfo);

    // Tick 0 - setup
    let tick = Instant::now();
    binding.handle_input(ServerCommand::Info(info()), tick);
    assert_eq!(binding.state.conn_state, ConnState::InfoReceived);
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
        assert!(binding.handle_send_ping_timeout(tick).is_ok()); // <-- Enqueues

        // No modifications should happen here
        assert!(binding.handle_send_pong_timeout(tick).is_ok());
        assert!(binding.handle_recv_pong_timeout(tick).is_ok());

        // Tick 1 - Commands
        assert_eq!(binding.poll_transmit(), Some(ClientCommand::Ping));
        assert_eq!(binding.poll_transmit(), None);
    }

    // Tick 2 - Receive PING, Enqueue PONG within given interval
    {
        let tick = now + Duration::from_secs(2);
        binding.handle_input(ServerCommand::Ping, tick);

        // Expect to send PING in upcoming tick
        assert_eq!(
            binding.poll_send_ping_timeout(tick),
            Some(tick + Duration::from_secs(1))
        );
        assert!(binding.handle_send_ping_timeout(tick).is_ok());

        // PING received, expect in upcoming
        assert_eq!(binding.poll_send_pong_timeout(), Some(tick + pong_delay));
        assert!(binding.handle_send_pong_timeout(tick).is_ok());

        // Do nothing
        assert!(binding.handle_recv_pong_timeout(tick).is_ok());

        // Enqueued Commands
        assert_eq!(binding.poll_transmit(), None);
    }

    // Tick 3 - Enqueue PING
    {
        let tick = now + Duration::from_secs(3);

        // Expect to send PING now
        assert_eq!(binding.poll_send_ping_timeout(tick), Some(tick));
        assert!(binding.handle_send_ping_timeout(tick).is_ok());

        // Do nothing
        assert!(binding.handle_send_pong_timeout(tick).is_ok());
        assert!(binding.handle_recv_pong_timeout(tick).is_ok());

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
        assert_eq!(binding.poll_send_ping_timeout(tick), Some(tick - Duration::from_secs(2)));
        // Detect punctual PONG
        assert_eq!(binding.poll_send_pong_timeout(), Some(tick));

        assert!(binding.handle_send_ping_timeout(tick).is_ok());
        assert!(binding.handle_recv_pong_timeout(tick).is_ok());
        assert!(binding.handle_send_pong_timeout(tick).is_ok());

        // Enqueued Commands
        assert_eq!(binding.poll_transmit(), Some(ClientCommand::Ping));
        assert_eq!(binding.poll_transmit(), Some(ClientCommand::Pong));
        assert_eq!(binding.poll_transmit(), None);
    }
}
