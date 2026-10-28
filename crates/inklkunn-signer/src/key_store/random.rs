//! Randomness for keys, salts, nonces and directory names: the operating system's, **or an error**.
//!
//! The store's id helper falls back to the standard library's hasher state when `/dev/urandom` cannot be
//! read, which is fine for an opaque id and wrong here: a nonce or a salt from a weak source is a silent
//! weakening of the cipher. This module has no fallback. The server is Linux-only (OQ-3), where
//! `/dev/urandom` is always seeded once the system has booted.

use std::io::Read;

/// Fill `buf` from the operating system's generator.
pub(super) fn fill(buf: &mut [u8]) -> std::io::Result<()> {
    std::fs::File::open("/dev/urandom")?.read_exact(buf)
}

/// `N` random bytes.
pub(super) fn bytes<const N: usize>() -> std::io::Result<[u8; N]> {
    let mut out = [0u8; N];
    fill(&mut out)?;
    Ok(out)
}

/// Lower-case hex of `data`.
pub(super) fn hex(data: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(data.len() * 2);
    for b in data {
        out.push(char::from(
            DIGITS.get(usize::from(b >> 4)).copied().unwrap_or(b'0'),
        ));
        out.push(char::from(
            DIGITS.get(usize::from(b & 0x0f)).copied().unwrap_or(b'0'),
        ));
    }
    out
}

/// Bytes from lower-case hex; `None` for anything else (upper case, odd length, a stray character).
pub(super) fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let digit = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    s.as_bytes()
        .chunks(2)
        .map(|pair| match pair {
            [hi, lo] => Some(digit(*hi)? << 4 | digit(*lo)?),
            _ => None,
        })
        .collect()
}
