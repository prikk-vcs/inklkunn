//! RFC 010 D-3: one prikk floor and one set of asset checksums everywhere; the corpus copy matches its
//! recorded source.
// Tests may `expect`/`unwrap`: a failed expectation is the test failing, not a product path.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;

use inklkunn_floor_check::*;

#[test]
fn the_prikk_floor_is_one_value_everywhere() {
    let root = workspace_root();
    let read = |p: &str| fs::read_to_string(root.join(p)).unwrap_or_default();
    let driver =
        driver_floor(&read("crates/inklkunn-prikk/src/lib.rs")).expect("MIN_PRIKK_VERSION");
    for wf in [".github/workflows/ci.yml", ".github/workflows/release.yml"] {
        let floors = workflow_floors(&read(wf));
        assert!(!floors.is_empty(), "{wf}: no PRIKK_VERSION");
        for f in floors {
            assert_eq!(
                driver, f,
                "{wf}: PRIKK_VERSION differs from the driver floor"
            );
        }
    }
    let docker = dockerfile_floor(&read("Dockerfile"));
    assert_eq!(
        Some(driver.clone()),
        docker,
        "driver floor and Dockerfile ARG PRIKK_VERSION differ"
    );
}

#[test]
fn the_release_asset_checksums_are_recorded_and_agree_everywhere() {
    let root = workspace_root();
    let read = |p: &str| fs::read_to_string(root.join(p)).unwrap_or_default();
    let ci = recorded_checksums(&read(".github/workflows/ci.yml"));
    let release = recorded_checksums(&read(".github/workflows/release.yml"));
    let docker = recorded_checksums(&read("Dockerfile"));
    assert!(
        ci.contains_key("X86_64"),
        "ci.yml records no PRIKK_SHA256_X86_64 for the asset it installs"
    );
    assert!(
        docker.contains_key("X86_64") && docker.contains_key("AARCH64"),
        "the Dockerfile records no checksum for one of its two assets: {docker:?}"
    );
    for (arch, sum) in &docker {
        for (name, set) in [("ci.yml", &ci), ("release.yml", &release)] {
            if let Some(other) = set.get(arch) {
                assert_eq!(
                    sum, other,
                    "{name}: PRIKK_SHA256_{arch} differs from the Dockerfile"
                );
            }
        }
    }
}

#[test]
fn floor_extractors_catch_a_planted_mismatch() {
    assert_eq!(
        driver_floor("pub const MIN_PRIKK_VERSION: &str = \"0.54.0\";"),
        Some("0.54.0".into())
    );
    assert_eq!(
        workflow_floors(
            "env:\n  PRIKK_VERSION: \"0.53.0\"\njobs:\n  x:\n    env:\n      PRIKK_VERSION: \"0.52.0\"\n"
        ),
        vec!["0.53.0", "0.52.0"],
        "every copy is seen"
    );
    assert_eq!(
        dockerfile_floor("ARG PRIKK_VERSION=0.52.0\n"),
        Some("0.52.0".into())
    );
    assert_ne!(
        driver_floor("pub const MIN_PRIKK_VERSION: &str = \"0.54.0\";"),
        workflow_floors("PRIKK_VERSION: \"0.53.0\"")
            .first()
            .cloned()
    );
    let sums = recorded_checksums(
        "  PRIKK_SHA256_X86_64: \"e998909328e4d1216c0e5ccf4b00141b32c6109498f04ba94a65cc61124ab687\"\nARG PRIKK_SHA256_AARCH64=9a325df0661c767ace382f0d7550a5a28ca9fd0a73bdf6f7878968133535df0d\nARG PRIKK_SHA256_BAD=nothex\n",
    );
    assert_eq!(sums.len(), 2, "{sums:?}");
    assert!(sums.contains_key("X86_64") && sums.contains_key("AARCH64"));
}

#[test]
fn the_copied_corpus_matches_its_recorded_source() {
    let root = workspace_root();
    let dir = root.join("crates/inklkunn-markup/tests/corpus");
    assert!(
        dir.is_dir(),
        "the conformance corpus is missing from the markup crate"
    );
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
