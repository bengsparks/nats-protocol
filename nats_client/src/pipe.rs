use futures::SinkExt;
use nats_codec::{ClientCodec, ClientCommand};
use tokio::{
    net::unix::pipe,
    sync::{broadcast, mpsc},
    task::JoinHandle,
};
use tokio_stream::StreamExt;
use tokio_util::codec::FramedRead;

struct Pipe {
    /// Receive notification when connection has been successfully established
    /// by exchanging `INFO` and `CONNECT.`
    connection_notifier: broadcast::Receiver<()>,

    /// Pass queue of commands
    command_sender: mpsc::Sender<ClientCommand>,
}

impl Pipe {
    pub async fn run(mut self, path: std::path::PathBuf) {
        self.connection_notifier
            .recv()
            .await
            .expect("Failed to receive confirmation to start sending messages from pipe");

        let fifo = pipe::OpenOptions::new()
            .open_receiver(path)
            .expect("Failed to read mkfifo file");

        let mut stream = FramedRead::new(fifo, ClientCodec);
        while let Ok(Some(command)) = stream.try_next().await {
            self.command_sender
                .send(command)
                .await
                .expect("Failed to pass command from file to command queue");
        }
    }
}

pub struct PipeHandle {
    pub task: JoinHandle<()>,
}

impl PipeHandle {
    pub fn new(
        connection_notifier: broadcast::Receiver<()>,
        command_sender: mpsc::Sender<ClientCommand>,
        path: std::path::PathBuf,
    ) -> Self {
        let actor = Pipe {
            connection_notifier,
            command_sender,
        };

        Self {
            task: tokio::spawn(actor.run(path)),
        }
    }
}
