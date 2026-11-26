//! The interwiki map of a bundle comes in with the wiki (RFC 007 D-8; handoff I-T4), with the real signer and a real prikk: the plan's judged
//! map is written to `governance/interwiki.yaml` in the first change, in its canonical bytes, and nothing else of `governance/` is; the
//! imported wiki resolves a mapped prefix as an external link and an unmapped one as it did; and an export of the imported wiki carries the
//! map, so that importing that export again makes the same map. Self-skips with a visible line when prikk is not on `PATH`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "support/replay.rs"]
mod replay;

use std::path::Path;
use std::sync::Arc;

use inklkunn_core::export::{HistoryInputs, KnowledgeRecords, export_bundle_with};
use inklkunn_core::message::Kind;
use inklkunn_core::read::{ReadConfig, ReadService, TransclusionRefusal};
use inklkunn_core::{PageOutcome, Presentation};
use replay::{Dir, dir_in, fixture, messages, plan_of, prikk_on_path, read_text, run, world};

/// The map of the hand-made fixture as the import writes it.
const FIXTURE_MAP: &str = "docs: \"https://docs.example.org/read?page={title}\"\nwikipedia: \"https://en.wikipedia.org/wiki/{title}\"\n";

/// English under `/w`.
struct En;

impl Presentation for En {
    fn lang(&self) -> &str {
        "en"
    }
    fn key(&self) -> String {
        "/w".to_owned()
    }
    fn page_href(&self, title: &str) -> String {
        format!("/w/{}", inklkunn_markup::percent_encode(title))
    }
    fn create_href(&self, title: &str) -> Option<String> {
        Some(format!(
            "/w/-/create?title={}",
            inklkunn_markup::percent_encode(title)
        ))
    }
    fn transclusion_label(&self, title: &str) -> String {
        format!("From {title}")
    }
    fn transclusion_notice(&self, refusal: &TransclusionRefusal) -> String {
        format!("Not included: {refusal}")
    }
}

