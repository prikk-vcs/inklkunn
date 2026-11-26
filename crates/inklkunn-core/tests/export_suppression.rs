//! The export of suppressed content (RFC 006 D-13, D-8's last bullet; acceptance criterion 13; G-T4b).
//!
//! The repository is a stand-in that holds files in memory and a history of several blocks with real patch messages, so
//! these run in both test runs (no prikk); the tombstones are planted in the store directly. **The frozen bundle**
//! (`tests/fixtures/export-before-suppression/`) is what the export writes for [`scenario`] when nothing is hidden: an export with
//! no tombstones must write exactly those bytes. It was what the export wrote before suppression existed until I-T1 (RFC 007 D-7),
//! which changed the shape of every history line (wiki-commons history records with the pages' texts, and the pages since removed
//! have their files); the fixture was written again then, from the code, and the history lines of 0.4.0 are no longer in it.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use inklkunn_core::export::{HistoryInputs, Suppressions, export_bundle_with, knowledge_records};
use inklkunn_core::import::{Existing, plan_folder};
use inklkunn_core::message::{Actor, BlockId, Message, Summary, SystemLabel};
use inklkunn_core::read::{ReadConfig, ReadService};
use inklkunn_prikk::storage::{Change, HistoryBlock};
use inklkunn_store::AcceptanceId;
use inklkunn_store::acceptance::{
    AcceptanceOutcome, AcceptanceRecord, AcceptanceResult, AcceptanceStore, IntentRecord,
};
use inklkunn_store::access::Role;
use inklkunn_store::governance::Actor as GovernanceActor;
use inklkunn_store::{
    Account, AccountId, AccountStore, Base, ClaimCommentRecord, ClaimThreadStore, CommentId,
    DisplayName, FileChange, GovernanceStore, HiddenItem, HiddenItemKind, HiddenParts, IntentId,
    MemoryStore, NewWiki, ProposalChange, ProposalId, ProposalOp, ProposalRecord, ProposalState,
    ProposalStore, ProposalTransition, ReasonCategory, RevisionContent, RevisionRecord, RootLayout,
    Slug, TombstoneAct, TombstoneActId, TombstoneEvent, TombstoneEventId, WikiRecord, WikiStore,
};
use sha2::{Digest, Sha256};

#[path = "support/bundle_manifest.rs"]
mod bundle_manifest;
#[path = "support/history_record.rs"]
mod history_record;
#[path = "support/json_schema.rs"]
mod json_schema;
#[path = "support/stand.rs"]
mod stand;

use stand::{Stand, block};

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

fn account(id: &str, name: &str) -> Actor {
    Actor::Account {
        id: AccountId::parse(id).expect("an account id"),
        name: DisplayName::new(name).expect("a name"),
    }
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
        block(B0, 1, "a0", &b0, &[("add", "pages/Gone.md")]),
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
    Stand::new(files(), history()).identified("pages/Gone.md", "gone-1")
}

struct Rig {
    _dir: Dir,
    layout: RootLayout,
    store: Arc<MemoryStore>,
    wiki: WikiRecord,
    out: PathBuf,
}

