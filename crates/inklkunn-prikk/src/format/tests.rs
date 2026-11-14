use super::*;

const V054: Version = (0, 54, 0);
const V055: Version = (0, 55, 0);
const V056: Version = (0, 56, 5);
const V057: Version = (0, 57, 0);
const V058: Version = (0, 58, 0);
const SAYS_8: &str = "error: unsupported format version: 8\n";

#[test]
fn the_one_table_says_format_8_is_read_from_0_57_0_on_and_knows_nothing_else() {
    assert_eq!(first_reader(8), Some(V057));
    for other in [0, 1, 7, 9, 10, 99, u64::MAX] {
        assert_eq!(first_reader(other), None, "{other}");
    }
}

#[test]
fn a_format_error_is_a_mismatch_only_where_the_running_prikk_is_known_not_to_read_it() {
    for old in [V054, V055, V056] {
        assert_eq!(
            classify_format_error(Some(1), b"", SAYS_8, Some(old)),
            Some(FormatError::Mismatch { format: 8 }),
            "{old:?}"
        );
    }
    // A prikk that reads format 8, a version nobody read, a version the table says nothing of.
    for (what, running) in [
        ("0.57.0", Some(V057)),
        ("0.58.0", Some(V058)),
        ("unknown", None),
    ] {
        assert_eq!(
            classify_format_error(Some(1), b"", SAYS_8, running),
            Some(FormatError::Marker { format: Some(8) }),
            "{what}"
        );
    }
    // A number the table does not know is a marker on every prikk, old or new.
    for running in [None, Some(V054), Some(V057), Some(V058)] {
        assert_eq!(
            classify_format_error(Some(1), b"", "unsupported format version: 9", running),
            Some(FormatError::Marker { format: Some(9) }),
            "{running:?}"
        );
    }
}

#[test]
fn the_driver_and_the_signer_ask_with_the_same_conditions() {
    let refusal = br#"{"schema_version":"refusal-v1","refusal":"precondition-not-met","reason":null,"message":"x"}"#;
    let not_a_refusal = br#"{"schema_version":"verify-report-v1"}"#;
    // Exit 1, and no `refusal-v1` report.
    assert!(classify_format_error(Some(1), not_a_refusal, SAYS_8, Some(V055)).is_some());
    assert!(classify_format_error(Some(1), b"", SAYS_8, Some(V055)).is_some());
    assert_eq!(
        classify_format_error(Some(1), refusal, SAYS_8, Some(V055)),
        None
    );
    for code in [Some(0), Some(2), Some(101), Some(137), None] {
        assert_eq!(
            classify_format_error(code, b"", SAYS_8, Some(V055)),
            None,
            "{code:?}"
        );
    }
    // The sentence is whole, and the number has no leading zero and at most nine digits.
    for said in [
        "error: unsupported format version: 08",
        "error: unsupported format version: 008",
        "error: unsupported format version: 1234567890",
        "error: unsupported format version: eight",
        "error: unsupported format version: 8 and more",
        "error: page \"unsupported format version: 8\" is not there",
        "",
    ] {
        assert_eq!(
            classify_format_error(Some(1), b"", said, Some(V055)),
            None,
            "{said}"
        );
    }
}

#[test]
fn the_sentences_name_the_number_prikk_gave_and_the_marker_sentence_claims_nothing_is_intact() {
    let mismatch = unsupported_format_sentence(8);
    assert!(
        mismatch.contains("format 8") && mismatch.contains("0.57.0"),
        "{mismatch}"
    );
    let marker = format_marker_sentence(9);
    assert!(marker.contains("format 9"), "{marker}");
    assert!(!marker.contains('8'), "never a fixed 8: {marker}");
    assert!(
        marker.contains(".prikk/FORMAT") && marker.contains("backup"),
        "{marker}"
    );
    for said in [&mismatch, &marker, &format_marker_sentence(8)] {
        assert!(
            !said.contains("nothing in the repository is damaged"),
            "{said}"
        );
    }
}

/// prikk's integrity error as measured on 0.55.0 (`reads` is what it lists) and 0.57.0, for a marker it cannot read as
/// a number (M20). `holds` is the marker's first line in Rust's `Debug` escapes, as prikk prints it.
fn integrity_error(holds: &str, reads: &str) -> String {
    format!(
        "error: integrity error: the repository format marker .prikk/FORMAT holds \"{holds}\", which is not a format number this version reads (it reads formats {reads})\n"
    )
}

