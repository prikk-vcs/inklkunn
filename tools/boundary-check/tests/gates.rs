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
    // (The line is also a `Mounts::write` match, by prefix; the point is that the keyless constructor is named.)
    assert!(
        signer_only_mentions("let m = Mounts::write_keyless_with_output(repo, out);\n")
            .contains(&(1, "Mounts::write_keyless_with_output".to_owned())),
        "the keyless writable mode is the signer's too"
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
fn only_the_driver_the_signer_and_the_fetcher_spawn_a_process() {
    let root = workspace_root();
    for src in rust_sources(&root.join("crates")) {
        let text = read(&src);
        if src.starts_with(root.join("crates/inklkunn-fetch")) {
            // The third crate allowed to spawn, and only the sandbox that runs curl (or curl, unconfined,
            // for tests): see `fetch_spawn_violations`.
            let hits = fetch_spawn_violations(&text);
            assert!(
                hits.is_empty(),
                "PRIKK SPAWN violation (LAY-3, IQ-3): {} spawns something other than bwrap or curl: {hits:?}",
                src.display()
            );
            continue;
        }
        let allowed = src.starts_with(root.join("crates/inklkunn-prikk"))
            || src.starts_with(root.join("crates/inklkunn-signer"));
        if allowed {
            continue;
        }
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
fn the_fetch_crate_spawns_only_bwrap_or_curl() {
    // Allowed: the sandbox by name, by field or by constant, and curl itself.
    for ok in [
        "let mut c = Command::new(bwrap);\n",
        "let mut c = Command::new(&self.bwrap);\n",
        "let mut c = Command::new(BWRAP);\n",
        "let mut c = std::process::Command::new(curl);\n",
        "let mut c = Command::new(\"curl\");\n",
        "let mut c = Command::new(\n    bwrap,\n);\n",
        "// Command::new(\"prikk\") in prose\n",
        "let c = something.new(x);\n",
    ] {
        assert!(fetch_spawn_violations(ok).is_empty(), "{ok}");
    }
    // Refused: anything else, in any spelling, with the line.
    for (bad, line) in [
        ("let c = Command::new(\"prikk\");\n", 1),
        ("\n\nlet c = Command::new(\"/bin/sh\");\n", 3),
        ("let c = Command::new(&binary);\n", 1),
        ("let c = std::process::Command::new(prikk_binary);\n", 1),
        ("let c = Command::new(program());\n", 1),
        ("let c = Command::new(\n  \"sh\",\n);\n", 1),
        ("let c = Command::new (\"sh\");\n", 1),
        ("let c = Command::new(\"curl-wrapper\");\n", 1),
    ] {
        let hits = fetch_spawn_violations(bad);
        assert_eq!(hits.len(), 1, "{bad}: {hits:?}");
        assert_eq!(hits.first().map(|h| h.0), Some(line), "{bad}");
    }
    assert!(!fetch_spawn_violations("let c = tokio::process::Command::new(bwrap);\n").is_empty());
    assert_eq!(
        command_new_arguments("let a = Command::new(f(x, (y)));\n"),
        vec![(1, "f(x, (y))".to_owned())]
    );
    // Discovery: a spawn planted anywhere under the fetch crate is found by the same walk the real gate does.
    let scratch = Scratch::new("fetchspawn");
    let dir = scratch.0.join("crates/inklkunn-fetch/tests/deep");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("x.rs"), "let c = Command::new(\"prikk\");\n").unwrap();
    let found = rust_sources_under(&scratch.0, &scratch.0.join("crates"));
    assert_eq!(found.len(), 1);
    assert!(
        found
            .first()
            .is_some_and(|f| !fetch_spawn_violations(&read(f)).is_empty())
    );
    // A spawn in the crate next door is still a spawn violation of the general kind.
    assert_eq!(spawn_lines("let c = Command::new(bwrap);\n"), vec![1]);
}

#[test]
fn only_the_fetch_crate_names_curl() {
    let root = workspace_root();
    for src in rust_sources(&root.join("crates")) {
        if src.starts_with(root.join("crates/inklkunn-fetch")) {
            continue;
        }
        let hits = curl_mentions(&read(&src));
        assert!(
            hits.is_empty(),
            "CURL violation (IQ-3): {} names curl at lines {hits:?}; only inklkunn-fetch may",
            src.display()
        );
    }
    // The fetch crate itself does name it (the gate is not looking at an empty set).
    let fetch_names: usize = rust_sources(&root.join("crates/inklkunn-fetch"))
        .iter()
        .map(|p| curl_mentions(&read(p)).len())
        .sum();
    assert!(fetch_names > 0);
}

#[test]
fn curl_gate_catches_every_spelling_and_spares_prose_and_other_words() {
    assert_eq!(curl_mentions("let c = Command::new(\"curl\");\n"), vec![1]);
    assert_eq!(curl_mentions("\n\nconst P: &str = \"CURL\";\n"), vec![3]);
    assert_eq!(curl_mentions("let curl = path;\n"), vec![1]);
    assert_eq!(curl_mentions("struct Curl;\n"), vec![1]);
    assert_eq!(curl_mentions("run(\"/usr/bin/curl\", args);\n"), vec![1]);
    assert_eq!(curl_mentions("x(&[\"curl\", \"-q\"]);\n"), vec![1]);
    assert!(curl_mentions("// curl is spawned by inklkunn-fetch\n").is_empty());
    assert!(curl_mentions("/* curl */ let x = 1;\n").is_empty());
    assert!(curl_mentions("let x = CurlFetcher::new();\n").is_empty());
    assert!(curl_mentions("let curly = 1;\n").is_empty());
}

#[test]
fn only_the_binary_depends_on_the_fetch_crate_and_the_fetch_crate_depends_on_the_driver_alone() {
    let manifests = read_manifests(&workspace_root());
    assert!(
        manifests.contains_key(FETCH_CRATE),
        "the fetch crate is a member"
    );
    let d = fetch_dependents(&manifests);
    assert!(
        d.is_empty(),
        "CURL violation (IQ-3): {d:?} depend on `inklkunn-fetch`; only the binary may"
    );
    let own = manifests.get(FETCH_CRATE).cloned().unwrap_or_default();
    let extra = fetch_disallowed_deps(&own);
    assert!(
        extra.is_empty(),
        "CURL violation (IQ-3): inklkunn-fetch depends on {extra:?}; only the driver is allowed (no TLS or HTTP crate)"
    );
    assert!(
        LOWER.contains(&FETCH_CRATE),
        "the fetch crate is a lower layer"
    );
    assert!(
        !test_feature_leaks(&manifests)
            .iter()
            .any(|(_, dep, _)| dep == FETCH_CRATE)
    );
}

#[test]
fn fetch_dependency_gates_catch_planted_edges_and_spare_the_binary() {
    for crate_name in [
        "inklkunn-core",
        "inklkunn-web",
        "inklkunn-auth",
        "inklkunn-store",
        "inklkunn-signer",
    ] {
        for plant in [
            "[dependencies]\ninklkunn-fetch = { path = \"../inklkunn-fetch\" }\n",
            "[dependencies]\ninklkunn-fetch.workspace = true\n",
            "[dependencies.inklkunn-fetch]\npath = \"../inklkunn-fetch\"\n",
            "[dependencies]\nfetch = { package = \"inklkunn-fetch\", path = \"../inklkunn-fetch\" }\n",
        ] {
            let mut m = read_manifests(&workspace_root());
            m.insert(crate_name.into(), plant.into());
            assert_eq!(
                fetch_dependents(&m),
                vec![crate_name.to_owned()],
                "{crate_name}: {plant}"
            );
        }
    }
    // The binary may; a dev-dependency is not an edge.
    let mut m = read_manifests(&workspace_root());
    m.insert(
        "inklkunn".into(),
        "[dependencies]\ninklkunn-fetch.workspace = true\n".into(),
    );
    assert!(fetch_dependents(&m).is_empty());
    m.insert(
        "inklkunn-core".into(),
        "[dev-dependencies]\ninklkunn-fetch = { path = \"x\" }\n".into(),
    );
    assert!(fetch_dependents(&m).is_empty());
    // The fetch crate reaching for a core crate is an upward edge (it is a lower layer)...
    let mut m = read_manifests(&workspace_root());
    m.insert(
        FETCH_CRATE.into(),
        "[dependencies]\ninklkunn-core.workspace = true\n".into(),
    );
    assert_eq!(
        layering_violations(&m),
        vec![(FETCH_CRATE.to_owned(), "inklkunn-core".to_owned())]
    );
    m.insert(
        FETCH_CRATE.into(),
        "[dependencies]\ninklkunn-auth.workspace = true\n".into(),
    );
    assert_eq!(
        layering_violations(&m),
        vec![(FETCH_CRATE.to_owned(), "inklkunn-auth".to_owned())]
    );
    // ...and any other dependency of it is refused by name, a TLS crate first.
    for dep in [
        "rustls",
        "reqwest",
        "native-tls",
        "openssl",
        "hyper",
        "inklkunn-store",
        "serde",
    ] {
        let manifest = format!(
            "[dependencies]\ninklkunn-prikk = {{ path = \"../inklkunn-prikk\" }}\n{dep} = \"1\"\n"
        );
        assert_eq!(
            fetch_disallowed_deps(&manifest),
            vec![dep.to_owned()],
            "{dep}"
        );
    }
    assert!(
        fetch_disallowed_deps(
            "[dependencies]\ninklkunn-prikk = \"0\"\n[dev-dependencies]\nproptest = \"1\"\n"
        )
        .is_empty()
    );
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

#[test]
fn routes_are_registered_in_the_registry_file_and_nowhere_else() {
    let root = workspace_root();
    // Exactly one file may register routes: matching by file name would let `src/admin/routes.rs` through.
    let registry = root
        .join("crates/inklkunn-web/src")
        .join(ROUTE_REGISTRY_FILE);
    for src in rust_sources(&root.join("crates/inklkunn-web/src")) {
        if src == registry {
            continue;
        }
        let hits = route_registrations(&read(&src));
        assert!(
            hits.is_empty(),
            "ROUTE REGISTRATION violation (ENF-3): {} registers routes outside `{ROUTE_REGISTRY_FILE}` at {hits:?}; \
             declare them in the route table and register them there",
            src.display()
        );
    }
}

#[test]
fn route_registration_gate_catches_every_spelling_and_spares_prose() {
    for planted in [
        "let app = Router::new();\n",
        "app.route(\"/x\", get(h))\n",
        "    .route(\"/leak\", get(leak))\n",
        "    .route (\"/leak\", get(leak))\n",
        "    .route\n    (\"/leak\", get(leak))\n",
        "app.nest(\"/admin\", admin())\n",
        "app.nest_service(\"/f\", serve)\n",
        "app.route_service(\"/f\", serve)\n",
        "app.merge(other)\n",
        "app.fallback(h)\n",
        "app.fallback_service(s)\n",
        "use axum::Router; let r = axum::Router::new();\n",
        "let app = Router::route(app, \"/leak\", get(h));\n",
        "let app = axum::Router::merge(a, b);\n",
        "let app = Router::nest(a, \"/x\", b);\n",
        "let app = Router::fallback(a, h);\n",
    ] {
        assert!(!route_registrations(planted).is_empty(), "{planted:?}");
    }
    // The line number is the call's.
    assert_eq!(
        route_registrations(
            "fn a() {}\n\nfn b() {\n    app\n        .route\n        (\"/x\", h)\n}\n"
        ),
        vec![(5, ".route(".to_owned())]
    );
    for fine in [
        "// Router::new() and .route(\"/x\", h) are only mentioned\n",
        "let s = \"app.route(\\\"/x\\\")\";\n",
        "let t = SubRouter::new();\n",
        "/* app.merge(other) */ let a = 1;\n",
        "let route = table.route_for(path);\n",
    ] {
        assert_eq!(route_registrations(fine), vec![], "{fine:?}");
    }
    // Discovery: a planted web source other than the registry file is found; the registry file is exempt.
    let scratch = Scratch::new("routes");
    let web = scratch.0.join("crates/inklkunn-web/src");
    fs::create_dir_all(web.join("deep")).unwrap();
    fs::write(web.join("routes.rs"), "app.route(\"/ok\", h)\n").unwrap();
    fs::write(web.join("deep/page.rs"), "app.nest(\"/x\", y)\n").unwrap();
    let offenders: Vec<_> = rust_sources_under(&scratch.0, &web)
        .into_iter()
        .filter(|p| p.file_name().is_none_or(|n| n != ROUTE_REGISTRY_FILE))
        .filter(|p| !route_registrations(&read(p)).is_empty())
        .collect();
    assert_eq!(offenders.len(), 1, "{offenders:?}");
}

#[test]
fn a_web_handler_receives_a_principal_it_never_builds() {
    let root = workspace_root();
    for src in rust_sources(&root.join("crates/inklkunn-web")) {
        if !is_product_source(&src) {
            continue;
        }
        let hits = privileged_principal_mentions(&read(&src));
        assert!(
            hits.is_empty(),
            "PRIVILEGED PRINCIPAL violation (INV-4): {} names {hits:?}; a handler is handed the principal \
             inklkunn-auth produced and never builds one",
            src.display()
        );
    }
}

#[test]
fn privileged_principal_gate_catches_every_construction_and_spares_prose() {
    for planted in [
        "let p = Principal::System(s);\n",
        "let p = Principal :: System(s);\n",
        "let p: SystemPrincipal = x;\n",
        "let p = Principal::Account { id, scope: Scope::Admin };\n",
        "let p = Principal::session(id);\n",
        "let p = Principal::token(id, scope);\n",
        "use inklkunn_core::Principal::System as S;\n",
        "match p { inklkunn_core::Principal::Account { .. } => {} _ => {} }\n",
        // The spellings that name a constructor or a variant without `Principal::Account` written out.
        "use inklkunn_core::Principal as P;\nlet p = P::session(id);\n",
        "use inklkunn_core::Principal::*;\nlet p = Account { id, scope };\n",
        "use inklkunn_core::Principal::{Account, Anonymous};\n",
        "let p = <Principal>::session(id);\n",
        // The witness itself: a surface has no business with it.
        "let a = IdentityAuthority::claim();\n",
        "fn f(a: &AccountPrincipal) {}\n",
    ] {
        assert!(
            !privileged_principal_mentions(planted).is_empty(),
            "{planted:?}"
        );
    }
    for fine in [
        "fn h(principal: Principal) {}\n",
        "let p = Principal::Anonymous;\n",
        "// Principal::System is minted by core only\n",
        "let m = \"Principal::session\";\n",
        "let n = MySystemPrincipalLike;\n",
    ] {
        assert_eq!(privileged_principal_mentions(fine), vec![], "{fine:?}");
    }
}

#[test]
fn no_public_item_of_the_signer_mentions_a_seed_a_secret_or_a_key_directory() {
    let root = workspace_root();
    for src in rust_sources(&root.join("crates/inklkunn-signer/src")) {
        let hits = public_seed_exposures(&read(&src));
        assert!(
            hits.is_empty(),
            "PUBLIC SEED violation (INV-2, RFC 002 D-3): {} declares public items that mention {hits:?}; \
             nothing the signer exports may return or name key material",
            src.display()
        );
    }
}

#[test]
fn the_public_seed_gate_catches_every_shape_and_spares_what_is_not_public() {
    for planted in [
        "pub fn seed_path(&self) -> PathBuf {\n",
        "pub fn read_seed(&self) -> Vec<u8> {\n",
        "pub fn Seed_bytes(&self) -> Vec<u8> {\n",
        "pub fn key_dir(&self) -> &KeyDir {\n",
        "pub struct Holder { pub dir: SeedDir }\n",
        "pub fn open(secret: Secret) {}\n",
        "pub use key_store::KeyStore;\n",
        "pub fn bytes(&self) -> Zeroizing<Vec<u8>> {\n",
        "    pub fn master(&self) -> &MasterKey {\n",
        "pub fn base(&self) -> &SeedBase {\n",
        "pub fn unlock(&self) -> Seeds {\n",
    ] {
        assert!(!public_seed_exposures(planted).is_empty(), "{planted:?}");
    }
    for fine in [
        "pub(crate) fn seed_path(&self) -> PathBuf {\n",
        "pub(super) struct Seeds {\n",
        "pub(crate) use seeds::SeedDir;\n",
        "fn seed_path(&self) -> PathBuf {\n",
        "/// pub fn seed_path() is only mentioned\n",
        "let s = \"pub fn seed_path\";\n",
        "pub fn sealed_ids(&self) -> SealedIds {\n",
        "pub struct KeyIds { pub author: String }\n",
        "pub fn secretary() {}\n",
        "pub fn open_keystore_at(p: &Path) {}\n",
    ] {
        assert_eq!(public_seed_exposures(fine), vec![], "{fine:?}");
    }
    // A type name is matched as a word: `Secretary` and `KeyStoreLike` are other names.
    assert_eq!(public_seed_exposures("pub fn f(a: Secretary) {}\n"), vec![]);
    assert_eq!(public_seed_exposures("pub fn f(a: Secret) {}\n").len(), 1);
}

#[test]
fn no_surface_names_the_signer() {
    let root = workspace_root();
    for krate in ["crates/inklkunn-web", "crates/inklkunn-auth"] {
        for src in rust_sources(&root.join(krate)) {
            if !is_product_source(&src) {
                continue;
            }
            let hits = signer_mentions_in_a_surface(&read(&src));
            assert!(
                hits.is_empty(),
                "SIGNER REACH violation (C-1a): {} names {hits:?}; a surface reaches signing through \
                 inklkunn-core's services and never opens or drives the signer",
                src.display()
            );
        }
    }
}

#[test]
fn the_signer_gate_catches_every_way_a_surface_could_open_one_and_spares_prose() {
    for planted in [
        "let s = Signer::open(cfg, ledger)?;\n",
        "let c = SignerConfig::new(a, b, c, d, e, f);\n",
        "s.rotate_keys(&repo, OldKey::Retire)?;\n",
        "impl AcceptanceLedger for MyLedger {}\n",
        "use inklkunn_signer::Signer;\n",
        "fn f(r: &dyn RepositoryBootstrap) {}\n",
        "let k = KeyAuthority::claim();\n",
    ] {
        assert!(
            !signer_mentions_in_a_surface(planted).is_empty(),
            "{planted:?}"
        );
    }
    for fine in [
        "// the Signer is opened by the binary\n",
        "let a = Action::Accept;\n",
        "let s = \"Signer::open\";\n",
        "let t = SignerLike::new();\n",
        "fn assigner() {}\n",
    ] {
        assert_eq!(signer_mentions_in_a_surface(fine), vec![], "{fine:?}");
    }
}
