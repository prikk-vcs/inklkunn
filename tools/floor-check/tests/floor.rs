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
fn the_test_only_prikk_is_one_value_in_both_workflows_and_is_never_a_floor() {
    let root = workspace_root();
    let read = |p: &str| fs::read_to_string(root.join(p)).unwrap_or_default();
    let floor = driver_floor(&read("crates/inklkunn-prikk/src/lib.rs")).expect("MIN_PRIKK_VERSION");
    let ci = read(".github/workflows/ci.yml");
    let release = read(".github/workflows/release.yml");
    let (ci_version, ci_sum) = test_prikk(&ci).expect("ci.yml records the test-only prikk");
    let (release_version, release_sum) =
        test_prikk(&release).expect("release.yml records the test-only prikk");
    assert_eq!(ci_version, release_version, "the test-only version differs");
    assert_eq!(ci_sum, release_sum, "the test-only checksum differs");
    // The tests it serves say `0.57.0 or newer`; it is another prikk than the floor, and newer.
    let parse = |v: &str| -> Vec<u32> { v.split('.').filter_map(|p| p.parse().ok()).collect() };
    assert!(
        parse(&ci_version) >= vec![0, 57, 0],
        "the test-only prikk is older than the 0.57.0 the tests need: {ci_version}"
    );
    assert_ne!(
        ci_version, floor,
        "the test-only prikk is the floor: it is a second pin"
    );
    // It is not read as a floor, and not recorded as a floor's checksum, anywhere.
    for (name, yaml) in [("ci.yml", &ci), ("release.yml", &release)] {
        assert!(
            workflow_floors(yaml).iter().all(|f| *f == floor),
            "{name}: a PRIKK_VERSION that is not the floor"
        );
        assert!(
            !recorded_checksums(yaml).values().any(|s| *s == ci_sum),
            "{name}: the test-only checksum is recorded as a floor's"
        );
        let v = test_prikk_shape_violations(yaml);
        assert!(v.is_empty(), "{name}: {v:#?}");
    }
    // The image carries the floor only.
    let docker = read("Dockerfile");
    assert!(!docker.contains(&ci_version) && !docker.contains(&ci_sum));
}

#[test]
fn the_test_only_prikk_shape_rules_catch_planted_failures() {
    let ok = "env:\n  PRIKK_TEST_VERSION: \"0.57.0\"\n  PRIKK_TEST_SHA256_X86_64: \"4e621147c3064c1f456803c67f120df9ea35d93e40554e920c9ea55e69d3f9a8\"\nsteps:\n  curl \"releases/download/${PRIKK_TEST_VERSION}/prikk\"\n  echo \"${PRIKK_TEST_SHA256_X86_64}  x\" | sha256sum -c -\n  curl \"releases/download/${PRIKK_VERSION}/prikk\"\n  cargo test --workspace --locked --no-fail-fast -- --nocapture 2>&1 | tee with-prikk.log\n  if grep -n -E 'skipping(: (prikk 0\\.57\\.0 or newer not found|no prikk older than 0\\.57\\.0|PID namespaces unavailable|prikk not on PATH|bubblewrap cannot confine|bwrap or curl not available|inklkunn 0\\.2\\.0 not found)| the confined policy)' with-prikk.log; then\n    echo \"skipped\" >&2\n    exit 1\n  fi\n  SYSTEM_PATH=\"$(echo \"$PATH\" | grep -v -x -e \"$HOME/.local/bin\" -e \"$HOME/.local/prikk-test/bin\")\"\n  cargo test --workspace --locked --no-fail-fast -- --nocapture 2>&1 | tee shimmed.log\n";
    assert!(test_prikk_shape_violations(ok).is_empty());
    assert_eq!(
        test_prikk(ok),
        Some((
            "0.57.0".into(),
            "4e621147c3064c1f456803c67f120df9ea35d93e40554e920c9ea55e69d3f9a8".into()
        ))
    );
    let broken = |from: &str, to: &str| ok.replacen(from, to, 1);
    for (what, yaml) in [
        ("not recorded", broken("PRIKK_TEST_VERSION: \"0.57.0\"", "")),
        (
            "a checksum that is not a checksum",
            broken("4e621147c3064c1f", "zzzzzzzzzzzzzzzz"),
        ),
        (
            "installed after the floor",
            broken(
                "releases/download/${PRIKK_TEST_VERSION}/",
                "releases/download/${SWAP}/",
            )
            .replacen(
                "releases/download/${PRIKK_VERSION}/",
                "releases/download/${PRIKK_TEST_VERSION}/",
                1,
            )
            .replacen(
                "releases/download/${SWAP}/",
                "releases/download/${PRIKK_VERSION}/",
                1,
            ),
        ),
        ("not checked", broken("sha256sum -c", "cat")),
        ("output not shown", broken(" --nocapture", "")),
        (
            "the first red stops the workspace's tests",
            broken(" --no-fail-fast", ""),
        ),
        (
            "the shimmed run's first red stops the workspace's tests",
            broken(
                "--no-fail-fast -- --nocapture 2>&1 | tee shimmed.log",
                "-- --nocapture 2>&1 | tee shimmed.log",
            ),
        ),
        (
            "output not kept",
            broken("tee with-prikk.log", "tee other.log"),
        ),
        (
            "only one skip line looked for",
            broken("|no prikk older than 0\\.57\\.0", ""),
        ),
        (
            "the PID-namespace skip line is not looked for",
            broken("|PID namespaces unavailable", ""),
        ),
        (
            "the silent prikk skip line is not looked for",
            broken("|prikk not on PATH", ""),
        ),
        (
            "bubblewrap's skip line is not looked for",
            broken("|bubblewrap cannot confine", ""),
        ),
        (
            "the confined policy's skip line is not looked for",
            broken("| the confined policy", ""),
        ),
        (
            "the real vector's skip line is not looked for",
            broken("|bwrap or curl not available", ""),
        ),
        (
            "the upgrade test's skip line is not looked for",
            broken("|inklkunn 0\\.2\\.0 not found", ""),
        ),
        ("a skip does not fail", broken("exit 1", "true")),
        (
            "the shim keeps the test-only directory",
            broken(" -e \"$HOME/.local/prikk-test/bin\"", ""),
        ),
    ] {
        assert!(
            !test_prikk_shape_violations(&yaml).is_empty(),
            "{what}: no violation reported"
        );
    }
}

