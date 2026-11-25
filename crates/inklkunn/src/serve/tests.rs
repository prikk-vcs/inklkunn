#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;

use inklkunn_web::serve::Shutdown;

use super::*;

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir(tag: &str) -> Dir {
    let p = std::env::temp_dir().join(format!("inklkunn-serve-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    Dir(p)
}

fn prikk_on_path() -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("prikk").is_file()))
}

fn settings(data: &std::path::Path, dev: bool) -> Settings {
    Settings {
        data_dir: data.to_path_buf(),
        key_dir: data.join("keys"),
        secret_file: data.join("secret"),
        seed_dir: PathBuf::from("/dev/shm/inklkunn-serve-tests"),
        prikk: OsString::from("prikk"),
        dev_unconfined: dev,
    }
}

fn web(bind: &str, app: &str, content: &str) -> WebSettings {
    WebSettings {
        bind: bind.into(),
        app_origin: app.into(),
        content_origin: content.into(),
        admission_budget_mib: 1024,
        admission_wait_ms: 200,
        read: crate::config::ReadSettings::default(),
        trusted_proxies: Vec::new(),
        join: inklkunn_auth::JoinPolicy::Off,
        oidc: None,
        governance: inklkunn_core::governance::PlatformLimits::default(),
        appeals: inklkunn_core::appeal::AppealLimits::default(),
    }
}

#[test]
fn a_refusal_names_the_setting_before_anything_is_opened() {
    let d = dir("refuse");
    let s = settings(&d.0, true);
    for (w, why) in [
        (
            web("not-an-address", "http://localhost:1", "http://localhost:2"),
            "INKLKUNN_BIND",
        ),
        (
            web("127.0.0.1:1", "wiki.example", "http://localhost:2"),
            "INKLKUNN_APP_ORIGIN",
        ),
        (
            web("127.0.0.1:1", "http://localhost:1", "http://localhost:1"),
            "same origin",
        ),
        (
            web("0.0.0.0:1", "https://a.example", "https://a.example:8443"),
            "own host name",
        ),
    ] {
        let e = prepare(&s, &w, IdentityAuthority::for_tests())
            .err()
            .unwrap();
        assert!(e.contains(why), "{why}: {e}");
    }
    assert!(
        !d.0.join("store.sqlite").exists(),
        "nothing was created by a refused start"
    );
}

#[test]
fn without_the_dev_flag_a_host_that_cannot_confine_prikk_refuses_to_start() {
    // The host's answer is injected (bubblewrap's own words, as `probe_bwrap` reports them), so this runs on every
    // host and never depends on what the runner allows (maintenance M40).
    let d = dir("unconfined");
    let e = prepare_probing(
        &settings(&d.0, false),
        &web("127.0.0.1:1", "http://localhost:1", "http://localhost:2"),
        IdentityAuthority::for_tests(),
        Arc::new(|_| {}),
        &|_| Err(CANNOT_CONFINE.to_owned()),
    )
    .err()
    .unwrap();
    assert!(
        e.contains("bubblewrap cannot confine") && e.contains("--dev-unconfined"),
        "{e}"
    );
    // The reason bubblewrap gave is in the refusal.
    assert!(e.contains("No permissions to create new namespace"), "{e}");
    assert!(
        !d.0.join("store.sqlite").exists(),
        "nothing was created by a refused start"
    );
}

/// What `probe_bwrap` says when bubblewrap cannot make the namespaces.
const CANNOT_CONFINE: &str = "bubblewrap cannot confine a process here (`bwrap`): bwrap: No permissions to create new namespace";

#[test]
fn the_sandbox_decision_follows_the_answer_it_is_given_not_the_host() {
    // Both answers, on any host: a host that can confine is refused when the answer says it cannot, and one that
    // cannot is let through when the answer says it can. The dev flag never asks.
    let d = dir("answer");
    let strict = settings(&d.0, false);
    let refused = strict
        .sandbox_probed(&|_| Err(CANNOT_CONFINE.to_owned()))
        .unwrap_err();
    assert!(refused.0.contains("--dev-unconfined"), "{refused}");
    assert!(
        matches!(strict.sandbox_probed(&|_| Ok(())), Ok(Sandbox::Bwrap(_))),
        "an answer of yes gives the confined policy"
    );
    let dev = settings(&d.0, true);
    assert!(matches!(
        dev.sandbox_probed(&|_| panic!("the dev flag does not ask the host")),
        Ok(Sandbox::Unconfined)
    ));
}

