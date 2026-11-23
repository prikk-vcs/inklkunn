//! The SQLite records of governance and moderation, and the derived governance snapshot (schema version 19; RFC 006,
//! handoff 006 G-T1). The tables, their `CHECK`s and their triggers are [`V19`](super::V19)'s; the meaning of the records
//! is [`crate::governance`]'s. Every write runs in one `IMMEDIATE` transaction: the checks that give a refusal its plain
//! name (the wiki and the accounts are there, an id is used once, an event has no follower yet) are made inside it, and
//! the database's own `CHECK`s and triggers say the rest from any connection.

use rusqlite::{OptionalExtension, Row, Transaction, TransactionBehavior, params};

use super::proposal::{count, invalid, probe};
use super::{SqliteStore, backend};
use crate::access::Role;
use crate::governance::{
    Actor, AppealDecision, AppealOutcome, AppealRecord, AppealState, AppealedAction, AppealedKind,
    Decided, GovernanceFault, GovernanceSnapshot, GovernanceSnapshotStore, GovernanceStore,
    GrantDecisionRecord, HiddenItem, HiddenItemKind, HiddenParts, MAX_PAGE, MeasureEvent,
    ProtectionEvent, ProtectionKind, ProtectionRequirement, ReasonCategory, RestrictionEvent,
    Reversal, TombstoneAct, TombstoneEvent,
};
use crate::record::{
    AccountId, AppealId, GrantDecisionId, ProtectionEventId, RestrictionEventId, TombstoneEventId,
    WikiId,
};
use crate::{Result, StoreError};

/// A write the database refused with a rule's own words (`… are append-only: <the rule>`) is a broken rule, said so; a
/// bare `append-only` or a unique key is a record that is already there. Anything else is the backend's.
fn refusal(e: rusqlite::Error) -> StoreError {
    if let rusqlite::Error::SqliteFailure(code, Some(msg)) = &e
        && code.code == rusqlite::ErrorCode::ConstraintViolation
    {
        if let Some((_, rule)) = msg.split_once("append-only: ") {
            return StoreError::Backend(rule.to_owned());
        }
        if msg.contains("append-only") || msg.contains("UNIQUE constraint failed") {
            return StoreError::AlreadyRecorded;
        }
        if msg.contains("CHECK constraint failed") {
            return StoreError::Backend(format!("the database refuses the record: {msg}"));
        }
    }
    backend(e)
}

fn limit_of(limit: usize) -> i64 {
    i64::try_from(limit.clamp(1, MAX_PAGE)).unwrap_or(i64::MAX)
}

fn require_wiki(tx: &Transaction<'_>, wiki: &WikiId) -> Result<()> {
    if probe(tx, "SELECT 1 FROM wikis WHERE id = ?1", &[wiki.as_str()])? {
        Ok(())
    } else {
        Err(StoreError::NotFound)
    }
}

fn require_account(tx: &Transaction<'_>, account: &AccountId) -> Result<()> {
    if probe(
        tx,
        "SELECT 1 FROM accounts WHERE id = ?1",
        &[account.as_str()],
    )? {
        Ok(())
    } else {
        Err(StoreError::AccountNotFound)
    }
}

fn require_actor(tx: &Transaction<'_>, by: Option<&Actor>) -> Result<()> {
    by.map_or(Ok(()), |a| require_account(tx, &a.account))
}

/// A record that must exist for this one to name it.
fn require_row(tx: &Transaction<'_>, table: &str, id: &str) -> Result<()> {
    if probe(tx, &format!("SELECT 1 FROM {table} WHERE id = ?1"), &[id])? {
        Ok(())
    } else {
        Err(StoreError::NotFound)
    }
}

/// An id used once: `AlreadyRecorded` when `table` has it.
fn require_new(tx: &Transaction<'_>, table: &str, id: &str) -> Result<()> {
    if probe(tx, &format!("SELECT 1 FROM {table} WHERE id = ?1"), &[id])? {
        Err(StoreError::AlreadyRecorded)
    } else {
        Ok(())
    }
}

/// An event is followed by at most one: `AlreadyRecorded` when something follows `of_event` in `table` already.
fn require_unfollowed(tx: &Transaction<'_>, table: &str, of_event: Option<&str>) -> Result<()> {
    let Some(of_event) = of_event else {
        return Ok(());
    };
    if probe(
        tx,
        &format!("SELECT 1 FROM {table} WHERE of_event = ?1"),
        &[of_event],
    )? {
        Err(StoreError::AlreadyRecorded)
    } else {
        Ok(())
    }
}

fn role(text: &str, what: &str) -> rusqlite::Result<Role> {
    Role::parse(text).ok_or_else(|| invalid(what))
}

fn opt_role(text: Option<String>, what: &str) -> rusqlite::Result<Option<Role>> {
    text.map(|t| role(&t, what)).transpose()
}

fn account_of(text: String, what: &str) -> rusqlite::Result<AccountId> {
    AccountId::parse(&text).ok_or_else(|| invalid(what))
}

