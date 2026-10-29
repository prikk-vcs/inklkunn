//! Export, `import-folder` and the round trip (handoff 003 T7; RFC 003 D-11, D-12): a wiki is filled by imports
//! and an accepted proposal, exported as a Portable Wiki Bundle, imported into a fresh wiki, and exported again;
//! the two bundles agree on text, links, metadata and the pages' histories. A real signer and a real prikk;
//! self-skips with a visible line when prikk is not on `PATH`; runs confined where bubblewrap works.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::{BTreeMap, HashMap};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use inklkunn_core::export::{ExportError, export_bundle};
use inklkunn_core::import::{ImportLedger, existing_paths, plan_folder, run_import};
use inklkunn_core::message::{Actor, BlockId, Message, Summary};
use inklkunn_core::{
    AcceptanceLedger, AcceptanceToken, CreatedWiki, ExpectedMessage, KeyAuthority, LedgerError,
    PreparedChange, PreparedFile, Signer, SignerConfig, ViewCreationAct, ViewCreationFacts,
    create_wiki,
};
use inklkunn_prikk::sandbox::probe_bwrap;
use inklkunn_prikk::storage::{
    Change, Diff, FileMeta, History, Materialized, Tree, Verification, Views,
};
use inklkunn_prikk::{Admission, ExtractBounds, Point, PrikkCli, Sandbox, Storage, StorageError};
use inklkunn_signer::Accept;
use inklkunn_store::{AccountId, DisplayName, MemoryStore, NewWiki, ProposalId, RootLayout, Slug};
use sha2::{Digest, Sha256};

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir_in(base: &Path, tag: &str) -> Dir {
    let p = base.join(format!("inklkunn-rt-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("dir");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).expect("private");
    Dir(p)
}

fn prikk_on_path() -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("prikk").is_file()))
        .unwrap_or(false)
}

fn policies() -> Vec<Sandbox> {
    if probe_bwrap(std::ffi::OsStr::new("bwrap")).is_ok() {
        vec![Sandbox::default(), Sandbox::Unconfined]
    } else {
        println!("skipping: bubblewrap cannot confine a process here; running unconfined");
        vec![Sandbox::Unconfined]
    }
}

/// The import ledger and a second one for the accepted proposal the test makes.
#[derive(Default)]
struct Ledgers {
    import: Arc<ImportLedger>,
    extra: Mutex<HashMap<String, ExpectedMessage>>,
}

impl AcceptanceLedger for Ledgers {
    fn pending(&self, token: &AcceptanceToken) -> Result<Option<ExpectedMessage>, LedgerError> {
        match self.import.pending(token)? {
            Some(e) => Ok(Some(e)),
            None => Ok(self.extra.lock().unwrap().get(token.as_str()).cloned()),
        }
    }
    fn view_creation(&self, _: &ViewCreationAct) -> Result<Option<ViewCreationFacts>, LedgerError> {
        Ok(None)
    }
}

struct World {
    _tmp: Dir,
    _shm: Dir,
    tmp: PathBuf,
    layout: RootLayout,
    signer: Signer,
    storage: PrikkCli,
    store: MemoryStore,
    ledgers: Arc<Ledgers>,
}

fn world(tag: &str, sandbox: Sandbox) -> World {
    let tmp = dir_in(&std::env::temp_dir(), tag);
    let shm = dir_in(Path::new("/dev/shm"), tag);
    let secret = tmp.0.join("secret");
    std::fs::write(&secret, "an operator secret long enough for the tests").expect("secret");
    let layout = RootLayout::new(tmp.0.join("root"));
    let ledgers = Arc::new(Ledgers::default());
    let mut cfg = SignerConfig::new(
        "prikk",
        sandbox.clone(),
        layout.clone(),
        tmp.0.join("keys"),
        &secret,
        &shm.0,
    );
    cfg.cheap_derivation = true;
    let signer = Signer::open_creating_store(cfg, ledgers.clone()).expect("signer");
    let storage = PrikkCli::open(
        "prikk",
        sandbox,
        Admission::new(1 << 30, 64 << 20, Duration::from_millis(200)),
    )
    .expect("prikk at the floor");
    World {
        tmp: tmp.0.clone(),
        _tmp: tmp,
        _shm: shm,
        layout,
        signer,
        storage,
        store: MemoryStore::new(),
        ledgers,
    }
}

