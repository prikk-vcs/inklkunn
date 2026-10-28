//! Reconciliation: the sealed history against the service's records (RFC 002 D-7; threat model C-1b, C-10b,
//! INV-1, INV-10). It answers one question for a wiki: **does every change in the history have a recorded
//! act behind it, and does every recorded act have its change in the history?**
//!
//! - Every block's message must be a patch message (a change made some other way is *unreadable*);
//!   a `proposal` or `rollback` block must be named by an **accepted record** (else an *unrecorded seal*, the
//!   signature of a forged or out-of-band change); an `import` or `system` block must match a **recorded
//!   system act**.
//! - Every accepted record must name a block that exists, in the view the record says, whose message says
//!   the same proposal, proposer, acceptor and base (else *missing* or *mismatch*: an acceptance recorded
//!   as done that is not in the history, or the other way round).
//!
//! **It reads and reports; it never repairs.** It takes a [`Storage`] (read-only by construction) and the
//! records through a trait, returns a [`Report`], and [`record_findings`] appends what it found to the audit
//! trail as events that carry ids and never content. A mismatch is something a person looks at (INV-1,
//! INV-10): acting on it automatically would be the service rewriting its own history.
//!
//! RFC 004 supplies the real acceptance records behind [`RecordedActs`]; the logic here is tested with
//! doubles and, end to end, against a real prikk.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use inklkunn_prikk::{HistoryBlock, Point, Storage, StorageError};
use inklkunn_signer::accept::ExpectedMessage;
use inklkunn_signer::message::Message;
use inklkunn_store::{AuditEvent, AuditOutcome, AuditSink, StoreError, WikiId};

/// How many blocks of a view's history are asked for at a time (the driver's own default is 100).
const PAGE: u64 = 500;

/// An acceptance the service recorded as **accepted**, with the block it says it sealed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedRecord {
    /// The record's id.
    pub id: String,
    /// The block it names.
    pub block_id: String,
    /// What it says was accepted: the same facts the signer is bound to.
    pub facts: ExpectedMessage,
}

/// A change the service made for itself (creating a wiki, an import) that it recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemActRecord {
    /// The record's id.
    pub id: String,
    /// The block it sealed, once known; a record of work not yet sealed has none and is not a finding.
    pub block_id: Option<String>,
    /// What it says was done.
    pub facts: ExpectedMessage,
}

/// The records could not be read. The text is for operators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordsError(pub String);

/// The service's records of acts, read by the reconciliation. RFC 004's acceptance records implement it.
pub trait RecordedActs: Send + Sync {
    /// Every acceptance recorded as accepted for the wiki.
    fn accepted(&self, wiki: &WikiId) -> Result<Vec<AcceptedRecord>, RecordsError>;
    /// Every system act recorded for the wiki.
    fn system_acts(&self, wiki: &WikiId) -> Result<Vec<SystemActRecord>, RecordsError>;
}

/// Why a reconciliation could not be done. It is never "clean".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileError {
    /// The history could not be read.
    Storage(StorageError),
    /// The records could not be read.
    Records(RecordsError),
}

impl std::fmt::Display for ReconcileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReconcileError::Storage(e) => write!(f, "the history could not be read: {e}"),
            ReconcileError::Records(e) => write!(f, "the records could not be read: {}", e.0),
        }
    }
}

impl std::error::Error for ReconcileError {}

/// One disagreement between the history and the records. Every field is an id or a fixed word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// A block whose message is not a patch message: a change that was not made by this service's composer
    /// (or by an older build).
    UnreadableBlock {
        /// The block.
        block_id: String,
    },
    /// A block that says it is an accepted proposal or rollback and that no accepted record names.
    UnrecordedSeal {
        /// The block.
        block_id: String,
        /// The proposal its message names.
        proposal: String,
    },
    /// An `import` or `system` block that no recorded system act matches.
    UnrecordedSystemChange {
        /// The block.
        block_id: String,
        /// `import` or `system`.
        kind: &'static str,
    },
    /// A record that says a change was sealed, naming a block the history does not have.
    MissingBlock {
        /// The record.
        record_id: String,
        /// The block it names.
        block_id: String,
    },
    /// A record and the block it names disagree.
    Mismatch {
        /// The record.
        record_id: String,
        /// The block it names.
        block_id: String,
        /// What differs, in a few fixed words.
        what: &'static str,
    },
}

