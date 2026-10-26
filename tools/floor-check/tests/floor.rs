//! RFC 010 D-3: one prikk floor everywhere; the corpus copy matches its recorded source.
// Tests may `expect`/`unwrap`: a failed expectation is the test failing, not a product path.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;

use inklkunn_floor_check::*;

#[test]
fn the_prikk_floor_is_one_value() {
    let root = workspace_root();
    let read = |p: &str| fs::read_to_string(root.join(p)).unwrap_or_default();
    let driver = driver_floor(&read("crates/inklkunn-prikk/src/lib.rs"))
        .or_else(|| driver_floor(&read("crates/inklkunn-prikk/src/version.rs")));
    let ci = workflow_floor(&read(".github/workflows/ci.yml"));
    let release = workflow_floor(&read(".github/workflows/release.yml"));
    let docker = dockerfile_floor(&read("Dockerfile"));
    assert!(driver.is_some(), "no MIN_PRIKK_VERSION in the driver");
    assert_eq!(driver, ci, "driver floor and ci.yml PRIKK_VERSION differ");
    assert_eq!(
        driver, release,
        "driver floor and release.yml PRIKK_VERSION differ"
    );
    assert_eq!(
        driver, docker,
        "driver floor and Dockerfile ARG PRIKK_VERSION differ"
    );
}

#[test]
fn floor_extractors_catch_a_planted_mismatch() {
    assert_eq!(
        driver_floor("pub const MIN_PRIKK_VERSION: &str = \"0.54.0\";"),
        Some("0.54.0".into())
    );
    assert_eq!(
        workflow_floor("env:\n  PRIKK_VERSION: \"0.53.0\"\n"),
        Some("0.53.0".into())
    );
    assert_eq!(
        dockerfile_floor("ARG PRIKK_VERSION=0.52.0\n"),
        Some("0.52.0".into())
    );
    assert_ne!(
        driver_floor("pub const MIN_PRIKK_VERSION: &str = \"0.54.0\";"),
        workflow_floor("PRIKK_VERSION: \"0.53.0\"")
    );
}

#[test]
fn the_copied_corpus_matches_its_recorded_source() {
    let root = workspace_root();
    let dir = root.join("crates/inklkunn-markup/tests/corpus");
    if !dir.exists() {
        // T2 copies the corpus; until then there is nothing to pin.
        return;
    }
    let source = fs::read_to_string(dir.join("CORPUS-SOURCE.md")).unwrap_or_default();
    let recorded = recorded_digest(&source).expect("CORPUS-SOURCE.md records a digest");
    let actual = tree_digest(&dir);
    assert_eq!(
        recorded, actual,
        "the corpus copy drifted from CORPUS-SOURCE.md (recorded {recorded:x}, actual {actual:x})"
    );
}

#[test]
fn the_corpus_digest_detects_drift() {
    let tmp = std::env::temp_dir().join(format!("inklkunn-floor-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).expect("tmp");
    fs::write(tmp.join("a.md"), "one").expect("write");
    let before = tree_digest(&tmp);
    fs::write(tmp.join("a.md"), "two").expect("write");
    assert_ne!(before, tree_digest(&tmp));
    assert_eq!(recorded_digest("digest: ff\n"), Some(0xff));
    let _ = fs::remove_dir_all(&tmp);
}