fn opt_account(text: Option<String>, what: &str) -> rusqlite::Result<Option<AccountId>> {
    text.map(|t| account_of(t, what)).transpose()
}

fn wiki_of(text: String) -> rusqlite::Result<WikiId> {
    WikiId::parse(&text).ok_or_else(|| invalid("wiki id"))
}

fn appeal_of(text: Option<String>) -> rusqlite::Result<Option<AppealId>> {
    text.map(|t| AppealId::parse(&t).ok_or_else(|| invalid("appeal id")))
        .transpose()
}

/// The actor columns of a record that always has one.
fn actor_of(account: String, rung: String) -> rusqlite::Result<Actor> {
    Ok(Actor {
        account: account_of(account, "actor")?,
        rung: role(&rung, "actor rung")?,
    })
}

// ------------------------------------------------------------------------------------------------ grant decisions

const GRANT_COLUMNS: &str = "id, wiki_id, at, actor, actor_rung, policy_block, target, rung_before, rung_after, reason, appeal_id";

fn grant_of(row: &Row<'_>) -> rusqlite::Result<GrantDecisionRecord> {
    let actor: Option<String> = row.get(3)?;
    let rung: Option<String> = row.get(4)?;
    Ok(GrantDecisionRecord {
        id: GrantDecisionId::parse(&row.get::<_, String>(0)?).ok_or_else(|| invalid("id"))?,
        wiki: wiki_of(row.get(1)?)?,
        at: row.get(2)?,
        by: match (actor, rung) {
            (None, None) => None,
            (Some(a), Some(r)) => Some(actor_of(a, r)?),
            _ => return Err(invalid("actor")),
        },
        policy_block: row.get(5)?,
        target: account_of(row.get(6)?, "target")?,
        before: opt_role(row.get(7)?, "rung before")?,
        after: opt_role(row.get(8)?, "rung after")?,
        reason: row.get(9)?,
        appeal: appeal_of(row.get(10)?)?,
    })
}

fn insert_grant_decision(tx: &Transaction<'_>, d: &GrantDecisionRecord) -> Result<()> {
    d.validate()?;
    require_wiki(tx, &d.wiki)?;
    require_actor(tx, d.by.as_ref())?;
    require_account(tx, &d.target)?;
    require_new(tx, "grant_decisions", d.id.as_str())?;
    if let Some(appeal) = &d.appeal {
        require_row(tx, "appeals", appeal.as_str())?;
    }
    // The decision says what the rung was; it applies only to a grant that still is that.
    let held: Option<String> = tx
        .query_row(
            "SELECT role FROM role_grants WHERE wiki_id = ?1 AND account_id = ?2",
            params![d.wiki.as_str(), d.target.as_str()],
            |r| r.get(0),
        )
        .optional()
        .map_err(backend)?;
    if held.as_deref() != d.before.map(Role::as_str) {
        return Err(StoreError::StateChanged);
    }
    tx.execute(
        "INSERT INTO grant_decisions (id, seq, wiki_id, at, actor, actor_rung, policy_block, target, rung_before, \
         rung_after, reason, appeal_id) VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM grant_decisions), ?2, ?3, \
         ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            d.id.as_str(),
            d.wiki.as_str(),
            d.at,
            d.by.as_ref().map(|a| a.account.as_str()),
            d.by.as_ref().map(|a| a.rung.as_str()),
            d.policy_block,
            d.target.as_str(),
            d.before.map(Role::as_str),
            d.after.map(Role::as_str),
            d.reason,
            d.appeal.as_ref().map(AppealId::as_str),
        ],
    )
    .map_err(refusal)?;
    match d.after {
        Some(after) => tx.execute(
            "INSERT INTO role_grants (wiki_id, account_id, role, granted_by, granted_at, seq) \
             VALUES (?1, ?2, ?3, ?4, ?5, (SELECT COALESCE(MAX(seq), 0) + 1 FROM role_grants)) \
             ON CONFLICT (wiki_id, account_id) DO UPDATE SET role = excluded.role, \
             granted_by = excluded.granted_by, granted_at = excluded.granted_at, seq = excluded.seq",
            params![
                d.wiki.as_str(),
                d.target.as_str(),
                after.as_str(),
                d.by.as_ref().map(|a| a.account.as_str()),
                d.at,
            ],
        ),
        None => tx.execute(
            "DELETE FROM role_grants WHERE wiki_id = ?1 AND account_id = ?2",
            params![d.wiki.as_str(), d.target.as_str()],
        ),
    }
    .map_err(backend)?;
    Ok(())
}

// ------------------------------------------------------------------------------------------------ restrictions

const RESTRICTION_COLUMNS: &str = "id, wiki_id, at, event, account, actor, actor_rung, policy_block, reason, \
                                   expires_at, of_event, appeal_id";

