//! The export's history records (RFC 007 D-7; handoff I-T1; wiki-commons XFER-7): each line of `history/<page id>.jsonl` is a
//! wiki-commons history record with the page's full text, read through the one reader of a page's text at a block (RFC 006 J-1),
//! and a page since removed or renamed has its file too. The repository is a stand-in that holds files in memory and a history of
//! blocks with real patch messages and the text each block gave each page (`support/stand.rs`), so these run in both test runs
//! (no prikk); the tombstones are planted in the store directly. The real prikk's round trip is `export_roundtrip.rs`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use inklkunn_core::export::{
    ExportError, ExportNote, Exported, HistoryInputs, INDEX_WAIT_ROUNDS, KnowledgeRecords,
    export_bundle_with, knowledge_records,
};
use inklkunn_core::message::{Actor, BlockId, Message, Summary, SystemLabel};
use inklkunn_core::read::{ReadConfig, ReadService};
use inklkunn_store::access::Role;
use inklkunn_store::governance::Actor as GovernanceActor;
use inklkunn_store::{
    Account, AccountId, AccountStore, DerivedStore, DisplayName, GovernanceStore, HiddenItem,
    HiddenItemKind, HiddenParts, MemoryStore, NewWiki, ProposalId, ReasonCategory, RootLayout,
    Slug, TombstoneAct, TombstoneActId, TombstoneEvent, TombstoneEventId, WikiRecord, WikiStore,
};

#[path = "support/history_record.rs"]
mod history_record;
#[path = "support/json_schema.rs"]
mod json_schema;
#[path = "support/stand.rs"]
mod stand;

use stand::{Stand, block, block_of, op, rename};

const ANN: &str = "019a0000-0000-7000-8000-0000000000a1";
const BEN: &str = "019a0000-0000-7000-8000-0000000000a2";
const CYD: &str = "019a0000-0000-7000-8000-0000000000a3";
const MONA: &str = "019a0000-0000-7000-8000-0000000000a7";
const NOW: i64 = 1_700_001_000;

/// The id of the block at place `n` of the stand-in's log.
fn b(n: u64) -> String {
    format!("b10c{n:060x}")
}

fn proposal(n: u64) -> ProposalId {
    ProposalId::parse(&format!("019a0000-0000-7000-8000-000000000e{n:02x}")).unwrap()
}

fn account(id: &str, name: &str) -> Actor {
    Actor::Account {
        id: AccountId::parse(id).unwrap(),
        name: DisplayName::new(name).unwrap(),
    }
}

fn summary(s: &str) -> Summary {
    Summary::from_text(s).unwrap()
}

/// An accepted proposal of `who`, accepted by Ben.
fn by(n: u64, words: &str, who: (&str, &str)) -> Message {
    Message::proposal(
        summary(words),
        proposal(n),
        account(who.0, who.1),
        account(BEN, "Ben"),
        BlockId::new(&b(n.saturating_sub(1))).unwrap(),
    )
    .unwrap()
}

/// The undo of an earlier change, accepted by Ben.
fn undo(n: u64, words: &str) -> Message {
    Message::rollback(
        summary(words),
        proposal(n),
        account(CYD, "Cyd"),
        account(BEN, "Ben"),
        BlockId::new(&b(n.saturating_sub(1))).unwrap(),
    )
    .unwrap()
}

fn system(words: &str) -> Message {
    Message::system(summary(words), SystemLabel::inklkunn())
}

/// The text of the harbour page after each change, each with a word of its own that a grep of a bundle finds.
fn harbour(title: &str, word: &str) -> String {
    format!("---\ntitle: {title}\nid: harbour-1\n---\n\nThe harbour opens at dawn. {word}\n")
}

const WORDS_0: &str = "WORDS-OF-ZERO";
const WORDS_1: &str = "WORDS-OF-ONE-PRIVATE";
const WORDS_2: &str = "WORDS-OF-TWO";
const WORDS_3: &str = "WORDS-OF-THREE";

const ANCHOR: &str = "---\ntitle: Anchor\nid: anchor-1\n---\n\nWritten by block 1.\n";

