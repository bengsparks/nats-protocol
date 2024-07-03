use super::{ClientDecodeError, CommandDecoderResult};

pub struct HPubDecoder;

impl super::CommandDecoder<crate::ClientCommand, ClientDecodeError> for HPubDecoder {
    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ClientCommand, ClientDecodeError> {
        CommandDecoderResult::WrongDecoder
    }
}
