#![allow(clippy::expect_used, clippy::unwrap_used)]
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use inklkunn_auth::oidc::MAX_RESPONSE_BYTES;
use inklkunn_core::{Resolver, StdEgressGuard};

use super::*;

struct Fixed(Vec<SocketAddr>);

impl Resolver for Fixed {
    fn resolve(&self, _host: &str, port: u16) -> std::io::Result<Vec<SocketAddr>> {
        Ok(self
            .0
            .iter()
            .map(|a| SocketAddr::new(a.ip(), port))
            .collect())
    }
}

const SECRET: &str = "Basic c2VjcmV0LWNsaWVudC1zZWNyZXQ=";

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let p =
            std::env::temp_dir().join(format!("inklkunn-outbound-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Scratch(p)
    }

    /// A stand-in standin that records its arguments and answers with `body`.
    fn standin(&self, body: &str) -> PathBuf {
        let rec = self.0.display();
        let p = self.0.join("standin");
        std::fs::write(
            &p,
            format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{rec}/argv'\ncat > /dev/null\n{body}\n"),
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    fn argv(&self) -> String {
        std::fs::read_to_string(self.0.join("argv")).unwrap_or_default()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fetcher(resolved: &[&str], standin: &std::path::Path) -> GuardedCurlFetcher {
    let addrs = resolved
        .iter()
        .map(|a| SocketAddr::new(a.parse().unwrap(), 443))
        .collect();
    GuardedCurlFetcher::new(
        Arc::new(StdEgressGuard::with_resolver(Arc::new(Fixed(addrs)))),
        CurlFetcher::new()
            .with_programs("bwrap", standin)
            .unconfined(),
    )
}

fn post() -> FetchRequest {
    FetchRequest {
        url: "https://idp.example.org/token".to_owned(),
        form: Some("grant_type=authorization_code&code=SECRETCODE".to_owned()),
        authorization: Some(SECRET.to_owned()),
    }
}

/// Retry while a script just written is still open in a forked child (`ETXTBSY`).
fn fetch(f: &GuardedCurlFetcher, r: &FetchRequest) -> Result<Vec<u8>, FetchError> {
    for _ in 0..50 {
        match f.fetch(r) {
            Err(FetchError::Transport(e)) if e.contains("busy") => {
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            other => return other,
        }
    }
    f.fetch(r)
}

#[test]
fn the_byte_cap_of_the_seam_and_of_the_fetcher_are_the_same_number() {
    assert_eq!(MAX_RESPONSE_BYTES, inklkunn_fetch::DEFAULT_MAX_BYTES);
    assert_eq!(
        CurlFetcher::new().limits().max_bytes,
        MAX_RESPONSE_BYTES,
        "the default limits and the seam agree"
    );
}

#[test]
fn the_guards_pinned_addresses_are_what_curl_is_told_to_connect_to() {
    let s = Scratch::new("pin");
    let standin = s.standin("printf '{\"ok\":true}\\n200'");
    let f = fetcher(&["93.184.216.34", "2606:2800:220:1::1"], &standin);
    let body = fetch(&f, &post()).expect("a 200");
    assert_eq!(body, br#"{"ok":true}"#);
    let argv = s.argv();
    assert!(
        argv.contains("idp.example.org:443:93.184.216.34,[2606:2800:220:1::1]\n"),
        "{argv}"
    );
    assert!(argv.contains("--proto\n=https\n") && argv.contains("--max-redirs\n0\n"));
    assert!(!argv.contains("SECRETCODE") && !argv.contains("c2VjcmV0"));
}

#[test]
fn a_destination_the_guard_refuses_never_reaches_curl() {
    let s = Scratch::new("refused");
    let standin = s.standin("printf 'x\\n200'");
    // The name resolves to a private address (a DNS answer that points inside).
    let f = fetcher(&["10.0.0.5"], &standin);
    let e = fetch(&f, &post()).unwrap_err();
    assert!(
        matches!(&e, FetchError::Refused(m) if m.contains("10.0.0.5")),
        "{e:?}"
    );
    // Not https.
    let f = fetcher(&["93.184.216.34"], &standin);
    let mut r = post();
    r.url = "http://idp.example.org/token".to_owned();
    assert!(matches!(fetch(&f, &r).unwrap_err(), FetchError::Refused(_)));
    assert_eq!(s.argv(), "", "standin was never started");
}

#[test]
fn failures_are_the_seams_errors_and_carry_no_secret() {
    let s = Scratch::new("fail");
    for (body, check) in [
        ("printf 'nope\\n404'", "404"),
        ("printf 'moved\\n302'", "302"),
        (
            "echo \"standin: $SECRETCODE c2VjcmV0LWNsaWVudC1zZWNyZXQ=\" >&2; exit 7",
            "connection",
        ),
    ] {
        let standin = s.standin(body);
        let f = fetcher(&["93.184.216.34"], &standin);
        let e = fetch(&f, &post()).unwrap_err();
        let shown = format!("{e} {e:?}");
        assert!(matches!(e, FetchError::Transport(_)), "{shown}");
        assert!(shown.contains(check), "{shown}");
        assert!(
            !shown.contains("c2VjcmV0") && !shown.contains("SECRETCODE"),
            "{shown}"
        );
    }
    let standin = s.standin("exit 63");
    let f = fetcher(&["93.184.216.34"], &standin);
    assert_eq!(fetch(&f, &post()).unwrap_err(), FetchError::TooLarge);
}
