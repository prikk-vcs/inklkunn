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
            Some(FormatError::Marker { format: 8 }),
            "{what}"
        );
    }
    // A number the table does not know is a marker on every prikk, old or new.
    for running in [None, Some(V054), Some(V057), Some(V058)] {
        assert_eq!(
            classify_format_error(Some(1), b"", "unsupported format version: 9", running),
            Some(FormatError::Marker { format: 9 }),
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