fn create(w: &World, slug: &str) -> CreatedWiki {
    create_wiki(
        &w.store,
        &w.layout,
        &w.signer,
        &w.storage,
        NewWiki::new(
            Slug::new(slug).unwrap(),
            DisplayName::new(&format!("Wiki {slug}")).unwrap(),
        ),
        "0.0.0",
        &KeyAuthority::for_tests(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

const HOME: &str = "---\ntitle: Home\nid: home-1\naliases: [Start]\ntags: [intro, docs]\nlicense: CC0-1.0\n---\n\nWelcome. See [[Guide]], [[Guide|the guide]], [[Missing Page]] and [an outside page](https://example.org/x).\n\n## Parts\n\nSee [[Docs/Deep Page]].\n";
const GUIDE: &str =
    "---\ntitle: Guide\nid: guide-1\nkind: page\nstatus: draft\n---\n\nBack to [[Home]].\n";
const DEEP: &str = "---\ntitle: Docs/Deep Page\nid: deep-1\n---\n\nDeep. [[Home#Parts]]\n";

fn first_folder(root: &Path) {
    put(root, "pages/Home.md", HOME.as_bytes());
    put(root, "pages/Guide.md", GUIDE.as_bytes());
    put(root, "pages/Docs/Deep Page.md", DEEP.as_bytes());
    let logo: Vec<u8> = (0..4000_u32).map(|i| (i * 7 % 251) as u8).collect();
    put(root, "attachments/logo.png", &logo);
    put(
        root,
        "attachments/logo.png.meta.yaml",
        b"license: CC0-1.0\n",
    );
}

fn do_import(
    w: &World,
    wiki: &CreatedWiki,
    folder: &Path,
    replace: bool,
) -> inklkunn_core::import::Imported {
    let view = wiki.record.default_view.clone();
    let have = existing_paths(&w.storage, &wiki.repo, &view).unwrap();
    let plan = plan_folder(folder, &|p| have.contains(p), replace).unwrap();
    assert!(plan.is_importable(), "{}", plan.report());
    run_import(&w.signer, &w.ledgers.import, &wiki.record, &plan).unwrap_or_else(|e| panic!("{e}"))
}

/// An accepted proposal by two accounts that edits the Home page.
fn proposal_edit(w: &World, wiki: &CreatedWiki) -> (String, String) {
    let view = wiki.record.default_view.clone();
    let tip = w
        .storage
        .history(&wiki.repo, &Point::View(view.clone()), Some(1))
        .unwrap();
    let base = tip.blocks.first().unwrap().block_id.clone();
    let proposer = Actor::Account {
        id: AccountId::new(),
        name: DisplayName::new("Aiko Tanaka").unwrap(),
    };
    let acceptor = Actor::Account {
        id: AccountId::new(),
        name: DisplayName::new("Ben (maintainer)").unwrap(),
    };
    let proposal = ProposalId::new();
    let message = Message::proposal(
        Summary::from_text("Say hello properly").unwrap(),
        proposal.clone(),
        proposer.clone(),
        acceptor.clone(),
        BlockId::new(&base).unwrap(),
    )
    .unwrap();
    let edited = format!("{HOME}\nThanks for reading.\n");
    let change = PreparedChange {
        files: vec![PreparedFile {
            path: "pages/Home.md".into(),
            content: Some(edited.clone().into_bytes()),
        }],
        renames: vec![],
        message: message.to_text(),
    };
    let token = AcceptanceToken::new(format!("acceptance-{proposal}"));
    w.ledgers.extra.lock().unwrap().insert(
        token.as_str().to_owned(),
        ExpectedMessage {
            wiki: wiki.record.id.clone(),
            view: view.clone(),
            kind: message.kind(),
            proposal: Some(proposal),
            proposed_by: proposer,
            accepted_by: acceptor,
            base: Some(BlockId::new(&base).unwrap()),
            imported_from: None,
            change_digest: change.digest(),
        },
    );
    w.signer
        .accept(&wiki.record.id, &view, &token, &change)
        .unwrap_or_else(|e| panic!("{e}"));
    (edited, base)
}

/// Every file of a bundle by relative path.
fn files_of(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, root, out);
            } else {
                out.insert(
                    p.strip_prefix(root).unwrap().display().to_string(),
                    std::fs::read(&p).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The `"path": "hash"` lines of a manifest's checksums.
fn manifest_checksums(manifest: &str) -> BTreeMap<String, String> {
    manifest
        .lines()
        .filter_map(|l| l.strip_prefix("    \""))
        .filter_map(|l| {
            let (path, rest) = l.split_once("\": \"")?;
            Some((path.to_owned(), rest.strip_suffix('"')?.to_owned()))
        })
        .collect()
}

fn get<'a>(files: &'a BTreeMap<String, Vec<u8>>, path: &str) -> &'a Vec<u8> {
    files
        .get(path)
        .unwrap_or_else(|| panic!("the bundle has no `{path}`"))
}

fn at<'a>(v: &'a serde_json::Value, pointer: &str) -> &'a serde_json::Value {
    v.pointer(pointer).unwrap_or(&serde_json::Value::Null)
}