#[test]
fn the_release_the_upgrade_test_starts_from_is_one_pinned_asset_in_both_workflows() {
    let root = workspace_root();
    let read = |p: &str| fs::read_to_string(root.join(p)).unwrap_or_default();
    let ci = read(".github/workflows/ci.yml");
    let release = read(".github/workflows/release.yml");
    let (ci_version, ci_sum) = upgrade_from(&ci).expect("ci.yml records the upgrade-from release");
    let (release_version, release_sum) =
        upgrade_from(&release).expect("release.yml records the upgrade-from release");
    assert_eq!(
        ci_version, release_version,
        "the upgrade-from version differs"
    );
    assert_eq!(ci_sum, release_sum, "the upgrade-from checksum differs");
    // The test looks for the 0.2.0 release (by the commit it was built from); the workflow installs that one.
    assert_eq!(ci_version, "0.2.0");
    // It is an inklkunn release: not a prikk, and not recorded as one's checksum anywhere.
    for (name, yaml) in [("ci.yml", &ci), ("release.yml", &release)] {
        assert!(
            !recorded_checksums(yaml).values().any(|s| *s == ci_sum),
            "{name}: the upgrade-from checksum is recorded as a prikk's"
        );
        assert!(
            test_prikk(yaml).is_some_and(|(_, s)| s != ci_sum),
            "{name}: the upgrade-from checksum is the test-only prikk's"
        );
        let v = upgrade_from_shape_violations(yaml);
        assert!(v.is_empty(), "{name}: {v:#?}");
    }
    // The image carries neither.
    assert!(!read("Dockerfile").contains(&ci_sum));
}

