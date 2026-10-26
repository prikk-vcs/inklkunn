//! The gates of RFC 001 D-2 applied to the real workspace, each with a planted-failure test.
// Tests may `expect`/`unwrap`: a failed expectation is the test failing, not a product path.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;

use inklkunn_boundary_check::*;

#[test]
fn all_layer_crates_exist() {
    let root = workspace_root();
    for name in LOWER.iter().chain(CORE).chain(UPPER) {
        let p = root.join("crates").join(name).join("Cargo.toml");
        assert!(
            p.exists(),
            "layer crate `{name}` missing at {}",
            p.display()
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
fn layering_gate_catches_a_planted_upward_edge() {
    let mut m = read_manifests(&workspace_root());
    m.insert(
        "inklkunn-store".into(),
        "[dependencies]\ninklkunn-web = { path = \"../inklkunn-web\" }\n".into(),
    );
    assert_eq!(
        layering_violations(&m),
        vec![("inklkunn-store".to_owned(), "inklkunn-web".to_owned())]
    );
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
    // A longer name is not a false positive.
    assert!(!declares_dep_on(
        "inklkunn-webhook = \"1\"\n",
        "inklkunn-web"
    ));
}

#[test]
fn only_core_depends_on_the_signer() {
    let manifests = read_manifests(&workspace_root());
    let d = signer_dependents(&manifests);
    assert!(
        d.is_empty(),
        "KEY REACHABILITY violation (RFC 001 D-2 b, INV-2): {d:?} depend on `inklkunn-signer`"
    );
}

#[test]
fn key_reachability_gate_catches_a_planted_dependency() {
    let mut m = read_manifests(&workspace_root());
    m.insert(
        "inklkunn-web".into(),
        "[dependencies]\ninklkunn-signer = { path = \"../inklkunn-signer\" }\n".into(),
    );
    assert_eq!(signer_dependents(&m), vec!["inklkunn-web".to_owned()]);
}

#[test]
fn the_key_store_module_is_private_and_unmentioned_elsewhere() {
    let root = workspace_root();
    let lib =
        fs::read_to_string(root.join("crates/inklkunn-signer/src/lib.rs")).unwrap_or_default();
    assert!(
        !key_store_exposed(&lib),
        "KEY REACHABILITY violation: the signer exports its key-store module"
    );
    for src in rust_sources(&root.join("crates")) {
        if src.starts_with(root.join("crates/inklkunn-signer")) {
            continue;
        }
        let text = fs::read_to_string(&src).unwrap_or_default();
        let hits = key_store_mentions(&text);
        assert!(
            hits.is_empty(),
            "KEY REACHABILITY violation: {} mentions `{KEY_STORE_MODULE}` at lines {hits:?}",
            src.display()
        );
    }
}

#[test]
fn key_store_exposure_gate_catches_a_planted_pub_mod() {
    assert!(key_store_exposed("pub mod key_store;\n"));
    assert!(!key_store_exposed(
        "mod key_store;\npub use key_store::KeyId;\n"
    ));
    assert_eq!(
        key_store_mentions("use inklkunn_signer::key_store::Seed;\n"),
        vec![1]
    );
}

#[test]
fn only_the_driver_and_the_signer_spawn_prikk() {
    let root = workspace_root();
    for src in rust_sources(&root.join("crates")) {
        let allowed = src.starts_with(root.join("crates/inklkunn-prikk"))
            || src.starts_with(root.join("crates/inklkunn-signer"));
        if allowed {
            continue;
        }
        let text = fs::read_to_string(&src).unwrap_or_default();
        let hits = prikk_spawn_lines(&text);
        assert!(
            hits.is_empty(),
            "PRIKK SPAWN violation (RFC 001 D-2 c): {} spawns prikk at lines {hits:?}",
            src.display()
        );
    }
}

#[test]
fn the_read_driver_names_no_write_verb() {
    let root = workspace_root();
    for src in rust_sources(&root.join("crates/inklkunn-prikk/src")) {
        let text = fs::read_to_string(&src).unwrap_or_default();
        let hits = write_verb_literals(&text);
        assert!(
            hits.is_empty(),
            "PRIKK SPAWN violation (RFC 001 D-2 c): the read driver {} names write verbs {hits:?}",
            src.display()
        );
    }
}

#[test]
fn prikk_spawn_gate_catches_planted_violations() {
    assert_eq!(
        prikk_spawn_lines("let c = Command::new(\"prikk\");\n"),
        vec![1]
    );
    assert!(prikk_spawn_lines("let c = Command::new(\"curl\");\n").is_empty());
    assert_eq!(
        write_verb_literals("repo.run(&[\"seal\", \"--allow-no-audit\"])\n"),
        vec![(1, "seal".to_owned())]
    );
    assert!(write_verb_literals("repo.run(&[\"log\", \"--format\", \"json\"])\n").is_empty());
    assert!(
        write_verb_literals("    #[serde(rename = \"ref\")]\n    pub view: String,\n").is_empty(),
        "a serde field attribute names a report key, not a spawn argument"
    );
}

#[test]
fn the_markup_crate_is_pure() {
    let root = workspace_root();
    let manifest =
        fs::read_to_string(root.join("crates/inklkunn-markup/Cargo.toml")).unwrap_or_default();
    for dep in runtime_dependencies(&manifest) {
        assert!(
            MARKUP_ALLOWED_DEPS.contains(&dep.as_str()),
            "MARKUP PURITY violation (RFC 001 D-2 d): `inklkunn-markup` depends on `{dep}`"
        );
    }
    for src in rust_sources(&root.join("crates/inklkunn-markup/src")) {
        let text = fs::read_to_string(&src).unwrap_or_default();
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
        runtime_dependencies("[dependencies]\ntokio = \"1\"\n\n[dev-dependencies]\nfoo = \"1\"\n"),
        vec!["tokio".to_owned()]
    );
}
