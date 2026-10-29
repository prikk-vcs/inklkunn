//! The bundle manifest against wiki-commons' `schemas/bundle-manifest.schema.json` (a copy under
//! `tests/fixtures/`, see `BUNDLE-MANIFEST-SOURCE.md` there), shared by the tests that write a manifest.
#![allow(dead_code)]

use super::json_schema;

/// The schema, as copied from wiki-commons.
pub const SCHEMA: &str = include_str!("../fixtures/bundle-manifest.schema.json");

/// Every way the manifest text fails the schema: it is read as the YAML the wiki reads, then checked as JSON
/// (empty when it conforms).
pub fn violations(manifest: &str) -> Vec<String> {
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).expect("the schema is JSON");
    match inklkunn_markup::frontmatter::parse_yaml(manifest) {
        Ok(value) => json_schema::violations(&schema, &value.to_json()),
        Err(e) => vec![format!("the manifest is not YAML the wiki reads: {e:?}")],
    }
}
