#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::ffi::OsStrExt;

use std::cell::RefCell;

use std::sync::{Arc, Mutex};

use inklkunn_signer::accept::{
    Accept, AcceptError, AcceptanceLedger, AcceptanceToken, ExpectedMessage, ViewCreationAct,
};
use inklkunn_signer::message::{Actor, Kind, Message};

use super::*;
use crate::acceptance::StoreLedger;
use crate::acceptance::testing::Rig;
use inklkunn_store::{AcceptanceStore as _, ActStore as _};

/// Where the walk is when a test's hook runs: a directory is open (its handle held, not yet listed) or has just
/// been listed (its entries known, none opened yet).
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Stage {
    Opened,
    Listed,
}

type Hook = Box<dyn FnMut(&Path, Stage)>;

thread_local! {
    static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
}

/// Called by the walk at each directory (only in a test build); runs the hook this thread's test installed.
pub(super) fn at_directory(path: &Path, stage: Stage) {
    HOOK.with(|h| {
        if let Some(hook) = h.borrow_mut().as_mut() {
            hook(path, stage);
        }
    });
}

/// Run `f` with `hook` installed for the walks of this thread.
fn with_hook<T>(hook: impl FnMut(&Path, Stage) + 'static, f: impl FnOnce() -> T) -> T {
    struct Remove;
    impl Drop for Remove {
        fn drop(&mut self) {
            HOOK.with(|h| *h.borrow_mut() = None);
        }
    }
    HOOK.with(|h| *h.borrow_mut() = Some(Box::new(hook)));
    let _remove = Remove;
    f()
}

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir(tag: &str) -> Dir {
    let p = std::env::temp_dir().join(format!("inklkunn-import-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    Dir(p)
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

const GOOD: &str = "---\ntitle: Good\nid: g-1\n---\n\nHello [[Other]].\n";

/// A tip no test cares about: 64 lower-case hex characters.
const TIP: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";

/// A wiki with no file.
fn nothing() -> Existing {
    Existing::at(TIP)
}

#[test]
fn a_clean_folder_is_planned_in_path_order_with_pages_and_attachments_counted() {
    let d = dir("clean");
    put(&d.0, "pages/B.md", GOOD.replace("g-1", "g-2").as_bytes());
    put(&d.0, "pages/A.md", GOOD.as_bytes());
    put(&d.0, "pages/Deep/Er.md", b"no frontmatter at all\n");
    put(&d.0, "attachments/pic.bin", &[0, 1, 2, 255]);
    put(&d.0, "attachments/pic.bin.meta.yaml", b"license: CC0-1.0\n");
    // Not part of what an import reads, and said so.
    put(&d.0, "wiki-bundle.yaml", b"format: portable-wiki-bundle\n");
    put(&d.0, "users.yaml", b"users: []\n");
    put(&d.0, "history/x.jsonl", b"{}\n");
    put(&d.0, "notes.txt", b"mine");
    let plan = plan_folder(&d.0, &nothing(), false).unwrap();
    assert!(plan.is_importable(), "{}", plan.report());
    let paths: Vec<&str> = plan.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "attachments/pic.bin",
            "attachments/pic.bin.meta.yaml",
            "pages/A.md",
            "pages/B.md",
            "pages/Deep/Er.md"
        ]
    );
    assert_eq!(plan.counts(), (3, 2));
    let skipped: Vec<&str> = plan.not_imported.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        skipped,
        vec!["history", "notes.txt", "users.yaml", "wiki-bundle.yaml"]
    );
    let report = plan.report();
    assert!(report.contains("5 file(s) to write (3 page(s), 2 attachment(s)"));
    assert!(report.contains("add     pages/A.md"));
    assert!(report.contains("not imported:") && !report.contains("refused"));
}

/// The hostile folder: every case a bundle from outside may carry (ENF-4).
fn hostile(root: &Path) {
    put(root, "pages/Good.md", GOOD.as_bytes());
    // Traversal by link: a link out of the folder, to a file and to a directory, and one to a sibling.
    std::os::unix::fs::symlink("/etc/passwd", root.join("pages/Evil.md")).unwrap();
    std::fs::create_dir_all(root.join("attachments")).unwrap();
    std::os::unix::fs::symlink("/", root.join("attachments/rootdir")).unwrap();
    std::os::unix::fs::symlink("../pages/Good.md", root.join("attachments/loop")).unwrap();
    // Names a repository refuses.
    put(root, "pages/CON.md", GOOD.replace("g-1", "g-3").as_bytes());
    put(root, "pages/.hidden.md", b"x");
    put(root, "pages/back\\slash.md", b"x");
    put(root, "pages/colon:name.md", b"x");
    put(root, "pages/trailing .md", b"x"); // "trailing " before the extension is fine…
    put(root, "pages/dot.", b"x"); // …a trailing dot and a missing extension are not
    put(root, "pages/readme.txt", b"not a page");
    put(root, "attachments/NUL", b"x");
    put(root, "pages/\u{65e5}\u{672c}\u{8a9e}.md", b"x");
    // A name that is not UTF-8.
    let bad = std::ffi::OsStr::from_bytes(b"pages/bad\xffname.md");
    std::fs::write(root.join(bad), b"x").unwrap();
    // A path over the 1024-byte cap (each component well under 255).
    let deep = (0..12)
        .map(|i| format!("{i}{}", "d".repeat(95)))
        .collect::<Vec<_>>()
        .join("/");
    put(root, &format!("attachments/{deep}/file.bin"), b"x");
    // Content that is refused.
    put(root, "pages/Latin1.md", b"---\ntitle: Caf\xe9\n---\n");
    put(
        root,
        "pages/BadFront.md",
        b"---\ntitle: &anchor Bad\n---\n\nbody\n",
    );
    put(root, "pages/ZDupId.md", GOOD.as_bytes());
    put(root, "pages/Big.md", &vec![b'a'; (1 << 20) + 1]);
    // Two paths one case-insensitive file system cannot tell apart.
    put(root, "pages/Same.md", b"one\n");
    put(root, "pages/SAME.md", b"two\n");
    // A socket where a file should be.
    let _sock = std::os::unix::net::UnixListener::bind(root.join("attachments/pipe.sock")).unwrap();
    std::mem::forget(_sock);
}

