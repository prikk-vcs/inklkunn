//! The self-assessment (RFC 007 D-10; handoff 007 I-T6; wiki-commons chapter 16): `wiki-commons.yaml` at the repository root
//! says, item by item, what inklkunn does of the conformance profiles it claims at 0.4, and **a `yes` is only where a named
//! test shows it**. This file reads it and fails when:
//!
//! - it is not in the shape of wiki-commons' `schemas/self-assessment.schema.json` (a copy under `tests/fixtures/`, see
//!   `SELF-ASSESSMENT-SOURCE.md` there);
//! - a status or a claim is a YAML boolean (an unquoted `yes` or `no` is `true` or `false` to a YAML 1.1 reader);
//! - the subject is not inklkunn at the workspace's version (the cut moves the file's version with the release's), or the
//!   profiles are not the seven of RFC 007 D-10 at `@0.4`, or a profile's items are not exactly the ones chapter 16 lists;
//! - an item says `yes` and its note names no test, or says `partial`, `no`, `planned` or `n/a` and gives no reason;
//! - a test a note names (`<path from the repository root>.rs::<fn>`) is not a file, has no such function, or the function
//!   is not a `#[test]` (or is `#[ignore]`d);
//! - a profile's claim is `yes` while one of its items is not `yes` or `n/a`;
//! - an item that is in several profiles has a different status in them.
//!
//! The checks are one function, [`problems`], and each rule has a planted failure below that proves it can fail. No prikk is
//! needed, and the named tests are not run here (CI runs the workspace).
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "support/json_schema.rs"]
mod json_schema;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use inklkunn_markup::Value;
use inklkunn_markup::frontmatter::parse_yaml;

/// wiki-commons' schema for the file, as copied.
const SCHEMA: &str = include_str!("fixtures/self-assessment.schema.json");

/// The repository root (this crate is `crates/inklkunn-core`).
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ids(prefix: &str, numbers: &[u32]) -> Vec<String> {
    numbers.iter().map(|n| format!("{prefix}-{n}")).collect()
}

fn range(prefix: &str, from: u32, to: u32) -> Vec<String> {
    (from..=to).map(|n| format!("{prefix}-{n}")).collect()
}

/// The items of each profile, as wiki-commons 0.4's chapter 16 lists them (§2.1, §2.2, §2.3, §2.6, §2.7, §2.11), with the
/// ones it calls *encouraged* (living-history's HIST-9, HIST-10 and HIST-12).
fn expected_profiles() -> Vec<(&'static str, BTreeSet<String>)> {
    let set = |parts: Vec<Vec<String>>| parts.into_iter().flatten().collect::<BTreeSet<String>>();
    vec![
        (
            "core-reading@0.4",
            set(vec![
                ids("MKUP", &[12]),
                ids("META", &[12]),
                ids("API", &[2]),
                ids("A11Y", &[3]),
                ids("WEB", &[1, 5]),
                ids("LIC", &[1]),
            ]),
        ),
        (
            "portable-content@0.4",
            set(vec![
                ids("MKUP", &[1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 13, 14, 17, 19]),
                ids("META", &[1, 2, 3, 4, 5, 7, 9, 10, 11, 13]),
                ids("XFER", &[1, 2, 3, 4, 5, 6, 12, 13, 15]),
                ids("NAV", &[2, 3, 8]),
            ]),
        ),
        (
            "living-history@0.4",
            set(vec![
                range("HIST", 1, 8),
                ids("HIST", &[11]),
                ids("XFER", &[7, 8]),
                ids("API", &[4]),
                ids("HIST", &[9, 10, 12]),
            ]),
        ),
        (
            "accessible-authoring@0.4",
            set(vec![
                range("A11Y", 1, 11),
                ids("AUTH", &[12, 13]),
                ids("HIST", &[2]),
                ids("I18N", &[1, 3, 7]),
            ]),
        ),
        (
            "many-languages@0.4",
            set(vec![range("I18N", 1, 9), ids("META", &[10])]),
        ),
        ("bundle-exporter@0.4", set(vec![range("XFER", 1, 12)])),
        (
            "bundle-importer@0.4",
            set(vec![
                ids("XFER", &[13, 14]),
                ids("META", &[11]),
                ids("LIC", &[4, 5]),
                ids("SEC", &[7]),
            ]),
        ),
    ]
}

