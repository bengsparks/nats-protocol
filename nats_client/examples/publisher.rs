use chrono::{DateTime, FixedOffset, NaiveDateTime, NaiveTime, TimeZone as _};
use clap::Parser;
use nats_codec::{ClientCodec, ClientCommand};
use std::{net::SocketAddr, time::Duration};
use tokio::{net::unix::pipe, sync::mpsc, task::JoinHandle};
use tokio_stream::StreamExt;
use tokio_util::codec::FramedRead;

use nats_client::{ConnectionCommand, ConnectionHandle, SubscriptionOptions};
#[derive(Parser)]
struct Cli {
    socket: SocketAddr,
}

struct Pipe {
    /// Pass queue of commands
    command_chan: mpsc::Sender<ClientCommand>,
}

impl Pipe {
    pub async fn run(self, path: std::path::PathBuf) {
        let mut fifo = pipe::OpenOptions::new()
            .open_receiver(path)
            .expect("Failed to read mkfifo file");

        loop {
            // Reopen FIFO every time the inner loop is `break`ed out of
            let mut stream = FramedRead::new(&mut fifo, ClientCodec);

            loop {
                let command = match stream.try_next().await {
                    Ok(Some(command)) => self
                        .command_chan
                        .send(command)
                        .await
                        .expect("Failed to pass command from file to command queue"),
                    Ok(None) => {
                        log::error!("Stream has been closed, reopening...");
                        break;
                    }
                    Err(e) => {
                        log::error!("Decoding error: {e}");
                        continue;
                    }
                };
            }
        }
    }
}

pub struct PipeHandle {
    pub task: JoinHandle<()>,
}

impl PipeHandle {
    pub fn new(command_chan: mpsc::Sender<ClientCommand>, path: std::path::PathBuf) -> Self {
        let actor = Pipe { command_chan };

        Self {
            task: tokio::spawn(actor.run(path)),
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    let Cli { socket } = Cli::parse();

    let mut connection = ConnectionHandle::connect(socket).await;
    let mut interval = tokio::time::interval(Duration::from_secs(5));

    let offset = FixedOffset::east_opt(5 * 60 * 60).unwrap();

    loop {
        interval.tick().await;

        let now = chrono::Utc::now().naive_utc();
        let timestamp = offset.from_utc_datetime(&now).to_rfc3339();

        println!("{timestamp}");
        connection
            .publish("time.us.east".into(), timestamp.into())
            .await
    }
}
