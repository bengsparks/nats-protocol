use super::{slice_spliterator, CommandDecoderResult, ServerError};

pub struct OkDecoder;

impl super::CommandDecoder<crate::ServerCommand, ServerError> for OkDecoder {
    const PREFIX: &'static [u8] = b"+OK";

    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ServerCommand, ServerError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);
        let Some((b"", end)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort;
        };

        CommandDecoderResult::Advance((crate::ServerCommand::Ok, end))
    }
}