fn restriction_of(row: &Row<'_>) -> rusqlite::Result<RestrictionEvent> {
    Ok(RestrictionEvent {
        id: RestrictionEventId::parse(&row.get::<_, String>(0)?).ok_or_else(|| invalid("id"))?,
        wiki: wiki_of(row.get(1)?)?,
        at: row.get(2)?,
        event: MeasureEvent::parse(&row.get::<_, String>(3)?).ok_or_else(|| invalid("event"))?,
        account: account_of(row.get(4)?, "account")?,
        by: actor_of(row.get(5)?, row.get(6)?)?,
        policy_block: row.get(7)?,
        reason: row.get(8)?,
        expires_at: row.get(9)?,
        of_event: row
            .get::<_, Option<String>>(10)?
            .map(|t| RestrictionEventId::parse(&t).ok_or_else(|| invalid("of_event")))
            .transpose()?,
        appeal: appeal_of(row.get(11)?)?,
    })
}

fn insert_restriction(tx: &Transaction<'_>, e: &RestrictionEvent) -> Result<()> {
    e.validate()?;
    require_wiki(tx, &e.wiki)?;
    require_account(tx, &e.account)?;
    require_account(tx, &e.by.account)?;
    require_new(tx, "restrictions", e.id.as_str())?;
    require_unfollowed(tx, "restrictions", e.of_event.as_ref().map(|i| i.as_str()))?;
    if let Some(of) = &e.of_event {
        require_row(tx, "restrictions", of.as_str())?;
    }
    if let Some(appeal) = &e.appeal {
        require_row(tx, "appeals", appeal.as_str())?;
    }
    tx.execute(
        "INSERT INTO restrictions (id, seq, wiki_id, at, event, account, actor, actor_rung, policy_block, reason, \
         expires_at, of_event, appeal_id) VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM restrictions), ?2, ?3, \
         ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            e.id.as_str(),
            e.wiki.as_str(),
            e.at,
            e.event.as_str(),
            e.account.as_str(),
            e.by.account.as_str(),
            e.by.rung.as_str(),
            e.policy_block,
            e.reason,
            e.expires_at,
            e.of_event.as_ref().map(RestrictionEventId::as_str),
            e.appeal.as_ref().map(AppealId::as_str),
        ],
    )
    .map_err(refusal)?;
    Ok(())
}

// ------------------------------------------------------------------------------------------------ protections

const PROTECTION_COLUMNS: &str = "id, wiki_id, at, event, kind, target, requirement, rung, actor, actor_rung, \
                                  policy_block, reason, expires_at, of_event, appeal_id";

fn protection_of(row: &Row<'_>) -> rusqlite::Result<ProtectionEvent> {
    let requirement: Option<String> = row.get(6)?;
    let rung: Option<String> = row.get(7)?;
    Ok(ProtectionEvent {
        id: ProtectionEventId::parse(&row.get::<_, String>(0)?).ok_or_else(|| invalid("id"))?,
        wiki: wiki_of(row.get(1)?)?,
        at: row.get(2)?,
        event: MeasureEvent::parse(&row.get::<_, String>(3)?).ok_or_else(|| invalid("event"))?,
        kind: ProtectionKind::parse(&row.get::<_, String>(4)?).ok_or_else(|| invalid("kind"))?,
        target: row.get(5)?,
        requirement: match (requirement.as_deref(), rung) {
            (None, None) => None,
            (Some("review"), None) => Some(ProtectionRequirement::Review),
            (Some("rung"), Some(r)) => Some(ProtectionRequirement::Rung(role(&r, "rung")?)),
            _ => return Err(invalid("requirement")),
        },
        by: actor_of(row.get(8)?, row.get(9)?)?,
        policy_block: row.get(10)?,
        reason: row.get(11)?,
        expires_at: row.get(12)?,
        of_event: row
            .get::<_, Option<String>>(13)?
            .map(|t| ProtectionEventId::parse(&t).ok_or_else(|| invalid("of_event")))
            .transpose()?,
        appeal: appeal_of(row.get(14)?)?,
    })
}

fn insert_protection(tx: &Transaction<'_>, e: &ProtectionEvent) -> Result<()> {
    e.validate()?;
    require_wiki(tx, &e.wiki)?;
    require_account(tx, &e.by.account)?;
    require_new(tx, "protections", e.id.as_str())?;
    require_unfollowed(tx, "protections", e.of_event.as_ref().map(|i| i.as_str()))?;
    if let Some(of) = &e.of_event {
        require_row(tx, "protections", of.as_str())?;
    }
    if let Some(appeal) = &e.appeal {
        require_row(tx, "appeals", appeal.as_str())?;
    }
    let (requirement, rung) = match e.requirement {
        None => (None, None),
        Some(ProtectionRequirement::Review) => (Some("review"), None),
        Some(ProtectionRequirement::Rung(r)) => (Some("rung"), Some(r.as_str())),
    };
    tx.execute(
        "INSERT INTO protections (id, seq, wiki_id, at, event, kind, target, requirement, rung, actor, actor_rung, \
         policy_block, reason, expires_at, of_event, appeal_id) VALUES (?1, \
         (SELECT COALESCE(MAX(seq), 0) + 1 FROM protections), ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, \
         ?14, ?15)",
        params![
            e.id.as_str(),
            e.wiki.as_str(),
            e.at,
            e.event.as_str(),
            e.kind.as_str(),
            e.target,
            requirement,
            rung,
            e.by.account.as_str(),
            e.by.rung.as_str(),
            e.policy_block,
            e.reason,
            e.expires_at,
            e.of_event.as_ref().map(ProtectionEventId::as_str),
            e.appeal.as_ref().map(AppealId::as_str),
        ],
    )
    .map_err(refusal)?;
    Ok(())
}

