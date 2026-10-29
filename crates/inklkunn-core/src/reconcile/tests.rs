#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;
use std::sync::Mutex;

use inklkunn_prikk::storage::{
    Change, Diff, FileMeta, History, Materialized, PatchMessage, Tree, Verification, View, Views,
};
use inklkunn_prikk::{ExtractBounds, Result as StorageResult};
use inklkunn_signer::message::{Actor, BlockId, ImportSource, Summary, SystemLabel};
use inklkunn_store::{AccountId, DisplayName, MemoryAudit, ProposalId};

use super::*;

fn id(n: usize) -> String {
    format!("{n:064x}")
}

fn block(n: usize, message: &str, root: bool) -> HistoryBlock {
    HistoryBlock {
        block_id: id(n),
        ref_state_id: id(10_000 + n),
        update_seq: n as u64,
        kind: "Normal".into(),
        rollback_block: false,
        patch_count: 1,
        patch_messages: vec![PatchMessage {
            patch_id: id(20_000 + n),
            message: message.into(),
        }],
        previous_ref_state_id: if root { None } else { Some(id(10_000 + n - 1)) },
    }
}

/// A storage with canned views; `history` pages like the real one (newest first, a limit, a block as a
/// point returns the chain from that block back). Everything else is out of scope and answers with an error.
struct Fake {
    /// `(view, chain newest first)`.
    chains: Vec<(String, Vec<HistoryBlock>)>,
    fail_history: bool,
    history_calls: Mutex<usize>,
}

impl Fake {
    fn new(chains: Vec<(&str, Vec<HistoryBlock>)>) -> Self {
        Fake {
            chains: chains.into_iter().map(|(v, c)| (v.to_owned(), c)).collect(),
            fail_history: false,
            history_calls: Mutex::new(0),
        }
    }

    /// The first view's chain, newest first, for a test to extend.
    fn first_chain(&mut self) -> &mut Vec<HistoryBlock> {
        &mut self.chains.first_mut().expect("a view").1
    }
}

fn nope(verb: &'static str) -> StorageError {
    StorageError::Driver {
        verb,
        detail: "the reconciliation reads views and history only".into(),
    }
}

impl Storage for Fake {
    fn views(&self, _repo: &Path) -> StorageResult<Views> {
        Ok(Views {
            schema_version: "branch-list-v1".into(),
            views: self
                .chains
                .iter()
                .map(|(name, _)| View {
                    name: name.clone(),
                    state_id: id(1),
                    closed: false,
                    current: false,
                })
                .collect(),
        })
    }
    fn tree(&self, _: &Path, _: &Point, _: Option<&str>) -> StorageResult<Tree> {
        Err(nope("tree"))
    }
    fn stat(&self, _: &Path, _: &Point, _: &str) -> StorageResult<FileMeta> {
        Err(nope("cat"))
    }
    fn read(&self, _: &Path, _: &Point, _: &str, _: u64) -> StorageResult<Vec<u8>> {
        Err(nope("cat"))
    }
    fn history(&self, _repo: &Path, point: &Point, limit: Option<u64>) -> StorageResult<History> {
        *self.history_calls.lock().unwrap() += 1;
        if self.fail_history {
            return Err(StorageError::Unavailable {
                verb: "log",
                reason: inklkunn_prikk::Unavailable::Busy,
            });
        }
        let limit = usize::try_from(limit.unwrap_or(100)).unwrap();
        // Like the real one: a view only (a block id there is an empty list, which the driver refuses before
        // asking), newest first, cut at the limit.
        let chain: Vec<HistoryBlock> = match point {
            Point::View(v) => self
                .chains
                .iter()
                .find(|(name, _)| name == v)
                .map(|(_, c)| c.clone())
                .ok_or_else(|| nope("log"))?,
            Point::Block(_) => return Err(nope("log")),
        };
        Ok(History {
            schema_version: "log-report-v1".into(),
            view: point.as_arg().to_owned(),
            blocks: chain.into_iter().take(limit).collect(),
        })
    }
    fn change(&self, _: &Path, _: &str) -> StorageResult<Change> {
        Err(nope("show"))
    }
    fn diff(&self, _: &Path, _: &Point, _: &Point, _: &[&str]) -> StorageResult<Diff> {
        Err(nope("diff"))
    }
    fn plan(&self, _: &Path, _: &Point) -> StorageResult<Materialized> {
        Err(nope("extract"))
    }
    fn materialize(
        &self,
        _: &Path,
        _: &Point,
        _: &Path,
        _: &str,
        _: ExtractBounds,
    ) -> StorageResult<Materialized> {
        Err(nope("extract"))
    }
    fn verify(&self, _: &Path) -> StorageResult<Verification> {
        Err(nope("verify"))
    }
}

