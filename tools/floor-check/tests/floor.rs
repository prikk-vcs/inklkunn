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
    let ok = "env:\n  PRIKK_TEST_VERSION: \"0.58.0\"\n  PRIKK_TEST_SHA256_X86_64: \"0462b9a590d0b945bb6266ce09983055768cb564415837bdb8ed462b5aa10dfe\"\nsteps:\n  curl \"releases/download/${PRIKK_TEST_VERSION}/prikk\"\n  echo \"${PRIKK_TEST_SHA256_X86_64}  x\" | sha256sum -c -\n  curl \"releases/download/${PRIKK_VERSION}/prikk\"\n  cargo test --workspace --locked --no-fail-fast -- --nocapture 2>&1 | tee with-prikk.log\n  if grep -n -E 'skipping(: (prikk 0\\.57\\.0 or newer not found|no prikk older than 0\\.57\\.0|PID namespaces unavailable|prikk not on PATH|bubblewrap cannot confine|bwrap or curl not available|inklkunn 0\\.2\\.0 not found|inklkunn 0\\.3\\.0 not found|inklkunn 0\\.4\\.0 not found)| the confined policy)' with-prikk.log; then\n    echo \"skipped\" >&2\n    exit 1\n  fi\n  SYSTEM_PATH=\"$(echo \"$PATH\" | grep -v -x -e \"$HOME/.local/bin\" -e \"$HOME/.local/prikk-test/bin\")\"\n  cargo test --workspace --locked --no-fail-fast -- --nocapture 2>&1 | tee shimmed.log\n";
    assert!(test_prikk_shape_violations(ok).is_empty());
    assert_eq!(
        test_prikk(ok),
        Some((
            "0.58.0".into(),
            "0462b9a590d0b945bb6266ce09983055768cb564415837bdb8ed462b5aa10dfe".into()
        ))
    );
    let broken = |from: &str, to: &str| ok.replacen(from, to, 1);
    for (what, yaml) in [
        ("not recorded", broken("PRIKK_TEST_VERSION: \"0.58.0\"", "")),
        (
            "a checksum that is not a checksum",
            broken("0462b9a590d0b945", "zzzzzzzzzzzzzzzz"),
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
        (
            "the 0.3.0 upgrade test's skip line is not looked for",
            broken("|inklkunn 0\\.3\\.0 not found", ""),
        ),
        (
            "the 0.4.0 upgrade test's skip line is not looked for",
            broken("|inklkunn 0\\.4\\.0 not found", ""),
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
fn the_releases_the_upgrade_tests_start_from_are_pinned_assets_held_equal_in_both_workflows() {
    let root = workspace_root();
    let read = |p: &str| fs::read_to_string(root.join(p)).unwrap_or_default();
    let ci = read(".github/workflows/ci.yml");
    let release = read(".github/workflows/release.yml");
    let recorded = upgrade_from(&ci);
    assert_eq!(
        recorded,
        upgrade_from(&release),
        "the upgrade-from releases or their checksums differ between ci.yml and release.yml"
    );
    // The tests look for the 0.2.0, the 0.3.0 and the 0.4.0 releases (each by the commit it was built from); the workflows
    // install those three, in this order, and no other.
    let versions: Vec<&str> = recorded.iter().map(|(v, _)| v.as_str()).collect();
    assert_eq!(versions, ["0.2.0", "0.3.0", "0.4.0"]);
    // The 0.3.0 asset is the one the release recorded (GitHub's digest of `inklkunn-x86_64-unknown-linux-gnu.tar.gz`).
    assert_eq!(
        recorded.get(1).map(|(_, sum)| sum.as_str()),
        Some("b6f6726c2d2fd21c6b420dedab5b7ffb301afcf2f50946a5aed221c2feb84acb")
    );
    // The 0.4.0 asset is the one recorded at its release (built from commit 162f0cc).
    assert_eq!(
        recorded.get(2).map(|(_, sum)| sum.as_str()),
        Some("277e7962612fe07ec380ba414ea3f8521397f6eb54499aaeed38eba3fd5b01f6")
    );
    // They are inklkunn releases: not a prikk, not recorded as one's checksum anywhere, and not each other's.
    let sums: Vec<&str> = recorded.iter().map(|(_, s)| s.as_str()).collect();
    assert_ne!(sums.first(), sums.get(1), "two releases, one checksum");
    assert_ne!(sums.get(1), sums.get(2), "two releases, one checksum");
    assert_ne!(sums.first(), sums.get(2), "two releases, one checksum");
    for (name, yaml) in [("ci.yml", &ci), ("release.yml", &release)] {
        for sum in &sums {
            assert!(
                !recorded_checksums(yaml).values().any(|s| s == sum),
                "{name}: an upgrade-from checksum is recorded as a prikk's"
            );
            assert!(
                test_prikk(yaml).is_some_and(|(_, s)| s != *sum),
                "{name}: an upgrade-from checksum is the test-only prikk's"
            );
        }
        let v = upgrade_from_shape_violations(yaml);
        assert!(v.is_empty(), "{name}: {v:#?}");
    }
    // The image carries none of them.
    for sum in &sums {
        assert!(!read("Dockerfile").contains(sum));
    }
}

/// A workflow's relevant lines for the releases `versions`, as the real ones are shaped.
fn upgrade_workflow(versions: &[(&str, &str)]) -> String {
    let mut out = String::from("env:\n");
    for (version, sum) in versions {
        out.push_str(&format!(
            "  {}: \"{sum}\"\n",
            upgrade_from_variable(version)
        ));
    }
    out.push_str("steps:\n");
    for (version, _) in versions {
        let variable = upgrade_from_variable(version);
        out.push_str(&format!(
            "  curl -fsSL -o \"old.tar.gz\" \"https://github.com/prikk-vcs/inklkunn/releases/download/{version}/inklkunn-${{T}}.tar.gz\"\n\
             \x20 echo \"${{{variable}}}  old.tar.gz\" | sha256sum -c -\n\
             \x20 mkdir -p \"$HOME/.local/inklkunn-{version}/bin\" && tar -xzf \"old.tar.gz\" -C \"$HOME/.local/inklkunn-{version}/bin\" inklkunn\n\
             \x20 test \"${{V#inklkunn {version} (commit }}\" != \"$V\" || {{ echo \"the asset is not inklkunn {version}: $V\" >&2; exit 1; }}\n\
             \x20 echo \"$HOME/.local/inklkunn-{version}/bin\" >> \"$GITHUB_PATH\"\n"
        ));
    }
    let skips: Vec<String> = versions
        .iter()
        .map(|(v, _)| format!("inklkunn {} not found", v.replace('.', "\\.")))
        .collect();
    out.push_str(&format!(
        "  if grep -n -E 'skipping(: ({}))' with-prikk.log; then\n",
        skips.join("|")
    ));
    out
}

const SUM_A: &str = "e29814e026390ee8b46b744f862977efca53e5ec025af7ef8e11255b043bff86";
const SUM_B: &str = "b6f6726c2d2fd21c6b420dedab5b7ffb301afcf2f50946a5aed221c2feb84acb";
const SUM_C: &str = "277e7962612fe07ec380ba414ea3f8521397f6eb54499aaeed38eba3fd5b01f6";

#[test]
fn the_upgrade_from_shape_rules_catch_planted_failures() {
    let ok = upgrade_workflow(&[("0.2.0", SUM_A), ("0.3.0", SUM_B), ("0.4.0", SUM_C)]);
    assert!(upgrade_from_shape_violations(&ok).is_empty());
    assert_eq!(
        upgrade_from(&ok),
        [
            ("0.2.0".to_owned(), SUM_A.to_owned()),
            ("0.3.0".to_owned(), SUM_B.to_owned()),
            ("0.4.0".to_owned(), SUM_C.to_owned())
        ]
    );
    // A later release is held to the same rules without a rule being written for it.
    let four = upgrade_workflow(&[
        ("0.2.0", SUM_A),
        ("0.3.0", SUM_B),
        ("0.4.0", SUM_C),
        ("0.5.0", SUM_A),
    ]);
    assert!(upgrade_from_shape_violations(&four).is_empty());
    // The name carries the version, exactly: the old single pair, a name that is not a version and a part that is not a
    // number are not releases.
    assert!(
        upgrade_from(
            "  UPGRADE_FROM_VERSION: \"0.2.0\"\n  UPGRADE_FROM_SHA256_X86_64: \"abc\"\n  UPGRADE_FROM_0_3_SHA256_X86_64: \"abc\"\n  UPGRADE_FROM_0_x_1_SHA256_X86_64: \"abc\"\n"
        )
        .is_empty()
    );
    let broken = |from: &str, to: &str| {
        assert!(ok.contains(from), "{from}");
        ok.replacen(from, to, 1)
    };
    // Each is planted against each of the two later releases in turn (the 0.3.0 one, which the rules were first written
    // for, and the 0.4.0 one), then the rules are run on the whole; the first release stands for "another release".
    for (second, sum) in [("0.3.0", SUM_B), ("0.4.0", SUM_C)] {
        let variable = upgrade_from_variable(second);
        let other = upgrade_from_variable("0.2.0");
        let dir = format!("\"$HOME/.local/inklkunn-{second}/bin\"");
        let download = format!("prikk-vcs/inklkunn/releases/download/{second}");
        for (what, yaml) in [
            (
                "no release recorded",
                ok.replace("UPGRADE_FROM_", "SOMETHING_ELSE_"),
            ),
            (
                "a checksum that is not a checksum",
                broken(sum.get(..16).unwrap_or_default(), "zzzzzzzzzzzzzzzz"),
            ),
            (
                "a checksum of the wrong length",
                broken(sum, sum.get(1..).unwrap_or_default()),
            ),
            (
                "not downloaded from this project's release",
                broken(
                    &download,
                    &format!("someone/else/releases/download/{second}"),
                ),
            ),
            (
                "downloaded from another release than the one it is checked as",
                broken(&download, "prikk-vcs/inklkunn/releases/download/9.9.9"),
            ),
            (
                "not checked",
                ok.replacen(
                    &format!("${{{variable}}}  old.tar.gz\" | sha256sum -c -"),
                    &format!("${{{variable}}}  old.tar.gz\" | cat"),
                    1,
                ),
            ),
            (
                "checked against another release's checksum",
                broken(
                    &format!("${{{variable}}}  old"),
                    &format!("${{{other}}}  old"),
                ),
            ),
            (
                "checked against another value",
                broken(&format!("${{{variable}}}  old"), "${SOMETHING}  old"),
            ),
            ("checked after it is unpacked", {
                let check = format!("  echo \"${{{variable}}}  old.tar.gz\" | sha256sum -c -\n");
                let unpack =
                    format!("  mkdir -p {dir} && tar -xzf \"old.tar.gz\" -C {dir} inklkunn\n");
                broken(&format!("{check}{unpack}"), &format!("{unpack}{check}"))
            }),
            (
                "not checked to be that release",
                broken(
                    &format!("inklkunn {second} (commit }}"),
                    "inklkunn 0.1.0 (commit }",
                ),
            ),
            (
                "not on PATH",
                broken(
                    &format!("{dir} >> \"$GITHUB_PATH\"\n"),
                    &format!("{dir} > /dev/null\n"),
                ),
            ),
            (
                "on PATH in another release's directory",
                broken(
                    &format!("{dir} >> \"$GITHUB_PATH\""),
                    "\"$HOME/.local/inklkunn-0.1.0/bin\" >> \"$GITHUB_PATH\"",
                ),
            ),
            (
                "the with-prikk run does not fail on the skip line",
                broken(
                    &format!("|inklkunn {} not found", second.replace('.', "\\.")),
                    "",
                ),
            ),
        ] {
            let violations = upgrade_from_shape_violations(&yaml);
            assert!(
                !violations.is_empty(),
                "{second}: {what}: no violation reported"
            );
            // A violation names the release it is about when it is one release's.
            if what != "no release recorded" {
                assert!(
                    violations.iter().any(|v| v.contains(second)),
                    "{second}: {what}: {violations:?}"
                );
            }
        }
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