#[test]
fn the_upgrade_from_shape_rules_catch_planted_failures() {
    let ok = "env:\n  UPGRADE_FROM_VERSION: \"0.2.0\"\n  UPGRADE_FROM_SHA256_X86_64: \"e29814e026390ee8b46b744f862977efca53e5ec025af7ef8e11255b043bff86\"\nsteps:\n  curl -fsSL -o \"old.tar.gz\" \"https://github.com/prikk-vcs/inklkunn/releases/download/${UPGRADE_FROM_VERSION}/inklkunn-${T}.tar.gz\"\n  echo \"${UPGRADE_FROM_SHA256_X86_64}  old.tar.gz\" | sha256sum -c -\n  tar -xzf \"old.tar.gz\" -C \"$HOME/.local/inklkunn-old/bin\" inklkunn\n  echo \"$HOME/.local/inklkunn-old/bin\" >> \"$GITHUB_PATH\"\n";
    assert!(upgrade_from_shape_violations(ok).is_empty());
    assert_eq!(
        upgrade_from(ok),
        Some((
            "0.2.0".into(),
            "e29814e026390ee8b46b744f862977efca53e5ec025af7ef8e11255b043bff86".into()
        ))
    );
    let broken = |from: &str, to: &str| ok.replacen(from, to, 1);
    for (what, yaml) in [
        (
            "not recorded",
            broken("UPGRADE_FROM_VERSION: \"0.2.0\"", ""),
        ),
        (
            "a checksum that is not a checksum",
            broken("e29814e026390ee8", "zzzzzzzzzzzzzzzz"),
        ),
        (
            "not downloaded from this project's release",
            broken("prikk-vcs/inklkunn/releases", "someone/else/releases"),
        ),
        ("not checked", broken("sha256sum -c -", "cat")),
        (
            "checked against another value",
            broken("${UPGRADE_FROM_SHA256_X86_64}  old", "${SOMETHING}  old"),
        ),
        (
            "checked after it is unpacked",
            broken(
                "  echo \"${UPGRADE_FROM_SHA256_X86_64}  old.tar.gz\" | sha256sum -c -\n  tar -xzf \"old.tar.gz\" -C \"$HOME/.local/inklkunn-old/bin\" inklkunn\n",
                "  tar -xzf \"old.tar.gz\" -C \"$HOME/.local/inklkunn-old/bin\" inklkunn\n  echo \"${UPGRADE_FROM_SHA256_X86_64}  old.tar.gz\" | sha256sum -c -\n",
            ),
        ),
        ("not on PATH", broken(">> \"$GITHUB_PATH\"", "> /dev/null")),
    ] {
        assert!(
            !upgrade_from_shape_violations(&yaml).is_empty(),
            "{what}: no violation reported"
        );
    }
}

#[test]
fn both_workflows_lift_the_user_namespace_restriction_before_their_tests() {
    let root = workspace_root();
    for wf in [".github/workflows/ci.yml", ".github/workflows/release.yml"] {
        let yaml = fs::read_to_string(root.join(wf)).unwrap_or_default();
        let v = user_namespace_violations(&yaml);
        assert!(v.is_empty(), "{wf}: {v:#?}");
    }
}

