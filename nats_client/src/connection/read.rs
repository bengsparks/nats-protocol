use nats_codec::{ClientCommand, ServerCodec, ServerCommand};
use tokio::{io::BufReader, net::tcp::OwnedReadHalf, sync::mpsc};
use tokio_stream::StreamExt as _;
use tokio_util::codec::FramedRead;

/// When the server accepts a connection from the client, from the TCP stream,
/// it will send information, configuration, and security requirements necessary for the
/// client to successfully authenticate with the server and exchange messages.
/// as a [`nats_codec::Info`] first.
pub struct Reader {
    /// Incoming byte stream over TCP.
    tcp_stream: BufReader<OwnedReadHalf>,

    /// Instruct connection how to react to received messages
    command_chan: mpsc::Sender<ClientCommand>,
}

impl Reader {
    pub fn new(reader: OwnedReadHalf, command_chan: mpsc::Sender<ClientCommand>) -> Self {
        Self {
            tcp_stream: BufReader::new(reader),
            command_chan,
        }
    }

    pub async fn run(self) {
        let mut stream = FramedRead::new(self.tcp_stream, ServerCodec);
        while let Ok(Some(command)) = stream.try_next().await {
            match command {
                ServerCommand::Info(info) => {
                    log::trace!("Server sent `INFO` command after initial connection {info:?}");
                }
                ServerCommand::Msg(msg) => {
                    log::info!("Server sent message: {msg:?}");
                }
                ServerCommand::HMsg(msg) => {
                    log::info!("Server sent message with headers: {msg:?}");
                }
                ServerCommand::Ping => self
                    .command_chan
                    .send(ClientCommand::Pong)
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
    }
}
