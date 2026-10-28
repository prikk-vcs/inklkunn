//! What the web surface is told at start, **validated before a socket is opened**, every refusal named
//! (RFC 003 D-5; threat model T-10). Two origins serve two kinds of thing: the **application origin** serves
//! pages, forms and the API; the **content origin** serves only wiki-authored bytes as inert attachments
//! (`-/raw/`, files). Scripts that might ever run in a hostile page are on the second, so they can never
//! read the first's cookies or storage.

use std::fmt;
use std::net::{IpAddr, SocketAddr};

/// `http` or `https`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// Plain HTTP: only for a loopback host in development.
    Http,
    /// HTTPS, terminated by the operator's proxy in front of this server.
    Https,
}

/// An origin as the operator writes it: `https://wiki.example` or `http://localhost:8080`. No path, query,
/// fragment or user information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    scheme: Scheme,
    host: String,
    port: Option<u16>,
}

/// Why an origin was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginError(pub String);

impl fmt::Display for OriginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for OriginError {}

/// A DNS name or IPv4 address (letters, digits, dots and hyphens, no empty or hyphen-edged label), or a
/// bracketed IPv6 literal.
fn valid_host(host: &str) -> bool {
    if let Some(inner) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        return inner.parse::<std::net::Ipv6Addr>().is_ok();
    }
    !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

impl Origin {
    /// Parse and check an origin.
    pub fn parse(text: &str) -> Result<Self, OriginError> {
        let bad = |why: &str| OriginError(format!("`{text}` is not an origin: {why}"));
        let (scheme, rest) = if let Some(r) = text.strip_prefix("https://") {
            (Scheme::Https, r)
        } else if let Some(r) = text.strip_prefix("http://") {
            (Scheme::Http, r)
        } else {
            return Err(bad(
                "it starts with https:// (or http:// for a loopback host)",
            ));
        };
        if rest.is_empty()
            || rest.contains(['/', '?', '#', '@', ' '])
            || rest.chars().any(char::is_control)
        {
            return Err(bad(
                "it is a scheme, a host and an optional port, with no path, query or user",
            ));
        }
        let (host, port) = match rest.rsplit_once(':') {
            Some((h, p)) if !p.contains(']') && !h.is_empty() => {
                let port: u16 = p
                    .parse()
                    .map_err(|_| bad("the port is not a number from 1 to 65535"))?;
                if port == 0 {
                    return Err(bad("the port is not a number from 1 to 65535"));
                }
                (h, Some(port))
            }
            _ => (rest, None),
        };
        if !valid_host(host) {
            return Err(bad("the host is not a plain DNS name or IP address"));
        }
        if scheme == Scheme::Http && !is_loopback_host(host) {
            return Err(bad(
                "http:// is for a loopback host in development; anything reachable uses https:// (behind your TLS proxy)",
            ));
        }
        Ok(Origin {
            scheme,
            host: host.to_ascii_lowercase(),
            port,
        })
    }

    /// The scheme.
    pub fn scheme(&self) -> Scheme {
        self.scheme
    }

    /// The host, lower case (an IPv6 literal keeps its brackets).
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port as written, if any.
    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// The port that matters: the one written, else the scheme's.
    pub fn effective_port(&self) -> u16 {
        self.port.unwrap_or(match self.scheme {
            Scheme::Http => 80,
            Scheme::Https => 443,
        })
    }

    /// Is `host_header` (a request's `Host`) this origin's authority? Case-insensitive; the port may be
    /// left out when it is the scheme's own.
    pub fn matches_host_header(&self, host_header: &str) -> bool {
        let h = host_header.trim().to_ascii_lowercase();
        let (host, port) = match h.rsplit_once(':') {
            Some((host, p)) if !p.contains(']') => match p.parse::<u16>() {
                Ok(port) => (host, Some(port)),
                Err(_) => return false,
            },
            _ => (h.as_str(), None),
        };
        host == self.host
            && port.unwrap_or(match self.scheme {
                Scheme::Http => 80,
                Scheme::Https => 443,
            }) == self.effective_port()
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let scheme = match self.scheme {
            Scheme::Http => "http",
            Scheme::Https => "https",
        };
        match self.port {
            Some(p) => write!(f, "{scheme}://{}:{p}", self.host),
            None => write!(f, "{scheme}://{}", self.host),
        }
    }
}

/// The validated settings of the web surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebConfig {
    /// Where the server listens.
    pub bind: SocketAddr,
    /// Where pages, forms and the API are served.
    pub app_origin: Origin,
    /// Where wiki-authored bytes are served as inert attachments.
    pub content_origin: Origin,
}

/// Why the settings were refused. The text names the setting and what is wanted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigRefusal(pub String);

impl fmt::Display for ConfigRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigRefusal {}

impl WebConfig {
    /// Check the three settings together.
    pub fn new(bind: &str, app_origin: &str, content_origin: &str) -> Result<Self, ConfigRefusal> {
        let bind: SocketAddr = bind.parse().map_err(|_| {
            ConfigRefusal(format!(
                "INKLKUNN_BIND `{bind}` is not an address and port such as 127.0.0.1:8080 or [::1]:8080"
            ))
        })?;
        let app_origin = Origin::parse(app_origin)
            .map_err(|e| ConfigRefusal(format!("INKLKUNN_APP_ORIGIN: {e}")))?;
        let content_origin = Origin::parse(content_origin)
            .map_err(|e| ConfigRefusal(format!("INKLKUNN_CONTENT_ORIGIN: {e}")))?;
        if app_origin == content_origin {
            return Err(ConfigRefusal(
                "INKLKUNN_CONTENT_ORIGIN is the same origin as INKLKUNN_APP_ORIGIN; wiki-authored bytes must be \
                 served from a different origin, or a hostile page could read the application's cookies"
                    .to_owned(),
            ));
        }
        // Cookies are shared by every port of a host, so on anything that is reachable a different port is
        // not a different origin in the sense that matters: the host must differ (planeter's IS-16).
        if !bind.ip().is_loopback() && app_origin.host() == content_origin.host() {
            return Err(ConfigRefusal(format!(
                "INKLKUNN_CONTENT_ORIGIN has the same host (`{}`) as INKLKUNN_APP_ORIGIN while the server is bound to \
                 {bind}, which is not a loopback address: cookies are shared across ports, so the content origin needs \
                 its own host name",
                app_origin.host()
            )));
        }
        Ok(WebConfig {
            bind,
            app_origin,
            content_origin,
        })
    }
}

#[cfg(test)]
mod tests;