#[test]
fn the_user_namespace_rule_catches_planted_failures() {
    let lift = "sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0";
    let job = |name: &str, head: &str, steps: &str| {
        format!("  {name}:\n    runs-on: ubuntu-24.04\n{head}    steps:\n{steps}")
    };
    let lift_step = format!("      - name: x\n        run: {lift}\n");
    let test_step = "      - run: cargo test --workspace\n";
    let ok = format!(
        "jobs:\n{}",
        job("a", "", &format!("{lift_step}{test_step}"))
    );
    assert!(user_namespace_violations(&ok).is_empty());
    // A lift and a test on one step, the lift first, is a lift; the test first is not.
    let one_step = format!(
        "jobs:\n{}",
        job(
            "a",
            "",
            &format!("      - name: t\n        run: |\n          {lift}\n          cargo test\n")
        )
    );
    assert!(user_namespace_violations(&one_step).is_empty());
    assert!(
        !user_namespace_violations(&one_step.replace(
            &format!("          {lift}\n          cargo test\n"),
            &format!("          cargo test\n          {lift}\n")
        ))
        .is_empty(),
        "a lift after the test on the same step"
    );
    // The same guard on both is shared; a job-level guard is shared by its steps.
    let both_guarded = format!(
        "jobs:\n{}",
        job(
            "a",
            "",
            &format!(
                "      - name: x\n        if: github.event_name == 'push'\n        run: {lift}\n      - if: ${{{{ github.event_name == 'push' }}}}\n        run: cargo test\n"
            )
        )
    );
    assert!(
        user_namespace_violations(&both_guarded).is_empty(),
        "{:?}",
        user_namespace_violations(&both_guarded)
    );
    let job_guarded = format!(
        "jobs:\n{}",
        job(
            "a",
            "    if: github.event_name == 'push'\n",
            &format!("{lift_step}{test_step}")
        )
    );
    assert!(user_namespace_violations(&job_guarded).is_empty());
    // A lift under an `if:` that the test does not share is no lift (the job-level and the step-level kind).
    let lift_guarded = format!(
        "jobs:\n{}",
        job(
            "a",
            "",
            &format!(
                "      - name: x\n        if: inputs.mode == 'rehearsal'\n        run: {lift}\n{test_step}"
            )
        )
    );
    // An unconditional lift covers a test that runs only under a condition.
    let test_guarded_more = format!(
        "jobs:\n{}",
        job(
            "a",
            "",
            &format!(
                "{lift_step}      - if: inputs.mode == 'rehearsal'\n        run: cargo test\n"
            )
        )
    );
    assert!(user_namespace_violations(&test_guarded_more).is_empty());
    // The lift of ANOTHER job does not count: it is the first job that lifts, the second that tests.
    let other_job = format!(
        "jobs:\n{}{}",
        job("a", "", &lift_step),
        job("b", "", test_step)
    );
    // The second job's own lift passes, the first job's test without one does not: one violation, and it
    // names the job that is wrong.
    let one_of_two = format!(
        "jobs:\n{}{}",
        job("a", "", test_step),
        job("b", "", &format!("{lift_step}{test_step}"))
    );
    let v = user_namespace_violations(&one_of_two);
    assert!(v.len() == 1 && v.iter().all(|m| m.contains("`a`")), "{v:?}");
    // M42 (E-d 1): a folded `if: >-` is read whole. Two guards of one first line are not one guard; the same
    // folded guard on the lift and the test is shared.
    let folded = |lift_if: &str, test_if: &str| {
        format!(
            "jobs:\n{}",
            job(
                "a",
                "",
                &format!(
                    "      - name: x\n        if: >-\n          {lift_if}\n        run: {lift}\n      - name: t\n        if: >-\n          {test_if}\n        run: cargo test\n"
                )
            )
        )
    };
    let same = folded("github.event_name == 'push'", "github.event_name == 'push'");
    assert!(
        user_namespace_violations(&same).is_empty(),
        "{:?}",
        user_namespace_violations(&same)
    );
    // The same words over two lines are the same guard (white space does not matter).
    let two_lines = format!(
        "jobs:\n{}",
        job(
            "a",
            "",
            &format!(
                "      - name: x\n        if: >-\n          github.event_name == 'push' ||\n          inputs.mode == 'rehearsal'\n        run: {lift}\n      - name: t\n        if: github.event_name == 'push' || inputs.mode == 'rehearsal'\n        run: cargo test\n"
            )
        )
    );
    assert!(
        user_namespace_violations(&two_lines).is_empty(),
        "{:?}",
        user_namespace_violations(&two_lines)
    );
    // A lift that fails without failing the job is no lift; `false` is the default and is fine.
    let optional = |value: &str| {
        format!(
            "jobs:\n{}",
            job(
                "a",
                "",
                &format!(
                    "      - name: x\n        continue-on-error: {value}\n        run: {lift}\n{test_step}"
                )
            )
        )
    };
    assert!(user_namespace_violations(&optional("false")).is_empty());
    // A test that is `cargo +<toolchain> test` is a test: the job that has one needs a lift too.
    let plus_test = format!(
        "jobs:\n{}{}",
        job("a", "", &format!("{lift_step}{test_step}")),
        job("b", "", "      - run: cargo +1.99.1 test --workspace\n")
    );
    let v = user_namespace_violations(&plus_test);
    assert!(v.len() == 1 && v.iter().all(|m| m.contains("`b`")), "{v:?}");
    assert!(
        user_namespace_violations(&format!(
            "jobs:\n{}",
            job(
                "a",
                "",
                &format!("{lift_step}      - run: cargo +1.99.1 test --workspace\n")
            )
        ))
        .is_empty()
    );
    for (what, yaml) in [
        (
            "folded guards that share a first line only",
            folded(
                "github.event_name == 'push'",
                "github.event_name == 'push' || inputs.mode == 'rehearsal'",
            ),
        ),
        (
            "folded guards whose first line is the same indicator and whose text differs",
            folded("github.event_name == 'push'", "inputs.mode == 'rehearsal'"),
        ),
        ("a lift with continue-on-error", optional("true")),
        (
            "a lift with a continue-on-error that is an expression",
            optional("${{ github.event_name == 'push' }}"),
        ),
        (
            "`cargo +1.99.1 test` with no lift",
            format!(
                "jobs:\n{}",
                job("a", "", "      - run: cargo +1.99.1 test --workspace\n")
            ),
        ),
        ("no lift", ok.replace(lift, "true")),
        (
            "lifted too late",
            format!(
                "jobs:\n{}",
                job("a", "", &format!("{test_step}{lift_step}"))
            ),
        ),
        (
            "the restriction set instead of lifted",
            ok.replace("userns=0", "userns=1"),
        ),
        (
            "the lift made optional",
            ok.replace(lift, &format!("{lift} || true")),
        ),
        (
            "no test at all",
            format!("jobs:\n{}", job("a", "", &lift_step)),
        ),
        (
            "a lift under a condition the test does not share",
            lift_guarded,
        ),
        ("a lift in another job", other_job),
        ("one job of two without a lift", one_of_two),
    ] {
        assert!(
            !user_namespace_violations(&yaml).is_empty(),
            "{what}: no violation reported"
        );
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
