//! A form's body, parsed **after** the checks (corrections P11). The handlers of the write forms take the body as
//! the bytes it arrived as, bounded by the route's limit, and nothing reads it as fields until the wiki is looked
//! up, the visitor is known to hold an account and the form came from this site: a request that fails any of
//! those is answered without its body ever being parsed. (axum's `Form` extractor parses at extraction, before the
//! handler's first line, which is the order this module exists to undo.)
//!
//! The format is `application/x-www-form-urlencoded`: `name=value` pairs joined by `&`, `+` for a space, `%XX`
//! for a byte. A name given twice keeps its last value, as the extractor it replaces did. The test
//! (`urlencoded/tests.rs`) holds this parser to that extractor's answers.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::http::{HeaderMap, StatusCode, header};

/// A form's body as it was sent: the bytes, or the reason they could not be taken (over the route's bound).
pub(crate) type Raw = Result<Bytes, BytesRejection>;

/// The fields of a form body. `Err` is the status to answer with: `413` for a body over the bound, `415` when the
/// request is not sent as a form, `400` when the body is not text.
pub(crate) fn fields(headers: &HeaderMap, raw: Raw) -> Result<HashMap<String, String>, StatusCode> {
    let body = raw.map_err(|rejection| rejection.status())?;
    if !is_form(headers) {
        return Err(StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    parse(&body).ok_or(StatusCode::BAD_REQUEST)
}

/// Is the request sent as `application/x-www-form-urlencoded` (parameters such as a charset are ignored)?
fn is_form(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_some_and(|essence| {
            essence
                .trim()
                .eq_ignore_ascii_case("application/x-www-form-urlencoded")
        })
}

/// The pairs of an urlencoded body; `None` when a name or a value is not UTF-8 once decoded.
pub(crate) fn parse(body: &[u8]) -> Option<HashMap<String, String>> {
    let mut fields = HashMap::new();
    for pair in body.split(|b| *b == b'&').filter(|p| !p.is_empty()) {
        let (name, value) = match pair.iter().position(|b| *b == b'=') {
            Some(at) => (pair.get(..at)?, pair.get(at + 1..)?),
            None => (pair, &[][..]),
        };
        fields.insert(decode(name)?, decode(value)?);
    }
    Some(fields)
}

/// One name or value: `+` is a space, `%XX` a byte, and a `%` not followed by two hex digits is itself.
fn decode(raw: &[u8]) -> Option<String> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while let Some(&b) = raw.get(i) {
        match b {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' => {
                let hex = |at: usize| {
                    raw.get(at)
                        .and_then(|h| (*h as char).to_digit(16))
                        .and_then(|d| u8::try_from(d).ok())
                };
                if let (Some(hi), Some(lo)) = (hex(i + 1), hex(i + 2)) {
                    out.push(hi * 16 + lo);
                    i += 3;
                } else {
                    out.push(b'%');
                    i += 1;
                }
            }
            other => {
                out.push(other);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests;
