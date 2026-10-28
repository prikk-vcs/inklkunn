#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn a_path_into_a_seed_directory_never_reaches_an_error() {
    let said = "error: author signing is required: no seed at /dev/shm/inklkunn/inklkunn-seeds-0123456789abcdef0123456789abcdef/author.seed. Create one with `prikk key generate --out /dev/shm/inklkunn/inklkunn-seeds-ffffffffffffffffffffffffffffffff/x.seed` first";
    let clean = scrub(said);
    assert!(
        !clean.contains("inklkunn-seeds-") && !clean.contains("/dev/shm"),
        "{clean}"
    );
    assert!(clean.contains("no seed at <seed directory>."), "{clean}");
    assert!(
        clean.contains("--out <seed directory>` first"),
        "the rest of the sentence stays: {clean}"
    );
    // Nothing to scrub, nothing changes; and a quoted path ends at the quote.
    assert_eq!(
        scrub("error: precondition not met"),
        "error: precondition not met"
    );
    assert_eq!(
        scrub("cannot read '/a/inklkunn-seeds-ab/x' now"),
        "cannot read '<seed directory>' now"
    );
    assert_eq!(scrub("inklkunn-seeds-ab"), "<seed directory>");
    assert_eq!(scrub(""), "");
}

#[test]
fn a_refused_write_carries_prikks_words_without_the_seed_path() {
    // Exercise `outcome` itself with a failed run whose stderr names a seed file.
    let run = Run {
        status: std::process::Command::new("false")
            .status()
            .expect("false runs"),
        stdout: Vec::new(),
        stderr: b"error: no seed at /run/inklkunn-seeds-00ff/author.seed\n".to_vec(),
        timed_out: false,
        output_exceeded: false,
    };
    let e = outcome("commit", run).unwrap_err();
    let shown = e.to_string();
    assert!(
        !shown.contains("inklkunn-seeds-") && !shown.contains("/run/"),
        "{shown}"
    );
    assert!(shown.contains("<seed directory>"), "{shown}");
}
