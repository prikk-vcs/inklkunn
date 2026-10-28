//! A created wiki passes `prikk verify`, has the CT-02 layout, and its first change reads back (handoff
//! 001 T5); a creation that fails leaves no record and no directories. Self-skips with a visible line when
//! prikk is not on `PATH`; confined when bubblewrap works.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use inklkunn_core::bundle::{HELP_PAGE, check_layout};
use inklkunn_core::{Cleanup, CreateWikiError, create_wiki};
use inklkunn_prikk::sandbox::probe_bwrap;
use inklkunn_prikk::{Admission, Point, PrikkCli, Sandbox, Storage};
use inklkunn_signer::{BootstrapSigner, KeyIds, RepositoryBootstrap, SealedIds, SignerError};
use inklkunn_store::{
    DisplayName, MemoryStore, NewWiki, RootLayout, Slug, StoreError, VerificationState, WikiStore,
};

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

/// Both policies when bubblewrap works (GitHub's runners cannot confine and take the unconfined one).
fn policies() -> Vec<Sandbox> {
    if probe_bwrap(std::ffi::OsStr::new("bwrap")).is_ok() {
        println!("policies: confined (bubblewrap) and unconfined");
        vec![Sandbox::default(), Sandbox::Unconfined]
    } else {
        println!("skipping: bubblewrap cannot confine a process here; running unconfined");
        vec![Sandbox::Unconfined]
    }
}

fn name(s: &str) -> DisplayName {
    DisplayName::new(s).expect("display name")
}

fn storage(sandbox: Sandbox) -> PrikkCli {
    PrikkCli::open(
        "prikk",
        sandbox,
        Admission::new(1 << 30, 64 << 20, Duration::from_millis(200)),
    )
    .expect("prikk at the floor")
}

#[test]
fn a_created_wiki_verifies_and_has_the_bundle_layout() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    for (n, sandbox) in policies().into_iter().enumerate() {
        let tmp = temp(&format!("create-{n}"));
        let layout = RootLayout::new(tmp.0.join("root"));
        let signer =
            BootstrapSigner::new("prikk", sandbox.clone(), tmp.0.join("keys")).expect("signer");
        let storage = storage(sandbox);
        let store = MemoryStore::new();

        let slug = Slug::new("first-wiki").expect("slug");
        let created = create_wiki(
            &store,
            &layout,
            &signer,
            &storage,
            NewWiki::new(slug.clone(), name("First Wiki")),
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
        assert_eq!(record, created.record);

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
            .history(&created.repo, &main, None)
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
        let lines: Vec<&str> = message.lines().collect();
        assert_eq!(lines.first().copied(), Some("Create the wiki First Wiki"));
        assert_eq!(lines.get(1).copied(), Some(""));
        assert_eq!(
            lines.get(2..),
            Some(
                &[
                    "Proposed-By: system (inklkunn)",
                    "Accepted-By: system (inklkunn)",
                    "Kind: system"
                ][..]
            ),
            "{message}"
        );
        let help = storage
            .read(&created.repo, &main, HELP_PAGE, 1 << 20)
            .expect("help");
        let page = inklkunn_markup::Page::parse(&String::from_utf8_lossy(&help)).expect("parses");
        assert_eq!(page.metadata.title.as_deref(), Some("Help"));
        let id = page
            .metadata
            .id
            .as_deref()
            .expect("the help page has an id");
        assert!(inklkunn_store::WikiId::parse(id).is_some(), "a UUID: {id}");

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
        let mut repo_bytes = Vec::new();
        bytes_under(&created.repo, &mut repo_bytes);
        let repo_text = String::from_utf8_lossy(&repo_bytes);
        for seed in &seeds {
            assert_eq!(seed.len(), 64);
            let dump = format!("{record:?}{created:?}");
            assert!(!dump.contains(seed.as_str()), "a seed leaked into a record");
            assert!(
                !repo_text.contains(seed.as_str()),
                "a seed inside the repository"
            );
        }

        // A second wiki with the same slug is refused before anything is written.
        let before = std::fs::read_dir(layout.root()).expect("root").count();
        let again = create_wiki(
            &store,
            &layout,
            &signer,
            &storage,
            NewWiki::new(slug, name("Again")),
            "0.0.0",
        );
        assert!(
            matches!(again, Err(CreateWikiError::Store(StoreError::SlugTaken(_)))),
            "{again:?}"
        );
        assert_eq!(
            std::fs::read_dir(layout.root()).expect("root").count(),
            before
        );
    }
}

