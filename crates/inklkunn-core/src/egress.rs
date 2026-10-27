//! The one egress guard (threat model C-8, INV-3; internal design LAY-5): every outbound fetch — OpenID
//! Connect, interwiki checks, link previews, webhooks — passes its URL here first and connects only to the
//! **pinned** addresses it returns, never re-resolving the name (a DNS rebinding would otherwise move the
//! target after the check). Public unicast only; HTTPS only; redirects are re-checked by the caller; a
//! host is ASCII (an internationalized name is given as its A-labels by the caller).
//!
//! The classification is re-derived from the registries (RFC 1122, 1918, 3927, 4193, 4291, 5737, 6598,
//! 6890, 7526, 7343, 4380) rather than copied, and tested against planeter's list as a cross-check. Name
//! resolution is behind [`Resolver`] so the guard's rules are testable without a network and a deployment
//! can bound it.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;

/// Why a URL was refused or could not be evaluated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EgressRefusal {
    /// Not `https`.
    Scheme(String),
    /// No host could be read out of the URL, or the URL carries bytes a request line must not (controls,
    /// whitespace, userinfo, a malformed port).
    Unparsable(String),
    /// The host is not ASCII: give an internationalized name as its A-labels (IDNA).
    NonAsciiHost(String),
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
            EgressRefusal::Unparsable(u) => write!(f, "outbound URL is not acceptable: {u}"),
            EgressRefusal::NonAsciiHost(h) => {
                write!(f, "outbound host `{h}` is not ASCII; give its A-labels")
            }
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
    /// The URL as the caller gave it (checked, not normalized).
    pub url: String,
    /// The host as written.
    pub host: String,
    /// The port (443 when absent).
    pub port: u16,
    /// The addresses to connect to; every one is public unicast; an IPv4-mapped IPv6 address is given as
    /// the IPv4 address it carries.
    pub addrs: Vec<SocketAddr>,
}

/// The admission point for outbound fetches.
pub trait EgressGuard: Send + Sync {
    /// Admit `url` and pin its addresses, or refuse.
    fn check(&self, url: &str) -> Result<PinnedTarget, EgressRefusal>;
}

/// Name resolution, behind a seam so that the guard's rules are testable without a network.
pub trait Resolver: Send + Sync {
    /// Every address `host` resolves to, with `port`.
    fn resolve(&self, host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>>;
}

/// The operating system's resolver. **Blocking and untimed**: a deployment that cannot accept that wraps
/// it (a worker with a deadline), which is RFC 002's handoff's concern (IQ-3).
#[derive(Debug, Default, Clone, Copy)]
pub struct StdResolver;

impl Resolver for StdResolver {
    fn resolve(&self, host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
        Ok((host, port).to_socket_addrs()?.collect())
    }
}

/// The production guard: HTTPS only, public unicast only, addresses pinned.
#[derive(Clone)]
pub struct StdEgressGuard {
    resolver: Arc<dyn Resolver>,
}

impl std::fmt::Debug for StdEgressGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StdEgressGuard")
    }
}

impl Default for StdEgressGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl StdEgressGuard {
    /// Over the operating system's resolver.
    pub fn new() -> Self {
        StdEgressGuard {
            resolver: Arc::new(StdResolver),
        }
    }

    /// Over another resolver (a bounded one in service; a double in tests).
    pub fn with_resolver(resolver: Arc<dyn Resolver>) -> Self {
        StdEgressGuard { resolver }
    }
}

/// `scheme://host[:port]/…` → (scheme, host, port). Hosts may be `[v6]`. Refuses userinfo, an empty or
/// non-ASCII host, a second colon outside brackets, a malformed or zero port, and any byte a request line
/// must not carry (controls, whitespace, DEL) anywhere in the URL.
pub fn split_url(url: &str) -> Result<(String, String, Option<u16>), EgressRefusal> {
    let bad = || EgressRefusal::Unparsable(url.to_owned());
    if url.bytes().any(|b| b <= 0x20 || b == 0x7f) {
        return Err(bad());
    }
    let (scheme, rest) = url.split_once("://").ok_or_else(bad)?;
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = rest.get(..authority_end).unwrap_or("");
    // Userinfo is refused outright: `https://trusted@evil/` is a classic confusion.
    if authority.contains('@') || authority.is_empty() {
        return Err(bad());
    }
    let (host, port_text) = if let Some(after) = authority.strip_prefix('[') {
        let close = after.find(']').ok_or_else(bad)?;
        let host = after.get(..close).unwrap_or("").to_owned();
        (host, after.get(close + 1..).unwrap_or(""))
    } else if let Some((h, p)) = authority.split_once(':') {
        if p.contains(':') {
            return Err(bad());
        }
        (h.to_owned(), authority.get(h.len()..).unwrap_or(""))
    } else {
        (authority.to_owned(), "")
    };
    let port = match port_text.strip_prefix(':') {
        Some(p) => {
            let n = p.parse::<u16>().map_err(|_| bad())?;
            if n == 0 {
                return Err(bad());
            }
            Some(n)
        }
        None if port_text.is_empty() => None,
        None => return Err(bad()),
    };
    if host.is_empty() {
        return Err(bad());
    }
    if !host.is_ascii() {
        return Err(EgressRefusal::NonAsciiHost(host));
    }
    Ok((scheme.to_ascii_lowercase(), host, port))
}

