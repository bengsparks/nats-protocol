use std::collections::HashMap;

/*
use winnow::{combinator::seq, token::{literal, take_until}, Parser};

use super::{char_spliterator, slice_spliterator, CommandDecoderResult, ServerError};

pub struct HeadersDecoder { pub headers_length: usize }

impl super::CommandDecoder<crate::HeaderMap> for HeadersDecoder {
    const PREFIX: &'static [u8] = b"NATS/1.0\r\n";

    fn decode_body(&self, buffer: &[u8]) -> CommandDecoderResult<crate::HeaderMap> {
        if self.headers_length > buffer.len() {
            return CommandDecoderResult::FrameTooShort;
        }
        if self.headers_length < buffer.len() || !buffer.ends_with(&crate::CRLF) {
            return CommandDecoderResult::FatalError(ServerError::BadHeaders)
        }
        // Guaranteed to consume entirety of `buffer` due to previous checks
        let mut spliterator = slice_spliterator(buffer, &crate::CRLF);

        let Some((b"NATS/1.0", _)) = spliterator.next() else {
            return CommandDecoderResult::FatalError(ServerError::BadHeaders)
        };

        let mut map: HashMap<_, Vec<_>> = HashMap::new();
        for (mut slice, _) in spliterator {
            let Ok(crate::Header { name, value }) = parse_header(&mut slice) else {
                return CommandDecoderResult::FatalError(ServerError::BadHeaders)
            };
            map.entry(name).or_default().push(value);
        }

        let headers = crate::HeaderMap(map);
        CommandDecoderResult::Advance((headers, buffer.len()))
    }
}
 */

// Dummy impl; definitely not safe!
fn parse_header<'a>(slice: &mut &'a [u8]) -> Result<crate::Header, ()> {
    Err(())
}

impl std::convert::From<&'_ [u8]> for crate::HeaderMap {
    fn from(value: &'_ [u8]) -> Self {
        Self(HashMap::new())
    }
}

impl std::str::FromStr for crate::HeaderMap {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(HashMap::new()))
    }
}
