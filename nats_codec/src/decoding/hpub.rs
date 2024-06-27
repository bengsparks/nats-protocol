use super::{ClientError, CommandDecoderResult};

pub struct HPubDecoder;

impl super::CommandDecoder<crate::ClientCommand, ClientError> for HPubDecoder {
    const PREFIX: &'static [u8] = b"HPUB ";

    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ClientCommand, ClientError> {
        CommandDecoderResult::WrongDecoder
    }
}
