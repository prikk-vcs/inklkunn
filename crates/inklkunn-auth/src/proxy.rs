//! The client address a throttle counts (RFC 002 D-1): **the address from a declared trusted proxy only**.
//! The server sees the peer that connected. When that peer is a proxy the operator declared (a CIDR list),
//! the real client is the address that proxy saw, which it appends to `X-Forwarded-For`; when it is anyone
//! else, the header is attacker-controlled text and is ignored. Walking the header from the right and
//! stopping at the first address that is not a trusted proxy means entries a client prepended on the left
//! cannot choose the address that is counted.

use std::fmt;
use std::net::{IpAddr, Ipv6Addr};

/// The address a throttle is keyed by: an IPv4 address, or the /64 an IPv6 address is in (one subscriber is
/// given at least a /64, so counting single IPv6 addresses would give an attacker 2^64 fresh ones).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClientAddress(String);

impl ClientAddress {
    /// The key for an address.
    pub fn from_ip(ip: IpAddr) -> Self {
        match ip.to_canonical() {
            IpAddr::V4(v4) => ClientAddress(v4.to_string()),
            IpAddr::V6(v6) => {
                let prefix = u128::from(v6) & (u128::MAX << 64);
                ClientAddress(format!("{}/64", Ipv6Addr::from(prefix)))
            }
        }
    }

    /// The text a throttle uses as its key.
    pub fn as_key(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ClientAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A network, `address/prefix`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr {
    network: IpAddr,
    prefix: u8,
}

impl Cidr {
    /// Parse `203.0.113.0/24`, `fd00::/8`, or a single address (a /32 or /128).
    pub fn parse(s: &str) -> Option<Cidr> {
        let (addr, prefix) = match s.split_once('/') {
            Some((a, p)) => (a, Some(p.parse::<u8>().ok()?)),
            None => (s, None),
        };
        let addr: IpAddr = addr.trim().parse::<IpAddr>().ok()?.to_canonical();
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = prefix.unwrap_or(max);
        if prefix > max {
            return None;
        }
        Some(Cidr {
            network: mask(addr, prefix),
            prefix,
        })
    }

    /// Is `ip` in the network?
    pub fn contains(&self, ip: IpAddr) -> bool {
        let ip = ip.to_canonical();
        ip.is_ipv4() == self.network.is_ipv4() && mask(ip, self.prefix) == self.network
    }
}

fn mask(ip: IpAddr, prefix: u8) -> IpAddr {
    match ip {
        IpAddr::V4(v4) => {
            let m = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - u32::from(prefix))
            };
            IpAddr::V4((u32::from(v4) & m).into())
        }
        IpAddr::V6(v6) => {
            let m = if prefix == 0 {
                0
            } else {
                u128::MAX << (128 - u32::from(prefix))
            };
            IpAddr::V6((u128::from(v6) & m).into())
        }
    }
}

/// A trusted-proxy entry the operator wrote that is not a network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotANetwork(pub String);

impl fmt::Display for NotANetwork {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` is not an address or an address/prefix", self.0)
    }
}

impl std::error::Error for NotANetwork {}

/// Which peers are trusted proxies. The default trusts none, so `X-Forwarded-For` is ignored.
#[derive(Debug, Clone, Default)]
pub struct ProxyPolicy {
    trusted: Vec<Cidr>,
}

/// The most entries of `X-Forwarded-For` read, and the longest header: beyond either the header is taken as
/// hostile and the peer is counted instead.
const MAX_HOPS: usize = 32;
const MAX_HEADER: usize = 2048;

impl ProxyPolicy {
    /// No trusted proxy.
    pub fn none() -> Self {
        Self::default()
    }

    /// Trust the proxies in these networks.
    pub fn trusting(networks: &[&str]) -> Result<Self, NotANetwork> {
        let trusted = networks
            .iter()
            .map(|n| Cidr::parse(n).ok_or_else(|| NotANetwork((*n).to_owned())))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ProxyPolicy { trusted })
    }

    fn is_trusted(&self, ip: IpAddr) -> bool {
        self.trusted.iter().any(|c| c.contains(ip))
    }

    /// The client behind a connection from `peer` that carried `forwarded_for` (the `X-Forwarded-For`
    /// header's values joined with `, `, if any). Anything unreadable counts the peer.
    pub fn client(&self, peer: IpAddr, forwarded_for: Option<&str>) -> ClientAddress {
        if !self.is_trusted(peer) {
            return ClientAddress::from_ip(peer);
        }
        let Some(hops) = forwarded_for.and_then(parse_hops) else {
            return ClientAddress::from_ip(peer);
        };
        let client = hops
            .iter()
            .rev()
            .find(|ip| !self.is_trusted(**ip))
            .or_else(|| hops.first())
            .copied()
            .unwrap_or(peer);
        ClientAddress::from_ip(client)
    }
}

fn parse_hops(header: &str) -> Option<Vec<IpAddr>> {
    if header.len() > MAX_HEADER {
        return None;
    }
    let hops = header
        .split(',')
        .map(|h| h.trim().parse::<IpAddr>().ok())
        .collect::<Option<Vec<_>>>()?;
    (!hops.is_empty() && hops.len() <= MAX_HOPS).then_some(hops)
}

#[cfg(test)]
mod tests;