// ------------------------------------------------------------------------------------------------ tombstones

const TOMBSTONE_COLUMNS: &str = "id, wiki_id, at, event, item_kind, item_id, item_ref, hides_content, hides_summary, \
                                 hides_author, reason_category, note, affected, actor, actor_rung, policy_block, \
                                 of_event, appeal_id, declined_transition";

fn tombstone_of(row: &Row<'_>) -> rusqlite::Result<TombstoneEvent> {
    let flag = |i: usize| -> rusqlite::Result<bool> { Ok(row.get::<_, i64>(i)? == 1) };
    Ok(TombstoneEvent {
        id: TombstoneEventId::parse(&row.get::<_, String>(0)?).ok_or_else(|| invalid("id"))?,
        wiki: wiki_of(row.get(1)?)?,
        at: row.get(2)?,
        act: TombstoneAct::parse(&row.get::<_, String>(3)?).ok_or_else(|| invalid("event"))?,
        item: HiddenItem {
            kind: HiddenItemKind::parse(&row.get::<_, String>(4)?)
                .ok_or_else(|| invalid("item kind"))?,
            id: row.get(5)?,
            reference: row.get(6)?,
        },
        parts: HiddenParts {
            content: flag(7)?,
            summary: flag(8)?,
            author: flag(9)?,
        },
        category: row
            .get::<_, Option<String>>(10)?
            .map(|c| ReasonCategory::parse(&c).ok_or_else(|| invalid("reason category")))
            .transpose()?,
        note: row.get(11)?,
        affected: opt_account(row.get(12)?, "affected")?,
        by: actor_of(row.get(13)?, row.get(14)?)?,
        policy_block: row.get(15)?,
        of_event: row
            .get::<_, Option<String>>(16)?
            .map(|t| TombstoneEventId::parse(&t).ok_or_else(|| invalid("of_event")))
            .transpose()?,
        appeal: appeal_of(row.get(17)?)?,
        declined_transition: row.get(18)?,
    })
}

fn insert_tombstone(tx: &Transaction<'_>, e: &TombstoneEvent) -> Result<()> {
    e.validate()?;
    require_wiki(tx, &e.wiki)?;
    require_account(tx, &e.by.account)?;
    if let Some(affected) = &e.affected {
        require_account(tx, affected)?;
    }
    require_new(tx, "tombstones", e.id.as_str())?;
    require_unfollowed(tx, "tombstones", e.of_event.as_ref().map(|i| i.as_str()))?;
    if let Some(of) = &e.of_event {
        require_row(tx, "tombstones", of.as_str())?;
    }
    if let Some(appeal) = &e.appeal {
        require_row(tx, "appeals", appeal.as_str())?;
    }
    if let Some(transition) = e.declined_transition
        && !probe(
            tx,
            "SELECT 1 FROM proposal_transitions t JOIN proposals p ON p.id = t.proposal_id \
             WHERE t.seq = ?1 AND p.wiki_id = ?2",
            &[&transition.to_string(), e.wiki.as_str()],
        )?
    {
        return Err(StoreError::NotFound);
    }
    tx.execute(
        "INSERT INTO tombstones (id, seq, wiki_id, at, event, item_kind, item_id, item_ref, hides_content, \
         hides_summary, hides_author, reason_category, note, affected, actor, actor_rung, policy_block, of_event, \
         appeal_id, declined_transition) VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM tombstones), ?2, ?3, ?4, \
         ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
        params![
            e.id.as_str(),
            e.wiki.as_str(),
            e.at,
            e.act.as_str(),
            e.item.kind.as_str(),
            e.item.id,
            e.item.reference,
            i64::from(e.parts.content),
            i64::from(e.parts.summary),
            i64::from(e.parts.author),
            e.category.map(ReasonCategory::as_str),
            e.note,
            e.affected.as_ref().map(AccountId::as_str),
            e.by.account.as_str(),
            e.by.rung.as_str(),
            e.policy_block,
            e.of_event.as_ref().map(TombstoneEventId::as_str),
            e.appeal.as_ref().map(AppealId::as_str),
            e.declined_transition,
        ],
    )
    .map_err(refusal)?;
    Ok(())
}

// ------------------------------------------------------------------------------------------------ appeals

const APPEAL_COLUMNS: &str = "id, wiki_id, at, action_kind, action_id, action_actor, appellant, text, status, \
                              decided_at, decided_by, decision_reason, overturn_id";

