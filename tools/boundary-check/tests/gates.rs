//! The gates of RFC 001 D-2 applied to the real workspace, each with a planted-failure test. Where a rule
//! walks the tree, one planted test writes a file into a scratch tree so the discovery path is exercised
//! too, not only the matcher.
// Tests may `expect`/`unwrap`: a failed expectation is the test failing, not a product path.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};

use inklkunn_boundary_check::*;

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("inklkunn-gates-{}-{tag}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).expect("scratch");
        Scratch(p)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn read(p: &Path) -> String {
    fs::read_to_string(p).unwrap_or_default()
}

#[test]
fn every_member_crate_is_in_exactly_one_layer() {
    let manifests = read_manifests(&workspace_root());
    let (unknown, missing) = unlayered_crates(&manifests);
    assert!(
        unknown.is_empty(),
        "LAYER violation: crates in no layer (add them to LOWER, CORE or UPPER with a reason): {unknown:?}"
    );
    assert!(
        missing.is_empty(),
        "layer crates missing from `crates/`: {missing:?}"
    );
    let all: Vec<&str> = LOWER.iter().chain(CORE).chain(UPPER).copied().collect();
    for (i, a) in all.iter().enumerate() {
        assert!(
            !all.get(i + 1..).is_some_and(|rest| rest.contains(a)),
            "`{a}` is in two layers"
        );
    }
}

#[test]
fn layering_points_downward_only() {
    let manifests = read_manifests(&workspace_root());
    let v = layering_violations(&manifests);
    assert!(
        v.is_empty(),
        "LAYER violation (RFC 001 D-2 a): {}",
        v.iter()
            .map(|(a, b)| format!("`{a}` depends on `{b}`"))
            .collect::<Vec<_>>()
            .join("; ")
    );
}

#[test]
fn layering_gate_catches_planted_upward_edges_in_every_spelling() {
    let plants = [
        "[dependencies]\ninklkunn-web = { path = \"../inklkunn-web\" }\n",
        "[dependencies]\ninklkunn-web.workspace = true\n",
        "[dependencies]\n\"inklkunn-web\" = { path = \"../inklkunn-web\" }\n",
        "[dependencies]\nweb = { package = \"inklkunn-web\", path = \"../inklkunn-web\" }\n",
        "[dependencies.inklkunn-web]\npath = \"../inklkunn-web\"\n",
        "[target.'cfg(unix)'.dependencies]\ninklkunn-web = { path = \"../inklkunn-web\" }\n",
        "[dependencies]\ninklkunn-web = {\n  path = \"../inklkunn-web\",\n  version = \"0.0.0\"\n}\n",
    ];
    for plant in plants {
        let mut m = read_manifests(&workspace_root());
        m.insert("inklkunn-store".into(), plant.into());
        assert_eq!(
            layering_violations(&m),
            vec![("inklkunn-store".to_owned(), "inklkunn-web".to_owned())],
            "{plant}"
        );
    }
    // A lower crate reaching for a core crate is upward too.
    let mut m = read_manifests(&workspace_root());
    m.insert(
        "inklkunn-prikk".into(),
        "[dependencies]\ninklkunn-core.workspace = true\n".into(),
    );
    assert_eq!(
        layering_violations(&m),
        vec![("inklkunn-prikk".to_owned(), "inklkunn-core".to_owned())]
    );
    // A longer name is not a false positive; a dev-dependency is not an edge.
    assert!(!declares_dep_on(
        "[dependencies]\ninklkunn-webhook = \"1\"\n",
        "inklkunn-web"
    ));
    assert!(!declares_dep_on(
        "[dev-dependencies]\ninklkunn-web = { path = \"x\" }\n",
        "inklkunn-web"
    ));
    // An unknown crate is reported.
    let mut m = read_manifests(&workspace_root());
    m.insert(
        "inklkunn-api".into(),
        "[package]\nname = \"inklkunn-api\"\n".into(),
    );
    assert_eq!(unlayered_crates(&m).0, vec!["inklkunn-api".to_owned()]);
}

#[test]
fn only_core_depends_on_the_signer_and_no_product_crate_enables_a_test_feature() {
    let manifests = read_manifests(&workspace_root());
    let d = signer_dependents(&manifests);
    assert!(
        d.is_empty(),
        "KEY REACHABILITY violation (RFC 001 D-2 b, INV-2): {d:?} depend on `inklkunn-signer`"
    );
    let leaks = test_feature_leaks(&manifests);
    assert!(
        leaks.is_empty(),
        "a product crate enables a test-only feature: {leaks:?}"
    );
}

#[test]
fn key_reachability_gate_catches_planted_dependencies_and_features() {
    let mut m = read_manifests(&workspace_root());
    m.insert(
        "inklkunn-web".into(),
        "[dependencies]\ninklkunn-signer = { path = \"../inklkunn-signer\" }\n".into(),
    );
    assert_eq!(signer_dependents(&m), vec!["inklkunn-web".to_owned()]);
    let mut m = read_manifests(&workspace_root());
    m.insert(
        "inklkunn".into(),
        "[dependencies]\ninklkunn-signer = { path = \"../inklkunn-signer\", features = [\"bootstrap\"] }\n"
            .into(),
    );
    assert_eq!(signer_dependents(&m), vec!["inklkunn".to_owned()]);
    assert_eq!(
        test_feature_leaks(&m),
        vec![(
            "inklkunn".to_owned(),
            "inklkunn-signer".to_owned(),
            "bootstrap".to_owned()
        )]
    );
    let mut m = read_manifests(&workspace_root());
    m.insert(
        "inklkunn-web".into(),
        "[dependencies]\ninklkunn-prikk = { path = \"../inklkunn-prikk\", features = [\"test-knobs\"] }\n"
            .into(),
    );
    assert_eq!(test_feature_leaks(&m).len(), 1);
}

#[test]
fn the_key_store_module_is_private_unmentioned_and_the_signer_names_stay_in_the_signer() {
    let root = workspace_root();
    let lib = read(&root.join("crates/inklkunn-signer/src/lib.rs"));
    assert!(
        !key_store_exposed(&lib),
        "KEY REACHABILITY violation: the signer exports its key-store module"
    );
    for src in rust_sources(&root.join("crates")) {
        if src.starts_with(root.join("crates/inklkunn-signer")) {
            continue;
        }
        let text = read(&src);
        let hits = key_store_mentions(&text);
        assert!(
            hits.is_empty(),
            "KEY REACHABILITY violation: {} mentions `{KEY_STORE_MODULE}` at lines {hits:?}",
            src.display()
        );
        // The writable mounts and the bootstrap signer: product sources outside the signer (and the
        // sandbox crate that defines the mounts) never name them. Tests may.
        if is_product_source(&src) && !src.starts_with(root.join("crates/inklkunn-prikk")) {
            let hits = signer_only_mentions(&text);
            assert!(
                hits.is_empty(),
                "KEY REACHABILITY violation: {} names a signer-only item {hits:?}",
                src.display()
            );
        }
    }
}

#[test]
fn key_store_exposure_gate_catches_planted_exposures() {
    assert!(key_store_exposed("pub mod key_store;\n"));
    assert!(key_store_exposed(
        "mod key_store;\npub use key_store::Seed;\n"
    ));
    assert!(!key_store_exposed(
        "mod key_store;\n// pub use key_store::Seed;\nuse key_store::Record;\n"
    ));
    assert_eq!(
        key_store_mentions("use inklkunn_signer::key_store::Seed;\n"),
        vec![1]
    );
    // Prose and strings are not code.
    assert!(key_store_mentions("// the signer's key_store is private\n").is_empty());
    assert!(key_store_mentions("let s = \"key_store\";\n").is_empty());
    assert!(key_store_mentions("let key_store_name = 1;\n").is_empty());
    assert_eq!(
        signer_only_mentions("let m = Mounts::write(repo, keys, env);\n"),
        vec![(1, "Mounts::write".to_owned())]
    );
    assert_eq!(
        signer_only_mentions("let s = BootstrapSigner::new(a, b, c);\n"),
        vec![(1, "BootstrapSigner".to_owned())]
    );
    assert!(signer_only_mentions("// Mounts::write is the signer's\n").is_empty());
    assert!(is_product_source(Path::new(
        "/w/crates/inklkunn-core/src/wiki.rs"
    )));
    assert!(!is_product_source(Path::new(
        "/w/crates/inklkunn-core/tests/create_wiki.rs"
    )));
}

#[test]
fn only_the_driver_and_the_signer_spawn_a_process() {
    let root = workspace_root();
    for src in rust_sources(&root.join("crates")) {
        let allowed = src.starts_with(root.join("crates/inklkunn-prikk"))
            || src.starts_with(root.join("crates/inklkunn-signer"));
        if allowed {
            continue;
        }
        let text = read(&src);
        let hits = spawn_lines(&text);
        assert!(
            hits.is_empty(),
            "PRIKK SPAWN violation (RFC 001 D-2 c): {} spawns a process at lines {hits:?}",
            src.display()
        );
    }
}

#[test]
fn the_read_driver_names_no_write_verb() {
    let root = workspace_root();
    for src in rust_sources(&root.join("crates/inklkunn-prikk")) {
        let text = read(&src);
        let hits = write_verb_literals(&text);
        assert!(
            hits.is_empty(),
            "PRIKK SPAWN violation (RFC 001 D-2 c): the read driver {} names write verbs {hits:?}",
            src.display()
        );
    }
}

#[test]
fn prikk_spawn_gate_catches_planted_violations_including_a_planted_file() {
    assert_eq!(spawn_lines("let c = Command::new(\"prikk\");\n"), vec![1]);
    assert_eq!(spawn_lines("let c = Command::new(&binary);\n"), vec![1]);
    assert_eq!(spawn_lines("use std::process::Command;\n"), vec![1]);
    assert_eq!(spawn_lines("use std::process::{id, Command};\n"), vec![1]);
    assert_eq!(
        spawn_lines("use std::process;\n"),
        vec![1],
        "the module itself is a spawn path"
    );
    assert_eq!(spawn_lines("tokio::process::Command::new(x)\n"), vec![1]);
    assert!(
        spawn_lines("let n = std::process::id();\n").is_empty(),
        "a pid is not a spawn"
    );
    assert!(spawn_lines("// Command::new(\"prikk\") would be a violation\n").is_empty());
    assert!(spawn_lines("let s = \"std::process\";\n").is_empty());
    assert_eq!(
        write_verb_literals("repo.run(&[\"seal\", \"--allow-no-audit\"])\n"),
        vec![(1, "seal".to_owned())]
    );
    assert_eq!(
        write_verb_literals("let v = [\"seal\"]; // serde(\n"),
        vec![(1, "seal".to_owned())],
        "a comment does not hide a literal"
    );
    assert!(write_verb_literals("repo.run(&[\"log\", \"--format\", \"json\"])\n").is_empty());
    assert!(
        write_verb_literals("    #[serde(rename = \"ref\")]\n    pub view: String,\n").is_empty(),
        "a serde field attribute names a report key, not a spawn argument"
    );
    // Discovery: a file planted anywhere under a crate (a `tests/` dir, a nested module) is found.
    let scratch = Scratch::new("spawn");
    let crate_dir = scratch.0.join("crates/inklkunn-web");
    fs::create_dir_all(crate_dir.join("tests/deep")).unwrap();
    fs::create_dir_all(crate_dir.join("src/target")).unwrap();
    fs::write(
        crate_dir.join("tests/deep/x.rs"),
        "use std::process::Command;\n",
    )
    .unwrap();
    fs::write(
        crate_dir.join("src/target/mod.rs"),
        "let c = Command::new(\"prikk\");\n",
    )
    .unwrap();
    let found = rust_sources_under(&scratch.0, &scratch.0.join("crates"));
    assert_eq!(found.len(), 2, "{found:?}");
    for f in found {
        assert!(!spawn_lines(&read(&f)).is_empty(), "{}", f.display());
    }
}

#[test]
fn the_markup_crate_is_pure() {
    let root = workspace_root();
    let manifest = read(&root.join("crates/inklkunn-markup/Cargo.toml"));
    for dep in runtime_dependencies(&manifest) {
        assert!(
            MARKUP_ALLOWED_DEPS.contains(&dep.as_str()),
            "MARKUP PURITY violation (RFC 001 D-2 d): `inklkunn-markup` depends on `{dep}`"
        );
    }
    for src in rust_sources(&root.join("crates/inklkunn-markup/src")) {
        let text = read(&src);
        let hits = impure_lines(&text);
        assert!(
            hits.is_empty(),
            "MARKUP PURITY violation: {} reaches for {hits:?}",
            src.display()
        );
    }
}

#[test]
fn markup_purity_gate_catches_planted_violations() {
    assert_eq!(
        impure_lines("let t = std::fs::read_to_string(p);\n"),
        vec![(1, "std::fs".to_owned())]
    );
    assert_eq!(
        impure_lines("use std::{fs, env};\n"),
        vec![(1, "std::env".to_owned()), (1, "std::fs".to_owned())]
    );
    assert_eq!(
        impure_lines("use std::time::{SystemTime, Instant};\n"),
        vec![(1, "std::time".to_owned())]
    );
    assert_eq!(
        impure_lines("use std::io::Read;\n"),
        vec![(1, "std::io::Read".to_owned())]
    );
    assert!(impure_lines("use std::collections::BTreeMap;\n").is_empty());
    assert!(impure_lines("// std::fs is forbidden here\n").is_empty());
    assert_eq!(
        runtime_dependencies("[dependencies]\ntokio = \"1\"\n\n[dev-dependencies]\nfoo = \"1\"\n"),
        vec!["tokio".to_owned()]
    );
    assert_eq!(
        runtime_dependencies("[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n"),
        vec!["libc".to_owned()]
    );
    assert_eq!(
        runtime_dependencies("[dependencies.libc]\nversion = \"0.2\"\n"),
        vec!["libc".to_owned()]
    );
    assert_eq!(
        expand_use("use std::{fs, time::{Instant, SystemTime}};"),
        vec!["std::fs", "std::time::Instant", "std::time::SystemTime"]
    );
}

#[test]
fn every_member_crate_forbids_unsafe_and_uses_the_workspace_lints() {
    let root = workspace_root();
    for dir in ["crates", "tools"] {
        for e in fs::read_dir(root.join(dir)).expect("dir").flatten() {
            let manifest_path = e.path().join("Cargo.toml");
            if !manifest_path.is_file() {
                continue;
            }
            let manifest = read(&manifest_path);
            assert!(
                uses_workspace_lints(&manifest),
                "{} lacks `[lints] workspace = true`",
                manifest_path.display()
            );
            for root_file in ["src/lib.rs", "src/main.rs"] {
                let p = e.path().join(root_file);
                if p.is_file() {
                    assert!(
                        forbids_unsafe(&read(&p)),
                        "{} does not open with `#![forbid(unsafe_code)]`",
                        p.display()
                    );
                }
            }
        }
    }
    assert!(forbids_unsafe(
        "// a comment\n\n#![forbid(unsafe_code)]\nfn x() {}\n"
    ));
    assert!(!forbids_unsafe("#![allow(unsafe_code)]\n"));
    assert!(!forbids_unsafe("fn x() {}\n#![forbid(unsafe_code)]\n"));
    assert!(uses_workspace_lints(
        "[package]\nname = \"a\"\n\n[lints]\nworkspace = true\n"
    ));
    assert!(!uses_workspace_lints(
        "[package]\nname = \"a\"\n\n[lints.rust]\nunsafe_code = \"allow\"\n"
    ));
}
