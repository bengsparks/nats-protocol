use memchr::memmem::find;
use tokio_util::bytes::{self, Buf};

use crate::{
    decoding::{
        ClientError as CE, CommandDecoder, CommandDecoderResult, ErrDecoder, HMsgDecoder,
        HPubDecoder, InfoDecoder, MsgDecoder, OkDecoder, PingDecoder, PongDecoder, PubDecoder,
        ServerError as SE, SubDecoder, UnsubDecoder,
    },
    ClientCodec, ClientCommand,
};

use super::{ServerCodec, ServerCommand, BUFSIZE_LIMIT, CRLF};

impl tokio_util::codec::Decoder for ServerCodec {
    type Item = ServerCommand;
    type Error = SE;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let clamped_len = src.len().min(BUFSIZE_LIMIT);

        if find(&src[..clamped_len], &CRLF).is_none() {
            return if src.len() < BUFSIZE_LIMIT {
                Ok(None)
            } else {
                Err(SE::ExceedsSoftLength)
            };
        }

        let decode_chain = InfoDecoder
            .decode(src)
            .chain(MsgDecoder.bind(src))
            .chain(HMsgDecoder.bind(src))
            .chain(<PingDecoder as CommandDecoder<_, SE>>::bind(
                &PingDecoder,
                src,
            ))
            .chain(<PongDecoder as CommandDecoder<_, SE>>::bind(
                &PongDecoder,
                src,
            ))
            .chain(OkDecoder.bind(src))
            .chain(ErrDecoder.bind(src));

        match decode_chain {
            CommandDecoderResult::Advance((frame, consume)) => {
                src.advance(consume);
                dbg!(&src);
                Ok(Some(frame))
            }
            CommandDecoderResult::FatalError(e) => Err(e),
            CommandDecoderResult::FrameTooShort => Ok(None),
            CommandDecoderResult::WrongDecoder => Err(SE::UnknownCommand),
        }
    }
}

impl tokio_util::codec::Decoder for ClientCodec {
    type Item = ClientCommand;
    type Error = CE;

    fn decode(&mut self, src: &mut bytes::BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let clamped_len = src.len().min(BUFSIZE_LIMIT);

        if find(&src[..clamped_len], &CRLF).is_none() {
            return if src.len() < BUFSIZE_LIMIT {
                Ok(None)
            } else {
                Err(CE::ExceedsSoftLength)
            };
        }

        let decode_chain = PubDecoder
            .decode(src)
            .chain(HPubDecoder.bind(src))
            .chain(SubDecoder.bind(src))
            .chain(UnsubDecoder.bind(src))
            .chain(<PingDecoder as CommandDecoder<_, CE>>::bind(
                &PingDecoder,
                src,
            ))
            .chain(<PongDecoder as CommandDecoder<_, CE>>::bind(
                &PongDecoder,
                src,
            ));

        match decode_chain {
            CommandDecoderResult::Advance((frame, consume)) => {
                src.advance(consume);
                dbg!(&src);
                Ok(Some(frame))
            }
            CommandDecoderResult::FatalError(e) => Err(e),
            CommandDecoderResult::FrameTooShort => Ok(None),
            CommandDecoderResult::WrongDecoder => Err(CE::UnknownCommand),
        }
    }
}