fn appeal_row(row: &Row<'_>) -> rusqlite::Result<AppealRecord> {
    let status: String = row.get(8)?;
    let decided = |row: &Row<'_>| -> rusqlite::Result<Decided> {
        Ok(Decided {
            at: row
                .get::<_, Option<i64>>(9)?
                .ok_or_else(|| invalid("decided_at"))?,
            by: account_of(
                row.get::<_, Option<String>>(10)?
                    .ok_or_else(|| invalid("decided_by"))?,
                "decided_by",
            )?,
            reason: row
                .get::<_, Option<String>>(11)?
                .ok_or_else(|| invalid("decision_reason"))?,
        })
    };
    let state = match status.as_str() {
        "open" => AppealState::Open,
        "upheld" => AppealState::Upheld(decided(row)?),
        "overturned" => AppealState::Overturned(
            decided(row)?,
            row.get::<_, Option<String>>(12)?
                .ok_or_else(|| invalid("overturn_id"))?,
        ),
        _ => return Err(invalid("status")),
    };
    Ok(AppealRecord {
        id: AppealId::parse(&row.get::<_, String>(0)?).ok_or_else(|| invalid("id"))?,
        wiki: wiki_of(row.get(1)?)?,
        at: row.get(2)?,
        action: AppealedAction {
            kind: AppealedKind::parse(&row.get::<_, String>(3)?)
                .ok_or_else(|| invalid("action kind"))?,
            id: row.get(4)?,
        },
        action_actor: opt_account(row.get(5)?, "action actor")?,
        appellant: account_of(row.get(6)?, "appellant")?,
        text: row.get(7)?,
        state,
    })
}

