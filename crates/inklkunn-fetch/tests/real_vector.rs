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
//!   variable;
//! - **the ceiling** (handoff 002's F3, handoff 010 M2) reaches the vector through the one shared `prlimit` wrapper:
//!   the real `curl` is the command the wrapper runs, with exactly the configured ceiling, and a stand-in that
//!   allocates past a small ceiling is stopped by it. No test assumes how much memory a build of `curl` needs.
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

// ---- the memory ceiling (handoff 002's F3; handoff 010 M2) ----
//
// What these tests hold is that the configured ceiling reaches the vector through the one shared `prlimit`
// wrapper. None assumes how much memory any build of `curl` needs to start (that is the host's, and it changes
// from release to release): the real `curl` is shown to be the command the wrapper runs, with exactly the
// ceiling; a stand-in that allocates past a small ceiling is shown to be stopped by it, in the real
// bubblewrap.

/// A `prlimit` that records its arguments, one per line, in `rec`, then runs the host's real one with them.
fn recording_prlimit(d: &Dir) -> (std::path::PathBuf, std::path::PathBuf) {
    let real = inklkunn_prikk::sandbox::resolve_binary(std::ffi::OsStr::new("prlimit"));
    let rec = d.path("prlimit.args");
    let script = d.raw_script(
        "prlimit",
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nexec '{}' \"$@\"\n",
            rec.display(),
            real.display()
        ),
        0o755,
    );
    (script, rec)
}

#[test]
fn the_real_vector_runs_under_the_configured_ceiling() {
    if !available() {
        return;
    }
    // Inside bubblewrap, what a child of the vector sees is the ceiling `prlimit` set in front of `bwrap`.
    let d = Dir::new("real-ceiling");
    let curl = d.script(
        "curl",
        r#"awk '/Max data size/ {printf "%s", $4}' /proc/self/limits; printf '\n200'"#,
    );
    let ca = d.raw_script("ca.pem", "x", 0o644);
    for bytes in [300u64 << 20, 1 << 30] {
        let fetcher = CurlFetcher::new()
            .with_programs(BWRAP, &curl)
            .with_ca_bundle(&ca)
            .with_data_ceiling(bytes);
        let body =
            fetch(&fetcher, &request("203.0.113.7", 443, "203.0.113.7", false)).expect("ran");
        assert_eq!(String::from_utf8(body).unwrap(), bytes.to_string());
    }
}

#[test]
fn the_real_curl_is_the_command_the_wrapper_runs_with_exactly_the_configured_ceiling() {
    if !available() {
        return;
    }
    let curl = inklkunn_prikk::sandbox::resolve_binary(std::ffi::OsStr::new("curl"));
    let curl_arg = curl.display().to_string();
    for bytes in [None, Some(123u64 << 20)] {
        let d = Dir::new("real-wrapper");
        let (prlimit, rec) = recording_prlimit(&d);
        let mut fetcher = CurlFetcher::new().with_prlimit(&prlimit);
        if let Some(bytes) = bytes {
            fetcher = fetcher.with_data_ceiling(bytes);
        }
        let ceiling = fetcher.data_ceiling();
        assert_eq!(
            ceiling,
            bytes.unwrap_or(inklkunn_fetch::DEFAULT_DATA_CEILING)
        );
        // The real curl, against a closed port: it is a typed connect failure, whatever this build of curl
        // needs to start (that it connects is not what is held here).
        let _ = fetcher.fetch(&request("127.0.0.1", 1, "127.0.0.1", true));
        let args = d.read("prlimit.args");
        let args: Vec<&str> = args.lines().collect();
        assert_eq!(
            args.first().copied(),
            Some(format!("--data={ceiling}").as_str()),
            "{args:?}"
        );
        assert_eq!(args.get(1).copied(), Some("--"), "{args:?}");
        assert_eq!(
            args.get(2).copied(),
            Some(BWRAP),
            "the wrapper runs bwrap: {args:?}"
        );
        assert!(rec.exists());
        // bwrap's own command line ends with `-- <the real curl> <its flags>`; the ceiling is nowhere else.
        let sep = args.iter().skip(2).position(|a| *a == "--").map(|i| i + 2);
        assert_eq!(
            sep.and_then(|i| args.get(i + 1)).copied(),
            Some(curl_arg.as_str()),
            "{args:?}"
        );
        assert_eq!(
            args.iter().filter(|a| a.starts_with("--data=")).count(),
            1,
            "{args:?}"
        );
    }
}

#[test]
fn a_stand_in_that_allocates_past_a_small_ceiling_is_stopped_by_it_in_the_real_vector() {
    if !available() {
        return;
    }
    let d = Dir::new("real-stopped");
    let r = request("203.0.113.7", 443, "203.0.113.7", true);
    // Holds `$1`-many bytes in one shell variable and says so. No `curl` is involved: the stand-in's needs are
    // the same on every host.
    let holds = |name: &str, bytes: u64| {
        d.script(
            name,
            &format!(
                r#"v=$(head -c {bytes} /dev/zero | tr '\0' a) || exit 70; test "${{#v}}" -eq {bytes} || exit 71; printf 'held'; printf '\n200'"#
            ),
        )
    };
    let small = holds("curl-small", 2 << 20);
    let large = holds("curl-large", 96 << 20);
    let ca = d.raw_script("ca.pem", "x", 0o644);
    let fetcher = |curl: &std::path::Path, bytes: u64| {
        CurlFetcher::new()
            .with_programs(BWRAP, curl)
            .with_ca_bundle(&ca)
            .with_data_ceiling(bytes)
    };
    // Under a small ceiling the vector itself starts and a small allocation fits (so what follows is not a
    // vector that cannot start) ...
    let ok =
        fetch(&fetcher(&small, 48 << 20), &r).expect("the vector starts under a small ceiling");
    assert_eq!(ok, b"held");
    // ... a large one does not, and with room enough the same stand-in holds it ...
    let ok = fetch(&fetcher(&large, 2 << 30), &r).expect("room enough");
    assert_eq!(ok, b"held");
    let stopped = fetch(&fetcher(&large, 48 << 20), &r).unwrap_err();
    assert!(
        matches!(
            stopped,
            CurlError::Curl { .. } | CurlError::Killed(_) | CurlError::Spawn(_)
        ),
        "{stopped:?}"
    );
    assert_no_secret(&stopped);
}
