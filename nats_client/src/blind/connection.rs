use futures::{SinkExt as _, StreamExt, TryStreamExt};
use nats_codec::{
    ClientCodec, ClientCommand, Connect, Message, Publish, ServerCodec, ServerCommand, Subscribe,
    Unsubscribe,
};

use tokio::io::{BufReader, BufWriter};
use tokio::net::{
    tcp::{OwnedReadHalf, OwnedWriteHalf},
    TcpStream,
};
use tokio::sync::{mpsc, oneshot, RwLock};

use tokio_stream::wrappers::ReceiverStream;
use tokio_util::codec::{FramedRead, FramedWrite};

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::{collections::HashMap, net::SocketAddr};

pub use crate::blind::{Subscriber, SubscriptionOptions};

#[derive(Debug)]
pub struct SubscriptionRequest {
    subject: String,
    options: SubscriptionOptions,
    channels: (mpsc::Sender<Message>, mpsc::Receiver<Message>),
    sub_chan: oneshot::Sender<Subscriber>,
}

#[derive(Debug)]
pub enum ConnectionCommand {
    Publish(nats_codec::Publish),
    Subscribe(SubscriptionRequest),
    Unsubscribe(nats_codec::Unsubscribe),
    Ping,
    Pong,
}
struct ReadState {
    reader: BufReader<OwnedReadHalf>,
    channel: mpsc::Sender<ConnectionCommand>,
}

struct WriteState {
    writer: BufWriter<OwnedWriteHalf>,
    channel: mpsc::Receiver<ClientCommand>,
}

struct ConnectionState {
    // Receive from the read task
    read_queue: mpsc::Receiver<ConnectionCommand>,

    // Send to the write task
    send_queue: mpsc::Sender<ClientCommand>,
}

struct State {
    read: ReadState,
    write: WriteState,
    conn: ConnectionState,

    sid: AtomicUsize,
    sid2chan: Arc<RwLock<HashMap<String, mpsc::Sender<Message>>>>,
}

struct ConnectionActor {
    state: State,

    pub conn_sender: mpsc::Sender<ConnectionCommand>,
    pub client_sender: mpsc::Sender<ClientCommand>,
}

impl ConnectionActor {
    pub fn new(reader: OwnedReadHalf, writer: OwnedWriteHalf) -> Self {
        let (conn_sender, conn_receiver) = mpsc::channel(1024);
        let (client_sender, client_receiver) = mpsc::channel(1024);

        let read = ReadState {
            reader: BufReader::new(reader),
            channel: conn_sender.clone(),
        };
        let write = WriteState {
            writer: BufWriter::new(writer),
            channel: client_receiver,
        };
        let conn = ConnectionState {
            read_queue: conn_receiver,
            send_queue: client_sender.clone(),
        };

        Self {
            state: State {
                read,
                write,
                conn,
                sid: AtomicUsize::new(0),
                sid2chan: RwLock::new(HashMap::new()).into(),
            },
            conn_sender,
            client_sender,
        }
    }

