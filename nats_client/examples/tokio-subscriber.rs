use std::num::NonZeroUsize;
use std::{net::SocketAddr, time::Duration};

use clap::Parser;

use futures::StreamExt;
use tokio::net::TcpStream;

use nats_client::tokio::NatsOverTcp;
use nats_client::tokio::SubscriptionOptions;
use tokio::sync::mpsc;

#[derive(Parser)]
struct Cli {
    socket: SocketAddr,
    subject: String,
    max_msgs: Option<NonZeroUsize>,
    queue_group: Option<String>,
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let Cli {
        socket,
        subject,
        max_msgs,
        queue_group,
    } = Cli::parse();

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