#[test]
fn prikks_integrity_error_for_a_marker_that_is_not_a_number_is_a_damaged_marker_on_every_prikk() {
    // The four markers of the review (`8\r\n`, ` 8\n`, `x\n`, an empty file), as 0.55.0 and 0.57.0 printed them, and
    // the escapes `Debug` writes for other bytes.
    for holds in [
        r"8\r",
        " 8",
        "x",
        "",
        r#"a\"b"#,
        r"x\ny",
        r"8\t",
        r"\\",
        r"\u{0}\u{7f}",
        "é",
        "99999999999999999999",
    ] {
        for reads in ["6 and 7", "6, 7 and 8"] {
            for running in [None, Some(V054), Some(V055), Some(V057), Some(V058)] {
                assert_eq!(
                    classify_format_error(Some(1), b"", &integrity_error(holds, reads), running),
                    Some(FormatError::Marker { format: None }),
                    "{holds} / {reads} / {running:?}"
                );
            }
        }
    }
    // With or without the `error:` prefix and padded, as for the other sentence.
    let bare = "integrity error: the repository format marker .prikk/FORMAT holds \"x\", which is not a format number this version reads (it reads formats 6 and 7)";
    assert_eq!(
        classify_format_error(Some(1), b"", &format!("  {bare}\n"), Some(V057)),
        Some(FormatError::Marker { format: None })
    );
}

#[test]
fn the_integrity_error_is_matched_whole_and_under_the_same_conditions_as_the_other_sentence() {
    let sound = integrity_error("x", "6 and 7");
    let refusal = br#"{"schema_version":"refusal-v1","refusal":"precondition-not-met","reason":null,"message":"x"}"#;
    assert!(classify_format_error(Some(1), b"", &sound, Some(V055)).is_some());
    assert_eq!(
        classify_format_error(Some(1), refusal, &sound, Some(V055)),
        None
    );
    for code in [Some(0), Some(2), Some(101), None] {
        assert_eq!(
            classify_format_error(code, b"", &sound, Some(V055)),
            None,
            "{code:?}"
        );
    }
    let head = "error: integrity error: the repository format marker .prikk/FORMAT holds ";
    let tail = ", which is not a format number this version reads (it reads formats 6 and 7)";
    for (what, said) in [
        // A name the repository's contributors chose never makes some other refusal a format error.
        ("a page title", "error: page \"integrity error: the repository format marker .prikk/FORMAT holds \\\"x\\\", which is not a format number this version reads (it reads formats 6 and 7)\" not found".to_owned()),
        ("a longer sentence", format!("{}\n and more", integrity_error("x", "6 and 7"))),
        ("a bare quote inside the text", format!("{head}\"x\"y\"{tail}")),
        ("a text that is not quoted", format!("{head}x{tail}")),
        ("an unclosed text", format!("{head}\"x{tail}")),
        ("an escape Debug does not write", format!("{head}\"\\q\"{tail}")),
        ("a unicode escape without braces", format!("{head}\"\\u0041\"{tail}")),
        ("an empty unicode escape", format!("{head}\"\\u{{}}\"{tail}")),
        ("a unicode escape of seven digits", format!("{head}\"\\u{{1234567}}\"{tail}")),
        ("a text that ends in a backslash", format!("{head}\"x\\\"{tail}")),
        ("another file", integrity_error("x", "6 and 7").replace(".prikk/FORMAT", ".prikk/OTHER")),
        ("another reason", integrity_error("x", "6 and 7").replace("is not a format number", "is a format number")),
        ("no list", integrity_error("x", "")),
        ("a list that is not numbers", integrity_error("x", "six and seven")),
        ("a list with a leading zero", integrity_error("x", "06 and 7")),
        ("a list with a number too long", integrity_error("x", "6 and 1234567890")),
        ("a list with an empty item", integrity_error("x", "6, , 7")),
        ("a list that goes on", integrity_error("x", "6 and 7) and more (")),
        ("nothing after the list", sound.replace(')', "")),
        ("another word for the error", sound.replace("integrity error", "i/o error")),
    ] {
        assert_eq!(
            classify_format_error(Some(1), b"", &said, Some(V057)),
            None,
            "{what}: {said}"
        );
    }
}

#[test]
fn the_sentence_for_a_marker_that_is_not_a_number_names_the_file_and_claims_nothing_is_intact() {
    let said = unreadable_marker_sentence();
    assert_eq!(marker_sentence(None), said);
    assert_eq!(marker_sentence(Some(9)), format_marker_sentence(9));
    assert!(
        said.contains(".prikk/FORMAT")
            && said.contains("backup")
            && said.contains("not a format number"),
        "{said}"
    );
    assert!(
        said.contains("do not take the rest of the repository to be intact"),
        "{said}"
    );
    assert!(!said.chars().any(|c| c.is_ascii_digit()), "{said}");
}
