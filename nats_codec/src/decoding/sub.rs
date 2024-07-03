use super::{char_spliterator, slice_spliterator, ClientDecodeError, CommandDecoderResult};

pub struct SubDecoder;

impl super::CommandDecoder<crate::ClientCommand, ClientDecodeError> for SubDecoder {
    fn decode_body(
        &self,
        buffer: &[u8],
    ) -> CommandDecoderResult<crate::ClientCommand, ClientDecodeError> {
        let mut crlf_iter = slice_spliterator(buffer, &crate::CRLF);
        let Some((message, end)) = crlf_iter.next() else {
            return CommandDecoderResult::FrameTooShort(None);
        };

        let mut spliterator = char_spliterator(message, b' ');
        let (subject, queue, sid) =
            match (spliterator.next(), spliterator.next(), spliterator.next()) {
                (Some((subject, _)), Some((queue, last)), None) => {
                    (subject, Some(queue), &message[last..])
                }
                (Some((subject, last)), None, None) => (subject, None, &message[last..]),
                _ => {
                    return CommandDecoderResult::FatalError(ClientDecodeError::BadSub);
                }
            };

        let parts = SubParts {
            subject,
            queue_group: queue,
            sid,
        };

        let sub = match parts.try_into() {
            Ok(sub) => sub,
            Err(e) => return CommandDecoderResult::FatalError(e),
        };

        CommandDecoderResult::Advance((crate::ClientCommand::Sub(sub), end))
    }
}

struct SubParts<'a> {
    subject: &'a [u8],
    queue_group: Option<&'a [u8]>,
    sid: &'a [u8],
}

impl std::convert::TryFrom<SubParts<'_>> for crate::Sub {
    type Error = ClientDecodeError;

    fn try_from(value: SubParts<'_>) -> Result<Self, Self::Error> {
        let subject = std::str::from_utf8(value.subject).map_err(|_| Self::Error::BadSub)?;
        let queue_group = value
            .queue_group
            .map(std::str::from_utf8)
            .transpose()
            .map_err(|_| Self::Error::BadSub)?;
        let sid = std::str::from_utf8(value.sid).map_err(|_| Self::Error::BadSub)?;

        Ok(Self {
            subject: subject.into(),
            queue_group: queue_group.map(Into::into),
            sid: sid.into(),
        })
    }
}