fn json_lines(bytes: &[u8]) -> Vec<serde_json::Value> {
    String::from_utf8(bytes.to_vec())
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn export_import_export_agrees_on_text_links_metadata_and_history() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    for (n, sandbox) in policies().into_iter().enumerate() {
        // Wiki A: created, filled by two imports and edited by an accepted proposal.
        let w = world(&format!("rt-{n}"), sandbox);
        let a = create(&w, "alpha");
        let folder = dir_in(&w.tmp, "folder1");
        first_folder(&folder.0);
        let first = do_import(&w, &a, &folder.0, false);
        assert_eq!(first.files, 5);
        let second = dir_in(&w.tmp, "folder2");
        put(
            &second.0,
            "pages/Guide.md",
            format!("{GUIDE}\nMore.\n").as_bytes(),
        );
        do_import(&w, &a, &second.0, true);
        let (edited_home, _) = proposal_edit(&w, &a);
        assert!(w.storage.verify(&a.repo).unwrap().verdict.ok);

        // The export.
        let out1 = dir_in(&w.tmp, "out1");
        let dir1 = out1.0.join("bundle");
        let done = export_bundle(
            &w.storage,
            &w.layout,
            &a.record,
            &dir1,
            "0.0.0",
            1_700_000_000,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(done.pages, 4, "Help, Home, Guide, Deep Page");
        assert_eq!(done.attachments, 1);
        let one = files_of(&dir1);
        // Files as stored.
        assert_eq!(
            get(&one, "pages/Home.md"),
            &edited_home.clone().into_bytes()
        );
        assert_eq!(
            get(&one, "pages/Docs/Deep Page.md").as_slice(),
            DEEP.as_bytes()
        );
        assert_eq!(
            get(&one, "attachments/logo.png.meta.yaml").as_slice(),
            b"license: CC0-1.0\n"
        );
        assert_eq!(get(&one, "attachments/logo.png").len(), 4000);
        // The manifest: fidelity, counts, block, interwiki, and a checksum for every other file.
        let manifest = String::from_utf8(get(&one, "wiki-bundle.yaml").clone()).unwrap();
        for needle in [
            "  level: 3",
            "  pages: 4",
            "  attachments: 1",
            "interwiki: {}",
            "  algorithm: sha256",
        ] {
            assert!(manifest.contains(needle), "{needle}\n{manifest}");
        }
        assert!(manifest.contains(&format!("as_of_block: \"{}\"", done.tip)));
        let sums = manifest_checksums(&manifest);
        let others: Vec<&String> = one.keys().filter(|k| *k != "wiki-bundle.yaml").collect();
        assert_eq!(sums.keys().collect::<Vec<_>>(), others);
        for (path, hash) in &sums {
            assert_eq!(&sha(get(&one, path)), hash, "{path}");
        }
        // Private on disk: the bundle may hold private pages.
        for path in one.keys() {
            let mode = std::fs::metadata(dir1.join(path))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0, "{path} is readable by others");
        }
        // History: one file per page id, authors from the messages' people.
        let home_history = json_lines(get(&one, "history/home-1.jsonl"));
        assert_eq!(home_history.len(), 2, "{home_history:#?}");
        let (first, second) = (home_history.first().unwrap(), home_history.get(1).unwrap());
        assert_eq!(at(first, "/kind"), "import");
        assert_eq!(at(first, "/authors/0/type"), "system");
        assert!(
            at(first, "/imported_from")
                .as_str()
                .unwrap()
                .starts_with("folder sha256:")
        );
        assert_eq!(at(second, "/kind"), "proposal");
        assert_eq!(at(second, "/authors/0/name"), "Aiko Tanaka");
        assert_eq!(at(second, "/authors/1/name"), "Ben (maintainer)");
        assert_eq!(at(second, "/authors/0/role"), "proposer");
        assert_eq!(json_lines(get(&one, "history/guide-1.jsonl")).len(), 2);
        assert_eq!(json_lines(get(&one, "history/deep-1.jsonl")).len(), 1);
        assert_eq!(done.histories, 4);
        let users = String::from_utf8(get(&one, "users.yaml").clone()).unwrap();
        assert!(
            users.contains("Aiko Tanaka") && users.contains("Ben (maintainer)"),
            "{users}"
        );
        assert_eq!(done.users, 2);
        // The destination is never overwritten, and a failed export leaves nothing beside it.
        let again = export_bundle(&w.storage, &w.layout, &a.record, &dir1, "0.0.0", 1).unwrap_err();
        assert!(matches!(again, ExportError::Destination(_)), "{again}");
        let beside: Vec<_> = std::fs::read_dir(&out1.0).unwrap().collect();
        assert_eq!(beside.len(), 1, "{beside:?}");

        // Wiki B: fresh, filled from A's bundle (its own Help page is replaced by A's).
        let b = create(&w, "beta");
        let have = existing_paths(&w.storage, &b.repo, &b.record.default_view).unwrap();
        let plan = plan_folder(&dir1, &|p| have.contains(p), true).unwrap();
        assert!(plan.is_importable(), "{}", plan.report());
        let skipped: Vec<&str> = plan.not_imported.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            skipped,
            vec![
                ".prikkignore",
                "governance",
                "history",
                "knowledge",
                "users.yaml",
                "wiki-bundle.yaml"
            ]
        );
        // Without --replace-existing the Help page that both wikis have is refused.
        let refused = plan_folder(&dir1, &|p| have.contains(p), false).unwrap();
        assert!(
            !refused.is_importable() && refused.refused.iter().any(|r| r.name == "pages/Help.md")
        );
        run_import(&w.signer, &w.ledgers.import, &b.record, &plan)
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(w.storage.verify(&b.repo).unwrap().verdict.ok);
        let out2 = dir_in(&w.tmp, "out2");
        let dir2 = out2.0.join("bundle");
        export_bundle(
            &w.storage,
            &w.layout,
            &b.record,
            &dir2,
            "0.0.0",
            1_700_000_001,
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let two = files_of(&dir2);

        // Text, and the bytes of every stored file: the same.
        let content = |m: &BTreeMap<String, Vec<u8>>| -> BTreeMap<String, Vec<u8>> {
            m.iter()
                .filter(|(k, _)| {
                    !k.starts_with("history/") && *k != "users.yaml" && *k != "wiki-bundle.yaml"
                })
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };
        assert_eq!(
            content(&one),
            content(&two),
            "the round trip changed a stored file"
        );
        // The manifests' checksums agree on every stored file.
        let sums2 =
            manifest_checksums(&String::from_utf8(get(&two, "wiki-bundle.yaml").clone()).unwrap());
        for (path, hash) in &sums {
            if !path.starts_with("history/") && path != "users.yaml" {
                assert_eq!(sums2.get(path), Some(hash), "{path}");
            }
        }
        // Links and metadata, read the way the wiki reads them.
        for (path, bytes) in content(&one)
            .iter()
            .filter(|(k, _)| k.starts_with("pages/"))
        {
            let a_text = std::str::from_utf8(bytes).unwrap();
            let b_text = std::str::from_utf8(get(&two, path)).unwrap();
            assert_eq!(
                format!("{:?}", inklkunn_markup::scan(a_text).links),
                format!("{:?}", inklkunn_markup::scan(b_text).links),
                "links of {path}"
            );
            assert_eq!(
                inklkunn_markup::parse_metadata(a_text).unwrap(),
                inklkunn_markup::parse_metadata(b_text).unwrap(),
                "metadata of {path}"
            );
        }
        // History: the same pages have histories (the page ids travel in the files); each is readable, names a
        // block, a path and its authors; and in B the pages' latest change is the import that brought them.
        let names = |m: &BTreeMap<String, Vec<u8>>| -> Vec<String> {
            m.keys()
                .filter(|k| k.starts_with("history/"))
                .cloned()
                .collect()
        };
        assert_eq!(names(&one), names(&two));
        for name in names(&two) {
            let lines = json_lines(get(&two, &name));
            assert!(!lines.is_empty(), "{name}");
            for l in &lines {
                for key in ["block", "path", "operation", "kind", "authors", "summary"] {
                    assert!(l.get(key).is_some(), "{name}: no `{key}` in {l}");
                }
            }
            let last = lines.last().unwrap();
            assert_eq!(at(last, "/kind"), "import", "{name}");
            assert_eq!(at(last, "/authors/0/type"), "system");
            // A's history of the same page ends at the same text.
            assert!(!json_lines(get(&one, &name)).is_empty());
        }
        assert!(w.storage.verify(&b.repo).unwrap().verdict.ok);
    }
}

