use super::{slice_spliterator, CommandDecoderResult, ServerDecodeError};

pub struct ErrDecoder;

impl super::CommandDecoder<crate::ServerCommand, ServerDecodeError> for ErrDecoder {
    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ServerCommand, ServerDecodeError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);
        let Some((message, end)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort(None);
        };

        if !(message.len() >= 2 && message.starts_with(b"'") && message.ends_with(b"'")) {
            return CommandDecoderResult::FatalError(ServerDecodeError::BadErr);
        };

        let Ok(decoded) = std::str::from_utf8(&message[1..message.len() - 1]) else {
            return CommandDecoderResult::FatalError(ServerDecodeError::BadErr);
        };
        CommandDecoderResult::Advance((crate::ServerCommand::Err(decoded.into()), end))
    }
}