/// A page's life: made with the wiki (with another page that stays, and an attachment), edited twice, moved and edited at once,
/// and removed: five records in all.
///
/// ```text
/// b(0) the service creates Harbour, Anchor and an attachment     create   WORDS-OF-ZERO
/// b(1) Ann edits Harbour                                         edit     WORDS-OF-ONE-PRIVATE
/// b(2) Cyd edits Harbour                                         edit     WORDS-OF-TWO
/// b(3) Ann moves Harbour to Port and edits it                    rename   WORDS-OF-THREE
/// b(4) Cyd removes Port                                          delete
/// ```
fn life() -> Stand {
    let blocks = vec![
        block(
            &b(0),
            1,
            "a0",
            &system("Create the wiki"),
            &[
                ("add", "pages/Harbour.md"),
                ("add", "pages/Anchor.md"),
                ("add", "attachments/map.png"),
            ],
        ),
        block(
            &b(1),
            2,
            "a1",
            &by(1, "Ann adds the tide table", (ANN, "Ann")),
            &[("modify", "pages/Harbour.md")],
        ),
        block(
            &b(2),
            3,
            "a2",
            &by(2, "Cyd corrects the tide table", (CYD, "Cyd")),
            &[("modify", "pages/Harbour.md")],
        ),
        block_of(
            &b(3),
            4,
            "a3",
            &by(3, "Ann moves the harbour to Port", (ANN, "Ann")),
            vec![
                rename("pages/Harbour.md", "pages/Port.md"),
                op("modify", "pages/Port.md"),
            ],
        ),
        block(
            &b(4),
            5,
            "a4",
            &by(4, "Cyd removes the page", (CYD, "Cyd")),
            &[("delete", "pages/Port.md")],
        ),
    ];
    let files = BTreeMap::from([
        ("pages/Anchor.md".to_owned(), ANCHOR.to_owned()),
        ("attachments/map.png".to_owned(), "PNG".to_owned()),
    ]);
    Stand::new(files, blocks)
        .giving(&b(0), "pages/Harbour.md", &harbour("Harbour", WORDS_0))
        .giving(&b(0), "pages/Anchor.md", ANCHOR)
        .giving(&b(1), "pages/Harbour.md", &harbour("Harbour", WORDS_1))
        .giving(&b(2), "pages/Harbour.md", &harbour("Harbour", WORDS_2))
        .giving(&b(3), "pages/Port.md", &harbour("Port", WORDS_3))
}

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Rig {
    _dir: Dir,
    layout: RootLayout,
    store: Arc<MemoryStore>,
    wiki: WikiRecord,
    out: PathBuf,
}

