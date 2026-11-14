//! The bundle's knowledge extension (RFC 005 part 2 D-11, D-12; XFER-1, XFER-10; handoff 005 K-T10, K-T11, K41): a claim's
//! thread is exported with the bundle, one JSON Lines file per thread under `extensions/inklkunn-knowledge/threads/` (never
//! under `discussions/`: that is wiki-commons' one file per page), its bodies escaped to portable Markdown that shows the
//! plain text, the views' creation records beside it, all listed in the checksums, the authors in `users.yaml`, the
//! manifest declaring `inklkunn-knowledge` in `extensions_used` and valid against wiki-commons' schema. The repository is a
//! stand-in that holds files in memory and a history of one block, so these run in both test runs (no prikk).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use inklkunn_core::export::{
    KnowledgeRecords, claim_discussions, export_bundle, export_bundle_with, knowledge_records,
};
use inklkunn_prikk::storage::{
    Change, Diff, FileMeta, History, HistoryBlock, Materialized, Tree, Verification, Views,
};
use inklkunn_prikk::{ExtractBounds, Point, Storage, StorageError};
use inklkunn_store::{
    Account, AccountId, AccountStore, ActStore, ClaimCommentRecord, ClaimThreadStore, CommentId,
    DisplayName, MemoryStore, NewWiki, RootLayout, Slug, ViewActEnd, ViewActId, ViewActOutcome,
    ViewActRecord, ViewKind, WikiRecord, WikiStore,
};
use sha2::{Digest, Sha256};

#[path = "support/bundle_manifest.rs"]
mod bundle_manifest;
#[path = "support/json_schema.rs"]
mod json_schema;

