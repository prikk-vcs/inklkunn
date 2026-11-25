#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use super::*;

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/interchange")
        .join(rel)
}

#[test]
fn explore() {
    for (name, opts) in [
        ("mediawiki/bundle", BundleOptions { bundle_license: Some("CC-BY-SA-4.0".into()), ..BundleOptions::default() }),
        ("bookstack/bundle", BundleOptions { bundle_license: Some("CC0-1.0".into()), ..BundleOptions::default() }),
        ("inklkunn/bundle", BundleOptions::default()),
    ] {
        let plan = plan_bundle(&fixture(name), &ReadConfig::default(), &opts).unwrap();
        println!("=== {name}\n{}", plan.render());
    }
}