#[derive(Default)]
struct Records {
    accepted: Vec<AcceptedRecord>,
    acts: Vec<SystemActRecord>,
    fail: bool,
}

impl RecordedActs for Records {
    fn accepted(&self, _: &WikiId) -> Result<Vec<AcceptedRecord>, RecordsError> {
        if self.fail {
            return Err(RecordsError("down".into()));
        }
        Ok(self.accepted.clone())
    }
    fn system_acts(&self, _: &WikiId) -> Result<Vec<SystemActRecord>, RecordsError> {
        Ok(self.acts.clone())
    }
}

fn person(name: &str) -> Actor {
    Actor::Account {
        id: AccountId::new(),
        name: DisplayName::new(name).unwrap(),
    }
}

struct World {
    proposer: Actor,
    acceptor: Actor,
}

impl World {
    fn new() -> Self {
        World {
            proposer: person("Aiko"),
            acceptor: person("Ben"),
        }
    }

    fn proposal(&self, n: usize) -> (Message, ProposalId) {
        let p = ProposalId::new();
        let m = Message::proposal(
            Summary::from_text(&format!("Change {n}")).unwrap(),
            p.clone(),
            self.proposer.clone(),
            self.acceptor.clone(),
            BlockId::new(&id(1)).unwrap(),
        )
        .unwrap();
        (m, p)
    }
}

fn facts(m: &Message, view: &str) -> ExpectedMessage {
    ExpectedMessage {
        change_digest: crate::accept::ChangeDigest::from_bytes([0; 32]),
        wiki: WikiId::new(),
        view: view.to_owned(),
        kind: m.kind(),
        proposal: m.proposal_id().cloned(),
        proposed_by: m.proposed_by().clone(),
        accepted_by: m.accepted_by().clone(),
        base: m.base().cloned(),
        imported_from: m.imported_from().cloned(),
        pinned_tip: None,
    }
}

fn system_message(text: &str) -> Message {
    Message::system(Summary::from_text(text).unwrap(), SystemLabel::inklkunn())
}

fn run(fake: &Fake, records: &Records) -> Report {
    reconcile(&WikiId::new(), Path::new("/unused"), fake, records).expect("reconciled")
}

fn run_with(fake: &Fake, records: &Records, bounds: ReadBounds) -> Result<Report, ReconcileError> {
    reconcile_with(&WikiId::new(), Path::new("/unused"), fake, records, bounds)
}

const MAIN: &str = "heads/main";

/// A clean wiki: a creation, two accepted proposals; every block has its act and every act its block.
fn clean() -> (Fake, Records, World) {
    let w = World::new();
    let create = system_message("Create the wiki");
    let (m1, _) = w.proposal(1);
    let (m2, _) = w.proposal(2);
    let chain = vec![
        block(3, &m2.to_text(), false),
        block(2, &m1.to_text(), false),
        block(1, &create.to_text(), true),
    ];
    let records = Records {
        accepted: vec![
            AcceptedRecord {
                id: "acc-1".into(),
                block_id: id(2),
                facts: facts(&m1, MAIN),
            },
            AcceptedRecord {
                id: "acc-2".into(),
                block_id: id(3),
                facts: facts(&m2, MAIN),
            },
        ],
        acts: vec![SystemActRecord {
            id: "sys-1".into(),
            block_id: Some(id(1)),
            facts: facts(&create, MAIN),
        }],
        fail: false,
    };
    (Fake::new(vec![(MAIN, chain)]), records, w)
}

