use std::{net::SocketAddr, num::NonZeroUsize};

use clap::Parser;
use tokio_stream::StreamExt;

use nats_client::blind::*;

#[derive(Parser)]
struct Cli {
    socket: SocketAddr,
    subject: String,
    max_msgs: Option<NonZeroUsize>,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let Cli {
        socket,
        subject,
        max_msgs,
    } = Cli::parse();

    let mut connection = Connection::connect(socket).await;

    let options = SubscriptionOptions {
        max_msgs,
        ..Default::default()
    };
    let mut atlanta = connection.subscribe_with_options(subject, options).await;

    let sid = atlanta.sid().clone();
    while let Some(message) = atlanta.next().await {
        println!("[{sid}]: {message:?}")
    }
}
