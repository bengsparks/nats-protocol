mod ping;
mod pipe;
mod read;
mod write;

use ping::PingHandle;
use pipe::PipeHandle;
use read::ReaderHandle;
use write::WriterHandle;

use std::net::SocketAddr;

use clap::Parser;

use futures::SinkExt;
use nats_codec::{ClientCodec, ClientCommand, Connect, Info, ServerCodec, ServerCommand};

#[derive(Parser)]
struct Cli {
    socket: SocketAddr,
    fifo: std::path::PathBuf,
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    env_logger::init();

    let (read, write) = tokio::net::TcpStream::connect(args.socket)
        .await
        .expect("Failed to connect to NATS server")
        .into_split();
    log::debug!("Successfully connected to {}", args.socket);

    let (command_sender, command_receiver) =
        tokio::sync::mpsc::channel::<ClientCommand>(1024 * 1024);
    let (ready_notifier, ready_receiver) = tokio::sync::broadcast::channel::<()>(1);

    let ReaderHandle {
        info_receiver,
        task: read_task,
    } = ReaderHandle::new(read, ready_notifier.subscribe(), command_sender.clone());
    let PingHandle { task: ping_task } =
        PingHandle::new(ready_notifier.subscribe(), command_sender.clone());
    let PipeHandle { task: pipe_task } = PipeHandle::new(
        ready_notifier.subscribe(),
        command_sender.clone(),
        args.fifo,
    );

    let WriterHandle { task: write_task } =
        WriterHandle::new(write, command_receiver, info_receiver, ready_notifier);

    drop(ready_receiver);

    let res = tokio::try_join!(read_task, write_task, ping_task, pipe_task);
    if let Err(e) = res {
        if e.is_panic() {
            std::panic::resume_unwind(e.into_panic());
        }
    }
}