fn rig(tag: &str) -> Rig {
    let path = std::env::temp_dir().join(format!("inklkunn-xh-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    let store = Arc::new(MemoryStore::new());
    let wiki = store
        .create(NewWiki::new(
            Slug::new("notes").unwrap(),
            DisplayName::new("Notes").unwrap(),
        ))
        .unwrap();
    for (id, name) in [
        (ANN, "Ann"),
        (BEN, "Ben"),
        (CYD, "Cyd"),
        (MONA, "Mona Moderator"),
    ] {
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
        layout: RootLayout::new(path.join("root")),
        out: path.join("out"),
        _dir: Dir(path),
        store,
        wiki,
    }
}

/// Moderation hides these parts of the change at block `at`, for `reason`.
fn hide(r: &Rig, at: &str, parts: &[&str]) -> TombstoneEvent {
    let event = TombstoneEvent {
        id: TombstoneEventId::new(),
        wiki: r.wiki.id.clone(),
        at: 1_700_000_500,
        act: TombstoneAct::Hide,
        act_id: TombstoneActId::new(),
        item: HiddenItem {
            kind: HiddenItemKind::PageRevision,
            id: at.to_owned(),
            reference: None,
        },
        parts: HiddenParts {
            content: parts.contains(&"content"),
            summary: parts.contains(&"summary"),
            author: parts.contains(&"author"),
        },
        category: Some(ReasonCategory::PersonalData),
        note: Some("a note for the moderators".to_owned()),
        affected: None,
        by: GovernanceActor {
            account: AccountId::parse(MONA).unwrap(),
            rung: Role::Maintainer,
        },
        policy_block: None,
        of_event: None,
        appeal: None,
        declined_transition: None,
    };
    r.store.record_tombstone_event(&event).unwrap();
    event
}

/// The one reader of a page's text at a block, over the stand-in and the rig's store: what the command line gives the export.
fn reads_of(r: &Rig, stand: &Stand) -> ReadService {
    ReadService::new(
        Arc::new(stand.clone()),
        r.store.clone(),
        r.store.clone(),
        r.layout.clone(),
        ReadConfig::default(),
        None,
    )
    .with_tombstones(Arc::new(inklkunn_core::tombstone::Tombstones::new(
        r.store.clone(),
        r.store.clone(),
    )))
}

/// The wiki exported as the command line exports it, to `<tmp>/<name>`: what it said, and what it made (or why not).
fn try_export(
    r: &Rig,
    stand: &Stand,
    name: &str,
) -> (Vec<ExportNote>, Result<Exported, ExportError>) {
    let records = knowledge_records(
        &*r.store, &*r.store, &*r.store, &*r.store, &*r.store, &r.wiki.id,
    )
    .unwrap();
    let reads = reads_of(r, stand);
    let said = RefCell::new(Vec::new());
    let mut say = |note: ExportNote| said.borrow_mut().push(note);
    let mut inputs = HistoryInputs::new(&reads, &*r.store).saying(&mut say);
    let result = export_bundle_with(
        stand,
        &r.layout,
        &r.wiki,
        &r.out.with_file_name(name),
        "0.0.0",
        NOW,
        &records,
        &mut inputs,
    );
    (said.into_inner(), result)
}

fn export(r: &Rig, stand: &Stand, name: &str) -> BTreeMap<String, Vec<u8>> {
    let (_, result) = try_export(r, stand, name);
    result.unwrap_or_else(|e| panic!("{e}"));
    files_of(&r.out.with_file_name(name))
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
            .unwrap_or_else(|| panic!("the bundle has no `{path}`: {:?}", files.keys()))
            .clone(),
    )
    .unwrap()
}

/// The records of a history file.
fn records(files: &BTreeMap<String, Vec<u8>>, id: &str) -> Vec<serde_json::Value> {
    text(files, &format!("history/{id}.jsonl"))
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn at<'a>(v: &'a serde_json::Value, pointer: &str) -> &'a serde_json::Value {
    v.pointer(pointer).unwrap_or(&serde_json::Value::Null)
}

fn has(v: &serde_json::Value, name: &str) -> bool {
    v.as_object().is_some_and(|o| o.contains_key(name))
}

fn everything(files: &BTreeMap<String, Vec<u8>>) -> String {
    files
        .iter()
        .map(|(path, bytes)| format!("{path}\n{}\n", String::from_utf8_lossy(bytes)))
        .collect()
}

/// Every history file of the bundle is wiki-commons' history records, in order, with the digests of their texts.
fn every_file_is_history_records(files: &BTreeMap<String, Vec<u8>>) {
    let mut seen = 0;
    for (path, bytes) in files.iter().filter(|(p, _)| p.starts_with("history/")) {
        let lines: Vec<serde_json::Value> = String::from_utf8_lossy(bytes)
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let problems = history_record::file_violations(&lines);
        assert!(problems.is_empty(), "{path}: {problems:#?}");
        seen += lines.len();
    }
    assert!(seen > 0, "no history record was checked");
}

// ------------------------------------------------------------------------------------------------ the records

