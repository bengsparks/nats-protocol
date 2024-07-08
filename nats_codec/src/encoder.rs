use std::io::Write;

use tokio_util::bytes::BufMut;

#[derive(thiserror::Error, Debug)]
pub enum ClientEncodeError {
    #[error("Underlying I/O Error: {0}")]
    IoError(#[from] std::io::Error),
}

impl tokio_util::codec::Encoder<crate::ClientCommand> for crate::ClientCodec {
    type Error = ClientEncodeError;

    fn encode(
        &mut self,
        item: crate::ClientCommand,
        dst: &mut tokio_util::bytes::BytesMut,
    ) -> Result<(), Self::Error> {
        match item {
            crate::ClientCommand::Connect(c) => connect(c, dst)?,
            crate::ClientCommand::Publish(p) => publish(p, dst)?,
            crate::ClientCommand::HPublish(h) => hpublish(h, dst)?,
            crate::ClientCommand::Subscribe(s) => subscribe(s, dst)?,
            crate::ClientCommand::Unsubscribe(u) => unsubscribe(u, dst)?,
            crate::ClientCommand::Ping => ping(dst)?,
            crate::ClientCommand::Pong => pong(dst)?,
        }

        Ok(())
    }
}

const CRLF: &str = "\r\n";

fn connect(
    connect: crate::Connect,
    dst: &mut tokio_util::bytes::BytesMut,
) -> Result<(), std::io::Error> {
    let mut writer = dst.writer();

    write!(writer, "CONNECT ")?;
    serde_json::to_writer(&mut writer, &connect)?;
    write!(writer, "{CRLF}")?;

    Ok(())
}

fn publish(p: crate::Publish, dst: &mut tokio_util::bytes::BytesMut) -> Result<(), std::io::Error> {
    let mut writer = dst.writer();

    let subject = p.subject;
    let bytes = p.bytes;
    let payload = p.payload;

    if let Some(reply_to) = p.reply_to {
        write!(writer, "PUB {subject} {reply_to} {bytes}{CRLF}")?;
        writer.write_all(&payload)?;
        write!(writer, "{CRLF}")?;
    } else {
        write!(writer, "PUB {subject} {bytes}{CRLF}")?;
        writer.write_all(&payload)?;
        write!(writer, "{CRLF}")?;
    }

    Ok(())
}

fn hpublish(
    p: crate::HPublish,
    dst: &mut tokio_util::bytes::BytesMut,
) -> Result<(), std::io::Error> {
    let mut writer = dst.writer();

    let subject = p.subject;
    write!(writer, "HPUB {subject} ")?;

    if let Some(reply_to) = p.reply_to {
        write!(writer, "{reply_to} ")?;
    }

    let header_bytes = p.header_bytes;
    let total_bytes = p.total_bytes;

    write!(writer, "{header_bytes} {total_bytes}{CRLF}")?;

    write!(writer, "NATS/1.0{CRLF}")?;
    for (key, values) in p.headers.0.into_iter() {
        for value in values {
            write!(writer, "{}: {}{CRLF}", key.0, value.0)?;
        }
    }
    write!(writer, "{CRLF}")?;

    let payload = p.payload;
    writer.write_all(&payload)?;
    write!(writer, "{CRLF}")?;

    Ok(())
}

fn subscribe(
    s: crate::Subscribe,
    dst: &mut tokio_util::bytes::BytesMut,
) -> Result<(), std::io::Error> {
    let mut writer = dst.writer();

    let subject = s.subject;
    let sid = s.sid;

    match s.queue_group {
        Some(qg) => write!(writer, "SUB {subject} {qg} {sid}{CRLF}")?,
        None => write!(writer, "SUB {subject} {sid}{CRLF}")?,
    };

    Ok(())
}

fn unsubscribe(
    u: crate::Unsubscribe,
    dst: &mut tokio_util::bytes::BytesMut,
) -> Result<(), std::io::Error> {
    let mut writer = dst.writer();

    let sid = u.sid;
    match u.max_msgs {
        Some(max_msgs) => write!(writer, "UNSUB {sid} {max_msgs}{CRLF}")?,
        None => write!(writer, "UNSUB {sid}{CRLF}")?,
    };

    Ok(())
}

fn ping(dst: &mut tokio_util::bytes::BytesMut) -> Result<(), std::io::Error> {
    let mut writer = dst.writer();
    write!(writer, "PING{CRLF}")?;

    Ok(())
}

fn pong(dst: &mut tokio_util::bytes::BytesMut) -> Result<(), std::io::Error> {
    let mut writer = dst.writer();
    write!(writer, "PONG{CRLF}")?;

    Ok(())
}
