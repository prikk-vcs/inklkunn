#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

#[test]
fn a_secret_and_its_pieces_are_replaced() {
    let auth = "Basic c2VjcmV0LWNsaWVudA==";
    let form = "grant_type=authorization_code&code=SECRETCODE";
    let stderr =
        b"curl: (35) bad Basic c2VjcmV0LWNsaWVudA== then SECRETCODE and authorization_code\n";
    let out = scrub(stderr, &[auth, form]);
    for gone in ["c2VjcmV0LWNsaWVudA==", "SECRETCODE", "authorization_code"] {
        assert!(!out.contains(gone), "{out}");
    }
    assert!(out.contains("[redacted]"));
    assert!(out.starts_with("curl: (35) bad"));
}

#[test]
fn only_the_first_line_survives_and_controls_are_dropped() {
    let out = scrub(b"\n\n  first line\x07 here  \nsecond line\n", &[]);
    assert_eq!(out, "first line here");
}

#[test]
fn the_length_is_bounded_after_redaction() {
    let long = "x".repeat(1000);
    assert_eq!(scrub(long.as_bytes(), &[]).chars().count(), MAX_EXCERPT);
    // A secret that straddles the cut is redacted whole before the cut, not leaked in part.
    let secret = "S".repeat(150);
    let line = format!("{} {secret}", "y".repeat(100));
    let out = scrub(line.as_bytes(), &[&secret]);
    assert!(!out.contains("SSSS"), "{out}");
}

#[test]
fn bytes_that_are_not_utf8_are_read_lossily() {
    let out = scrub(&[0xff, b'o', b'k'], &[]);
    assert!(out.ends_with("ok"));
}

#[test]
fn short_fragments_are_not_hunted() {
    // `a=1` would otherwise blank every `a` and `1` of the message.
    let out = scrub(b"a connection 1 failed", &["a=1"]);
    assert_eq!(out, "a connection 1 failed");
}
