use super::{slice_spliterator, CommandDecoderResult, ServerDecodeError};

pub struct InfoDecoder;

impl super::CommandDecoder<crate::ServerCommand, ServerDecodeError> for InfoDecoder {
    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ServerCommand, ServerDecodeError> {
        let mut spliterator = slice_spliterator(buffer, &crate::CRLF);

        let Some((slice, ending)) = spliterator.next() else {
            return CommandDecoderResult::FatalError(ServerDecodeError::BadInfo);
        };
        let Ok(info) = serde_json::from_slice(slice) else {
            return CommandDecoderResult::FatalError(ServerDecodeError::BadInfo);
        };

        CommandDecoderResult::Advance((crate::ServerCommand::Info(info), ending))
    }
}
