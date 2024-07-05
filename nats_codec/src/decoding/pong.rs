use super::{slice_spliterator, ClientDecodeError, CommandDecoderResult, ServerDecodeError};

pub struct Decoder;

impl super::CommandDecoder<crate::ClientCommand, ClientDecodeError> for Decoder {
    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ClientCommand, ClientDecodeError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);
        let Some((b"", end)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort(None);
        };

        CommandDecoderResult::Advance((crate::ClientCommand::Pong, end))
    }
}

impl super::CommandDecoder<crate::ServerCommand, ServerDecodeError> for Decoder {
    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ServerCommand, ServerDecodeError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);
        let Some((b"", end)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort(None);
        };

        CommandDecoderResult::Advance((crate::ServerCommand::Pong, end))
    }
}
