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

// ---- the configured provider (the OpenID Connect handoff's U3) ----

fn oidc_settings(secret_file: Option<PathBuf>) -> OidcSettings {
    OidcSettings {
        issuer: "https://accounts.example.org".to_owned(),
        client_id: "inklkunn-wiki".to_owned(),
        client_secret_file: secret_file,
        scopes: vec!["groups".to_owned()],
    }
}

#[test]
fn the_client_secret_is_read_from_its_file_once_without_its_line_break_and_never_shown() {
    let s = Scratch::new("secret");
    let file = |name: &str, content: &[u8]| {
        let p = s.0.join(name);
        std::fs::write(&p, content).unwrap();
        p
    };
    assert_eq!(
        read_client_secret(&file("plain", b"s3cr3t-value")).unwrap(),
        "s3cr3t-value"
    );
    // The line break an editor or `echo` adds is not part of it; nothing else is trimmed.
    assert_eq!(
        read_client_secret(&file("nl", b"s3cr3t-value\r\n")).unwrap(),
        "s3cr3t-value"
    );
    assert_eq!(
        read_client_secret(&file("sp", b" with spaces ")).unwrap(),
        " with spaces "
    );
    // A symbolic link is followed (secret mounts are often links).
    let target = file("target", b"via-link");
    let link = s.0.join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert_eq!(read_client_secret(&link).unwrap(), "via-link");
    // What is refused says why, names the setting, and never repeats the content.
    for (name, content, why) in [
        ("empty", &b"\n"[..], "empty"),
        ("two", &b"one\ntwo\n"[..], "more than one line"),
        ("binary", &b"\xff\xfe"[..], "not UTF-8"),
        ("big", &[b'a'; 5000][..], "longer than 4096"),
    ] {
        let e = read_client_secret(&file(name, content)).unwrap_err();
        assert!(
            e.contains("INKLKUNN_OIDC_CLIENT_SECRET_FILE") && e.contains(why),
            "{name}: {e}"
        );
        assert!(!e.contains("one") || name == "two", "{name}: {e}");
    }
    let e = read_client_secret(&s.0.join("missing")).unwrap_err();
    assert!(e.contains("could not be read"), "{e}");
    let e = read_client_secret(&s.0).unwrap_err();
    assert!(e.contains("not a regular file"), "{e}");
}

#[test]
fn the_callback_the_operator_registers_is_the_app_origin_and_the_route_in_full() {
    assert_eq!(
        callback_address("https://wiki.example"),
        "https://wiki.example/-/sign-in/oidc/callback"
    );
    assert_eq!(
        callback_address("https://wiki.example/"),
        "https://wiki.example/-/sign-in/oidc/callback"
    );
    assert_eq!(
        inklkunn_web::OIDC_CALLBACK_PATH,
        "/-/sign-in/oidc/callback",
        "the route and the registered address are one constant"
    );
}

#[test]
fn the_provider_is_built_from_the_settings_and_asks_for_openid_and_the_scopes_given() {
    let provider = provider_over(
        &oidc_settings(None),
        "https://wiki.example",
        Some("the-secret".to_owned()),
        Arc::new(Discovery),
    );
    assert_eq!(provider.issuer(), "https://accounts.example.org");
    let store = Arc::new(inklkunn_store::MemoryStore::new());
    let auth = attach(
        inklkunn_auth::Authenticator::new(
            store,
            inklkunn_auth::AuthConfig::default(),
            inklkunn_core::IdentityAuthority::for_tests(),
        )
        .unwrap(),
        Some(provider),
    );
    assert!(auth.oidc_configured());
    assert_eq!(auth.oidc_issuer(), Some("https://accounts.example.org"));
    let client = inklkunn_auth::ProxyPolicy::none().client("203.0.113.1".parse().unwrap(), None);
    let begun = auth
        .begin_oidc(
            inklkunn_auth::OidcPurpose::SignIn {
                next: None,
                after_joining: None,
            },
            &client,
            1_800_000_000,
        )
        .unwrap();
    let url = begun.authorization_url;
    assert!(url.contains("&scope=openid%20groups&"), "{url}");
    assert!(
        url.contains("redirect_uri=https%3A%2F%2Fwiki.example%2F-%2Fsign-in%2Foidc%2Fcallback"),
        "{url}"
    );
    assert!(url.contains("client_id=inklkunn-wiki"), "{url}");
    assert!(!url.contains("the-secret"), "{url}");
    // Without one, the authenticator offers no provider.
    let without = attach(
        inklkunn_auth::Authenticator::new(
            Arc::new(inklkunn_store::MemoryStore::new()),
            inklkunn_auth::AuthConfig::default(),
            inklkunn_core::IdentityAuthority::for_tests(),
        )
        .unwrap(),
        None,
    );
    assert!(!without.oidc_configured());
}