/// A storage whose `extract` refuses for want of prikk's checkpoint record, as after a prikk upgrade.
struct NoCheckpoint<'a>(&'a PrikkCli);

impl Storage for NoCheckpoint<'_> {
    fn views(&self, r: &Path) -> inklkunn_prikk::Result<Views> {
        self.0.views(r)
    }
    fn tree(&self, r: &Path, p: &Point, x: Option<&str>) -> inklkunn_prikk::Result<Tree> {
        self.0.tree(r, p, x)
    }
    fn stat(&self, r: &Path, p: &Point, x: &str) -> inklkunn_prikk::Result<FileMeta> {
        self.0.stat(r, p, x)
    }
    fn read(&self, r: &Path, p: &Point, x: &str, m: u64) -> inklkunn_prikk::Result<Vec<u8>> {
        self.0.read(r, p, x, m)
    }
    fn history(&self, r: &Path, p: &Point, l: Option<u64>) -> inklkunn_prikk::Result<History> {
        self.0.history(r, p, l)
    }
    fn change(&self, r: &Path, i: &str) -> inklkunn_prikk::Result<Change> {
        self.0.change(r, i)
    }
    fn diff(&self, r: &Path, a: &Point, b: &Point, p: &[&str]) -> inklkunn_prikk::Result<Diff> {
        self.0.diff(r, a, b, p)
    }
    fn plan(&self, r: &Path, p: &Point) -> inklkunn_prikk::Result<Materialized> {
        self.0.plan(r, p)
    }
    fn materialize(
        &self,
        _: &Path,
        _: &Point,
        _: &Path,
        _: &str,
        _: ExtractBounds,
    ) -> inklkunn_prikk::Result<Materialized> {
        Err(StorageError::Refused {
            verb: "extract",
            message: "precondition not met: no admitted snapshot within 63 blocks of this point"
                .into(),
            kind: Some("no-checkpoint-in-reach".into()),
            reason: Some("not-recorded".into()),
        })
    }
    fn verify(&self, r: &Path) -> inklkunn_prikk::Result<Verification> {
        self.0.verify(r)
    }
}

