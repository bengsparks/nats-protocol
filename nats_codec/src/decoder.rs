use memchr::memmem::find;
use tokio_util::bytes::{self, Buf};

use crate::{
    decoding::{
        connect, err, hmsg, hpub, info, msg, ok, ping, pong, publish, sub, unsub,
        ClientDecodeError, CommandDecoder, CommandDecoderResult, ServerDecodeError,
    },
    ClientCodec, ClientCommand,
};

use super::{ServerCodec, ServerCommand, BUFSIZE_LIMIT, CRLF};

impl tokio_util::codec::Decoder for ServerCodec {
    type Item = ServerCommand;
    type Error = ServerDecodeError;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let decoders: &[(&[u8], &dyn CommandDecoder<_, _>)] = &[
            (b"PING", &ping::Decoder),
            (b"PONG", &pong::Decoder),
            (b"HMSG ", &hmsg::Decoder),
            (b"MSG ", &msg::Decoder),
            (b"+OK", &ok::Decoder),
            (b"-ERR ", &err::Decoder),
            (b"INFO ", &info::Decoder),
        ];

        decoding(src, decoders)
    }
}

impl tokio_util::codec::Decoder for ClientCodec {
    type Item = ClientCommand;
    type Error = ClientDecodeError;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let decoders: &[(&[u8], &dyn CommandDecoder<_, _>)] = &[
            (b"PING", &ping::Decoder),
            (b"PONG", &pong::Decoder),
            (b"HPUB ", &hpub::Decoder),
            (b"PUB ", &publish::Decoder),
            (b"SUB ", &sub::Decoder),
            (b"UNSUB ", &unsub::Decoder),
            (b"CONNECT ", &connect::Decoder),
        ];

        decoding(src, decoders)
    }
}

fn decoding<T, E: CommonDecodeError, D: CommandDecoder<T, E> + ?Sized>(
    src: &mut bytes::BytesMut,
    decoders: &[(&'static [u8], &D)],
) -> Result<Option<T>, E> {
    let clamped_len = src.len().min(BUFSIZE_LIMIT);
    let Some(first_newline) = find(&src[..clamped_len], &CRLF) else {
        return if src.len() < BUFSIZE_LIMIT {
            Ok(None)
        } else {
            Err(E::exceeds_short_length())
        };
    };

    for (prefix, decoder) in decoders {
        if src.len() < prefix.len() {
            src.reserve(prefix.len());
            return Ok(None);
        }
        if !src[..prefix.len()].eq_ignore_ascii_case(prefix) {
            continue;
        }

        let (_matched, body) = src.split_at(prefix.len());
        match decoder.decode_body(body) {
            CommandDecoderResult::Advance((frame, consume)) => {
                src.advance(consume + prefix.len());
                return Ok(Some(frame));
            }
            CommandDecoderResult::FatalError(e) => return Err(e),
            CommandDecoderResult::FrameTooShort(Some(required)) => {
                src.reserve(required);
                return Ok(None);
            }
            CommandDecoderResult::FrameTooShort(None) => return Ok(None),
            CommandDecoderResult::WrongDecoder => {
                continue;
            }
        }
    }

    log::error!("Unknown command encountered; skipping until after next CRLF");
    src.advance(first_newline + 1);
    Err(E::unknown_command())
}

trait CommonDecodeError {
    fn exceeds_short_length() -> Self;
    fn unknown_command() -> Self;
}

impl CommonDecodeError for ClientDecodeError {
    fn exceeds_short_length() -> Self {
        Self::ExceedsSoftLength
    }

    fn unknown_command() -> Self {
        Self::UnknownCommand
    }
}

impl CommonDecodeError for ServerDecodeError {
    fn exceeds_short_length() -> Self {
        Self::ExceedsSoftLength
    }

    fn unknown_command() -> Self {
        Self::UnknownCommand
    }
}
