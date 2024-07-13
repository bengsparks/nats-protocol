use std::{net::SocketAddr, time::Duration};

use chrono::{FixedOffset, TimeZone as _};
use clap::Parser;

use tokio::{net::TcpStream, sync::mpsc};

use nats_client::tokio::NatsOverTcp;

#[derive(Parser)]
struct Cli {
    socket: SocketAddr,
    subject: String,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let Cli { socket, subject } = Cli::parse();
    let tcp = TcpStream::connect(socket)
        .await
        .expect("Failed to connect to TCP socket");

    let protocol = NatsOverTcp::new(tcp);
    let (send, mut recv) = mpsc::channel(64);

    let conn_task = tokio::spawn(async move {
        let timeouts = nats_sans_io::Timeouts {
            ping_interval: Duration::from_secs(15),
            pong_delay: Duration::from_secs(0),
            pong_interval: Duration::from_secs(30),
        };
        let _ = protocol.run(timeouts, send).await.unwrap();
    });

    let user_task = tokio::spawn(async move {
        let client = recv.recv().await.unwrap();
        let offset = FixedOffset::east_opt(5 * 60 * 60).unwrap();

        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;

            let now = chrono::Utc::now().naive_utc();
            let timestamp = offset.from_utc_datetime(&now).to_rfc3339();

            println!("{timestamp}");
            client.publish(subject.clone(), timestamp.into()).await
        }
    });

    let _ = tokio::try_join!(conn_task, user_task);
}
