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
            if p.extension().is_some_and(|x| x == "yml" || x == "yaml") {
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
    assert!(!out.is_empty(), "no workflows found");
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
fn one_compiler_builds_everything_except_the_msrv_job_and_nothing_moves() {
    let root = workspace_root();
    let msrv = declared_msrv(&fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default())
        .expect("rust-version declared");
    let mut build_versions = std::collections::BTreeSet::new();
    let mut saw_msrv_job = false;
    for (name, yaml) in workflows() {
        assert!(
            moving_toolchains(&yaml).is_empty(),
            "{name}: a toolchain that is not X.Y.Z: {:?}",
            moving_toolchains(&yaml)
        );
        for v in toolchain_versions(&yaml) {
            if v == msrv {
                saw_msrv_job = true;
            } else {
                build_versions.insert(v);
            }
        }
        assert!(
            moving_runners(&yaml).is_empty(),
            "{name}: moving runner label {:?}",
            moving_runners(&yaml)
        );
        assert!(
            unpinned_installs(&yaml).is_empty(),
            "{name}: `cargo install` without name@X.Y.Z {:?}",
            unpinned_installs(&yaml)
        );
    }
    assert_eq!(
        build_versions.len(),
        1,
        "more than one build compiler: {build_versions:?}"
    );
    assert!(saw_msrv_job, "no job builds on the declared minimum {msrv}");
    // One version of each installed tool across every workflow.
    let mut pins: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for (_, yaml) in workflows() {
        for (name, v) in install_pins(&yaml) {
            pins.entry(name).or_default().insert(v);
        }
    }
    assert!(!pins.is_empty(), "no pinned cargo install found");
    for (name, versions) in &pins {
        assert_eq!(
            versions.len(),
            1,
            "`{name}` is installed at more than one version: {versions:?}"
        );
    }
}

#[test]
fn the_container_base_is_digested() {
    let d = fs::read_to_string(workspace_root().join("Dockerfile")).unwrap_or_default();
    assert!(
        undigested_from(&d).is_empty(),
        "undigested FROM in Dockerfile: {:?}",
        undigested_from(&d)
    );
}

#[test]
fn the_release_rehearsal_cannot_reach_the_real_package() {
    let y = fs::read_to_string(workspace_root().join(".github/workflows/release.yml"))
        .unwrap_or_default();
    let v = rehearsal_violations(&y);
    assert!(v.is_empty(), "{v:#?}");
    // The real workflow has steps that are push-guarded and steps that are rehearsal-marked: the parser
    // sees both, so the rule above is not passing vacuously.
    let s = steps(&y);
    assert!(s.iter().any(|st| st.guards.iter().any(|g| g == PUSH_GUARD)));
    assert!(
        s.iter()
            .any(|st| st.guards.iter().any(|g| g == REHEARSAL_GUARD))
    );
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
    assert_eq!(
        moving_toolchains("  rustup toolchain install stable --profile minimal\n"),
        vec!["stable"]
    );
    assert_eq!(moving_runners("    runs-on: ubuntu-latest\n").len(), 1);
    assert_eq!(
        moving_runners("      matrix:\n        include:\n          - os: ubuntu-latest\n").len(),
        1
    );
    assert!(
        moving_runners("    runs-on: ${{ matrix.os }}\n          - os: ubuntu-24.04\n").is_empty()
    );
    assert_eq!(
        unpinned_installs("        run: cargo install cargo-deny cargo-audit --locked\n").len(),
        2
    );
    assert!(
        unpinned_installs(
            "        run: cargo install cargo-deny@0.20.2 cargo-audit@0.22.2 --locked\n"
        )
        .is_empty()
    );
    assert_eq!(undigested_from("FROM debian:bookworm-slim\n").len(), 1);
    assert_eq!(
        undigested_from("from debian:bookworm-slim\n").len(),
        1,
        "case"
    );
    assert!(
        undigested_from(
            "FROM debian:bookworm-slim@sha256:7c7b2c966bc9ee8cedfeef67e0e279108992c77681fa595db4a9d65c06ccc587 AS fetch\nFROM fetch\nCOPY --from=fetch /a /b\n"
        )
        .is_empty(),
        "a declared stage may be reused"
    );
    assert_eq!(
        undigested_from("COPY --from=ghcr.io/x/y:1 /a /b\n").len(),
        1
    );
    assert_eq!(
        declared_msrv("rust-version = \"1.88.0\"\n"),
        Some("1.88.0".into())
    );
    let bad = "jobs:\n  image:\n    steps:\n      - name: push\n        run: docker push ghcr.io/prikk-vcs/inklkunn:1\n";
    assert_eq!(rehearsal_violations(bad).len(), 1);
    let bad2 = "jobs:\n  crates:\n    steps:\n      - name: publish\n        if: inputs.mode == 'rehearsal'\n        run: cargo publish\n";
    assert_eq!(
        rehearsal_violations(bad2).len(),
        2,
        "unguarded and a rehearsal publish"
    );
    let ok = "jobs:\n  image:\n    steps:\n      - name: push\n        if: github.event_name == 'push'\n        run: docker push ghcr.io/prikk-vcs/inklkunn:1\n";
    assert!(rehearsal_violations(ok).is_empty());
    // A job-level guard covers its steps; a widened guard is no guard; a guard in a comment or an echo is
    // not a guard; a step beginning `- id:` is its own step.
    let job_guarded = "jobs:\n  crates:\n    if: github.event_name == 'push'\n    steps:\n      - name: publish\n        run: cargo publish\n";
    assert!(
        rehearsal_violations(job_guarded).is_empty(),
        "{:?}",
        rehearsal_violations(job_guarded)
    );
    let widened = "jobs:\n  crates:\n    steps:\n      - name: publish\n        if: github.event_name == 'push' || inputs.mode == 'force'\n        run: cargo publish\n";
    assert_eq!(rehearsal_violations(widened).len(), 1);
    let echoed = "jobs:\n  crates:\n    steps:\n      - name: publish\n        run: |\n          echo \"github.event_name == 'push'\"\n          cargo publish\n";
    assert_eq!(rehearsal_violations(echoed).len(), 1);
    let id_step = "jobs:\n  image:\n    steps:\n      - name: a\n        if: github.event_name == 'push'\n        run: true\n      - id: b\n        run: docker push ghcr.io/prikk-vcs/inklkunn:1\n";
    assert_eq!(
        rehearsal_violations(id_step).len(),
        1,
        "`- id:` starts a new, unguarded step"
    );
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
