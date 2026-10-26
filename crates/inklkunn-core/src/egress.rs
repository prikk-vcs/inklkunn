//! The one egress guard (threat model C-8, INV-3; internal design LAY-5): every outbound fetch — OpenID
//! Connect, interwiki checks, link previews, webhooks — passes its URL here first and connects only to the
//! **pinned** addresses it returns, never re-resolving the name (a DNS rebinding would otherwise move the
//! target after the check). Public unicast only; HTTPS only; redirects are re-checked by the caller.
//!
//! The classification is re-derived from the registries (RFC 1122, 1918, 3927, 4193, 4291, 5737, 6598,
//! 6890) rather than copied, and tested against planeter's list as a cross-check.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};

/// Why a URL was refused or could not be evaluated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EgressRefusal {
    /// Not `https`.
    Scheme(String),
    /// No host could be read out of the URL.
    Unparsable(String),
    /// The target is not a public unicast address.
    BlockedAddress(String),
    /// The host resolved to nothing.
    Unresolvable(String),
}

impl std::fmt::Display for EgressRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EgressRefusal::Scheme(s) => {
                write!(f, "outbound scheme `{s}` is not allowed; only https is")
            }
            EgressRefusal::Unparsable(u) => write!(f, "outbound URL has no readable host: {u}"),
            EgressRefusal::BlockedAddress(a) => {
                write!(f, "outbound address {a} is not public unicast")
            }
            EgressRefusal::Unresolvable(h) => write!(f, "outbound host {h} did not resolve"),
        }
    }
}

impl std::error::Error for EgressRefusal {}

/// An admitted destination with its pinned addresses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedTarget {
    /// The URL as given.
    pub url: String,
    /// The host as written.
    pub host: String,
    /// The port (443 when absent).
    pub port: u16,
    /// The addresses to connect to; every one is public unicast.
    pub addrs: Vec<SocketAddr>,
}

/// The admission point for outbound fetches.
pub trait EgressGuard: Send + Sync {
    /// Admit `url` and pin its addresses, or refuse.
    fn check(&self, url: &str) -> Result<PinnedTarget, EgressRefusal>;
}

/// The production guard: HTTPS only, public unicast only, OS resolver, addresses pinned.
#[derive(Debug, Default, Clone, Copy)]
pub struct StdEgressGuard;

/// `scheme://host[:port]/…` → (scheme, host, port). Hosts may be `[v6]`.
pub fn split_url(url: &str) -> Result<(String, String, Option<u16>), EgressRefusal> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| EgressRefusal::Unparsable(url.to_owned()))?;
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = rest.get(..authority_end).unwrap_or("");
    // Userinfo is refused outright: `https://trusted@evil/` is a classic confusion.
    if authority.contains('@') || authority.is_empty() {
        return Err(EgressRefusal::Unparsable(url.to_owned()));
    }
    let (host, port) = if let Some(after) = authority.strip_prefix('[') {
        let close = after
            .find(']')
            .ok_or_else(|| EgressRefusal::Unparsable(url.to_owned()))?;
        let host = after.get(..close).unwrap_or("").to_owned();
        let port_text = after.get(close + 1..).unwrap_or("");
        let port = match port_text.strip_prefix(':') {
            Some(p) => Some(
                p.parse::<u16>()
                    .map_err(|_| EgressRefusal::Unparsable(url.to_owned()))?,
            ),
            None if port_text.is_empty() => None,
            None => return Err(EgressRefusal::Unparsable(url.to_owned())),
        };
        (host, port)
    } else {
        match authority.rsplit_once(':') {
            Some((h, p)) => (
                h.to_owned(),
                Some(
                    p.parse::<u16>()
                        .map_err(|_| EgressRefusal::Unparsable(url.to_owned()))?,
                ),
            ),
            None => (authority.to_owned(), None),
        }
    };
    if host.is_empty() {
        return Err(EgressRefusal::Unparsable(url.to_owned()));
    }
    Ok((scheme.to_ascii_lowercase(), host, port))
}

