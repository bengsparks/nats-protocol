use tokio_util::bytes::Bytes;

use super::{
    char_spliterator, header::parse_headers, slice_spliterator, CommandDecoderResult,
    ServerDecodeError,
};

pub struct Decoder;

impl super::CommandDecoder<crate::ServerCommand, ServerDecodeError> for Decoder {
    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ServerCommand, ServerDecodeError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);

        // Load fixed sized blocks, namely metadata and NATS version field; buffer if too short
        let Some((metadata, metadata_len)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort(None);
        };

        // Split metadata by space and gather 4 to 5 fields
        let mut meta_iter = char_spliterator(metadata, b' ');
        let (subject, sid, reply_to, header_bytes, total_bytes) = match (
            meta_iter.next(),
            meta_iter.next(),
            meta_iter.next(),
            meta_iter.next(),
            meta_iter.next(),
        ) {
            (
                Some((subject, _)),
                Some((sid, _)),
                Some((reply_to, _)),
                Some((header_bytes, last)),
                None,
            ) => (
                subject,
                sid,
                Some(reply_to),
                header_bytes,
                &metadata[last..],
            ),
            (Some((subject, _)), Some((sid, _)), Some((header_bytes, last)), None, None) => {
                (subject, sid, None, header_bytes, &metadata[last..])
            }
            _ => {
                return CommandDecoderResult::FrameTooShort(None);
            }
        };

        let (Ok(headers), Ok(totals)) = (
            std::str::from_utf8(header_bytes),
            std::str::from_utf8(total_bytes),
        ) else {
            return CommandDecoderResult::FatalError(ServerDecodeError::BadHMsg);
        };
        let (Ok(header_bytes), Ok(total_bytes)) =
            (headers.parse::<usize>(), totals.parse::<usize>())
        else {
            return CommandDecoderResult::FatalError(ServerDecodeError::BadHMsg);
        };

        if total_bytes < header_bytes {
            return CommandDecoderResult::FatalError(ServerDecodeError::BadHMsg);
        }
        if total_bytes > buffer.len() {
            return CommandDecoderResult::FrameTooShort(None);
        }

        let headers = &buffer[metadata_len..metadata_len + header_bytes];
        let payload = &buffer[metadata_len + header_bytes..metadata_len + total_bytes];

        let parts = HMsgParts {
            subject,
            sid,
            reply_to,
            header_bytes,
            total_bytes,
            headers,
            payload,
        };
        let hmsg = match parts.try_into() {
            Ok(hmsg) => hmsg,
            Err(e) => return CommandDecoderResult::FatalError(e),
        };

        CommandDecoderResult::Advance((
            crate::ServerCommand::HMsg(hmsg),
            metadata_len + total_bytes + 2,
        ))
    }
}

struct HMsgParts<'a> {
    subject: &'a [u8],
    sid: &'a [u8],
    reply_to: Option<&'a [u8]>,
    header_bytes: usize,
    total_bytes: usize,
    headers: &'a [u8],
    payload: &'a [u8],
}

impl std::convert::TryFrom<HMsgParts<'_>> for crate::HMsg {
    type Error = ServerDecodeError;

    fn try_from(value: HMsgParts<'_>) -> Result<Self, Self::Error> {
        let Ok(subject) = std::str::from_utf8(value.subject) else {
            return Err(Self::Error::BadHMsg);
        };

        let Ok(sid) = std::str::from_utf8(value.sid) else {
            return Err(Self::Error::BadHMsg);
        };

        let Ok(reply_to) = value.reply_to.map(std::str::from_utf8).transpose() else {
            return Err(Self::Error::BadHMsg);
        };

        if value.total_bytes < value.header_bytes
            || value.payload.len() != value.total_bytes - value.header_bytes
        {
            return Err(Self::Error::BadHMsg);
        }

        let Ok(headers) = parse_headers(value.headers, value.header_bytes) else {
            return Err(Self::Error::BadHMsg);
        };

        Ok(crate::HMsg {
            subject: subject.into(),
            sid: sid.into(),
            reply_to: reply_to.map(Into::into),
            header_bytes: value.header_bytes,
            total_bytes: value.total_bytes,
            headers,
            payload: Bytes::copy_from_slice(value.payload),
        })
    }
}
