//! The SQLite proposal records (schema versions 9 and 10). Every write is one immediate transaction, so a check and
//! the change that follows it cannot be separated by another connection; the tables whose rows are written
//! once carry the triggers of [`V9`](super::V9) as a second wall.

use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::{SqliteStore, backend};
use crate::proposal::{
    Anchor, CommentRecord, DecisionKind, DecisionRecord, ProposalChange, ProposalFilter,
    ProposalOp, ProposalRecord, ProposalState, ProposalStore, ProposalTransition, RevisionContent,
    RevisionFlags, RevisionRecord,
};
use crate::record::{AccountId, CommentId, DecisionId, ProposalId, WikiId};
use crate::{Result, StoreError};

pub(super) fn invalid(what: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(0, what.to_owned(), rusqlite::types::Type::Text)
}

/// Does a row exist? One probe with up to two text arguments, inside the caller's transaction.
pub(super) fn probe(conn: &Connection, sql: &str, args: &[&str]) -> Result<bool> {
    conn.query_row(sql, rusqlite::params_from_iter(args), |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
        .map_err(backend)
}

pub(super) fn to_json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|e| StoreError::Backend(e.to_string()))
}

pub(super) fn from_json<T: DeserializeOwned>(text: &str, what: &str) -> rusqlite::Result<T> {
    serde_json::from_str(text).map_err(|_| invalid(what))
}

/// A write that a write-once rule or a unique key refused: the record is already there. The triggers say
/// `append-only`; a unique key says `UNIQUE constraint failed`. Anything else is the backend's.
pub(super) fn write_refusal(e: rusqlite::Error) -> StoreError {
    if let rusqlite::Error::SqliteFailure(code, msg) = &e
        && code.code == rusqlite::ErrorCode::ConstraintViolation
        && msg
            .as_deref()
            .is_some_and(|m| m.contains("append-only") || m.contains("UNIQUE constraint failed"))
    {
        return StoreError::AlreadyRecorded;
    }
    backend(e)
}

pub(super) fn count(n: i64) -> u64 {
    u64::try_from(n).unwrap_or(0)
}

const PROPOSAL_COLUMNS: &str = "id, wiki_id, view, state, author, summary, sources, superseded_by, \
                                created_at, updated_at, submitted_at, closed_at";

fn row_to_proposal(row: &Row<'_>) -> rusqlite::Result<ProposalRecord> {
    let id: String = row.get(0)?;
    let wiki: String = row.get(1)?;
    let state: String = row.get(3)?;
    let author: String = row.get(4)?;
    let sources: String = row.get(6)?;
    let superseded_by: Option<String> = row.get(7)?;
    Ok(ProposalRecord {
        id: ProposalId::parse(&id).ok_or_else(|| invalid("id"))?,
        wiki: WikiId::parse(&wiki).ok_or_else(|| invalid("wiki_id"))?,
        view: row.get(2)?,
        state: ProposalState::parse(&state).ok_or_else(|| invalid("state"))?,
        author: AccountId::parse(&author).ok_or_else(|| invalid("author"))?,
        summary: row.get(5)?,
        sources: from_json(&sources, "sources")?,
        superseded_by: superseded_by
            .map(|s| ProposalId::parse(&s).ok_or_else(|| invalid("superseded_by")))
            .transpose()?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        submitted_at: row.get(10)?,
        closed_at: row.get(11)?,
    })
}

fn row_to_revision(row: &Row<'_>) -> rusqlite::Result<RevisionRecord> {
    let id: String = row.get(0)?;
    let content: String = row.get(2)?;
    Ok(RevisionRecord {
        proposal: ProposalId::parse(&id).ok_or_else(|| invalid("proposal_id"))?,
        seq: row.get(1)?,
        content: from_json(&content, "revision content")?,
        created_at: row.get(3)?,
    })
}

