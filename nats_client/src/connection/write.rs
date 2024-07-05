use futures::SinkExt;
use nats_codec::{ClientCodec, ClientCommand};
use tokio::task::JoinHandle;
use tokio::{io::BufWriter, net::tcp::OwnedWriteHalf, sync::mpsc};
use tokio_util::codec::FramedWrite;

pub struct Writer {
    /// Outgoing byte stream over TCP.
    tcp_sink: BufWriter<OwnedWriteHalf>,

    command_chan: mpsc::Receiver<ClientCommand>,
}

impl Writer {
    pub fn new(writer: OwnedWriteHalf, command_chan: mpsc::Receiver<ClientCommand>) -> Self {
        Self {
            tcp_sink: BufWriter::new(writer),
            command_chan,
        }
    }

    pub async fn run(mut self) {
        let mut sink = FramedWrite::new(self.tcp_sink, ClientCodec);
        while let Some(command) = self.command_chan.recv().await {
            sink.send(command.into()).await.unwrap()
        }

        log::error!("Send task finished; terminating...")
    }
}