/// The version of the workspace (`[workspace.package]` of the root manifest).
fn workspace_version() -> String {
    let manifest = std::fs::read_to_string(root().join("Cargo.toml")).expect("the root manifest");
    let mut in_package = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[workspace.package]";
        } else if in_package && let Some(rest) = line.strip_prefix("version = \"") {
            return rest.trim_end_matches('"').to_owned();
        }
    }
    panic!("the root manifest has no [workspace.package] version");
}

/// Every YAML boolean in a value, with where it is.
fn booleans(value: &Value, at: &str, out: &mut Vec<String>) {
    match value {
        Value::Bool(b) => out.push(format!(
            "{at} is the boolean {b}: quote it (\"yes\", \"no\")"
        )),
        Value::Seq(items) => {
            for (i, item) in items.iter().enumerate() {
                booleans(item, &format!("{at}[{i}]"), out);
            }
        }
        Value::Map(entries) => {
            for (key, item) in entries {
                booleans(item, &format!("{at}/{key}"), out);
            }
        }
        _ => {}
    }
}

/// The test references of a note: words of the form `<path>.rs::<name>`, with the punctuation of prose trimmed.
fn test_refs(note: &str) -> Vec<&str> {
    note.split_whitespace()
        .map(|word| word.trim_matches(|c: char| matches!(c, ',' | ';' | '.' | ':' | '(' | ')')))
        .filter(|word| word.contains(".rs::"))
        .collect()
}

/// What is wrong with one reference to a test, or `None`.
fn ref_problem(root: &Path, reference: &str) -> Option<String> {
    let (file, name) = reference.split_once("::")?;
    let path_chars = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | '-');
    if !file.ends_with(".rs") || !file.chars().all(path_chars) || file.starts_with('/') {
        return Some(format!(
            "{reference}: the file is not a path from the repository root"
        ));
    }
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Some(format!(
            "{reference}: the test's name is not a function name"
        ));
    }
    let Ok(source) = std::fs::read_to_string(root.join(file)) else {
        return Some(format!("{reference}: {file} is not a file"));
    };
    let lines: Vec<&str> = source.lines().collect();
    let needle = format!("fn {name}(");
    let at = lines.iter().position(|line| {
        line.find(&needle).is_some_and(|i| {
            line.get(..i)
                .is_some_and(|before| before.is_empty() || before.ends_with(' '))
        })
    });
    let Some(at) = at else {
        return Some(format!("{reference}: {file} has no function {name}"));
    };
    let above = lines.get(at.saturating_sub(8)..at).unwrap_or_default();
    if !above.iter().any(|l| {
        matches!(l.trim(), "#[test]" | "#[tokio::test]") || l.trim().starts_with("#[tokio::test(")
    }) {
        return Some(format!("{reference}: {name} is not a #[test]"));
    }
    if above.iter().any(|l| l.trim().starts_with("#[ignore")) {
        return Some(format!("{reference}: {name} is ignored"));
    }
    None
}

