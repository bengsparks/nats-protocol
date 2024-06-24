use super::{slice_spliterator, CommandDecoderResult, ServerError};

pub struct ErrDecoder;

impl super::CommandDecoder<crate::ServerCommand, ServerError> for ErrDecoder {
    const PREFIX: &'static [u8] = b"-ERR ";

    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ServerCommand, ServerError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);
        let Some((message, end)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort;
        };

        if !(message.len() >= 2 && message.starts_with(b"'") && message.ends_with(b"'")) {
            return CommandDecoderResult::FatalError(ServerError::BadErr);
        };

        let Ok(decoded) = std::str::from_utf8(&message[1..message.len() - 1]) else {
            return CommandDecoderResult::FatalError(ServerError::BadErr);
        };
        CommandDecoderResult::Advance((crate::ServerCommand::Err(decoded.into()), end))
    }
}
