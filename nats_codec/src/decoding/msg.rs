use super::{char_spliterator, slice_spliterator, CommandDecoderResult, ServerError};

pub struct MsgDecoder;

impl super::CommandDecoder<crate::ServerCommand, ServerError> for MsgDecoder {
    const PREFIX: &'static [u8] = b"MSG ";

    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ServerCommand, ServerError> {
        let mut blockiter = slice_spliterator(buffer, &crate::CRLF);

        // MSG terminates the payload with a second CR-LF sequence to separate it from the prefix metadata.
        // Do not consume iterator entirely as that will buffer the entire stream into memory.
        let (Some((metadata_block, _)), Some((payload_block, msg_ending))) =
            (blockiter.next(), blockiter.next())
        else {
            return CommandDecoderResult::FrameTooShort;
        };

        let mut spliterator = char_spliterator(metadata_block, b' ');

        // First two spaces are mandatory, third is optional, fourth is forbidden
        // Consume iterator entirely, as entire frame should now be in memory
        let parts = match (
            spliterator.next(),
            spliterator.next(),
            spliterator.next(),
            spliterator.next(),
        ) {
            (Some((subject, _)), Some((sid, _)), Some((reply_to, last)), None) => MsgParts {
                subject,
                sid,
                reply_to: Some(reply_to),
                bytes: &metadata_block[last..],
                payload: payload_block,
            },
            (Some((subject, _)), Some((sid, last)), None, None) => MsgParts {
                subject,
                sid,
                reply_to: None,
                bytes: &metadata_block[last..],
                payload: payload_block,
            },
            _ => {
                return CommandDecoderResult::FatalError(self::ServerError::BadMsg);
            }
        };

        let msg = match parts.try_into() {
            Ok(msg) => msg,
            Err(e) => return CommandDecoderResult::FatalError(e),
        };

        CommandDecoderResult::Advance((crate::ServerCommand::Msg(msg), msg_ending))
    }
}

struct MsgParts<'a> {
    subject: &'a [u8],
    sid: &'a [u8],
    reply_to: Option<&'a [u8]>,
    bytes: &'a [u8],
    payload: &'a [u8],
}

impl std::convert::TryFrom<MsgParts<'_>> for crate::Msg {
    type Error = self::ServerError;

    fn try_from(value: MsgParts<'_>) -> Result<Self, Self::Error> {
        let Ok(subject) = std::str::from_utf8(value.subject) else {
            return Err(Self::Error::BadMsg);
        };

        let Ok(sid) = std::str::from_utf8(value.sid) else {
            return Err(Self::Error::BadMsg);
        };

        let Ok(utf8_bytes) = std::str::from_utf8(value.bytes) else {
            return Err(Self::Error::BadMsg);
        };

        let Ok(bytes) = utf8_bytes.parse::<u64>() else {
            return Err(Self::Error::BadMsg);
        };
        let bytes = bytes as usize;

        let Ok(reply_to) = value.reply_to.map(std::str::from_utf8).transpose() else {
            return Err(Self::Error::BadMsg);
        };

        // Clamp to shorter value
        let shorter = bytes.min(value.payload.len());
        let Ok(payload) = (shorter != 0)
            .then_some(value.payload)
            .map(|b| &b[..shorter])
            .map(std::str::from_utf8)
            .transpose()
        else {
            return Err(Self::Error::BadMsg);
        };

        Ok(crate::Msg {
            subject: subject.into(),
            sid: sid.into(),
            reply_to: reply_to.map(Into::into),
            bytes,
            payload: payload.map(Into::into),
        })
    }
}