/// Is `ip` a public unicast address: not loopback, unspecified, private, link-local, multicast,
/// broadcast, shared (100.64/10), documentation, benchmarking, ULA, site-local, a mapped or
/// translated IPv4 that is itself not public, or an IPv4-compatible address?
pub fn is_public_unicast(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

fn is_public_v4(v4: Ipv4Addr) -> bool {
    let o = v4.octets();
    !(v4.is_loopback()
        || v4.is_unspecified()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_broadcast()
        || v4.is_multicast()
        || v4.is_documentation()
        || o[0] == 0
        || (o[0] == 100 && (o[1] & 0xC0) == 64) // 100.64.0.0/10 shared address space
        || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0.0/24 IETF protocol assignments
        || (o[0] == 198 && (o[1] & 0xFE) == 18) // 198.18.0.0/15 benchmarking
        || o[0] >= 240) // 240.0.0.0/4 reserved
}

fn is_public_v6(v6: Ipv6Addr) -> bool {
    if let Some(v4) = v6.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    let seg = v6.segments();
    // Deprecated IPv4-compatible (::a.b.c.d) and NAT64 well-known prefix (64:ff9b::/96) carry an IPv4
    // address that must itself be public.
    if seg.first().is_some_and(|s| *s == 0x0064) && seg.get(1).is_some_and(|s| *s == 0xff9b) {
        let v4 = Ipv4Addr::new(
            (seg.get(6).copied().unwrap_or(0) >> 8) as u8,
            (seg.get(6).copied().unwrap_or(0) & 0xff) as u8,
            (seg.get(7).copied().unwrap_or(0) >> 8) as u8,
            (seg.get(7).copied().unwrap_or(0) & 0xff) as u8,
        );
        return is_public_v4(v4);
    }
    let s0 = seg.first().copied().unwrap_or(0);
    let s1 = seg.get(1).copied().unwrap_or(0);
    !(v6.is_loopback()
        || v6.is_unspecified()
        || v6.is_multicast()
        || (s0 & 0xffc0) == 0xfe80 // link-local
        || (s0 & 0xfe00) == 0xfc00 // unique local
        || (s0 & 0xffc0) == 0xfec0 // site-local (deprecated)
        || (s0 == 0x2001 && s1 == 0x0db8) // documentation
        || (s0 == 0x2001 && s1 == 0x0002 && seg.get(2).is_some_and(|s| *s == 0)) // benchmarking
        || (s0 == 0x2002) // 6to4 (carries an IPv4 we cannot vouch for)
        || (s0 == 0x0100 && s1 == 0) // discard-only
        || (s0 == 0 && s1 == 0 && seg.get(2..6).is_some_and(|m| m.iter().all(|s| *s == 0)))) // ::a.b.c.d compatible
}

impl StdEgressGuard {
    fn resolve(host: &str, port: u16) -> Result<Vec<SocketAddr>, EgressRefusal> {
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Ok(vec![SocketAddr::new(ip, port)]);
        }
        let addrs: Vec<SocketAddr> = (host, port)
            .to_socket_addrs()
            .map_err(|_| EgressRefusal::Unresolvable(host.to_owned()))?
            .collect();
        if addrs.is_empty() {
            return Err(EgressRefusal::Unresolvable(host.to_owned()));
        }
        Ok(addrs)
    }
}

impl EgressGuard for StdEgressGuard {
    fn check(&self, url: &str) -> Result<PinnedTarget, EgressRefusal> {
        let (scheme, host, port) = split_url(url)?;
        if scheme != "https" {
            return Err(EgressRefusal::Scheme(scheme));
        }
        let port = port.unwrap_or(443);
        let addrs = Self::resolve(&host, port)?;
        // Every address must be public: one private record among public ones is the rebinding trick.
        if let Some(bad) = addrs.iter().find(|a| !is_public_unicast(a.ip())) {
            return Err(EgressRefusal::BlockedAddress(bad.ip().to_string()));
        }
        Ok(PinnedTarget {
            url: url.to_owned(),
            host,
            port,
            addrs,
        })
    }
}

#[cfg(test)]
mod tests;
