use nats_codec::ClientCommand;
use tokio::{
    sync::{broadcast, mpsc},
    task::JoinHandle,
};

/// The connection must be kept alive by sending [`nats_codec::Ping`] regularly.
/// This task may commence when [`nats_codec::Connect`] command has been received 
/// by the NATS server.
struct Ping {
    /// Receive notification when connection has been successfully established
    /// by exchanging `INFO` and `CONNECT.`
    connection_notifier: broadcast::Receiver<()>,

    /// Pass commands to 
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

/// Spawn a [`self::Ping`] task and related resources using [`PingHandle::new`].
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