#[test]
fn a_page_edited_renamed_and_removed_has_five_records_in_order_each_with_its_text_in_the_file_of_its_last_id()
 {
    let r = rig("life");
    let files = export(&r, &life(), "bundle");
    // One file by the page's id, though the page has no file at the tip; the page that stayed has its own.
    assert_eq!(
        files
            .keys()
            .filter(|k| k.starts_with("history/"))
            .collect::<Vec<_>>(),
        ["history/anchor-1.jsonl", "history/harbour-1.jsonl"]
    );
    let lines = records(&files, "harbour-1");
    assert_eq!(lines.len(), 5, "{lines:#?}");
    let types: Vec<&str> = lines
        .iter()
        .map(|l| at(l, "/type").as_str().unwrap())
        .collect();
    assert_eq!(types, ["create", "edit", "edit", "rename", "delete"]);
    // The revision is the block, and each record's parent is the one before it.
    let revs: Vec<String> = (0..5).map(b).collect();
    assert_eq!(
        lines
            .iter()
            .map(|l| at(l, "/rev").as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        revs
    );
    assert!(at(lines.first().unwrap(), "/parent").is_null());
    for (n, line) in lines.iter().enumerate().skip(1) {
        assert_eq!(
            at(line, "/parent").as_str(),
            Some(b((n - 1) as u64).as_str()),
            "{line}"
        );
    }
    // Each with the page's whole text as it was then (and a removal with none).
    let texts = [
        Some(harbour("Harbour", WORDS_0)),
        Some(harbour("Harbour", WORDS_1)),
        Some(harbour("Harbour", WORDS_2)),
        Some(harbour("Port", WORDS_3)),
        None,
    ];
    for (line, expected) in lines.iter().zip(&texts) {
        match expected {
            Some(expected) => {
                assert_eq!(
                    at(line, "/content").as_str(),
                    Some(expected.as_str()),
                    "{line}"
                );
                assert!(at(line, "/sha256").is_string(), "{line}");
            }
            None => assert!(!has(line, "content") && !has(line, "sha256"), "{line}"),
        }
    }
    // A rename says where from and to, as titles, and the paths are inklkunn's own.
    let moved = lines.get(3).unwrap();
    assert_eq!(at(moved, "/from"), "Harbour");
    assert_eq!(at(moved, "/to"), "Port");
    assert_eq!(at(moved, "/inklkunn/from_path"), "pages/Harbour.md");
    assert_eq!(at(moved, "/inklkunn/path"), "pages/Port.md");
    assert_eq!(at(lines.get(4).unwrap(), "/inklkunn/path"), "pages/Port.md");
    // Who: the proposer is the author, by the id `users.yaml` has; the people of the message are inklkunn's own facts.
    let edit = lines.get(1).unwrap();
    assert_eq!(at(edit, "/author/id"), ANN);
    assert_eq!(at(edit, "/author/name"), "Ann");
    assert_eq!(at(edit, "/summary"), "Ann adds the tide table");
    assert_eq!(at(edit, "/inklkunn/proposed_by/id"), ANN);
    assert_eq!(at(edit, "/inklkunn/accepted_by/id"), BEN);
    assert_eq!(at(edit, "/inklkunn/kind"), "proposal");
    assert_eq!(*at(edit, "/inklkunn/block"), b(1));
    assert_eq!(at(edit, "/inklkunn/seq"), 2);
    assert_eq!(at(edit, "/inklkunn/message_readable"), true);
    let created = lines.first().unwrap();
    assert_eq!(at(created, "/author/kind"), "bot");
    assert_eq!(at(created, "/inklkunn/kind"), "system");
    let users = text(&files, "users.yaml");
    for id in [ANN, BEN, CYD] {
        assert!(users.contains(id), "{users}");
    }
    // The manifest: what no page's file holds is the attachment alone, and the page since removed is not among the pages left out.
    let manifest = text(&files, "wiki-bundle.yaml");
    assert!(
        manifest.contains("  history_changes_without_a_page: 1\n"),
        "{manifest}"
    );
    assert!(
        manifest.contains("  histories_not_written: 0\n"),
        "{manifest}"
    );
    assert!(manifest.contains("  history: true\n"), "{manifest}");
    every_file_is_history_records(&files);
}

#[test]
fn every_record_has_the_schemas_required_members_and_types_and_its_files_order() {
    // The schema is wiki-commons' own (a copy; `HISTORY-RECORD-SOURCE.md`); a record that breaks it is named.
    let r = rig("shape");
    let files = export(&r, &life(), "bundle");
    every_file_is_history_records(&files);
    for line in records(&files, "harbour-1")
        .iter()
        .chain(&records(&files, "anchor-1"))
    {
        assert!(history_record::violations(line).is_empty(), "{line}");
        for member in ["rev", "at", "type", "parent", "inklkunn"] {
            assert!(has(line, member), "no `{member}` in {line}");
        }
    }
    // The check itself checks: a record without its `rev`, with a time that is no date, a digest that is no digest, and a parent
    // that is a number, is four violations.
    let bad = serde_json::json!({"at": "yesterday", "sha256": "abc", "parent": 3});
    let problems = history_record::violations(&bad);
    assert!(problems.len() >= 3, "{problems:?}");
    assert!(
        problems.iter().any(|p| p.contains("`rev` is required")),
        "{problems:?}"
    );
}

#[test]
fn a_hidden_revision_has_no_text_and_the_next_revisions_text_is_whole() {
    let r = rig("hidden");
    hide(&r, &b(1), &["content"]);
    let files = export(&r, &life(), "bundle");
    let lines = records(&files, "harbour-1");
    assert_eq!(lines.len(), 5);
    let hidden = lines.get(1).unwrap();
    assert_eq!(*at(hidden, "/rev"), b(1));
    assert_eq!(at(hidden, "/suppressed"), &serde_json::json!(["content"]));
    assert_eq!(at(hidden, "/reasons/content"), "personal-data");
    assert!(
        !has(hidden, "content") && !has(hidden, "sha256"),
        "{hidden}"
    );
    // Nothing else of it is lost: its place, its people and its words.
    assert_eq!(at(hidden, "/type"), "edit");
    assert_eq!(*at(hidden, "/parent"), b(0));
    assert_eq!(at(hidden, "/summary"), "Ann adds the tide table");
    // The revision after it is whole: its own text is its own, not the hidden one's.
    let next = lines.get(2).unwrap();
    assert_eq!(
        at(next, "/content").as_str(),
        Some(harbour("Harbour", WORDS_2).as_str())
    );
    assert!(!has(next, "suppressed"));
    // The words of the hidden text are in no file of the bundle.
    assert!(!everything(&files).contains(WORDS_1));
    assert!(everything(&files).contains(WORDS_0) && everything(&files).contains(WORDS_2));
    every_file_is_history_records(&files);
    let manifest = text(&files, "wiki-bundle.yaml");
    assert!(
        manifest.contains("  tombstones:\n    entries: 1\n"),
        "{manifest}"
    );
}

/// The wiki of a page whose text moves without being written: b(0) writes it, b(1) hides it, b(2) only moves it, b(3) edits it.
fn moved_only() -> Stand {
    let blocks = vec![
        block(
            &b(0),
            1,
            "a0",
            &system("Create the wiki"),
            &[("add", "pages/Harbour.md")],
        ),
        block(
            &b(1),
            2,
            "a1",
            &by(1, "Ann writes it", (ANN, "Ann")),
            &[("modify", "pages/Harbour.md")],
        ),
        block_of(
            &b(2),
            3,
            "a2",
            &by(
                2,
                "Cyd moves it and says nothing of what it holds",
                (CYD, "Cyd"),
            ),
            vec![rename("pages/Harbour.md", "pages/Port.md")],
        ),
        block(
            &b(3),
            4,
            "a3",
            &by(3, "Ann writes it again", (ANN, "Ann")),
            &[("modify", "pages/Port.md")],
        ),
    ];
    let tip = harbour("Port", WORDS_3);
    Stand::new(
        BTreeMap::from([("pages/Port.md".to_owned(), tip.clone())]),
        blocks,
    )
    .giving(&b(0), "pages/Harbour.md", &harbour("Harbour", WORDS_0))
    .giving(&b(1), "pages/Harbour.md", &harbour("Harbour", WORDS_1))
    .giving(&b(3), "pages/Port.md", &tip)
}

#[test]
fn a_text_that_a_hidden_change_wrote_is_hidden_where_a_later_change_only_moved_the_page() {
    // The hide names the change that wrote the words; the change that only moved the page wrote nothing, so its own record is not
    // among the hidden ones. The text it carries is the hidden change's, and the one reader says so: the record has none.
    let r = rig("moved");
    hide(&r, &b(1), &["content"]);
    let files = export(&r, &moved_only(), "bundle");
    let lines = records(&files, "harbour-1");
    assert_eq!(
        lines
            .iter()
            .map(|l| at(l, "/type").as_str().unwrap())
            .collect::<Vec<_>>(),
        ["create", "edit", "rename", "edit"]
    );
    let moved = lines.get(2).unwrap();
    assert_eq!(
        at(moved, "/suppressed"),
        &serde_json::json!(["content"]),
        "{moved}"
    );
    assert_eq!(at(moved, "/reasons/content"), "personal-data");
    assert!(!has(moved, "content") && !has(moved, "sha256"), "{moved}");
    assert_eq!(at(moved, "/from"), "Harbour");
    assert!(
        !everything(&files).contains(WORDS_1),
        "the hidden words are in the bundle"
    );
    // The change after it wrote the page again, and its text is its own.
    assert_eq!(
        at(lines.get(3).unwrap(), "/content").as_str(),
        Some(harbour("Port", WORDS_3).as_str())
    );
    // Nothing hidden: the moved page carries the text it had.
    let open = rig("moved-open");
    let files = export(&open, &moved_only(), "bundle");
    assert_eq!(
        at(records(&files, "harbour-1").get(2).unwrap(), "/content").as_str(),
        Some(harbour("Harbour", WORDS_1).as_str())
    );
}

#[test]
fn the_same_tip_is_the_same_bytes_twice() {
    let r = rig("twice");
    let stand = life();
    hide(&r, &b(2), &["summary"]);
    let one = export(&r, &stand, "one");
    let two = export(&r, &stand, "two");
    assert_eq!(
        one.keys().collect::<Vec<_>>(),
        two.keys().collect::<Vec<_>>()
    );
    for (path, bytes) in &one {
        assert_eq!(
            String::from_utf8_lossy(bytes),
            String::from_utf8_lossy(two.get(path).unwrap()),
            "{path}"
        );
    }
    // And through another reader and another store of the same history: the order is the log's, not an accident of a map.
    let other = rig("twice-other");
    hide(&other, &b(2), &["summary"]);
    let three = export(&other, &stand, "three");
    assert_eq!(
        text(&one, "history/harbour-1.jsonl"),
        text(&three, "history/harbour-1.jsonl")
    );
}

#[test]
fn the_time_of_a_record_is_when_the_block_was_first_seen_and_never_runs_backwards() {
    let r = rig("times");
    // The service saw b(0) first, then b(2) (a page of the changes list asked for the newer ones), and b(1) only afterwards.
    for (n, seen) in [(0, 100), (2, 200), (1, 300), (3, 400), (4, 500)] {
        r.store.first_seen(&r.wiki.id, &[b(n)], seen).unwrap();
    }
    let files = export(&r, &life(), "bundle");
    let times: Vec<String> = records(&files, "harbour-1")
        .iter()
        .map(|l| at(l, "/at").as_str().unwrap().to_owned())
        .collect();
    let seconds = |n: i64| inklkunn_store::record::rfc3339(n);
    // b(1) was first seen after b(2), so it was made no later than b(2) was first seen.
    assert_eq!(
        times,
        [
            seconds(100),
            seconds(200),
            seconds(200),
            seconds(400),
            seconds(500)
        ]
    );
    for line in records(&files, "harbour-1") {
        assert_eq!(at(&line, "/inklkunn/at_from"), "first-seen");
    }
    // A block no surface has asked for is recorded as seen when the export ran, and stays so.
    let fresh = rig("times-fresh");
    let files = export(&fresh, &life(), "bundle");
    assert_eq!(
        at(records(&files, "harbour-1").first().unwrap(), "/at"),
        &serde_json::json!(inklkunn_store::record::rfc3339(NOW))
    );
}

#[test]
fn an_undo_is_a_revert_and_a_page_that_comes_back_with_its_id_is_one_file() {
    let blocks = vec![
        block(
            &b(0),
            1,
            "a0",
            &system("Create the wiki"),
            &[("add", "pages/Harbour.md")],
        ),
        block(
            &b(1),
            2,
            "a1",
            &by(1, "Ann removes it", (ANN, "Ann")),
            &[("delete", "pages/Harbour.md")],
        ),
        block(
            &b(2),
            3,
            "a2",
            &undo(2, "Undo: Ann removes it"),
            &[("add", "pages/Harbour.md")],
        ),
        block(
            &b(3),
            4,
            "a3",
            &by(3, "Cyd corrects it", (CYD, "Cyd")),
            &[("modify", "pages/Harbour.md")],
        ),
    ];
    let tip = harbour("Harbour", WORDS_3);
    let stand = Stand::new(
        BTreeMap::from([("pages/Harbour.md".to_owned(), tip.clone())]),
        blocks,
    )
    .giving(&b(0), "pages/Harbour.md", &harbour("Harbour", WORDS_0))
    .giving(&b(2), "pages/Harbour.md", &harbour("Harbour", WORDS_0))
    .giving(&b(3), "pages/Harbour.md", &tip);
    let r = rig("revert");
    let files = export(&r, &stand, "bundle");
    let lines = records(&files, "harbour-1");
    assert_eq!(
        lines
            .iter()
            .map(|l| at(l, "/type").as_str().unwrap())
            .collect::<Vec<_>>(),
        ["create", "delete", "revert", "edit"]
    );
    assert_eq!(at(lines.get(2).unwrap(), "/inklkunn/kind"), "rollback");
    assert_eq!(*at(lines.get(2).unwrap(), "/parent"), b(1));
    assert_eq!(
        at(lines.get(2).unwrap(), "/content").as_str(),
        Some(harbour("Harbour", WORDS_0).as_str())
    );
    assert_eq!(
        files.keys().filter(|k| k.starts_with("history/")).count(),
        1,
        "{:?}",
        files.keys().collect::<Vec<_>>()
    );
    every_file_is_history_records(&files);
}

#[test]
fn a_page_none_of_whose_texts_can_name_it_is_named_by_its_path_and_its_hidden_text_is_not_in_it() {
    // The only text the page had is hidden, so no id of it is known (XFER-7: *or by its path when no identifier exists*): its
    // file is named by its path, `~` first (which no id can start with), and the hidden text is in no byte of the bundle.
    let blocks = vec![
        block(
            &b(0),
            1,
            "a0",
            &system("Create the wiki"),
            &[("add", "pages/Harbour.md"), ("add", "pages/Anchor.md")],
        ),
        block(
            &b(1),
            2,
            "a1",
            &by(1, "Ann removes the harbour", (ANN, "Ann")),
            &[("delete", "pages/Harbour.md")],
        ),
    ];
    let stand = Stand::new(
        BTreeMap::from([("pages/Anchor.md".to_owned(), ANCHOR.to_owned())]),
        blocks,
    )
    .giving(&b(0), "pages/Anchor.md", ANCHOR)
    .giving(&b(0), "pages/Harbour.md", &harbour("Harbour", WORDS_1));
    let r = rig("nameless");
    // b(0) wrote both pages: hiding it hides both, so neither has a text that names it, and the one at the tip is named by the tip.
    hide(&r, &b(0), &["content"]);
    let files = export(&r, &stand, "bundle");
    assert!(
        !files.keys().any(|k| k == "history/harbour-1.jsonl"),
        "{:?}",
        files.keys().collect::<Vec<_>>()
    );
    assert!(!everything(&files).contains(WORDS_1));
    // Its file is named by its path, and says what happened to it without the text.
    let harbour_records = records(&files, "~%48arbour");
    assert_eq!(
        harbour_records.len(),
        2,
        "{:?}",
        files.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        at(harbour_records.first().unwrap(), "/suppressed"),
        &serde_json::json!(["content"])
    );
    assert_eq!(
        at(harbour_records.last().unwrap(), "/type"),
        &serde_json::json!("delete")
    );
    let manifest = text(&files, "wiki-bundle.yaml");
    assert!(
        manifest.contains("  histories_not_written: 0\n"),
        "{manifest}"
    );
    // The page at the tip has its file, with the hidden record marked.
    let anchor = records(&files, "anchor-1");
    assert_eq!(anchor.len(), 1);
    assert_eq!(
        at(anchor.first().unwrap(), "/suppressed"),
        &serde_json::json!(["content"])
    );
}

#[test]
fn the_files_of_pages_with_no_id_are_named_so_that_a_file_system_that_ignores_case_keeps_them_apart() {
    // `Ab` and `aB` are two pages; a bundle unpacked on a file system that does not tell them apart must keep two files (X-5).
    let nameless = |title: &str, word: &str| format!("---\ntitle: {title}\n---\n\n{word}\n");
    let blocks = vec![block(
        &b(0),
        1,
        "a0",
        &system("Create the wiki"),
        &[("add", "pages/Ab.md"), ("add", "pages/aB.md")],
    )];
    let stand = Stand::new(
        BTreeMap::from([
            ("pages/Ab.md".to_owned(), nameless("Ab", "one")),
            ("pages/aB.md".to_owned(), nameless("aB", "two")),
        ]),
        blocks,
    )
    .giving(&b(0), "pages/Ab.md", &nameless("Ab", "one"))
    .giving(&b(0), "pages/aB.md", &nameless("aB", "two"));
    let r = rig("case");
    let files = export(&r, &stand, "bundle");
    let names: Vec<&String> = files.keys().filter(|k| k.starts_with("history/")).collect();
    assert_eq!(
        names,
        ["history/~%41b.jsonl", "history/~a%42.jsonl"],
        "no upper-case letter is written as one"
    );
    let folded: std::collections::BTreeSet<String> =
        names.iter().map(|n| n.to_ascii_lowercase()).collect();
    assert_eq!(folded.len(), names.len(), "{names:?}");
    assert_eq!(records(&files, "~%41b").len(), 1);
    assert_eq!(records(&files, "~a%42").len(), 1);
}

#[test]
fn a_text_that_cannot_be_checked_yet_is_waited_for_and_then_the_export_fails_and_writes_nothing() {
    // A block of a kind whose effect on the files its operations do not list (a merge) is opaque: while a change of the wiki has
    // its content hidden, a text that passes through it cannot be attributed, and no waiting settles that.
    let mut stand = life();
    if let Some((merge, _)) = stand.blocks.get_mut(1) {
        merge.kind = "Merge".to_owned();
    }
    let r = rig("waiting");
    hide(&r, &b(2), &["content"]);
    let (said, result) = try_export(&r, &stand, "bundle");
    let err = result.unwrap_err();
    assert!(
        matches!(err, ExportError::IndexNotSettled { rounds } if rounds == INDEX_WAIT_ROUNDS),
        "{err}"
    );
    let waits: Vec<usize> = said
        .iter()
        .filter_map(|n| match n {
            ExportNote::Waiting { round, rounds } => {
                assert_eq!(*rounds, INDEX_WAIT_ROUNDS);
                Some(*round)
            }
            ExportNote::Revisions { .. } => None,
        })
        .collect();
    assert_eq!(
        waits,
        (1..=INDEX_WAIT_ROUNDS).collect::<Vec<_>>(),
        "it said so, round by round"
    );
    assert!(
        said.iter().any(|n| matches!(
            n,
            ExportNote::Revisions {
                revisions: 6,
                pages: 2
            }
        )),
        "it said how many revisions it reads: {said:?}"
    );
    let sentence = err.to_string();
    assert!(
        sentence.contains("not built yet") && sentence.contains("none was written"),
        "{sentence}"
    );
    // Nothing of it is left: no bundle, no partial one beside it.
    assert!(!r.out.with_file_name("bundle").exists());
    let beside: Vec<String> = std::fs::read_dir(r.out.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(!beside.iter().any(|n| n.contains("bundle")), "{beside:?}");

    // The same wiki with nothing hidden has nothing to wait for, whatever the blocks are.
    let calm = rig("waiting-calm");
    let (said, result) = try_export(&calm, &stand, "bundle");
    result.unwrap_or_else(|e| panic!("{e}"));
    assert!(
        said.iter()
            .all(|n| matches!(n, ExportNote::Revisions { .. })),
        "{said:?}"
    );
}

#[test]
fn an_export_that_is_not_given_the_services_records_still_has_its_histories() {
    // The history records need no knowledge records: the default records (no threads, no views, nothing hidden) are a bundle too.
    let r = rig("plain");
    let stand = life();
    let reads = reads_of(&r, &stand);
    let dest = r.out.with_file_name("plain");
    export_bundle_with(
        &stand,
        &r.layout,
        &r.wiki,
        &dest,
        "0.0.0",
        NOW,
        &KnowledgeRecords::default(),
        &mut HistoryInputs::new(&reads, &*r.store),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(records(&files_of(&dest), "harbour-1").len(), 5);
}
