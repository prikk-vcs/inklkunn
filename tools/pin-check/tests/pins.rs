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

/// The one compiler every build job installs (the declared minimum aside).
fn build_compiler() -> String {
    let msrv =
        declared_msrv(&fs::read_to_string(workspace_root().join("Cargo.toml")).unwrap_or_default())
            .expect("rust-version declared");
    let versions: std::collections::BTreeSet<String> = workflows()
        .iter()
        .flat_map(|(_, y)| toolchain_versions(y))
        .filter(|v| *v != msrv)
        .collect();
    assert_eq!(versions.len(), 1, "{versions:?}");
    versions.into_iter().next().expect("one")
}

#[test]
fn the_toolchain_file_names_the_compiler_ci_installs_and_the_components_it_installs() {
    let root = workspace_root();
    let file = fs::read_to_string(root.join("rust-toolchain.toml")).unwrap_or_default();
    let channel = toolchain_file_channel(&file).expect("rust-toolchain.toml names a channel");
    assert!(
        is_exact_version(&channel),
        "the channel is not X.Y.Z: {channel}"
    );
    assert_eq!(
        channel,
        build_compiler(),
        "rust-toolchain.toml differs from the compiler the workflows install"
    );
    let release =
        fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap_or_default();
    assert_eq!(
        declared_toolchain_env(&release).as_deref(),
        Some(channel.as_str()),
        "release.yml's RUST_TOOLCHAIN differs from rust-toolchain.toml"
    );
    // The components are the ones CI installs for the build compiler, no more and no fewer.
    let mut installed: Vec<String> = workflows()
        .iter()
        .flat_map(|(_, y)| installed_components(y))
        .filter(|(v, _)| *v == channel)
        .map(|(_, c)| c)
        .collect();
    installed.sort();
    installed.dedup();
    let mut named = toolchain_file_components(&file);
    named.sort();
    assert!(!named.is_empty(), "rust-toolchain.toml names no components");
    assert_eq!(
        named, installed,
        "rust-toolchain.toml's components differ from what the workflows install"
    );
    // And on EVERY install of the pin, not only somewhere: the file makes the first plain `cargo` of a job
    // fetch what its install step left out (M29).
    let mut installs_of_the_pin = 0;
    for (name, yaml) in workflows() {
        installs_of_the_pin += toolchain_installs(&yaml)
            .iter()
            .filter(|i| i.version == channel)
            .count();
        let missing = installs_missing_components(&yaml, &channel, &named);
        assert!(
            missing.is_empty(),
            "{name}: an install of {channel} leaves out components rust-toolchain.toml names: {missing:?}"
        );
    }
    assert!(
        installs_of_the_pin >= 5,
        "the rule is not vacuous: ci.yml (2), release.yml (3) and supply-chain.yml (1) install the pin, saw {installs_of_the_pin}"
    );
}

#[test]
fn an_install_of_the_pin_without_the_files_components_is_caught() {
    let want = vec!["rustfmt".to_owned(), "clippy".to_owned()];
    // The three shapes of the bug: no component, one of two, and a component on a continuation line.
    let none = "        run: |\n          rustup toolchain install 1.99.1 --profile minimal --no-self-update\n";
    assert_eq!(
        installs_missing_components(none, "1.99.1", &want),
        vec![(2, vec!["rustfmt".to_owned(), "clippy".to_owned()])]
    );
    let one = "          rustup toolchain install 1.99.1 --profile minimal --component rustfmt --no-self-update\n";
    assert_eq!(
        installs_missing_components(one, "1.99.1", &want),
        vec![(1, vec!["clippy".to_owned()])]
    );
    let both = "          rustup toolchain install 1.99.1 --profile minimal --component rustfmt --component clippy --no-self-update\n";
    assert!(installs_missing_components(both, "1.99.1", &want).is_empty());
    let continued = "          rustup toolchain install 1.99.1 --profile minimal \\\n            --component rustfmt --component clippy --no-self-update\n";
    assert!(
        installs_missing_components(continued, "1.99.1", &want).is_empty(),
        "a continuation line is part of the command"
    );
    assert_eq!(
        installed_components(continued).len(),
        2,
        "installed_components reads the continued command whole too"
    );
    // The minimum compiler is not the pin: its install names no components and is not asked to.
    let msrv = "          rustup toolchain install 1.89.0 --profile minimal --no-self-update\n";
    assert!(installs_missing_components(msrv, "1.99.1", &want).is_empty());
    // One good install does not excuse a bad one beside it.
    let mixed = format!("{both}{none}");
    assert_eq!(
        installs_missing_components(&mixed, "1.99.1", &want).len(),
        1
    );
}

#[test]
fn every_job_has_a_timeout() {
    for (name, yaml) in workflows() {
        let bare = jobs_without_timeout(&yaml);
        assert!(
            bare.is_empty(),
            "{name}: jobs without `timeout-minutes` (a hung test holds one for six hours): {bare:?}"
        );
    }
    let set: usize = workflows()
        .iter()
        .map(|(_, y)| y.matches("    timeout-minutes:").count())
        .sum();
    assert!(set >= 10, "the rule is not vacuous: saw {set} timeouts");
}