fn row_to_transition(row: &Row<'_>) -> rusqlite::Result<ProposalTransition> {
    let id: String = row.get(0)?;
    let op: String = row.get(1)?;
    let from: Option<String> = row.get(2)?;
    let to: String = row.get(3)?;
    let actor: String = row.get(4)?;
    Ok(ProposalTransition {
        proposal: ProposalId::parse(&id).ok_or_else(|| invalid("proposal_id"))?,
        op: ProposalOp::parse(&op).ok_or_else(|| invalid("op"))?,
        from: from
            .map(|s| ProposalState::parse(&s).ok_or_else(|| invalid("from_state")))
            .transpose()?,
        to: ProposalState::parse(&to).ok_or_else(|| invalid("to_state"))?,
        actor: AccountId::parse(&actor).ok_or_else(|| invalid("actor"))?,
        reason: row.get(5)?,
        at: row.get(6)?,
    })
}

fn row_to_comment(row: &Row<'_>) -> rusqlite::Result<CommentRecord> {
    let id: String = row.get(0)?;
    let proposal: String = row.get(1)?;
    let author: String = row.get(3)?;
    let parent: Option<String> = row.get(4)?;
    let anchor: Option<String> = row.get(6)?;
    Ok(CommentRecord {
        id: CommentId::parse(&id).ok_or_else(|| invalid("id"))?,
        proposal: ProposalId::parse(&proposal).ok_or_else(|| invalid("proposal_id"))?,
        revision: row.get(2)?,
        author: AccountId::parse(&author).ok_or_else(|| invalid("author"))?,
        parent: parent
            .map(|s| CommentId::parse(&s).ok_or_else(|| invalid("parent")))
            .transpose()?,
        body: row.get(5)?,
        anchor: anchor
            .map(|s| from_json::<Anchor>(&s, "anchor"))
            .transpose()?,
        created_at: row.get(7)?,
    })
}

fn row_to_decision(row: &Row<'_>) -> rusqlite::Result<DecisionRecord> {
    let id: String = row.get(0)?;
    let proposal: String = row.get(1)?;
    let reviewer: String = row.get(3)?;
    let kind: String = row.get(4)?;
    Ok(DecisionRecord {
        id: DecisionId::parse(&id).ok_or_else(|| invalid("id"))?,
        proposal: ProposalId::parse(&proposal).ok_or_else(|| invalid("proposal_id"))?,
        revision: row.get(2)?,
        reviewer: AccountId::parse(&reviewer).ok_or_else(|| invalid("reviewer"))?,
        kind: DecisionKind::parse(&kind).ok_or_else(|| invalid("kind"))?,
        body: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn unreadable(what: &str) -> StoreError {
    StoreError::Backend(format!("a stored {what} could not be read"))
}

/// The state of a proposal, or `NotFound`.
fn state_of(conn: &Connection, id: &ProposalId) -> Result<ProposalState> {
    let stored: Option<String> = conn
        .query_row(
            "SELECT state FROM proposals WHERE id = ?1",
            params![id.as_str()],
            |r| r.get(0),
        )
        .optional()
        .map_err(backend)?;
    let stored = stored.ok_or(StoreError::NotFound)?;
    ProposalState::parse(&stored).ok_or_else(|| unreadable("proposal state"))
}

fn latest_revision(conn: &Connection, id: &ProposalId) -> Result<u32> {
    let n: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) FROM proposal_revisions WHERE proposal_id = ?1",
            params![id.as_str()],
            |r| r.get(0),
        )
        .map_err(backend)?;
    u32::try_from(n).map_err(|_| unreadable("revision number"))
}

fn account_known(conn: &Connection, id: &AccountId) -> Result<bool> {
    probe(conn, "SELECT 1 FROM accounts WHERE id = ?1", &[id.as_str()])
}

fn insert_revision(conn: &Connection, r: &RevisionRecord) -> Result<()> {
    conn.execute(
        "INSERT INTO proposal_revisions (proposal_id, seq, content, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![
            r.proposal.as_str(),
            r.seq,
            to_json(&r.content)?,
            r.created_at
        ],
    )
    .map(|_| ())
    .map_err(write_refusal)
}

fn insert_transition(conn: &Connection, t: &ProposalTransition) -> Result<()> {
    conn.execute(
        "INSERT INTO proposal_transitions (proposal_id, op, from_state, to_state, actor, reason, at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            t.proposal.as_str(),
            t.op.as_str(),
            t.from.map(ProposalState::as_str),
            t.to.as_str(),
            t.actor.as_str(),
            t.reason,
            t.at
        ],
    )
    .map(|_| ())
    .map_err(write_refusal)
}