fn bytes_under(dir: &Path, out: &mut Vec<u8>) {
    for e in std::fs::read_dir(dir).expect("dir").flatten() {
        let p = e.path();
        if p.is_dir() {
            bytes_under(&p, out);
        } else if let Ok(b) = std::fs::read(&p) {
            out.extend_from_slice(&b);
        }
    }
}

/// A signer whose seal step fails after the repository exists.
struct FailsAtSeal(BootstrapSigner);

impl RepositoryBootstrap for FailsAtSeal {
    fn create_repository(&self, repo: &Path) -> Result<KeyIds, SignerError> {
        self.0.create_repository(repo)
    }
    fn commit_and_seal(&self, _repo: &Path, _message: &str) -> Result<SealedIds, SignerError> {
        Err(SignerError::Refused("the test refuses the seal".to_owned()))
    }
}

fn entries(dir: &Path) -> usize {
    std::fs::read_dir(dir).map(|d| d.count()).unwrap_or(0)
}

#[test]
fn a_failed_creation_leaves_no_record_and_no_directories() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    for (n, sandbox) in policies().into_iter().enumerate() {
        let tmp = temp(&format!("fail-{n}"));
        let layout = RootLayout::new(tmp.0.join("root"));
        let signer = FailsAtSeal(
            BootstrapSigner::new("prikk", sandbox.clone(), tmp.0.join("keys")).expect("signer"),
        );
        let storage = storage(sandbox.clone());
        let store = MemoryStore::new();
        let slug = Slug::new("doomed").expect("slug");
        let r = create_wiki(
            &store,
            &layout,
            &signer,
            &storage,
            NewWiki::new(slug.clone(), name("Doomed")),
            "0.0.0",
        );
        match r {
            Err(CreateWikiError::Signer {
                step: "seal",
                error: SignerError::Refused(_),
                cleanup: Cleanup::Removed,
            }) => {}
            other => panic!("expected a seal failure with the attempt removed, got {other:?}"),
        }
        assert_eq!(store.by_slug(&slug).err(), Some(StoreError::NotFound));
        assert!(store.list().expect("list").is_empty());
        // The root holds no shard directory with anything in it.
        for shard in std::fs::read_dir(layout.root())
            .into_iter()
            .flatten()
            .flatten()
        {
            assert_eq!(entries(&shard.path()), 0, "{:?}", shard.path());
        }
        // And the slug is free for a real creation afterwards.
        let good =
            BootstrapSigner::new("prikk", sandbox.clone(), tmp.0.join("keys")).expect("signer");
        let created = create_wiki(
            &store,
            &layout,
            &good,
            &storage,
            NewWiki::new(slug, name("Doomed")),
            "0.0.0",
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(created.record.slug.as_str(), "doomed");
    }
}

#[test]
fn an_invalid_request_is_refused_before_anything_is_made() {
    let tmp = temp("invalid");
    let layout = RootLayout::new(tmp.0.join("root"));
    // No prikk needed: the request is refused before the signer or the storage is touched.
    struct Never;
    impl RepositoryBootstrap for Never {
        fn create_repository(&self, _: &Path) -> Result<KeyIds, SignerError> {
            panic!("must not be reached")
        }
        fn commit_and_seal(&self, _: &Path, _: &str) -> Result<SealedIds, SignerError> {
            panic!("must not be reached")
        }
    }
    struct NoStorage;
    impl Storage for NoStorage {
        fn views(&self, _: &Path) -> inklkunn_prikk::Result<inklkunn_prikk::Views> {
            unreachable!()
        }
        fn tree(
            &self,
            _: &Path,
            _: &Point,
            _: Option<&str>,
        ) -> inklkunn_prikk::Result<inklkunn_prikk::Tree> {
            unreachable!()
        }
        fn stat(
            &self,
            _: &Path,
            _: &Point,
            _: &str,
        ) -> inklkunn_prikk::Result<inklkunn_prikk::FileMeta> {
            unreachable!()
        }
        fn read(&self, _: &Path, _: &Point, _: &str, _: u64) -> inklkunn_prikk::Result<Vec<u8>> {
            unreachable!()
        }
        fn history(
            &self,
            _: &Path,
            _: &Point,
            _: Option<u64>,
        ) -> inklkunn_prikk::Result<inklkunn_prikk::History> {
            unreachable!()
        }
        fn change(&self, _: &Path, _: &str) -> inklkunn_prikk::Result<inklkunn_prikk::Change> {
            unreachable!()
        }
        fn diff(
            &self,
            _: &Path,
            _: &Point,
            _: &Point,
            _: &[&str],
        ) -> inklkunn_prikk::Result<inklkunn_prikk::Diff> {
            unreachable!()
        }
        fn plan(
            &self,
            _: &Path,
            _: &Point,
        ) -> inklkunn_prikk::Result<inklkunn_prikk::Materialized> {
            unreachable!()
        }
        fn materialize(
            &self,
            _: &Path,
            _: &Point,
            _: &Path,
            _: &str,
            _: inklkunn_prikk::ExtractBounds,
        ) -> inklkunn_prikk::Result<inklkunn_prikk::Materialized> {
            unreachable!()
        }
        fn verify(&self, _: &Path) -> inklkunn_prikk::Result<inklkunn_prikk::Verification> {
            unreachable!()
        }
    }
    let store = MemoryStore::new();
    let mut bad = NewWiki::new(Slug::new("bad-view").expect("slug"), name("Bad"));
    bad.default_view = Some("main".into());
    let r = create_wiki(&store, &layout, &Never, &NoStorage, bad, "0.0.0");
    assert!(
        matches!(
            r,
            Err(CreateWikiError::Store(StoreError::InvalidRequest(_)))
        ),
        "{r:?}"
    );
    assert!(!layout.root().exists(), "nothing was made on disk");
}