#[test]
fn a_wiki_whose_history_and_records_agree_is_clean_and_says_what_it_looked_at() {
    let (fake, records, _) = clean();
    let r = run(&fake, &records);
    assert!(r.is_clean(), "{:?}", r.findings);
    assert_eq!((r.views, r.blocks, r.accepted, r.system_acts), (1, 3, 2, 1));
}

#[test]
fn a_seal_no_record_names_is_reported_with_its_block_and_proposal() {
    let (fake, mut records, w) = clean();
    let dropped = records.accepted.remove(1);
    let r = run(&fake, &records);
    let (_, p) = (&dropped, dropped.facts.proposal.clone().unwrap());
    assert_eq!(
        r.findings,
        vec![Finding::UnrecordedSeal {
            block_id: id(3),
            proposal: p.to_string()
        }],
        "{w:?}",
        w = w.proposer
    );
}

#[test]
fn a_change_made_some_other_way_is_unreadable_and_so_is_one_with_no_message() {
    let (mut fake, records, _) = clean();
    let chain = fake.first_chain();
    chain.insert(0, block(4, "Sneaky fix", false));
    let mut empty = block(5, "", false);
    empty.patch_messages.clear();
    chain.insert(0, empty);
    let r = run(&fake, &records);
    assert_eq!(
        r.findings,
        vec![
            Finding::UnreadableBlock { block_id: id(4) },
            Finding::UnreadableBlock { block_id: id(5) },
        ]
    );
}

#[test]
fn an_acceptance_recorded_as_done_that_is_not_in_the_history_is_missing() {
    let (fake, mut records, w) = clean();
    let (m, _) = w.proposal(9);
    records.accepted.push(AcceptedRecord {
        id: "acc-9".into(),
        block_id: id(99),
        facts: facts(&m, MAIN),
    });
    let r = run(&fake, &records);
    assert_eq!(
        r.findings,
        vec![Finding::MissingBlock {
            record_id: "acc-9".into(),
            block_id: id(99)
        }]
    );
}

#[test]
fn a_record_and_its_block_that_disagree_are_a_mismatch_in_each_way_they_can() {
    // The record names another acceptor.
    let (fake, mut records, _) = clean();
    records
        .accepted
        .first_mut()
        .expect("a record")
        .facts
        .accepted_by = person("Mallory");
    let r = run(&fake, &records);
    assert!(
        matches!(r.findings.as_slice(), [Finding::Mismatch { record_id, .. }] if record_id == "acc-1"),
        "{:?}",
        r.findings
    );
    // The record names a block whose message names another proposal.
    let (fake, mut records, _) = clean();
    records.accepted.first_mut().expect("a record").block_id = id(3);
    let r = run(&fake, &records);
    assert!(r.findings.iter().any(|f| matches!(f, Finding::Mismatch { what, .. } if what.contains("names no such proposal"))), "{:?}", r.findings);
    assert!(
        r.findings
            .iter()
            .any(|f| matches!(f, Finding::UnrecordedSeal { .. })),
        "{:?}",
        r.findings
    );
    // The record puts the change in a view the block is not in.
    let (fake, mut records, _) = clean();
    records
        .accepted
        .get_mut(1)
        .expect("a second record")
        .facts
        .view = "heads/draft".into();
    let r = run(&fake, &records);
    assert!(
        matches!(r.findings.as_slice(), [Finding::Mismatch { what, .. }] if what.contains("not in the view")),
        "{:?}",
        r.findings
    );
}

