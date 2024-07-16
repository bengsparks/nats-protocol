use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use nats_sans_io::{NatsBinding, SubscribeResponse};
use tokio::{
    io::{BufReader, BufWriter},
    net::TcpStream,
    sync::{mpsc, oneshot},
    time,
};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::codec::{FramedRead, FramedWrite};

use super::{Subscriber, SubscriptionOptions};

#[derive(Debug)]
pub enum NatsError {}

pub struct NatsOverTcp {
    conn: TcpStream,
}

impl NatsOverTcp {
    pub fn new(tcp: TcpStream) -> Self {
        Self { conn: tcp }
    }

    pub async fn run(
        self,
        timeouts: nats_sans_io::Timeouts,
        chan: oneshot::Sender<UserHandle>,
    ) -> Result<(), NatsError> {
        let Self { conn: tcp } = self;
        let (reader, writer) = tcp.into_split();

        let (mut reader, mut writer) = (
            FramedRead::new(BufReader::new(reader), nats_codec::ServerCodec),
            FramedWrite::new(BufWriter::new(writer), nats_codec::ClientCodec),
        );

        let mut send_ping_ticker = time::interval(Duration::from_secs(5));
        let mut recv_ping_ticker = time::interval(Duration::from_secs(5));
        let mut recv_pong_ticker = time::interval(Duration::from_secs(5));

        let mut binding = NatsBinding::new(timeouts);

        let (sender, mut receiver) = mpsc::channel(1024 * 1024);
        chan.send(UserHandle {
            chan: sender.clone(),
        })
        .unwrap();

        loop {
            if let Some(transmit) = binding.poll_transmit() {
                let _ = writer.send(transmit).await;
                continue;
            }

            tokio::select! {
                // TODO: Make the handle_* functions error when connection has been lost so we can break here
                time = send_ping_ticker.tick() => {
                    let _ = binding.handle_send_ping_timeout(time.into());
                }
                time = recv_ping_ticker.tick() => {
                    let _ = binding.handle_send_pong_timeout(time.into());
                }
                time = recv_pong_ticker.tick() => {
                    let _ = binding.handle_keep_alive_timeout(time.into());
                }
                Some(command) = receiver.recv() => {
                    binding.handle_client_input(command, Instant::now());
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

            if let Some(tick) = binding.poll_keep_alive_timeout() {
                recv_pong_ticker.reset_at(tick.into());
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct UserHandle {
    chan: mpsc::Sender<nats_sans_io::ConnectionCommand>,
}

impl UserHandle {
    pub async fn subscribe(&self, subject: String, options: SubscriptionOptions) -> Subscriber {
        let (sender, receiver) = oneshot::channel();
        let _ = self
            .chan
            .send(nats_sans_io::ConnectionCommand::Subscribe {
                subject,
                options,
                sender,
            })
            .await;

        let SubscribeResponse {
            msg_chan,
            sid,
            max_msgs,
        } = receiver.await.unwrap();

        let messages = if let Some(limit) = max_msgs {
            ReceiverStream::new(msg_chan).take(limit.into()).boxed()
        } else {
            ReceiverStream::new(msg_chan).boxed()
        };

        Subscriber {
            sid,
            conn_chan: self.chan.clone(),
            messages,
        }
    }

    pub async fn publish(&self, subject: String, message: tokio_util::bytes::Bytes) {
        self.chan
            .send(nats_sans_io::ConnectionCommand::Publish {
                subject,
                payload: message,
            })
            .await
            .unwrap();
    }

    pub async fn close(self) {
        /* 
        self.chan
            .send(nats_sans_io::ConnectionCommand::Close)
            .await
            .unwrap();
        */
    }
}
