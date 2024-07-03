use nats_codec::{ClientCodec, ClientCommand};
use tokio::{
    net::unix::pipe,
    sync::{broadcast, mpsc},
    task::JoinHandle,
};
use tokio_stream::StreamExt as _;
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

        let mut fifo = pipe::OpenOptions::new()
            .open_receiver(path)
            .expect("Failed to read mkfifo file");

        loop {
            let mut stream = FramedRead::new(&mut fifo, ClientCodec);

            loop {
                let command = stream.try_next().await;
                match command {
                    Ok(Some(command)) => {
                        self.command_sender
                            .send(command)
                            .await
                            .expect("Failed to pass command from file to command queue");
                    }
                    Ok(None) => {
                        log::error!("Stream has been closed, reopening...");
                        break;
                    }
                    Err(e) => {
                        log::error!("Decoding error: {e}");
                        continue;
                    }
                }
            }
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
