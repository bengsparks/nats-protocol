use super::{char_spliterator, slice_spliterator, ClientError, CommandDecoderResult};

pub struct PubDecoder;

impl super::CommandDecoder<crate::ClientCommand, ClientError> for PubDecoder {
    const PREFIX: &'static [u8] = b"PUB ";

    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ClientCommand, ClientError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);
        let Some((metadata, _)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort;
        };

        let mut meta_splits = char_spliterator(metadata, b' ');
        let (subject, reply_to, bytes) =
            match (meta_splits.next(), meta_splits.next(), meta_splits.next()) {
                (Some((subject, _)), Some((reply_to, last)), None) => {
                    (subject, Some(reply_to), &metadata[last..])
                }
                (Some((subject, last)), None, None) => (subject, None, &metadata[last..]),
                _ => {
                    return CommandDecoderResult::FatalError(ClientError::BadPub);
                }
            };

        let Ok(decoded_bytes) = std::str::from_utf8(bytes) else {
            return CommandDecoderResult::FatalError(ClientError::BadPub);
        };
        let Ok(bytes) = decoded_bytes.parse::<usize>() else {
            return CommandDecoderResult::FatalError(ClientError::BadPub);
        };

        let Some((payload, end)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort;
        };

        let parts = PubParts {
            subject,
            reply_to,
            bytes,
            payload,
        };
        let pb = match parts.try_into() {
            Ok(pb) => pb,
            Err(e) => {
                return CommandDecoderResult::FatalError(e);
            }
        };

        CommandDecoderResult::Advance((crate::ClientCommand::Pub(pb), end))
    }
}

struct PubParts<'a> {
    pub subject: &'a [u8],
    pub reply_to: Option<&'a [u8]>,
    pub bytes: usize,
    pub payload: &'a [u8],
}

impl std::convert::TryFrom<PubParts<'_>> for crate::Pub {
    type Error = ClientError;

    fn try_from(value: PubParts<'_>) -> Result<Self, Self::Error> {
        let Ok(subject) = std::str::from_utf8(value.subject) else {
            return Err(ClientError::BadPub);
        };

        let Ok(reply_to) = value.reply_to.map(std::str::from_utf8).transpose() else {
            return Err(ClientError::BadPub);
        };

        let (bytes, payload) = match (value.bytes, value.payload) {
            (0, b"") => (value.bytes, None),
            (length, payload) if payload.len() == length => {
                let Ok(p) = std::str::from_utf8(payload) else {
                    return Err(ClientError::BadPub);
                };
                (value.bytes, Some(p))
            }
            _ => {
                return Err(ClientError::BadPub);
            }
        };

        Ok(Self {
            subject: subject.into(),
            reply_to: reply_to.map(Into::into),
            bytes,
            payload: payload.map(Into::into),
        })
    }
}