use clap::Parser;
use std::net::SocketAddr;
use tokio_stream::StreamExt;

use nats_client::{ConnectionHandle, SubscriptionOptions};

#[derive(Parser)]
struct Cli {
    socket: SocketAddr,
    max_msgs: Option<usize>,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let Cli { socket, max_msgs } = Cli::parse();

    let mut connection = ConnectionHandle::connect(socket).await;

    let mut atlanta = connection
        .subscribe_with_options(
            "time.us.east.atlanta".into(),
            SubscriptionOptions {
                max_msgs,
                ..Default::default()
            },
        )
        .await;

    let sid = atlanta.sid().clone();
    while let Some(message) = atlanta.next().await {
        log::info!("[{sid}]: {message:?}")
    }
}