#[test]
fn a_hostile_folder_is_refused_with_every_reason_named_and_the_good_page_does_not_go_alone() {
    let d = dir("hostile");
    hostile(&d.0);
    let plan = plan_folder(&d.0, &nothing(), false).unwrap();
    assert!(!plan.is_importable());
    let reasons: HashMap<String, String> = plan
        .refused
        .iter()
        .map(|r| (r.name.clone(), r.reason.clone()))
        .collect();
    let expect = |name: &str, needle: &str| {
        let got = reasons
            .get(name)
            .unwrap_or_else(|| panic!("`{name}` was not refused: {reasons:#?}"));
        assert!(got.contains(needle), "`{name}`: {got}");
    };
    expect("pages/Evil.md", "symbolic link");
    expect("attachments/rootdir", "symbolic link");
    expect("attachments/loop", "symbolic link");
    expect("pages/CON.md", "Windows reserves");
    expect("pages/.hidden.md", "does not start with a dot");
    expect("pages/back\\\\slash.md", "backslash or colon");
    expect("pages/colon:name.md", "backslash or colon");
    expect("pages/dot.", "does not end in a space or a dot");
    expect("pages/readme.txt", "`.md` file");
    expect("attachments/NUL", "Windows reserves");
    expect("pages/\u{65e5}\u{672c}\u{8a9e}.md", "ASCII");
    expect("pages/bad\u{fffd}name.md", "not UTF-8");
    expect("pages/Latin1.md", "not UTF-8 text");
    expect("pages/BadFront.md", "frontmatter the parser refuses");
    expect("pages/ZDupId.md", "page id of `pages/Good.md`");
    expect("pages/Big.md", "cap of a page");
    expect("pages/Same.md", "case-insensitive");
    expect("attachments/pipe.sock", "not a regular file");
    let long = reasons
        .iter()
        .find(|(n, _)| n.starts_with("attachments/0d"))
        .map(|(_, r)| r.clone())
        .expect("the over-cap path was refused");
    assert!(long.contains("over the 1024-byte cap"), "{long}");
    // The one good page is not imported alone: all or nothing.
    assert!(plan.files.iter().any(|f| f.path == "pages/Good.md"));
    let err = ImportError::Refused(plan.refused.len());
    assert!(err.to_string().contains("none of the folder was imported"));
    // The report names every reason and never repeats a byte of any file's content.
    let report = plan.report();
    for r in &plan.refused {
        assert!(
            report.contains(&r.reason),
            "{} missing from the report",
            r.name
        );
    }
    assert!(!report.contains("Hello"));
    assert!(!report.contains("root:x:"));
    assert!(report.contains("all or nothing"));
}

#[test]
fn a_component_over_255_bytes_and_a_path_over_1024_are_refused_by_prikks_caps_with_the_numbers() {
    // A file system will not hold a 256-byte name, so the rule is exercised on the walker directly.
    let have = nothing();
    let mut walk = Walk {
        existing: &have,
        replace_existing: false,
        page_cap: 1 << 20,
        plan: ImportPlan::default(),
        keys: HashMap::new(),
        ids: HashMap::new(),
        pages: Vec::new(),
    };
    let long_name = format!("attachments/{}.bin", "x".repeat(256));
    // Both are refused before anything is opened, so any directory handle will do.
    let nowhere = open_root(Path::new("/")).unwrap();
    walk.file(
        &nowhere,
        OsStr::new("x"),
        Path::new("/nonexistent"),
        &long_name,
    );
    let long_path = format!("attachments/{}/f.bin", vec!["d".repeat(200); 6].join("/"));
    assert!(long_path.len() > 1024);
    walk.file(
        &nowhere,
        OsStr::new("x"),
        Path::new("/nonexistent"),
        &long_path,
    );
    let reasons: Vec<&str> = walk
        .plan
        .refused
        .iter()
        .map(|r| r.reason.as_str())
        .collect();
    assert!(
        reasons
            .first()
            .is_some_and(|r| r.contains("260 bytes") && r.contains("255-byte cap")),
        "{reasons:?}"
    );
    assert!(
        reasons
            .get(1)
            .is_some_and(|r| r.contains("over the 1024-byte cap")),
        "{reasons:?}"
    );
}

