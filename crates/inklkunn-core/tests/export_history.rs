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
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use inklkunn_core::export::{
    ExportError, ExportNote, Exported, HistoryInputs, INDEX_WAIT_ROUNDS, KnowledgeRecords,
    export_bundle_with, knowledge_records,
};
use inklkunn_core::message::{Actor, BlockId, Message, Summary, SystemLabel};
use inklkunn_core::read::{Monotonic, ReadConfig, ReadService};
use inklkunn_prikk::{ExtractBounds, Point, Storage, StorageError};
use inklkunn_store::access::Role;
use inklkunn_store::governance::Actor as GovernanceActor;
use inklkunn_store::{
    AcceptanceId, AcceptanceOutcome, AcceptanceRecord, AcceptanceResult, AcceptanceStore, Account,
    AccountId, AccountStore, ActStore, Base, DerivedStore, DisplayName, FileChange,
    GovernanceStore, HiddenItem, HiddenItemKind, HiddenParts, IntentId, IntentRecord, MemoryStore,
    NewWiki, ProposalChange, ProposalId, ProposalOp, ProposalRecord, ProposalState, ProposalStore,
    ProposalTransition, ReasonCategory, RevisionContent, RevisionRecord, RootLayout, Slug,
    SystemActEnd, SystemActId, SystemActKind, SystemActRecord, TombstoneAct, TombstoneActId,
    TombstoneEvent, TombstoneEventId, WikiRecord, WikiStore,
};

#[path = "support/derived_spy.rs"]
mod derived_spy;
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

/// What an export is asked with besides the wiki: the repository, the one reader, the store the first-seen times are kept in, and
/// the bounds a test may make smaller.
struct Ask<'a> {
    storage: &'a dyn Storage,
    reads: &'a ReadService,
    derived: &'a dyn DerivedStore,
    cap: Option<u64>,
    chunk: Option<usize>,
}

/// The wiki exported as the command line exports it, to `<tmp>/<name>`, with no pause between rounds of waiting.
fn export_ask(
    r: &Rig,
    ask: Ask<'_>,
    name: &str,
    say: &mut dyn FnMut(ExportNote),
) -> Result<Exported, ExportError> {
    let records = knowledge_records(
        &*r.store, &*r.store, &*r.store, &*r.store, &*r.store, &r.wiki.id,
    )
    .unwrap();
    let mut inputs = HistoryInputs::new(ask.reads, ask.derived)
        .saying(say)
        .pausing(Duration::ZERO);
    if let Some(cap) = ask.cap {
        inputs = inputs.holding_at_most(cap);
    }
    if let Some(blocks) = ask.chunk {
        inputs = inputs.first_seen_in_chunks_of(blocks);
    }
    export_bundle_with(
        ask.storage,
        &r.layout,
        &r.wiki,
        &r.out.with_file_name(name),
        "0.0.0",
        NOW,
        &records,
        &mut inputs,
    )
}

/// A clock that is a second further at every look: a request's budget of nothing is then spent at its first look after a `show`, and
/// it builds one block of the index a call.
fn ticking() -> Monotonic {
    let now = Arc::new(AtomicU64::new(0));
    Arc::new(move || Duration::from_secs(now.fetch_add(1, Ordering::SeqCst)))
}

/// The same reader, building the index one block at a call.
fn slow(reads: ReadService) -> ReadService {
    reads.with_writes_budget(Duration::ZERO, ticking())
}

/// `n` edits of an attachment, then a page made and edited once: the first text of the history is `n` blocks into the log, so a
/// reader that builds one block a call has to be waited for.
fn late_page(n: u64) -> Stand {
    let mut blocks = vec![block(
        &b(0),
        1,
        "a0",
        &system("Create the wiki"),
        &[("add", "attachments/map.png")],
    )];
    for k in 1..n {
        blocks.push(block(
            &b(k),
            k + 1,
            &format!("a{k}"),
            &by(k, "Ann redraws the map", (ANN, "Ann")),
            &[("modify", "attachments/map.png")],
        ));
    }
    blocks.push(block(
        &b(n),
        n + 1,
        "an",
        &by(n, "Ann writes the harbour", (ANN, "Ann")),
        &[("add", "pages/Harbour.md")],
    ));
    blocks.push(block(
        &b(n + 1),
        n + 2,
        "ao",
        &by(n + 1, "Cyd corrects the harbour", (CYD, "Cyd")),
        &[("modify", "pages/Harbour.md")],
    ));
    let tip = harbour("Harbour", WORDS_1);
    Stand::new(
        BTreeMap::from([
            ("pages/Harbour.md".to_owned(), tip.clone()),
            ("attachments/map.png".to_owned(), "PNG".to_owned()),
        ]),
        blocks,
    )
    .giving(&b(n), "pages/Harbour.md", &harbour("Harbour", WORDS_0))
    .giving(&b(n + 1), "pages/Harbour.md", &tip)
}

