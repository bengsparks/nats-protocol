use super::{slice_spliterator, CommandDecoderResult, ServerDecodeError};

pub struct Decoder;

impl super::CommandDecoder<crate::ServerCommand, ServerDecodeError> for Decoder {
    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ServerCommand, ServerDecodeError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);
        let Some((b"", end)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort(None);
        };

        CommandDecoderResult::Advance((crate::ServerCommand::Ok, end))
    }
}
