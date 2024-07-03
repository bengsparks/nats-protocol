mod ping;
mod read;
mod write;

use nats_codec::{ClientCommand, Message, Sub, Unsub};
use ping::PingHandle;
use read::ReaderHandle;
use tokio::{
    net::TcpStream,
    sync::{broadcast, mpsc},
    task::JoinHandle,
};
use write::WriterHandle;

use std::net::SocketAddr;

struct Connection {}

pub struct Subscriber {
    command_sender: mpsc::Sender<ClientCommand>,
    subscription_stream: mpsc::Receiver<Message>,
    sid: u32,
}

impl Subscriber {
    pub async fn unsubscribe(self) {
        self.command_sender
            .send(ClientCommand::Unsub(Unsub {
                sid: self.sid.to_string(),
                max_msgs: None,
            }))
            .await
            .unwrap();
    }
}

impl Connection {}

pub struct ConnectionHandle {
    pub command_sender: mpsc::Sender<ClientCommand>,
    pub ready_notifier: broadcast::Sender<()>,

    // per-connection SID counter
    sid: u32,

    // task handles
    read_task: JoinHandle<()>,
    ping_task: JoinHandle<()>,
    write_task: JoinHandle<()>,
}

impl ConnectionHandle {
    pub async fn connect(socket: SocketAddr) -> Self {
        log::trace!("Connecting to {}", socket);
        let (read, write) = TcpStream::connect(socket)
            .await
            .expect("Failed to connect to NATS server")
            .into_split();
        log::debug!("Successfully connected to {}", socket);

        // Broadcast channel to signal a newly established connection with a NATS server.
        let (ready_notifier, ready_receiver) = broadcast::channel::<()>(8);

        // MPSC channel pair whose consumer half is passed to the sending task.
        // Passing a clone of the sending half to a task allows for that task to send [`nats_codec::ClientCommand`].
        let (command_sender, command_receiver) = mpsc::channel(1024 * 1024);

        let ReaderHandle {
            info_receiver,
            task: read_task,
        } = ReaderHandle::new(read, ready_notifier.subscribe(), command_sender.clone());
        let PingHandle { task: ping_task } =
            PingHandle::new(ready_notifier.subscribe(), command_sender.clone());
        let WriterHandle { task: write_task } = WriterHandle::new(
            write,
            command_receiver,
            info_receiver,
            ready_notifier.clone(),
        );
        drop(ready_receiver);

        Self {
            command_sender,
            ready_notifier,
            sid: 0,
            read_task,
            ping_task,
            write_task,
        }
    }

    pub async fn subscribe(&mut self, subject: String, queue_group: Option<String>) -> Subscriber {
        self.sid += 1;

        let sid = self.sid.to_string();
        self.command_sender
            .send(ClientCommand::Sub(Sub {
                subject,
                queue_group,
                sid,
            }))
            .await;

        let (subscription_sink, subscription_stream) = mpsc::channel(1024);

        Subscriber {
            sid: self.sid,
            command_sender: self.command_sender.clone(),
            subscription_stream
        }
    }

    pub async fn publish(&mut self, subject: String) {}

    pub async fn close(self) {
        // Drop TCP connection by dropping the write half of the connection, i.e. cancelling the write task
        if !self.write_task.is_finished() {
            self.write_task.abort();
        }
        let res = tokio::try_join!(self.read_task, self.write_task, self.ping_task);
        if let Err(e) = res {
            if e.is_panic() {
                std::panic::resume_unwind(e.into_panic());
            }
        }
    }
}
