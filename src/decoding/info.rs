use super::{slice_spliterator, CommandDecoderResult, ServerError};

pub struct InfoDecoder;

impl super::CommandDecoder<crate::ServerCommand, ServerError> for InfoDecoder {
    const PREFIX: &'static [u8] = b"INFO ";

    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ServerCommand, ServerError> {
        let mut spliterator = slice_spliterator(buffer, &crate::CRLF);

        let Some((slice, ending)) = spliterator.next() else {
            return CommandDecoderResult::FatalError(ServerError::BadInfo);
        };
        let Ok(info) = serde_json::from_slice(slice) else {
            return CommandDecoderResult::FatalError(ServerError::BadInfo);
        };

        CommandDecoderResult::Advance((crate::ServerCommand::Info(info), ending))
    }
}