/// Is `ip` a public unicast address: not loopback, unspecified, private, link-local, multicast,
/// broadcast, shared (100.64/10), documentation, benchmarking, the 6to4 relay anycast, ULA, site-local,
/// Teredo, ORCHID, a mapped or translated IPv4 that is itself not public, or an IPv4-compatible address?
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
        || (o[0] == 192 && o[1] == 88 && o[2] == 99) // 192.88.99.0/24 deprecated 6to4 relay anycast
        || (o[0] == 198 && (o[1] & 0xFE) == 18) // 198.18.0.0/15 benchmarking
        || o[0] >= 240) // 240.0.0.0/4 reserved
}

fn is_public_v6(v6: Ipv6Addr) -> bool {
    if let Some(v4) = v6.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    let seg = v6.segments();
    let s = |i: usize| seg.get(i).copied().unwrap_or(0);
    // NAT64: the well-known prefix 64:ff9b::/96 carries an IPv4 address that must itself be public; the
    // local-use prefix 64:ff9b:1::/48 is never public.
    if s(0) == 0x0064 && s(1) == 0xff9b {
        if s(2) != 0 || s(3) != 0 || s(4) != 0 || s(5) != 0 {
            return false;
        }
        let v4 = Ipv4Addr::new(
            (s(6) >> 8) as u8,
            (s(6) & 0xff) as u8,
            (s(7) >> 8) as u8,
            (s(7) & 0xff) as u8,
        );
        return is_public_v4(v4);
    }
    let (s0, s1) = (s(0), s(1));
    !(v6.is_loopback()
        || v6.is_unspecified()
        || v6.is_multicast()
        || (s0 & 0xffc0) == 0xfe80 // link-local
        || (s0 & 0xfe00) == 0xfc00 // unique local
        || (s0 & 0xffc0) == 0xfec0 // site-local (deprecated)
        || (s0 == 0x2001 && s1 == 0x0db8) // documentation
        || (s0 == 0x2001 && s1 == 0x0002 && s(2) == 0) // benchmarking
        || (s0 == 0x2001 && s1 == 0x0000) // Teredo (carries an IPv4 we cannot vouch for)
        || (s0 == 0x2001 && (s1 & 0xfff0) == 0x0010) // ORCHID (deprecated)
        || (s0 == 0x2001 && (s1 & 0xfff0) == 0x0020) // ORCHIDv2
        || (s0 == 0x2002) // 6to4 (carries an IPv4 we cannot vouch for)
        || (s0 == 0x0100 && s1 == 0) // discard-only
        || (s0 == 0 && s1 == 0 && (2..6).all(|i| s(i) == 0))) // ::a.b.c.d compatible
}

/// An IPv4-mapped IPv6 socket address, given as the IPv4 address it carries.
fn unmapped(a: SocketAddr) -> SocketAddr {
    match a.ip() {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => SocketAddr::new(IpAddr::V4(v4), a.port()),
            None => a,
        },
        IpAddr::V4(_) => a,
    }
}

impl EgressGuard for StdEgressGuard {
    fn check(&self, url: &str) -> Result<PinnedTarget, EgressRefusal> {
        let (scheme, host, port) = split_url(url)?;
        if scheme != "https" {
            return Err(EgressRefusal::Scheme(scheme));
        }
        let port = port.unwrap_or(443);
        let addrs: Vec<SocketAddr> = if let Ok(ip) = host.parse::<IpAddr>() {
            vec![SocketAddr::new(ip, port)]
        } else {
            self.resolver
                .resolve(&host, port)
                .map_err(|_| EgressRefusal::Unresolvable(host.clone()))?
        };
        if addrs.is_empty() {
            return Err(EgressRefusal::Unresolvable(host));
        }
        // Every address must be public: one private record among public ones is the rebinding trick.
        if let Some(bad) = addrs.iter().find(|a| !is_public_unicast(a.ip())) {
            return Err(EgressRefusal::BlockedAddress(bad.ip().to_string()));
        }
        Ok(PinnedTarget {
            url: url.to_owned(),
            host,
            port,
            addrs: addrs.into_iter().map(unmapped).collect(),
        })
    }
}

#[cfg(test)]
mod tests;
