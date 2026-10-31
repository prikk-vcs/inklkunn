//! The SQLite invitation records (schema version 14). Both tables carry the write-once triggers of
//! [`V14`](super::V14); redeeming is one immediate transaction that checks the invitation, adds the account,
//! grants the role and records the use, so the link is spent exactly when the account exists.

use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::identity::insert_account;
use super::proposal::{invalid, probe, write_refusal};
use super::{SqliteStore, backend, is_foreign_key_refusal};
use crate::access::{Role, RoleGrant};
use crate::identity::{Account, looks_like_an_argon2id_hash};
use crate::invitation::{InvitationRecord, InvitationStore, InvitationUse};
use crate::record::{AccountId, InvitationId, WikiId};
use crate::{Result, StoreError};

const COLUMNS: &str =
    "i.id, i.hash, i.created_at, i.expires_at, i.wiki_id, i.role, u.account_id, u.at";

fn read(conn: &Connection, hash: &str) -> Result<Option<InvitationRecord>> {
    conn.query_row(
        &format!(
            "SELECT {COLUMNS} FROM invitations i LEFT JOIN invitation_uses u ON u.invitation_id = i.id \
             WHERE i.hash = ?1"
        ),
        params![hash],
        |row| {
            let id: String = row.get(0)?;
            let wiki: Option<String> = row.get(4)?;
            let role: Option<String> = row.get(5)?;
            let used_by: Option<String> = row.get(6)?;
            let grant = match (wiki, role) {
                (Some(w), Some(r)) => Some((
                    WikiId::parse(&w).ok_or_else(|| invalid("wiki_id"))?,
                    Role::parse(&r).ok_or_else(|| invalid("role"))?,
                )),
                (None, None) => None,
                _ => return Err(invalid("wiki_id/role")),
            };
            let used = match used_by {
                Some(a) => Some(InvitationUse {
                    account: AccountId::parse(&a).ok_or_else(|| invalid("account_id"))?,
                    at: row.get(7)?,
                }),
                None => None,
            };
            Ok(InvitationRecord {
                id: InvitationId::parse(&id).ok_or_else(|| invalid("id"))?,
                hash: row.get(1)?,
                created_at: row.get(2)?,
                expires_at: row.get(3)?,
                grant,
                used,
            })
        },
    )
    .optional()
    .map_err(backend)
}

/// A foreign key that refused is a wiki that is not there.
fn wiki_missing(e: rusqlite::Error) -> StoreError {
    if is_foreign_key_refusal(&e) {
        StoreError::NotFound
    } else {
        backend(e)
    }
}

impl InvitationStore for SqliteStore {
    fn add_invitation(&self, invitation: &InvitationRecord) -> Result<()> {
        if invitation.used.is_some() {
            return Err(StoreError::AlreadyRecorded);
        }
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if probe(
                &tx,
                "SELECT 1 FROM invitations WHERE id = ?1 OR hash = ?2",
                &[invitation.id.as_str(), &invitation.hash],
            )? {
                return Err(StoreError::IdTaken(invitation.id.to_string()));
            }
            if let Some((wiki, _)) = &invitation.grant
                && !probe(&tx, "SELECT 1 FROM wikis WHERE id = ?1", &[wiki.as_str()])?
            {
                return Err(StoreError::NotFound);
            }
            tx.execute(
                "INSERT INTO invitations (id, hash, created_at, expires_at, wiki_id, role) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    invitation.id.as_str(),
                    invitation.hash,
                    invitation.created_at,
                    invitation.expires_at,
                    invitation.grant.as_ref().map(|(w, _)| w.as_str()),
                    invitation.grant.as_ref().map(|(_, r)| r.as_str()),
                ],
            )
            .map_err(wiki_missing)?;
            tx.commit().map_err(backend)
        })
    }

    fn invitation_by_hash(&self, hash: &str) -> Result<Option<InvitationRecord>> {
        self.with(|conn| read(conn, hash))
    }

    fn redeem_invitation(
        &self,
        hash: &str,
        account: &Account,
        now: i64,
    ) -> Result<Option<RoleGrant>> {
        if let Some(local) = &account.local
            && !looks_like_an_argon2id_hash(&local.password_hash)
        {
            return Err(StoreError::InvalidCredential);
        }
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            let invitation = read(&tx, hash)?
                .filter(|i| i.usable_at(now))
                .ok_or(StoreError::InvitationUnusable)?;
            // The account first (its refusals are the account store's own), then the role, then the use.
            insert_account(&tx, account)?;
            let grant = invitation.grant.as_ref().map(|(wiki, role)| RoleGrant {
                account: account.id.clone(),
                wiki: wiki.clone(),
                role: *role,
                granted_by: None,
                granted_at: now,
            });
            if let Some(g) = &grant {
                tx.execute(
                    "INSERT INTO role_grants (wiki_id, account_id, role, granted_by, granted_at, seq) \
                     VALUES (?1, ?2, ?3, NULL, ?4, (SELECT COALESCE(MAX(seq), 0) + 1 FROM role_grants))",
                    params![g.wiki.as_str(), g.account.as_str(), g.role.as_str(), g.granted_at],
                )
                .map_err(wiki_missing)?;
            }
            tx.execute(
                "INSERT INTO invitation_uses (invitation_id, account_id, at) VALUES (?1, ?2, ?3)",
                params![invitation.id.as_str(), account.id.as_str(), now],
            )
            .map_err(write_refusal)?;
            tx.commit().map_err(backend)?;
            Ok(grant)
        })
    }
}