fn put(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// The files under `governance/` of a repository, by name, without the `.keep` the wiki's creation puts in every directory.
fn governance_files(repo: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(repo.join("governance"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name != ".keep")
        .collect();
    out.sort();
    out
}

fn reads_of(w: &replay::World) -> ReadService {
    ReadService::new(
        w.storage.clone(),
        w.store.clone(),
        w.store.clone(),
        w.layout.clone(),
        ReadConfig::default(),
        None,
    )
    .with_tombstones(Arc::new(inklkunn_core::tombstone::Tombstones::new(
        w.store.clone(),
        w.store.clone(),
    )))
}

#[test]
fn the_bundles_map_comes_in_with_the_first_change_in_its_canonical_bytes_and_the_pages_resolve_with_it()
 {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let w = world("iw-import");
    let plan = plan_of(&fixture("interwiki/bundle"), None);
    assert_eq!(plan.interwiki.to_yaml(), FIXTURE_MAP);
    let done = run(&w, &plan, "seeds", "CC-BY-SA-4.0", &|_| {}).expect("imported");
    // The creation and one change: the two pages and the map, in the first change.
    assert!(done.first_change && done.revisions == 0 && done.final_changes == 0);
    let blocks = messages(&w, &done.record.id);
    assert_eq!(blocks.len(), 2);
    let first = blocks.get(1).unwrap();
    assert_eq!(first.kind(), Kind::Import);
    assert_eq!(
        first.summary().as_str(),
        "Import 2 page(s) and 0 other file(s) with no history, and the interwiki map"
    );
    // The file is the plan's map in the canonical text, byte for byte, and it is the only file of the bundle's governance there is.
    assert_eq!(
        std::fs::read(done.repo.join("governance/interwiki.yaml")).unwrap(),
        FIXTURE_MAP.as_bytes()
    );
    assert_eq!(governance_files(&done.repo), vec!["interwiki.yaml"]);
    assert!(w.storage.verify(&done.repo).expect("verify").verdict.ok);
    // The wiki's own reader has the map, and renders a mapped prefix as an external link and an unmapped one as it was: a missing page.
    let reads = reads_of(&w);
    assert_eq!(
        reads.interwiki_of(&done.record, None).unwrap().to_yaml(),
        FIXTURE_MAP
    );
    let PageOutcome::Found(view) = reads
        .page(&done.record, None, "Seed Library", &En)
        .expect("page")
    else {
        panic!("the page is there");
    };
    let html = view.html.as_str();
    assert!(
        html.contains("href=\"https://en.wikipedia.org/wiki/Seed%20bank\""),
        "{html}"
    );
    assert!(
        html.contains("href=\"https://docs.example.org/read?page=Sowing%20guide\""),
        "{html}"
    );
    assert!(html.contains("rel=\"nofollow ugc\""), "{html}");
    assert!(
        view.links.dangling.iter().any(|t| t == "forum:Seed swap"),
        "{:?}",
        view.links
    );
    assert!(!view.links.interwiki.iter().any(|t| t.starts_with("forum")));
    // The report kept with the act says what was written and what was left out.
    let kept = read_text(&w.layout.import_report(&done.record.id));
    assert!(
        kept.contains("the interwiki map the wiki will have"),
        "{kept}"
    );
    assert!(kept.contains("help: prefix-is-namespace"), "{kept}");
}

#[test]
fn an_exported_imported_wiki_carries_the_map_and_importing_that_makes_the_same_map() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let w = world("iw-roundtrip");
    let plan = plan_of(&fixture("interwiki/bundle"), None);
    let done = run(&w, &plan, "seeds", "CC-BY-SA-4.0", &|_| {}).expect("imported");
    let out = dir_in(&std::env::temp_dir(), "iw-roundtrip-out");
    let bundle = out.0.join("bundle");
    let reads = reads_of(&w);
    export_bundle_with(
        &*w.storage,
        &w.layout,
        &done.record,
        &bundle,
        "0.0.0",
        1_700_000_000,
        &KnowledgeRecords::default(),
        &mut HistoryInputs::new(&reads, &*w.store),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    // The manifest carries the map (wiki-commons' shape), and so does the file of the tree.
    let manifest = read_text(&bundle.join("wiki-bundle.yaml"));
    assert!(
        manifest.contains(
            "interwiki:\n  \"docs\": \"https://docs.example.org/read?page={title}\"\n  \"wikipedia\": \"https://en.wikipedia.org/wiki/{title}\"\n"
        ),
        "{manifest}"
    );
    assert_eq!(
        std::fs::read(bundle.join("governance/interwiki.yaml")).unwrap(),
        FIXTURE_MAP.as_bytes()
    );
    // Importing the export gives the same map (the left-out `help` is not in it, and is not asked for again).
    let again = plan_of(&bundle, None);
    assert_eq!(again.interwiki, plan.interwiki);
    assert!(again.report.interwiki_left_out.is_empty());
    let w2 = world("iw-roundtrip-2");
    let done2 = run(&w2, &again, "seeds2", "CC-BY-SA-4.0", &|_| {}).expect("imported again");
    assert_eq!(
        std::fs::read(done2.repo.join("governance/interwiki.yaml")).unwrap(),
        FIXTURE_MAP.as_bytes()
    );
    assert_eq!(governance_files(&done2.repo), vec!["interwiki.yaml"]);
}

/// A bundle of one page that has a history (so nothing comes in with the first change but the map) and a manifest with `lines` below
/// `interwiki:`, and any `extra` files.
fn history_bundle(tag: &str, lines: &[&str], extra: &[(&str, &str)]) -> Dir {
    let d = dir_in(&std::env::temp_dir(), &format!("bundle-{tag}"));
    let text =
        |n: u32| format!("---\nid: home\ntitle: Home\n---\n\nVersion {n}. See [[wp:Home]].\n");
    let record = |rev: &str, parent: Option<&str>, at: &str, n: u32| {
        use sha2::{Digest, Sha256};
        let sha: String = Sha256::digest(text(n).as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        serde_json::json!({
            "rev": rev, "parent": parent, "type": if parent.is_none() { "create" } else { "edit" },
            "at": at, "author": {"name": "Ann"}, "summary": format!("change {rev}"),
            "content": text(n), "sha256": sha,
        })
        .to_string()
    };
    let mut manifest = String::from(
        "format: portable-wiki-bundle\nversion: '0.4'\nlicense: CC-BY-SA-4.0\nfidelity:\n  level: 3\ncontents:\n  history: true\ninterwiki:\n",
    );
    for l in lines {
        manifest.push_str("  ");
        manifest.push_str(l);
        manifest.push('\n');
    }
    put(&d.0, "wiki-bundle.yaml", &manifest);
    put(&d.0, "pages/Home.md", &text(2));
    put(
        &d.0,
        "history/home.jsonl",
        &format!(
            "{}\n{}\n",
            record("r1", None, "2025-01-01T00:00:00Z", 1),
            record("r2", Some("r1"), "2025-01-02T00:00:00Z", 2)
        ),
    );
    for (path, body) in extra {
        put(&d.0, path, body);
    }
    d
}

#[test]
fn a_bundle_whose_pages_all_have_a_history_gets_a_first_change_of_the_map_alone_and_a_bundle_with_no_map_gets_none()
 {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    // The map alone: every page has a history, so the first change holds the map and nothing else.
    let d = history_bundle(
        "iw-map-only",
        &["wp: https://en.wikipedia.org/wiki/{title}"],
        &[],
    );
    let plan = plan_of(&d.0, None);
    assert!(plan.first_pages.is_empty() && plan.plan.files.len() == 1);
    let w = world("iw-map-only");
    let done = run(&w, &plan, "maponly", "CC-BY-SA-4.0", &|_| {}).expect("imported");
    assert!(done.first_change);
    assert_eq!(done.revisions, 2);
    let blocks = messages(&w, &done.record.id);
    // The creation, the map, the two revisions, and the last change that brings the page to the bundle's file (if any).
    assert_eq!(
        blocks.get(1).unwrap().summary().as_str(),
        "Import the interwiki map"
    );
    assert!(blocks.get(1).unwrap().source().is_none());
    assert!(blocks.get(2).unwrap().source().is_some());
    assert_eq!(
        read_text(&done.repo.join("governance/interwiki.yaml")),
        "wp: \"https://en.wikipedia.org/wiki/{title}\"\n"
    );
    // The same bundle with no map: no first change at all, and no file in `governance/` but the `.keep` the wiki began with.
    let d = history_bundle("iw-no-map", &[], &[]);
    let plan = plan_of(&d.0, None);
    assert!(plan.interwiki.is_empty());
    let w = world("iw-no-map");
    let done = run(&w, &plan, "nomap", "CC-BY-SA-4.0", &|_| {}).expect("imported");
    assert!(!done.first_change);
    assert!(governance_files(&done.repo).is_empty());
}

#[test]
fn a_bundles_own_policy_and_constitution_are_never_written_by_an_import() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    // The hand-made bundle with the three governance files of an inklkunn export beside it, each one a file that would change who may do
    // what (or where a link leads) if it came in: the checksums name them, and the import still leaves them out.
    let d = dir_in(&std::env::temp_dir(), "bundle-iw-governance");
    copy_tree(&fixture("interwiki/bundle"), &d.0);
    put(
        &d.0,
        "governance/policy.yaml",
        "rungs:\n  propose: anonymous\n  accept: anonymous\n",
    );
    put(&d.0, "governance/constitution.yaml", "requirements: {}\n");
    put(
        &d.0,
        "governance/interwiki.yaml",
        "evil: \"https://evil.example.org/{title}\"\n",
    );
    let mut sums = String::new();
    for rel in [
        "governance/constitution.yaml",
        "governance/interwiki.yaml",
        "governance/policy.yaml",
        "pages/Help/Sowing.md",
        "pages/Seed Library.md",
    ] {
        use sha2::{Digest, Sha256};
        let sum: String = Sha256::digest(std::fs::read(d.0.join(rel)).unwrap())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        sums.push_str(&format!("{sum}  {rel}\n"));
    }
    put(&d.0, "sha256sums.txt", &sums);
    let plan = plan_of(&d.0, None);
    let w = world("iw-governance");
    let done = run(&w, &plan, "gov", "CC-BY-SA-4.0", &|_| {}).expect("imported");
    // Only the manifest's map is written, and not the bundle's file of it either.
    assert_eq!(governance_files(&done.repo), vec!["interwiki.yaml"]);
    assert_eq!(
        std::fs::read(done.repo.join("governance/interwiki.yaml")).unwrap(),
        FIXTURE_MAP.as_bytes()
    );
    assert!(!done.repo.join("governance/policy.yaml").exists());
    assert!(!done.repo.join("governance/constitution.yaml").exists());
}