/// Every way `text` fails what the self-assessment must be (empty when it is what it must be). `version` is the workspace's.
fn problems(text: &str, root: &Path, version: &str) -> Vec<String> {
    let mut out = Vec::new();
    let doc = match parse_yaml(text) {
        Ok(doc) => doc,
        Err(e) => return vec![format!("the file is not the YAML subset: {e}")],
    };
    booleans(&doc, "", &mut out);
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).expect("the schema is JSON");
    out.extend(
        json_schema::violations(&schema, &doc.to_json())
            .into_iter()
            .map(|v| format!("schema: {v}")),
    );

    let subject = doc.get("subject");
    let said = |key: &str| subject.and_then(|s| s.get(key)).and_then(Value::as_str);
    if said("name") != Some("inklkunn") {
        out.push("subject.name is not inklkunn".to_owned());
    }
    if said("kind") != Some("engine") {
        out.push("subject.kind is not engine".to_owned());
    }
    if said("version") != Some(version) {
        out.push(format!(
            "subject.version is {:?}, the workspace's is {version:?}: the cut moves the file's version",
            said("version")
        ));
    }

    let profiles: Vec<(&str, &Value)> = match doc.get("profiles") {
        Some(Value::Map(entries)) => entries.iter().map(|(k, v)| (k.as_str(), v)).collect(),
        _ => return out,
    };
    let expected = expected_profiles();
    for (name, _) in &expected {
        if !profiles.iter().any(|(p, _)| p == name) {
            out.push(format!("{name} is not assessed"));
        }
    }
    let mut statuses: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for (name, profile) in &profiles {
        let Some((_, wanted)) = expected.iter().find(|(p, _)| p == name) else {
            out.push(format!("{name} is not one of the seven profiles at @0.4"));
            continue;
        };
        let items: Vec<(&str, &Value)> = match profile.get("items") {
            Some(Value::Map(entries)) => entries.iter().map(|(k, v)| (k.as_str(), v)).collect(),
            _ => {
                out.push(format!("{name} has no items"));
                continue;
            }
        };
        let present: BTreeSet<String> = items.iter().map(|(id, _)| (*id).to_owned()).collect();
        for id in wanted.difference(&present) {
            out.push(format!(
                "{name}: {id} is not assessed (chapter 16 lists it)"
            ));
        }
        for id in present.difference(wanted) {
            out.push(format!("{name}: {id} is not an item of the profile"));
        }
        let mut all_yes = true;
        for (id, item) in &items {
            let here = format!("{name}: {id}");
            let status = item.get("status").and_then(Value::as_str).unwrap_or("");
            let note = item.get("note").and_then(Value::as_str).unwrap_or("");
            statuses
                .entry((*id).to_owned())
                .or_default()
                .entry(status.to_owned())
                .or_default()
                .push((*name).to_owned());
            if !matches!(status, "yes" | "n/a") {
                all_yes = false;
            }
            let refs = test_refs(note);
            match status {
                "yes" if refs.is_empty() => {
                    out.push(format!("{here}: says yes and names no test"));
                }
                "partial" | "no" | "planned" | "n/a" if note.trim().is_empty() => {
                    out.push(format!("{here}: says {status} and gives no reason"));
                }
                _ => {}
            }
            for reference in refs {
                out.extend(ref_problem(root, reference).map(|p| format!("{here}: {p}")));
            }
        }
        let claim = profile.get("claim").and_then(Value::as_str);
        if claim == Some("yes") && !all_yes {
            out.push(format!(
                "{name}: claims yes while an item is not yes or n/a"
            ));
        }
    }
    for (id, by_status) in &statuses {
        if by_status.len() > 1 {
            out.push(format!(
                "{id} has one status in each profile it is in, and they differ: {by_status:?}"
            ));
        }
    }
    out
}

/// A small file with one profile and one item, for the planted failures: whatever else is missing is reported as well, so each
/// test looks for the one problem it plants.
fn one_item(status: &str, note: &str) -> String {
    format!(
        "format: wiki-commons-self-assessment\nversion: \"0.1\"\nsubject:\n  name: inklkunn\n  version: \"{}\"\n  kind: engine\nprofiles:\n  core-reading@0.4:\n    claim: partial\n    items:\n      WEB-1:\n        status: {status}\n        note: \"{note}\"\n",
        workspace_version()
    )
}

fn has(found: &[String], needle: &str) -> bool {
    found.iter().any(|p| p.contains(needle))
}

const A_TEST: &str = "crates/inklkunn-core/tests/self_assessment.rs::the_file_at_the_root_is_what_a_self_assessment_must_be";

/// The file at the repository root has no problem.
#[test]
fn the_file_at_the_root_is_what_a_self_assessment_must_be() {
    let text = std::fs::read_to_string(root().join("wiki-commons.yaml"))
        .expect("wiki-commons.yaml at the root");
    let found = problems(&text, &root(), &workspace_version());
    assert!(found.is_empty(), "{}", found.join("\n"));
    // What it claims, for the reviewer: how many items, and how many tests it names.
    let doc = parse_yaml(&text).expect("it parses");
    let (mut items, mut yes, mut named) = (0, 0, BTreeSet::new());
    if let Some(Value::Map(profiles)) = doc.get("profiles") {
        for (_, profile) in profiles {
            if let Some(Value::Map(entries)) = profile.get("items") {
                for (_, item) in entries {
                    items += 1;
                    yes += usize::from(item.get("status").and_then(Value::as_str) == Some("yes"));
                    let note = item.get("note").and_then(Value::as_str).unwrap_or("");
                    named.extend(test_refs(note).into_iter().map(str::to_owned));
                }
            }
        }
    }
    println!(
        "self-assessment: {items} items, {yes} yes, {} distinct tests named",
        named.len()
    );
}

