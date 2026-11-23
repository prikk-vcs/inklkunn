//! The export of suppressed content (RFC 006 D-13, D-8's last bullet; acceptance criterion 13; G-T4b).
//!
//! The repository is a stand-in that holds files in memory and a history of several blocks with real patch messages, so
//! these run in both test runs (no prikk); the tombstones are planted in the store directly. **The frozen bundle**
//! (`tests/fixtures/export-before-suppression/`) is what the export wrote for [`scenario`] before any of this existed: an
//! export with no tombstones must still write exactly those bytes.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use inklkunn_core::export::{export_bundle_with, knowledge_records};
use inklkunn_core::message::{Actor, BlockId, Message, Summary, SystemLabel};
use inklkunn_prikk::storage::{
    Change, Diff, Entry, FileMeta, History, HistoryBlock, Materialized, Operation, Patch,
    PatchMessage, PathRef, Tree, Verification, Views,
};
use inklkunn_prikk::{ExtractBounds, Point, Storage, StorageError};
use inklkunn_store::{
    Account, AccountId, AccountStore, ClaimCommentRecord, ClaimThreadStore, CommentId, DisplayName,
    MemoryStore, NewWiki, ProposalId, RootLayout, Slug, WikiRecord, WikiStore,
};

#[path = "support/bundle_manifest.rs"]
mod bundle_manifest;
#[path = "support/json_schema.rs"]
mod json_schema;

/// The blocks of the stand-in's history, oldest first.
const B0: &str = "b10c000000000000000000000000000000000000000000000000000000000000";
const B1: &str = "b10c000000000000000000000000000000000000000000000000000000000001";
const B2: &str = "b10c000000000000000000000000000000000000000000000000000000000002";
/// The tip.
const B3: &str = "b10c000000000000000000000000000000000000000000000000000000000003";
const C1: &str = "019a0000-0000-7000-8000-000000000c01";
const C2: &str = "019a0000-0000-7000-8000-000000000c02";
const ANN: &str = "019a0000-0000-7000-8000-0000000000a1";
const BEN: &str = "019a0000-0000-7000-8000-0000000000a2";
const CYD: &str = "019a0000-0000-7000-8000-0000000000a3";
const P1: &str = "019a0000-0000-7000-8000-000000000e01";
const P3: &str = "019a0000-0000-7000-8000-000000000e03";

