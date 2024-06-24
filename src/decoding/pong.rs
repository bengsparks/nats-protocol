use super::{slice_spliterator, ClientError, CommandDecoderResult, ServerError};

pub struct PongDecoder;

impl super::CommandDecoder<crate::ServerCommand, ServerError> for PongDecoder {
    const PREFIX: &'static [u8] = b"PONG";

    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ServerCommand, ServerError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);
        let Some((b"", end)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort;
        };

        CommandDecoderResult::Advance((crate::ServerCommand::Pong, end))
    }
}

impl super::CommandDecoder<crate::ClientCommand, ClientError> for PongDecoder {
    const PREFIX: &'static [u8] = b"PONG";

    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ClientCommand, ClientError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);
        let Some((b"", end)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort;
        };

        CommandDecoderResult::Advance((crate::ClientCommand::Pong, end))
    }
}