#[test]
fn a_job_without_a_timeout_is_caught() {
    let bare = "name: x\njobs:\n  build:\n    runs-on: ubuntu-24.04\n    steps:\n      - run: true\n  test:\n    runs-on: ubuntu-24.04\n    timeout-minutes: 10\n    steps:\n      - run: true\n";
    assert_eq!(jobs_without_timeout(bare), vec![(3, "build".to_owned())]);
    // A timeout that is not a number of minutes, or is zero, is none; one set on a step does not count.
    let weird = "jobs:\n  a:\n    timeout-minutes: ${{ inputs.t }}\n    steps:\n      - run: true\n  b:\n    timeout-minutes: 0\n    steps:\n      - run: true\n  c:\n    steps:\n      - run: true\n        timeout-minutes: 5\n";
    let names: Vec<String> = jobs_without_timeout(weird)
        .into_iter()
        .map(|(_, n)| n)
        .collect();
    assert_eq!(names, vec!["a", "b", "c"]);
    // A job that has one, with a trailing comment, is fine; the keys of other top-level blocks are not jobs.
    let ok = "on:\n  push:\n    branches: [main]\njobs:\n  only:\n    timeout-minutes: 7 # twice the slowest\n    steps:\n      - run: true\n";
    assert!(jobs_without_timeout(ok).is_empty());
}

#[test]
fn a_rustup_default_never_names_a_compiler_the_toolchain_file_would_override() {
    // The checkout's rust-toolchain.toml beats `rustup default`: a default naming anything but the pinned
    // compiler is a default that does nothing (the msrv job names its compiler on every command instead).
    let channel = toolchain_file_channel(
        &fs::read_to_string(workspace_root().join("rust-toolchain.toml")).unwrap_or_default(),
    )
    .expect("a channel");
    let msrv =
        declared_msrv(&fs::read_to_string(workspace_root().join("Cargo.toml")).unwrap_or_default())
            .expect("rust-version declared");
    for (name, yaml) in workflows() {
        for d in default_toolchains(&yaml) {
            assert!(d == channel || d == msrv, "{name}: `rustup default {d}`");
        }
        // The minimum compiler is named on every cargo and rustc of the job that installs it.
        assert!(
            msrv_commands_without_the_toolchain(&yaml, &msrv).is_empty(),
            "{name}: a command of the msrv job does not name +{msrv}: {:?}",
            msrv_commands_without_the_toolchain(&yaml, &msrv)
        );
    }
    // Not vacuous: the msrv job exists and is seen.
    let ci =
        fs::read_to_string(workspace_root().join(".github/workflows/ci.yml")).unwrap_or_default();
    assert!(
        ci.contains(&format!("cargo +{msrv} check")),
        "the msrv job does not run `cargo +{msrv} check`"
    );
}

#[test]
fn the_toolchain_file_rules_catch_planted_failures() {
    let file = "[toolchain]\nchannel = \"1.99.1\"\nprofile = \"minimal\"\ncomponents = [\"rustfmt\", \"clippy\"]\n";
    assert_eq!(toolchain_file_channel(file), Some("1.99.1".into()));
    assert_eq!(toolchain_file_components(file), vec!["rustfmt", "clippy"]);
    assert!(is_exact_version("1.99.1"));
    assert!(!is_exact_version("stable"));
    assert!(!is_exact_version("1.99"));
    assert_eq!(
        installed_components(
            "  rustup toolchain install 1.99.1 --profile minimal --component rustfmt --component clippy --no-self-update\n  rustup toolchain install 1.89.0 --profile minimal\n"
        ),
        vec![
            ("1.99.1".to_owned(), "rustfmt".to_owned()),
            ("1.99.1".to_owned(), "clippy".to_owned())
        ]
    );
    assert_eq!(
        default_toolchains("  rustup default 1.99.1\n  rustup default stable\n"),
        vec!["1.99.1", "stable"]
    );
    assert_eq!(
        declared_toolchain_env("env:\n  RUST_TOOLCHAIN: \"1.99.1\"\n"),
        Some("1.99.1".into())
    );
    // The msrv job: a bare `cargo` or `rustc` would run the toolchain file's compiler, not 1.89.0.
    let bare = "jobs:\n  msrv:\n    steps:\n      - name: t\n        run: |\n          rustup toolchain install 1.89.0 --profile minimal\n          rustup default 1.89.0\n          rustc --version\n      - name: c\n        run: cargo check --workspace\n";
    assert_eq!(
        msrv_commands_without_the_toolchain(bare, "1.89.0").len(),
        2,
        "{:?}",
        msrv_commands_without_the_toolchain(bare, "1.89.0")
    );
    let named = "jobs:\n  msrv:\n    steps:\n      - name: t\n        run: |\n          rustup toolchain install 1.89.0 --profile minimal\n          rustc +1.89.0 --version\n      - name: c\n        run: cargo +1.89.0 check --workspace\n  build:\n    steps:\n      - name: b\n        run: cargo test\n";
    assert!(
        msrv_commands_without_the_toolchain(named, "1.89.0").is_empty(),
        "{:?}",
        msrv_commands_without_the_toolchain(named, "1.89.0")
    );
    // A job that does not install the minimum is not its subject.
    let other = "jobs:\n  build:\n    steps:\n      - name: b\n        run: |\n          rustup toolchain install 1.99.1 --profile minimal\n          cargo test\n";
    assert!(msrv_commands_without_the_toolchain(other, "1.89.0").is_empty());
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
