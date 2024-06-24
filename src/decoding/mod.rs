mod err;
mod header;
mod hmsg;
mod hpub;
mod info;
mod msg;
mod ok;
mod ping;
mod pong;
mod r#pub;
mod sub;
mod unsub;

use memchr::memmem;
use std::io;
use tokio_util::bytes::{self, Buf};

pub use err::ErrDecoder;
pub use hmsg::HMsgDecoder;
pub use hpub::HPubDecoder;
pub use info::InfoDecoder;
pub use msg::MsgDecoder;
pub use ok::OkDecoder;
pub use ping::PingDecoder;
pub use pong::PongDecoder;
pub use r#pub::PubDecoder;
pub use sub::SubDecoder;
pub use unsub::UnsubDecoder;

pub trait CommandDecoder<T, E> {
    const PREFIX: &'static [u8];

    fn bind(&self, buffer: &mut bytes::BytesMut) -> impl FnOnce() -> CommandDecoderResult<T, E> {
        move || self.decode(buffer)
    }

    fn decode(&self, buffer: &mut bytes::BytesMut) -> CommandDecoderResult<T, E> {
        if buffer[..Self::PREFIX.len()].eq_ignore_ascii_case(Self::PREFIX) {
            buffer.advance(Self::PREFIX.len())
        } else {
            return CommandDecoderResult::WrongDecoder;
        };

        self.decode_body(&buffer)
    }

    fn decode_body(&self, buffer: &[u8]) -> CommandDecoderResult<T, E>;
}

#[derive(thiserror::Error, Debug)]
pub enum ServerError {
    #[error("Message is too long to fit into buffer")]
    ExceedsSoftLength,

    #[error("INFO's body is malformed")]
    BadInfo,

    #[error("MSG's body is malformed")]
    BadMsg,

    #[error("HMSG's body is malformed")]
    BadHMsg,

    #[error("Ping is malformed")]
    BadPing,

    #[error("Pong is malformed")]
    BadPong,

    #[error("+OK is malformed")]
    BadOk,

    #[error("-ERR is malformed")]
    BadErr,

    #[error("Headers are malformed")]
    BadHeaders,

    #[error("Command is unknown")]
    UnknownCommand,

    #[error("Underlying I/O Error: {0}")]
    IoError(#[from] io::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum ClientError {
    #[error("Message is too long to fit into buffer")]
    ExceedsSoftLength,

    #[error("SUB's body is malformed")]
    BadSub,

    #[error("UNSUB's body is malformed")]
    BadUnsub,

    #[error("PUB's body is malformed")]
    BadPub,

    #[error("HPUB's body is malformed")]
    BadHPub,

    #[error("Headers are malformed")]
    BadHeaders,

    #[error("Command is unknown")]
    UnknownCommand,

    #[error("Underlying I/O Error: {0}")]
    IoError(#[from] io::Error),
}

pub enum CommandDecoderResult<T, E> {
    /// Success: Frame consumed, `buffer` should be advanced.
    Advance((T, usize)),

    /// Fatal error: `PREFIX` was matched, but an unrecoverable error occured thereafter.
    /// This frame should be dropped.
    FatalError(E),

    /// Nonfatal error: Buffer is shorter than full frame
    /// Decoder should read more buffer into memory and retry.
    FrameTooShort,

    /// Nonfatal error: the `PREFIX` could not be detected
    /// Decoder should try a different command.
    WrongDecoder,
}

impl<T, E> CommandDecoderResult<T, E> {
    pub fn chain(self, decoder: impl FnOnce() -> CommandDecoderResult<T, E>) -> Self {
        match self {
            CommandDecoderResult::WrongDecoder => decoder(),

            CommandDecoderResult::Advance((frame, bytes)) => {
                CommandDecoderResult::Advance((frame, bytes))
            }

            CommandDecoderResult::FatalError(e) => CommandDecoderResult::FatalError(e),
            CommandDecoderResult::FrameTooShort => CommandDecoderResult::FrameTooShort,
        }
    }
}

pub(crate) fn slice_spliterator<'a>(
    bytes: &'a [u8],
    needle: &'static [u8],
) -> impl Iterator<Item = (&'a [u8], usize)> {
    memmem::find_iter(bytes, needle).scan(0usize, |acc, curr| {
        // Slice from beginning to just before needle
        let slice = &bytes[*acc..curr];
        // Skip needle
        *acc = curr + needle.len();
        // Return slice and the length of the consumed segment including the terminator
        Some((slice, *acc))
    })
}

pub(crate) fn char_spliterator<'a>(
    bytes: &'a [u8],
    needle: u8,
) -> impl Iterator<Item = (&'a [u8], usize)> {
    memchr::memchr_iter(needle, &bytes).scan(0usize, |acc, curr| {
        // Slice from beginning to just before needle
        let slice = &bytes[*acc..curr];
        // Skip needle (length is known here)
        *acc = curr + 1;
        //
        Some((slice, *acc))
    })
}
