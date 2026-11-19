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

/// The pin, the components `rust-toolchain.toml` names and the declared minimum: what the canonical form is
/// written in terms of.
fn canonical_terms() -> (String, Vec<String>, String) {
    let root = workspace_root();
    let file = fs::read_to_string(root.join("rust-toolchain.toml")).unwrap_or_default();
    (
        toolchain_file_channel(&file).expect("a channel"),
        toolchain_file_components(&file),
        declared_msrv(&fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default())
            .expect("rust-version declared"),
    )
}

#[test]
fn the_workflows_are_in_the_one_canonical_form_and_the_reader_sees_them() {
    // M38: pin-check refuses what it cannot read. Every workflow is read in the canonical form; one line
    // outside it fails here, with its line.
    let (pin, components, msrv) = canonical_terms();
    assert_eq!(components, vec!["rustfmt", "clippy"]);
    let (mut jobs, mut pin_installs, mut msrv_installs) = (0, 0, 0);
    for (name, yaml) in workflows() {
        let r = read_workflow(&yaml, &pin, &components, &msrv);
        assert!(
            r.refusals.is_empty(),
            "{name} is outside the canonical form:\n{:#?}",
            r.refusals
        );
        jobs += r.jobs.len();
        pin_installs += r.installs.iter().filter(|i| i.version == pin).count();
        msrv_installs += r.installs.iter().filter(|i| i.version == msrv).count();
        // Every job that installs a compiler is a job the reader saw.
        for i in &r.installs {
            assert!(r.jobs.iter().any(|j| j.name == i.job), "{name}: {i:?}");
        }
        // And the reader's count is not short of the text's: each canonical install is a line that starts
        // `rustup toolchain install`, and no other line of the file does.
        let textual = yaml
            .lines()
            .filter(|l| l.trim().starts_with("rustup toolchain install "))
            .count();
        assert_eq!(textual, r.installs.len(), "{name}");
    }
    // Not vacuous: ci.yml has 3 jobs, release.yml 6 and supply-chain.yml 1; the pin is installed by ci.yml's
    // `build` and `supply-chain`, release.yml's `gates`, `binaries` and `crates` and supply-chain.yml's
    // `checkpoint`; the declared minimum by ci.yml's `msrv`.
    assert!(jobs >= 10, "the reader saw {jobs} jobs");
    assert!(
        pin_installs >= 6,
        "the reader saw {pin_installs} installs of the pin"
    );
    assert!(
        msrv_installs >= 1,
        "the reader saw {msrv_installs} installs of the minimum"
    );
    // Every job the reader saw has a timeout of its own.
    for (name, yaml) in workflows() {
        let r = read_workflow(&yaml, &pin, &components, &msrv);
        assert!(
            r.jobs.iter().all(|j| j.timeout.is_some()),
            "{name}: {:?}",
            r.jobs
        );
    }
}