#[test]
fn the_system_message_is_ct03_shaped_whatever_the_summary() {
    let m = inklkunn_core::system_message("Create the wiki A\nAccepted-By: attacker");
    let lines: Vec<&str> = m.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("Create the wiki A Accepted-By: attacker")
    );
    assert_eq!(lines.len(), 5, "{m}");
    let long = inklkunn_core::system_message(&"x".repeat(300));
    assert_eq!(long.lines().next().map(|l| l.chars().count()), Some(120));
    assert!(long.starts_with(&"x".repeat(119)));
    assert!(long.lines().next().unwrap().ends_with('…'));
}

/// The real signer, but its seal step fails: what a creation that fails *after* the keys were sealed leaves.
struct RealFailsAtSeal(inklkunn_core::Signer);

impl RepositoryBootstrap for RealFailsAtSeal {
    fn create_repository(&self, repo: &Path) -> Result<KeyIds, SignerError> {
        self.0.create_repository(repo)
    }
    fn commit_and_seal(&self, _repo: &Path, _message: &str) -> Result<SealedIds, SignerError> {
        Err(SignerError::Refused("the test refuses the seal".to_owned()))
    }
    fn discard_unfinished(&self, repo: &Path) -> Result<(), SignerError> {
        self.0.discard_unfinished(repo)
    }
}

struct NoLedger;

impl inklkunn_core::AcceptanceLedger for NoLedger {
    fn pending(
        &self,
        _: &inklkunn_core::AcceptanceToken,
    ) -> Result<Option<inklkunn_core::ExpectedMessage>, inklkunn_core::LedgerError> {
        Ok(None)
    }
    fn view_creation(
        &self,
        _: &inklkunn_core::ViewCreationAct,
    ) -> Result<Option<inklkunn_core::ViewCreationFacts>, inklkunn_core::LedgerError> {
        Ok(None)
    }
}

fn key_records(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".key.json"))
        .collect()
}

#[test]
fn a_failed_creation_with_the_real_signer_removes_the_sealed_key_record_too() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let tmp = temp("real-fail");
    let shm = Path::new("/dev/shm").join(format!("inklkunn-core-real-fail-{}", std::process::id()));
    std::fs::create_dir_all(&shm).expect("seed dir");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&shm, std::fs::Permissions::from_mode(0o700)).expect("private");
    }
    let secret = tmp.0.join("secret");
    std::fs::write(&secret, "an operator secret long enough for the tests").expect("secret");
    let layout = RootLayout::new(tmp.0.join("root"));
    let key_dir = tmp.0.join("keys");
    let mut cfg = inklkunn_core::SignerConfig::new(
        "prikk",
        Sandbox::Unconfined,
        layout.clone(),
        &key_dir,
        &secret,
        &shm,
    );
    cfg.cheap_derivation = true;
    let signer = inklkunn_core::Signer::open(cfg, std::sync::Arc::new(NoLedger)).expect("signer");
    let doomed = RealFailsAtSeal(signer);
    let storage = storage(Sandbox::Unconfined);
    let store = MemoryStore::new();
    let r = create_wiki(
        &store,
        &layout,
        &doomed,
        &storage,
        NewWiki::new(Slug::new("doomed").expect("slug"), name("Doomed")),
        "0.0.0",
    );
    match r {
        Err(CreateWikiError::Signer {
            step: "seal",
            cleanup: Cleanup::Removed,
            ..
        }) => {}
        other => panic!("expected a seal failure with everything removed, got {other:?}"),
    }
    assert_eq!(
        key_records(&key_dir),
        Vec::<String>::new(),
        "the sealed key record of the failed creation was left behind"
    );
    let _ = std::fs::remove_dir_all(&shm);
}