#[test]
fn a_path_the_wiki_has_is_refused_unless_replacing_is_asked_for() {
    let d = dir("exists");
    put(&d.0, "pages/Help.md", GOOD.as_bytes());
    put(&d.0, "pages/New.md", GOOD.replace("g-1", "g-9").as_bytes());
    let has = nothing().with_file("pages/Help.md", b"an older Help\n");
    let refused = plan_folder(&d.0, &has, false).unwrap();
    assert!(!refused.is_importable());
    assert_eq!(refused.refused.len(), 1);
    assert!(
        refused
            .refused
            .first()
            .is_some_and(|r| r.reason.contains("--replace-existing"))
    );
    let replaced = plan_folder(&d.0, &has, true).unwrap();
    assert!(replaced.unchanged.is_empty());
    assert!(replaced.is_importable());
    let actions: Vec<(&str, Action)> = replaced
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.action))
        .collect();
    assert_eq!(
        actions,
        vec![
            ("pages/Help.md", Action::Replace),
            ("pages/New.md", Action::Add)
        ]
    );
    assert!(replaced.report().contains("replace pages/Help.md"));
}

#[test]
fn nothing_to_import_and_not_a_directory_are_said_plainly() {
    let d = dir("nothing");
    std::fs::create_dir_all(d.0.join("pages")).unwrap();
    put(&d.0, "users.yaml", b"users: []\n");
    assert!(matches!(
        plan_folder(&d.0, &nothing(), false),
        Err(ImportError::Nothing)
    ));
    put(&d.0, "afile", b"x");
    let err = plan_folder(&d.0.join("afile"), &nothing(), false).unwrap_err();
    assert!(err.to_string().contains("not a directory"), "{err}");
    let err = plan_folder(&d.0.join("missing"), &nothing(), false).unwrap_err();
    assert!(matches!(err, ImportError::Folder(_)), "{err}");
    // A top-level `pages` that is a link is refused, not followed.
    let e = dir("toplink");
    std::os::unix::fs::symlink(&d.0, e.0.join("pages")).unwrap();
    let plan = plan_folder(&e.0, &nothing(), false).unwrap();
    assert!(!plan.is_importable());
    assert!(
        plan.refused
            .first()
            .is_some_and(|r| r.reason.contains("symbolic link"))
    );
}

#[test]
fn a_planning_and_a_dry_run_write_nothing_and_a_file_swapped_for_a_link_is_not_followed() {
    let d = dir("readonly");
    put(&d.0, "pages/A.md", GOOD.as_bytes());
    put(&d.0, "attachments/a.bin", b"abc");
    let before = snapshot(&d.0);
    let plan = plan_folder(&d.0, &nothing(), false).unwrap();
    let _ = plan.report();
    assert_eq!(snapshot(&d.0), before, "planning changed the folder");
    // Between the plan and the read, an attachment becomes a link to something else.
    let secret = dir("secret");
    put(&secret.0, "s.txt", b"abc");
    std::fs::remove_file(d.0.join("attachments/a.bin")).unwrap();
    std::os::unix::fs::symlink(secret.0.join("s.txt"), d.0.join("attachments/a.bin")).unwrap();
    let err = read_files(&plan).unwrap_err();
    assert!(matches!(err, ImportError::Changed(_)), "{err}");
    assert!(err.to_string().contains("is a symbolic link now"), "{err}");
}

