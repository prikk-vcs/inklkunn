//! Every failure shape of a fetch, with a **stand-in `curl`** (a shell script that records what it was
//! given and answers on cue), the way `inklkunn-signer`'s tests use a stand-in prikk. The stand-in runs
//! unconfined (the crate's `test-knobs` mode) except where a test says it stands in for `bwrap` as well; none
//! of these tests needs bubblewrap, curl or a network, so none can self-skip.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use std::time::{Duration, Instant};

use inklkunn_fetch::{CurlError, CurlFetcher, FetchLimits};
use support::*;

fn fetcher(curl: &std::path::Path) -> CurlFetcher {
    CurlFetcher::new().with_programs("bwrap", curl).unconfined()
}

fn quick() -> FetchLimits {
    FetchLimits {
        max_time: Duration::from_secs(1),
        connect_timeout: Duration::from_secs(1),
        max_bytes: 100,
        kill_slack: Duration::from_millis(300),
    }
}

#[test]
fn a_200_gives_the_body_and_curl_was_told_everything_it_must_be() {
    let d = Dir::new("ok");
    let curl = d.script("curl", r#"printf '{"ok":true}'; printf '\n200'"#);
    let body = fetch(
        &fetcher(&curl),
        &request("idp.example.org", 443, "203.0.113.7", true),
    )
    .expect("a 200");
    assert_eq!(body, br#"{"ok":true}"#);

    let argv = d.read("curl.argv");
    let lines: Vec<&str> = argv.lines().collect();
    let after = |flag: &str| {
        lines
            .iter()
            .position(|l| *l == flag)
            .and_then(|i| lines.get(i + 1))
            .copied()
    };
    assert_eq!(lines.first(), Some(&"-q"));
    assert_eq!(after("--proto"), Some("=https"));
    assert_eq!(after("--proto-redir"), Some("=https"));
    assert_eq!(after("--max-redirs"), Some("0"));
    assert_eq!(after("--resolve"), Some("idp.example.org:443:203.0.113.7"));
    assert_eq!(after("--url"), Some("https://idp.example.org/token"));
    assert_eq!(after("--noproxy"), Some("*"));
    assert!(after("--max-time").is_some() && after("--max-filesize").is_some());

    // The secrets reached curl on standard input, and in no argument.
    for piece in SECRET_PIECES {
        assert!(!argv.contains(piece), "{piece} in argv:\n{argv}");
    }
    let stdin = d.read("curl.stdin");
    assert!(stdin.contains(&format!("header = \"Authorization: {SECRET_AUTH}\"")));
    assert!(stdin.contains(&format!("data-raw = \"{SECRET_FORM}\"")));
}

#[test]
fn a_get_sends_no_configuration() {
    let d = Dir::new("get");
    let curl = d.script("curl", r#"printf 'x\n200'"#);
    fetch(
        &fetcher(&curl),
        &request("idp.example.org", 443, "203.0.113.7", false),
    )
    .expect("a 200");
    assert_eq!(d.read("curl.stdin"), "");
}

#[test]
fn a_status_other_than_200_is_typed() {
    let d = Dir::new("status");
    let curl = d.script("curl", r#"printf 'not here\n404'"#);
    let e = fetch(
        &fetcher(&curl),
        &request("idp.example.org", 443, "203.0.113.7", true),
    )
    .unwrap_err();
    assert_eq!(e, CurlError::Status(404));
    let curl = d.script("curl2", r#"printf '\n503'"#);
    assert_eq!(
        fetch(
            &fetcher(&curl),
            &request("idp.example.org", 443, "203.0.113.7", true)
        )
        .unwrap_err(),
        CurlError::Status(503)
    );
}

#[test]
fn a_redirect_is_an_error_and_is_never_followed() {
    let d = Dir::new("redirect");
    for code in [301, 302, 307] {
        let curl = d.script(
            "curl",
            &format!(r#"printf '<a href="https://evil.example.net/">moved</a>\n{code}'"#),
        );
        let e = fetch(
            &fetcher(&curl),
            &request("idp.example.org", 443, "203.0.113.7", true),
        )
        .unwrap_err();
        assert_eq!(e, CurlError::Redirect(code));
        // curl was told not to follow one, and the crate would not hand the body of a 3xx on.
        assert!(d.read("curl.argv").contains("--max-redirs\n0\n"));
    }
}

#[test]
fn a_child_that_outlives_the_deadline_is_killed_and_reported_as_a_timeout() {
    let d = Dir::new("timeout");
    let curl = d.script("curl", "exec sleep 30");
    let started = Instant::now();
    let e = fetch(
        &fetcher(&curl).with_limits(quick()),
        &request("idp.example.org", 443, "203.0.113.7", true),
    )
    .unwrap_err();
    assert_eq!(e, CurlError::TimedOut);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn curls_own_timeout_and_size_exits_are_typed() {
    let d = Dir::new("exits");
    let curl = d.script("curl", "echo 'curl: (28) Operation timed out' >&2; exit 28");
    let r = request("idp.example.org", 443, "203.0.113.7", true);
    assert_eq!(fetch(&fetcher(&curl), &r).unwrap_err(), CurlError::TimedOut);
    let curl = d.script(
        "curl",
        "echo 'curl: (63) Maximum file size exceeded' >&2; exit 63",
    );
    assert_eq!(fetch(&fetcher(&curl), &r).unwrap_err(), CurlError::TooLarge);
}

#[test]
fn a_body_over_the_cap_is_too_large_and_an_endless_one_is_cut_at_the_cap() {
    let d = Dir::new("oversize");
    let r = request("idp.example.org", 443, "203.0.113.7", true);
    // Finite, longer than the cap.
    let curl = d.script(
        "curl",
        "head -c 5000 /dev/zero | tr '\\0' a; printf '\\n200'",
    );
    assert_eq!(
        fetch(&fetcher(&curl).with_limits(quick()), &r).unwrap_err(),
        CurlError::TooLarge
    );
    // Endless: only the Rust-side read cap stops this one before the deadline (curl is not trusted to
    // enforce `--max-filesize` on a body of unknown length).
    let curl = d.script("curl", "exec yes");
    let started = Instant::now();
    let limits = FetchLimits {
        max_time: Duration::from_secs(5),
        ..quick()
    };
    assert_eq!(
        fetch(&fetcher(&curl).with_limits(limits), &r).unwrap_err(),
        CurlError::TooLarge
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the cap, not the deadline, ended it: {:?}",
        started.elapsed()
    );
    // At the cap exactly is fine.
    let curl = d.script(
        "curl",
        "head -c 100 /dev/zero | tr '\\0' a; printf '\\n200'",
    );
    let body = fetch(&fetcher(&curl).with_limits(quick()), &r).expect("exactly at the cap");
    assert_eq!(body.len(), 100);
}

#[test]
fn a_program_that_cannot_be_started_is_a_typed_error() {
    let d = Dir::new("spawn");
    let r = request("idp.example.org", 443, "203.0.113.7", true);
    // Present but not executable.
    let curl = d.raw_script("curl", "#!/bin/sh\nexit 0\n", 0o644);
    let e = fetcher(&curl).fetch(&r).unwrap_err();
    assert!(matches!(e, CurlError::Spawn(_)), "{e:?}");
    assert_no_secret(&e);
    // bwrap missing, in the confined mode: the vector cannot be built.
    let ca = d.raw_script("ca.pem", "x", 0o644);
    let e = CurlFetcher::new()
        .with_programs("/nonexistent/dir/bwrap", d.script("curl3", "exit 0"))
        .with_ca_bundle(&ca)
        .fetch(&r)
        .unwrap_err();
    assert!(matches!(e, CurlError::SandboxUnavailable(_)), "{e:?}");
    assert!(e.to_string().contains("/nonexistent/dir/bwrap"));
}

#[test]
fn a_missing_curl_or_ca_bundle_is_found_before_anything_runs() {
    let r = request("idp.example.org", 443, "203.0.113.7", true);
    let e = CurlFetcher::new()
        .with_programs("bwrap", "/nonexistent/dir/curl")
        .unconfined()
        .fetch(&r)
        .unwrap_err();
    assert_eq!(
        e,
        CurlError::CurlMissing("/nonexistent/dir/curl".to_owned())
    );
    let d = Dir::new("noca");
    let curl = d.script("curl", "exit 0");
    let e = CurlFetcher::new()
        .with_programs("bwrap", &curl)
        .with_ca_bundle("/nonexistent/ca.pem")
        .fetch(&r)
        .unwrap_err();
    assert_eq!(e, CurlError::NoCaBundle);
}

#[test]
fn a_signal_death_and_a_garbled_answer_are_typed() {
    let d = Dir::new("shapes");
    let r = request("idp.example.org", 443, "203.0.113.7", true);
    let curl = d.script("curl", "kill -9 $$");
    assert!(matches!(
        fetch(&fetcher(&curl), &r).unwrap_err(),
        CurlError::Killed(m) if m.contains('9')
    ));
    let curl = d.script("curl", "printf 'no trailer here'");
    assert!(matches!(
        fetch(&fetcher(&curl), &r).unwrap_err(),
        CurlError::Malformed(_)
    ));
}

#[test]
fn no_error_carries_a_secret_even_when_curl_prints_one() {
    let d = Dir::new("leak");
    let r = request("idp.example.org", 443, "203.0.113.7", true);
    // A misbehaving curl that echoes its configuration (the header and the form) into standard error,
    // for each exit shape that carries a detail.
    for code in [7, 35, 22, 1] {
        let curl = d.script(
            "curl",
            &format!(
                "echo \"curl: ($(cat '{}/curl.stdin' | tr '\\n' ' '))\" >&2; exit {code}",
                d.0.display()
            ),
        );
        let e = fetch(&fetcher(&curl), &r).unwrap_err();
        assert_no_secret(&e);
        let text = e.to_string();
        assert!(text.contains("[redacted]"), "{text}");
        assert!(text.chars().count() < 400, "{text}");
    }
    // And the request itself, shown.
    assert!(!format!("{r:?}").contains("SECRETCODE"));
    assert!(!format!("{r:?}").contains("c2VjcmV0"));
}

#[test]
fn a_refused_request_never_runs_curl_and_never_repeats_the_secret() {
    let d = Dir::new("refused");
    let curl = d.script("curl", "printf 'x\\n200'");
    let mut r = request("idp.example.org", 443, "203.0.113.7", true);
    r.authorization = Some("Bearer x\nurl = \"https://evil.example.net/\"".to_owned());
    let e = fetcher(&curl).fetch(&r).unwrap_err();
    assert!(matches!(e, CurlError::InvalidRequest(_)), "{e:?}");
    assert!(!e.to_string().contains("evil"));
    assert!(
        !d.path("curl.argv").exists(),
        "curl must not have been started"
    );
    // A URL that is not the pinned host's.
    let mut r = request("idp.example.org", 443, "203.0.113.7", false);
    r.target.url = "https://evil.example.net/token".to_owned();
    assert!(matches!(
        fetcher(&curl).fetch(&r).unwrap_err(),
        CurlError::InvalidRequest(_)
    ));
    assert!(!d.path("curl.argv").exists());
}

#[test]
fn the_confined_path_hands_bwrap_the_whole_vector_and_the_configuration_on_stdin() {
    // A stand-in `bwrap`: records its arguments and stdin, then answers as the curl inside would.
    let d = Dir::new("bwrap");
    let bwrap = d.script("bwrap", r#"printf '{"v":1}\n200'"#);
    let curl = d.script("curl", "exit 99");
    let ca = d.raw_script("ca.pem", "x", 0o644);
    let f = CurlFetcher::new()
        .with_programs(&bwrap, &curl)
        .with_ca_bundle(&ca);
    let body = fetch(&f, &request("idp.example.org", 443, "203.0.113.7", true)).expect("answers");
    assert_eq!(body, br#"{"v":1}"#);
    let argv = d.read("bwrap.argv");
    let lines: Vec<&str> = argv.lines().collect();
    for flag in [
        "--unshare-all",
        "--share-net",
        "--die-with-parent",
        "--clearenv",
        "--tmpfs",
    ] {
        assert!(lines.contains(&flag), "{flag} missing:\n{argv}");
    }
    let sep = lines.iter().position(|l| *l == "--").expect("separator");
    assert_eq!(
        lines
            .get(sep + 1)
            .map(|p| std::path::Path::new(p).file_name()),
        Some(Some(std::ffi::OsStr::new("curl"))),
        "curl is what runs inside:\n{argv}"
    );
    assert!(lines.contains(&"--proto") && lines.contains(&"=https"));
    assert!(lines.contains(&"/etc/ssl/certs/ca-certificates.crt"));
    for piece in SECRET_PIECES {
        assert!(!argv.contains(piece), "{piece} in bwrap's argv");
    }
    assert!(d.read("bwrap.stdin").contains(SECRET_AUTH));
}

#[test]
fn bubblewraps_own_refusal_is_a_sandbox_error_whatever_its_exit_code() {
    let d = Dir::new("bwrapfail");
    let bwrap = d.script(
        "bwrap",
        "echo 'bwrap: No permissions to creating new namespace' >&2; exit 1",
    );
    let curl = d.script("curl", "exit 99");
    let ca = d.raw_script("ca.pem", "x", 0o644);
    let e = fetch(
        &CurlFetcher::new()
            .with_programs(&bwrap, &curl)
            .with_ca_bundle(&ca),
        &request("idp.example.org", 443, "203.0.113.7", true),
    )
    .unwrap_err();
    assert!(
        matches!(&e, CurlError::SandboxUnavailable(m) if m.contains("namespace")),
        "{e:?}"
    );
}

#[test]
fn the_probe_reports_a_sandbox_that_cannot_confine() {
    let d = Dir::new("probe");
    let curl = d.script("curl", "exit 0");
    let ca = d.raw_script("ca.pem", "x", 0o644);
    let e = CurlFetcher::new()
        .with_programs("/nonexistent/dir/bwrap", &curl)
        .with_ca_bundle(&ca)
        .probe()
        .unwrap_err();
    assert!(matches!(e, CurlError::SandboxUnavailable(_)), "{e:?}");
}