/// A fetcher that serves the provider's discovery document and nothing else.
struct Discovery;

impl HttpsFetcher for Discovery {
    fn fetch(&self, request: &FetchRequest) -> Result<Vec<u8>, FetchError> {
        if request.url == "https://accounts.example.org/.well-known/openid-configuration" {
            Ok(br#"{"issuer":"https://accounts.example.org","authorization_endpoint":"https://accounts.example.org/auth","token_endpoint":"https://accounts.example.org/token","jwks_uri":"https://accounts.example.org/jwks"}"#.to_vec())
        } else {
            Err(FetchError::Transport(format!(
                "no route for {}",
                request.url
            )))
        }
    }
}

#[test]
fn a_provider_whose_secret_cannot_be_read_is_refused_at_start_naming_the_setting() {
    let web = WebSettings {
        bind: "127.0.0.1:1".into(),
        app_origin: "https://wiki.example".into(),
        content_origin: "https://files.example".into(),
        admission_budget_mib: 1024,
        admission_wait_ms: 200,
        read: crate::config::ReadSettings::default(),
        trusted_proxies: Vec::new(),
        join: inklkunn_auth::JoinPolicy::Off,
        oidc: Some(oidc_settings(Some(PathBuf::from(
            "/nonexistent/oidc-secret",
        )))),
    };
    let e = provider_if_configured(&web).err().unwrap();
    assert!(e.contains("INKLKUNN_OIDC_CLIENT_SECRET_FILE"), "{e}");
    // No provider configured: nothing to check and nothing built.
    let none = WebSettings {
        oidc: None,
        ..web.clone()
    };
    assert!(provider_if_configured(&none).unwrap().is_none());
}

#[test]
fn the_start_says_a_provider_is_on_with_the_address_to_register_and_warns_of_joining_with_none() {
    let mut web = WebSettings {
        bind: "127.0.0.1:1".into(),
        app_origin: "https://wiki.example".into(),
        content_origin: "https://files.example".into(),
        admission_budget_mib: 1024,
        admission_wait_ms: 200,
        read: crate::config::ReadSettings::default(),
        trusted_proxies: Vec::new(),
        join: inklkunn_auth::JoinPolicy::Off,
        oidc: None,
    };
    assert!(start_notes(&web).is_empty(), "nothing to say about nothing");
    web.join = inklkunn_auth::JoinPolicy::Provider;
    let notes = start_notes(&web);
    assert_eq!(notes.len(), 1);
    assert!(
        notes.first().unwrap().contains("INKLKUNN_JOIN=provider")
            && notes.first().unwrap().contains("INKLKUNN_OIDC_ISSUER"),
        "{notes:?}"
    );
    web.oidc = Some(oidc_settings(None));
    let notes = start_notes(&web);
    assert_eq!(notes.len(), 1);
    assert!(
        notes
            .first()
            .unwrap()
            .contains("https://accounts.example.org")
            && notes
                .first()
                .unwrap()
                .contains("https://wiki.example/-/sign-in/oidc/callback"),
        "{notes:?}"
    );
    // The secret is never in what the start prints: only its path is a setting, and it is not printed.
    web.oidc = Some(oidc_settings(Some(PathBuf::from("/run/secrets/oidc"))));
    assert!(!start_notes(&web).first().unwrap().contains("/run/secrets"));
}
