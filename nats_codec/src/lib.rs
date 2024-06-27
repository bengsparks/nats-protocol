use std::collections::HashMap;

mod decoder;
mod decoding;

mod encoder;

pub(crate) const BUFSIZE_LIMIT: usize = u16::MAX as usize;

const CR: u8 = 0x0D;
const LF: u8 = 0x0A;
const CRLF: [u8; 2] = [CR, LF];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Info {
    pub server_id: String,
    pub server_name: String,
    pub version: String,
    pub go: String,
    pub host: String,
    pub port: u32,
    pub headers: bool,
    pub max_payload: usize,
    pub proto: u8,
    pub client_id: Option<u64>,
    pub auth_required: Option<bool>,
    pub tls_required: Option<bool>,
    pub tls_verify: Option<bool>,
    pub tls_available: Option<bool>,
    pub connect_urls: Option<Vec<String>>,
    pub ws_connect_urls: Option<Vec<String>>,
    pub ldm: Option<bool>,
    pub git_commit: Option<String>,
    pub jetstream: Option<bool>,
    pub ip: Option<String>,
    pub client_ip: Option<String>,
    pub nonce: Option<String>,
    pub cluster: Option<String>,
    pub domain: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Msg {
    pub subject: String,
    pub sid: String,
    pub reply_to: Option<String>,
    pub bytes: usize,
    pub payload: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct HeaderName(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderValue(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderMap(HashMap<HeaderName, Vec<HeaderValue>>);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    name: HeaderName,
    value: HeaderValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HMsg {
    pub subject: String,
    pub sid: String,
    pub reply_to: Option<String>,
    pub header_bytes: usize,
    pub total_bytes: usize,
    pub headers: Option<HeaderMap>,
    pub payload: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerCommand {
    Info(Info),
    Msg(Msg),
    HMsg(HMsg),
    Ping,
    Pong,
    Ok,
    Err(String),
}

pub struct ServerCodec;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Connect {
    pub verbose: bool,
    pub pedantic: bool,
    pub tls_required: bool,
    pub auth_token: Option<String>,
    pub user: Option<String>,
    pub pass: Option<String>,
    pub name: Option<String>,
    pub lang: String,
    pub version: String,
    pub protocol: Option<u8>,
    pub echo: Option<bool>,

    pub sig: Option<String>,
    pub jwt: Option<String>,
    pub no_responders: Option<bool>,
    pub headers: Option<bool>,
    pub nkey: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pub {
    pub subject: String,
    pub reply_to: Option<String>,
    pub bytes: usize,
    pub payload: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientCommand {
    Connect(Connect),
    Pub(Pub),
    HPub,
    Sub(Sub),
    Unsub(Unsub),
    Ping,
    Pong,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sub {
    pub subject: String,
    pub queue_group: Option<String>,
    pub sid: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unsub {
    pub sid: String,
    pub max_msgs: Option<usize>,
}

pub struct ClientCodec;

#[cfg(test)]
mod msg {
    use crate::ServerCodec;
    use tokio_stream::StreamExt;
    use tokio_util::codec::FramedRead;

    #[tokio::test]
    async fn short() {
        let mut reader = FramedRead::new(&b"MSG FOO.BAR 9 11\r\nHello World\r\n"[..], ServerCodec);
        assert_eq!(
            reader.try_next().await.expect("Failed to match"),
            Some(crate::ServerCommand::Msg(crate::Msg {
                subject: "FOO.BAR".into(),
                sid: "9".into(),
                reply_to: None,
                bytes: 11,
                payload: Some("Hello World".into())
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn long() {
        let mut reader = FramedRead::new(
            &b"MSG FOO.BAR 9 GREETING.34 11\r\nHello World\r\n"[..],
            ServerCodec,
        );
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ServerCommand::Msg(crate::Msg {
                subject: "FOO.BAR".into(),
                sid: "9".into(),
                reply_to: Some("GREETING.34".into()),
                bytes: 11,
                payload: Some("Hello World".into())
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }
}

#[cfg(test)]
mod hmsg {
    use crate::ServerCodec;
    use tokio_stream::StreamExt;
    use tokio_util::codec::FramedRead;

    #[tokio::test]
    async fn short() {
        let mut reader = FramedRead::new(
            &b"HMSG FOO.BAR 9 34 45\r\nNATS/1.0\r\nFoodGroup: vegetable\r\n\r\nHello World\r\n"[..],
            ServerCodec,
        );
        assert_eq!(
            reader.try_next().await.expect("Failed to match"),
            Some(crate::ServerCommand::HMsg(crate::HMsg {
                subject: "FOO.BAR".into(),
                sid: "9".into(),
                reply_to: None,
                header_bytes: 34,
                total_bytes: 45,
                headers: Some("FoodGroup: vegetable".parse().unwrap()),
                payload: Some("Hello World".into())
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn long() {
        let mut reader = FramedRead::new(
            &b"HMSG FOO.BAR 9 BAZ.69 34 45\r\nNATS/1.0\r\nFoodGroup: vegetable\r\n\r\nHello World\r\n"[..],
            ServerCodec,
        );
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ServerCommand::HMsg(crate::HMsg {
                subject: "FOO.BAR".into(),
                sid: "9".into(),
                reply_to: Some("BAZ.69".into()),
                header_bytes: 34,
                total_bytes: 45,
                headers: Some("FoodGroup: vegetable".parse().unwrap()),
                payload: Some("Hello World".into())
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }
}

#[cfg(test)]
mod pingpong {
    use crate::{ClientCodec, ServerCodec};
    use tokio_stream::StreamExt;
    use tokio_util::codec::FramedRead;

    #[tokio::test]
    async fn ping_server() {
        let mut reader = FramedRead::new(&b"PING\r\n"[..], ServerCodec);
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ServerCommand::Ping)
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn pong_server() {
        let mut reader = FramedRead::new(&b"PONG\r\n"[..], ServerCodec);
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ServerCommand::Pong)
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn ping_client() {
        let mut reader = FramedRead::new(&b"PING\r\n"[..], ClientCodec);
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::Ping)
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn pong_client() {
        let mut reader = FramedRead::new(&b"PONG\r\n"[..], ClientCodec);
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::Pong)
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }
}

#[cfg(test)]
mod okerr {
    use crate::ServerCodec;
    use tokio_stream::StreamExt;
    use tokio_util::codec::FramedRead;

    #[tokio::test]
    async fn ok() {
        let mut reader = FramedRead::new(&b"+OK\r\n"[..], ServerCodec);
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ServerCommand::Ok)
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn err() {
        let mut reader =
            FramedRead::new(&b"-ERR 'Unknown Protocol Operation'\r\n"[..], ServerCodec);
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ServerCommand::Err(
                "Unknown Protocol Operation".into()
            ))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }
}

#[cfg(test)]
mod publish {
    use crate::ClientCodec;
    use tokio_stream::StreamExt;
    use tokio_util::codec::FramedRead;

    #[tokio::test]
    async fn r#pub() {
        let mut reader = FramedRead::new(&b"PUB FOO 11\r\nHello NATS!\r\n"[..], ClientCodec);
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::Pub(crate::Pub {
                subject: "FOO".into(),
                reply_to: None,
                bytes: 11,
                payload: Some("Hello NATS!".into())
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn pub_reply_to() {
        let mut reader = FramedRead::new(
            &b"PUB FRONT.DOOR JOKE.22 11\r\nKnock Knock\r\n"[..],
            ClientCodec,
        );
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::Pub(crate::Pub {
                subject: "FRONT.DOOR".into(),
                reply_to: Some("JOKE.22".into()),
                bytes: 11,
                payload: Some("Knock Knock".into())
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn empty_msg() {
        let mut reader = FramedRead::new(&b"PUB NOTIFY 0\r\n\r\n"[..], ClientCodec);
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::Pub(crate::Pub {
                subject: "NOTIFY".into(),
                reply_to: None,
                bytes: 0,
                payload: None
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }
}

#[cfg(test)]
mod subscribe {
    use crate::ClientCodec;
    use tokio_stream::StreamExt;
    use tokio_util::codec::FramedRead;

    #[tokio::test]
    async fn sub() {
        let mut reader = FramedRead::new(&b"SUB FOO 1\r\n"[..], ClientCodec);
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::Sub(crate::Sub {
                subject: "FOO".into(),
                queue_group: None,
                sid: "1".into(),
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn sub_queue_group() {
        let mut reader = FramedRead::new(&b"SUB BAR G1 44\r\n"[..], ClientCodec);
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::Sub(crate::Sub {
                subject: "BAR".into(),
                queue_group: Some("G1".into()),
                sid: "44".into(),
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn unsub() {
        let mut reader = FramedRead::new(&b"UNSUB 1\r\n"[..], ClientCodec);
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::Unsub(crate::Unsub {
                sid: "1".into(),
                max_msgs: None,
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn unsub_max_msgs() {
        let mut reader = FramedRead::new(&b"UNSUB 1 5\r\n"[..], ClientCodec);
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::Unsub(crate::Unsub {
                sid: "1".into(),
                max_msgs: Some(5),
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }
}

#[cfg(test)]
mod connection {
    use crate::ClientCodec;
    use tokio_stream::StreamExt;
    use tokio_util::codec::FramedRead;

    #[tokio::test]
    async fn connect() {
        let mut reader = FramedRead::new(concat!(
            r#"CONNECT {"verbose":false,"pedantic":false,"tls_required":false,"name":"","lang":"go","version":"1.2.2","protocol":1}"#, 
            "\r\n"
        ).as_bytes(), ClientCodec);
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::Connect(crate::Connect {
                verbose: false,
                pedantic: false,
                tls_required: false,
                auth_token: None,
                user: None,
                pass: None,
                name: Some("".into()),
                lang: "go".into(),
                version: "1.2.2".into(),
                protocol: Some(1),
                echo: None,
                sig: None,
                jwt: None,
                no_responders: None,
                headers: None,
                nkey: None
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }
}
