//! A history record against wiki-commons' `schemas/history-record.schema.json` (a copy under `tests/fixtures/`, see
//! `HISTORY-RECORD-SOURCE.md` there), shared by the tests that read a bundle's history.
#![allow(dead_code)]

use super::json_schema;

/// The schema, as copied from wiki-commons.
pub const SCHEMA: &str = include_str!("../fixtures/history-record.schema.json");

/// Every way one line of a history file fails the schema (empty when it conforms).
pub fn violations(record: &serde_json::Value) -> Vec<String> {
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).expect("the schema is JSON");
    json_schema::violations(&schema, record)
}

/// What wiki-commons' own `validate_bundle.py` checks of a history file beyond the schema, and what an importer relies on
/// (RFC 007 D-2): within a file, every `rev` is new, a `parent` names the record before it (the first has none), the times never
/// go back, and a record with `content` and `sha256` has the digest of its content. (The validator compares `at` as strings; this
/// compares instants, as the import does, so the two differ only where the validator is wrong.)
pub fn file_violations(lines: &[serde_json::Value]) -> Vec<String> {
    use sha2::{Digest, Sha256};
    let mut out = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for (n, line) in lines.iter().enumerate() {
        let here = format!("line {}", n + 1);
        out.extend(violations(line).into_iter().map(|v| format!("{here}: {v}")));
        let rev = line.get("rev").and_then(|v| v.as_str()).unwrap_or_default();
        if seen.contains(&rev) {
            out.push(format!("{here}: the rev {rev} is not new"));
        }
        let parent = line.get("parent").and_then(|v| v.as_str());
        if parent != seen.last().copied() {
            out.push(format!(
                "{here}: the parent is {parent:?}, not the record before it ({:?})",
                seen.last()
            ));
        }
        seen.push(rev);
        if let (Some(content), Some(sha)) = (
            line.get("content").and_then(|v| v.as_str()),
            line.get("sha256").and_then(|v| v.as_str()),
        ) {
            let digest: String = Sha256::digest(content.as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            if digest != sha {
                out.push(format!("{here}: sha256 is not the content's"));
            }
        }
        if line.get("suppressed").is_some() && line.get("content").is_some() {
            out.push(format!("{here}: a suppressed record carries content"));
        }
    }
    let times: Vec<&str> = lines
        .iter()
        .filter_map(|l| l.get("at").and_then(|v| v.as_str()))
        .collect();
    // The times are `YYYY-MM-DDTHH:MM:SSZ` in UTC, so they compare as the instants they are.
    if times.windows(2).any(|w| match w {
        [a, b] => b < a,
        _ => false,
    }) {
        out.push("the times go back".to_owned());
    }
    out
}