/// Is `id` a block id as prikk prints it: 64 lower-case hex characters?
fn is_block_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

impl SqliteStore {
    /// Create a proposal as a draft, and, for a rollback, the note of the block it undoes, in one transaction.
    fn create_with(
        &self,
        proposal: &ProposalRecord,
        draft: &RevisionContent,
        created: &ProposalTransition,
        undoes: Option<&str>,
    ) -> Result<()> {
        if undoes.is_some_and(|u| !is_block_id(u)) {
            return Err(StoreError::Backend(
                "a rollback names the block it undoes by its 64-digit id".to_owned(),
            ));
        }
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !probe(
                &tx,
                "SELECT 1 FROM wikis WHERE id = ?1",
                &[proposal.wiki.as_str()],
            )? {
                return Err(StoreError::NotFound);
            }
            if Self::proposal_known(&tx, &proposal.id)? {
                return Err(StoreError::IdTaken(proposal.id.to_string()));
            }
            if !account_known(&tx, &proposal.author)? || !account_known(&tx, &created.actor)? {
                return Err(StoreError::AccountNotFound);
            }
            tx.execute(
                "INSERT INTO proposals (id, seq, wiki_id, view, state, author, summary, sources, \
                 superseded_by, created_at, updated_at, submitted_at, closed_at) \
                 VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM proposals), ?2, ?3, ?4, ?5, ?6, ?7, \
                 NULL, ?8, ?9, ?10, ?11)",
                params![
                    proposal.id.as_str(),
                    proposal.wiki.as_str(),
                    proposal.view,
                    proposal.state.as_str(),
                    proposal.author.as_str(),
                    proposal.summary,
                    to_json(&proposal.sources)?,
                    proposal.created_at,
                    proposal.updated_at,
                    proposal.submitted_at,
                    proposal.closed_at,
                ],
            )
            .map_err(backend)?;
            tx.execute(
                "INSERT INTO proposal_drafts (proposal_id, content) VALUES (?1, ?2)",
                params![proposal.id.as_str(), to_json(draft)?],
            )
            .map_err(backend)?;
            if let Some(undoes) = undoes {
                tx.execute(
                    "INSERT INTO proposal_rollbacks (proposal_id, undoes) VALUES (?1, ?2)",
                    params![proposal.id.as_str(), undoes],
                )
                .map_err(write_refusal)?;
            }
            insert_transition(&tx, created)?;
            tx.commit().map_err(backend)
        })
    }

    /// Is there a proposal with this id (whoever may see it)?
    pub(super) fn proposal_known(conn: &Connection, id: &ProposalId) -> Result<bool> {
        probe(
            conn,
            "SELECT 1 FROM proposals WHERE id = ?1",
            &[id.as_str()],
        )
    }
}

impl ProposalStore for SqliteStore {
    fn create_proposal(
        &self,
        proposal: &ProposalRecord,
        draft: &RevisionContent,
        created: &ProposalTransition,
    ) -> Result<()> {
        self.create_with(proposal, draft, created, None)
    }

    fn create_rollback_proposal(
        &self,
        proposal: &ProposalRecord,
        draft: &RevisionContent,
        created: &ProposalTransition,
        undoes: &str,
    ) -> Result<()> {
        self.create_with(proposal, draft, created, Some(undoes))
    }

    fn rollback_of(&self, id: &ProposalId, viewer: Option<&AccountId>) -> Result<Option<String>> {
        self.with(|conn| {
            conn.query_row(
                "SELECT r.undoes FROM proposal_rollbacks r JOIN proposals p ON p.id = r.proposal_id \
                 WHERE r.proposal_id = ?1 AND (p.submitted_at IS NOT NULL OR p.author = ?2)",
                params![id.as_str(), viewer.map(AccountId::as_str)],
                |r| r.get(0),
            )
            .optional()
            .map_err(backend)
        })
    }

