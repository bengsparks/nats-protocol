use super::{char_spliterator, slice_spliterator, ClientError, CommandDecoderResult};

pub struct UnsubDecoder;

impl super::CommandDecoder<crate::ClientCommand, ClientError> for UnsubDecoder {
    const PREFIX: &'static [u8] = b"UNSUB ";

    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ClientCommand, ClientError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);
        let Some((metadata, end)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort;
        };
        let mut meta_iter = char_spliterator(metadata, b' ');

        let (sid, max_msgs) = match (meta_iter.next(), meta_iter.next()) {
            (Some((sid, last)), None) => (sid, Some(&metadata[last..])),
            (None, None) => (metadata, None),
            _ => return CommandDecoderResult::FatalError(ClientError::BadUnsub),
        };

        let parts = UnsubParts { sid, max_msgs };

        let unsub = match parts.try_into() {
            Ok(unsub) => unsub,
            Err(e) => {
                return CommandDecoderResult::FatalError(e);
            }
        };

        CommandDecoderResult::Advance((crate::ClientCommand::Unsub(unsub), end))
    }
}

struct UnsubParts<'a> {
    sid: &'a [u8],
    max_msgs: Option<&'a [u8]>,
}

impl std::convert::TryFrom<UnsubParts<'_>> for crate::Unsub {
    type Error = ClientError;

    fn try_from(value: UnsubParts<'_>) -> Result<Self, Self::Error> {
        let sid = std::str::from_utf8(value.sid).map_err(|_| ClientError::BadUnsub)?;

        let decoded_msgs = value
            .max_msgs
            .map(std::str::from_utf8)
            .transpose()
            .map_err(|_| ClientError::BadUnsub)?;

        let max_msgs = decoded_msgs
            .map(|m| m.parse())
            .transpose()
            .map_err(|_| ClientError::BadUnsub)?;

        Ok(Self {
            sid: sid.into(),
            max_msgs,
        })
    }
}
