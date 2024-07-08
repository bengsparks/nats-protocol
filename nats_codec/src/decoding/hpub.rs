use super::{
    char_spliterator, header::parse_headers, slice_spliterator, ClientDecodeError,
    CommandDecoderResult,
};
use tokio_util::bytes::Bytes;

pub struct Decoder;

impl super::CommandDecoder<crate::ClientCommand, ClientDecodeError> for Decoder {
    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ClientCommand, ClientDecodeError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);

        // Load fixed sized blocks, namely metadata and NATS version field; buffer if too short
        let Some((metadata, metadata_len)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort(None);
        };

        // Split metadata by space and gather 4 to 5 fields
        let mut meta_iter = char_spliterator(metadata, b' ');
        let (subject, reply_to, header_bytes, total_bytes) = match (
            meta_iter.next(),
            meta_iter.next(),
            meta_iter.next(),
            meta_iter.next(),
        ) {
            (Some((subject, _)), Some((reply_to, _)), Some((header_bytes, last)), None) => {
                (subject, Some(reply_to), header_bytes, &metadata[last..])
            }
            (Some((subject, _)), Some((header_bytes, last)), None, None) => {
                (subject, None, header_bytes, &metadata[last..])
            }
            _ => {
                return CommandDecoderResult::FrameTooShort(None);
            }
        };

        let (Ok(headers), Ok(totals)) = (
            std::str::from_utf8(header_bytes),
            std::str::from_utf8(total_bytes),
        ) else {
            return CommandDecoderResult::FatalError(ClientDecodeError::BadHPub);
        };
        let (Ok(header_bytes), Ok(total_bytes)) =
            (headers.parse::<usize>(), totals.parse::<usize>())
        else {
            return CommandDecoderResult::FatalError(ClientDecodeError::BadHPub);
        };

        if total_bytes < header_bytes {
            return CommandDecoderResult::FatalError(ClientDecodeError::BadHPub);
        }
        if total_bytes > buffer.len() {
            return CommandDecoderResult::FrameTooShort(None);
        }

        dbg!(buffer[metadata_len..].len());
        dbg!(total_bytes);

        let headers = &buffer[metadata_len..metadata_len + header_bytes];
        dbg!(std::str::from_utf8(headers).unwrap());

        let payload = &buffer[metadata_len + header_bytes..metadata_len + total_bytes];
        dbg!(std::str::from_utf8(payload).unwrap());

        let parts = HPubParts {
            subject,
            reply_to,
            header_bytes,
            total_bytes,
            headers,
            payload,
        };

        let Ok(hpub) = parts.try_into() else {
            return CommandDecoderResult::FatalError(ClientDecodeError::BadHPub);
        };

        CommandDecoderResult::Advance((
            crate::ClientCommand::HPublish(hpub),
            metadata_len + total_bytes + 2,
        ))
    }
}

struct HPubParts<'a> {
    subject: &'a [u8],
    reply_to: Option<&'a [u8]>,
    header_bytes: usize,
    total_bytes: usize,
    headers: &'a [u8],
    payload: &'a [u8],
}

impl std::convert::TryFrom<HPubParts<'_>> for crate::HPublish {
    type Error = ClientDecodeError;

    fn try_from(value: HPubParts<'_>) -> Result<Self, Self::Error> {
        let Ok(subject) = std::str::from_utf8(value.subject) else {
            return Err(Self::Error::BadHPub);
        };

        let Ok(reply_to) = value.reply_to.map(std::str::from_utf8).transpose() else {
            return Err(Self::Error::BadHPub);
        };

        if value.total_bytes < value.header_bytes
            || value.payload.len() != value.total_bytes - value.header_bytes
        {
            return Err(Self::Error::BadHPub);
        }

        let Ok(headers) = parse_headers(value.headers, value.header_bytes) else {
            return Err(Self::Error::BadHPub);
        };

        Ok(crate::HPublish {
            subject: subject.into(),
            reply_to: reply_to.map(Into::into),
            header_bytes: value.header_bytes,
            total_bytes: value.total_bytes,
            headers,
            payload: Bytes::copy_from_slice(value.payload),
        })
    }
}
