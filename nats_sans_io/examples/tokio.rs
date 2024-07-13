use std::time::{Duration, Instant};

use futures::SinkExt;
use tokio::{
    io::{BufReader, BufWriter},
    net::TcpStream,
    time,
};
use tokio_stream::StreamExt;
use tokio_util::codec::{FramedRead, FramedWrite};

use nats_sans_io::{NatsBinding, Timeouts};

#[tokio::main]
async fn main() {
    env_logger::init();

    let (reader, writer) = TcpStream::connect(
        "147.75.47.215:4222"
            .parse::<std::net::SocketAddr>()
            .unwrap(),
    )
    .await
    .expect("Failed to connect to TCP socket")
    .into_split();

    let mut reader = FramedRead::new(BufReader::new(reader), nats_codec::ServerCodec);
    let mut writer = FramedWrite::new(BufWriter::new(writer), nats_codec::ClientCodec);

    let mut binding = NatsBinding::new(Timeouts {
        ping_interval: Duration::from_secs(1),
        pong_delay: Duration::from_secs(0),
        pong_interval: Duration::from_secs(30),
    });

    log::info!("Starting event loop");

    // Send a `PING` within the first 5 seconds
    let mut send_ping_ticker = time::interval(Duration::from_secs(5));

    // Expect a `PING` within the first 5 seconds
    let mut recv_ping_ticker = time::interval(Duration::from_secs(5));

    // Expect a `PONG` within the first 30 seconds
    let mut recv_pong_ticker = time::interval(Duration::from_secs(30));

    loop {
        if let Some(transmit) = binding.poll_transmit() {
            let _ = writer.send(transmit).await;
            continue;
        }

        tokio::select! {
            time = send_ping_ticker.tick() => {
                let _ = binding.handle_send_ping_timeout(time.into());
            }
            time = recv_ping_ticker.tick() => {
                let _ = binding.handle_send_pong_timeout(time.into());
            }
            time = recv_pong_ticker.tick() => {
                let _ = binding.handle_recv_pong_timeout(time.into());
            }
            command = reader.next() => {
                match command {
                    Some(Ok(command)) => binding.handle_server_input(command, Instant::now()),
                    Some(Err(e)) => log::error!("Server produced invalid command: {e:?}"),
                    None => {
                        log::error!("NATS Server disconnected TCP Stream");
                        break;
                    }
                };
            }
        };

        if let Some(tick) = binding.poll_send_ping_timeout(Instant::now()) {
            send_ping_ticker.reset_at(tick.into());
        }

        if let Some(tick) = binding.poll_send_pong_timeout() {
            recv_ping_ticker.reset_at(tick.into());
        }

        if let Some(tick) = binding.poll_recv_pong_timeout() {
            recv_pong_ticker.reset_at(tick.into());
        }
    }
}
