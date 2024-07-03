use std::collections::HashMap;

use nats_codec::{ClientCommand, Info, Message, ServerCodec, ServerCommand};
use tokio::{
    io::BufReader,
    net::tcp::OwnedReadHalf,
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
};
use tokio_stream::StreamExt as _;
use tokio_util::codec::FramedRead;

/// When the server accepts a connection from the client, from the TCP stream,
/// it will send information, configuration, and security requirements necessary for the
/// client to successfully authenticate with the server and exchange messages.
/// as a [`nats_codec::Info`] first.
struct Reader {
    /// Incoming byte stream over TCP.
    tcp_stream: BufReader<OwnedReadHalf>,

    /// A client will need to start as a plain TCP connection,
    /// then when the server accepts a connection from the client,
    /// it will send information about itself, the configuration
    /// and security requirements necessary for the client to successfully
    /// authenticate with the server and exchange messages.
    info_sender: oneshot::Sender<Info>,

    /// Receive notification when the [nats_codec::Connect] command has been sent
    connection_notifier: broadcast::Receiver<()>,

    /// Pass queue of commands
    command_sender: mpsc::Sender<ClientCommand>,

    /// Send received messages to subscribers; SID -> subscriber channel
    subscriber_map: HashMap<String, mpsc::Sender<Message>>,
}

impl Reader {
    async fn run(mut self) {
        let mut stream = FramedRead::new(self.tcp_stream, ServerCodec);

        log::debug!("Awaiting `INFO` message");
        let Ok(Some(ServerCommand::Info(info))) = stream.try_next().await else {
            panic!("Expected first message to be INFO! Have you connected to a real NATS server? :eyes:");
        };

        log::info!("Received {:#?}", info);
        self.info_sender
            .send(info)
            .expect("Failed to send `INFO` to auth manager");

        self.connection_notifier
            .recv()
            .await
            .expect("Successful connection could not be established");

        log::info!("Connection established, ready to read commands over TCP!");
        while let Ok(Some(command)) = stream.try_next().await {
            match command {
                ServerCommand::Info(info) => {
                    log::trace!("Server sent `INFO` command after initial connection {info:?}");
                }
                ServerCommand::Msg(msg) => {
                    log::info!("Server sent message: {msg:?}");
                    let Some(sender) = self.subscriber_map.get(&msg.sid) else {
                        continue;
                    };
                    sender.send(msg.into()).await.unwrap()
                }
                ServerCommand::HMsg(msg) => {
                    log::info!("Server sent message with headers: {msg:?}");
                    let Some(sender) = self.subscriber_map.get(&msg.sid) else {
                        continue;
                    };
                    sender.send(msg.into()).await.unwrap()
                }
                ServerCommand::Ping => self
                    .command_sender
                    .send(ClientCommand::Pong)
                    .await
                    .expect("Failed to send"),

                ServerCommand::Pong => {
                }
                ServerCommand::Ok => {
                    log::trace!("+OK received");
                }
                ServerCommand::Err(message) => {
                    log::error!("{message}");
                }
            }
        }
    }
}

/// Spawn a [`self::Reader`] task and related resources using [`ReaderHandle::new`].
pub struct ReaderHandle {
    /// Allow for one subtask to receive the `INFO` message when sent by the server.
    pub info_receiver: oneshot::Receiver<Info>,
    /// Handle to the asynchronous task that reads from the provided TCP Stream.
    /// This task terminates if the address connected to is not a NATS server, or it does not respond with an `INFO`
    pub task: JoinHandle<()>,
}

impl ReaderHandle {
    pub fn new(
        tcp_stream: OwnedReadHalf,
        connection_notifier: broadcast::Receiver<()>,
        command_sender: mpsc::Sender<ClientCommand>,
    ) -> Self {
        let (info_sender, info_receiver) = tokio::sync::oneshot::channel::<Info>();

        let actor = Reader {
            tcp_stream: BufReader::new(tcp_stream),
            info_sender,
            connection_notifier,
            command_sender,
            subscriber_broadcast: HashMap::default()
        };

        Self {
            info_receiver,
            task: tokio::spawn(actor.run()),
        }
    }
}
