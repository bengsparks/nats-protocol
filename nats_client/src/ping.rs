use nats_codec::ClientCommand;
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};

struct Ping {
    /// Receive notification when connection has been successfully established
    /// by exchanging `INFO` and `CONNECT.`
    connection_notifier: broadcast::Receiver<()>,

    /// Pass queue of commands
    command_sender: mpsc::Sender<ClientCommand>,
}

impl Ping {
    pub async fn every(mut self, tick: std::time::Duration) {
        self.connection_notifier
            .recv()
            .await
            .expect("Successful connection could not be established");

        let mut interval = tokio::time::interval(tick);
        loop {
            interval.tick().await;
            self.command_sender
                .send(ClientCommand::Ping)
                .await
                .expect("Failed to send PING!");
        }
    }
}

pub struct PingHandle {
    pub task: JoinHandle<()>,
}

impl PingHandle {
    pub fn new(
        connection_notifier: broadcast::Receiver<()>,
        command_sender: mpsc::Sender<ClientCommand>,
    ) -> Self {
        let actor = Ping {
            connection_notifier,
            command_sender,
        };

        Self {
            task: tokio::spawn(actor.every(std::time::Duration::from_secs(20))),
        }
    }
}