#[test]
fn an_export_without_prikks_checkpoint_says_so_and_leaves_nothing() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let w = world("nockpt", Sandbox::Unconfined);
    let a = create(&w, "gamma");
    let out = dir_in(&w.tmp, "out");
    let dest = out.0.join("bundle");
    let err = export_bundle(
        &NoCheckpoint(&w.storage),
        &w.layout,
        &a.record,
        &dest,
        "0.0.0",
        1,
    )
    .unwrap_err();
    assert!(matches!(err, ExportError::CheckpointNotRecorded), "{err}");
    assert!(!dest.exists());
    assert_eq!(
        std::fs::read_dir(&out.0).unwrap().count(),
        0,
        "a partial bundle was left"
    );
    assert_eq!(
        std::fs::read_dir(w.layout.scratch_dir(&a.record.id))
            .unwrap()
            .count(),
        0,
        "an extraction was left in the scratch directory"
    );
}

/// Every file of the wiki's root, with its size: what a dry run must leave exactly as it found it.
fn root_snapshot(root: &Path) -> BTreeMap<String, u64> {
    fn walk(dir: &Path, root: &Path, out: &mut BTreeMap<String, u64>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            let rel = p.strip_prefix(root).unwrap().display().to_string();
            let meta = std::fs::symlink_metadata(&p).unwrap();
            if meta.is_dir() {
                out.insert(format!("{rel}/"), 0);
                walk(&p, root, out);
            } else {
                out.insert(rel, meta.len());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn a_dry_run_writes_nothing_to_the_wiki_the_folder_or_anywhere() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let w = world("dry", Sandbox::Unconfined);
    let a = create(&w, "delta");
    let folder = dir_in(&w.tmp, "folder");
    first_folder(&folder.0);
    let wiki_before = root_snapshot(w.layout.root());
    let folder_before = root_snapshot(&folder.0);
    let log_before = w
        .storage
        .history(&a.repo, &Point::View(a.record.default_view.clone()), None)
        .unwrap();
    let have = existing_paths(&w.storage, &a.repo, &a.record.default_view).unwrap();
    let plan = plan_folder(&folder.0, &|p| have.contains(p), false).unwrap();
    assert!(plan.is_importable());
    let report = plan.report();
    assert!(report.contains("5 file(s) to write"), "{report}");
    assert_eq!(
        root_snapshot(w.layout.root()),
        wiki_before,
        "a dry run changed the wiki's files"
    );
    assert_eq!(
        root_snapshot(&folder.0),
        folder_before,
        "a dry run changed the folder"
    );
    let log_after = w
        .storage
        .history(&a.repo, &Point::View(a.record.default_view.clone()), None)
        .unwrap();
    assert_eq!(log_before, log_after, "a dry run changed the history");
    // The same plan, run, is the one change the report promised.
    let done = run_import(&w.signer, &w.ledgers.import, &a.record, &plan).unwrap();
    let log_run = w
        .storage
        .history(&a.repo, &Point::View(a.record.default_view.clone()), None)
        .unwrap();
    assert_eq!(log_run.blocks.len(), log_before.blocks.len() + 1);
    assert_eq!(
        log_run.blocks.first().unwrap().block_id,
        done.sealed.block_id
    );
}
