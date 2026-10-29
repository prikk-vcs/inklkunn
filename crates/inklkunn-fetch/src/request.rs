//! What the caller hands this crate: the egress guard's answer ([`FetchTarget`]) and the request's secrets
//! ([`CurlRequest`]). Both are checked here before anything runs, because this crate does not trust its caller
//! to have agreed with itself: the URL must be `https`, must carry no userinfo or control byte, and must
//! name the very host and port the target pins.

use std::fmt;
use std::net::IpAddr;

use crate::error::CurlError;

/// The longest URL accepted.
const MAX_URL: usize = 4096;
/// The longest header value or form body accepted (they travel on `curl`'s standard input, which must fit a
/// pipe's buffer).
pub(crate) const MAX_SECRET_FIELD: usize = 16 << 10;

/// An admitted destination with its pinned addresses: what the egress guard returned, in this crate's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchTarget {
    /// The URL as the caller gave it; it must name `host` and `port`.
    pub url: String,
    /// The host as written (an internationalized name as its A-labels; an IPv6 literal without brackets).
    pub host: String,
    /// The port (443 when the URL has none).
    pub port: u16,
    /// The addresses to connect to. The guard has checked every one is public unicast; this crate connects to
    /// these and to no other, and never resolves the name.
    pub addrs: Vec<IpAddr>,
}

/// One request: a target, and optionally a form body (then it is a `POST`) and an `Authorization` header
/// value. **Its `Debug` shows the URL and nothing else**: the form carries the authorization code and the
/// PKCE verifier, and the header the client's secret.
#[derive(Clone, PartialEq, Eq)]
pub struct CurlRequest {
    /// Where to go.
    pub target: FetchTarget,
    /// An `application/x-www-form-urlencoded` body; the request is a `POST` when present.
    pub form: Option<String>,
    /// The `Authorization` header's value, when the request needs one.
    pub authorization: Option<String>,
}

impl fmt::Debug for CurlRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CurlRequest")
            .field("url", &self.target.url)
            .field("form", &self.form.as_ref().map(|_| "…"))
            .field("authorization", &self.authorization.as_ref().map(|_| "…"))
            .finish()
    }
}

/// What a checked target comes to: the URL to pass, and the `--resolve` entry when the host is a name.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Checked {
    pub(crate) url: String,
    pub(crate) resolve: Option<String>,
}

fn invalid(why: impl Into<String>) -> CurlError {
    CurlError::InvalidRequest(why.into())
}

/// `https://host[:port]/…` → (host without brackets, port).
fn authority_of(url: &str) -> Result<(String, Option<u16>), CurlError> {
    if url.len() > MAX_URL {
        return Err(invalid("the URL is too long"));
    }
    if url.bytes().any(|b| b <= 0x20 || b == 0x7f) {
        return Err(invalid("the URL carries a control byte or whitespace"));
    }
    let Some(prefix) = url.get(..8) else {
        return Err(invalid("the URL is not https"));
    };
    if !prefix.eq_ignore_ascii_case("https://") {
        return Err(invalid("the URL is not https"));
    }
    let rest = url.get(8..).unwrap_or("");
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = rest.get(..end).unwrap_or("");
    if authority.contains('@') {
        return Err(invalid("the URL carries userinfo"));
    }
    let (host, port_text) = if let Some(after) = authority.strip_prefix('[') {
        let close = after
            .find(']')
            .ok_or_else(|| invalid("the URL has an unterminated IPv6 literal"))?;
        (
            after.get(..close).unwrap_or("").to_owned(),
            after.get(close + 1..).unwrap_or(""),
        )
    } else if let Some((h, _)) = authority.split_once(':') {
        (h.to_owned(), authority.get(h.len()..).unwrap_or(""))
    } else {
        (authority.to_owned(), "")
    };
    let port = match port_text.strip_prefix(':') {
        Some(p) => match p.parse::<u16>() {
            Ok(n) if n != 0 => Some(n),
            _ => return Err(invalid("the URL has a malformed port")),
        },
        None if port_text.is_empty() => None,
        None => return Err(invalid("the URL has a malformed authority")),
    };
    if host.is_empty() {
        return Err(invalid("the URL has no host"));
    }
    Ok((host, port))
}

/// Is `host` a name that can sit inside a `--resolve host:port:addr` entry without changing its meaning?
fn plain_name(host: &str) -> bool {
    !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.' || b == b'_')
}

impl FetchTarget {
    /// Check that the URL, the host, the port and the addresses agree, and build what `curl` needs of them.
    pub(crate) fn check(&self) -> Result<Checked, CurlError> {
        let (url_host, url_port) = authority_of(&self.url)?;
        if !url_host.eq_ignore_ascii_case(&self.host) {
            return Err(invalid("the URL's host is not the pinned target's host"));
        }
        if url_port.unwrap_or(443) != self.port {
            return Err(invalid("the URL's port is not the pinned target's port"));
        }
        if self.addrs.is_empty() {
            return Err(invalid("the pinned target has no address"));
        }
        // A literal address in the URL needs no resolution, and must be one of the pinned addresses.
        if let Ok(ip) = self.host.parse::<IpAddr>() {
            if !self.addrs.contains(&ip) {
                return Err(invalid("the URL's address is not a pinned address"));
            }
            return Ok(Checked {
                url: self.url.clone(),
                resolve: None,
            });
        }
        if !plain_name(&self.host) {
            return Err(invalid("the host is not a plain ASCII name"));
        }
        let addrs = self
            .addrs
            .iter()
            .map(|a| match a {
                IpAddr::V4(v4) => v4.to_string(),
                IpAddr::V6(v6) => format!("[{v6}]"),
            })
            .collect::<Vec<_>>()
            .join(",");
        Ok(Checked {
            url: self.url.clone(),
            resolve: Some(format!("{}:{}:{addrs}", self.host, self.port)),
        })
    }
}

#[cfg(test)]
mod tests;
