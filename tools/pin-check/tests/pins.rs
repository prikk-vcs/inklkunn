//! RFC 010 D-9/D-10 applied to the real workflows, each rule with a planted failure.
// Tests may `expect`/`unwrap`: a failed expectation is the test failing, not a product path.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;

use inklkunn_pin_check::*;

fn workflows() -> Vec<(String, String)> {
    let dir = workspace_root().join(".github/workflows");
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "yml") {
                out.push((
                    p.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    fs::read_to_string(&p).unwrap_or_default(),
                ));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn every_action_is_pinned_to_a_sha_in_a_github_owned_namespace() {
    for (name, yaml) in workflows() {
        let bad = unpinned_uses(&yaml);
        assert!(
            bad.is_empty(),
            "{name}: unpinned or third-party actions {bad:?}"
        );
    }
}

#[test]
fn one_compiler_builds_everything_except_the_msrv_job() {
    let root = workspace_root();
    let msrv = declared_msrv(&fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default())
        .expect("rust-version declared");
    let mut build_versions = std::collections::BTreeSet::new();
    let mut saw_msrv_job = false;
    for (name, yaml) in workflows() {
        for v in toolchain_versions(&yaml) {
            if v == msrv {
                saw_msrv_job = true;
            } else {
                build_versions.insert(v);
            }
        }
        assert!(
            moving_runners(&yaml).is_empty(),
            "{name}: moving runner label"
        );
    }
    assert_eq!(
        build_versions.len(),
        1,
        "more than one build compiler: {build_versions:?}"
    );
    assert!(saw_msrv_job, "no job builds on the declared minimum {msrv}");
}

#[test]
fn the_container_base_is_digested() {
    let d = fs::read_to_string(workspace_root().join("Dockerfile")).unwrap_or_default();
    assert!(
        undigested_from(&d).is_empty(),
        "undigested FROM in Dockerfile"
    );
}

#[test]
fn the_release_rehearsal_cannot_reach_the_real_package() {
    let y = fs::read_to_string(workspace_root().join(".github/workflows/release.yml"))
        .unwrap_or_default();
    let v = rehearsal_violations(&y);
    assert!(v.is_empty(), "{v:#?}");
}

#[test]
fn pin_rules_catch_planted_failures() {
    assert_eq!(unpinned_uses("- uses: actions/checkout@v4\n").len(), 1);
    assert_eq!(
        unpinned_uses("- uses: docker/login-action@3d3c42e5aac5ba805825da76410c181273ba90b1\n")
            .len(),
        1
    );
    assert!(
        unpinned_uses("- uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7\n")
            .is_empty()
    );
    assert_eq!(
        toolchain_versions("  rustup toolchain install 1.99.0 --profile minimal\n"),
        vec!["1.99.0"]
    );
    assert_eq!(moving_runners("    runs-on: ubuntu-latest\n").len(), 1);
    assert_eq!(undigested_from("FROM debian:bookworm-slim\n").len(), 1);
    assert_eq!(
        declared_msrv("rust-version = \"1.88.0\"\n"),
        Some("1.88.0".into())
    );
    let bad = "jobs:\n  image:\n    steps:\n      - name: push\n        run: docker push ghcr.io/prikk-vcs/inklkunn:1\n";
    assert_eq!(rehearsal_violations(bad).len(), 1);
    let bad2 = "      - name: publish\n        if: inputs.mode == 'rehearsal'\n        run: cargo publish\n";
    assert_eq!(rehearsal_violations(bad2).len(), 1);
    let ok = "      - name: push\n        if: github.event_name == 'push'\n        run: docker push ghcr.io/prikk-vcs/inklkunn:1\n";
    assert!(rehearsal_violations(ok).is_empty());
}

#[test]
fn the_rehearsal_package_is_not_mistaken_for_the_real_image() {
    assert!(!names_real_image(
        "docker push ghcr.io/prikk-vcs/inklkunn-rehearsal:rehearsal-amd64"
    ));
    assert!(names_real_image(
        "docker push ghcr.io/prikk-vcs/inklkunn:1.0.0-amd64"
    ));
    assert!(names_real_image(
        "the image `ghcr.io/prikk-vcs/inklkunn:%s`"
    ));
}