#[test]
fn a_refusal_for_a_program_bubblewrap_cannot_find_names_the_loader_not_user_namespaces() {
    // IS-19's shape: bubblewrap made its namespaces and found no program (the host's loader or libraries are not where the
    // sandbox lays them out), so advising user namespaces would send the operator the wrong way (M46).
    let d = dir("loader");
    let strict = settings(&d.0, false);
    let loader = strict
        .sandbox_probed(&|_| {
            Err("bubblewrap cannot confine a process here (`bwrap`): bwrap: execvp /usr/bin/true: No such file or directory".to_owned())
        })
        .unwrap_err()
        .0;
    assert!(loader.contains("execvp /usr/bin/true"), "{loader}");
    assert!(loader.contains("loader and libraries"), "{loader}");
    assert!(
        loader.contains("What a host needs for confinement"),
        "{loader}"
    );
    assert!(!loader.contains("user namespaces"), "{loader}");
    assert!(loader.contains("--dev-unconfined"), "{loader}");
    // The refusal says *bubblewrap cannot confine* once; a reason that does not say it gets the sentence put before it.
    let once = strict
        .sandbox_probed(&|_| Err(CANNOT_CONFINE.to_owned()))
        .unwrap_err()
        .0;
    assert_eq!(
        once.matches("bubblewrap cannot confine").count(),
        1,
        "{once}"
    );
    let plain = strict
        .sandbox_probed(&|_| Err("a probe's own words".to_owned()))
        .unwrap_err()
        .0;
    assert!(
        plain.starts_with("bubblewrap cannot confine prikk here (a probe's own words);"),
        "{plain}"
    );
    // A refusal of the namespaces keeps its advice, and a missing `bwrap` (no `execvp`) is not a loader's.
    for why in [
        CANNOT_CONFINE,
        "bubblewrap cannot confine a process here (`bwrap` could not run: No such file or directory (os error 2))",
    ] {
        let e = strict
            .sandbox_probed(&|_| Err(why.to_owned()))
            .unwrap_err()
            .0;
        assert!(
            e.contains("install bubblewrap and allow user namespaces"),
            "{e}"
        );
        assert!(!e.contains("loader"), "{e}");
    }
}

#[test]
fn the_real_probe_decides_the_real_start_and_an_injected_answer_refuses_on_any_host() {
    // The proof that the refusal test above is not the host's doing, in both halves, and neither half is skipped:
    // where the real bubblewrap confines, the real probe lets the start through to the confined policy; where it does
    // not, the real start is refused and carries bubblewrap's own reason (M42). On either host the injected answer
    // still refuses the start.
    let probe = inklkunn_prikk::sandbox::probe_bwrap(std::ffi::OsStr::new("bwrap"));
    let d = dir("realprobe");
    let s = settings(&d.0, false);
    match &probe {
        Ok(()) => assert!(matches!(s.sandbox(), Ok(Sandbox::Bwrap(_)))),
        Err(why) => {
            assert!(!why.is_empty(), "bubblewrap's reason is empty");
            let refused = s.sandbox().unwrap_err();
            assert!(
                refused.0.contains("bubblewrap cannot confine") && refused.0.contains(why),
                "the refusal does not carry bubblewrap's reason `{why}`: {refused}"
            );
            assert!(refused.0.contains("--dev-unconfined"), "{refused}");
        }
    }
    let e = prepare_probing(
        &s,
        &web("127.0.0.1:1", "http://localhost:1", "http://localhost:2"),
        IdentityAuthority::for_tests(),
        Arc::new(|_| {}),
        &|_| Err(CANNOT_CONFINE.to_owned()),
    )
    .err()
    .unwrap();
    assert!(e.contains("bubblewrap cannot confine"), "{e}");
}