    pub async fn run(self) {
        // Read from the TCP stream
        let sid2chan = self.state.sid2chan.clone();

        let reader = tokio::spawn(async move {
            let ReadState { reader, channel } = self.state.read;

            let mut stream = FramedRead::new(reader, ServerCodec);
            while let Ok(Some(command)) = stream.try_next().await {
                match command {
                    ServerCommand::Info(info) => {
                        log::trace!("Server sent `INFO` command after initial connection {info:?}");
                    }
                    ServerCommand::Msg(msg) => {
                        log::info!("Server sent message: {msg:?}");
                        let sid2channel = sid2chan.read().await;

                        let Some(chan) = sid2channel.get(&msg.sid.to_string()) else {
                            log::error!("MSG contained unknown SID: {}", msg.sid);
                            continue;
                        };
                        chan.send(msg.into())
                            .await
                            .expect("Failed to send MSG to subscriber");
                    }
                    ServerCommand::HMsg(hmsg) => {
                        log::info!("Server sent message with headers: {hmsg:?}");
                        let sid2chan = sid2chan.read().await;

                        let Some(chan) = sid2chan.get(&hmsg.sid.to_string()) else {
                            log::error!("MSG contained unknown SID: {}", hmsg.sid);
                            continue;
                        };
                        chan.send(hmsg.into())
                            .await
                            .expect("Failed to send HMSG to subscriber");
                    }
                    ServerCommand::Ping => channel
                        .send(ConnectionCommand::Pong)
                        .await
                        .expect("Failed to send"),

                    ServerCommand::Pong => {}
                    ServerCommand::Ok => {
                        log::trace!("+OK received");
                    }
                    ServerCommand::Err(message) => {
                        log::error!("{message}");
                    }
                }
            }
        });

        // Write to TCP stream
        let writer = tokio::spawn(async move {
            let WriteState { writer, channel } = self.state.write;

            let mut stream = ReceiverStream::new(channel);
            let mut sink = FramedWrite::new(writer, ClientCodec);

            while let Some(command) = stream.next().await {
                sink.send(command).await.unwrap();
            }
        });

        // Regularly instruct write task to ping server
        let ping_sender = self.conn_sender.clone();
        let pinger = tokio::spawn(async move {
            let tick = std::time::Duration::from_secs(30);
            let mut interval = tokio::time::interval(tick);
            loop {
                interval.tick().await;
                ping_sender
                    .send(ConnectionCommand::Ping)
                    .await
                    .expect("Failed to send PING!");
            }
        });

        let sid2chan = self.state.sid2chan.clone();
        let negotiator = tokio::spawn(async move {
            let ConnectionState {
                mut read_queue,
                send_queue,
            } = self.state.conn;

            while let Some(message) = read_queue.recv().await {
                match message {
                    ConnectionCommand::Publish(publish) => {
                        send_queue
                            .send(ClientCommand::Publish(publish))
                            .await
                            .unwrap();
                    }
                    ConnectionCommand::Subscribe(SubscriptionRequest {
                        subject,
                        options,
                        channels: (send, recv),
                        sub_chan,
                    }) => {
                        let sid = {
                            let mut sid2chan = sid2chan.write().await;
                            let sid = self.state.sid.fetch_add(1, Ordering::Relaxed).to_string();
                            sid2chan.insert(sid.clone(), send);
                            sid
                        };

                        let receiver = if let Some(max_msgs) = options.max_msgs {
                            ReceiverStream::new(recv).take(max_msgs.into()).boxed()
                        } else {
                            ReceiverStream::new(recv).boxed()
                        };

                        // Send the subscriber back to the client thread ASAP so they can continue work.
                        // It does not matter that the `SUB` (and potential `UNSUB`) have not yet been sent yet,
                        // as the `.take` combinator on the `ReceiverStream` will keep count of received messages by itself,
                        let subscriber =
                            Subscriber::new(sid.clone(), receiver, self.conn_sender.clone());
                        sub_chan.send(subscriber).unwrap();

                        send_queue
                            .send(ClientCommand::Subscribe(Subscribe {
                                subject,
                                queue_group: options.queue_group.clone(),
                                sid: sid.clone(),
                            }))
                            .await
                            .unwrap();

                        if let Some(max_msgs) = options.max_msgs {
                            send_queue
                                .send(ClientCommand::Unsubscribe(Unsubscribe {
                                    max_msgs: Some(max_msgs),
                                    sid: sid.clone(),
                                }))
                                .await
                                .unwrap();
                        }
                    }
                    ConnectionCommand::Unsubscribe(unsubscibe) => {
                        let mut sid2chan = sid2chan.write().await;
                        sid2chan.remove(&unsubscibe.sid);
                        send_queue
                            .send(ClientCommand::Unsubscribe(unsubscibe))
                            .await
                            .unwrap();
                    }
                    ConnectionCommand::Ping => {
                        send_queue.send(ClientCommand::Ping).await.unwrap();
                    }
                    ConnectionCommand::Pong => {
                        send_queue.send(ClientCommand::Pong).await.unwrap();
                    }
                };
            }
        });

        let _ = tokio::join!(reader, writer, pinger, negotiator);
    }
}

pub struct Connection {
    pub conn_chan: mpsc::Sender<ConnectionCommand>,
    pub client_chan: mpsc::Sender<ClientCommand>,

    pub task: tokio::task::JoinHandle<()>,
}

impl Connection {
    pub async fn connect(socket: SocketAddr) -> Self {
        log::trace!("Connecting to {}", socket);
        let (read, write) = TcpStream::connect(socket)
            .await
            .expect("Failed to connect to NATS server")
            .into_split();
        log::debug!("Successfully connected to {}", socket);

        let (mut reader, mut writer) = (BufReader::new(read), BufWriter::new(write));

        // Extract `INFO`.
        let mut stream = FramedRead::new(&mut reader, ServerCodec);
        let Ok(Some(ServerCommand::Info(_info))) = stream.try_next().await else {
            panic!("Expected first message to be INFO! Have you connected to a real NATS server? :eyes:");
        };

        let mut sink = FramedWrite::new(&mut writer, ClientCodec);
        sink.send(ClientCommand::Connect(Connect {
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
        }))
        .await
        .expect("Failed to send `CONNECT`");

        let actor = ConnectionActor::new(reader.into_inner(), writer.into_inner());

        let client_sender = actor.client_sender.clone();
        let conn_sender = actor.conn_sender.clone();

        Self {
            task: tokio::spawn(actor.run()),
            client_chan: client_sender,
            conn_chan: conn_sender,
        }
    }

    pub async fn subscribe(&mut self, subject: String) -> Subscriber {
        self.subscribe_with_options(subject, SubscriptionOptions::default())
            .await
    }

    pub async fn subscribe_with_options(
        &mut self,
        subject: String,
        options: SubscriptionOptions,
    ) -> Subscriber {
        let (sender, receiver) = oneshot::channel();

        self.conn_chan
            .send(ConnectionCommand::Subscribe(SubscriptionRequest {
                subject,
                options,
                sub_chan: sender,
                channels: mpsc::channel(1024),
            }))
            .await
            .unwrap();

        receiver.await.unwrap()
    }

    pub async fn publish(&mut self, subject: String, payload: tokio_util::bytes::Bytes) {
        self.conn_chan
            .send(ConnectionCommand::Publish(Publish {
                subject,
                reply_to: None,
                bytes: payload.len(),
                payload,
            }))
            .await
            .unwrap();
    }
}
