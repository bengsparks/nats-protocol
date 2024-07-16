use std::{num::NonZeroUsize, time::Duration};

use chrono::{FixedOffset, TimeZone as _};
use clap::Parser;

use futures::StreamExt;
use tokio::{net::TcpStream, sync::oneshot};

use nats_client::tokio::NatsOverTcp;

#[derive(Parser)]
struct Cli {
    host: String,
    port: u16,
    subject: String,
    limit: Option<NonZeroUsize>,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let Cli {
        host,
        port,
        subject,
        limit,
    } = Cli::parse();

    log::info!("Connecting to {host}:{port}");
    let tcp = TcpStream::connect((host, port))
        .await
        .expect("Failed to connect to TCP socket");

    let protocol = NatsOverTcp::new(tcp);
    let (send, recv) = oneshot::channel();

    let conn_task = tokio::spawn(async move {
        let timeouts = nats_sans_io::Timeouts {
            ping_interval: Duration::from_secs(15),
            pong_delay: Duration::from_secs(0),
            keep_alive: Duration::from_secs(30),
        };
        let _ = protocol.run(timeouts, send).await.unwrap();
    });

    let client = recv.await.unwrap();
    let offset = FixedOffset::east_opt(5 * 60 * 60 + 30 * 60).unwrap();

    let timer = std::iter::from_fn(move || {
        let now = chrono::Utc::now().naive_utc();
        let timestamp = offset
            .from_utc_datetime(&now)
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        Some(timestamp)
    });

    let mut stream = if let Some(l) = limit {
        tokio_stream::iter(timer).take(l.into()).boxed()
    } else {
        tokio_stream::iter(timer).boxed()
    };


    let mut interval = tokio::time::interval(Duration::from_secs(1));
    while let Some(timestamp) = stream.next().await {
        interval.tick().await;
        println!("{timestamp}");
        client.publish(subject.clone(), timestamp.into()).await;
    }

    tokio::time::sleep(Duration::from_secs(1)).await;
    client.close().await;

    conn_task.abort();
}
