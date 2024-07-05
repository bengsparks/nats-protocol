use super::{ClientDecodeError, CommandDecoderResult};

pub struct Decoder;

impl super::CommandDecoder<crate::ClientCommand, ClientDecodeError> for Decoder {
    fn decode_body(
        &self,
        _buffer: &[u8],
    ) -> CommandDecoderResult<crate::ClientCommand, ClientDecodeError> {
        CommandDecoderResult::WrongDecoder
    }
}
