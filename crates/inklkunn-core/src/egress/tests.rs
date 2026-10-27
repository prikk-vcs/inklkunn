use super::*;

#[test]
fn literal_addresses_that_must_be_refused() {
    let g = StdEgressGuard::new();
    for url in [
        "https://127.0.0.1/",
        "https://127.1.2.3:8443/x",
        "https://10.0.0.1/",
        "https://172.16.5.5/",
        "https://192.168.1.1/",
        "https://169.254.169.254/latest/meta-data",
        "https://0.0.0.0/",
        "https://100.64.0.1/",
        "https://192.0.0.1/",
        "https://192.88.99.1/",
        "https://198.18.0.1/",
        "https://224.0.0.1/",
        "https://255.255.255.255/",
        "https://[::1]/",
        "https://[::]/",
        "https://[::ffff:127.0.0.1]/",
        "https://[::ffff:10.0.0.1]:8443/",
        "https://[64:ff9b::7f00:1]/",
        "https://[64:ff9b:1::5db8:d822]/",
        "https://[fe80::1]/",
        "https://[fc00::1]/",
        "https://[fd12::1]/",
        "https://[fec0::1]/",
        "https://[2001:db8::1]/",
        "https://[2001::1]/",
        "https://[2001:10::1]/",
        "https://[2001:20::1]/",
        "https://[2002:7f00:1::1]/",
        "https://[ff02::1]/",
    ] {
        assert!(
            matches!(g.check(url), Err(EgressRefusal::BlockedAddress(_))),
            "{url}"
        );
    }
}

#[test]
fn schemes_and_shapes_are_refused_before_resolution() {
    let g = StdEgressGuard::new();
    assert!(
        matches!(g.check("http://93.184.216.34/"), Err(EgressRefusal::Scheme(s)) if s == "http")
    );
    assert!(matches!(
        g.check("ftp://93.184.216.34/"),
        Err(EgressRefusal::Scheme(_))
    ));
    for bad in [
        "no-scheme",
        "https://",
        "https://user@93.184.216.34/",
        "https://[::1/",
        "https://93.184.216.34:99999/",
        "https://93.184.216.34:0/",
        "https://93.184.216.34:443:1/",
        "https://93.184.216.34/\n",
        "https://93.184.216.34/a b",
        "https://93.184.216.34/\u{7f}",
        "https://93.184.216.34:/",
    ] {
        assert!(
            matches!(g.check(bad), Err(EgressRefusal::Unparsable(_))),
            "{bad:?}"
        );
    }
    assert!(matches!(
        g.check("https://bücher.example/"),
        Err(EgressRefusal::NonAsciiHost(h)) if h == "bücher.example"
    ));
}

#[test]
fn public_literals_are_pinned_without_dns() {
    let g = StdEgressGuard::new();
    let t = g
        .check("https://93.184.216.34/path?q#f")
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(t.port, 443);
    assert_eq!(
        t.addrs,
        vec![SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)),
            443
        )]
    );
    let t6 = g
        .check("https://[2606:4700::6810:84e5]:8443/")
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(t6.port, 8443);
    assert!(is_public_unicast(IpAddr::V6(
        "2606:4700::6810:84e5"
            .parse()
            .unwrap_or(Ipv6Addr::LOCALHOST)
    )));
    // A mapped public IPv4 literal is admitted and pinned as the IPv4 address it carries.
    let mapped = g
        .check("https://[::ffff:93.184.216.34]/")
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        mapped.addrs,
        vec![SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)),
            443
        )]
    );
}

struct Fixed(Vec<SocketAddr>);

impl Resolver for Fixed {
    fn resolve(&self, _host: &str, _port: u16) -> std::io::Result<Vec<SocketAddr>> {
        Ok(self.0.clone())
    }
}

struct Failing;

impl Resolver for Failing {
    fn resolve(&self, _host: &str, _port: u16) -> std::io::Result<Vec<SocketAddr>> {
        Err(std::io::Error::other("no network"))
    }
}

#[test]
fn every_resolved_address_must_be_public_and_the_pin_is_what_resolved() {
    let public = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34)), 443);
    let private = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 7)), 443);
    let g = StdEgressGuard::with_resolver(Arc::new(Fixed(vec![public, private])));
    assert!(matches!(
        g.check("https://example.test/"),
        Err(EgressRefusal::BlockedAddress(a)) if a == "10.0.0.7"
    ));
    let g = StdEgressGuard::with_resolver(Arc::new(Fixed(vec![public])));
    let t = g
        .check("https://example.test/x")
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(t.addrs, vec![public]);
    assert_eq!(t.host, "example.test");
    let g = StdEgressGuard::with_resolver(Arc::new(Fixed(Vec::new())));
    assert!(matches!(
        g.check("https://example.test/"),
        Err(EgressRefusal::Unresolvable(_))
    ));
    let g = StdEgressGuard::with_resolver(Arc::new(Failing));
    assert!(matches!(
        g.check("https://example.test/"),
        Err(EgressRefusal::Unresolvable(_))
    ));
}
