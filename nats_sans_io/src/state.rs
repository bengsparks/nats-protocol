use std::{
    collections::{HashMap, VecDeque},
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use nats_codec::{ClientCommand, Connect, ServerCommand};
use tokio::sync::mpsc;

use crate::{subscription::SubscribeResponse, ConnectionCommand};

#[derive(Debug)]
pub enum ConnState {
    /// First message sent from a server to the client must be [INFO](nats_codec::Info).
    AwaitingInfo(AwaitingInfo),
    /// The first message received was [INFO](nats_codec::Info); connection has now been established.
    InfoReceived(InfoReceived),
    /// The first message received was not [INFO](nats_codec::Info),
    NotInfoReceived,

    // Server is not responding to `PINGs`
    ConnectionLost,
}

#[derive(Debug)]
pub struct AwaitingInfo {
    pub preliminary: Vec<(ConnectionCommand, Instant)>,
}

#[derive(Debug)]
pub struct InfoReceived {
    pub buffered_transmits: VecDeque<nats_codec::ClientCommand>,
    pub keep_alive: KeepAliveState,
    pub sid_generator: AtomicU64,
    pub sid2subscriber: HashMap<String, mpsc::Sender<nats_codec::Message>>,
}

#[derive(Clone, Debug, Default)]
pub struct KeepAliveState {
    /// Keep-Alive Client: when did the client last check that the server is still alive?
    pub last_ping_sent_at: Option<Instant>,

    /// Keep-Alive Server: when did the server last check that the client is still alive?
    pub last_ping_received_at: Option<Instant>,

    /// Keep-Alive Server: when did the server last indicate that it is still alive?
    pub last_pong_received_at: Option<Instant>,
}

pub trait Step<Command> {
    fn step(&mut self, command: Command, now: Instant) -> Option<ConnState>;
}

impl Step<ServerCommand> for ConnState {
    fn step(&mut self, command: ServerCommand, now: Instant) -> Option<ConnState> {
        let new_state = match (self, command) {
            (ConnState::AwaitingInfo(AwaitingInfo { preliminary }), ServerCommand::Info(_info)) => {
                // https://doc.rust-lang.org/std/collections/struct.VecDeque.html#method.from
                let preliminary = std::mem::take(preliminary);

                let mut buffered_transmits: VecDeque<_> =
                    VecDeque::with_capacity(preliminary.len());

                buffered_transmits.push_front(ClientCommand::Connect(Connect {
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

                let s = ConnState::InfoReceived(InfoReceived {
                    buffered_transmits,
                    keep_alive: KeepAliveState {
                        last_ping_sent_at: None,
                        last_ping_received_at: None,
                        last_pong_received_at: None,
                    },
                    sid_generator: AtomicU64::new(1),
                    sid2subscriber: HashMap::new(),
                });

                let replayed = preliminary.into_iter().fold(s, |mut s, (backlog, _when)| {
                    s.step(backlog, now).unwrap_or(s)
                });

                Some(replayed)
            }
            (ConnState::AwaitingInfo { .. }, _otherwise) => Some(ConnState::NotInfoReceived),

            // Connection upheld
            (ConnState::InfoReceived(_inner), ServerCommand::Ok) => {
                log::trace!("Received OK");
                None
            }
            (ConnState::InfoReceived(_inner), ServerCommand::Err(error)) => {
                log::error!("Received error!: {error}");
                None
            }
            (ConnState::InfoReceived(_inner), ServerCommand::Info(_info)) => {
                log::warn!("Received new `INFO` during already established connection");
                None
            }
            (
                ConnState::InfoReceived(InfoReceived { sid2subscriber, .. }),
                ServerCommand::Msg(message),
            ) => {
                log::trace!("Received message: {message:?}");
                let Some(subscriber) = sid2subscriber.get(&message.sid) else {
                    log::warn!("Subscriber with SID {} is unknown", message.sid);
                    return None;
                };

                subscriber.try_send(message.into()).unwrap();
                None
            }
            (
                ConnState::InfoReceived(InfoReceived { sid2subscriber, .. }),
                ServerCommand::HMsg(message),
            ) => {
                log::trace!("Received message with headers: {message:?}");
                let Some(subscriber) = sid2subscriber.get(&message.sid) else {
                    log::warn!("Subscriber with SID {} is unknown", message.sid);
                    return None;
                };

                subscriber.blocking_send(message.into()).unwrap();
                None
            }
            (ConnState::InfoReceived(inner), ServerCommand::Ping) => {
                log::trace!("Received `PING`");
                inner.keep_alive.last_ping_received_at = Some(now);
                None
            }
            (ConnState::InfoReceived(inner), ServerCommand::Pong) => {
                log::trace!("Received `PONG`");
                inner.keep_alive.last_pong_received_at = Some(now);
                None
            }
            (ConnState::NotInfoReceived, otherwise) => {
                log::error!("Received {otherwise:?} despite not having connected!");
                None
            }
            (ConnState::ConnectionLost, otherwise) => {
                log::error!("Received {otherwise:?} despite having lost the connection!");
                None
            }
        };

        new_state
    }
}

impl Step<ConnectionCommand> for ConnState {
    fn step(&mut self, command: ConnectionCommand, now: Instant) -> Option<ConnState> {
        match (self, command) {
            (
                ConnState::InfoReceived(InfoReceived {
                    buffered_transmits,
                    sid_generator,
                    sid2subscriber,
                    ..
                }),
                ConnectionCommand::Subscribe {
                    subject,
                    options,
                    sender,
                },
            ) => {
                let sid = sid_generator.fetch_add(1, Ordering::Relaxed);
                let (sub_send, sub_recv) = mpsc::channel(1024);

                sid2subscriber.insert(sid.to_string(), sub_send);

                sender
                    .send(SubscribeResponse {
                        sid: sid.to_string(),
                        msg_chan: sub_recv,
                        max_msgs: options.max_msgs,
                    })
                    .unwrap();

                buffered_transmits.push_back(ClientCommand::Subscribe(nats_codec::Subscribe {
                    subject,
                    queue_group: options.queue_group,
                    sid: sid.to_string(),
                }));
                if let Some(max_msgs) = options.max_msgs {
                    buffered_transmits.push_back(ClientCommand::Unsubscribe(
                        nats_codec::Unsubscribe {
                            sid: sid.to_string(),
                            max_msgs: Some(max_msgs),
                        },
                    ))
                }
            }
            (
                ConnState::InfoReceived(InfoReceived {
                    buffered_transmits,
                    sid2subscriber,
                    ..
                }),
                ConnectionCommand::Unsubscribe { sid, max_msgs },
            ) => {
                let subscriber = sid2subscriber.remove(&sid);
                if subscriber.is_none() {
                    log::warn!("Cannot unsubscribe {sid} because it is unknown");
                }
                buffered_transmits.push_back(ClientCommand::Unsubscribe(nats_codec::Unsubscribe {
                    sid,
                    max_msgs,
                }));
            }
            (
                ConnState::InfoReceived(InfoReceived {
                    buffered_transmits, ..
                }),
                ConnectionCommand::Publish { subject, payload },
            ) => buffered_transmits.push_back(ClientCommand::Publish(nats_codec::Publish {
                subject,
                reply_to: None,
                bytes: payload.len(),
                payload,
            })),
            (ConnState::AwaitingInfo(AwaitingInfo { preliminary }), command) => {
                preliminary.push((command, now));
            }
            (ConnState::NotInfoReceived | ConnState::ConnectionLost, command) => {
                log::error!("Discarding {command:?}; protocol error occurred");
            }
        }

        None
    }
}
