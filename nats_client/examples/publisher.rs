use std::{net::SocketAddr, time::Duration};

use chrono::{FixedOffset, TimeZone as _};
use clap::Parser;

use nats_client::Connection;

#[derive(Parser)]
struct Cli {
    socket: SocketAddr,
    subject: String,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let Cli { socket, subject } = Cli::parse();

    let mut connection = Connection::connect(socket).await;
    let mut interval = tokio::time::interval(Duration::from_secs(5));

    let offset = FixedOffset::east_opt(5 * 60 * 60).unwrap();

    loop {
        interval.tick().await;

        let now = chrono::Utc::now().naive_utc();
        let timestamp = offset.from_utc_datetime(&now).to_rfc3339();

        println!("{timestamp}");
        connection.publish(subject.clone(), timestamp.into()).await
    }
}
