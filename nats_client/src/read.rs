use nats_codec::{ClientCommand, Info, ServerCodec, ServerCommand};
use tokio::{
    io::BufReader,
    net::tcp::OwnedReadHalf,
    sync::{broadcast, mpsc, oneshot},
    task::JoinHandle,
};
use tokio_stream::StreamExt as _;
use tokio_util::codec::FramedRead;

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

        log::info!("Awaiting connection creation confirmation");
        self.connection_notifier
            .recv()
            .await
            .expect("Successful connection could not be established");
        while let Ok(Some(command)) = stream.try_next().await {
            log::trace!("Server sent {command:?}");

            let response = match command {
                ServerCommand::Info(_) => todo!(),
                ServerCommand::Msg(msg) => {
                    log::info!("Server sent message: {msg:?}");
                    continue;
                }
                ServerCommand::HMsg(_) => todo!(),
                ServerCommand::Ping => ClientCommand::Pong,
                ServerCommand::Pong => {
                    continue;
                }
                ServerCommand::Ok => {
                    continue;
                }
                ServerCommand::Err(message) => {
                    log::error!("{message}");
                    continue;
                }
            };
            self.command_sender
                .send(response)
                .await
                .expect("Failed to send");
        }
    }
}

pub struct ReaderHandle {
    pub info_receiver: oneshot::Receiver<Info>,
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
        };

        Self {
            info_receiver,
            task: tokio::spawn(actor.run()),
        }
    }
}
