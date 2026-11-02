//! The client address a throttle counts (RFC 002 D-1): **the address from a declared trusted proxy only**.
//! The server sees the peer that connected. When that peer is a proxy the operator declared (a CIDR list),
//! the real client is the address that proxy saw, which it appends to `X-Forwarded-For`; when it is anyone
//! else, the header is attacker-controlled text and is ignored. Walking the header from the right and
//! stopping at the first address that is not a trusted proxy means entries a client prepended on the left
//! cannot choose the address that is counted.

use std::fmt;
use std::net::{IpAddr, Ipv6Addr};

/// The address a throttle is keyed by: an IPv4 address, or the /64 an IPv6 address is in (one subscriber is
/// given at least a /64, so counting single IPv6 addresses would give an attacker 2^64 fresh ones). It is
/// made only by [`ProxyPolicy::client`], so no caller can name an address that skips the trusted-proxy rule.
#[derive(Debug, Clone)]
pub struct ClientAddress {
    key: String,
    fell_back: bool,
}

impl PartialEq for ClientAddress {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for ClientAddress {}

impl std::hash::Hash for ClientAddress {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

impl ClientAddress {
    /// The key for an address (tests; the product builds one only through [`ProxyPolicy::client`]).
    #[cfg(test)]
    pub(crate) fn from_ip(ip: IpAddr) -> Self {
        Self::keyed(ip, false)
    }

    fn keyed(ip: IpAddr, fell_back: bool) -> Self {
        let key = match ip.to_canonical() {
            IpAddr::V4(v4) => v4.to_string(),
            IpAddr::V6(v6) => {
                let prefix = u128::from(v6) & (u128::MAX << 64);
                format!("{}/64", Ipv6Addr::from(prefix))
            }
        };
        ClientAddress { key, fell_back }
    }

    /// The text a throttle uses as its key.
    pub fn as_key(&self) -> &str {
        &self.key
    }

    /// `true` when the connection came from a trusted proxy but its `X-Forwarded-For` was missing or could
    /// not be read, so the proxy's own address stands for the client. **Every client behind that proxy then
    /// shares one budget** (thirty bad attempts by anyone lock everyone) until the proxy's header is fixed:
    /// a web layer logs it when it sees it.
    pub fn fell_back(&self) -> bool {
        self.fell_back
    }
}

impl fmt::Display for ClientAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.key)
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

/// The most hops of `X-Forwarded-For` read from the right before the header is taken as hostile or broken and
/// the peer is counted instead. A chain of proxies is a handful long.
const MAX_HOPS: usize = 32;

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
    /// header's values joined with `, `, if any). Anything unreadable counts the peer, and says so
    /// ([`ClientAddress::fell_back`]).
    pub fn client(&self, peer: IpAddr, forwarded_for: Option<&str>) -> ClientAddress {
        if !self.is_trusted(peer) {
            return ClientAddress::keyed(peer, false);
        }
        match forwarded_for.and_then(|header| self.rightmost_client(header)) {
            Some(client) => ClientAddress::keyed(client, false),
            None => ClientAddress::keyed(peer, true),
        }
    }

    /// Walk the header **from the right**, which is the end a trusted proxy writes to: a hop that is a trusted
    /// proxy is skipped, the first valid hop that is not one is the client, and what stands further left is
    /// never read (so junk, or any number of hops, a client prepended cannot move the answer or make the
    /// header unreadable). A hop that cannot be read *before* that point (a proxy writes only addresses) makes
    /// the header unreadable; so does walking [`MAX_HOPS`] hops without finding a client. When every hop is a
    /// trusted proxy the leftmost one is the original inside client.
    fn rightmost_client(&self, header: &str) -> Option<IpAddr> {
        let mut leftmost_trusted = None;
        let mut hops = header.rsplit(',');
        for _ in 0..MAX_HOPS {
            let Some(hop) = hops.next() else {
                // Every hop there was is a trusted proxy.
                return leftmost_trusted;
            };
            let ip = hop.trim().parse::<IpAddr>().ok()?;
            if !self.is_trusted(ip) {
                return Some(ip);
            }
            leftmost_trusted = Some(ip);
        }
        // More hops than any chain of proxies has, all of them trusted: hostile or broken.
        if hops.next().is_some() {
            return None;
        }
        leftmost_trusted
    }
}

#[cfg(test)]
mod tests;