#[test]
fn a_missing_prikk_is_named() {
    let d = dir("noprikk");
    let mut s = settings(&d.0, true);
    s.prikk = OsString::from("/nonexistent/prikk");
    let e = prepare(
        &s,
        &web("127.0.0.1:1", "http://localhost:1", "http://localhost:2"),
        IdentityAuthority::for_tests(),
    )
    .err()
    .unwrap();
    assert!(
        e.contains("prikk is not usable") && e.contains("0.54.0"),
        "{e}"
    );
}

fn request(port: u16, host: &str, path: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

#[test]
fn a_started_server_answers_over_a_socket_with_its_headers_and_stops_when_told() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let d = dir("socket");
    let (app, content) = super::test_ports::two_free_ports();
    let s = settings(&d.0, true);
    let w = web(
        &format!("127.0.0.1:{app}"),
        &format!("http://localhost:{app}"),
        &format!("http://localhost:{content}"),
    );
    let prepared = prepare(&s, &w, IdentityAuthority::for_tests()).unwrap();
    assert!(
        prepared.banner.iter().any(|l| l.contains("UNCONFINED")),
        "{:?}",
        prepared.banner
    );
    assert!(
        d.0.join("store.sqlite").is_file(),
        "the store was opened (and made) at start"
    );
    let (shutdown, stop) = Shutdown::manual();
    let (tx, rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let mut err = Vec::new();
        let result = run(prepared, shutdown, &mut err, move |addr| {
            tx.send(addr).unwrap();
        });
        (result, String::from_utf8(err).unwrap())
    });
    let addr = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the server bound");
    assert_eq!(addr.port(), app);

    let health = request(app, &format!("localhost:{app}"), "/-/health");
    assert!(health.starts_with("HTTP/1.1 200"), "{health}");
    assert!(
        health
            .to_ascii_lowercase()
            .contains("content-security-policy: default-src 'none'"),
        "{health}"
    );
    assert!(health.ends_with("ok\n"), "{health}");
    let misdirected = request(app, "evil.example", "/-/health");
    assert!(misdirected.starts_with("HTTP/1.1 421"), "{misdirected}");
    let missing = request(app, &format!("localhost:{app}"), "/handbook/Home");
    assert!(missing.starts_with("HTTP/1.1 404"), "{missing}");
    // Sign-in is served (with the double-submit cookie and its token), joining is not open by default, and a
    // visitor who is not signed in has no account page.
    let sign_in = request(app, &format!("localhost:{app}"), "/-/sign-in");
    assert!(sign_in.starts_with("HTTP/1.1 200"), "{sign_in}");
    assert!(
        sign_in
            .to_ascii_lowercase()
            .contains("set-cookie: __host-inklkunn-csrf="),
        "{sign_in}"
    );
    assert!(
        sign_in.contains("Ask the person who runs this wiki for an invitation."),
        "{sign_in}"
    );
    assert!(!sign_in.contains("Join this wiki"), "{sign_in}");
    let join = request(app, &format!("localhost:{app}"), "/-/join");
    assert!(join.starts_with("HTTP/1.1 404"), "{join}");
    let account = request(app, &format!("localhost:{app}"), "/-/account");
    assert!(account.starts_with("HTTP/1.1 303"), "{account}");
    assert!(
        account.contains("location: /-/sign-in?next=/-/account"),
        "{account}"
    );

    stop.stop();
    let (result, banner) = server.join().unwrap();
    assert!(result.is_ok(), "{result:?}");
    assert!(
        banner.contains("serving http://localhost:") && banner.contains("prikk 0.5"),
        "{banner}"
    );
}

fn bind(text: &str) -> std::net::SocketAddr {
    text.parse().unwrap()
}

