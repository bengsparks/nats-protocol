use nats_codec::ClientCommand;
use tokio::sync::mpsc;

/// The connection must be kept alive by sending [`nats_codec::Ping`] regularly.
/// This task may commence when [`nats_codec::Connect`] command has been received
/// by the NATS server.
pub struct Ping {
    /// Pass commands to connection
    command_chan: mpsc::Sender<ClientCommand>,
}

impl Ping {
    pub fn new(command_chan: mpsc::Sender<ClientCommand>) -> Self {
        Self { command_chan }
    }

    pub async fn every(self, tick: std::time::Duration) {
        let mut interval = tokio::time::interval(tick);
        loop {
            interval.tick().await;
            self.command_chan
                .send(ClientCommand::Ping)
                .await
                .expect("Failed to send PING!");
        }
    }
}
