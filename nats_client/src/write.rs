use futures::SinkExt;
use nats_codec::{ClientCodec, ClientCommand, Connect, Info};
use tokio::sync::{broadcast, oneshot};
use tokio::task::JoinHandle;
use tokio::{io::BufWriter, net::tcp::OwnedWriteHalf, sync::mpsc};
use tokio_util::codec::FramedWrite;

struct Writer {
    /// Outgoing byte stream over TCP.
    tcp_sink: BufWriter<OwnedWriteHalf>,

    command_receiver: mpsc::Receiver<ClientCommand>,

    info_receiver: oneshot::Receiver<Info>,

    ready_sender: broadcast::Sender<()>,
}

impl Writer {
    pub async fn send_messages(mut self) {
        let _info = self
            .info_receiver
            .await
            .expect("Failed to receive information required to establish connection! :sad_face:");
        let mut sink = FramedWrite::new(self.tcp_sink, ClientCodec);

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
            headers: None,
        }))
        .await
        .expect("Failed to send CONNECT");

        self.ready_sender
            .send(())
            .expect("Failed to send connection confirmation signal to subtasks");
        while let Some(command) = self.command_receiver.recv().await {
            if !matches!(command, ClientCommand::Ping | ClientCommand::Pong) {
                log::info!("Client sent {command:?}");
            } else {
                log::trace!("Client sent {command:?}");
            }
            sink.send(command)
                .await
                .expect("Failed to send command to server!");
        }

        log::error!("Send task finished; terminating...")
    }
}

pub struct WriterHandle {
    pub task: JoinHandle<()>,
}

impl WriterHandle {
    pub fn new(
        write: OwnedWriteHalf,
        command_receiver: mpsc::Receiver<ClientCommand>,
        info_receiver: oneshot::Receiver<Info>,
        ready_sender: broadcast::Sender<()>,
    ) -> Self {
        let actor = Writer {
            tcp_sink: BufWriter::new(write),
            command_receiver,
            info_receiver,
            ready_sender,
        };

        Self {
            task: tokio::spawn(actor.send_messages()),
        }
    }
}
