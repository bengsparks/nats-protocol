use super::{char_spliterator, slice_spliterator, CommandDecoderResult, ServerError};

pub struct HMsgDecoder;

struct Metadata<'a> {
    subject: &'a [u8],
    sid: &'a [u8],
    reply_to: Option<&'a [u8]>,
    header_bytes: usize,
    total_bytes: usize,
}

impl super::CommandDecoder<crate::ServerCommand, ServerError> for HMsgDecoder {
    const PREFIX: &'static [u8] = b"HMSG ";

    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ServerCommand, ServerError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);

        // Load fixed sized blocks, namely metadata and NATS version field; buffer if too short
        let Some((metadata, _)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort;
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
                return CommandDecoderResult::FrameTooShort;
            }
        };

        let (Ok(headers), Ok(totals)) = (
            std::str::from_utf8(header_bytes),
            std::str::from_utf8(total_bytes),
        ) else {
            return CommandDecoderResult::FatalError(ServerError::BadHMsg);
        };
        let (Ok(header_bytes), Ok(total_bytes)) =
            (headers.parse::<usize>(), totals.parse::<usize>())
        else {
            return CommandDecoderResult::FatalError(ServerError::BadHMsg);
        };

        if total_bytes <= header_bytes {
            return CommandDecoderResult::FatalError(ServerError::BadHMsg);
        }
        if total_bytes > buffer.len() {
            return CommandDecoderResult::FrameTooShort;
        }

        // Metadata parsing complete!
        let metadata = Metadata {
            subject,
            sid,
            reply_to,
            header_bytes,
            total_bytes,
        };

        // TODO: Add length checks
        /*let headers = if header_bytes > 0 {
            let parsing = crlf_iter
                .take_while(|(_, length)| *length <= header_bytes)
                .map(|(slice, ending)| {
                    if ending != header_bytes {
                        return Err(());
                    }
                    // TODO parse headers here
                    return Ok((slice.into(), ending));
                });

            let Ok(headers) = parsing.collect::<Result<Vec<_>, _>>() else {
                return CommandDecoderResult::FatalError(Error::BadHMsg);
            };
            Some(headers)
        } else {
            None
        };*/
        // crlf_iter.next();

        let payload = if total_bytes > 0 {
            let Some((p, len)) = crlf_iter.next() else {
                return CommandDecoderResult::FatalError(ServerError::BadHMsg);
            };
            if len != total_bytes {
                return CommandDecoderResult::FatalError(ServerError::BadHMsg);
            }
            Some(p)
        } else {
            None
        };

        let parts = HMsgParts {
            subject: metadata.subject,
            sid: metadata.sid,
            reply_to: metadata.reply_to,
            header_bytes: metadata.header_bytes,
            total_bytes: metadata.total_bytes,
            headers: None,
            payload,
        };
        let hmsg = match parts.try_into() {
            Ok(hmsg) => hmsg,
            Err(e) => return CommandDecoderResult::FatalError(e),
        };

        CommandDecoderResult::Advance((crate::ServerCommand::HMsg(hmsg), 0))
    }
}

struct HMsgParts<'a> {
    subject: &'a [u8],
    sid: &'a [u8],
    reply_to: Option<&'a [u8]>,
    header_bytes: usize,
    total_bytes: usize,
    headers: Option<&'a [u8]>,
    payload: Option<&'a [u8]>,
}

impl std::convert::TryFrom<HMsgParts<'_>> for crate::HMsg {
    type Error = ServerError;

    fn try_from(value: HMsgParts<'_>) -> Result<Self, Self::Error> {
        let Ok(subject) = std::str::from_utf8(value.subject) else {
            return Err(Self::Error::BadMsg);
        };

        let Ok(sid) = std::str::from_utf8(value.sid) else {
            return Err(Self::Error::BadMsg);
        };

        let Ok(reply_to) = value.reply_to.map(std::str::from_utf8).transpose() else {
            return Err(Self::Error::BadMsg);
        };

        let Ok(payload) = value.payload.map(std::str::from_utf8).transpose() else {
            return Err(Self::Error::BadMsg);
        };

        Ok(crate::HMsg {
            subject: subject.into(),
            sid: sid.into(),
            reply_to: reply_to.map(Into::into),
            header_bytes: value.header_bytes,
            total_bytes: value.total_bytes,
            headers: value.headers.map(Into::into),
            payload: payload.map(Into::into),
        })
    }
}