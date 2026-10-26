//! A created wiki passes `prikk verify`, has the CT-02 layout, and its first change reads back (handoff
//! 001 T5). Self-skips with a visible line when prikk is not on `PATH`; confined when bubblewrap works.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::Duration;

use inklkunn_core::bundle::{HELP_PAGE, check_layout};
use inklkunn_core::create_wiki;
use inklkunn_prikk::sandbox::probe_bwrap;
use inklkunn_prikk::{Admission, Point, PrikkCli, Sandbox, Storage};
use inklkunn_signer::{BootstrapSigner, KeyDir};
use inklkunn_store::{MemoryStore, NewWiki, RootLayout, Slug, VerificationState, WikiStore};

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> TempDir {
    let p = std::env::temp_dir().join(format!("inklkunn-core-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp");
    TempDir(p)
}

fn prikk_on_path() -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("prikk").is_file()))
        .unwrap_or(false)
}

fn sandbox() -> Sandbox {
    if probe_bwrap(std::ffi::OsStr::new("bwrap")).is_ok() {
        println!("policy: confined (bubblewrap)");
        Sandbox::default()
    } else {
        println!("skipping: bubblewrap cannot confine a process here; running unconfined");
        Sandbox::Unconfined
    }
}

#[test]
fn a_created_wiki_verifies_and_has_the_bundle_layout() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let tmp = temp("create");
    let sandbox = sandbox();
    let layout = RootLayout::new(tmp.0.join("root"));
    let keys = KeyDir::new(tmp.0.join("keys"));
    let signer = BootstrapSigner::new("prikk", sandbox.clone(), keys);
    let storage = PrikkCli::open(
        "prikk",
        sandbox,
        Admission::new(1 << 30, 64 << 20, Duration::from_millis(200)),
    )
    .expect("prikk at the floor");
    let store = MemoryStore::new();

    let slug = Slug::new("first-wiki").expect("slug");
    let created = create_wiki(
        &store,
        &layout,
        &signer,
        &storage,
        NewWiki::new(slug.clone(), "First Wiki"),
        "0.0.0",
    )
    .unwrap_or_else(|e| panic!("{e}"));

    // CT-02 layout, and nothing of ours inside .prikk.
    check_layout(&created.repo).unwrap_or_else(|e| panic!("{e}"));
    assert!(created.repo.join(".prikk").is_dir());
    assert!(created.repo.starts_with(layout.root()));
    assert!(
        !created.repo.to_string_lossy().contains("first-wiki"),
        "the directory is the id's"
    );

    // The record carries key ids (never seeds) and the verification.
    let record = store.by_slug(&slug).expect("record");
    assert!(
        record
            .author_key_id
            .as_deref()
            .is_some_and(|k| k.starts_with("ed25519-"))
    );
    assert!(
        record
            .maintainer_key_id
            .as_deref()
            .is_some_and(|k| k.starts_with("ed25519-"))
    );
    assert_eq!(record.verification.state, VerificationState::Verified);
    assert!(record.verification.at.is_some());

    // The repository verifies through the read driver, and the first change is the help page.
    let v = storage.verify(&created.repo).expect("verify");
    assert!(v.verdict.ok);
    let main = Point::View(record.default_view.clone());
    let tree = storage.tree(&created.repo, &main, None).expect("tree");
    let paths: Vec<&str> = tree.entries.iter().map(|e| e.path.as_str()).collect();
    for expected in [
        HELP_PAGE,
        "wiki-bundle.yaml",
        ".prikkignore",
        "attachments/.keep",
        "knowledge/.keep",
        "governance/.keep",
    ] {
        assert!(paths.contains(&expected), "{expected} in {paths:?}");
    }
    let history = storage
        .history(&created.repo, &record.default_view, None)
        .expect("history");
    assert_eq!(history.blocks.len(), 1);
    assert_eq!(
        history.blocks.first().map(|b| b.block_id.as_str()),
        Some(created.first_block_id.as_str())
    );
    let message = &history
        .blocks
        .first()
        .expect("block")
        .patch_messages
        .first()
        .expect("patch")
        .message;
    assert!(message.contains("Kind: system"), "{message}");
    let help = storage
        .read(&created.repo, &main, HELP_PAGE, 1 << 20)
        .expect("help");
    let page = inklkunn_markup::Page::parse(&String::from_utf8_lossy(&help)).expect("parses");
    assert_eq!(page.metadata.title.as_deref(), Some("Help"));

    // No seed anywhere in the repository or the record.
    let seeds: Vec<String> = ["author", "maintainer"]
        .iter()
        .map(|r| {
            std::fs::read_to_string(tmp.0.join("keys").join(format!("{r}.seed")))
                .expect("seed")
                .trim()
                .to_owned()
        })
        .collect();
    for seed in &seeds {
        assert_eq!(seed.len(), 64);
        let dump = format!("{record:?}{created:?}");
        assert!(!dump.contains(seed.as_str()), "a seed leaked into a record");
    }

    // A second wiki with the same slug is refused before anything is written.
    let again = create_wiki(
        &store,
        &layout,
        &signer,
        &storage,
        NewWiki::new(slug, "Again"),
        "0.0.0",
    );
    assert!(
        matches!(again, Err(inklkunn_core::CreateWikiError::Store(_))),
        "{again:?}"
    );
}