impl SqliteStore {
    /// Run `f` over the rows a query returns, each mapped by `map`.
    fn governance_rows<T>(
        &self,
        sql: &str,
        args: impl rusqlite::Params,
        map: fn(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Vec<T>> {
        self.with(|conn| {
            let mut stmt = conn.prepare(sql).map_err(backend)?;
            let rows = stmt.query_map(args, map).map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(backend)
        })
    }

    fn governance_one<T>(
        &self,
        sql: &str,
        id: &str,
        map: fn(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Option<T>> {
        self.with(|conn| {
            conn.query_row(sql, params![id], map)
                .optional()
                .map_err(backend)
        })
    }

    /// Run `f` in one immediate transaction and commit it when it succeeds.
    fn governance_write<T>(&self, f: impl FnOnce(&Transaction<'_>) -> Result<T>) -> Result<T> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            let done = f(&tx)?;
            tx.commit().map_err(backend)?;
            Ok(done)
        })
    }
}

impl GovernanceStore for SqliteStore {
    fn record_grant_decision(&self, decision: &GrantDecisionRecord) -> Result<()> {
        self.governance_write(|tx| insert_grant_decision(tx, decision))
    }

    fn grant_decision(&self, id: &GrantDecisionId) -> Result<Option<GrantDecisionRecord>> {
        self.governance_one(
            &format!("SELECT {GRANT_COLUMNS} FROM grant_decisions WHERE id = ?1"),
            id.as_str(),
            grant_of,
        )
    }

    fn grant_decisions(
        &self,
        wiki: &WikiId,
        target: Option<&AccountId>,
        limit: usize,
    ) -> Result<Vec<GrantDecisionRecord>> {
        self.governance_rows(
            &format!(
                "SELECT {GRANT_COLUMNS} FROM grant_decisions WHERE wiki_id = ?1 AND (?2 IS NULL OR target = ?2) \
                 ORDER BY seq DESC LIMIT ?3"
            ),
            params![wiki.as_str(), target.map(AccountId::as_str), limit_of(limit)],
            grant_of,
        )
    }

    fn record_restriction_event(&self, event: &RestrictionEvent) -> Result<()> {
        self.governance_write(|tx| insert_restriction(tx, event))
    }

    fn restriction_event(&self, id: &RestrictionEventId) -> Result<Option<RestrictionEvent>> {
        self.governance_one(
            &format!("SELECT {RESTRICTION_COLUMNS} FROM restrictions WHERE id = ?1"),
            id.as_str(),
            restriction_of,
        )
    }

    fn restriction_events(
        &self,
        wiki: &WikiId,
        account: &AccountId,
    ) -> Result<Vec<RestrictionEvent>> {
        self.governance_rows(
            &format!(
                "SELECT {RESTRICTION_COLUMNS} FROM restrictions WHERE wiki_id = ?1 AND account = ?2 ORDER BY seq"
            ),
            params![wiki.as_str(), account.as_str()],
            restriction_of,
        )
    }

    fn active_restrictions(
        &self,
        wiki: &WikiId,
        account: &AccountId,
        at: i64,
    ) -> Result<Vec<RestrictionEvent>> {
        self.governance_rows(
            "SELECT r.id, r.wiki_id, r.at, r.event, r.account, r.actor, r.actor_rung, r.policy_block, r.reason, \
             r.expires_at, r.of_event, r.appeal_id FROM restrictions r WHERE r.wiki_id = ?1 AND r.account = ?2 \
             AND r.event IN ('set', 'renewed') AND r.at <= ?3 AND r.expires_at > ?3 \
             AND NOT EXISTS (SELECT 1 FROM restrictions n WHERE n.of_event = r.id) \
             ORDER BY r.expires_at DESC, r.seq DESC",
            params![wiki.as_str(), account.as_str(), at],
            restriction_of,
        )
    }

    fn record_protection_event(&self, event: &ProtectionEvent) -> Result<()> {
        self.governance_write(|tx| insert_protection(tx, event))
    }

    fn protection_event(&self, id: &ProtectionEventId) -> Result<Option<ProtectionEvent>> {
        self.governance_one(
            &format!("SELECT {PROTECTION_COLUMNS} FROM protections WHERE id = ?1"),
            id.as_str(),
            protection_of,
        )
    }

    fn protection_events(&self, wiki: &WikiId, limit: usize) -> Result<Vec<ProtectionEvent>> {
        self.governance_rows(
            &format!(
                "SELECT {PROTECTION_COLUMNS} FROM protections WHERE wiki_id = ?1 ORDER BY seq DESC LIMIT ?2"
            ),
            params![wiki.as_str(), limit_of(limit)],
            protection_of,
        )
    }

    fn active_protections(&self, wiki: &WikiId, at: i64) -> Result<Vec<ProtectionEvent>> {
        self.governance_rows(
            &active_protections_sql(""),
            params![wiki.as_str(), at],
            protection_of,
        )
    }

    fn active_protections_on(
        &self,
        wiki: &WikiId,
        kind: ProtectionKind,
        target: &str,
        at: i64,
    ) -> Result<Vec<ProtectionEvent>> {
        self.governance_rows(
            &active_protections_sql("AND p.kind = ?3 AND p.target = ?4"),
            params![wiki.as_str(), at, kind.as_str(), target],
            protection_of,
        )
    }

    fn record_tombstone_event(&self, event: &TombstoneEvent) -> Result<()> {
        self.governance_write(|tx| insert_tombstone(tx, event))
    }

    fn tombstone_event(&self, id: &TombstoneEventId) -> Result<Option<TombstoneEvent>> {
        self.governance_one(
            &format!("SELECT {TOMBSTONE_COLUMNS} FROM tombstones WHERE id = ?1"),
            id.as_str(),
            tombstone_of,
        )
    }

    fn tombstone_events_of(&self, wiki: &WikiId, item: &HiddenItem) -> Result<Vec<TombstoneEvent>> {
        self.governance_rows(
            &format!(
                "SELECT {TOMBSTONE_COLUMNS} FROM tombstones WHERE wiki_id = ?1 AND item_kind = ?2 AND item_id = ?3 \
                 AND item_ref IS ?4 ORDER BY seq"
            ),
            params![wiki.as_str(), item.kind.as_str(), item.id, item.reference],
            tombstone_of,
        )
    }

    fn tombstone_log(
        &self,
        wiki: &WikiId,
        before: Option<&TombstoneEventId>,
        limit: usize,
    ) -> Result<Vec<TombstoneEvent>> {
        let start: i64 = match before {
            None => i64::MAX,
            Some(id) => self.with(|conn| {
                conn.query_row(
                    "SELECT seq FROM tombstones WHERE id = ?1 AND wiki_id = ?2",
                    params![id.as_str(), wiki.as_str()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(backend)?
                .ok_or(StoreError::NotFound)
            })?,
        };
        self.governance_rows(
            &format!(
                "SELECT {TOMBSTONE_COLUMNS} FROM tombstones WHERE wiki_id = ?1 AND seq < ?2 ORDER BY seq DESC LIMIT ?3"
            ),
            params![wiki.as_str(), start, limit_of(limit)],
            tombstone_of,
        )
    }

    fn file_appeal(&self, appeal: &AppealRecord) -> Result<()> {
        appeal.validate()?;
        self.governance_write(|tx| {
            require_wiki(tx, &appeal.wiki)?;
            require_account(tx, &appeal.appellant)?;
            if let Some(actor) = &appeal.action_actor {
                require_account(tx, actor)?;
            }
            require_new(tx, "appeals", appeal.id.as_str())?;
            if probe(
                tx,
                "SELECT 1 FROM appeals WHERE status = 'open' AND wiki_id = ?1 AND action_kind = ?2 \
                 AND action_id = ?3 AND appellant = ?4",
                &[
                    appeal.wiki.as_str(),
                    appeal.action.kind.as_str(),
                    &appeal.action.id,
                    appeal.appellant.as_str(),
                ],
            )? {
                return Err(StoreError::AlreadyRecorded);
            }
            tx.execute(
                "INSERT INTO appeals (id, seq, wiki_id, at, action_kind, action_id, action_actor, appellant, text, \
                 status) VALUES (?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM appeals), ?2, ?3, ?4, ?5, ?6, ?7, ?8, \
                 'open')",
                params![
                    appeal.id.as_str(),
                    appeal.wiki.as_str(),
                    appeal.at,
                    appeal.action.kind.as_str(),
                    appeal.action.id,
                    appeal.action_actor.as_ref().map(AccountId::as_str),
                    appeal.appellant.as_str(),
                    appeal.text,
                ],
            )
            .map_err(refusal)?;
            Ok(())
        })
    }

    fn appeal(&self, id: &AppealId) -> Result<Option<AppealRecord>> {
        self.governance_one(
            &format!("SELECT {APPEAL_COLUMNS} FROM appeals WHERE id = ?1"),
            id.as_str(),
            appeal_row,
        )
    }

    fn decide_appeal(&self, id: &AppealId, decision: &AppealDecision) -> Result<()> {
        decision.validate()?;
        self.governance_write(|tx| {
            let appeal = tx
                .query_row(
                    &format!("SELECT {APPEAL_COLUMNS} FROM appeals WHERE id = ?1"),
                    params![id.as_str()],
                    appeal_row,
                )
                .optional()
                .map_err(backend)?
                .ok_or(StoreError::NotFound)?;
            if appeal.state != AppealState::Open {
                return Err(StoreError::StateChanged);
            }
            require_account(tx, &decision.by)?;
            let overturn = match &decision.outcome {
                AppealOutcome::Upheld => None,
                AppealOutcome::Overturned(reversal) => {
                    if reversal.kind() != appeal.action.kind {
                        return Err(StoreError::Backend(
                            "an overturn reverses an action of the kind that was appealed".to_owned(),
                        ));
                    }
                    if let Some((named, wiki)) = reversal.names()
                        && (named != id || *wiki != appeal.wiki)
                    {
                        return Err(StoreError::Backend(
                            "the record that reverses an action names the appeal it overturns".to_owned(),
                        ));
                    }
                    match &**reversal {
                        Reversal::Grant(d) => insert_grant_decision(tx, d)?,
                        Reversal::Restriction(e) => insert_restriction(tx, e)?,
                        Reversal::Protection(e) => insert_protection(tx, e)?,
                        Reversal::Tombstone(e) => insert_tombstone(tx, e)?,
                        Reversal::Decline { transition } => {
                            if !probe(
                                tx,
                                "SELECT 1 FROM proposal_transitions t JOIN proposals p ON p.id = t.proposal_id \
                                 WHERE t.seq = ?1 AND p.wiki_id = ?2",
                                &[&transition.to_string(), appeal.wiki.as_str()],
                            )? {
                                return Err(StoreError::NotFound);
                            }
                        }
                    }
                    Some(reversal.record_id())
                }
            };
            let changed = tx
                .execute(
                    "UPDATE appeals SET status = ?2, decided_at = ?3, decided_by = ?4, decision_reason = ?5, \
                     overturn_id = ?6 WHERE id = ?1 AND status = 'open'",
                    params![
                        id.as_str(),
                        if overturn.is_some() { "overturned" } else { "upheld" },
                        decision.at,
                        decision.by.as_str(),
                        decision.reason,
                        overturn,
                    ],
                )
                .map_err(refusal)?;
            if changed == 0 {
                return Err(StoreError::StateChanged);
            }
            Ok(())
        })
    }

    fn open_appeals(&self, wiki: &WikiId, limit: usize) -> Result<Vec<AppealRecord>> {
        self.governance_rows(
            &format!(
                "SELECT {APPEAL_COLUMNS} FROM appeals WHERE wiki_id = ?1 AND status = 'open' ORDER BY seq LIMIT ?2"
            ),
            params![wiki.as_str(), limit_of(limit)],
            appeal_row,
        )
    }

    fn appeals_by(&self, wiki: &WikiId, appellant: &AccountId) -> Result<Vec<AppealRecord>> {
        self.governance_rows(
            &format!(
                "SELECT {APPEAL_COLUMNS} FROM appeals WHERE wiki_id = ?1 AND appellant = ?2 ORDER BY seq DESC"
            ),
            params![wiki.as_str(), appellant.as_str()],
            appeal_row,
        )
    }

    fn appeals_about(&self, wiki: &WikiId, action: &AppealedAction) -> Result<Vec<AppealRecord>> {
        self.governance_rows(
            &format!(
                "SELECT {APPEAL_COLUMNS} FROM appeals WHERE wiki_id = ?1 AND action_kind = ?2 AND action_id = ?3 \
                 ORDER BY seq"
            ),
            params![wiki.as_str(), action.kind.as_str(), action.id],
            appeal_row,
        )
    }

    fn open_appeal_count(&self, wiki: &WikiId, appellant: &AccountId) -> Result<u64> {
        self.with(|conn| {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM appeals WHERE wiki_id = ?1 AND appellant = ?2 AND status = 'open'",
                    params![wiki.as_str(), appellant.as_str()],
                    |r| r.get(0),
                )
                .map_err(backend)?;
            Ok(count(n))
        })
    }

    fn appeals_since(
        &self,
        wiki: &WikiId,
        appellant: Option<&AccountId>,
        since: i64,
    ) -> Result<Vec<i64>> {
        self.governance_rows(
            "SELECT at FROM appeals WHERE wiki_id = ?1 AND (?2 IS NULL OR appellant = ?2) AND at >= ?3 ORDER BY at",
            params![wiki.as_str(), appellant.map(AccountId::as_str), since],
            |r| r.get::<_, i64>(0),
        )
    }
}

/// The protections of a wiki in force at a time: `?1` the wiki, `?2` the time, then what `extra` adds.
fn active_protections_sql(extra: &str) -> String {
    format!(
        "SELECT p.id, p.wiki_id, p.at, p.event, p.kind, p.target, p.requirement, p.rung, p.actor, p.actor_rung, \
         p.policy_block, p.reason, p.expires_at, p.of_event, p.appeal_id FROM protections p \
         WHERE p.wiki_id = ?1 AND p.event IN ('set', 'renewed') AND p.at <= ?2 AND p.expires_at > ?2 {extra} \
         AND NOT EXISTS (SELECT 1 FROM protections n WHERE n.of_event = p.id) \
         ORDER BY p.expires_at DESC, p.seq DESC"
    )
}

/// A stored row of the derived snapshot as columns: the block, the two files, and the fault's file, line and rule.
type SnapshotRow = (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<String>,
);

/// The snapshot a row says, or `None` when it does not decode (a fault with a half of its columns, a line that is no line).
fn decoded(
    (policy_block, constitution, policy, file, line, rule): SnapshotRow,
) -> Option<GovernanceSnapshot> {
    let fault = match (file, rule) {
        (None, None) => None,
        (Some(file), Some(rule)) => Some(GovernanceFault {
            file,
            line: match line {
                None => None,
                Some(n) => Some(u32::try_from(n).ok()?),
            },
            rule,
        }),
        _ => return None,
    };
    Some(GovernanceSnapshot {
        policy_block,
        constitution,
        policy,
        fault,
    })
}

impl GovernanceSnapshotStore for SqliteStore {
    fn governance_snapshot(&self, wiki: &WikiId, tip: &str) -> Result<Option<GovernanceSnapshot>> {
        self.with(|conn| {
            let found = conn
                .query_row(
                    "SELECT policy_block, constitution, policy, fault_file, fault_line, fault_rule \
                     FROM governance_derived WHERE wiki_id = ?1 AND tip = ?2",
                    params![wiki.as_str(), tip],
                    |r| {
                        Ok((
                            r.get::<_, Option<String>>(0)?,
                            r.get::<_, Option<String>>(1)?,
                            r.get::<_, Option<String>>(2)?,
                            r.get::<_, Option<String>>(3)?,
                            r.get::<_, Option<i64>>(4)?,
                            r.get::<_, Option<String>>(5)?,
                        ))
                    },
                )
                .optional()
                .map_err(backend)?;
            // A row that does not decode is a miss: the snapshot is derived, so it is derived again.
            Ok(found.and_then(decoded))
        })
    }

    fn latest_governance_snapshot(
        &self,
        wiki: &WikiId,
    ) -> Result<Option<(String, GovernanceSnapshot)>> {
        self.with(|conn| {
            let found = conn
                .query_row(
                    "SELECT policy_block, constitution, policy, fault_file, fault_line, fault_rule, tip \
                     FROM governance_derived WHERE wiki_id = ?1 ORDER BY built_at DESC, rowid DESC LIMIT 1",
                    params![wiki.as_str()],
                    |r| {
                        Ok((
                            (
                                r.get::<_, Option<String>>(0)?,
                                r.get::<_, Option<String>>(1)?,
                                r.get::<_, Option<String>>(2)?,
                                r.get::<_, Option<String>>(3)?,
                                r.get::<_, Option<i64>>(4)?,
                                r.get::<_, Option<String>>(5)?,
                            ),
                            r.get::<_, String>(6)?,
                        ))
                    },
                )
                .optional()
                .map_err(backend)?;
            // The newest row that does not decode is a miss, and no older one stands in for it: what the newest said is what
            // is not known.
            Ok(found.and_then(|(row, tip)| decoded(row).map(|snapshot| (tip, snapshot))))
        })
    }

    fn put_governance_snapshot(
        &self,
        wiki: &WikiId,
        tip: &str,
        snapshot: &GovernanceSnapshot,
        built_at: i64,
    ) -> Result<()> {
        snapshot.validate()?;
        self.with(|conn| {
            let fault = snapshot.fault.as_ref();
            conn.execute(
                "INSERT OR REPLACE INTO governance_derived (wiki_id, tip, policy_block, constitution, policy, \
                 fault_file, fault_line, fault_rule, built_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    wiki.as_str(),
                    tip,
                    snapshot.policy_block,
                    snapshot.constitution,
                    snapshot.policy,
                    fault.map(|f| f.file.as_str()),
                    fault.and_then(|f| f.line),
                    fault.map(|f| f.rule.as_str()),
                    built_at,
                ],
            )
            .map_err(refusal)?;
            Ok(())
        })
    }
}