#[test]
fn system_changes_need_a_system_act_matched_once() {
    let (mut fake, mut records, _) = clean();
    // No act for the creation: reported with its kind.
    records.acts.clear();
    let r = run(&fake, &records);
    assert_eq!(
        r.findings,
        vec![Finding::UnrecordedSystemChange {
            block_id: id(1),
            kind: "system"
        }]
    );
    // An act that has no block yet matches by what it says; two identical changes and one act leave one
    // unexplained (an act explains one change).
    let import = Message::import(
        Summary::from_text("Import 3 pages").unwrap(),
        SystemLabel::new("import").unwrap(),
        ImportSource::new("sha256:abc").unwrap(),
    );
    let chain = fake.first_chain();
    chain.insert(0, block(6, &import.to_text(), false));
    chain.insert(0, block(7, &import.to_text(), false));
    records.acts.push(SystemActRecord {
        id: "imp".into(),
        block_id: None,
        facts: facts(&import, MAIN),
    });
    records.acts.push(SystemActRecord {
        id: "sys".into(),
        block_id: Some(id(1)),
        facts: facts(&system_message("Create the wiki"), MAIN),
    });
    let r = run(&fake, &records);
    assert_eq!(r.findings.len(), 1, "{:?}", r.findings);
    assert!(matches!(
        r.findings.first(),
        Some(Finding::UnrecordedSystemChange { kind: "import", .. })
    ));
    // An act that names a block the history lacks is missing.
    records.acts.push(SystemActRecord {
        id: "ghost".into(),
        block_id: Some(id(77)),
        facts: facts(&import, MAIN),
    });
    let r = run(&fake, &records);
    assert!(r.findings.contains(&Finding::MissingBlock {
        record_id: "ghost".into(),
        block_id: id(77)
    }));
}

#[test]
fn a_long_history_is_read_in_full_and_a_block_in_two_views_counts_once() {
    let w = World::new();
    let create = system_message("Create the wiki");
    // 1,300 accepted proposals after the creation: the first read of 100 comes back full and the read grows.
    let mut chain = Vec::new();
    let mut accepted = Vec::new();
    for n in 2..=1301 {
        let (m, _) = w.proposal(n);
        accepted.push(AcceptedRecord {
            id: format!("acc-{n}"),
            block_id: id(n),
            facts: facts(&m, MAIN),
        });
        chain.insert(0, block(n, &m.to_text(), false));
    }
    chain.push(block(1, &create.to_text(), true));
    // A second view that branched at block 600 and shares everything below it.
    let draft: Vec<HistoryBlock> = chain
        .iter()
        .filter(|b| b.update_seq <= 600)
        .cloned()
        .collect();
    let fake = Fake::new(vec![(MAIN, chain), ("heads/draft", draft)]);
    let records = Records {
        accepted: accepted
            .iter()
            .map(|a| {
                let mut a = a.clone();
                // Blocks up to 600 are in both views; the record names main.
                a.facts.view = MAIN.into();
                a
            })
            .collect(),
        acts: vec![SystemActRecord {
            id: "sys".into(),
            block_id: Some(id(1)),
            facts: facts(&create, MAIN),
        }],
        fail: false,
    };
    let r = run_with(
        &fake,
        &records,
        ReadBounds {
            first_read: 100,
            max_read: 100_000,
        },
    )
    .unwrap();
    assert!(
        r.is_clean(),
        "{} findings, the first {:?}",
        r.findings.len(),
        r.findings.first()
    );
    assert_eq!(r.blocks, 1301, "every block, once");
    assert_eq!(r.views, 2);
    // Main (1,301 blocks): reads of 100, 400 and 1,600, the last short. The draft view (600 blocks): the same
    // three. A full read is never taken for the end.
    assert_eq!(
        *fake.history_calls.lock().unwrap(),
        6,
        "the read grew until it came back short"
    );
}

#[test]
fn a_history_exactly_as_long_as_a_read_is_still_read_in_full() {
    let create = system_message("c");
    let chain: Vec<HistoryBlock> = (1..=100)
        .rev()
        .map(|n| block(n, &create.to_text(), n == 1))
        .collect();
    let fake = Fake::new(vec![(MAIN, chain)]);
    let r = run_with(
        &fake,
        &Records::default(),
        ReadBounds {
            first_read: 100,
            max_read: 10_000,
        },
    )
    .unwrap();
    assert_eq!(r.blocks, 100);
    assert_eq!(
        *fake.history_calls.lock().unwrap(),
        2,
        "a full read is not proof it was the end"
    );
}