#[test]
fn the_canonical_form_refuses_every_shape_the_reviewer_found_and_reads_its_own() {
    let comps = vec!["rustfmt".to_owned(), "clippy".to_owned()];
    let install = "rustup toolchain install 1.99.1 --profile minimal --component rustfmt --component clippy --no-self-update";
    let msrv_install = "rustup toolchain install 1.89.0 --profile minimal --no-self-update";
    let read = |y: &str| read_workflow(y, "1.99.1", &comps, "1.89.0");
    // A canonical workflow is read whole: two jobs, two installs, nothing refused.
    let good = format!(
        "name: x\non:\n  push:\n    branches: [main]\njobs:\n  build:\n    runs-on: ubuntu-24.04\n    timeout-minutes: 10\n    steps:\n      - name: t\n        run: |\n          set -euo pipefail\n          {install}\n          rustup default 1.99.1\n  msrv-check:\n    timeout-minutes: 5\n    steps:\n      - run: |\n          {msrv_install}\n"
    );
    let r = read(&good);
    assert!(r.refusals.is_empty(), "{:#?}", r.refusals);
    assert_eq!(r.jobs.len(), 2);
    assert_eq!(
        r.installs
            .iter()
            .map(|i| (i.job.as_str(), i.version.as_str()))
            .collect::<Vec<_>>(),
        vec![("build", "1.99.1"), ("msrv-check", "1.89.0")]
    );
    let job = |body: &str| format!("jobs:\n  a:\n    timeout-minutes: 5\n    steps:\n{body}");
    let refused = |what: &str, y: String| {
        let r = read(&y);
        assert!(!r.refusals.is_empty(), "{what}: not refused:\n{y}");
    };
    // The reviewer's shapes, each a refusal.
    refused("a one-line run", job(&format!("      - run: {install}\n")));
    refused(
        "a one-line run without the dash",
        job(&format!("      - name: t\n        run: {install}\n")),
    );
    refused(
        "`rustup install`",
        job("      - run: |\n          rustup install 1.99.1\n"),
    );
    refused(
        "after `;`",
        job(&format!("      - run: |\n          set -e; {install}\n")),
    );
    refused(
        "after `&&`",
        job(&format!("      - run: |\n          true && {install}\n")),
    );
    refused(
        "after `||`",
        job(&format!("      - run: |\n          false || {install}\n")),
    );
    refused(
        "a job key with a comment",
        good.replace("  build:\n", "  build: # main\n"),
    );
    refused(
        "a job key with a trailing space",
        good.replace("  build:\n", "  build: \n"),
    );
    refused(
        "`jobs:` with a comment",
        good.replace("jobs:\n", "jobs: # all\n"),
    );
    refused(
        "`jobs:` in flow style",
        good.replace("jobs:\n", "jobs: {}\n"),
    );
    refused("a quoted `jobs`", good.replace("jobs:\n", "\"jobs\":\n"));
    // A following job's timeout must not be credited to a job without one: here the first job's key has a
    // comment, so it is no job and `b`'s timeout is `b`'s alone, and the refusal names the key.
    let credited = "jobs:\n  a: # first\n    steps:\n      - run: true\n  b:\n    timeout-minutes: 5\n    steps:\n      - run: true\n";
    let r = read(credited);
    assert!(
        r.refusals
            .iter()
            .any(|m| m.contains("not a canonical job key")),
        "{:#?}",
        r.refusals
    );
    assert_eq!(r.jobs.len(), 1, "{:?}", r.jobs);
    assert_eq!(r.jobs.first().map(|j| j.name.as_str()), Some("b"));
    // And in the canonical shape the job without one is named, `b`'s is not credited to it.
    let bare_then_timed = "jobs:\n  a:\n    steps:\n      - run: true\n  b:\n    timeout-minutes: 5\n    steps:\n      - run: true\n";
    assert_eq!(
        jobs_without_timeout(bare_then_timed),
        vec![(2, "a".to_owned())]
    );
    assert!(
        read(bare_then_timed)
            .refusals
            .iter()
            .any(|m| m.contains("job `a` has no `timeout-minutes"))
    );
    // Other shapes of the same family.
    refused(
        "a quoted job key",
        good.replace("  build:\n", "  \"build\":\n"),
    );
    refused(
        "an upper-case job key",
        good.replace("  build:\n", "  Build:\n"),
    );
    refused(
        "a job key at one space",
        good.replace("  build:\n", " build:\n"),
    );
    refused(
        "a job key with a space before its colon",
        good.replace("  build:\n", "  build :\n"),
    );
    refused(
        "a tab in the indentation",
        good.replace("    runs-on", "\truns-on"),
    );
    refused(
        "a timeout after `steps:`",
        "jobs:\n  a:\n    steps:\n      - run: true\n    timeout-minutes: 5\n".to_owned(),
    );
    refused(
        "a timeout that is not a number",
        job("      - run: true\n")
            .replace("timeout-minutes: 5", "timeout-minutes: ${{ inputs.t }}"),
    );
    refused(
        "a timeout of zero",
        job("      - run: true\n").replace("timeout-minutes: 5", "timeout-minutes: 0"),
    );
    refused(
        "two timeouts",
        job("      - run: true\n").replace(
            "    timeout-minutes: 5\n",
            "    timeout-minutes: 5\n    timeout-minutes: 6\n",
        ),
    );
    refused(
        "a timeout on a step only",
        "jobs:\n  a:\n    steps:\n      - run: true\n        timeout-minutes: 5\n".to_owned(),
    );
    refused(
        "a job without steps",
        "jobs:\n  a:\n    timeout-minutes: 5\n".to_owned(),
    );
    refused("no jobs at all", "name: x\n".to_owned());
    refused(
        "two `jobs:`",
        format!("{good}jobs:\n  c:\n    timeout-minutes: 5\n    steps:\n      - run: true\n"),
    );
    // A rustup line that is not one of the forms, in a block: another version, another flag, a continued
    // command, a variable that overrides the toolchain, the pin's install without its components.
    for (what, line) in [
        (
            "another version",
            "rustup toolchain install 1.99.0 --profile minimal --component rustfmt --component clippy --no-self-update",
        ),
        (
            "a channel",
            "rustup toolchain install stable --profile minimal --component rustfmt --component clippy --no-self-update",
        ),
        (
            "no components",
            "rustup toolchain install 1.99.1 --profile minimal --no-self-update",
        ),
        (
            "a continued command",
            "rustup toolchain install 1.99.1 --profile minimal \\",
        ),
        ("a trailing comment", "rustup default 1.99.1 # now"),
        ("`rustup default` of a channel", "rustup default stable"),
        ("`rustup override`", "rustup override set nightly"),
        (
            "the environment override",
            "export RUSTUP_TOOLCHAIN=nightly",
        ),
        ("`rustup update`", "rustup update"),
    ] {
        refused(what, job(&format!("      - run: |\n          {line}\n")));
    }
    // A rustup line in a `run: |-` block, a folded block, an `env:` value or a name is outside a block `run: |`.
    refused(
        "`run: |-`",
        job(&format!("      - run: |-\n          {install}\n")),
    );
    refused(
        "a folded block",
        job(&format!("      - run: >\n          {install}\n")),
    );
    refused(
        "a plain multi-line scalar",
        job(&format!("      - run:\n          {install}\n")),
    );
    refused(
        "an env value",
        job("      - env:\n          X: rustup toolchain install 1.99.1\n        run: true\n"),
    );
    refused("an install outside any job", format!("{install}\n{good}"));
    // The same install is read when it is in a canonical block, in either list form.
    let ok_block = job(&format!("      - run: |\n          {install}\n"));
    assert!(
        read(&ok_block).refusals.is_empty(),
        "{:?}",
        read(&ok_block).refusals
    );
    assert_eq!(read(&ok_block).installs.len(), 1);
    let ok_named = job(&format!(
        "      - name: t\n        run: |\n          {install}\n"
    ));
    assert_eq!(read(&ok_named).installs.len(), 1);
    // The shimmed run's two lines are canonical, exactly.
    let shim = job(
        "      - run: |\n          for tool in cargo rustc rustdoc; do ln -s \"$(rustup which \"$tool\")\" \"$SHIM/$tool\"; done\n          ln -s \"$(command -v rustup)\" \"$SHIM/rustup\"\n",
    );
    assert!(
        read(&shim).refusals.is_empty(),
        "{:?}",
        read(&shim).refusals
    );
    refused(
        "a shim line with more",
        job("      - run: |\n          ln -s \"$(command -v rustup)\" \"$SHIM/rustup\" && true\n"),
    );
    // A comment that mentions rustup is not a command; a trailing comment on a command is.
    assert!(
        read(&job(
            "      # rustup default is not what we use\n      - run: true\n"
        ))
        .refusals
        .is_empty()
    );
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

/// Maintenance M42 (review result E-c): `cargo +X` and `rustc +X` name a compiler without a `rustup` line, so the
/// canonical reader never sees it. They are allowed in the `msrv` job and only there, and only with the minimum.
#[test]
fn a_plus_toolchain_stands_only_in_the_msrv_job_and_names_only_the_minimum() {
    let msrv =
        declared_msrv(&fs::read_to_string(workspace_root().join("Cargo.toml")).unwrap_or_default())
            .expect("rust-version declared");
    for (name, yaml) in workflows() {
        let found = plus_toolchains_outside_the_minimum(&yaml, &msrv);
        assert!(
            found.is_empty(),
            "{name}: a `+<toolchain>` outside the msrv job, or one that is not +{msrv}: {found:?}"
        );
    }
    // Not vacuous: the real msrv job names the minimum, and the rule reads its lines.
    let ci =
        fs::read_to_string(workspace_root().join(".github/workflows/ci.yml")).unwrap_or_default();
    assert!(ci.contains(&format!("cargo +{msrv} check")));
    let planted = ci.replace(&format!("cargo +{msrv} check"), "cargo +nightly check");
    assert_eq!(
        plus_toolchains_outside_the_minimum(&planted, &msrv).len(),
        1
    );

    let wrong = [
        (
            "`cargo +nightly fmt` in the build job",
            "jobs:\n  build:\n    steps:\n      - name: f\n        run: cargo +nightly fmt --all\n",
            1,
        ),
        (
            "`rustc +1.99.1` in a job that is not the msrv job (the pin is the toolchain file's, not a `+X`)",
            "jobs:\n  build:\n    steps:\n      - name: v\n        run: |\n          rustc +1.99.1 --version\n",
            1,
        ),
        (
            "the minimum itself, outside the msrv job",
            "jobs:\n  gates:\n    steps:\n      - name: c\n        run: cargo +1.89.0 check\n",
            1,
        ),
        (
            "another compiler in the msrv job",
            "jobs:\n  msrv:\n    steps:\n      - name: c\n        run: cargo +1.88.0 check\n      - name: d\n        run: cargo +nightly doc\n",
            2,
        ),
        (
            "after `&&`, in a block",
            "jobs:\n  build:\n    steps:\n      - name: a\n        run: |\n          cd x && cargo +beta test\n",
            1,
        ),
        (
            "in a release job",
            "jobs:\n  binaries:\n    steps:\n      - name: b\n        run: cargo +stable build --release --locked\n",
            1,
        ),
    ];
    for (what, yaml, count) in wrong {
        let found = plus_toolchains_outside_the_minimum(yaml, "1.89.0");
        assert_eq!(found.len(), count, "{what}: {found:?}");
    }
    let right = [
        "jobs:\n  msrv:\n    steps:\n      - name: t\n        run: |\n          rustc +1.89.0 --version\n      - name: c\n        run: cargo +1.89.0 check --workspace\n",
        // No `+` after the command, a comment, and a `+` that is not a toolchain.
        "jobs:\n  build:\n    steps:\n      - name: b\n        run: cargo test --workspace # not cargo +nightly\n      # cargo +nightly fmt\n      - name: c\n        run: echo 1 + 2\n",
    ];
    for yaml in right {
        let found = plus_toolchains_outside_the_minimum(yaml, "1.89.0");
        assert!(found.is_empty(), "{yaml}: {found:?}");
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

/// The attest steps of a workflow that attest an image (a `subject-name`).
fn image_attestations(y: &str) -> Vec<Step> {
    steps(y)
        .into_iter()
        .filter(|s| s.text.contains("actions/attest@") && s.text.contains("subject-name:"))
        .collect()
}

#[test]
fn the_joined_image_tag_is_attested_in_the_tag_run_and_in_rehearsal() {
    let y = fs::read_to_string(workspace_root().join(".github/workflows/release.yml"))
        .unwrap_or_default();
    let all = image_attestations(&y);
    // The joined tag's attestation (M59): the manifest list's digest from the join step, pushed to the registry,
    // under the tag-push guard for the real image and under the rehearsal guard for the throwaway package only.
    let joined = |guard: &str, image: &str, id: &str| {
        all.iter().any(|s| {
            s.guards.iter().any(|g| g == guard)
                && s.text
                    .lines()
                    .any(|l| l.trim() == format!("subject-name: {image}"))
                && s.text.contains(&format!("steps.{id}.outputs.digest"))
                && s.text.lines().any(|l| l.trim() == "push-to-registry: true")
        })
    };
    assert!(
        joined(PUSH_GUARD, REAL_IMAGE, "join"),
        "the joined real tag is not attested"
    );
    assert!(
        joined(REHEARSAL_GUARD, REHEARSAL_IMAGE, "join_rehearsal"),
        "the rehearsal does not run the joined image's attestation"
    );
    // No attestation under the rehearsal guard names the real image (the general rule, stated for attestations).
    for s in all
        .iter()
        .filter(|s| s.guards.iter().any(|g| g == REHEARSAL_GUARD))
    {
        assert!(
            !names_real_image(&s.text),
            "a rehearsal attestation names the real image"
        );
    }
    // The job that does it holds the permissions an attestation needs.
    let job = y
        .split("\n  manifest:")
        .nth(1)
        .and_then(|t| t.split("\n  release:").next())
        .unwrap_or_default();
    for need in ["packages: write", "id-token: write", "attestations: write"] {
        assert!(job.contains(need), "the manifest job lacks `{need}`");
    }
    // The planted failure: a rehearsal attestation of the real image is refused; of the throwaway package, not.
    let bad = "jobs:\n  manifest:\n    steps:\n      - name: attest\n        if: inputs.mode == 'rehearsal'\n        uses: actions/attest@1e69f48acb82d1966a394da916b4c1698aa569d6 # v4.2.2\n        with:\n          subject-name: ghcr.io/prikk-vcs/inklkunn\n";
    assert!(
        !rehearsal_violations(bad).is_empty(),
        "a rehearsal attestation of the real image must be refused"
    );
    let good = "jobs:\n  manifest:\n    steps:\n      - name: attest\n        if: inputs.mode == 'rehearsal'\n        uses: actions/attest@1e69f48acb82d1966a394da916b4c1698aa569d6 # v4.2.2\n        with:\n          subject-name: ghcr.io/prikk-vcs/inklkunn-rehearsal\n";
    assert!(rehearsal_violations(good).is_empty());
}
