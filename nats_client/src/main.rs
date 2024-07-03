mod connection;

mod pipe;

use clap::Parser;
use connection::ConnectionHandle;
use pipe::PipeHandle;
use std::net::SocketAddr;

#[derive(Parser)]
struct Cli {
    socket: SocketAddr,
    fifo: std::path::PathBuf,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let args = Cli::parse();

    let ref connection @ ConnectionHandle {
        ref ready_notifier,
        ref command_sender,
        ..
    } = ConnectionHandle::connect(args.socket).await;

    let PipeHandle { task: pipe_task } = PipeHandle::new(
        ready_notifier.subscribe(),
        command_sender.clone(),
        args.fifo,
    );

    let nixrust = connection.subscribe("nixrust.*".into(), None).await;

}