fn snapshot(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        entries.sort();
        for p in entries {
            let rel = p.strip_prefix(root).unwrap().display().to_string();
            if p.is_dir() {
                out.push((rel, Vec::new()));
                walk(&p, root, out);
            } else {
                out.push((rel, std::fs::read(&p).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out
}

/// A signer stand-in that records what it was asked and accepts.
struct Recorder {
    seen: Mutex<Option<PreparedChange>>,
    ledger: Arc<StoreLedger>,
    pending_during: Mutex<Option<ExpectedMessage>>,
}

impl Accept for Recorder {
    fn accept(
        &self,
        _wiki: &inklkunn_store::WikiId,
        _view: &str,
        token: &AcceptanceToken,
        change: &PreparedChange,
    ) -> Result<SealedChange, AcceptError> {
        *self.pending_during.lock().unwrap() = self.ledger.pending(token).unwrap();
        *self.seen.lock().unwrap() = Some(change.clone());
        Ok(SealedChange {
            block_id: "b".repeat(64),
            patch_ids: vec![],
        })
    }
    fn create_view(
        &self,
        _: &inklkunn_store::WikiId,
        _: &str,
        _: &str,
        _: &ViewCreationAct,
    ) -> Result<(), AcceptError> {
        Err(AcceptError::NotPending)
    }
}

/// A wiki in a memory store with the lease, the service's own path over `Recorder`, and the recorder.
fn world(tag: &str) -> (Rig, Arc<Recorder>, SystemActs) {
    let rig = Rig::new(tag);
    let accept = Arc::new(Recorder {
        seen: Mutex::new(None),
        ledger: rig.ledger.clone(),
        pending_during: Mutex::new(None),
    });
    let acts = rig.system_acts(accept.clone());
    (rig, accept, acts)
}

#[test]
fn the_run_binds_the_seal_to_the_planned_files_under_the_system_principal_and_forgets_the_record() {
    let d = dir("run");
    put(&d.0, "pages/A.md", GOOD.as_bytes());
    put(&d.0, "attachments/a.bin", b"abc");
    let plan = plan_folder(&d.0, &nothing(), false).unwrap();
    let (rig, accept, acts) = world("import");
    let wiki = rig.wiki.clone();
    let done = run_import(&acts, &wiki, &plan).unwrap();
    assert_eq!(done.files, 2);
    let change = accept.seen.lock().unwrap().clone().unwrap();
    let message = Message::parse(&change.message).unwrap();
    assert_eq!(message.kind(), Kind::Import);
    assert!(matches!(message.proposed_by(), Actor::System(_)));
    assert!(matches!(message.accepted_by(), Actor::System(_)));
    assert!(
        message
            .imported_from()
            .unwrap()
            .as_str()
            .contains(&done.digest.to_hex())
    );
    assert_eq!(change.digest(), done.digest);
    assert_eq!(change.files.len(), 2);
    // The record the signer reads was there for the call, said exactly this, and is gone after.
    let during = accept.pending_during.lock().unwrap().clone().unwrap();
    assert!(during.matches(&message));
    assert_eq!(during.change_digest, done.digest);
    assert_eq!(during.view, wiki.default_view);
    // The act is recorded, ended and sealed; the signer's guard no longer finds it pending.
    let recorded = rig.store.system_acts(&wiki.id).unwrap();
    assert_eq!(recorded.len(), 1);
    let act = recorded.first().unwrap();
    assert_eq!(act.act.id, done.act);
    assert_eq!(act.act.kind, SystemActKind::Import);
    assert_eq!(act.act.digest, done.digest.to_hex());
    assert!(matches!(
        act.end.as_ref().map(|e| &e.outcome),
        Some(inklkunn_store::AcceptanceOutcome::Sealed { block_id, .. }) if *block_id == done.sealed.block_id
    ));
    assert!(
        rig.ledger
            .pending(&AcceptanceToken::new(done.act.as_str()))
            .unwrap()
            .is_none()
    );
    // The lease was held for the act and is free again.
    assert!(rig.store.lease_holder(&wiki.id).unwrap().is_none());
    // A plan with a refusal never reaches the signer.
    put(&d.0, "pages/CON.md", b"x");
    let bad = plan_folder(&d.0, &nothing(), false).unwrap();
    *accept.seen.lock().unwrap() = None;
    let err = run_import(&acts, &wiki, &bad).unwrap_err();
    assert!(matches!(err, ImportError::Refused(1)), "{err}");
    assert!(accept.seen.lock().unwrap().is_none());
}

// ---- R4: the folder is not trusted between the plan and the run ----

/// Run `f` on another thread; a call that has not answered in `secs` fails the test instead of hanging the suite.
fn within<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_secs(secs))
        .expect("the call did not answer in time: it waits on something it must not wait on")
}

fn mkfifo(path: &Path) {
    inklkunn_signer::make_fifo_for_tests(path).expect("a named pipe is made");
}

fn changed(err: ImportError) -> String {
    match err {
        ImportError::Changed(why) => why,
        other => panic!("expected `Changed`, got {other:?}"),
    }
}

#[test]
fn a_directory_swapped_for_a_link_between_the_plan_and_the_run_is_not_followed() {
    let d = dir("swapdir");
    put(&d.0, "pages/Sub/A.md", GOOD.as_bytes());
    put(&d.0, "attachments/x/y.bin", b"abc");
    let plan = plan_folder(&d.0, &nothing(), false).unwrap();
    assert!(plan.is_importable(), "{}", plan.report());
    assert_eq!(read_files(&plan).unwrap().len(), 2);

    // An intermediate directory becomes a link to a twin: other inodes, the same names and bytes. The old
    // check re-read the identity through the same link and found the twin as consistent as the original.
    let twin = dir("swapdir-twin");
    put(&twin.0, "A.md", GOOD.as_bytes());
    std::fs::rename(d.0.join("pages/Sub"), d.0.join("pages/Sub.moved")).unwrap();
    std::os::unix::fs::symlink(&twin.0, d.0.join("pages/Sub")).unwrap();
    let why = changed(read_files(&plan).unwrap_err());
    assert!(
        why.contains("`pages/Sub/A.md`")
            && why.contains("`pages/Sub`, which is no longer the directory that was planned"),
        "{why}"
    );

    // …or to the very directory it was, moved: the same inodes, but reached through a link.
    std::fs::remove_file(d.0.join("pages/Sub")).unwrap();
    std::os::unix::fs::symlink(d.0.join("pages/Sub.moved"), d.0.join("pages/Sub")).unwrap();
    let why = changed(read_files(&plan).unwrap_err());
    assert!(
        why.contains("no longer the directory that was planned"),
        "{why}"
    );

    // A deeper one, and a top-level one.
    std::fs::remove_file(d.0.join("pages/Sub")).unwrap();
    std::fs::rename(d.0.join("pages/Sub.moved"), d.0.join("pages/Sub")).unwrap();
    assert_eq!(
        read_files(&plan).unwrap().len(),
        2,
        "restored, it reads again"
    );
    std::fs::rename(d.0.join("attachments"), d.0.join("attachments.moved")).unwrap();
    std::os::unix::fs::symlink(d.0.join("attachments.moved"), d.0.join("attachments")).unwrap();
    let why = changed(read_files(&plan).unwrap_err());
    assert!(
        why.contains("`attachments/x/y.bin`") && why.contains("`attachments`"),
        "{why}"
    );

    // The folder itself, replaced by another directory at the same place.
    std::fs::remove_file(d.0.join("attachments")).unwrap();
    std::fs::rename(d.0.join("attachments.moved"), d.0.join("attachments")).unwrap();
    let moved = d.0.with_extension("away");
    std::fs::rename(&d.0, &moved).unwrap();
    put(&d.0, "pages/Sub/A.md", GOOD.as_bytes());
    put(&d.0, "attachments/x/y.bin", b"abc");
    let why = changed(read_files(&plan).unwrap_err());
    let _ = std::fs::remove_dir_all(&moved);
    assert!(
        why.contains("no longer the directory that was planned"),
        "{why}"
    );
}

#[test]
fn a_file_replaced_by_another_with_the_same_bytes_is_not_the_file_that_was_planned() {
    let d = dir("replaced");
    put(&d.0, "attachments/a.bin", b"abc");
    let plan = plan_folder(&d.0, &nothing(), false).unwrap();
    // A new inode renamed over the old one: same path, same size, same bytes.
    put(&d.0, "attachments/a.tmp", b"abc");
    std::fs::rename(d.0.join("attachments/a.tmp"), d.0.join("attachments/a.bin")).unwrap();
    let why = changed(read_files(&plan).unwrap_err());
    assert!(why.contains("is not the file that was listed"), "{why}");
}

#[test]
fn a_pipe_swapped_in_for_a_file_between_the_plan_and_the_run_is_refused_not_waited_for() {
    let d = dir("fifo");
    put(&d.0, "pages/A.md", GOOD.as_bytes());
    put(&d.0, "attachments/a.bin", b"abc");
    let plan = plan_folder(&d.0, &nothing(), false).unwrap();
    std::fs::remove_file(d.0.join("attachments/a.bin")).unwrap();
    mkfifo(&d.0.join("attachments/a.bin"));
    let why = changed(within(10, move || read_files(&plan)).unwrap_err());
    assert!(why.contains("no longer a regular file"), "{why}");

    // The same, past the check that precedes the open (the race): the open itself must not wait for a writer.
    let attachments = open_root(&d.0.join("attachments")).unwrap();
    let why = within(10, move || {
        read_regular(
            &attachments,
            OsStr::new("a.bin"),
            Identity { dev: 0, ino: 0 },
            1024,
        )
    })
    .unwrap_err();
    assert!(why.contains("no longer a regular file"), "{why}");

    // A pipe is also refused when it is found at the plan, without being opened.
    let plan = plan_folder(&d.0, &nothing(), false).unwrap();
    assert!(
        plan.refused
            .iter()
            .any(|r| r.name == "attachments/a.bin" && r.reason.contains("not a regular file")),
        "{}",
        plan.report()
    );
}

#[test]
fn open_refuses_a_link_and_does_not_wait_for_a_pipe() {
    // The open flags are rustix's; this is what says they mean what the import relies on.
    let d = dir("flags");
    put(&d.0, "f", b"abc");
    std::os::unix::fs::symlink(d.0.join("f"), d.0.join("l")).unwrap();
    std::fs::create_dir(d.0.join("sub")).unwrap();
    std::os::unix::fs::symlink(d.0.join("sub"), d.0.join("sublink")).unwrap();
    let root = open_root(&d.0).unwrap();
    let id = Identity::of(&rustix::fs::statat(&root, "f", AtFlags::SYMLINK_NOFOLLOW).unwrap());
    assert!(open_checked(&root, OsStr::new("f"), id).is_ok());
    let why = open_checked(&root, OsStr::new("l"), id).unwrap_err();
    assert!(why.contains("symbolic link"), "{why}");
    assert!(open_dir_at(&root, OsStr::new("sub")).is_ok());
    let err = open_dir_at(&root, OsStr::new("sublink")).unwrap_err();
    assert!(is_not_a_directory(err), "{err}");
    mkfifo(&d.0.join("p"));
    let why = within(10, move || open_checked(&root, OsStr::new("p"), id)).unwrap_err();
    assert!(why.contains("no longer a regular file"), "{why}");
}

// ---- S1: the walk is anchored at one handle ----

/// A page the folder does not have, in the twin a test swaps in.
const EVIL: &str = "---\ntitle: Evil\nid: e-1\n---\n\nNot in the folder.\n";

#[test]
fn a_directory_swapped_for_a_link_while_the_walk_is_in_it_is_still_walked_as_the_directory_it_was()
{
    let d = dir("walk-open");
    put(&d.0, "pages/Sub/A.md", GOOD.as_bytes());
    put(&d.0, "attachments/x/y.bin", b"abc");
    let twin = dir("walk-open-twin");
    put(&twin.0, "Evil.md", EVIL.as_bytes());
    let (root, twin_path) = (d.0.clone(), twin.0.clone());
    let mut done = false;
    let plan = with_hook(
        move |path, stage| {
            if stage == Stage::Opened && path.ends_with("pages/Sub") && !done {
                done = true;
                // Between opening the directory and listing it, its place becomes a link to the twin.
                std::fs::rename(root.join("pages/Sub"), root.join("pages/Sub.moved")).unwrap();
                std::os::unix::fs::symlink(&twin_path, root.join("pages/Sub")).unwrap();
            }
        },
        || plan_folder(&d.0, &nothing(), false).unwrap(),
    );
    // The walk went on in the directory it held: the folder's own page, not the twin's.
    let paths: Vec<&str> = plan.files.iter().map(|f| f.path.as_str()).collect();
    assert!(
        paths.contains(&"pages/Sub/A.md") && !paths.iter().any(|p| p.contains("Evil")),
        "{}",
        plan.report()
    );
    assert!(!plan.report().contains("Evil"), "{}", plan.report());
    // The path is a link now, so the run refuses it (the identities are the second line).
    let why = changed(read_files(&plan).unwrap_err());
    assert!(
        why.contains("`pages/Sub`") && why.contains("no longer the directory that was planned"),
        "{why}"
    );
}

#[test]
fn a_directory_swapped_for_a_link_after_it_was_listed_is_refused_and_not_entered() {
    let d = dir("walk-listed");
    put(&d.0, "pages/Sub/A.md", GOOD.as_bytes());
    let twin = dir("walk-listed-twin");
    put(&twin.0, "Evil.md", EVIL.as_bytes());
    let (root, twin_path) = (d.0.clone(), twin.0.clone());
    let mut done = false;
    let plan = with_hook(
        move |path, stage| {
            if stage == Stage::Listed && path.ends_with("pages") && !done {
                done = true;
                // The listing of `pages` said `Sub` is a directory; now it is a link.
                std::fs::rename(root.join("pages/Sub"), root.join("pages/Sub.moved")).unwrap();
                std::os::unix::fs::symlink(&twin_path, root.join("pages/Sub")).unwrap();
            }
        },
        || plan_folder(&d.0, &nothing(), false).unwrap(),
    );
    assert!(
        plan.refused
            .iter()
            .any(|r| r.name == "pages/Sub" && r.reason.contains("no longer a directory")),
        "{}",
        plan.report()
    );
    assert!(!plan.report().contains("Evil"), "{}", plan.report());
    assert!(!plan.is_importable());
}

#[test]
fn the_folder_named_through_a_link_is_walked_and_its_replacement_is_refused_at_the_run() {
    // The operator may name a link to the folder (that is their choice of place); what is under it is not
    // followed. The run reaches it the same way and checks it is the same inode.
    let d = dir("rootlink");
    put(&d.0, "real/pages/A.md", GOOD.as_bytes());
    let link = d.0.join("link");
    std::os::unix::fs::symlink(d.0.join("real"), &link).unwrap();
    let plan = plan_folder(&link, &nothing(), false).unwrap();
    assert!(plan.is_importable(), "{}", plan.report());
    assert_eq!(read_files(&plan).unwrap().len(), 1);
    put(&d.0, "other/pages/A.md", GOOD.as_bytes());
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(d.0.join("other"), &link).unwrap();
    let why = changed(read_files(&plan).unwrap_err());
    assert!(why.contains("`the folder`"), "{why}");
}

#[test]
fn a_file_with_another_name_is_refused_at_the_plan_and_when_the_name_appears_after_it() {
    let d = dir("hardlink");
    put(&d.0, "pages/A.md", GOOD.as_bytes());
    put(&d.0, "attachments/a.bin", b"abc");
    std::fs::hard_link(d.0.join("pages/A.md"), d.0.join("pages/Twin.md")).unwrap();
    // An attachment is not read at the plan, so the link count is what refuses it there.
    std::fs::hard_link(d.0.join("attachments/a.bin"), d.0.join("attachments/b.bin")).unwrap();
    let plan = plan_folder(&d.0, &nothing(), false).unwrap();
    for name in [
        "pages/A.md",
        "pages/Twin.md",
        "attachments/a.bin",
        "attachments/b.bin",
    ] {
        assert!(
            plan.refused
                .iter()
                .any(|r| r.name == name && r.reason.contains("more than one hard link")),
            "{name}: {}",
            plan.report()
        );
    }
    // Planned with one name, run after another has appeared (a name outside the folder keeps the bytes
    // changeable after they were checked).
    std::fs::remove_file(d.0.join("pages/Twin.md")).unwrap();
    std::fs::remove_file(d.0.join("pages/A.md")).unwrap();
    std::fs::remove_file(d.0.join("attachments/b.bin")).unwrap();
    let plan = plan_folder(&d.0, &nothing(), false).unwrap();
    assert!(plan.is_importable(), "{}", plan.report());
    let outside = dir("hardlink-outside");
    std::fs::hard_link(d.0.join("attachments/a.bin"), outside.0.join("a")).unwrap();
    let why = changed(read_files(&plan).unwrap_err());
    assert!(why.contains("more than one hard link"), "{why}");
}

// ---- R5: no reason carries a byte of the folder's content ----

#[test]
fn a_refusal_never_carries_a_byte_of_the_pages_text_and_every_reason_is_escaped() {
    let d = dir("escape");
    // A page whose key, repeated, is a terminal escape sequence.
    put(
        &d.0,
        "pages/Dup.md",
        "---\ntitle: Dup\n\u{1b}[2Jowned: 1\n\u{1b}[2Jowned: 2\n---\nbody\n".as_bytes(),
    );
    let plan = plan_folder(&d.0, &nothing(), false).unwrap();
    let reason = &plan
        .refused
        .iter()
        .find(|r| r.name == "pages/Dup.md")
        .expect("the page is refused")
        .reason;
    assert!(
        reason.contains("frontmatter the parser refuses") && reason.contains("a key is repeated"),
        "{reason}"
    );
    let report = plan.report();
    assert!(
        !report.contains("owned") && !report.contains('\u{1b}'),
        "{report}"
    );

    // And whatever a reason is made of is escaped the way a name is, whichever rule wrote it.
    let have = nothing();
    let mut walk = Walk {
        existing: &have,
        replace_existing: false,
        page_cap: 1 << 20,
        plan: ImportPlan::default(),
        keys: HashMap::new(),
        ids: HashMap::new(),
        pages: Vec::new(),
    };
    walk.refuse("n\u{1b}", "a\u{1b}[31mb\nc \u{202e}d it's \"q\"");
    let r = walk.plan.refused.first().unwrap();
    assert!(
        !r.name.chars().any(char::is_control) && !r.reason.chars().any(char::is_control),
        "{r:?}"
    );
    assert_eq!(r.reason, "a\\u{1b}[31mb\\nc \\u{202e}d it's \"q\"");
}

// ---- R7: against the wiki, at one tip ----

#[test]
fn a_page_path_must_be_the_canonical_one_of_its_title() {
    let d = dir("canonical");
    // Spelled with an escape that need not be one, or in lower-case hex: two paths for one title.
    put(&d.0, "pages/Fine%20Page.md", GOOD.as_bytes());
    put(&d.0, "pages/a%41.md", GOOD.replace("g-1", "g-2").as_bytes());
    put(
        &d.0,
        "pages/caf%c3%a9.md",
        GOOD.replace("g-1", "g-3").as_bytes(),
    );
    // Canonical: a space as it is, an escape that must be one, a hierarchy.
    put(
        &d.0,
        "pages/Getting Started.md",
        GOOD.replace("g-1", "g-4").as_bytes(),
    );
    put(
        &d.0,
        "pages/caf%C3%A9.md",
        GOOD.replace("g-1", "g-5").as_bytes(),
    );
    put(
        &d.0,
        "pages/Docs/Deep.md",
        GOOD.replace("g-1", "g-6").as_bytes(),
    );
    // An attachment's name is its own business.
    put(&d.0, "attachments/My File.bin", b"x");
    let plan = plan_folder(&d.0, &nothing(), false).unwrap();
    let reason = |name: &str| {
        plan.refused
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.reason.clone())
    };
    for (name, canonical) in [
        ("pages/Fine%20Page.md", "pages/Fine Page.md"),
        ("pages/a%41.md", "pages/aA.md"),
        ("pages/caf%c3%a9.md", "pages/caf%C3%A9.md"),
    ] {
        assert!(
            reason(name)
                .is_some_and(|r| r.contains("canonical") && r.contains(&format!("`{canonical}`"))),
            "{name}: {}",
            plan.report()
        );
    }
    assert_eq!(plan.refused.len(), 3, "{}", plan.report());
}

#[test]
fn names_and_page_ids_are_checked_against_the_wiki_not_only_the_folder() {
    let d = dir("against");
    // The wiki has Help (id h-1) and Old (id g-1).
    let wiki = nothing()
        .with_file("pages/Help.md", b"---\ntitle: Help\nid: h-1\n---\n")
        .with_file("pages/Old.md", GOOD.as_bytes());
    // A path that differs from the wiki's only in case.
    put(&d.0, "pages/HELP.md", b"---\ntitle: HELP\n---\n");
    // A new page that takes the id of a page the wiki keeps.
    put(&d.0, "pages/New.md", GOOD.replace("Good", "New").as_bytes());
    let plan = plan_folder(&d.0, &wiki, true).unwrap();
    let reason = |name: &str| {
        plan.refused
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.reason.clone())
            .unwrap_or_default()
    };
    assert!(
        reason("pages/HELP.md").contains("collides with `pages/Help.md`, which the wiki has"),
        "{}",
        plan.report()
    );
    assert!(
        reason("pages/New.md").contains("has the page id of `pages/Old.md`, which the wiki has"),
        "{}",
        plan.report()
    );
    assert!(!plan.is_importable());

    // The same id at the same path (a replacement), and an id whose page this import rewrites, are not taken.
    let e = dir("against-ok");
    put(
        &e.0,
        "pages/Old.md",
        GOOD.replace("Good", "Newer").as_bytes(),
    );
    let plan = plan_folder(&e.0, &wiki, true).unwrap();
    assert!(plan.is_importable(), "{}", plan.report());
    let e = dir("against-moved");
    put(&e.0, "pages/Old.md", GOOD.replace("g-1", "g-7").as_bytes());
    put(&e.0, "pages/New.md", GOOD.as_bytes());
    let plan = plan_folder(&e.0, &wiki, true).unwrap();
    assert!(plan.is_importable(), "{}", plan.report());
}