const TIP: &str = "b10c000000000000000000000000000000000000000000000000000000000001";
const C1: &str = "019a0000-0000-7000-8000-000000000c01";
const C2: &str = "019a0000-0000-7000-8000-000000000c02";

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir(tag: &str) -> Dir {
    let p = std::env::temp_dir().join(format!("inklkunn-xd-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("dir");
    Dir(p)
}

/// A repository that holds these files and one block.
struct Stand {
    files: BTreeMap<String, String>,
}

fn unsupported<T>() -> Result<T, StorageError> {
    Err(StorageError::Refused {
        verb: "stand-in",
        message: "the stand-in answers only what an export asks".to_owned(),
        kind: None,
        reason: None,
    })
}

impl Storage for Stand {
    fn views(&self, _: &Path) -> Result<Views, StorageError> {
        unsupported()
    }
    fn all_views(&self, _: &Path) -> Result<Views, StorageError> {
        unsupported()
    }
    fn tree(&self, _: &Path, point: &Point, _: Option<&str>) -> Result<Tree, StorageError> {
        Ok(Tree {
            schema_version: "tree-listing-v1".to_owned(),
            point: point.to_string(),
            target_block_id: TIP.to_owned(),
            prefix: None,
            entries: Vec::new(),
        })
    }
    fn stat(&self, _: &Path, _: &Point, _: &str) -> Result<FileMeta, StorageError> {
        unsupported()
    }
    fn read(&self, _: &Path, _: &Point, _: &str, _: u64) -> Result<Vec<u8>, StorageError> {
        unsupported()
    }
    fn history(&self, _: &Path, view: &Point, _: Option<u64>) -> Result<History, StorageError> {
        Ok(History {
            schema_version: "log-report-v1".to_owned(),
            view: view.to_string(),
            blocks: vec![HistoryBlock {
                block_id: TIP.to_owned(),
                ref_state_id: "a".repeat(64),
                update_seq: 1,
                kind: "Root".to_owned(),
                rollback_block: false,
                patch_count: 0,
                patch_messages: Vec::new(),
                previous_ref_state_id: None,
            }],
        })
    }
    fn change(&self, _: &Path, _: &str) -> Result<Change, StorageError> {
        Ok(Change {
            schema_version: "show-report-v1".to_owned(),
            patches: Vec::new(),
        })
    }
    fn diff(&self, _: &Path, _: &Point, _: &Point, _: &[&str]) -> Result<Diff, StorageError> {
        unsupported()
    }
    fn plan(&self, _: &Path, _: &Point) -> Result<Materialized, StorageError> {
        unsupported()
    }
    fn materialize(
        &self,
        _: &Path,
        point: &Point,
        scratch: &Path,
        name: &str,
        _: ExtractBounds,
    ) -> Result<Materialized, StorageError> {
        let target = scratch.join(name);
        std::fs::create_dir_all(&target).expect("target");
        for (path, text) in &self.files {
            let at = target.join(path);
            std::fs::create_dir_all(at.parent().expect("a parent")).expect("dirs");
            std::fs::write(at, text).expect("a file");
        }
        Ok(Materialized {
            schema_version: "extract-report-v1".to_owned(),
            point: point.to_string(),
            point_kind: "block".to_owned(),
            target_block_id: TIP.to_owned(),
            claim_parent_block_id: None,
            provisional: false,
            written: true,
            file_count: self.files.len() as u64,
            total_bytes: self.files.values().map(|t| t.len() as u64).sum(),
            largest_file_bytes: 0,
        })
    }
    fn verify(&self, _: &Path) -> Result<Verification, StorageError> {
        unsupported()
    }
}

fn claim_file(id: &str) -> (String, String) {
    (
        format!("knowledge/claims/{id}.yaml"),
        "kind: observation\nlifecycle: proposed\nexpressions:\n  - page_id: p1\n    block: b1\n"
            .to_owned(),
    )
}

fn repository() -> Stand {
    let mut files: BTreeMap<String, String> = BTreeMap::new();
    files.insert(
        "pages/One.md".to_owned(),
        "---\ntitle: One\nid: p1\n---\n\nSaid. ^b1\n".to_owned(),
    );
    for id in [C1, C2] {
        let (path, text) = claim_file(id);
        files.insert(path, text);
    }
    Stand { files }
}

struct Rig {
    _dir: Dir,
    layout: RootLayout,
    store: MemoryStore,
    wiki: WikiRecord,
    out: PathBuf,
}

fn rig(tag: &str) -> Rig {
    let d = dir(tag);
    let store = MemoryStore::new();
    let wiki = store
        .create(NewWiki::new(
            Slug::new("notes").unwrap(),
            DisplayName::new("Notes").unwrap(),
        ))
        .unwrap();
    Rig {
        layout: RootLayout::new(d.0.join("root")),
        out: d.0.join("out"),
        _dir: d,
        store,
        wiki,
    }
}

fn person(store: &MemoryStore, name: &str) -> AccountId {
    let id = AccountId::new();
    store
        .add_account(&Account {
            id: id.clone(),
            display_name: DisplayName::new(name).unwrap(),
            local: None,
            created_at: 1,
            disabled: false,
        })
        .unwrap();
    id
}

fn say(
    r: &Rig,
    claim: &str,
    author: &AccountId,
    parent: Option<&CommentId>,
    body: &str,
    at: i64,
) -> CommentId {
    let c = ClaimCommentRecord {
        id: CommentId::new(),
        wiki: r.wiki.id.clone(),
        claim: claim.to_owned(),
        author: author.clone(),
        parent: parent.cloned(),
        body: body.to_owned(),
        created_at: at,
    };
    r.store.add_claim_comment(&c).unwrap();
    c.id
}

fn files_of(root: &Path) -> BTreeMap<String, Vec<u8>> {
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
    walk(root, root, &mut out);
    out
}

fn text(files: &BTreeMap<String, Vec<u8>>, path: &str) -> String {
    String::from_utf8(
        files
            .get(path)
            .unwrap_or_else(|| panic!("the bundle has no `{path}`"))
            .clone(),
    )
    .unwrap()
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn view_made(r: &Rig, name: &str, kind: ViewKind, by: &AccountId, at: i64, ended: Option<bool>) {
    let act = ViewActRecord {
        id: ViewActId::new(),
        wiki: r.wiki.id.clone(),
        name: name.to_owned(),
        forked_from: "main".to_owned(),
        from_block: "ab".repeat(32),
        kind,
        created_by: by.clone(),
        created_at: at,
    };
    r.store.begin_view_act(&act).unwrap();
    if let Some(created) = ended {
        r.store
            .finish_view_act(&ViewActEnd {
                act: act.id.clone(),
                outcome: if created {
                    ViewActOutcome::Created
                } else {
                    ViewActOutcome::Failed {
                        word: "refused".to_owned(),
                    }
                },
                at: at + 1,
            })
            .unwrap();
    }
}

#[test]
fn a_claims_thread_is_exported_in_the_knowledge_extension_beside_the_claim_and_changes_nothing_of_it()
 {
    let r = rig("threads");
    let (ann, ben) = (person(&r.store, "Ann"), person(&r.store, "Ben"));
    let first = say(
        &r,
        C1,
        &ann,
        None,
        "Where was this measured?",
        1_700_000_000,
    );
    let reply = say(
        &r,
        C1,
        &ben,
        Some(&first),
        "At the north gauge.",
        1_700_000_060,
    );
    say(
        &r,
        C1,
        &ann,
        Some(&reply),
        "Thank you — <b>noted</b>.",
        1_700_000_120,
    );
    say(&r, C2, &ben, None, "A different claim.", 1_700_000_200);
    // Two views were made (one has people, one failed and took nothing in, one is not known to have ended).
    view_made(
        &r,
        "draft",
        ViewKind::Experimental,
        &ben,
        1_700_000_300,
        Some(true),
    );
    view_made(
        &r,
        "rival",
        ViewKind::Interpretation,
        &ann,
        1_700_000_400,
        Some(true),
    );
    view_made(
        &r,
        "lost",
        ViewKind::Experimental,
        &ann,
        1_700_000_500,
        Some(false),
    );
    view_made(
        &r,
        "open",
        ViewKind::Experimental,
        &ann,
        1_700_000_600,
        None,
    );

    let records = knowledge_records(&r.store, &r.store, &r.store, &r.wiki.id).unwrap();
    assert_eq!(
        records
            .threads
            .iter()
            .map(|d| d.claim.as_str())
            .collect::<Vec<_>>(),
        [C1, C2],
        "by claim id"
    );
    assert_eq!(records.threads.first().unwrap().comments.len(), 3);
    assert_eq!(
        records
            .views
            .iter()
            .map(|v| v.name.as_str())
            .collect::<Vec<_>>(),
        ["draft", "rival"],
        "only the views that were created"
    );

    let stand = repository();
    let done = export_bundle_with(
        &stand,
        &r.layout,
        &r.wiki,
        &r.out,
        "0.0.0",
        1_700_001_000,
        &records,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((done.threads, done.views, done.knowledge_files), (2, 2, 2));
    let files = files_of(&r.out);

    // Nothing of a claim's thread is where wiki-commons puts a page's discussion.
    assert!(
        !files.keys().any(|k| k.starts_with("discussions")),
        "{:?}",
        files.keys().collect::<Vec<_>>()
    );

    // One file per thread, one line per comment in the order written, in wiki-commons' record shape.
    let thread = text(
        &files,
        &format!("extensions/inklkunn-knowledge/threads/{C1}.jsonl"),
    );
    let lines: Vec<serde_json::Value> = thread
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 3);
    let at =
        |v: &serde_json::Value, p: &str| v.pointer(p).cloned().unwrap_or(serde_json::Value::Null);
    assert_eq!(at(lines.first().unwrap(), "/id"), first.as_str());
    assert_eq!(
        at(lines.first().unwrap(), "/parent"),
        serde_json::Value::Null
    );
    assert_eq!(at(lines.first().unwrap(), "/at"), "2023-11-14T22:13:20Z");
    assert_eq!(at(lines.first().unwrap(), "/author/id"), ann.as_str());
    assert_eq!(at(lines.first().unwrap(), "/author/name"), "Ann");
    // The body is portable Markdown that shows the plain text: the `?` is not markup, so it is as written.
    assert_eq!(
        at(lines.first().unwrap(), "/body"),
        "Where was this measured?"
    );
    assert_eq!(at(lines.get(1).unwrap(), "/parent"), first.as_str());
    assert_eq!(at(lines.get(1).unwrap(), "/author/name"), "Ben");
    // Markup in a text is escaped: the HTML in it is letters in the bundle.
    assert_eq!(
        at(lines.get(2).unwrap(), "/body"),
        "Thank you — \\<b\\>noted\\</b\\>."
    );
    assert_eq!(
        text(
            &files,
            &format!("extensions/inklkunn-knowledge/threads/{C2}.jsonl")
        )
        .lines()
        .count(),
        1
    );

    // The views' creation records, in the order the views were made.
    let views: Vec<serde_json::Value> = text(&files, "extensions/inklkunn-knowledge/views.jsonl")
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(views.len(), 2);
    assert_eq!(at(views.first().unwrap(), "/name"), "draft");
    assert_eq!(at(views.first().unwrap(), "/kind"), "experimental");
    assert_eq!(at(views.first().unwrap(), "/forked_from"), "main");
    assert_eq!(at(views.first().unwrap(), "/from_block"), "ab".repeat(32));
    assert_eq!(at(views.first().unwrap(), "/created_by/name"), "Ben");
    assert_eq!(at(views.get(1).unwrap(), "/kind"), "interpretation");

    // The claim files are the repository's, byte for byte: the discussion is beside them, not in them.
    for id in [C1, C2] {
        let (path, original) = claim_file(id);
        assert_eq!(text(&files, &path), original);
    }

    // The people who wrote or made a view are in users.yaml with the names they have.
    let users = text(&files, "users.yaml");
    for (id, name) in [(&ann, "Ann"), (&ben, "Ben")] {
        assert!(users.contains(&format!("id: {id}\n")), "{users}");
        assert!(users.contains(&format!("names: [{name}]")), "{users}");
    }
    assert_eq!(done.users, 2);

    // The checksums list every file but themselves and the manifest, and each is right.
    let sums: BTreeMap<String, String> = text(&files, "SHA256SUMS")
        .lines()
        .map(|l| {
            let (sum, path) = l.split_once("  ").unwrap();
            (path.to_owned(), sum.to_owned())
        })
        .collect();
    for path in [
        format!("extensions/inklkunn-knowledge/threads/{C1}.jsonl"),
        format!("extensions/inklkunn-knowledge/threads/{C2}.jsonl"),
        "extensions/inklkunn-knowledge/views.jsonl".to_owned(),
    ] {
        assert_eq!(
            sums.get(&path),
            Some(&sha(files.get(&path).unwrap())),
            "{path}"
        );
    }
    assert_eq!(sums.len(), files.len() - 2);

    // The manifest declares the extension, says nothing of page discussions, and is valid against wiki-commons' schema.
    let manifest = text(&files, "wiki-bundle.yaml");
    assert!(
        manifest.contains(
            "\nextensions_used:\n  - name: inklkunn-knowledge\n    version: \"1\"\n    directory: extensions/inklkunn-knowledge\n"
        ),
        "{manifest}"
    );
    assert!(manifest.contains("\n  discussions: false\n"), "{manifest}");
    assert!(
        !manifest.contains("discussions_representation"),
        "{manifest}"
    );
    assert!(manifest.contains("  dropped: []\n"), "{manifest}");
    let problems = bundle_manifest::violations(&manifest);
    assert!(problems.is_empty(), "{problems:#?}\n{manifest}");
}

#[test]
fn a_wiki_with_no_thread_and_no_view_carries_the_extension_only_for_its_knowledge_files() {
    let r = rig("none");
    let records = knowledge_records(&r.store, &r.store, &r.store, &r.wiki.id).unwrap();
    assert_eq!(records, KnowledgeRecords::default());
    // With claim files: `knowledge/` is exported as it is stored, and the manifest declares what gives it meaning.
    let stand = repository();
    let done = export_bundle_with(
        &stand,
        &r.layout,
        &r.wiki,
        &r.out,
        "0.0.0",
        1_700_001_000,
        &records,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!((done.threads, done.views, done.knowledge_files), (0, 0, 2));
    let files = files_of(&r.out);
    assert!(
        !files
            .keys()
            .any(|k| k.contains("discussions") || k.starts_with("extensions")),
        "{files:?}"
    );
    for id in [C1, C2] {
        let (path, original) = claim_file(id);
        assert_eq!(text(&files, &path), original, "knowledge/ as it is stored");
    }
    let manifest = text(&files, "wiki-bundle.yaml");
    assert!(manifest.contains("\nextensions_used:\n"), "{manifest}");
    assert!(manifest.contains("\n  discussions: false\n"), "{manifest}");
    assert!(bundle_manifest::violations(&manifest).is_empty());

    // Without a single knowledge file there is nothing the extension would give meaning to: no declaration.
    let bare = Stand {
        files: BTreeMap::from([(
            "pages/One.md".to_owned(),
            "---\ntitle: One\nid: p1\n---\n\nSaid.\n".to_owned(),
        )]),
    };
    let again = dir("again");
    let plain = again.0.join("out");
    export_bundle_with(
        &bare,
        &r.layout,
        &r.wiki,
        &plain,
        "0.0.0",
        1_700_001_000,
        &records,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let bare_manifest = text(&files_of(&plain), "wiki-bundle.yaml");
    assert!(
        !bare_manifest.contains("extensions_used"),
        "{bare_manifest}"
    );
    assert!(bundle_manifest::violations(&bare_manifest).is_empty());
}

#[test]
fn an_export_that_was_not_given_the_services_records_says_so_in_the_manifest() {
    let r = rig("unread");
    let ann = person(&r.store, "Ann");
    say(&r, C1, &ann, None, "A thread the export was not given.", 5);
    let stand = repository();
    export_bundle(&stand, &r.layout, &r.wiki, &r.out, "0.0.0", 1_700_001_000)
        .unwrap_or_else(|e| panic!("{e}"));
    let files = files_of(&r.out);
    assert!(
        !files.keys().any(|k| k.starts_with("extensions")),
        "{files:?}"
    );
    let manifest = text(&files, "wiki-bundle.yaml");
    assert!(
        manifest.contains("claim threads and view-creation records"),
        "the manifest does not look complete when it is not\n{manifest}"
    );
    assert!(bundle_manifest::violations(&manifest).is_empty());
}

#[test]
fn the_names_of_the_people_are_the_names_they_have_and_a_person_the_service_lost_is_nameless_not_dropped()
 {
    let r = rig("names");
    let ann = person(&r.store, "Ann");
    say(&r, C1, &ann, None, "Hello", 5);
    let discussions = claim_discussions(&r.store, &r.store, &r.wiki.id).unwrap();
    let comment = discussions.first().unwrap().comments.first().unwrap();
    assert_eq!(comment.author_id, ann.as_str());
    assert_eq!(comment.author_name, "Ann");
    // Another store, where the account is not known: the comment is still carried, the name empty.
    let empty = MemoryStore::new();
    let none = claim_discussions(&r.store, &empty, &r.wiki.id).unwrap();
    assert_eq!(
        none.first().unwrap().comments.first().unwrap().author_name,
        ""
    );
}
