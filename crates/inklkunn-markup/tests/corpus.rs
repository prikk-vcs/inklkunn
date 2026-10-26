//! The wiki-commons conformance corpus is this crate's acceptance test (RFC 001 D-7). Every case under
//! `tests/corpus/markup` must scan to its `expected.scan.json`; every case under `tests/corpus/resolution`
//! must resolve to its `expected.json`. Provenance and scope: `tests/corpus/CORPUS-SOURCE.md`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use inklkunn_markup::frontmatter::{Value, parse_yaml};
use inklkunn_markup::resolve::{Index, PageEntry};
use inklkunn_markup::scan::{ScanOptions, scan_with};

fn corpus(sub: &str) -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(sub);
    let mut dirs: Vec<PathBuf> = fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("{}: {e}", root.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty(), "no cases under {}", root.display());
    dirs
}

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn case_yaml(dir: &Path) -> Value {
    let p = dir.join("case.yaml");
    if p.exists() {
        parse_yaml(&read(&p)).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    } else {
        Value::Map(Vec::new())
    }
}

fn pretty(v: &serde_json::Value) -> String {
    serde_json::to_string_pretty(v).unwrap_or_default()
}

/// A line-oriented diff good enough to point at the first divergence.
fn first_difference(a: &str, b: &str) -> String {
    for (i, (x, y)) in a.lines().zip(b.lines()).enumerate() {
        if x != y {
            return format!("line {}:\n  expected: {x}\n  actual:   {y}", i + 1);
        }
    }
    format!(
        "lengths differ: expected {} lines, actual {} lines",
        a.lines().count(),
        b.lines().count()
    )
}

#[test]
fn every_markup_case_scans_to_its_expectation() {
    let mut failures = Vec::new();
    let mut passed = 0;
    for dir in corpus("markup") {
        let case = case_yaml(&dir);
        let label_first = case.get("label_order").and_then(Value::as_str) == Some("label-first");
        let input = read(&dir.join("input.md"));
        let expected: serde_json::Value =
            serde_json::from_str(&read(&dir.join("expected.scan.json")))
                .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        let actual = serde_json::to_value(scan_with(&input, ScanOptions { label_first }))
            .expect("serializable");
        if actual == expected {
            passed += 1;
        } else {
            let (e, a) = (pretty(&expected), pretty(&actual));
            failures.push(format!(
                "{}\n{}",
                dir.file_name().unwrap_or_default().to_string_lossy(),
                first_difference(&e, &a)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} passed, {} failed:\n\n{}",
        passed,
        failures.len(),
        failures.join("\n\n")
    );
}

fn index_from_case(case: &Value) -> Index {
    let prefix = case
        .get("pages_prefix")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut ix = Index::new(prefix);
    if let Some(Value::Seq(ns)) = case.get("namespaces") {
        for n in ns {
            if let Some(s) = n.as_str() {
                ix.namespaces.insert(s.to_owned());
            }
        }
    }
    if let Some(Value::Map(iw)) = case.get("interwiki") {
        for (k, v) in iw {
            if let Some(s) = v.as_str() {
                ix.interwiki.insert(k.clone(), s.to_owned());
            }
        }
    }
    if let Some(Value::Seq(pages)) = case.get("pages") {
        for p in pages {
            ix.add(PageEntry {
                path: p
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                title: p
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                aliases: p
                    .get("aliases")
                    .map(Value::as_string_list)
                    .unwrap_or_default(),
                kind: p
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("page")
                    .to_owned(),
            });
        }
    }
    ix
}

#[test]
fn every_resolution_case_resolves_to_its_expectation() {
    let mut failures = Vec::new();
    let mut passed = 0;
    for dir in corpus("resolution") {
        let case = case_yaml(&dir);
        let from_path = case
            .get("from_path")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let ix = index_from_case(&case);
        let input = read(&dir.join("input.md"));
        let expected: serde_json::Value = serde_json::from_str(&read(&dir.join("expected.json")))
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        let report = scan_with(&input, ScanOptions::default());
        let links: Vec<serde_json::Value> = report
            .links
            .iter()
            .map(|l| {
                let r = ix.resolve(&l.target, from_path.as_deref());
                let mut m = serde_json::Map::new();
                m.insert("raw".into(), l.raw.clone().into());
                m.insert("target".into(), l.target.clone().into());
                m.insert(
                    "path".into(),
                    r.path.clone().map_or(serde_json::Value::Null, Into::into),
                );
                m.insert("via".into(), r.via.as_str().into());
                if let Some(u) = &r.url {
                    m.insert("url".into(), u.clone().into());
                }
                if !r.candidates.is_empty() {
                    m.insert("candidates".into(), r.candidates.clone().into());
                }
                serde_json::Value::Object(m)
            })
            .collect();
        let actual = serde_json::json!({"from_path": from_path, "links": links});
        if actual == expected {
            passed += 1;
        } else {
            let (e, a) = (pretty(&expected), pretty(&actual));
            failures.push(format!(
                "{}\n{}",
                dir.file_name().unwrap_or_default().to_string_lossy(),
                first_difference(&e, &a)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} passed, {} failed:\n\n{}",
        passed,
        failures.len(),
        failures.join("\n\n")
    );
}

#[test]
fn the_corpus_is_complete() {
    assert_eq!(corpus("markup").len(), 24, "markup cases");
    assert_eq!(corpus("resolution").len(), 10, "resolution cases");
}
