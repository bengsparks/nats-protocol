use super::{slice_spliterator, ClientError, CommandDecoderResult, ServerError};

pub struct PingDecoder;

impl super::CommandDecoder<crate::ServerCommand, ServerError> for PingDecoder {
    const PREFIX: &'static [u8] = b"PING";

    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ServerCommand, ServerError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);
        let Some((b"", end)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort;
        };

        CommandDecoderResult::Advance((crate::ServerCommand::Ping, end))
    }
}

impl super::CommandDecoder<crate::ClientCommand, ClientError> for PingDecoder {
    const PREFIX: &'static [u8] = b"PING";

    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ClientCommand, ClientError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);
        let Some((b"", end)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort;
        };

        CommandDecoderResult::Advance((crate::ClientCommand::Ping, end))
    }
}