/// `life()` with its second block a merge: a block of a kind whose effect on the files its operations do not list.
fn opaque_life() -> Stand {
    let mut stand = life();
    if let Some((merge, _)) = stand.blocks.get_mut(1) {
        merge.kind = "Merge".to_owned();
    }
    stand
}

/// A repository whose `show` fails once it has been asked for this many blocks in all: the export's own pass takes one for each
/// block, and what the index asks for after them is what fails.
struct Failing {
    inner: Stand,
    asked: AtomicUsize,
    after: usize,
}

impl Failing {
    fn after(inner: Stand, calls: usize) -> Self {
        Failing {
            inner,
            asked: AtomicUsize::new(0),
            after: calls,
        }
    }
}

impl Storage for Failing {
    fn views(&self, p: &Path) -> Result<inklkunn_prikk::storage::Views, StorageError> {
        self.inner.views(p)
    }
    fn all_views(&self, p: &Path) -> Result<inklkunn_prikk::storage::Views, StorageError> {
        self.inner.all_views(p)
    }
    fn tree(
        &self,
        p: &Path,
        point: &Point,
        prefix: Option<&str>,
    ) -> Result<inklkunn_prikk::storage::Tree, StorageError> {
        self.inner.tree(p, point, prefix)
    }
    fn stat(
        &self,
        p: &Path,
        point: &Point,
        path: &str,
    ) -> Result<inklkunn_prikk::storage::FileMeta, StorageError> {
        self.inner.stat(p, point, path)
    }
    fn read(&self, p: &Path, point: &Point, path: &str, max: u64) -> Result<Vec<u8>, StorageError> {
        self.inner.read(p, point, path, max)
    }
    fn history(
        &self,
        p: &Path,
        view: &Point,
        limit: Option<u64>,
    ) -> Result<inklkunn_prikk::storage::History, StorageError> {
        self.inner.history(p, view, limit)
    }
    fn change(
        &self,
        p: &Path,
        block: &str,
    ) -> Result<inklkunn_prikk::storage::Change, StorageError> {
        if self.asked.fetch_add(1, Ordering::SeqCst) >= self.after {
            return Err(StorageError::Refused {
                verb: "show",
                message: "the repository could not show the block".to_owned(),
                kind: None,
                reason: None,
            });
        }
        self.inner.change(p, block)
    }
    fn diff(
        &self,
        p: &Path,
        a: &Point,
        b: &Point,
        paths: &[&str],
    ) -> Result<inklkunn_prikk::storage::Diff, StorageError> {
        self.inner.diff(p, a, b, paths)
    }
    fn plan(
        &self,
        p: &Path,
        point: &Point,
    ) -> Result<inklkunn_prikk::storage::Materialized, StorageError> {
        self.inner.plan(p, point)
    }
    fn materialize(
        &self,
        p: &Path,
        point: &Point,
        scratch: &Path,
        name: &str,
        bounds: ExtractBounds,
    ) -> Result<inklkunn_prikk::storage::Materialized, StorageError> {
        self.inner.materialize(p, point, scratch, name, bounds)
    }
    fn verify(&self, p: &Path) -> Result<inklkunn_prikk::storage::Verification, StorageError> {
        self.inner.verify(p)
    }
}

/// The act that made the block `block` of the wiki sealed it at `at`: a system act.
fn sealed_by_a_system_act(r: &Rig, block: &str, at: i64) {
    let act = SystemActRecord {
        id: SystemActId::new(),
        wiki: r.wiki.id.clone(),
        view: "heads/main".to_owned(),
        kind: SystemActKind::System,
        base_block: "a".repeat(64),
        message: "A change\n\nKind: system\n".to_owned(),
        message_digest: "b".repeat(64),
        digest: "c".repeat(64),
        bytes: 1,
        created_at: at - 1,
    };
    r.store.begin_system_act(&act).unwrap();
    r.store
        .finish_system_act(&SystemActEnd {
            act: act.id,
            outcome: AcceptanceOutcome::Sealed {
                block_id: block.to_owned(),
                patch_id: "a0".to_owned(),
            },
            at,
        })
        .unwrap();
}

/// A proposal of Ann's whose acceptance sealed the block `block` at `at`.
fn sealed_by_an_acceptance(r: &Rig, proposal_number: u64, block: &str, at: i64) {
    let p = proposal(proposal_number);
    let author = AccountId::parse(ANN).unwrap();
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
            path: "pages/Harbour.md".to_owned(),
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
        created_at: at - 1,
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
                block_id: block.to_owned(),
                patch_id: "a3".to_owned(),
            },
            at,
        })
        .unwrap();
}