#[test]
fn a_history_longer_than_the_most_it_reads_is_an_error_never_a_silent_cut() {
    let create = system_message("c");
    let chain: Vec<HistoryBlock> = (1..=100)
        .rev()
        .map(|n| block(n, &create.to_text(), n == 1))
        .collect();
    let fake = Fake::new(vec![(MAIN, chain)]);
    let e = run_with(
        &fake,
        &Records::default(),
        ReadBounds {
            first_read: 10,
            max_read: 40,
        },
    )
    .unwrap_err();
    assert_eq!(
        e,
        ReconcileError::TooLarge {
            view: MAIN.into(),
            limit: 40
        }
    );
    assert!(e.to_string().contains("nothing was concluded"), "{e}");
}

#[test]
fn a_reconciliation_that_could_not_read_is_an_error_never_a_clean_report() {
    let (mut fake, mut records, _) = clean();
    fake.fail_history = true;
    assert!(matches!(
        reconcile(&WikiId::new(), Path::new("/x"), &fake, &records),
        Err(ReconcileError::Storage(StorageError::Unavailable { .. }))
    ));
    fake.fail_history = false;
    records.fail = true;
    let e = reconcile(&WikiId::new(), Path::new("/x"), &fake, &records).unwrap_err();
    assert!(matches!(e, ReconcileError::Records(_)));
    assert!(e.to_string().contains("records"));
}

#[test]
fn findings_reach_the_audit_trail_as_ids_only_and_a_clean_report_writes_nothing() {
    let (fake, mut records, _) = clean();
    let audit = MemoryAudit::new();
    let clean_report = run(&fake, &records);
    record_findings(&clean_report, &[], &audit, 1).unwrap();
    assert!(audit.events().is_empty());
    records.accepted.remove(0);
    let report = run(&fake, &records);
    record_findings(&report, &[], &audit, 1_700_000_000).unwrap();
    let events = audit.events();
    assert_eq!(events.len(), 1);
    let [e] = events.as_slice() else {
        panic!("one event: {events:?}");
    };
    assert_eq!(
        (e.principal.as_str(), e.action.as_str(), e.outcome),
        ("system:reconciliation", "reconcile", AuditOutcome::Reported)
    );
    assert!(
        e.detail.starts_with("unrecorded-seal block="),
        "{}",
        e.detail
    );
    assert!(e.resource.starts_with("wiki:"));
    // No message text, no names: the detail is a word and ids.
    assert!(!e.detail.contains("Aiko") && !e.detail.contains("Change 1"));
}

#[test]
fn findings_are_in_a_stable_order_without_duplicates() {
    let (mut fake, records, _) = clean();
    let chain = fake.first_chain();
    chain.insert(0, block(9, "z", false));
    chain.insert(0, block(8, "y", false));
    let a = run(&fake, &records).findings;
    let b = run(&fake, &records).findings;
    assert_eq!(a, b);
    let mut sorted = a.clone();
    sorted.sort_by_key(Finding::detail);
    assert_eq!(a, sorted);
    sorted.dedup();
    assert_eq!(a.len(), sorted.len());
}

#[test]
fn a_standing_disagreement_is_reported_once_and_a_new_one_when_it_appears() {
    let (fake, mut records, _) = clean();
    records.accepted.remove(0);
    let first = run(&fake, &records);
    let audit = MemoryAudit::new();
    record_findings(&first, &[], &audit, 10).unwrap();
    assert_eq!(audit.events().len(), 1);
    // The same report again, a day later: nothing new to say.
    let second = run(&fake, &records);
    record_findings(&second, &first.findings, &audit, 90_000).unwrap();
    assert_eq!(
        audit.events().len(),
        1,
        "a schedule must not fill the trail with the same finding"
    );
    // A second record goes missing: only that one is added.
    records.accepted.remove(0);
    let third = run(&fake, &records);
    assert!(
        third.findings.len() > first.findings.len(),
        "{:?}",
        third.findings
    );
    record_findings(&third, &second.findings, &audit, 180_000).unwrap();
    assert_eq!(
        audit.events().len(),
        1 + (third.findings.len() - first.findings.len())
    );
}