/// The planted file is what it should be when its one item is right: the checks do not fail what is right.
#[test]
fn a_yes_that_names_a_test_that_is_one_has_no_problem_of_its_own() {
    let found = problems(&one_item("\"yes\"", A_TEST), &root(), &workspace_version());
    assert!(
        !has(&found, "WEB-1"),
        "WEB-1 is right and was found wrong:\n{}",
        found.join("\n")
    );
    assert!(
        !has(&found, "boolean") && !has(&found, "schema:"),
        "{}",
        found.join("\n")
    );
}

#[test]
fn a_yes_that_names_no_test_is_found() {
    let found = problems(
        &one_item("\"yes\"", "it works"),
        &root(),
        &workspace_version(),
    );
    assert!(
        has(&found, "WEB-1: says yes and names no test"),
        "{found:?}"
    );
}

#[test]
fn a_test_that_does_not_exist_is_found_whatever_the_status() {
    let file = "crates/inklkunn-core/tests/self_assessment.rs";
    for status in ["\"yes\"", "partial"] {
        let found = problems(
            &one_item(status, &format!("{file}::there_is_no_such_test")),
            &root(),
            &workspace_version(),
        );
        assert!(
            has(&found, "has no function there_is_no_such_test"),
            "{status}: {found:?}"
        );
    }
}

#[test]
fn a_file_that_does_not_exist_and_a_path_that_is_not_one_are_found() {
    let found = problems(
        &one_item(
            "partial",
            "crates/inklkunn-core/tests/no_such_file.rs::x because",
        ),
        &root(),
        &workspace_version(),
    );
    assert!(has(&found, "no_such_file.rs is not a file"), "{found:?}");
    let found = problems(
        &one_item("partial", "/etc/passwd.rs::x and ../x.rs::y because"),
        &root(),
        &workspace_version(),
    );
    assert!(
        has(&found, "not a path from the repository root"),
        "{found:?}"
    );
}

#[test]
fn a_function_that_is_not_a_test_is_found() {
    // `fn link` is a helper of the markup renderer's tests, not one of its tests.
    let found = problems(
        &one_item(
            "\"yes\"",
            "crates/inklkunn-markup/src/render/tests.rs::link",
        ),
        &root(),
        &workspace_version(),
    );
    assert!(has(&found, "link is not a #[test]"), "{found:?}");
}

#[test]
fn an_unquoted_yes_or_no_is_a_boolean_and_is_found() {
    for bare in ["yes", "no"] {
        let found = problems(&one_item(bare, A_TEST), &root(), &workspace_version());
        assert!(has(&found, "is the boolean"), "{bare}: {found:?}");
    }
}

#[test]
fn a_status_the_schema_does_not_have_is_found() {
    let found = problems(
        &one_item("maybe", "a reason"),
        &root(),
        &workspace_version(),
    );
    assert!(has(&found, "schema:") && has(&found, "maybe"), "{found:?}");
}

#[test]
fn a_partial_no_planned_or_not_applicable_item_without_a_reason_is_found() {
    for status in ["partial", "\"no\"", "planned", "n/a"] {
        let found = problems(&one_item(status, ""), &root(), &workspace_version());
        assert!(has(&found, "gives no reason"), "{status}: {found:?}");
    }
}

#[test]
fn a_date_that_is_not_one_and_a_missing_format_are_found_by_the_schema() {
    let base = one_item("\"yes\"", A_TEST);
    let found = problems(
        &format!("{base}assessed_at: \"last week\"\n"),
        &root(),
        &workspace_version(),
    );
    assert!(
        has(&found, "assessed_at") && has(&found, "none of the alternatives"),
        "{found:?}"
    );
    let found = problems(
        &format!("{base}assessed_at: \"2026-11-26\"\n"),
        &root(),
        &workspace_version(),
    );
    assert!(!has(&found, "assessed_at"), "{found:?}");
    let without = base.replace("format: wiki-commons-self-assessment\n", "");
    let found = problems(&without, &root(), &workspace_version());
    assert!(has(&found, "`format` is required"), "{found:?}");
}

