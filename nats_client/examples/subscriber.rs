use std::net::ToSocketAddrs;
use std::num::NonZeroUsize;
use std::time::Duration;

use clap::Parser;

use futures::StreamExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use nats_client::tokio::{NatsOverTcp, SubscriptionOptions};

#[derive(Parser)]
struct Cli {
    host: String,
    port: u16,
    subject: String,
    max_msgs: Option<NonZeroUsize>,
    queue_group: Option<String>,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let Cli {
        host,
        port,
        subject,
        max_msgs,
        queue_group,
    } = Cli::parse();

    let Ok(Some(socket)) = format!("{host}:{port}")
        .to_socket_addrs()
        .map(|mut i| i.next())
    else {
        log::error!("Failed to resolve {host}:{port}");
        return;
    };
    log::info!("Connecting to {socket}");

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
        while let Some(client) = recv.recv().await {
            let options = SubscriptionOptions {
                max_msgs,
                queue_group: queue_group.clone(),
            };

            let mut subscriber = client.subscribe(subject.clone(), options).await;
            while let Some(message) = subscriber.next().await {
                println!("{message:?}");
            }

            log::info!("Subscriber completed");
        }
    });

    let _ = tokio::try_join!(conn_task, user_task);
}