/// Every history file of the bundle has the times of its records in order, none before the one before it.
fn every_file_is_in_order(files: &BTreeMap<String, Vec<u8>>) {
    let mut seen = 0;
    for (path, bytes) in files.iter().filter(|(p, _)| p.starts_with("history/")) {
        let times: Vec<String> = String::from_utf8_lossy(bytes)
            .lines()
            .map(|l| {
                let line: serde_json::Value = serde_json::from_str(l).unwrap();
                at(&line, "/at").as_str().unwrap().to_owned()
            })
            .collect();
        // RFC 3339 times of one zone, in whole seconds, sort as they read.
        assert!(
            times.windows(2).all(|w| w[0] <= w[1]),
            "{path} runs backwards: {times:?}"
        );
        seen += times.len();
    }
    assert!(seen > 0, "no file was checked");
}

fn reads_with(r: &Rig, storage: Arc<dyn Storage>, derived: Arc<dyn DerivedStore>) -> ReadService {
    ReadService::new(
        storage,
        derived,
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

/// Nothing of a failed export is left: no bundle, and no partial one beside it.
fn nothing_is_left(r: &Rig, name: &str) {
    assert!(!r.out.with_file_name(name).exists());
    let beside: Vec<String> = std::fs::read_dir(r.out.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(!beside.iter().any(|n| n.contains(name)), "{beside:?}");
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
    let mut inputs = HistoryInputs::new(&reads, &*r.store)
        .saying(&mut say)
        .pausing(Duration::ZERO);
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
fn the_files_of_pages_with_no_id_are_named_so_that_a_file_system_that_ignores_case_keeps_them_apart()
 {
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

// ------------------------------------------------------------------------------------------------ not yet and never (X-1)

#[test]
fn a_text_no_index_can_attribute_is_withheld_with_its_reason_not_waited_for_and_the_export_goes_on()
{
    // b(1) is a merge, a block whose effect on the files its operations do not list, and b(2) is hidden: the text of b(1) cannot be
    // told from a hidden one, whatever is waited for.
    let r = rig("withheld");
    hide(&r, &b(2), &["content"]);
    let (said, result) = try_export(&r, &opaque_life(), "bundle");
    let done = result.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(done.revisions_withheld, 1);
    assert!(
        said.iter()
            .all(|n| !matches!(n, ExportNote::Waiting { .. })),
        "nothing was waited for: {said:?}"
    );
    assert!(
        said.contains(&ExportNote::Withheld {
            block: b(1),
            path: "pages/Harbour.md".to_owned(),
            reason: "opaque-block",
        }),
        "it said so for the revision: {said:?}"
    );
    let files = files_of(&r.out.with_file_name("bundle"));
    let lines = records(&files, "harbour-1");
    assert_eq!(lines.len(), 5);
    // The withheld record is whole but for its text, and is not moderation's.
    let withheld = lines.get(1).unwrap();
    assert_eq!(*at(withheld, "/rev"), b(1));
    assert!(
        !has(withheld, "content") && !has(withheld, "sha256"),
        "{withheld}"
    );
    assert_eq!(at(withheld, "/inklkunn/withheld"), "opaque-block");
    assert!(!has(withheld, "suppressed") && !has(withheld, "reasons"));
    assert_eq!(at(withheld, "/type"), "edit");
    assert_eq!(at(withheld, "/summary"), "Ann adds the tide table");
    assert_eq!(at(withheld, "/author/id"), ANN);
    // The hidden revision is moderation's, and says so.
    assert_eq!(
        at(lines.get(2).unwrap(), "/suppressed"),
        &serde_json::json!(["content"])
    );
    assert!(at(lines.get(2).unwrap(), "/inklkunn/withheld").is_null());
    // The revisions that can be attributed are whole, before it and after it.
    assert_eq!(
        at(lines.first().unwrap(), "/content").as_str(),
        Some(harbour("Harbour", WORDS_0).as_str())
    );
    assert_eq!(
        at(lines.get(3).unwrap(), "/content").as_str(),
        Some(harbour("Port", WORDS_3).as_str())
    );
    assert!(!everything(&files).contains(WORDS_1));
    assert!(!everything(&files).contains(WORDS_2));
    let manifest = text(&files, "wiki-bundle.yaml");
    assert!(manifest.contains("  revisions_withheld: 1\n"), "{manifest}");
    every_file_is_history_records(&files);
}

#[test]
fn an_opaque_block_withholds_nothing_where_nothing_is_hidden() {
    // No index is asked of when nothing is hidden: the same wiki gives every text, and still says it passes a block it cannot trace.
    let r = rig("calm");
    let (said, result) = try_export(&r, &opaque_life(), "bundle");
    let done = result.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(done.revisions_withheld, 0);
    assert!(
        said.iter()
            .all(|n| matches!(n, ExportNote::Revisions { .. })),
        "{said:?}"
    );
    let files = files_of(&r.out.with_file_name("bundle"));
    assert!(everything(&files).contains(WORDS_1));
    let manifest = text(&files, "wiki-bundle.yaml");
    assert!(manifest.contains("  revisions_withheld: 0\n"), "{manifest}");
}

#[test]
fn an_index_that_is_only_short_is_waited_for_a_round_at_a_time_and_then_the_text_is_given() {
    // The first text is six blocks into the log, and a request builds one block a call: the index is short, not hopeless.
    let r = rig("short");
    hide(&r, &b(7), &["content"]);
    let stand = late_page(6);
    let reads = slow(reads_of(&r, &stand));
    let said = RefCell::new(Vec::new());
    let mut say = |n: ExportNote| said.borrow_mut().push(n);
    let done = export_ask(
        &r,
        Ask {
            storage: &stand,
            reads: &reads,
            derived: &*r.store,
            cap: None,
            chunk: None,
        },
        "bundle",
        &mut say,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let said = said.into_inner();
    let rounds: Vec<usize> = said
        .iter()
        .filter_map(|n| match n {
            ExportNote::Waiting { round, rounds } => {
                assert_eq!(*rounds, INDEX_WAIT_ROUNDS);
                Some(*round)
            }
            _ => None,
        })
        .collect();
    assert!(!rounds.is_empty(), "it waited: {said:?}");
    assert_eq!(rounds, (1..=rounds.len()).collect::<Vec<_>>());
    assert!(rounds.len() < INDEX_WAIT_ROUNDS);
    assert_eq!(done.revisions_withheld, 0);
    let files = files_of(&r.out.with_file_name("bundle"));
    let lines = records(&files, "harbour-1");
    assert_eq!(lines.len(), 2);
    assert_eq!(
        at(lines.first().unwrap(), "/content").as_str(),
        Some(harbour("Harbour", WORDS_0).as_str())
    );
    assert_eq!(
        at(lines.get(1).unwrap(), "/suppressed"),
        &serde_json::json!(["content"])
    );
    // The hidden revision's text is the page's text at the tip, which a bundle carries as it is stored (a tip cannot be hidden), and
    // it is in no history record.
    assert!(!has(lines.get(1).unwrap(), "content"));
    assert!(!text(&files, "history/harbour-1.jsonl").contains(WORDS_1));
}

#[test]
fn an_index_that_keeps_moving_for_more_rounds_than_the_export_waits_is_not_yet_and_nothing_is_written()
 {
    let r = rig("long-wait");
    hide(&r, &b(71), &["content"]);
    let stand = late_page(70);
    let reads = slow(reads_of(&r, &stand));
    let said = RefCell::new(Vec::new());
    let mut say = |n: ExportNote| said.borrow_mut().push(n);
    let err = export_ask(
        &r,
        Ask {
            storage: &stand,
            reads: &reads,
            derived: &*r.store,
            cap: None,
            chunk: None,
        },
        "bundle",
        &mut say,
    )
    .unwrap_err();
    assert!(
        matches!(err, ExportError::IndexNotSettled { rounds } if rounds == INDEX_WAIT_ROUNDS),
        "{err}"
    );
    let rounds: Vec<usize> = said
        .into_inner()
        .iter()
        .filter_map(|n| match n {
            ExportNote::Waiting { round, .. } => Some(*round),
            _ => None,
        })
        .collect();
    assert_eq!(rounds, (1..=INDEX_WAIT_ROUNDS).collect::<Vec<_>>());
    let sentence = err.to_string();
    assert!(
        sentence.contains("not built yet") && sentence.contains("none was written"),
        "{sentence}"
    );
    nothing_is_left(&r, "bundle");
}

#[test]
fn an_index_that_does_not_move_at_all_is_given_up_on_after_a_few_tries_with_the_same_sentence() {
    let r = rig("stalled");
    hide(&r, &b(7), &["content"]);
    let stand = late_page(6);
    let spy = Arc::new(derived_spy::Spy::over(r.store.clone()));
    spy.drop_appends
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let reads = reads_with(&r, Arc::new(stand.clone()), spy.clone());
    let said = RefCell::new(Vec::new());
    let mut say = |n: ExportNote| said.borrow_mut().push(n);
    let err = export_ask(
        &r,
        Ask {
            storage: &stand,
            reads: &reads,
            derived: &*spy,
            cap: None,
            chunk: None,
        },
        "bundle",
        &mut say,
    )
    .unwrap_err();
    assert!(matches!(err, ExportError::IndexNotSettled { .. }), "{err}");
    assert!(err.to_string().contains("not built yet"), "{err}");
    // No round of the index moving was counted, since it did not move.
    assert!(
        said.into_inner()
            .iter()
            .all(|n| !matches!(n, ExportNote::Waiting { .. }))
    );
    nothing_is_left(&r, "bundle");
}

#[test]
fn an_index_that_cannot_be_built_fails_the_export_with_a_sentence_that_says_what_failed() {
    let blocks = late_page(6).blocks.len();
    // The repository fails at the very first block the index asks for: the text's own read says so.
    let r = rig("failing-first");
    hide(&r, &b(7), &["content"]);
    let stand = Arc::new(Failing::after(late_page(6), blocks));
    let reads = reads_with(&r, stand.clone(), r.store.clone());
    let err = export_ask(
        &r,
        Ask {
            storage: &*stand,
            reads: &reads,
            derived: &*r.store,
            cap: None,
            chunk: None,
        },
        "bundle",
        &mut |_| {},
    )
    .unwrap_err();
    let sentence = err.to_string();
    assert!(
        sentence.contains("pages/Harbour.md")
            && sentence.contains(&b(6))
            && sentence.contains("the repository could not show the block")
            && sentence.contains("nothing was written"),
        "{sentence}"
    );
    nothing_is_left(&r, "bundle");

    // The repository fails after the index has begun, while the export is waiting for it to be built.
    let r = rig("failing-later");
    hide(&r, &b(7), &["content"]);
    let stand = Arc::new(Failing::after(late_page(6), blocks + 3));
    let reads = slow(reads_with(&r, stand.clone(), r.store.clone()));
    let err = export_ask(
        &r,
        Ask {
            storage: &*stand,
            reads: &reads,
            derived: &*r.store,
            cap: None,
            chunk: None,
        },
        "bundle",
        &mut |_| {},
    )
    .unwrap_err();
    let sentence = err.to_string();
    assert!(
        sentence.contains("the index of who wrote each page's text could not be built")
            && sentence.contains("the repository could not show the block")
            && sentence.contains("nothing was written"),
        "{sentence}"
    );
    nothing_is_left(&r, "bundle");
}

// ------------------------------------------------------------------------------------------------ the time of a record (X-2)

#[test]
fn a_wiki_whose_blocks_were_never_viewed_exports_the_times_its_acts_sealed_and_a_block_made_outside_it_is_first_seen()
 {
    let r = rig("sealed");
    // b(0) was made by a system act, b(1) and b(2) by acceptances; b(3) and b(4) were made by no act of this service's.
    sealed_by_a_system_act(&r, &b(0), 1_600_000_000);
    sealed_by_an_acceptance(&r, 1, &b(1), 1_600_000_100);
    sealed_by_an_acceptance(&r, 2, &b(2), 1_600_000_200);
    let files = export(&r, &life(), "bundle");
    let lines = records(&files, "harbour-1");
    let seconds = inklkunn_store::record::rfc3339;
    assert_eq!(
        lines
            .iter()
            .map(|l| at(l, "/at").as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        [
            seconds(1_600_000_000),
            seconds(1_600_000_100),
            seconds(1_600_000_200),
            seconds(NOW),
            seconds(NOW)
        ],
        "the seal, and today only for what no act of this service made"
    );
    assert_eq!(
        lines
            .iter()
            .map(|l| at(l, "/inklkunn/at_from").as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        ["sealed", "sealed", "sealed", "first-seen", "first-seen"]
    );
    // The other page of b(0) has the same time, from the same seal.
    assert_eq!(
        at(records(&files, "anchor-1").first().unwrap(), "/at"),
        &serde_json::json!(seconds(1_600_000_000))
    );
    // Only the blocks with neither a seal nor a time of their own were written to the store.
    every_file_is_in_order(&files);
}

#[test]
fn a_time_that_would_run_backwards_in_a_file_is_the_one_before_it_and_says_so() {
    let r = rig("ordered");
    // b(0) sealed late, b(1) first seen long before it, b(2) sealed before b(0): clocks and sources that do not agree.
    sealed_by_a_system_act(&r, &b(0), 1_650_000_000);
    r.store.first_seen(&r.wiki.id, &[b(1)], 100).unwrap();
    sealed_by_an_acceptance(&r, 2, &b(2), 1_600_000_000);
    let files = export(&r, &life(), "bundle");
    let lines = records(&files, "harbour-1");
    let seconds = inklkunn_store::record::rfc3339;
    assert_eq!(
        lines
            .iter()
            .map(|l| at(l, "/inklkunn/at_from").as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        ["sealed", "ordered", "ordered", "first-seen", "first-seen"]
    );
    assert_eq!(
        lines
            .iter()
            .map(|l| at(l, "/at").as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        [
            seconds(1_650_000_000),
            seconds(1_650_000_000),
            seconds(1_650_000_000),
            seconds(NOW),
            seconds(NOW)
        ]
    );
    every_file_is_in_order(&files);
    // And in every file of a bundle of a wiki whose times are whatever they are.
    let fresh = rig("ordered-fresh");
    sealed_by_a_system_act(&fresh, &b(4), 1_500_000_000);
    every_file_is_in_order(&export(&fresh, &life(), "bundle"));
}

// ------------------------------------------------------------------------------------------------ what is not listed (X-3)

/// A merge between the second and the third change: a block that did something to the files its operations do not say.
fn with_a_merge() -> Stand {
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
            &by(1, "Ann edits the harbour", (ANN, "Ann")),
            &[("modify", "pages/Harbour.md")],
        ),
        block_of(&b(2), 3, "a2", &system("A merge"), vec![]),
        block(
            &b(3),
            4,
            "a3",
            &by(3, "Cyd edits the harbour", (CYD, "Cyd")),
            &[("modify", "pages/Harbour.md")],
        ),
        block(
            &b(4),
            5,
            "a4",
            &by(4, "Ann edits the harbour again", (ANN, "Ann")),
            &[("modify", "pages/Harbour.md")],
        ),
    ];
    let mut stand = Stand::new(
        BTreeMap::from([
            ("pages/Harbour.md".to_owned(), harbour("Harbour", WORDS_3)),
            ("pages/Anchor.md".to_owned(), ANCHOR.to_owned()),
        ]),
        blocks,
    )
    .giving(&b(0), "pages/Anchor.md", ANCHOR);
    if let Some((merge, _)) = stand.blocks.get_mut(2) {
        merge.kind = "Merge".to_owned();
    }
    stand
        .giving(&b(0), "pages/Harbour.md", &harbour("Harbour", WORDS_0))
        .giving(&b(1), "pages/Harbour.md", &harbour("Harbour", WORDS_1))
        .giving(&b(3), "pages/Harbour.md", &harbour("Harbour", WORDS_2))
        .giving(&b(4), "pages/Harbour.md", &harbour("Harbour", WORDS_3))
}

#[test]
fn the_first_record_after_a_block_whose_effect_is_not_listed_says_so_and_no_record_is_made_up() {
    let r = rig("untraced");
    let (_, result) = try_export(&r, &with_a_merge(), "bundle");
    let done = result.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(done.histories_after_untraced, 1);
    let files = files_of(&r.out.with_file_name("bundle"));
    let lines = records(&files, "harbour-1");
    // The merge has no record of its own: four blocks, four records.
    assert_eq!(
        lines
            .iter()
            .map(|l| at(l, "/rev").as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        [b(0), b(1), b(3), b(4)]
    );
    // The record after the merge says which block, and its parent is the record before the merge.
    let after = lines.get(2).unwrap();
    assert_eq!(*at(after, "/inklkunn/after_untraced"), b(2));
    assert_eq!(*at(after, "/parent"), b(1));
    for n in [0, 1, 3] {
        assert!(
            at(lines.get(n).unwrap(), "/inklkunn/after_untraced").is_null(),
            "{n}"
        );
    }
    // The page the merge may have changed nothing of the records of has none.
    assert!(
        at(
            records(&files, "anchor-1").first().unwrap(),
            "/inklkunn/after_untraced"
        )
        .is_null()
    );
    let manifest = text(&files, "wiki-bundle.yaml");
    assert!(
        manifest.contains("  histories_after_untraced:\n    - \"history/harbour-1.jsonl\"\n"),
        "{manifest}"
    );
    assert!(
        manifest.contains("  history_changes_without_a_page: 1\n"),
        "{manifest}"
    );
    every_file_is_history_records(&files);
    // A wiki with none of them says so in the same place.
    let plain = rig("untraced-none");
    let files = export(&plain, &life(), "bundle");
    assert!(
        text(&files, "wiki-bundle.yaml").contains("  histories_after_untraced: []\n"),
        "{}",
        text(&files, "wiki-bundle.yaml")
    );
}

// ------------------------------------------------------------------------------------------------ within one block (X-4)

#[test]
fn a_page_removed_and_made_again_with_its_id_by_one_block_is_one_file_with_a_rename_record() {
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
            &by(
                1,
                "Ann moves the harbour by removing and making it again",
                (ANN, "Ann"),
            ),
            &[("delete", "pages/Harbour.md"), ("add", "pages/Port.md")],
        ),
        block(
            &b(2),
            3,
            "a2",
            &by(2, "Cyd edits the port", (CYD, "Cyd")),
            &[("modify", "pages/Port.md")],
        ),
    ];
    let tip = harbour("Port", WORDS_2);
    let stand = Stand::new(
        BTreeMap::from([("pages/Port.md".to_owned(), tip.clone())]),
        blocks,
    )
    .giving(&b(0), "pages/Harbour.md", &harbour("Harbour", WORDS_0))
    .giving(&b(1), "pages/Port.md", &harbour("Port", WORDS_1))
    .giving(&b(2), "pages/Port.md", &tip);
    let r = rig("remade");
    let (_, result) = try_export(&r, &stand, "bundle");
    let done = result.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(done.histories_not_written, 0);
    assert_eq!(done.histories, 1);
    let files = files_of(&r.out.with_file_name("bundle"));
    let lines = records(&files, "harbour-1");
    assert_eq!(
        lines
            .iter()
            .map(|l| at(l, "/type").as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        ["create", "rename", "edit"]
    );
    let moved = lines.get(1).unwrap();
    assert_eq!(*at(moved, "/rev"), b(1));
    assert_eq!(at(moved, "/from"), "Harbour");
    assert_eq!(at(moved, "/to"), "Port");
    assert_eq!(at(moved, "/inklkunn/from_path"), "pages/Harbour.md");
    assert_eq!(at(moved, "/inklkunn/path"), "pages/Port.md");
    assert_eq!(
        at(moved, "/content").as_str(),
        Some(harbour("Port", WORDS_1).as_str())
    );
    assert_eq!(*at(lines.get(2).unwrap(), "/parent"), b(1));
    // One record a block, so a record's `rev` is unique in its file.
    let revs: std::collections::BTreeSet<&str> = lines
        .iter()
        .map(|l| at(l, "/rev").as_str().unwrap())
        .collect();
    assert_eq!(revs.len(), lines.len());
    every_file_is_history_records(&files);
}

#[test]
fn a_page_moved_out_of_the_pages_and_back_by_one_block_is_the_same_page_carried() {
    // Pages with no id are named by their path, so a removal and a new page would be two files.
    let nameless = |title: &str, word: &str| format!("---\ntitle: {title}\n---\n\n{word}\n");
    let blocks = vec![
        block(
            &b(0),
            1,
            "a0",
            &system("Create the wiki"),
            &[("add", "pages/Harbour.md")],
        ),
        block_of(
            &b(1),
            2,
            "a1",
            &by(
                1,
                "Ann parks the harbour and brings it back as Port",
                (ANN, "Ann"),
            ),
            vec![
                rename("pages/Harbour.md", "attachments/parked.md"),
                rename("attachments/parked.md", "pages/Port.md"),
            ],
        ),
    ];
    let tip = nameless("Port", "one");
    let stand = Stand::new(
        BTreeMap::from([("pages/Port.md".to_owned(), tip.clone())]),
        blocks,
    )
    .giving(&b(0), "pages/Harbour.md", &nameless("Harbour", "one"))
    .giving(&b(1), "pages/Port.md", &tip);
    let r = rig("parked");
    let (_, result) = try_export(&r, &stand, "bundle");
    let done = result.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(done.histories, 1, "one page, one file");
    let files = files_of(&r.out.with_file_name("bundle"));
    let names: Vec<&String> = files.keys().filter(|k| k.starts_with("history/")).collect();
    assert_eq!(names, ["history/~%50ort.jsonl"], "{names:?}");
    let lines = records(&files, "~%50ort");
    assert_eq!(
        lines
            .iter()
            .map(|l| at(l, "/type").as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        ["create", "rename"]
    );
    assert_eq!(
        at(lines.get(1).unwrap(), "/inklkunn/from_path"),
        "pages/Harbour.md"
    );
    assert_eq!(at(lines.get(1).unwrap(), "/inklkunn/path"), "pages/Port.md");
}

// ------------------------------------------------------------------------------------------------ what is written, and when (X-6, X-7, X-8)

#[test]
fn the_times_blocks_were_first_seen_are_written_in_chunks_and_only_for_the_blocks_with_no_better_time()
 {
    let stand = life();
    let chunks = |sealed: usize, chunk: Option<usize>| {
        let r = rig(&format!("chunks-{sealed}-{chunk:?}"));
        for n in 0..sealed {
            sealed_by_a_system_act(&r, &b(n as u64), 1_600_000_000 + n as i64);
        }
        let spy = Arc::new(derived_spy::Spy::over(r.store.clone()));
        let reads = reads_with(&r, Arc::new(stand.clone()), spy.clone());
        export_ask(
            &r,
            Ask {
                storage: &stand,
                reads: &reads,
                derived: &*spy,
                cap: None,
                chunk,
            },
            "bundle",
            &mut |_| {},
        )
        .unwrap_or_else(|e| panic!("{e}"));
        spy.seen_calls()
    };
    // Five blocks have records; two at a time is 2, 2 and 1, and all at once is one write.
    assert_eq!(chunks(0, Some(2)), [2, 2, 1]);
    assert_eq!(chunks(0, None), [5]);
    // A block an act of this service sealed is asked of no one: two sealed leave three, then 2 and 1.
    assert_eq!(chunks(2, Some(2)), [2, 1]);
    assert_eq!(chunks(5, None), Vec::<usize>::new());
}

#[test]
fn a_moderation_act_recorded_while_the_export_runs_fails_it_and_writes_nothing() {
    let r = rig("moderated");
    let stand = life();
    let records = knowledge_records(
        &*r.store, &*r.store, &*r.store, &*r.store, &*r.store, &r.wiki.id,
    )
    .unwrap();
    let reads = reads_of(&r, &stand);
    // The history is read and then the texts: a hide made as the texts begin is in the log when they are asked, and in none of what
    // was read of what is hidden at the start.
    let mut say = |n: ExportNote| {
        if matches!(n, ExportNote::Revisions { .. }) {
            hide(&r, &b(1), &["content"]);
        }
    };
    let mut inputs = HistoryInputs::new(&reads, &*r.store)
        .saying(&mut say)
        .pausing(Duration::ZERO);
    let err = export_bundle_with(
        &stand,
        &r.layout,
        &r.wiki,
        &r.out.with_file_name("bundle"),
        "0.0.0",
        NOW,
        &records,
        &mut inputs,
    )
    .unwrap_err();
    assert!(matches!(err, ExportError::ModerationMoved), "{err}");
    assert!(
        err.to_string()
            .contains("a moderation act was recorded during the export; run it again"),
        "{err}"
    );
    nothing_is_left(&r, "bundle");
    // The same export, with nothing recorded meanwhile, is made.
    export(&r, &stand, "again");
}

#[test]
fn the_cap_on_what_an_export_holds_counts_the_lines_as_they_are_written_escapes_included() {
    let page = |filler: &str| {
        format!(
            "---\ntitle: Harbour\nid: harbour-1\n---\n\n{}\n",
            filler.repeat(200)
        )
    };
    let stand_of = |text: &str| {
        Stand::new(
            BTreeMap::from([("pages/Harbour.md".to_owned(), text.to_owned())]),
            vec![block(
                &b(0),
                1,
                "a0",
                &system("Create the wiki"),
                &[("add", "pages/Harbour.md")],
            )],
        )
        .giving(&b(0), "pages/Harbour.md", text)
    };
    let plain = page("a");
    let control = page("\u{1}");
    assert_eq!(
        plain.len(),
        control.len(),
        "the same bytes as written in the page"
    );
    // The plain history is the size of its one line; a cap just above it holds it.
    let r = rig("cap-plain");
    let files = export(&r, &stand_of(&plain), "bundle");
    let held = files.get("history/harbour-1.jsonl").unwrap().len() as u64;
    let with_cap = |tag: &str, stand: &Stand, cap: u64| {
        let r = rig(tag);
        let reads = reads_of(&r, stand);
        let result = export_ask(
            &r,
            Ask {
                storage: stand,
                reads: &reads,
                derived: &*r.store,
                cap: Some(cap),
                chunk: None,
            },
            "bundle",
            &mut |_| {},
        );
        (r, result)
    };
    let (_, result) = with_cap("cap-fits", &stand_of(&plain), held + 10);
    result.unwrap_or_else(|e| panic!("{e}"));
    // The same bytes with control characters are six times as long once they are escaped, and the cap says so.
    let (r, result) = with_cap("cap-escapes", &stand_of(&control), held + 10);
    let err = result.unwrap_err();
    assert!(matches!(err, ExportError::HistoryTextTooLarge), "{err}");
    nothing_is_left(&r, "bundle");
}

// ------------------------------------------------------------------------------------------------ the checksums file (X-9)

#[test]
fn the_manifest_names_the_checksums_file_wiki_commons_calls_it_by_and_it_is_there() {
    let r = rig("sums");
    let files = export(&r, &life(), "bundle");
    assert!(
        text(&files, "wiki-bundle.yaml").contains("\nchecksums: sha256sums.txt\n"),
        "{}",
        text(&files, "wiki-bundle.yaml")
    );
    assert!(files.contains_key("sha256sums.txt"));
    assert!(!files.contains_key("SHA256SUMS"));
    // Every other file is listed, the history files with the digests of what was written.
    let sums = text(&files, "sha256sums.txt");
    for path in files.keys().filter(|p| p.starts_with("history/")) {
        assert!(
            sums.contains(&format!("  {path}\n")),
            "{path} is not listed"
        );
    }
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