#[test]
fn a_subject_that_is_not_inklkunn_at_the_workspaces_version_is_found() {
    let base = one_item("\"yes\"", A_TEST);
    let found = problems(
        &base.replace("version: \"", "version: \"9."),
        &root(),
        &workspace_version(),
    );
    assert!(has(&found, "subject.version is"), "{found:?}");
    let found = problems(
        &base.replace("name: inklkunn", "name: other"),
        &root(),
        &workspace_version(),
    );
    assert!(has(&found, "subject.name is not inklkunn"), "{found:?}");
    let found = problems(
        &base.replace("kind: engine", "kind: tool"),
        &root(),
        &workspace_version(),
    );
    assert!(has(&found, "subject.kind is not engine"), "{found:?}");
}

#[test]
fn a_profile_missing_an_unknown_one_and_a_missing_or_extra_item_are_found() {
    let found = problems(&one_item("\"yes\"", A_TEST), &root(), &workspace_version());
    assert!(
        has(&found, "portable-content@0.4 is not assessed"),
        "{found:?}"
    );
    assert!(
        has(&found, "core-reading@0.4: MKUP-12 is not assessed"),
        "{found:?}"
    );
    let renamed = one_item("\"yes\"", A_TEST).replace("core-reading@0.4", "core-reading@0.3");
    assert!(has(
        &problems(&renamed, &root(), &workspace_version()),
        "core-reading@0.3 is not one of the seven"
    ),);
    let extra = one_item("\"yes\"", A_TEST).replace("WEB-1:", "WEB-9:");
    let found = problems(&extra, &root(), &workspace_version());
    assert!(
        has(&found, "WEB-9 is not an item of the profile"),
        "{found:?}"
    );
    assert!(
        has(&found, "core-reading@0.4: WEB-1 is not assessed"),
        "{found:?}"
    );
}

#[test]
fn a_claim_of_yes_over_an_item_that_is_not_yes_is_found() {
    let text = one_item("partial", "a reason").replace("claim: partial", "claim: \"yes\"");
    let found = problems(&text, &root(), &workspace_version());
    assert!(
        has(
            &found,
            "core-reading@0.4: claims yes while an item is not yes or n/a"
        ),
        "{found:?}"
    );
    // And not over what is: a profile whose one listed item is n/a may claim it (the rest are reported as missing).
    let text = one_item("n/a", "nothing to do").replace("claim: partial", "claim: \"yes\"");
    assert!(!has(
        &problems(&text, &root(), &workspace_version()),
        "claims yes"
    ));
}

#[test]
fn an_item_in_two_profiles_with_two_statuses_is_found() {
    let text = format!(
        "{}  bundle-exporter@0.4:\n    claim: partial\n    items:\n      XFER-1:\n        status: partial\n        note: \"a reason\"\n  portable-content@0.4:\n    claim: partial\n    items:\n      XFER-1:\n        status: \"yes\"\n        note: \"{A_TEST}\"\n",
        one_item("partial", "a reason")
    );
    let found = problems(&text, &root(), &workspace_version());
    assert!(
        has(&found, "XFER-1 has one status in each profile it is in"),
        "{found:?}"
    );
}

/// The expected lists are what chapter 16 says in numbers: the counts a reviewer can check against the chapter.
#[test]
fn the_profiles_have_the_items_chapter_sixteen_lists() {
    let counts: BTreeMap<&str, usize> = expected_profiles()
        .into_iter()
        .map(|(name, items)| (name, items.len()))
        .collect();
    assert_eq!(
        counts,
        BTreeMap::from([
            ("accessible-authoring@0.4", 11 + 2 + 1 + 3),
            ("bundle-exporter@0.4", 12),
            ("bundle-importer@0.4", 6),
            ("core-reading@0.4", 7),
            ("living-history@0.4", 8 + 1 + 2 + 1 + 3),
            ("many-languages@0.4", 9 + 1),
            ("portable-content@0.4", 14 + 10 + 9 + 3),
        ])
    );
}
