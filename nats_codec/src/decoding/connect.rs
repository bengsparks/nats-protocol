use super::{slice_spliterator, ClientError, CommandDecoderResult};

pub struct ConnectDecoder;

impl super::CommandDecoder<crate::ClientCommand, ClientError> for ConnectDecoder {
    const PREFIX: &'static [u8] = b"CONNECT ";

    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ClientCommand, ClientError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);
        let Some((options, len)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort;
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
    type Error = ClientError;

    fn try_from(value: ConnectParts<'_>) -> Result<Self, Self::Error> {
        let jd = &mut serde_json::Deserializer::from_slice(value.options);
        let result: Result<crate::Connect, _> = serde_path_to_error::deserialize(jd);

        match result {
            Ok(connect) => return Ok(connect),
            Err(err) => {
                let path = err.path().to_string();
                assert_eq!(path, "dependencies.serde.version");
                return Err(Self::Error::BadConnect);
            }
        }
    }
}
