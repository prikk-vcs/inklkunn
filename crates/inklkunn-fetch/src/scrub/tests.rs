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
fn a_format_character_inside_a_secret_does_not_hide_it() {
    // Z′-b 1. A zero-width space is category Cf, not Cc: the filter let it stand, so `s3cr3t<ZWSP>T0ken` was neither
    // the secret `s3cr3tT0ken` nor found. Format characters are dropped from the text and from the secret alike.
    let out = scrub(
        "bad s3cr3t\u{200B}T0ken here\n".as_bytes(),
        &["s3cr3t\u{200B}T0ken"],
    );
    assert_eq!(out, "bad [redacted] here");
    // In the text only, in the secret only, and with a direction override or a byte-order mark.
    let out = scrub("bad s3cr3t\u{200B}T0ken here".as_bytes(), &["s3cr3tT0ken"]);
    assert_eq!(out, "bad [redacted] here");
    let out = scrub(b"bad s3cr3tT0ken here", &["s3cr3t\u{200B}T0ken"]);
    assert_eq!(out, "bad [redacted] here");
    for fmt in ['\u{202E}', '\u{FEFF}', '\u{2060}', '\u{AD}', '\u{E0041}'] {
        let line = format!("bad s3cr3t{fmt}T0ken here");
        let out = scrub(line.as_bytes(), &["s3cr3tT0ken"]);
        assert_eq!(out, "bad [redacted] here", "U+{:04X}", fmt as u32);
    }
    // And the excerpt itself shows none.
    let out = scrub("a\u{200B}b\u{202E}c\u{FEFF}d".as_bytes(), &[]);
    assert_eq!(out, "abcd");
}

#[test]
fn the_format_characters_cover_what_the_knowledge_files_refuse() {
    // The twelve direction characters the knowledge files refuse (the signer's `is_direction_or_format`, the one
    // definition: U+202A - U+202E, U+2066 - U+2069, U+200E, U+200F and the Arabic letter mark U+061C), as a constant
    // list. This crate cannot depend on the signer, a key-material crate, even for a test (and a test that asked it
    // would show only that twelve characters are in Cf, which they are by definition); the list is the contract the
    // two sides keep, and a change to the signer's set is a change here by review. Every one is dropped.
    const DIRECTION: [char; 12] = [
        '\u{202A}', '\u{202B}', '\u{202C}', '\u{202D}', '\u{202E}', '\u{2066}', '\u{2067}',
        '\u{2068}', '\u{2069}', '\u{200E}', '\u{200F}', '\u{061C}',
    ];
    for c in DIRECTION {
        assert!(
            is_dropped(c),
            "U+{:04X} is refused by the knowledge files but kept here",
            c as u32
        );
    }
}

#[test]
fn a_piece_is_measured_after_the_controls_are_dropped() {
    // Z′-b 2. `o<control><control>r` is four characters long and two once flattened. Measured before, it was the
    // needle `or` and blanked the ordinary text `example.org`.
    let out = scrub(b"lookup of example.org failed", &["o\x01\x01r"]);
    assert_eq!(out, "lookup of example.org failed");
    let out = scrub(
        b"lookup of example.org failed",
        &["o\x01\x01r", "k=o\x01\x01r", "o\u{200B}\u{200B}r"],
    );
    assert_eq!(out, "lookup of example.org failed");
    // A short secret that is short as given is still hunted whole (as before); only a remnant is not.
    let out = scrub(b"the pin is 99 here", &["99"]);
    assert_eq!(out, "the pin is [redacted] here");
    // A long enough remnant is hunted: four characters are left.
    let out = scrub(b"bad abcd here", &["ab\x01cd"]);
    assert_eq!(out, "bad [redacted] here");
}

#[test]
fn a_secret_split_by_a_newline_is_hidden_on_the_line_that_is_shown() {
    // Z′-b 3. The first line was chosen before flattening, so `token=abcd` showed half of `token=abcdefgh1234`. The
    // text is flattened as a whole, the secret found across the break, and each line's part hidden.
    let stderr = b"failed: token=abcd\nefgh1234 rejected\n";
    let out = scrub(stderr, &["token=abcdefgh1234"]);
    assert_eq!(out, "failed: [redacted]");
    assert!(!out.contains("abcd"));
    // A piece alone (the part after `=`) is found across the break too.
    let out = scrub(stderr, &["abcdefgh1234"]);
    assert_eq!(out, "failed: token=[redacted]");
    // The second half, when the first line says nothing, is the line that is shown, and it is hidden as well.
    let out = scrub(b"\n  \nabcd\nefgh1234 rejected\n", &["abcdefgh1234"]);
    assert_eq!(out, "[redacted]");
    let out = scrub(b"error\nabcd\nefgh1234 rejected\n", &["abcdefgh1234"]);
    assert_eq!(out, "error");
    // Still one line: the lines after the first are not shown.
    let out = scrub(b"first\nsecond\n", &["unrelated-secret"]);
    assert_eq!(out, "first");
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