/// The words of the summaries, each distinct, so that a grep of a bundle finds where one went.
const SUMMARY_B0: &str = "Create the wiki";
const SUMMARY_B1: &str = "Cyd writes about the harbour";
const SUMMARY_B2: &str = "Import the old pages";
const SUMMARY_B3: &str = "Ann corrects the tide table";

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir(tag: &str) -> Dir {
    let p = std::env::temp_dir().join(format!("inklkunn-xs-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("dir");
    Dir(p)
}

/// A repository that holds these files and this history.
struct Stand {
    files: BTreeMap<String, String>,
    /// Oldest first.
    blocks: Vec<(HistoryBlock, Change)>,
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
            target_block_id: B3.to_owned(),
            prefix: None,
            entries: self
                .files
                .iter()
                .map(|(path, text)| Entry {
                    path: path.clone(),
                    kind: "file".to_owned(),
                    encoding: None,
                    mode: None,
                    size: Some(text.len() as u64),
                })
                .collect(),
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
            blocks: self.blocks.iter().rev().map(|(b, _)| b.clone()).collect(),
        })
    }
    fn change(&self, _: &Path, block: &str) -> Result<Change, StorageError> {
        self.blocks
            .iter()
            .find(|(b, _)| b.block_id == block)
            .map(|(_, c)| c.clone())
            .ok_or(StorageError::Refused {
                verb: "stand-in",
                message: "no such block".to_owned(),
                kind: None,
                reason: None,
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
            target_block_id: B3.to_owned(),
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

fn account(id: &str, name: &str) -> Actor {
    Actor::Account {
        id: AccountId::parse(id).expect("an account id"),
        name: DisplayName::new(name).expect("a name"),
    }
}

fn block(
    id: &str,
    seq: u64,
    patch: &str,
    message: &Message,
    ops: &[(&str, &str)],
) -> (HistoryBlock, Change) {
    (
        HistoryBlock {
            block_id: id.to_owned(),
            ref_state_id: "a".repeat(64),
            update_seq: seq,
            kind: "Update".to_owned(),
            rollback_block: false,
            patch_count: 1,
            patch_messages: vec![PatchMessage {
                patch_id: patch.to_owned(),
                message: message.to_text(),
            }],
            previous_ref_state_id: None,
        },
        Change {
            schema_version: "show-report-v1".to_owned(),
            patches: vec![Patch {
                patch_id: patch.to_owned(),
                queued: false,
                operations: ops
                    .iter()
                    .map(|(kind, path)| Operation {
                        kind: (*kind).to_owned(),
                        paths: vec![PathRef {
                            path: Some((*path).to_owned()),
                            unresolved_node_id: None,
                        }],
                        content: serde_json::Value::Null,
                    })
                    .collect(),
            }],
        },
    )
}

fn summary(s: &str) -> Summary {
    Summary::from_text(s).expect("a summary")
}

fn base(id: &str) -> BlockId {
    BlockId::new(id).expect("a block id")
}

/// The pages the tip has: two pages, a claim and its source.
fn files() -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    files.insert(
        "pages/One.md".to_owned(),
        "---\ntitle: One\nid: p1\n---\n\nSaid. ^b1\n".to_owned(),
    );
    files.insert(
        "pages/Two.md".to_owned(),
        "---\ntitle: Two\nid: p2\n---\n\nAlso said.\n".to_owned(),
    );
    for id in [C1, C2] {
        files.insert(
            format!("knowledge/claims/{id}.yaml"),
            "kind: observation\nlifecycle: proposed\nexpressions:\n  - page_id: p1\n    block: b1\n"
                .to_owned(),
        );
    }
    files
}

/// The history: the wiki's creation (a page since removed), Cyd's proposal that Ben accepted (a page of the tip and a page
/// since removed), an import that touched both pages, and Ann's proposal that Ben accepted, on the first page.
fn history() -> Vec<(HistoryBlock, Change)> {
    let label = SystemLabel::inklkunn();
    let b0 = Message::system(summary(SUMMARY_B0), label.clone());
    let b1 = Message::proposal(
        summary(SUMMARY_B1),
        ProposalId::parse(P1).expect("a proposal id"),
        account(CYD, "Cyd"),
        account(BEN, "Ben"),
        base(B0),
    )
    .expect("a message");
    let b2 = Message::import(
        summary(SUMMARY_B2),
        label,
        inklkunn_core::message::ImportSource::new("folder sha256:00ff").expect("a source"),
    );
    let b3 = Message::proposal(
        summary(SUMMARY_B3),
        ProposalId::parse(P3).expect("a proposal id"),
        account(ANN, "Ann"),
        account(BEN, "Ben"),
        base(B2),
    )
    .expect("a message");
    vec![
        block(B0, 1, "a0", &b0, &[("add", "pages/Help.md")]),
        block(
            B1,
            2,
            "a1",
            &b1,
            &[("modify", "pages/Two.md"), ("delete", "pages/Gone.md")],
        ),
        block(
            B2,
            3,
            "a2",
            &b2,
            &[("modify", "pages/One.md"), ("modify", "pages/Two.md")],
        ),
        block(B3, 4, "a3", &b3, &[("modify", "pages/One.md")]),
    ]
}

fn stand() -> Stand {
    Stand {
        files: files(),
        blocks: history(),
    }
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
    for (id, name) in [(ANN, "Ann"), (BEN, "Ben"), (CYD, "Cyd")] {
        store
            .add_account(&Account {
                id: AccountId::parse(id).unwrap(),
                display_name: DisplayName::new(name).unwrap(),
                local: None,
                created_at: 1,
                disabled: false,
            })
            .unwrap();
    }
    Rig {
        layout: RootLayout::new(d.0.join("root")),
        out: d.0.join("out"),
        _dir: d,
        store,
        wiki,
    }
}

fn comment_id(n: u8) -> CommentId {
    CommentId::parse(&format!("019a0000-0000-7000-8000-0000000000{n:02x}")).unwrap()
}

/// Four comments in two threads: Ann asks, Ben answers, Cyd adds, and a comment on the other claim.
fn threads(r: &Rig) {
    let say = |n: u8, claim: &str, who: &str, parent: Option<u8>, body: &str, at: i64| {
        r.store
            .add_claim_comment(&ClaimCommentRecord {
                id: comment_id(n),
                wiki: r.wiki.id.clone(),
                claim: claim.to_owned(),
                author: AccountId::parse(who).unwrap(),
                parent: parent.map(comment_id),
                body: body.to_owned(),
                created_at: at,
            })
            .unwrap();
    };
    say(1, C1, ANN, None, "Where was this measured?", 1_700_000_000);
    say(2, C1, BEN, Some(1), "At the north gauge.", 1_700_000_060);
    say(3, C1, CYD, Some(2), "Thank you, noted.", 1_700_000_120);
    say(4, C2, BEN, None, "A different claim.", 1_700_000_200);
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

const NOW: i64 = 1_700_001_000;

/// Export the scenario, as the command line does: the records read from the store, then the bundle.
fn export(r: &Rig) -> BTreeMap<String, Vec<u8>> {
    let records = knowledge_records(&r.store, &r.store, &r.store, &r.wiki.id).unwrap();
    export_bundle_with(&stand(), &r.layout, &r.wiki, &r.out, "0.0.0", NOW, &records)
        .unwrap_or_else(|e| panic!("{e}"));
    files_of(&r.out)
}

fn frozen() -> BTreeMap<String, Vec<u8>> {
    files_of(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/export-before-suppression"),
    )
}

#[test]
fn an_export_with_no_tombstone_is_the_bundle_the_code_before_this_wrote_byte_for_byte() {
    let r = rig("frozen");
    threads(&r);
    let files = export(&r);
    let frozen = frozen();
    assert_eq!(
        files.keys().collect::<Vec<_>>(),
        frozen.keys().collect::<Vec<_>>(),
        "the bundle's files"
    );
    for (path, bytes) in &files {
        assert_eq!(
            String::from_utf8_lossy(bytes),
            String::from_utf8_lossy(frozen.get(path).unwrap()),
            "{path}"
        );
    }
    // The scenario itself is not empty: there is history, people and threads to compare.
    for path in [
        "history/p1.jsonl",
        "history/p2.jsonl",
        "users.yaml",
        &format!("extensions/inklkunn-knowledge/threads/{C1}.jsonl"),
    ] {
        assert!(!files.get(path).expect(path).is_empty(), "{path}");
    }
    let manifest = String::from_utf8(files.get("wiki-bundle.yaml").unwrap().clone()).unwrap();
    assert!(
        bundle_manifest::violations(&manifest).is_empty(),
        "{manifest}"
    );
}