fn rig(tag: &str) -> Rig {
    let d = dir(tag);
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
        // The moderator: in no bundle.
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

/// The one reader of a page's text at a block, over the stand-in and the rig's store (where the tombstones are planted): what the
/// command line gives the export.
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

/// Export the scenario, as the command line does: the records read from the store, then the bundle.
fn export(r: &Rig) -> BTreeMap<String, Vec<u8>> {
    let records = knowledge_records(
        &*r.store, &*r.store, &*r.store, &*r.store, &*r.store, &r.wiki.id,
    )
    .unwrap();
    let stand = stand();
    let reads = reads_of(r, &stand);
    export_bundle_with(
        &stand,
        &r.layout,
        &r.wiki,
        &r.out,
        "0.0.0",
        NOW,
        &records,
        &mut HistoryInputs::new(&reads, &*r.store),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    files_of(&r.out)
}

fn frozen() -> BTreeMap<String, Vec<u8>> {
    files_of(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/export-before-suppression"),
    )
}

#[test]
fn an_export_with_no_tombstone_is_the_frozen_bundle_byte_for_byte() {
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

// ------------------------------------------------------------------------------------------------ the extended wiki

const ZED: &str = "019a0000-0000-7000-8000-0000000000a4";
const YUKI: &str = "019a0000-0000-7000-8000-0000000000a5";
const QUILL: &str = "019a0000-0000-7000-8000-0000000000a6";
const MONA: &str = "019a0000-0000-7000-8000-0000000000a7";
const P4: &str = "019a0000-0000-7000-8000-000000000e04";
const P5: &str = "019a0000-0000-7000-8000-000000000e05";
const P6: &str = "019a0000-0000-7000-8000-000000000e06";
const B4: &str = "b10c000000000000000000000000000000000000000000000000000000000004";
const B5: &str = "b10c000000000000000000000000000000000000000000000000000000000005";
const B6: &str = "b10c000000000000000000000000000000000000000000000000000000000006";
const SUMMARY_B4: &str = "Zed rewrites both pages in private";
const SUMMARY_B5: &str = "Yuki starts the glossary";
const SUMMARY_B6: &str = "Yuki finishes the glossary";
const QUILL_SAYS: &str = "Quill's private words about the harbour";
const ANN_REPLIES: &str = "Ann replies in the open";

/// The history of [`history`] and three blocks more: Zed's change to both pages (a person who appears nowhere else), and two
/// changes of Yuki's (a person who appears in two places).
fn history_extended() -> Vec<(HistoryBlock, Change)> {
    let mut blocks = history();
    let by = |summary_text: &str, proposal: &str, who: (&str, &str), base_block: &str| {
        Message::proposal(
            summary(summary_text),
            ProposalId::parse(proposal).expect("a proposal id"),
            account(who.0, who.1),
            account(BEN, "Ben"),
            base(base_block),
        )
        .expect("a message")
    };
    let zed = by(SUMMARY_B4, P4, (ZED, "Zed Hidden"), B3);
    let yuki1 = by(SUMMARY_B5, P5, (YUKI, "Yuki Visible"), B4);
    let yuki2 = by(SUMMARY_B6, P6, (YUKI, "Yuki Visible"), B5);
    blocks.push(block(
        B4,
        5,
        "a4",
        &zed,
        &[("modify", "pages/One.md"), ("modify", "pages/Two.md")],
    ));
    blocks.push(block(B5, 6, "a5", &yuki1, &[("modify", "pages/Two.md")]));
    blocks.push(block(B6, 7, "a6", &yuki2, &[("modify", "pages/One.md")]));
    blocks
}

fn stand_extended() -> Stand {
    Stand::new(files(), history_extended()).identified("pages/Gone.md", "gone-1")
}

/// The people of the extended wiki, and two comments more on the second claim: Quill's, who writes nowhere else, and Ann's.
fn people_and_comments(r: &Rig) {
    for (id, name) in [
        (ZED, "Zed Hidden"),
        (YUKI, "Yuki Visible"),
        (QUILL, "Quill Hidden"),
    ] {
        r.store
            .add_account(&Account {
                id: AccountId::parse(id).unwrap(),
                display_name: DisplayName::new(name).unwrap(),
                local: None,
                created_at: 1,
                disabled: false,
            })
            .unwrap();
    }
    threads(r);
    for (n, who, body, at) in [
        (5, QUILL, QUILL_SAYS, 1_700_000_300),
        (6, ANN, ANN_REPLIES, 1_700_000_360),
    ] {
        r.store
            .add_claim_comment(&ClaimCommentRecord {
                id: comment_id(n),
                wiki: r.wiki.id.clone(),
                claim: C2.to_owned(),
                author: AccountId::parse(who).unwrap(),
                parent: None,
                body: body.to_owned(),
                created_at: at,
            })
            .unwrap();
    }
}

fn extended(tag: &str) -> Rig {
    let r = rig(tag);
    people_and_comments(&r);
    r
}

// ------------------------------------------------------------------------------------------------ planting tombstones

fn parts(names: &[&str]) -> HiddenParts {
    HiddenParts {
        content: names.contains(&"content"),
        summary: names.contains(&"summary"),
        author: names.contains(&"author"),
    }
}

fn plant(
    r: &Rig,
    kind: HiddenItemKind,
    id: &str,
    reference: Option<&str>,
    names: &[&str],
    reason: ReasonCategory,
) -> TombstoneEvent {
    let event = TombstoneEvent {
        id: TombstoneEventId::new(),
        wiki: r.wiki.id.clone(),
        at: 1_700_000_500,
        act: TombstoneAct::Hide,
        act_id: TombstoneActId::new(),
        item: HiddenItem {
            kind,
            id: id.to_owned(),
            reference: reference.map(str::to_owned),
        },
        parts: parts(names),
        category: Some(reason),
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

fn page_hide(r: &Rig, block_id: &str, names: &[&str]) -> TombstoneEvent {
    plant(
        r,
        HiddenItemKind::PageRevision,
        block_id,
        None,
        names,
        ReasonCategory::PersonalData,
    )
}

fn comment_hide(r: &Rig, n: u8, names: &[&str]) -> TombstoneEvent {
    plant(
        r,
        HiddenItemKind::ClaimComment,
        comment_id(n).as_str(),
        None,
        names,
        ReasonCategory::Harassment,
    )
}

fn unhide(r: &Rig, hide: &TombstoneEvent) {
    r.store
        .record_tombstone_event(&TombstoneEvent {
            id: TombstoneEventId::new(),
            at: hide.at + 60,
            act: TombstoneAct::Unhide,
            act_id: TombstoneActId::new(),
            category: None,
            note: None,
            of_event: Some(hide.id.clone()),
            ..hide.clone()
        })
        .unwrap();
}

/// A proposal that was accepted, and the block its acceptance sealed.
fn accepted(r: &Rig, proposal: &str, proposer: &str, block_id: &str) {
    let p = ProposalId::parse(proposal).unwrap();
    let author = AccountId::parse(proposer).unwrap();
    let record = ProposalRecord {
        id: p.clone(),
        wiki: r.wiki.id.clone(),
        view: "heads/main".to_owned(),
        state: ProposalState::Draft,
        author: author.clone(),
        summary: "A summary".to_owned(),
        sources: Vec::new(),
        superseded_by: None,
        created_at: 1,
        updated_at: 1,
        submitted_at: None,
        closed_at: None,
    };
    let content = RevisionContent {
        base: Base::new(&"1".repeat(64), &"2".repeat(64)).unwrap(),
        files: vec![FileChange {
            path: "pages/One.md".to_owned(),
            text: Some("text".to_owned()),
        }],
        renames: Vec::new(),
        minor: false,
        automated: false,
    };
    let step = |op, from, to| ProposalTransition {
        proposal: p.clone(),
        op,
        from,
        to,
        actor: author.clone(),
        reason: None,
        at: 1,
    };
    r.store
        .create_proposal(
            &record,
            &content,
            &step(ProposalOp::Create, None, ProposalState::Draft),
        )
        .unwrap();
    r.store
        .change_proposal(&ProposalChange {
            transition: step(
                ProposalOp::Submit,
                Some(ProposalState::Draft),
                ProposalState::HeldForReview,
            ),
            text: None,
            revision: Some(RevisionRecord {
                proposal: p.clone(),
                seq: 1,
                content,
                created_at: 1,
            }),
            superseded_by: None,
        })
        .unwrap();
    let acceptance = AcceptanceRecord {
        id: AcceptanceId::new(),
        proposal: p,
        revision: 1,
        wiki: r.wiki.id.clone(),
        view: "heads/main".to_owned(),
        acceptor: AccountId::parse(BEN).unwrap(),
        acceptor_role: Role::Maintainer,
        policy_version: "bootstrap-1".to_owned(),
        policy_block: None,
        hatch: false,
        proposer: author,
        digest: "0".repeat(64),
        bytes: 1,
        created_at: 1,
    };
    r.store.begin_acceptance(&acceptance).unwrap();
    r.store
        .record_intent(&IntentRecord {
            id: IntentId::new(),
            acceptance: acceptance.id.clone(),
            wiki: r.wiki.id.clone(),
            view: "heads/main".to_owned(),
            base_block: "a".repeat(64),
            message_digest: "b".repeat(64),
            message: "A summary\n\nKind: system\n".to_owned(),
            created_at: 1,
        })
        .unwrap();
    r.store
        .finish_acceptance(&AcceptanceResult {
            acceptance: acceptance.id,
            outcome: AcceptanceOutcome::Sealed {
                block_id: block_id.to_owned(),
                patch_id: "a3".to_owned(),
            },
            at: 2,
        })
        .unwrap();
}

// ------------------------------------------------------------------------------------------------ reading a bundle

/// The wiki exported as the command line exports it, into `<tmp>/<name>`.
fn export_as(r: &Rig, stand: &Stand, name: &str) -> BTreeMap<String, Vec<u8>> {
    let records = knowledge_records(
        &*r.store, &*r.store, &*r.store, &*r.store, &*r.store, &r.wiki.id,
    )
    .unwrap();
    let out = r.out.with_file_name(name);
    let reads = reads_of(r, stand);
    export_bundle_with(
        stand,
        &r.layout,
        &r.wiki,
        &out,
        "0.0.0",
        NOW,
        &records,
        &mut HistoryInputs::new(&reads, &*r.store),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    files_of(&out)
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

/// Every byte of every file of the bundle, as one text, to look for a word in.
fn everything(files: &BTreeMap<String, Vec<u8>>) -> String {
    files
        .iter()
        .map(|(path, bytes)| format!("{path}\n{}\n", String::from_utf8_lossy(bytes)))
        .collect()
}

fn json_lines(files: &BTreeMap<String, Vec<u8>>, path: &str) -> Vec<serde_json::Value> {
    text(files, path)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// The history line of `block_id` in the history file of the page `page`.
fn line_of(files: &BTreeMap<String, Vec<u8>>, page: &str, block_id: &str) -> serde_json::Value {
    json_lines(files, &format!("history/{page}.jsonl"))
        .into_iter()
        .find(|l| *at(l, "/rev") == block_id)
        .unwrap_or_else(|| panic!("no line of {block_id} in the history of {page}"))
}

/// The member at a JSON pointer, `null` when there is none (tests do not index).
fn at<'a>(v: &'a serde_json::Value, pointer: &str) -> &'a serde_json::Value {
    v.pointer(pointer).unwrap_or(&serde_json::Value::Null)
}

fn has_member(v: &serde_json::Value, name: &str) -> bool {
    v.as_object().is_some_and(|o| o.contains_key(name))
}

fn user_ids(files: &BTreeMap<String, Vec<u8>>) -> Vec<String> {
    text(files, "users.yaml")
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- id: ").map(str::to_owned))
        .collect()
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The manifest is valid against wiki-commons' schema and every file but the manifest and the sums is listed with its digest.
fn the_bundle_is_whole(files: &BTreeMap<String, Vec<u8>>) {
    let manifest = text(files, "wiki-bundle.yaml");
    let problems = bundle_manifest::violations(&manifest);
    assert!(problems.is_empty(), "{problems:#?}\n{manifest}");
    let sums: BTreeMap<String, String> = text(files, "sha256sums.txt")
        .lines()
        .map(|l| {
            let (sum, path) = l.split_once("  ").unwrap();
            (path.to_owned(), sum.to_owned())
        })
        .collect();
    assert_eq!(sums.len(), files.len() - 2, "{sums:?}");
    for (path, sum) in &sums {
        assert_eq!(&sha(files.get(path).unwrap()), sum, "{path}");
    }
}

// ------------------------------------------------------------------------------------------------ the tests

#[test]
fn a_hidden_page_revision_is_marked_with_what_is_hidden_and_why_and_omits_only_that() {
    for (names, left_out) in [
        (vec!["content"], vec!["content"]),
        (vec!["summary"], vec!["summary"]),
        (vec!["author"], vec!["author"]),
        (
            vec!["content", "summary", "author"],
            vec!["summary", "author", "content"],
        ),
    ] {
        let r = rig("revision");
        threads(&r);
        page_hide(&r, B3, &names);
        let files = export_as(&r, &stand(), "bundle");
        let line = line_of(&files, "p1", B3);
        // The marker, with the words of the store, and the line's identity, position and kind kept.
        assert_eq!(
            *at(&line, "/suppressed"),
            serde_json::json!(names),
            "{line}"
        );
        let reasons: serde_json::Map<String, serde_json::Value> = names
            .iter()
            .map(|n| ((*n).to_owned(), "personal-data".into()))
            .collect();
        assert_eq!(*at(&line, "/reasons"), serde_json::Value::Object(reasons));
        assert_eq!(*at(&line, "/rev"), B3);
        assert_eq!(*at(&line, "/inklkunn/block"), B3);
        assert_eq!(*at(&line, "/inklkunn/seq"), 4);
        assert_eq!(*at(&line, "/inklkunn/path"), "pages/One.md");
        assert_eq!(*at(&line, "/type"), "edit");
        assert_eq!(*at(&line, "/inklkunn/kind"), "proposal");
        // What is hidden is not in it, and what is not hidden still is.
        for member in ["summary", "author", "content"] {
            assert_eq!(
                has_member(&line, member),
                !left_out.contains(&member),
                "{names:?}: {member}: {line}"
            );
        }
        let all = everything(&files);
        assert_eq!(
            all.contains(SUMMARY_B3),
            !names.contains(&"summary"),
            "{names:?}"
        );
        // The line before it, and the other page's lines, are the lines they were.
        assert!(!has_member(&line_of(&files, "p1", B2), "suppressed"));
        assert!(!has_member(&line_of(&files, "p2", B1), "suppressed"));
        // The manifest says what the bundle carries, and the bundle is whole.
        let manifest = text(&files, "wiki-bundle.yaml");
        assert!(manifest.contains("  tombstones:\n    entries: 1\n    marked_by: suppressed\n    reasons_in: reasons\n    hidden_content: omitted\n"), "{manifest}");
        assert!(
            manifest.contains("count: 1, how:") && manifest.contains("hidden by moderation"),
            "{manifest}"
        );
        the_bundle_is_whole(&files);
    }
}

#[test]
fn a_hidden_thread_comment_loses_its_body_or_its_author_and_says_so() {
    for (names, body, author) in [
        (vec!["content"], false, true),
        (vec!["author"], true, false),
        (vec!["content", "author"], false, false),
    ] {
        let r = extended("thread");
        comment_hide(&r, 5, &names);
        let files = export_as(&r, &stand_extended(), "bundle");
        let thread = json_lines(
            &files,
            &format!("extensions/inklkunn-knowledge/threads/{C2}.jsonl"),
        );
        let hidden = thread
            .iter()
            .find(|l| *at(l, "/id") == comment_id(5).as_str())
            .unwrap();
        assert_eq!(
            *at(hidden, "/suppressed"),
            serde_json::json!(names),
            "{hidden}"
        );
        assert_eq!(has_member(hidden, "body"), body, "{hidden}");
        assert_eq!(has_member(hidden, "author"), author, "{hidden}");
        // Its place in the thread is kept: its id, its time and the comment it answers.
        assert_eq!(*at(hidden, "/at"), "2023-11-14T22:18:20Z");
        assert!(has_member(hidden, "parent"));
        // The comments beside it are as they were.
        let other = thread
            .iter()
            .find(|l| *at(l, "/id") == comment_id(6).as_str())
            .unwrap();
        assert!(!has_member(other, "suppressed") && !has_member(other, "reasons"));
        assert_eq!(*at(other, "/body"), format!("```text\n{ANN_REPLIES}\n```"));
        let all = everything(&files);
        assert_eq!(all.contains(QUILL_SAYS), body, "{names:?}");
        assert_eq!(all.contains("Quill Hidden"), author, "{names:?}");
        assert_eq!(all.contains(QUILL), author, "{names:?}");
        the_bundle_is_whole(&files);
    }
}

#[test]
fn nothing_hidden_or_hiding_a_person_is_in_any_file_and_a_person_is_listed_only_for_what_is_not_hidden()
 {
    let r = extended("leak");
    // Zed: the one change he is in (the people of it, its summary, its text); Quill: the one comment; Yuki: one of her two.
    page_hide(&r, B4, &["content", "summary", "author"]);
    page_hide(&r, B5, &["author"]);
    comment_hide(&r, 5, &["content", "author"]);
    let files = export_as(&r, &stand_extended(), "bundle");
    let all = everything(&files);
    for word in [
        "Zed Hidden",
        ZED,
        SUMMARY_B4,
        "Quill Hidden",
        QUILL,
        QUILL_SAYS,
        // The moderator is no person of the bundle either.
        "Mona Moderator",
        MONA,
        "a note for the moderators",
    ] {
        assert!(!all.contains(word), "`{word}` is in the bundle");
    }
    // Who is listed: the ones with something visible. Yuki is hidden on B5 and shown on B6.
    let listed = user_ids(&files);
    assert_eq!(
        listed,
        [ANN, BEN, CYD, YUKI],
        "{}",
        text(&files, "users.yaml")
    );
    assert!(text(&files, "users.yaml").contains("names: [\"Yuki Visible\"]"));
    assert!(all.contains("Yuki Visible") && all.contains(SUMMARY_B5) && all.contains(SUMMARY_B6));
    // Her hidden change names nobody; her visible one names her.
    assert!(!has_member(&line_of(&files, "p2", B5), "author"));
    assert_eq!(
        *at(&line_of(&files, "p1", B6), "/author/name"),
        "Yuki Visible"
    );
    // What is left of the thread beside Quill's comment.
    assert!(all.contains(ANN_REPLIES));
    the_bundle_is_whole(&files);
}

#[test]
fn the_summary_and_the_people_of_a_change_are_hidden_on_every_page_it_touched_and_its_text_on_the_page_named()
 {
    // Zed's change touched both pages. A hide of the text of one page hides the change's text, as every surface does and as the one
    // reader of a text at a block says (`Tombstones::of_change`: a surface cannot tell the pages apart without reading them): both
    // pages' records are marked, with the reason of the hide. (Changed by I-T1: 0.4.0 marked the named page's line only, because
    // no line carried a text; now the text is read through that reader, and the export cannot be finer than it is.)
    let r = extended("pages-content");
    plant(
        &r,
        HiddenItemKind::PageRevision,
        B4,
        Some("pages/Two.md"),
        &["content"],
        ReasonCategory::Copyright,
    );
    let files = export_as(&r, &stand_extended(), "bundle");
    for page in ["p1", "p2"] {
        let line = line_of(&files, page, B4);
        assert_eq!(
            *at(&line, "/suppressed"),
            serde_json::json!(["content"]),
            "{page}: {line}"
        );
        assert_eq!(*at(&line, "/reasons/content"), "copyright", "{line}");
        assert!(
            !has_member(&line, "content") && !has_member(&line, "sha256"),
            "{page}: {line}"
        );
    }
    // The line of the same page before it, and the others, keep their text.
    assert!(has_member(&line_of(&files, "p2", B1), "content"));
    assert!(
        everything(&files).contains(SUMMARY_B4),
        "the summary is not hidden"
    );

    // A hide of its summary or its author, on one page, is a hide of the change: its other page's line is marked too.
    for (part, member) in [("summary", "summary"), ("author", "author")] {
        let r = extended("pages-change");
        plant(
            &r,
            HiddenItemKind::PageRevision,
            B4,
            Some("pages/Two.md"),
            &[part],
            ReasonCategory::Spam,
        );
        let files = export_as(&r, &stand_extended(), "bundle");
        for page in ["p1", "p2"] {
            let line = line_of(&files, page, B4);
            assert_eq!(
                *at(&line, "/suppressed"),
                serde_json::json!([part]),
                "{line}"
            );
            assert_eq!(*at(&line, &format!("/reasons/{part}")), "spam");
            assert!(!has_member(&line, member), "{page}: {line}");
        }
        let all = everything(&files);
        assert_eq!(all.contains(SUMMARY_B4), part != "summary");
        assert_eq!(all.contains("Zed Hidden"), part != "author");
    }
}

/// The extended history and two rollbacks made before H-7, which quoted the summary of the change they undid: one of Zed's
/// change (B4), one of Yuki's first (B5).
fn stand_with_old_undos() -> (Stand, String, String) {
    let quoting = |of: &str, proposal: &str, base_block: &str| {
        Message::proposal(
            summary(&format!("Undo: {of}")),
            ProposalId::parse(proposal).expect("a proposal id"),
            account(ANN, "Ann"),
            account(BEN, "Ben"),
            base(base_block),
        )
        .expect("a message")
    };
    let mut blocks = history_extended();
    let undo_zed = "b10c000000000000000000000000000000000000000000000000000000000007";
    let undo_yuki = "b10c000000000000000000000000000000000000000000000000000000000008";
    blocks.push(block(
        undo_zed,
        8,
        "a7",
        &quoting(SUMMARY_B4, "019a0000-0000-7000-8000-000000000e07", B6),
        &[("modify", "pages/One.md")],
    ));
    blocks.push(block(
        undo_yuki,
        9,
        "a8",
        &quoting(SUMMARY_B5, "019a0000-0000-7000-8000-000000000e08", undo_zed),
        &[("modify", "pages/Two.md")],
    ));
    (
        Stand::new(files(), blocks).identified("pages/Gone.md", "gone-1"),
        undo_zed.to_owned(),
        undo_yuki.to_owned(),
    )
}

#[test]
fn an_old_undo_that_quotes_a_hidden_summary_is_masked_in_the_bundle_as_that_summary_is() {
    let (stand, undo_zed, undo_yuki) = stand_with_old_undos();

    // Nothing hidden: the lines are as they were written, the quoted words and all.
    let r = extended("undo-open");
    let files = export_as(&r, &stand, "bundle");
    assert_eq!(
        *at(&line_of(&files, "p1", &undo_zed), "/summary"),
        format!("Undo: {SUMMARY_B4}")
    );

    // The summary of Zed's change is hidden: the rollback that quoted it loses its summary, with the reason of that hide, and
    // the words are in no file of the bundle. The rollback of a summary that is not hidden is untouched.
    let r = extended("undo-hidden");
    page_hide(&r, B4, &["summary"]);
    let files = export_as(&r, &stand, "bundle");
    let line = line_of(&files, "p1", &undo_zed);
    assert_eq!(*at(&line, "/suppressed"), serde_json::json!(["summary"]));
    assert_eq!(*at(&line, "/reasons/summary"), "personal-data");
    assert!(!has_member(&line, "summary"), "{line}");
    assert!(!everything(&files).contains(SUMMARY_B4));
    let other = line_of(&files, "p2", &undo_yuki);
    assert!(!has_member(&other, "suppressed"), "{other}");
    assert_eq!(*at(&other, "/summary"), format!("Undo: {SUMMARY_B5}"));
    the_bundle_is_whole(&files);

    // An unhide gives the words back.
    let r = extended("undo-unhidden");
    let hide = page_hide(&r, B4, &["summary"]);
    unhide(&r, &hide);
    let files = export_as(&r, &stand, "bundle");
    assert_eq!(
        *at(&line_of(&files, "p1", &undo_zed), "/summary"),
        format!("Undo: {SUMMARY_B4}")
    );
}

#[test]
fn the_revision_of_an_accepted_proposal_is_the_revision_of_the_block_it_sealed() {
    let r = extended("proposal");
    accepted(&r, P3, ANN, B3);
    plant(
        &r,
        HiddenItemKind::ProposalRevision,
        P3,
        Some("1"),
        &["summary", "author"],
        ReasonCategory::Spam,
    );
    let files = export_as(&r, &stand_extended(), "bundle");
    let line = line_of(&files, "p1", B3);
    assert_eq!(
        *at(&line, "/suppressed"),
        serde_json::json!(["summary", "author"])
    );
    assert_eq!(*at(&line, "/reasons/summary"), "spam");
    assert!(!has_member(&line, "summary") && !has_member(&line, "author"));
    assert!(!everything(&files).contains(SUMMARY_B3));
    // The content and the summary of a revision that was never the one accepted are something no bundle carries.
    let r = extended("proposal-unaccepted");
    accepted(&r, P3, ANN, B3);
    plant(
        &r,
        HiddenItemKind::ProposalRevision,
        P3,
        Some("2"),
        &["content", "summary"],
        ReasonCategory::Spam,
    );
    plant(
        &r,
        HiddenItemKind::ProposalRevision,
        P4,
        Some("1"),
        &["summary"],
        ReasonCategory::Spam,
    );
    // The same wiki with the same acceptances (a seal is a time the export carries) and no hide at all.
    let none = extended("proposal-none");
    accepted(&none, P3, ANN, B3);
    let untouched = export_as(&none, &stand_extended(), "bundle");
    assert_eq!(export_as(&r, &stand_extended(), "bundle"), untouched);
}

/// The author of a proposal is one person's (H-2): a hide of the author of a revision that was **not** the one accepted still
/// hides the proposer's name on the change the proposal became, in the line, in `users.yaml` and in the manifest's declaration.
#[test]
fn the_author_hidden_on_another_revision_of_an_accepted_proposal_is_hidden_on_its_change() {
    let r = extended("proposal-author-elsewhere");
    accepted(&r, P3, ANN, B3);
    plant(
        &r,
        HiddenItemKind::ProposalRevision,
        P3,
        Some("2"),
        &["content", "summary", "author"],
        ReasonCategory::Harassment,
    );
    let files = export_as(&r, &stand_extended(), "bundle");
    let line = line_of(&files, "p1", B3);
    // Only the author: what the revision's content and summary hid is not what the change wrote.
    assert_eq!(*at(&line, "/suppressed"), serde_json::json!(["author"]));
    assert_eq!(*at(&line, "/reasons/author"), "harassment");
    assert!(!has_member(&line, "author"));
    assert!(
        has_member(&line, "summary"),
        "the change's summary is shown"
    );
    assert!(everything(&files).contains(SUMMARY_B3));
    // An unhide gives the export back exactly.
    let again = extended("proposal-author-elsewhere-unhidden");
    accepted(&again, P3, ANN, B3);
    let hide = plant(
        &again,
        HiddenItemKind::ProposalRevision,
        P3,
        Some("2"),
        &["author"],
        ReasonCategory::Harassment,
    );
    unhide(&again, &hide);
    let plain = extended("proposal-author-elsewhere-none");
    accepted(&plain, P3, ANN, B3);
    assert_eq!(
        export_as(&again, &stand_extended(), "bundle"),
        export_as(&plain, &stand_extended(), "bundle")
    );
}

#[test]
fn what_no_bundle_carries_changes_nothing_of_it_and_is_not_declared() {
    let plain = export_as(&extended("plain"), &stand_extended(), "bundle");
    let r = extended("not-carried");
    // A comment on a proposal (comments on proposals are not exported), a block that is in no history, a comment that is
    // nobody's.
    plant(
        &r,
        HiddenItemKind::ProposalComment,
        comment_id(7).as_str(),
        None,
        &["content", "author"],
        ReasonCategory::Other,
    );
    plant(
        &r,
        HiddenItemKind::PageRevision,
        &"c".repeat(64),
        None,
        &["summary"],
        ReasonCategory::Other,
    );
    plant(
        &r,
        HiddenItemKind::ClaimComment,
        comment_id(99).as_str(),
        None,
        &["content"],
        ReasonCategory::Other,
    );
    let files = export_as(&r, &stand_extended(), "bundle");
    assert_eq!(
        files, plain,
        "an export with nothing of it hidden is the one it was"
    );
    let manifest = text(&files, "wiki-bundle.yaml");
    assert!(!manifest.contains("tombstones") && !manifest.contains("hidden by moderation"));
}

#[test]
fn an_unhide_restores_exactly_and_the_export_after_it_is_the_export_before_the_hide() {
    let r = extended("roundtrip");
    accepted(&r, P5, YUKI, B5);
    let before = export_as(&r, &stand_extended(), "before");
    let hides = [
        page_hide(&r, B4, &["content", "summary", "author"]),
        comment_hide(&r, 5, &["content", "author"]),
        plant(
            &r,
            HiddenItemKind::ProposalRevision,
            P5,
            Some("1"),
            &["summary"],
            ReasonCategory::Unlawful,
        ),
    ];
    let hidden = export_as(&r, &stand_extended(), "hidden");
    assert_ne!(hidden, before);
    assert!(text(&hidden, "wiki-bundle.yaml").contains("    entries: 4\n"));
    for hide in &hides {
        unhide(&r, hide);
    }
    let after = export_as(&r, &stand_extended(), "after");
    assert_eq!(after, before, "byte for byte");
    // A hide again, after an unhide, hides again.
    page_hide(&r, B4, &["summary"]);
    let again = export_as(&r, &stand_extended(), "again");
    assert!(!has_member(&line_of(&again, "p1", B4), "summary"));
}

#[test]
fn an_unhide_of_one_hide_leaves_the_others_and_each_part_keeps_the_reason_it_was_hidden_for() {
    let r = extended("two-hides");
    let content = plant(
        &r,
        HiddenItemKind::PageRevision,
        B3,
        None,
        &["content"],
        ReasonCategory::PersonalData,
    );
    let author = plant(
        &r,
        HiddenItemKind::PageRevision,
        B3,
        None,
        &["author"],
        ReasonCategory::Harassment,
    );
    let files = export_as(&r, &stand_extended(), "both");
    let line = line_of(&files, "p1", B3);
    assert_eq!(
        *at(&line, "/suppressed"),
        serde_json::json!(["content", "author"])
    );
    assert_eq!(
        *at(&line, "/reasons"),
        serde_json::json!({"content": "personal-data", "author": "harassment"})
    );
    unhide(&r, &author);
    let files = export_as(&r, &stand_extended(), "one");
    let line = line_of(&files, "p1", B3);
    assert_eq!(*at(&line, "/suppressed"), serde_json::json!(["content"]));
    assert_eq!(
        *at(&line, "/reasons"),
        serde_json::json!({"content": "personal-data"})
    );
    assert!(has_member(&line, "author"));
    unhide(&r, &content);
    let files = export_as(&r, &stand_extended(), "none");
    assert!(!has_member(&line_of(&files, "p1", B3), "suppressed"));
}

#[test]
fn every_reason_is_written_in_the_stores_own_words_in_history_and_threads_alike() {
    let r = extended("reasons");
    let all = [
        ReasonCategory::PersonalData,
        ReasonCategory::Harassment,
        ReasonCategory::Unlawful,
        ReasonCategory::Copyright,
        ReasonCategory::Spam,
        ReasonCategory::Other,
    ];
    for (reason, (block_id, n)) in
        all.into_iter()
            .zip([(B1, 1), (B2, 2), (B3, 3), (B4, 4), (B5, 5), (B6, 6)])
    {
        plant(
            &r,
            HiddenItemKind::PageRevision,
            block_id,
            None,
            &["summary"],
            reason,
        );
        plant(
            &r,
            HiddenItemKind::ClaimComment,
            comment_id(n).as_str(),
            None,
            &["content"],
            reason,
        );
    }
    let files = export_as(&r, &stand_extended(), "bundle");
    let text_all = everything(&files);
    for reason in all {
        assert_eq!(reason.as_str(), reason.as_str().to_ascii_lowercase());
        // Once in a history line at least, and once in a thread line.
        let word = format!("\"summary\":\"{}\"", reason.as_str());
        assert!(text_all.contains(&word), "{word}");
        let word = format!("\"content\":\"{}\"", reason.as_str());
        assert!(text_all.contains(&word), "{word}");
    }
    for word in [
        "personal-data",
        "harassment",
        "unlawful",
        "copyright",
        "spam",
        "other",
    ] {
        assert!(text_all.contains(&format!("\"{word}\"")), "{word}");
    }
    the_bundle_is_whole(&files);
}

#[test]
fn an_export_with_a_hide_is_refused_by_the_import_by_name() {
    let r = extended("import");
    page_hide(&r, B3, &["summary"]);
    export_as(&r, &stand_extended(), "bundle");
    let folder = r.out.with_file_name("bundle");
    let plan = plan_folder(
        &folder,
        &Existing::at(&"a".repeat(64)),
        false,
        &ReadConfig::default(),
    )
    .unwrap();
    assert!(!plan.is_importable(), "{}", plan.report());
    assert!(
        plan.refused.iter().any(|f| f.name == "wiki-bundle.yaml"
            && f.reason.contains("tombstones are not imported yet")),
        "{}",
        plan.report()
    );
    // The same wiki with nothing hidden has no such refusal.
    let none = extended("import-none");
    export_as(&none, &stand_extended(), "bundle");
    let plan = plan_folder(
        &none.out.with_file_name("bundle"),
        &Existing::at(&"a".repeat(64)),
        false,
        &ReadConfig::default(),
    )
    .unwrap();
    assert!(
        !plan.refused.iter().any(|f| f.reason.contains("tombstones")),
        "{}",
        plan.report()
    );
}

/// More hides than one page of the log holds, on whichever store.
fn every_hide_is_read<S>(store: &S)
where
    S: GovernanceStore + AcceptanceStore + WikiStore + AccountStore,
{
    let wiki = store
        .create(NewWiki::new(
            Slug::new("many").unwrap(),
            DisplayName::new("Many").unwrap(),
        ))
        .unwrap();
    let mona = AccountId::parse(MONA).unwrap();
    store
        .add_account(&Account {
            id: mona.clone(),
            display_name: DisplayName::new("Mona").unwrap(),
            local: None,
            created_at: 1,
            disabled: false,
        })
        .unwrap();
    let total: u64 = 450;
    for n in 0..total {
        store
            .record_tombstone_event(&TombstoneEvent {
                id: TombstoneEventId::new(),
                wiki: wiki.id.clone(),
                at: 1_700_000_000 + n as i64,
                act: TombstoneAct::Hide,
                act_id: TombstoneActId::new(),
                item: HiddenItem {
                    kind: HiddenItemKind::PageRevision,
                    id: format!("{n:064x}"),
                    reference: None,
                },
                parts: parts(&["summary"]),
                category: Some(ReasonCategory::Spam),
                note: None,
                affected: None,
                by: GovernanceActor {
                    account: mona.clone(),
                    rung: Role::Maintainer,
                },
                policy_block: None,
                of_event: None,
                appeal: None,
                declined_transition: None,
            })
            .unwrap();
    }
    let read = Suppressions::read(store, store, &wiki.id).unwrap();
    for n in 0..total {
        assert!(
            read.of_change(&format!("{n:064x}")).summary.is_some(),
            "hide {n} of {total} was not read"
        );
    }
    assert!(read.of_change(&format!("{total:064x}")).is_empty());
}

#[test]
fn every_hide_of_the_wiki_is_read_however_many_there_are_on_both_stores() {
    every_hide_is_read(&MemoryStore::new());
    every_hide_is_read(&inklkunn_store::SqliteStore::open_in_memory().unwrap());
}

/// A document of the repository with its whitespace collapsed, so that a sentence wrapped across lines is the sentence.
fn document(relative: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(relative),
    )
    .unwrap_or_else(|e| panic!("{relative}: {e}"))
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ")
}

#[test]
fn security_md_and_the_operator_docs_say_what_still_holds_hidden_content_in_the_words_the_export_uses()
 {
    let security = document("SECURITY.md");
    assert!(security.contains("## What still holds hidden content"));
    // The sentence the moderation form says too (RFC 006 D-8): what holds the content, and what removes it.
    for sentence in [
        "Hiding is not removal",
        "What still holds the content: the repository (prikk keeps every object), the operator's backups and `archive export`, and any copy made before the hide.",
        "Removal from those is the operator's P-1 procedure, not offered yet",
        "A person who must have content removed cannot get that from the moderation form.",
        "made **after** a hide leaves the hidden part out",
        "A bundle made **before** the hide, and every `--archive`",
    ] {
        assert!(security.contains(sentence), "SECURITY.md: {sentence}");
    }
    // The names it uses are the names the export writes: the members of a marked line and the manifest's declaration.
    let r = extended("docs");
    page_hide(&r, B3, &["summary"]);
    let files = export_as(&r, &stand_extended(), "bundle");
    let line = text(&files, "history/p1.jsonl");
    let manifest = text(&files, "wiki-bundle.yaml");
    for (name, written) in [
        ("`suppressed`", line.contains("\"suppressed\":")),
        ("`reasons`", line.contains("\"reasons\":")),
        (
            "`inklkunn.tombstones`",
            manifest.contains("\n  tombstones:\n"),
        ),
        (
            "`fidelity.dropped`",
            manifest.contains("\n  dropped: [{construct:"),
        ),
    ] {
        assert!(written, "the export does not write {name}");
        assert!(security.contains(name), "SECURITY.md does not name {name}");
    }
    let keys = document("docs/KEYS.md");
    for sentence in [
        "**Hidden content and an export.**",
        "An export made **after** moderation hid something leaves that part out of the bundle",
        "refuses a bundle that declares it",
        "`--archive` (prikk's own archive holds every object, hidden or not)",
    ] {
        assert!(keys.contains(sentence), "docs/KEYS.md: {sentence}");
    }
    assert!(document("docs/RECOVERY.md").contains("What still holds hidden content"));
}