#[test]
fn no_declared_proxy_is_warned_about_for_any_bind_and_declaring_one_ends_it() {
    for reachable in [
        "0.0.0.0:8080",
        "[::]:8080",
        "192.0.2.10:8080",
        "[2001:db8::1]:8080",
    ] {
        let w = proxy_warning(bind(reachable), false).expect(reachable);
        assert!(
            w.starts_with("warning: ") && !w.contains('\n'),
            "one line: {w}"
        );
        assert!(
            w.contains("INKLKUNN_TRUSTED_PROXIES"),
            "names the setting: {w}"
        );
        assert!(w.contains(reachable), "names the address: {w}");
        assert!(w.contains("lock everyone out"), "says what happens: {w}");
    }
    // G1: the default bind is a loopback one and the likeliest deployment is a TLS proxy on the same host.
    for loopback in ["127.0.0.1:8080", "[::1]:8080", "127.0.0.2:1"] {
        let w = proxy_warning(bind(loopback), false).expect(loopback);
        assert!(
            w.starts_with("warning: ") && !w.contains('\n'),
            "one line: {w}"
        );
        assert!(
            w.contains("INKLKUNN_TRUSTED_PROXIES")
                && w.contains(loopback)
                && w.contains("lock everyone out"),
            "{w}"
        );
    }
    // Declaring a trusted proxy ends the warning.
    for any in ["0.0.0.0:8080", "127.0.0.1:8080"] {
        assert_eq!(proxy_warning(bind(any), true), None, "{any}");
    }
}

#[test]
fn the_start_notes_put_the_banner_first_and_the_warning_after_it_whatever_the_bind() {
    let notes = |addr: &str| {
        let web = WebConfig::new(addr, "https://wiki.example", "https://content.example").unwrap();
        let mut err = Vec::new();
        write_start_notes(&mut err, &["the banner".to_owned()], &[], &web, false);
        String::from_utf8(err).unwrap()
    };
    let open = notes("0.0.0.0:8080");
    let lines: Vec<&str> = open.lines().collect();
    assert_eq!(lines.first(), Some(&"the banner"));
    assert_eq!(lines.len(), 2, "{open}");
    assert!(lines.get(1).unwrap().starts_with("warning: "), "{open}");
    let loopback = notes("127.0.0.1:8080");
    let lines: Vec<&str> = loopback.lines().collect();
    assert_eq!(lines.len(), 2, "{loopback}");
    assert!(lines.get(1).unwrap().starts_with("warning: "), "{loopback}");
}

#[test]
fn a_declared_trusted_proxy_ends_the_warning_and_a_typo_stops_the_start() {
    let web = WebConfig::new(
        "0.0.0.0:8080",
        "https://wiki.example",
        "https://content.example",
    )
    .unwrap();
    let mut err = Vec::new();
    write_start_notes(&mut err, &["the banner".to_owned()], &[], &web, true);
    assert_eq!(String::from_utf8(err).unwrap(), "the banner\n");

    let d = dir("proxies");
    let mut w = web_settings();
    w.trusted_proxies = vec!["10.0.0.0/8".into(), "not a network".into()];
    let e = prepare(&settings(&d.0, true), &w, IdentityAuthority::for_tests())
        .err()
        .unwrap();
    assert!(
        e.contains("INKLKUNN_TRUSTED_PROXIES") && e.contains("not a network"),
        "{e}"
    );
    assert!(!d.0.join("store.sqlite").exists(), "nothing was created");
}

fn web_settings() -> WebSettings {
    web("127.0.0.1:1", "http://localhost:1", "http://localhost:2")
}

#[test]
fn an_open_door_is_said_at_every_start_and_a_closed_one_needs_no_word() {
    use inklkunn_auth::JoinPolicy;
    assert_eq!(join_note(JoinPolicy::Off), None);
    let anyone = join_note(JoinPolicy::Anyone).unwrap();
    assert!(
        anyone.contains("INKLKUNN_JOIN=anyone")
            && anyone.contains("anyone who can reach this server"),
        "{anyone}"
    );
    let provider = join_note(JoinPolicy::Provider).unwrap();
    assert!(provider.contains("open registration"), "{provider}");
}
