use super::*;

#[test]
fn literal_addresses_that_must_be_refused() {
    let g = StdEgressGuard;
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
        "https://198.18.0.1/",
        "https://224.0.0.1/",
        "https://255.255.255.255/",
        "https://[::1]/",
        "https://[::]/",
        "https://[::ffff:127.0.0.1]/",
        "https://[::ffff:10.0.0.1]:8443/",
        "https://[64:ff9b::7f00:1]/",
        "https://[fe80::1]/",
        "https://[fc00::1]/",
        "https://[fd12::1]/",
        "https://[fec0::1]/",
        "https://[2001:db8::1]/",
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
    let g = StdEgressGuard;
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
    ] {
        assert!(
            matches!(g.check(bad), Err(EgressRefusal::Unparsable(_))),
            "{bad}"
        );
    }
}

#[test]
fn public_literals_are_pinned_without_dns() {
    let g = StdEgressGuard;
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
    assert!(is_public_unicast(IpAddr::V6(
        "::ffff:93.184.216.34"
            .parse()
            .unwrap_or(Ipv6Addr::LOCALHOST)
    )));
}
