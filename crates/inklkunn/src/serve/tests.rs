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
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
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
        let e = prepare(&s, &w).err().unwrap();
        assert!(e.contains(why), "{why}: {e}");
    }
    assert!(
        !d.0.join("store.sqlite").exists(),
        "nothing was created by a refused start"
    );
}

#[test]
fn without_the_dev_flag_a_host_that_cannot_confine_prikk_refuses_to_start() {
    if inklkunn_prikk::sandbox::probe_bwrap(std::ffi::OsStr::new("bwrap")).is_ok() {
        println!("skipping: bubblewrap works here, so the refusal cannot be shown");
        return;
    }
    let d = dir("unconfined");
    let e = prepare(
        &settings(&d.0, false),
        &web("127.0.0.1:1", "http://localhost:1", "http://localhost:2"),
    )
    .err()
    .unwrap();
    assert!(
        e.contains("bubblewrap cannot confine prikk") && e.contains("--dev-unconfined"),
        "{e}"
    );
}

#[test]
fn a_missing_prikk_is_named() {
    let d = dir("noprikk");
    let mut s = settings(&d.0, true);
    s.prikk = OsString::from("/nonexistent/prikk");
    let e = prepare(
        &s,
        &web("127.0.0.1:1", "http://localhost:1", "http://localhost:2"),
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
    let (app, content) = (free_port(), free_port());
    let s = settings(&d.0, true);
    let w = web(
        &format!("127.0.0.1:{app}"),
        &format!("http://localhost:{app}"),
        &format!("http://localhost:{content}"),
    );
    let prepared = prepare(&s, &w).unwrap();
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
fn an_address_anybody_can_reach_without_a_declared_proxy_is_warned_about_and_a_loopback_one_is_not()
{
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
    for loopback in ["127.0.0.1:8080", "[::1]:8080", "127.0.0.2:1"] {
        assert_eq!(proxy_warning(bind(loopback), false), None, "{loopback}");
    }
    // Declaring a trusted proxy ends the warning.
    assert_eq!(proxy_warning(bind("0.0.0.0:8080"), true), None);
}

#[test]
fn the_start_notes_put_the_banner_first_and_the_warning_on_stderr_only_for_a_reachable_bind() {
    let notes = |addr: &str| {
        let web = WebConfig::new(addr, "https://wiki.example", "https://content.example").unwrap();
        let mut err = Vec::new();
        write_start_notes(&mut err, &["the banner".to_owned()], &web, false);
        String::from_utf8(err).unwrap()
    };
    let open = notes("0.0.0.0:8080");
    let lines: Vec<&str> = open.lines().collect();
    assert_eq!(lines.first(), Some(&"the banner"));
    assert_eq!(lines.len(), 2, "{open}");
    assert!(lines.get(1).unwrap().starts_with("warning: "), "{open}");
    assert_eq!(notes("127.0.0.1:8080"), "the banner\n");
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
    write_start_notes(&mut err, &["the banner".to_owned()], &web, true);
    assert_eq!(String::from_utf8(err).unwrap(), "the banner\n");

    let d = dir("proxies");
    let mut w = web_settings();
    w.trusted_proxies = vec!["10.0.0.0/8".into(), "not a network".into()];
    let e = prepare(&settings(&d.0, true), &w).err().unwrap();
    assert!(
        e.contains("INKLKUNN_TRUSTED_PROXIES") && e.contains("not a network"),
        "{e}"
    );
    assert!(!d.0.join("store.sqlite").exists(), "nothing was created");
}

fn web_settings() -> WebSettings {
    web("127.0.0.1:1", "http://localhost:1", "http://localhost:2")
}