#[test]
fn a_file_over_the_cap_of_one_acceptance_is_refused_at_the_plan_and_sizes_are_summed_without_overflow()
 {
    let d = dir("cap");
    let big = std::fs::File::create(d.0.join("big.bin")).unwrap();
    big.set_len(MAX_CHANGE_BYTES + 1).unwrap();
    std::fs::create_dir_all(d.0.join("attachments")).unwrap();
    std::fs::rename(d.0.join("big.bin"), d.0.join("attachments/big.bin")).unwrap();
    let plan = plan_folder(&d.0, &nothing(), false).unwrap();
    assert!(
        plan.refused
            .iter()
            .any(|r| r.name == "attachments/big.bin" && r.reason.contains("cap of one acceptance")),
        "{}",
        plan.report()
    );
    let mut sums = ImportPlan::default();
    for path in ["attachments/a", "attachments/b"] {
        sums.files.push(PlannedFile {
            path: path.to_owned(),
            size: u64::MAX,
            action: Action::Add,
            source: PathBuf::new(),
            identity: Identity { dev: 0, ino: 0 },
        });
    }
    assert_eq!(sums.total_bytes(), u64::MAX);
}

#[test]
fn the_plan_is_pinned_to_the_tip_and_a_plan_without_one_is_not_run() {
    let d = dir("pinned");
    put(&d.0, "pages/A.md", GOOD.as_bytes());
    let plan = plan_folder(&d.0, &nothing(), false).unwrap();
    assert_eq!(plan.base, TIP);
    let (rig, accept, acts) = world("pinned");
    run_import(&acts, &rig.wiki, &plan).unwrap();
    let during = accept.pending_during.lock().unwrap().clone().unwrap();
    assert_eq!(
        during.pinned_tip.as_ref().map(BlockId::as_str),
        Some(TIP),
        "the signer is told the tip the plan was made against"
    );
    *accept.seen.lock().unwrap() = None;
    let mut unpinned = plan.clone();
    unpinned.base = String::new();
    let err = run_import(&acts, &rig.wiki, &unpinned).unwrap_err();
    assert!(matches!(err, ImportError::Message(_)), "{err}");
    assert!(accept.seen.lock().unwrap().is_none());
}

