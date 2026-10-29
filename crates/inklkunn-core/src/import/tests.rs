#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::ffi::OsStrExt;

use super::*;

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

fn none_exist(_: &str) -> bool {
    false
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
    let plan = plan_folder(&d.0, &none_exist, false).unwrap();
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
    let plan = plan_folder(&d.0, &none_exist, false).unwrap();
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
    let mut walk = Walk {
        existing: &none_exist,
        replace_existing: false,
        page_cap: 1 << 20,
        plan: ImportPlan::default(),
        keys: HashMap::new(),
        ids: HashMap::new(),
    };
    let long_name = format!("attachments/{}.bin", "x".repeat(256));
    walk.file(Path::new("/nonexistent"), &long_name);
    let long_path = format!("attachments/{}/f.bin", vec!["d".repeat(200); 6].join("/"));
    assert!(long_path.len() > 1024);
    walk.file(Path::new("/nonexistent"), &long_path);
    let reasons: Vec<&str> = walk
        .plan
        .refused
        .iter()
        .map(|r| r.reason.as_str())
        .collect();
    assert!(
        reasons[0].contains("260 bytes") && reasons[0].contains("255-byte cap"),
        "{reasons:?}"
    );
    assert!(reasons[1].contains("over the 1024-byte cap"), "{reasons:?}");
}

#[test]
fn a_path_the_wiki_has_is_refused_unless_replacing_is_asked_for() {
    let d = dir("exists");
    put(&d.0, "pages/Help.md", GOOD.as_bytes());
    put(&d.0, "pages/New.md", GOOD.replace("g-1", "g-9").as_bytes());
    let has = |p: &str| p == "pages/Help.md";
    let refused = plan_folder(&d.0, &has, false).unwrap();
    assert!(!refused.is_importable());
    assert_eq!(refused.refused.len(), 1);
    assert!(refused.refused[0].reason.contains("--replace-existing"));
    let replaced = plan_folder(&d.0, &has, true).unwrap();
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
        plan_folder(&d.0, &none_exist, false),
        Err(ImportError::Nothing)
    ));
    put(&d.0, "afile", b"x");
    let err = plan_folder(&d.0.join("afile"), &none_exist, false).unwrap_err();
    assert!(err.to_string().contains("not a directory"), "{err}");
    let err = plan_folder(&d.0.join("missing"), &none_exist, false).unwrap_err();
    assert!(matches!(err, ImportError::Folder(_)), "{err}");
    // A top-level `pages` that is a link is refused, not followed.
    let e = dir("toplink");
    std::os::unix::fs::symlink(&d.0, e.0.join("pages")).unwrap();
    let plan = plan_folder(&e.0, &none_exist, false).unwrap();
    assert!(!plan.is_importable());
    assert!(plan.refused[0].reason.contains("symbolic link"));
}

#[test]
fn a_planning_and_a_dry_run_write_nothing_and_a_file_swapped_for_a_link_is_not_followed() {
    let d = dir("readonly");
    put(&d.0, "pages/A.md", GOOD.as_bytes());
    put(&d.0, "attachments/a.bin", b"abc");
    let before = snapshot(&d.0);
    let plan = plan_folder(&d.0, &none_exist, false).unwrap();
    let _ = plan.report();
    assert_eq!(snapshot(&d.0), before, "planning changed the folder");
    // Between the plan and the read, an attachment becomes a link to something else.
    let secret = dir("secret");
    put(&secret.0, "s.txt", b"abc");
    std::fs::remove_file(d.0.join("attachments/a.bin")).unwrap();
    std::os::unix::fs::symlink(secret.0.join("s.txt"), d.0.join("attachments/a.bin")).unwrap();
    let err = read_files(&plan).unwrap_err();
    assert!(matches!(err, ImportError::Changed(_)), "{err}");
    assert!(
        err.to_string().contains("no longer a regular file"),
        "{err}"
    );
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
    ledger: std::sync::Arc<ImportLedger>,
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

fn record() -> WikiRecord {
    use inklkunn_store::{DisplayName, NewWiki, Slug, WikiId};
    WikiRecord::from_new(
        WikiId::new(),
        NewWiki::new(Slug::new("imp").unwrap(), DisplayName::new("Imp").unwrap()),
        1,
    )
    .unwrap()
}

#[test]
fn the_run_binds_the_seal_to_the_planned_files_under_the_system_principal_and_forgets_the_record() {
    let d = dir("run");
    put(&d.0, "pages/A.md", GOOD.as_bytes());
    put(&d.0, "attachments/a.bin", b"abc");
    let plan = plan_folder(&d.0, &none_exist, false).unwrap();
    let ledger = std::sync::Arc::new(ImportLedger::new());
    let accept = Recorder {
        seen: Mutex::new(None),
        ledger: ledger.clone(),
        pending_during: Mutex::new(None),
    };
    let wiki = record();
    let done = run_import(&accept, &ledger, &wiki, &plan).unwrap();
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
    assert!(ledger.pending.lock().unwrap().is_empty());
    // A plan with a refusal never reaches the signer.
    put(&d.0, "pages/CON.md", b"x");
    let bad = plan_folder(&d.0, &none_exist, false).unwrap();
    *accept.seen.lock().unwrap() = None;
    let err = run_import(&accept, &ledger, &wiki, &bad).unwrap_err();
    assert!(matches!(err, ImportError::Refused(1)), "{err}");
    assert!(accept.seen.lock().unwrap().is_none());
}
