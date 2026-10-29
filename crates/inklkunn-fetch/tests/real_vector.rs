//! The real vector: bubblewrap and curl as installed. Each test self-skips, visibly, when `bwrap` cannot
//! confine a process here or `curl` is not found: it prints `skipping: bwrap or curl not available` (a
//! message of its own: CI's count of `skipping: prikk not on PATH` lines is for prikk-dependent tests only).
//!
//! What is shown, against a local listener and a closed port (an `https` URL is the only kind accepted, so
//! the "server" is a socket that accepts and hangs up, which is enough to tell *connected* from *not*):
//! - the vector reaches the host's loopback (the network is shared) and connects to **the pinned address
//!   for a name that resolves nowhere** (`--resolve` is in force, and the vector has no resolver);
//! - a closed port is a typed connect failure;
//! - **inside the vector** the repository, the key directory, the home directory and a marker file in the
//!   host's `/tmp` do not exist, the system is read-only, and the environment holds no `HOME` and no proxy
//!   variable.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::net::TcpListener;
use std::sync::mpsc;
use std::time::Duration;

use inklkunn_fetch::{CurlError, CurlFetcher};
use inklkunn_prikk::sandbox::{BWRAP, probe_bwrap};
use support::*;

/// `true` when the real vector can run; otherwise prints the skip line.
fn available() -> bool {
    let curl = inklkunn_prikk::sandbox::resolve_binary(std::ffi::OsStr::new("curl"));
    if probe_bwrap(std::ffi::OsStr::new(BWRAP)).is_ok()
        && curl.is_file()
        && CurlFetcher::new().probe().is_ok()
    {
        return true;
    }
    println!("skipping: bwrap or curl not available");
    false
}

#[test]
fn a_closed_port_is_a_typed_connect_failure_and_the_error_is_clean() {
    if !available() {
        return;
    }
    let started = std::time::Instant::now();
    let e = CurlFetcher::new()
        .fetch(&request("127.0.0.1", 1, "127.0.0.1", true))
        .unwrap_err();
    assert!(matches!(e, CurlError::Connect(_)), "{e:?}");
    assert_no_secret(&e);
    assert!(started.elapsed() < Duration::from_secs(20));
}

#[test]
fn the_pinned_address_is_connected_for_a_name_that_resolves_nowhere() {
    if !available() {
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    let accepter = std::thread::spawn(move || {
        // Accept one connection, then hang up: curl's TLS handshake fails, which proves it got here.
        if let Ok((stream, _)) = listener.accept() {
            let _ = tx.send(());
            drop(stream);
        }
    });
    // `.invalid` never resolves (RFC 6761); with no resolver in the vector, only `--resolve` can get curl
    // to 127.0.0.1.
    let e = CurlFetcher::new()
        .fetch(&request("pinned.invalid", port, "127.0.0.1", true))
        .unwrap_err();
    assert!(
        rx.recv_timeout(Duration::from_secs(5)).is_ok(),
        "the listener on the host's loopback saw no connection; the network is not shared or the pin is not used ({e:?})"
    );
    assert!(matches!(e, CurlError::Tls { .. }), "{e:?}");
    assert_no_secret(&e);
    accepter.join().expect("accepter");
}

#[test]
fn inside_the_vector_there_is_no_repository_no_key_no_home_and_a_read_only_system() {
    if !available() {
        return;
    }
    // A host with a "repository", a "key directory" and a marker in /tmp, none of which is mounted.
    let host = Dir::new("host");
    let repo = host.path("wiki-repository");
    let keys = host.path("key-directory");
    std::fs::create_dir_all(repo.join(".prikk")).unwrap();
    std::fs::create_dir_all(&keys).unwrap();
    std::fs::write(keys.join("seed"), "not a real seed").unwrap();
    let marker = std::env::temp_dir().join(format!("inklkunn-fetch-marker-{}", std::process::id()));
    std::fs::write(&marker, "host /tmp").unwrap();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/nonexistent-home".to_owned());

    let probe = Dir::new("probe");
    let rec = probe.0.display().to_string();
    let (repo_p, seed_p, marker_p) = (
        repo.display().to_string(),
        keys.join("seed").display().to_string(),
        marker.display().to_string(),
    );
    // The stand-in "curl" answers with a report of what it can see, as the body of a 200.
    let curl = probe.script(
        "curl",
        &format!(
            r#"
see() {{ if [ -e "$2" ]; then echo "$1=present"; else echo "$1=absent"; fi; }}
see repo '{repo_p}'
see repo_inside '{repo_p}/.prikk'
see seed '{seed_p}'
see marker '{marker_p}'
see home '{home}'
see etc_passwd_dir '/etc/passwd'
if ( : > /usr/inklkunn-fetch-probe ) 2>/dev/null; then echo usr=writable; else echo usr=readonly; fi
if ( : > /tmp/inklkunn-fetch-probe ) 2>/dev/null; then echo tmp=writable; else echo tmp=readonly; fi
echo "env=$(env | grep -v '^PWD=\|^SHLVL=\|^_=\|^OLDPWD=' | sort | tr '\n' ' ')"
echo "pwd=$(pwd)"
echo "stdin_auth=$(grep -c 'Authorization: ' '{rec}/curl.stdin')"
echo "argv_secret=$(grep -c 'c2VjcmV0\|SECRETCODE' '{rec}/curl.argv')"
printf '\n200'
"#
        ),
    );
    let ca = host.raw_script("ca.pem", "x", 0o644);
    let fetcher = CurlFetcher::new()
        .with_programs(BWRAP, &curl)
        .with_ca_bundle(&ca);
    let body =
        fetch(&fetcher, &request("203.0.113.7", 443, "203.0.113.7", true)).expect("the probe ran");
    let report = String::from_utf8(body).unwrap();
    // (A home directory under /tmp or /usr would exist in the vector for reasons of its own.)
    let home_testable = !["/tmp", "/usr", "/"]
        .iter()
        .any(|p| home == *p || home.starts_with(&format!("{p}/")));
    for gone in [
        "repo",
        "repo_inside",
        "seed",
        "marker",
        "home",
        "etc_passwd_dir",
    ] {
        if gone == "home" && !home_testable {
            continue;
        }
        assert!(
            report.contains(&format!("{gone}=absent")),
            "{gone} visible inside the vector:\n{report}"
        );
    }
    assert!(report.contains("usr=readonly"), "{report}");
    assert!(report.contains("tmp=writable"), "a fresh tmpfs: {report}");
    assert!(report.contains("pwd=/tmp"), "{report}");
    let env_line = report.lines().find(|l| l.starts_with("env=")).unwrap_or("");
    assert!(env_line.contains("PATH=/usr/bin"), "{env_line}");
    for var in [
        "HOME=",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "CURL_",
        "SSL_CERT",
        "USER=",
        "XDG_",
    ] {
        assert!(!env_line.contains(var), "{var} in {env_line}");
    }
    // The secrets reached the stand-in on standard input and in no argument.
    assert!(report.contains("stdin_auth=1"), "{report}");
    assert!(report.contains("argv_secret=0"), "{report}");
    // The marker is still on the host: the vector's /tmp is its own.
    assert!(marker.exists());
    let _ = std::fs::remove_file(&marker);
}
