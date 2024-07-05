use std::collections::HashMap;

use memchr::memchr_iter;

use crate::{HeaderName, HeaderValue};

use super::slice_spliterator;

pub enum HeaderDecodeError {
    BadLength,
    MissingNatsVersion,
    BadHeaderName,
    BadHeaderValue,
    NoColon,
}

pub fn parse_headers(
    header_buffer: &[u8],
    headers_length: usize,
) -> Result<crate::HeaderMap, HeaderDecodeError> {
    if headers_length < 2 {
        return Err(HeaderDecodeError::BadLength);
    }

    let mut spliterator = slice_spliterator(header_buffer, &crate::CRLF);
    let Some((b"NATS/1.0", _)) = spliterator.next() else {
        return Err(HeaderDecodeError::MissingNatsVersion);
    };

    let mut headers = HashMap::new();
    loop {
        let Some((slice, offset)) = spliterator.next() else {
            return Err(HeaderDecodeError::BadLength);
        };

        if slice.is_empty() && offset == headers_length {
            break;
        } else if !slice.is_empty() && offset != headers_length {
            let (name, value) = parse_header(slice)?;
            let vs: &mut Vec<_> = headers.entry(name).or_default();
            vs.push(value);
        } else {
            return Err(HeaderDecodeError::BadLength);
        }
    }

    Ok(crate::HeaderMap(headers))
}

fn parse_header(slice: &[u8]) -> Result<(HeaderName, HeaderValue), HeaderDecodeError> {
    let mut colon_iter = memchr_iter(b':', slice);
    let (Some(colon_index), None) = (colon_iter.next(), colon_iter.next()) else {
        return Err(HeaderDecodeError::NoColon);
    };

    let name = &slice[..colon_index];

    let value = &slice[colon_index + 1..];
    let value = value.strip_prefix(&b" "[..]).unwrap_or(value);
    let value = value.strip_suffix(&b" "[..]).unwrap_or(value);

    let Ok(name) = std::str::from_utf8(name) else {
        return Err(HeaderDecodeError::BadHeaderName);
    };

    let Ok(value) = std::str::from_utf8(value) else {
        return Err(HeaderDecodeError::BadHeaderValue);
    };

    Ok((HeaderName(name.into()), HeaderValue(value.into())))
}

// Dummy impl; definitely not safe!
/*
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
*/
