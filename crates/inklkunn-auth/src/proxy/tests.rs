#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::net::IpAddr;

use super::*;

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

#[test]
fn a_client_is_an_ipv4_address_or_an_ipv6_slash_64() {
    assert_eq!(
        ClientAddress::from_ip(ip("203.0.113.7")).as_key(),
        "203.0.113.7"
    );
    assert_eq!(
        ClientAddress::from_ip(ip("2001:db8:1:2:aaaa:bbbb:cccc:dddd")).as_key(),
        "2001:db8:1:2::/64"
    );
    assert_eq!(
        ClientAddress::from_ip(ip("2001:db8:1:2::1")),
        ClientAddress::from_ip(ip("2001:db8:1:2:ffff::9")),
        "one subscriber is one key"
    );
    assert_ne!(
        ClientAddress::from_ip(ip("2001:db8:1:2::1")),
        ClientAddress::from_ip(ip("2001:db8:1:3::1"))
    );
    assert_eq!(
        ClientAddress::from_ip(ip("::ffff:203.0.113.7")).as_key(),
        "203.0.113.7",
        "an IPv4-mapped address is the IPv4 address"
    );
}

#[test]
fn networks_parse_and_contain() {
    let n = Cidr::parse("10.1.0.0/16").unwrap();
    assert!(n.contains(ip("10.1.255.255")) && !n.contains(ip("10.2.0.0")));
    assert!(!n.contains(ip("fd00::1")), "another family never matches");
    assert!(Cidr::parse("192.0.2.1").unwrap().contains(ip("192.0.2.1")));
    assert!(!Cidr::parse("192.0.2.1").unwrap().contains(ip("192.0.2.2")));
    assert!(Cidr::parse("fd00::/8").unwrap().contains(ip("fdff::1")));
    assert!(Cidr::parse("0.0.0.0/0").unwrap().contains(ip("8.8.8.8")));
    assert!(
        Cidr::parse("10.1.2.3/16").unwrap().contains(ip("10.1.9.9")),
        "host bits are masked"
    );
    for bad in [
        "",
        "10.0.0.0/33",
        "fd00::/129",
        "ten",
        "10.0.0.0/x",
        "10.0.0.0/-1",
        "/8",
    ] {
        assert!(Cidr::parse(bad).is_none(), "{bad:?}");
    }
    assert!(
        Cidr::parse("::ffff:10.1.0.5")
            .unwrap()
            .contains(ip("10.1.0.5"))
    );
}

#[test]
fn without_a_declared_proxy_the_header_is_ignored() {
    let p = ProxyPolicy::none();
    assert_eq!(
        p.client(ip("203.0.113.7"), Some("198.51.100.1")).as_key(),
        "203.0.113.7",
        "a client cannot name its own address"
    );
}

#[test]
fn a_trusted_proxy_vouches_for_the_address_it_saw() {
    let p = ProxyPolicy::trusting(&["10.0.0.0/8"]).unwrap();
    assert_eq!(
        p.client(ip("10.0.0.5"), Some("198.51.100.1")).as_key(),
        "198.51.100.1"
    );
    // A chain of two trusted proxies.
    assert_eq!(
        p.client(ip("10.0.0.5"), Some("198.51.100.1, 10.0.0.9"))
            .as_key(),
        "198.51.100.1"
    );
    // The client wrote its own entries on the left; only the rightmost untrusted one is believed.
    assert_eq!(
        p.client(
            ip("10.0.0.5"),
            Some("1.1.1.1, 2.2.2.2, 198.51.100.1, 10.0.0.9")
        )
        .as_key(),
        "198.51.100.1"
    );
    // A request that did not come through a trusted proxy is not believed whatever it says.
    assert_eq!(
        p.client(ip("192.0.2.9"), Some("198.51.100.1")).as_key(),
        "192.0.2.9"
    );
}

#[test]
fn a_header_that_cannot_be_read_counts_the_peer() {
    let p = ProxyPolicy::trusting(&["10.0.0.0/8"]).unwrap();
    let peer = "10.0.0.5";
    for bad in [
        None,
        Some(""),
        Some("not-an-address"),
        Some("198.51.100.1, garbage"),
        Some("198.51.100.1,"),
        Some(&*"1.1.1.1, ".repeat(40)),
    ] {
        assert_eq!(p.client(ip(peer), bad).as_key(), peer, "{bad:?}");
    }
    let long = format!("{}1.1.1.1", " ".repeat(3000));
    assert_eq!(p.client(ip(peer), Some(&long)).as_key(), peer);
}

#[test]
fn a_chain_of_only_trusted_hops_is_the_original_inside_client() {
    let p = ProxyPolicy::trusting(&["10.0.0.0/8"]).unwrap();
    assert_eq!(
        p.client(ip("10.0.0.5"), Some("10.9.9.9, 10.0.0.7"))
            .as_key(),
        "10.9.9.9"
    );
}

#[test]
fn the_operator_is_told_which_entry_is_not_a_network() {
    assert_eq!(
        ProxyPolicy::trusting(&["10.0.0.0/8", "nope"]).unwrap_err(),
        NotANetwork("nope".into())
    );
}

#[test]
fn a_trusted_proxy_whose_header_cannot_be_read_is_said_to_have_fallen_back() {
    let p = ProxyPolicy::trusting(&["10.0.0.0/8"]).unwrap();
    assert!(!p.client(ip("10.0.0.5"), Some("198.51.100.1")).fell_back());
    for unreadable in [None, Some("198.51.100.1:5555"), Some("garbage")] {
        let c = p.client(ip("10.0.0.5"), unreadable);
        assert_eq!(c.as_key(), "10.0.0.5", "{unreadable:?}");
        assert!(
            c.fell_back(),
            "{unreadable:?}: every client behind the proxy now shares this address"
        );
    }
    // A peer that is not a trusted proxy never "falls back": its header is simply not believed.
    assert!(
        !ProxyPolicy::none()
            .client(ip("203.0.113.7"), Some("garbage"))
            .fell_back()
    );
    // Equality is the key's alone.
    assert_eq!(
        p.client(ip("10.0.0.5"), None),
        ProxyPolicy::none().client(ip("10.0.0.5"), None)
    );
}
