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
fn a_line_separator_is_dropped_like_a_control_character() {
    // U+2028 and U+2029 end a line for readers that split on them, and `str::lines` does not: the excerpt is one
    // line, so they go too, with the controls (the C0 and C1 ranges) and not with the printable characters.
    let out = scrub(
        "one\u{2028}line\u{2029}only\u{85}here\u{1b}[0m \u{3042}\n".as_bytes(),
        &[],
    );
    assert_eq!(out, "onelineonlyhere[0m \u{3042}");
    assert!(is_line_or_control('\u{2028}') && is_line_or_control('\u{2029}'));
    assert!(is_line_or_control('\0') && is_line_or_control('\u{9f}'));
    assert!(
        !is_line_or_control(' ')
            && !is_line_or_control('\u{3042}')
            && !is_line_or_control('\u{2027}')
    );
}

#[test]
fn a_secret_split_by_a_line_separator_or_a_control_is_redacted_not_joined_back() {
    // Z-c. Redacting and then filtering left `token=abcd<U+2028>efgh1234` as `token=abcdefgh1234`: the secret's
    // pieces did not match the text with the separator inside, and the filter then joined them. The text is
    // flattened first, so the secret (looked for flattened as well) is found.
    for gap in ['\u{2028}', '\u{2029}', '\u{85}', '\u{7}', '\0'] {
        let line = format!("failed: token=abcd{gap}efgh1234 rejected\n");
        let out = scrub(line.as_bytes(), &["token=abcdefgh1234"]);
        assert_eq!(out, "failed: [redacted] rejected", "{gap:?}");
        // A piece of it alone (the part after `=`) goes too.
        let out = scrub(line.as_bytes(), &["abcdefgh1234"]);
        assert_eq!(out, "failed: token=[redacted] rejected", "{gap:?}");
    }
    // A secret that itself holds a separator is found in its flattened form.
    let out = scrub(b"bad abcdefgh1234 here", &["abcd\u{2028}efgh1234"]);
    assert_eq!(out, "bad [redacted] here");
    // A secret of only controls is not hunted (it would match between every two characters).
    let out = scrub(b"plain text", &["\u{2028}\u{7}"]);
    assert_eq!(out, "plain text");
}

#[test]
fn the_scrub_copy_of_the_line_or_control_rule_is_the_stores_over_every_code_point() {
    // The crate cannot call the store's `is_line_or_control` (it depends on the driver alone; the boundary gate reads
    // runtime dependencies), so it holds a copy. The store is a dev-dependency for this test: the two agree on every
    // Unicode scalar value, and the walk is not empty of either answer.
    let (mut yes, mut no) = (0_u32, 0_u32);
    for code in 0..=0x0010_FFFF_u32 {
        let Some(c) = char::from_u32(code) else {
            continue;
        };
        let ours = is_line_or_control(c);
        assert_eq!(
            ours,
            inklkunn_store::record::is_line_or_control(c),
            "U+{code:04X} is judged differently by the store"
        );
        if ours {
            yes += 1;
        } else {
            no += 1;
        }
    }
    // 65 controls (C0, DEL, C1) and the two separators.
    assert_eq!(yes, 65 + 2);
    assert!(no > 1_000_000);
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
