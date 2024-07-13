use std::{collections::HashMap, num::NonZeroUsize};

mod decoder;
mod decoding;

mod encoder;

pub use decoding::{ClientDecodeError, ServerDecodeError};
use tokio_util::bytes::Bytes;

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
    pub payload: Bytes,
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
    pub headers: HeaderMap,
    pub payload: Bytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerCommand {
    // Clippy notes that `Info` contains at least 352 bytes, far more than the other variants
    // which is we `Box` it here
    Info(Box<Info>),
    Msg(Msg),
    HMsg(HMsg),
    Ping,
    Pong,
    Ok,
    Err(String),
}

#[derive(Debug)]
pub struct Message {
    pub subject: String,
    pub sid: String,
    pub reply_to: Option<String>,
    pub headers: HeaderMap,
    pub payload: Bytes,
}

impl std::convert::From<HMsg> for Message {
    fn from(value: HMsg) -> Self {
        Self {
            subject: value.subject,
            sid: value.sid,
            reply_to: value.reply_to,
            headers: value.headers,
            payload: value.payload,
        }
    }
}

impl std::convert::From<Msg> for Message {
    fn from(value: Msg) -> Self {
        Self {
            subject: value.subject,
            sid: value.sid,
            reply_to: value.reply_to,
            headers: HeaderMap(HashMap::new()),
            payload: value.payload,
        }
    }
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
pub struct Publish {
    pub subject: String,
    pub reply_to: Option<String>,
    pub bytes: usize,
    pub payload: Bytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HPublish {
    pub subject: String,
    pub reply_to: Option<String>,
    pub header_bytes: usize,
    pub total_bytes: usize,
    pub headers: HeaderMap,
    pub payload: Bytes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subscribe {
    pub subject: String,
    pub queue_group: Option<String>,
    pub sid: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unsubscribe {
    pub sid: String,
    pub max_msgs: Option<NonZeroUsize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientCommand {
    Connect(Connect),
    Publish(Publish),
    HPublish(HPublish),
    Subscribe(Subscribe),
    Unsubscribe(Unsubscribe),
    Ping,
    Pong,
}

pub struct ClientCodec;

#[cfg(test)]
mod msg {
    use crate::ServerCodec;
    use bytes::Bytes;
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
                payload: Bytes::from_static(b"Hello World"),
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
                payload: Bytes::from_static(b"Hello World"),
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }
}

#[cfg(test)]
mod hmsg {
    use std::collections::HashMap;

    use crate::{HeaderName, HeaderValue, ServerCodec};
    use tokio_stream::StreamExt;
    use tokio_util::bytes::Bytes;
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
                headers: crate::HeaderMap(HashMap::from_iter([(
                    HeaderName("FoodGroup".into()),
                    vec![HeaderValue("vegetable".into())]
                )])),
                payload: Bytes::from_static(b"Hello World"),
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
                headers: crate::HeaderMap(HashMap::from_iter([(
                    HeaderName("FoodGroup".into()),
                    vec![HeaderValue("vegetable".into())]
                )])),
                payload: Bytes::from_static(b"Hello World"),
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
    use bytes::Bytes;
    use tokio_stream::StreamExt;
    use tokio_util::codec::FramedRead;

    #[tokio::test]
    async fn r#pub() {
        let mut reader = FramedRead::new(&b"PUB FOO 11\r\nHello NATS!\r\n"[..], ClientCodec);
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::Publish(crate::Publish {
                subject: "FOO".into(),
                reply_to: None,
                bytes: 11,
                payload: Bytes::from_static(b"Hello NATS!")
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
            Some(crate::ClientCommand::Publish(crate::Publish {
                subject: "FRONT.DOOR".into(),
                reply_to: Some("JOKE.22".into()),
                bytes: 11,
                payload: Bytes::from_static(b"Knock Knock")
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn empty_msg() {
        let mut reader = FramedRead::new(&b"PUB NOTIFY 0\r\n\r\n"[..], ClientCodec);
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::Publish(crate::Publish {
                subject: "NOTIFY".into(),
                reply_to: None,
                bytes: 0,
                payload: Bytes::from_static(b""),
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }
}

#[cfg(test)]
mod subscribe {
    use std::num::NonZeroUsize;

    use crate::ClientCodec;
    use tokio_stream::StreamExt;
    use tokio_util::codec::FramedRead;

    #[tokio::test]
    async fn sub() {
        let mut reader = FramedRead::new(&b"SUB FOO 1\r\n"[..], ClientCodec);
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::Subscribe(crate::Subscribe {
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
            Some(crate::ClientCommand::Subscribe(crate::Subscribe {
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
            Some(crate::ClientCommand::Unsubscribe(crate::Unsubscribe {
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
            Some(crate::ClientCommand::Unsubscribe(crate::Unsubscribe {
                sid: "1".into(),
                max_msgs: NonZeroUsize::new(5),
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }
}

#[cfg(test)]
mod hpub {
    use std::collections::HashMap;

    use crate::{ClientCodec, HeaderName, HeaderValue};
    use tokio_stream::StreamExt;
    use tokio_util::bytes::Bytes;
    use tokio_util::codec::FramedRead;

    #[tokio::test]
    async fn simple() {
        let mut reader = FramedRead::new(
            &b"HPUB FOO 22 33\r\nNATS/1.0\r\nBar: Baz\r\n\r\nHello NATS!\r\n"[..],
            ClientCodec,
        );
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::HPublish(crate::HPublish {
                subject: "FOO".into(),
                reply_to: None,
                header_bytes: 22,
                total_bytes: 33,
                headers: crate::HeaderMap(HashMap::from_iter([(
                    HeaderName("Bar".into()),
                    vec![HeaderValue("Baz".into())]
                )])),
                payload: Bytes::from_static(b"Hello NATS!"),
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn reply_to() {
        let mut reader = FramedRead::new(
            &b"HPUB FRONT.DOOR JOKE.22 45 56\r\nNATS/1.0\r\nBREAKFAST: donut\r\nLUNCH: burger\r\n\r\nKnock Knock\r\n"[..],
            ClientCodec,
        );
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::HPublish(crate::HPublish {
                subject: "FRONT.DOOR".into(),
                reply_to: Some("JOKE.22".into()),
                header_bytes: 45,
                total_bytes: 56,
                headers: crate::HeaderMap(HashMap::from_iter([
                    (
                        HeaderName("BREAKFAST".into()),
                        vec![HeaderValue("donut".into())]
                    ),
                    (
                        HeaderName("LUNCH".into()),
                        vec![HeaderValue("burger".into())]
                    )
                ])),
                payload: Bytes::from_static(b"Knock Knock"),
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn empty_msg() {
        let mut reader = FramedRead::new(
            &b"HPUB NOTIFY 22 22\r\nNATS/1.0\r\nBar: Baz\r\n\r\n\r\n"[..],
            ClientCodec,
        );
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::HPublish(crate::HPublish {
                subject: "NOTIFY".into(),
                reply_to: None,
                header_bytes: 22,
                total_bytes: 22,
                headers: crate::HeaderMap(HashMap::from_iter([(
                    HeaderName("Bar".into()),
                    vec![HeaderValue("Baz".into())]
                ),])),
                payload: Bytes::from_static(b""),
            }))
        );
        assert_eq!(reader.try_next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn double_header() {
        let mut reader = FramedRead::new(
            &b"HPUB MORNING.MENU 47 51\r\nNATS/1.0\r\nBREAKFAST: donut\r\nBREAKFAST: eggs\r\n\r\nYum!\r\n"[..],
            ClientCodec,
        );
        assert_eq!(
            reader.try_next().await.unwrap(),
            Some(crate::ClientCommand::HPublish(crate::HPublish {
                subject: "MORNING.MENU".into(),
                reply_to: None,
                header_bytes: 47,
                total_bytes: 51,
                headers: crate::HeaderMap(HashMap::from_iter([(
                    HeaderName("BREAKFAST".into()),
                    vec![HeaderValue("donut".into()), HeaderValue("eggs".into())]
                ),])),
                payload: Bytes::from_static(b"Yum!"),
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