#[test]
fn a_folder_the_wiki_already_has_is_nothing_changed_and_a_mixed_one_writes_only_what_differs() {
    let d = dir("unchanged");
    put(&d.0, "pages/Home.md", GOOD.as_bytes());
    put(&d.0, "attachments/a.bin", b"abc");
    let has = nothing()
        .with_file("pages/Home.md", GOOD.as_bytes())
        .with_file("attachments/a.bin", b"abc");
    // Without --replace-existing the paths are still taken: that rule is not softened.
    let taken = plan_folder(&d.0, &has, false).unwrap();
    assert_eq!(taken.refused.len(), 2);
    let plan = plan_folder(&d.0, &has, true).unwrap();
    assert!(
        plan.is_unchanged() && !plan.is_importable(),
        "{}",
        plan.report()
    );
    assert_eq!(plan.unchanged, vec!["attachments/a.bin", "pages/Home.md"]);
    assert!(plan.report().contains("unchanged (2)"), "{}", plan.report());
    let (rig, accept, acts) = world("unchanged");
    let err = run_import(&acts, &rig.wiki, &plan).unwrap_err();
    assert!(matches!(err, ImportError::NothingChanged(2)), "{err}");
    assert!(
        err.to_string().starts_with("nothing changed:")
            && err.to_string().contains("all 2 files")
            && err.to_string().contains("nothing was written"),
        "{err}"
    );
    assert!(
        accept.seen.lock().unwrap().is_none(),
        "the signer was asked"
    );

    // One file differs: it alone is written, the other is reported unchanged.
    put(&d.0, "attachments/a.bin", b"abcd");
    let plan = plan_folder(&d.0, &has, true).unwrap();
    assert!(plan.is_importable() && !plan.is_unchanged());
    let written: Vec<(&str, Action)> = plan
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.action))
        .collect();
    assert_eq!(written, vec![("attachments/a.bin", Action::Replace)]);
    assert_eq!(plan.unchanged, vec!["pages/Home.md"]);
    run_import(&acts, &rig.wiki, &plan).unwrap();
    let change = accept.seen.lock().unwrap().clone().unwrap();
    assert_eq!(change.files.len(), 1);
}