    fn revision_flags(&self, ids: &[ProposalId]) -> Result<Vec<(ProposalId, RevisionFlags)>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT COALESCE(json_extract(content, '$.minor'), 0), \
                            COALESCE(json_extract(content, '$.automated'), 0) \
                     FROM proposal_revisions WHERE proposal_id = ?1 \
                     ORDER BY seq DESC LIMIT 1",
                )
                .map_err(backend)?;
            let mut out = Vec::new();
            for id in ids {
                let flags = stmt
                    .query_row(params![id.as_str()], |r| {
                        Ok(RevisionFlags {
                            minor: r.get::<_, i64>(0)? != 0,
                            automated: r.get::<_, i64>(1)? != 0,
                        })
                    })
                    .optional()
                    .map_err(backend)?;
                if let Some(flags) = flags {
                    out.push((id.clone(), flags));
                }
            }
            Ok(out)
        })
    }

    fn proposal(
        &self,
        id: &ProposalId,
        viewer: Option<&AccountId>,
    ) -> Result<Option<ProposalRecord>> {
        self.with(|conn| {
            conn.query_row(
                &format!(
                    "SELECT {PROPOSAL_COLUMNS} FROM proposals \
                     WHERE id = ?1 AND (submitted_at IS NOT NULL OR author = ?2)"
                ),
                params![id.as_str(), viewer.map(AccountId::as_str)],
                row_to_proposal,
            )
            .optional()
            .map_err(backend)
        })
    }

    fn proposals(
        &self,
        wiki: &WikiId,
        filter: &ProposalFilter,
        viewer: Option<&AccountId>,
    ) -> Result<Vec<ProposalRecord>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {PROPOSAL_COLUMNS} FROM proposals \
                     WHERE wiki_id = ?1 AND (submitted_at IS NOT NULL OR author = ?2) ORDER BY seq"
                ))
                .map_err(backend)?;
            let rows = stmt
                .query_map(
                    params![wiki.as_str(), viewer.map(AccountId::as_str)],
                    row_to_proposal,
                )
                .map_err(backend)?;
            let all = rows
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| unreadable("proposal"))?;
            Ok(all
                .into_iter()
                .filter(|p| filter.states.as_ref().is_none_or(|s| s.contains(&p.state)))
                .filter(|p| filter.author.as_ref().is_none_or(|a| *a == p.author))
                .collect())
        })
    }

    fn draft(
        &self,
        id: &ProposalId,
        viewer: Option<&AccountId>,
    ) -> Result<Option<RevisionContent>> {
        self.with(|conn| {
            let stored: Option<String> = conn
                .query_row(
                    "SELECT d.content FROM proposal_drafts d JOIN proposals p ON p.id = d.proposal_id \
                     WHERE d.proposal_id = ?1 AND p.author = ?2",
                    params![id.as_str(), viewer.map(AccountId::as_str)],
                    |r| r.get(0),
                )
                .optional()
                .map_err(backend)?;
            stored
                .map(|s| from_json(&s, "draft").map_err(|_| unreadable("draft")))
                .transpose()
        })
    }

    fn save_draft(
        &self,
        id: &ProposalId,
        summary: &str,
        sources: &[String],
        draft: &RevisionContent,
        at: i64,
    ) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if state_of(&tx, id)? != ProposalState::Draft {
                return Err(StoreError::StateChanged);
            }
            tx.execute(
                "UPDATE proposals SET summary = ?1, sources = ?2, updated_at = ?3 WHERE id = ?4",
                params![summary, to_json(&sources)?, at, id.as_str()],
            )
            .map_err(backend)?;
            tx.execute(
                "INSERT INTO proposal_drafts (proposal_id, content) VALUES (?1, ?2) \
                 ON CONFLICT (proposal_id) DO UPDATE SET content = excluded.content",
                params![id.as_str(), to_json(draft)?],
            )
            .map_err(backend)?;
            tx.commit().map_err(backend)
        })
    }

    fn change_proposal(&self, change: &ProposalChange) -> Result<ProposalRecord> {
        let t = &change.transition;
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !account_known(&tx, &t.actor)? {
                // The proposal is looked for first, as every store answers.
                state_of(&tx, &t.proposal)?;
                return Err(StoreError::AccountNotFound);
            }
            let state = state_of(&tx, &t.proposal)?;
            if Some(state) != t.from {
                return Err(StoreError::StateChanged);
            }
            if let Some(r) = &change.revision
                && (r.proposal != t.proposal || r.seq != latest_revision(&tx, &t.proposal)? + 1)
            {
                return Err(StoreError::StateChanged);
            }
            if let Some(by) = &change.superseded_by
                && !Self::proposal_known(&tx, by)?
            {
                return Err(StoreError::NotFound);
            }
            if let Some(r) = &change.revision {
                insert_revision(&tx, r)?;
            }
            if t.op == ProposalOp::Submit {
                tx.execute(
                    "DELETE FROM proposal_drafts WHERE proposal_id = ?1",
                    params![t.proposal.as_str()],
                )
                .map_err(backend)?;
            }
            insert_transition(&tx, t)?;
            if let Some((summary, sources)) = &change.text {
                tx.execute(
                    "UPDATE proposals SET summary = ?1, sources = ?2 WHERE id = ?3",
                    params![summary, to_json(sources)?, t.proposal.as_str()],
                )
                .map_err(backend)?;
            }
            tx.execute(
                "UPDATE proposals SET state = ?1, updated_at = ?2, \
                 submitted_at = CASE WHEN ?3 THEN COALESCE(submitted_at, ?2) ELSE submitted_at END, \
                 closed_at = CASE WHEN ?4 THEN ?2 ELSE closed_at END, \
                 superseded_by = COALESCE(?5, superseded_by) WHERE id = ?6",
                params![
                    t.to.as_str(),
                    t.at,
                    t.op == ProposalOp::Submit,
                    !t.to.is_open(),
                    change.superseded_by.as_ref().map(ProposalId::as_str),
                    t.proposal.as_str(),
                ],
            )
            .map_err(backend)?;
            let record = tx
                .query_row(
                    &format!("SELECT {PROPOSAL_COLUMNS} FROM proposals WHERE id = ?1"),
                    params![t.proposal.as_str()],
                    row_to_proposal,
                )
                .map_err(backend)?;
            tx.commit().map_err(backend)?;
            Ok(record)
        })
    }

    fn revisions(
        &self,
        id: &ProposalId,
        viewer: Option<&AccountId>,
    ) -> Result<Vec<RevisionRecord>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT r.proposal_id, r.seq, r.content, r.created_at FROM proposal_revisions r \
                     JOIN proposals p ON p.id = r.proposal_id \
                     WHERE r.proposal_id = ?1 AND (p.submitted_at IS NOT NULL OR p.author = ?2) \
                     ORDER BY r.seq",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(
                    params![id.as_str(), viewer.map(AccountId::as_str)],
                    row_to_revision,
                )
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| unreadable("revision"))
        })
    }

    fn transitions(
        &self,
        id: &ProposalId,
        viewer: Option<&AccountId>,
    ) -> Result<Vec<ProposalTransition>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT t.proposal_id, t.op, t.from_state, t.to_state, t.actor, t.reason, t.at \
                     FROM proposal_transitions t JOIN proposals p ON p.id = t.proposal_id \
                     WHERE t.proposal_id = ?1 AND (p.submitted_at IS NOT NULL OR p.author = ?2) \
                     ORDER BY t.seq",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(
                    params![id.as_str(), viewer.map(AccountId::as_str)],
                    row_to_transition,
                )
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| unreadable("transition"))
        })
    }

    fn add_comment(&self, comment: &CommentRecord, states: &[ProposalState]) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            let state = state_of(&tx, &comment.proposal)?;
            if !account_known(&tx, &comment.author)? {
                return Err(StoreError::AccountNotFound);
            }
            if !states.contains(&state)
                || comment.revision != latest_revision(&tx, &comment.proposal)?
            {
                return Err(StoreError::StateChanged);
            }
            if let Some(parent) = &comment.parent
                && !probe(
                    &tx,
                    "SELECT 1 FROM proposal_comments WHERE id = ?1 AND proposal_id = ?2",
                    &[parent.as_str(), comment.proposal.as_str()],
                )?
            {
                return Err(StoreError::NotFound);
            }
            let anchor = comment.anchor.as_ref().map(to_json).transpose()?;
            tx.execute(
                "INSERT INTO proposal_comments (id, seq, proposal_id, revision, author, parent, body, \
                 anchor, created_at) VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM proposal_comments), \
                 ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    comment.id.as_str(),
                    comment.proposal.as_str(),
                    comment.revision,
                    comment.author.as_str(),
                    comment.parent.as_ref().map(CommentId::as_str),
                    comment.body,
                    anchor,
                    comment.created_at
                ],
            )
            .map_err(write_refusal)?;
            tx.commit().map_err(backend)
        })
    }

    fn comments(&self, id: &ProposalId, viewer: Option<&AccountId>) -> Result<Vec<CommentRecord>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT c.id, c.proposal_id, c.revision, c.author, c.parent, c.body, c.anchor, \
                     c.created_at FROM proposal_comments c JOIN proposals p ON p.id = c.proposal_id \
                     WHERE c.proposal_id = ?1 AND (p.submitted_at IS NOT NULL OR p.author = ?2) \
                     ORDER BY c.seq",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(
                    params![id.as_str(), viewer.map(AccountId::as_str)],
                    row_to_comment,
                )
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| unreadable("comment"))
        })
    }

    fn add_decision(&self, decision: &DecisionRecord) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            let state = state_of(&tx, &decision.proposal)?;
            if !account_known(&tx, &decision.reviewer)? {
                return Err(StoreError::AccountNotFound);
            }
            if state != ProposalState::HeldForReview
                || decision.revision != latest_revision(&tx, &decision.proposal)?
            {
                return Err(StoreError::StateChanged);
            }
            tx.execute(
                "INSERT INTO proposal_decisions (id, seq, proposal_id, revision, reviewer, kind, body, \
                 created_at) VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM proposal_decisions), \
                 ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    decision.id.as_str(),
                    decision.proposal.as_str(),
                    decision.revision,
                    decision.reviewer.as_str(),
                    decision.kind.as_str(),
                    decision.body,
                    decision.created_at
                ],
            )
            .map_err(write_refusal)?;
            tx.commit().map_err(backend)
        })
    }

    fn decisions(
        &self,
        id: &ProposalId,
        viewer: Option<&AccountId>,
    ) -> Result<Vec<DecisionRecord>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT d.id, d.proposal_id, d.revision, d.reviewer, d.kind, d.body, d.created_at \
                     FROM proposal_decisions d JOIN proposals p ON p.id = d.proposal_id \
                     WHERE d.proposal_id = ?1 AND (p.submitted_at IS NOT NULL OR p.author = ?2) \
                     ORDER BY d.seq",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(
                    params![id.as_str(), viewer.map(AccountId::as_str)],
                    row_to_decision,
                )
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| unreadable("decision"))
        })
    }

    fn wiki_decisions(&self, wiki: &WikiId) -> Result<Vec<DecisionRecord>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT d.id, d.proposal_id, d.revision, d.reviewer, d.kind, d.body, d.created_at \
                     FROM proposal_decisions d JOIN proposals p ON p.id = d.proposal_id \
                     WHERE p.wiki_id = ?1 AND p.submitted_at IS NOT NULL ORDER BY d.seq",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![wiki.as_str()], row_to_decision)
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| unreadable("decision"))
        })
    }

    fn open_count(&self, wiki: &WikiId, author: &AccountId) -> Result<u64> {
        self.with(|conn| {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM proposals WHERE wiki_id = ?1 AND author = ?2 \
                     AND state IN ('draft', 'held-for-review', 'needs-revision')",
                    params![wiki.as_str(), author.as_str()],
                    |r| r.get(0),
                )
                .map_err(backend)?;
            Ok(count(n))
        })
    }

    fn opened_since(
        &self,
        wiki: &WikiId,
        author: Option<&AccountId>,
        since: i64,
    ) -> Result<Vec<i64>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT created_at FROM proposals WHERE wiki_id = ?1 AND created_at >= ?2 \
                     AND (?3 IS NULL OR author = ?3) ORDER BY created_at",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(
                    params![wiki.as_str(), since, author.map(AccountId::as_str)],
                    |r| r.get::<_, i64>(0),
                )
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(backend)
        })
    }
}