impl Finding {
    /// The audit detail: a fixed word and ids, nothing else.
    fn detail(&self) -> String {
        match self {
            Finding::UnreadableBlock { block_id } => format!("unreadable-block block={block_id}"),
            Finding::UnrecordedSeal { block_id, proposal } => {
                format!("unrecorded-seal block={block_id} proposal={proposal}")
            }
            Finding::UnrecordedSystemChange { block_id, kind } => {
                format!("unrecorded-system-change block={block_id} kind={kind}")
            }
            Finding::MissingBlock {
                record_id,
                block_id,
            } => {
                format!("missing-block record={record_id} block={block_id}")
            }
            Finding::Mismatch {
                record_id,
                block_id,
                what,
            } => format!("mismatch record={record_id} block={block_id} what={what}"),
        }
    }
}

/// What a reconciliation looked at and found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The wiki.
    pub wiki: WikiId,
    /// Views read.
    pub views: usize,
    /// Distinct blocks examined (a block in two views counts once).
    pub blocks: usize,
    /// Accepted records examined.
    pub accepted: usize,
    /// System act records examined.
    pub system_acts: usize,
    /// Disagreements, in a stable order.
    pub findings: Vec<Finding>,
}

impl Report {
    /// Did the history and the records agree everywhere?
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

/// Each block of the wiki once, with the names of the views whose history holds it.
type BlockIndex = BTreeMap<String, (HistoryBlock, BTreeSet<String>)>;

/// Every block of every open view, once, with the views that hold it.
fn blocks_of(storage: &dyn Storage, repo: &Path) -> Result<(BlockIndex, usize), StorageError> {
    let views = storage.views(repo)?.views;
    let mut blocks = BlockIndex::new();
    for view in &views {
        let mut point = Point::View(view.name.clone());
        loop {
            let history = storage.history(repo, &point, Some(PAGE))?;
            let page = history.blocks.len();
            let mut new = 0;
            let mut oldest: Option<&HistoryBlock> = None;
            for block in &history.blocks {
                let entry = blocks
                    .entry(block.block_id.clone())
                    .or_insert_with(|| (block.clone(), BTreeSet::new()));
                if entry.1.insert(view.name.clone()) {
                    new += 1;
                }
                oldest = Some(block);
            }
            // Done when the page was short, the root was reached, or a page added nothing (so the loop
            // always ends, whatever the storage answers).
            let Some(oldest) = oldest else { break };
            if (page as u64) < PAGE || oldest.previous_ref_state_id.is_none() || new == 0 {
                break;
            }
            point = Point::Block(oldest.block_id.clone());
        }
    }
    Ok((blocks, views.len()))
}

/// Reconcile one wiki's history with its records. Reads only.
pub fn reconcile(
    wiki: &WikiId,
    repo: &Path,
    storage: &dyn Storage,
    records: &dyn RecordedActs,
) -> Result<Report, ReconcileError> {
    let accepted = records.accepted(wiki).map_err(ReconcileError::Records)?;
    let system_acts = records.system_acts(wiki).map_err(ReconcileError::Records)?;
    let (blocks, views) = blocks_of(storage, repo).map_err(ReconcileError::Storage)?;

    let mut findings: Vec<Finding> = Vec::new();
    let mut by_block: BTreeMap<&str, Vec<&AcceptedRecord>> = BTreeMap::new();
    for r in &accepted {
        by_block.entry(r.block_id.as_str()).or_default().push(r);
    }
    let mut acts_by_block: BTreeMap<&str, Vec<&SystemActRecord>> = BTreeMap::new();
    let mut unbound: Vec<&SystemActRecord> = Vec::new();
    for a in &system_acts {
        match a.block_id.as_deref() {
            Some(b) => acts_by_block.entry(b).or_default().push(a),
            None => unbound.push(a),
        }
    }
    let mut claimed_records: BTreeSet<&str> = BTreeSet::new();
    let mut claimed_unbound: BTreeSet<&str> = BTreeSet::new();

    // 1. Every block has a recorded act behind it.
    for (block_id, (block, in_views)) in &blocks {
        if block.patch_messages.is_empty() {
            findings.push(Finding::UnreadableBlock {
                block_id: block_id.clone(),
            });
            continue;
        }
        let mut unreadable = false;
        for pm in &block.patch_messages {
            let Ok(message) = Message::parse(&pm.message) else {
                unreadable = true;
                continue;
            };
            if message.kind().names_a_proposal() {
                let named = message.proposal_id();
                let record = by_block
                    .get(block_id.as_str())
                    .and_then(|rs| rs.iter().find(|r| r.facts.proposal.as_ref() == named));
                match record {
                    None => findings.push(Finding::UnrecordedSeal {
                        block_id: block_id.clone(),
                        proposal: named.map_or_else(String::new, |p| p.to_string()),
                    }),
                    Some(r) => {
                        claimed_records.insert(r.id.as_str());
                        if !r.facts.matches(&message) {
                            findings.push(Finding::Mismatch {
                                record_id: r.id.clone(),
                                block_id: block_id.clone(),
                                what: "the record and the block's message say different things",
                            });
                        } else if !in_views.contains(&r.facts.view) {
                            findings.push(Finding::Mismatch {
                                record_id: r.id.clone(),
                                block_id: block_id.clone(),
                                what: "the block is not in the view the record names",
                            });
                        }
                    }
                }
            } else {
                let bound = acts_by_block
                    .get(block_id.as_str())
                    .and_then(|acts| acts.iter().find(|a| a.facts.matches(&message)))
                    .map(|a| a.id.as_str());
                let found = bound.or_else(|| {
                    unbound
                        .iter()
                        .find(|a| {
                            !claimed_unbound.contains(a.id.as_str()) && a.facts.matches(&message)
                        })
                        .map(|a| a.id.as_str())
                });
                match found {
                    Some(id) => {
                        if bound.is_none() {
                            claimed_unbound.insert(id);
                        }
                    }
                    None => findings.push(Finding::UnrecordedSystemChange {
                        block_id: block_id.clone(),
                        kind: message.kind().as_str(),
                    }),
                }
            }
        }
        if unreadable {
            findings.push(Finding::UnreadableBlock {
                block_id: block_id.clone(),
            });
        }
    }

    // 2. Every recorded act has its block.
    for r in &accepted {
        if !blocks.contains_key(&r.block_id) {
            findings.push(Finding::MissingBlock {
                record_id: r.id.clone(),
                block_id: r.block_id.clone(),
            });
        } else if !claimed_records.contains(r.id.as_str()) {
            findings.push(Finding::Mismatch {
                record_id: r.id.clone(),
                block_id: r.block_id.clone(),
                what: "the block's message names no such proposal",
            });
        }
    }
    for a in &system_acts {
        if let Some(b) = &a.block_id
            && !blocks.contains_key(b)
        {
            findings.push(Finding::MissingBlock {
                record_id: a.id.clone(),
                block_id: b.clone(),
            });
        }
    }
    findings.sort_by_key(|f| f.detail());
    findings.dedup();
    Ok(Report {
        wiki: wiki.clone(),
        views,
        blocks: blocks.len(),
        accepted: accepted.len(),
        system_acts: system_acts.len(),
        findings,
    })
}

/// Append one `reported` event per finding to the audit trail (nothing for a clean report): the wiki, a
/// fixed word and ids. It changes nothing else.
pub fn record_findings(report: &Report, audit: &dyn AuditSink, at: i64) -> Result<(), StoreError> {
    for f in &report.findings {
        audit.record(&AuditEvent::new(
            at,
            "system:reconciliation",
            "reconcile",
            &format!("wiki:{}", report.wiki),
            AuditOutcome::Reported,
            &f.detail(),
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
