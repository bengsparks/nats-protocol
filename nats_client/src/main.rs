use std::net::SocketAddr;

use clap::Parser;

use futures::SinkExt;
use tokio::net::unix::pipe;
use tokio_stream::StreamExt as _;
use tokio_util::{
    codec::{FramedRead, FramedWrite},
    sync::PollSender,
};

use nats_codec::{ClientCodec, ClientCommand, Connect, Info, ServerCodec, ServerCommand};

#[derive(Parser)]
struct Cli {
    socket: SocketAddr,
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();
    env_logger::init();

    let (mut read, mut write) = tokio::net::TcpStream::connect(args.socket)
        .await
        .expect("Failed to connect to NATS server")
        .into_split();
    log::debug!("Successfully connected to {}", args.socket);

    let (info_notifier, info_receiver) = tokio::sync::oneshot::channel::<Info>();
    let (cmd_snd, mut cmd_rcv) = tokio::sync::mpsc::channel::<ClientCommand>(1024 * 1024);

    let (ready_notifier, ready_receiver) = tokio::sync::broadcast::channel::<()>(64);

    let (mut can_send_msgs, recv2send) = (ready_notifier.subscribe(), cmd_snd.clone());
    let recv_task = tokio::spawn(async move {
        log::debug!("Awaiting `INFO` message");
        let mut stream = FramedRead::new(&mut read, ServerCodec);
        let Ok(Some(ServerCommand::Info(info))) = stream.try_next().await else {
            panic!("Expected first message to be INFO! Have you connected to a real NATS server? :eyes:");
        };

        log::info!("Received {:#?}", info);
        info_notifier
            .send(info)
            .expect("Failed to send information required to establish connection! :sad_face:");
        let _ = can_send_msgs
            .recv()
            .await
            .expect("Failed to await `CONNECT` transmission");

        while let Ok(Some(command)) = stream.try_next().await {
            log::trace!("Server sent {command:?}");

            let response = match command {
                ServerCommand::Info(_) => todo!(),
                ServerCommand::Msg(msg) => {
                    log::info!("Server sent message: {msg:?}");
                    continue;
                },
                ServerCommand::HMsg(_) => todo!(),
                ServerCommand::Ping => ClientCommand::Pong,
                ServerCommand::Pong => {
                    continue;
                }
                ServerCommand::Ok => {
                    continue;
                }
                ServerCommand::Err(message) => {
                    log::error!("{message}");
                    continue;
                }
            };
            recv2send.send(response).await.expect("Failed to send");
        }
    });

    let (mut can_send_ping, ping2send) = (ready_notifier.subscribe(), cmd_snd.clone());
    let ping_task = tokio::spawn(async move {
        can_send_ping
            .recv()
            .await
            .expect("Failed to receive connection confirmation to start sending PING");
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(20));
        loop {
            interval.tick().await;
            ping2send
                .send(ClientCommand::Ping)
                .await
                .expect("Failed to send PONG!");
        }
    });

    let (mut console_ready, console_sender) = (ready_notifier.subscribe(), cmd_snd.clone());
    let pipe_task = tokio::spawn(async move {
        console_ready
            .recv()
            .await
            .expect("Failed to receive confirmation to start sending messages from pipe");

        let fifo = pipe::OpenOptions::new()
            .open_receiver("./nats-client-input")
            .expect("Failed to read mkfifo file `nats-client-input`");

        let mut stream = FramedRead::new(fifo, ClientCodec);
        let mut sink = PollSender::new(console_sender);

        while let Ok(Some(command)) = stream.try_next().await {
            sink.send(command)
                .await
                .expect("Failed to pass command from stdin to command queue");
        }
    });

    let send_task = tokio::spawn(async move {
        use futures::sink::SinkExt;

        let _info = info_receiver
            .await
            .expect("Failed to receive information required to establish connection! :sad_face:");
        let mut sink = FramedWrite::new(&mut write, ClientCodec);

        sink.send(ClientCommand::Connect(Connect {
            // From INFO
            sig: None,
            nkey: None,
            // Hardcoded
            verbose: true,
            pedantic: true,
            tls_required: false,
            auth_token: None,
            user: None,
            lang: "Rust".into(),
            name: None,
            pass: None,
            version: "1.0".into(),
            protocol: None,
            echo: None,
            jwt: None,
            no_responders: None,
            headers: None,
        }))
        .await
        .expect("Failed to send CONNECT");

        ready_notifier
            .send(())
            .expect("Failed to send connection confirmation signal to subtasks");
        while let Some(command) = cmd_rcv.recv().await {
            if !matches!(command, ClientCommand::Ping | ClientCommand::Pong) {
                log::info!("Client sent {command:?}");
            } else {
                log::trace!("Client sent {command:?}");
            }
            sink.send(command)
                .await
                .expect("Failed to send command to server!");
        }

        log::error!("Send task finished; terminating...")
    });

    drop(ready_receiver);

    let res = tokio::try_join!(recv_task, send_task, ping_task, pipe_task);
    if let Err(e) = res {
        if e.is_panic() {
            std::panic::resume_unwind(e.into_panic());
        }
    }
}
