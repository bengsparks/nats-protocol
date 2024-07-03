use super::{slice_spliterator, ClientDecodeError, CommandDecoderResult};

pub struct ConnectDecoder;

impl super::CommandDecoder<crate::ClientCommand, ClientDecodeError> for ConnectDecoder {
    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ClientCommand, ClientDecodeError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);
        let Some((options, len)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort(None);
        };

        let parts = ConnectParts { options };
        let connect = match parts.try_into() {
            Ok(hmsg) => hmsg,
            Err(e) => return CommandDecoderResult::FatalError(e),
        };

        CommandDecoderResult::Advance((crate::ClientCommand::Connect(connect), len))
    }
}

struct ConnectParts<'a> {
    options: &'a [u8],
}

impl std::convert::TryFrom<ConnectParts<'_>> for crate::Connect {
    type Error = ClientDecodeError;

    fn try_from(value: ConnectParts<'_>) -> Result<Self, Self::Error> {
        serde_json::from_slice(value.options).map_err(|_| Self::Error::BadConnect)
    }
}
