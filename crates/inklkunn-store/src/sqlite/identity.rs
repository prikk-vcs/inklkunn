//! The SQLite accounts, sessions and tokens (schema version 4).

use rusqlite::{OptionalExtension, Row, TransactionBehavior, params};

use super::{SqliteStore, backend};
use crate::access::Scope;
use crate::identity::{
    Account, AccountStore, ExternalIdentity, LocalCredential, LoginName, SessionRecord,
    SessionStore, TokenRecord, TokenStore, clean_label,
};
use crate::record::{AccountId, DisplayName, TokenId};
use crate::{Result, StoreError};

fn invalid(what: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidColumnType(0, what.to_owned(), rusqlite::types::Type::Text)
}

/// Which uniqueness rule refused an insert or an update, read from SQLite's own message.
fn refusal(e: &rusqlite::Error) -> Option<&str> {
    let rusqlite::Error::SqliteFailure(code, Some(msg)) = e else {
        return None;
    };
    (code.code == rusqlite::ErrorCode::ConstraintViolation).then_some(msg.as_str())
}

fn map_account_write(
    e: rusqlite::Error,
    account: &AccountId,
    login: Option<&LoginName>,
) -> StoreError {
    match refusal(&e) {
        Some(m) if m.contains("accounts.login") => {
            StoreError::LoginTaken(login.map(ToString::to_string).unwrap_or_default())
        }
        Some(m) if m.contains("accounts.id") => StoreError::IdTaken(account.to_string()),
        _ => backend(e),
    }
}

/// Does a row exist? (One probe inside the caller's transaction.)
fn exists(tx: &rusqlite::Transaction<'_>, sql: &str, value: &str) -> Result<bool> {
    tx.query_row(sql, params![value], |_| Ok(()))
        .optional()
        .map(|found| found.is_some())
        .map_err(backend)
}

const ACCOUNT_COLUMNS: &str = "id, display_name, login, password_hash, created_at, disabled";

fn row_to_account(row: &Row<'_>) -> rusqlite::Result<Account> {
    let id: String = row.get(0)?;
    let display_name: String = row.get(1)?;
    let login: Option<String> = row.get(2)?;
    let hash: Option<String> = row.get(3)?;
    let disabled: i64 = row.get(5)?;
    let local = match (login, hash) {
        (Some(login), Some(password_hash)) => Some(LocalCredential {
            login: LoginName::parse(&login).map_err(|_| invalid("login"))?,
            password_hash,
        }),
        (None, None) => None,
        _ => return Err(invalid("login/password_hash")),
    };
    Ok(Account {
        id: AccountId::parse(&id).ok_or_else(|| invalid("id"))?,
        display_name: DisplayName::new(&display_name).map_err(|_| invalid("display_name"))?,
        local,
        created_at: row.get(4)?,
        disabled: disabled != 0,
    })
}

impl SqliteStore {
    fn account_where(&self, clause: &str, value: &str) -> Result<Option<Account>> {
        self.with(|conn| {
            conn.query_row(
                &format!("SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE {clause}"),
                params![value],
                row_to_account,
            )
            .optional()
            .map_err(backend)
        })
    }
}

impl AccountStore for SqliteStore {
    fn add_account(&self, account: &Account) -> Result<()> {
        let login = account.local.as_ref().map(|l| &l.login);
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            // The id is checked before the name, so the refusal does not depend on which index SQLite
            // happens to trip first (the in-memory store answers in this order too).
            if exists(&tx, "SELECT 1 FROM accounts WHERE id = ?1", account.id.as_str())? {
                return Err(StoreError::IdTaken(account.id.to_string()));
            }
            if let Some(login) = login
                && exists(&tx, "SELECT 1 FROM accounts WHERE login = ?1", login.as_str())?
            {
                return Err(StoreError::LoginTaken(login.to_string()));
            }
            tx.execute(
                "INSERT INTO accounts (id, display_name, login, password_hash, created_at, disabled) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    account.id.as_str(),
                    account.display_name.as_str(),
                    login.map(LoginName::as_str),
                    account.local.as_ref().map(|l| l.password_hash.as_str()),
                    account.created_at,
                    i64::from(account.disabled),
                ],
            )
            .map_err(|e| map_account_write(e, &account.id, login))?;
            tx.commit().map_err(backend)
        })
    }

    fn account(&self, id: &AccountId) -> Result<Option<Account>> {
        self.account_where("id = ?1", id.as_str())
    }

    fn account_by_login(&self, login: &LoginName) -> Result<Option<Account>> {
        self.account_where("login = ?1", login.as_str())
    }

    fn account_by_identity(&self, identity: &ExternalIdentity) -> Result<Option<Account>> {
        self.with(|conn| {
            conn.query_row(
                &format!(
                    "SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE id = \
                     (SELECT account_id FROM account_identities WHERE issuer = ?1 AND subject = ?2)"
                ),
                params![identity.issuer(), identity.subject()],
                row_to_account,
            )
            .optional()
            .map_err(backend)
        })
    }

    fn link_identity(&self, id: &AccountId, identity: &ExternalIdentity, at: i64) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            let exists = tx
                .query_row("SELECT 1 FROM accounts WHERE id = ?1", params![id.as_str()], |_| {
                    Ok(())
                })
                .optional()
                .map_err(backend)?
                .is_some();
            if !exists {
                return Err(StoreError::AccountNotFound);
            }
            let owner: Option<String> = tx
                .query_row(
                    "SELECT account_id FROM account_identities WHERE issuer = ?1 AND subject = ?2",
                    params![identity.issuer(), identity.subject()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(backend)?;
            match owner {
                Some(o) if o == id.as_str() => return Ok(()),
                Some(_) => return Err(StoreError::IdentityTaken),
                None => {}
            }
            tx.execute(
                "INSERT INTO account_identities (issuer, subject, account_id, linked_at) VALUES (?1, ?2, ?3, ?4)",
                params![identity.issuer(), identity.subject(), id.as_str(), at],
            )
            .map_err(backend)?;
            tx.commit().map_err(backend)
        })
    }

    fn set_local_credential(
        &self,
        id: &AccountId,
        credential: Option<&LocalCredential>,
    ) -> Result<()> {
        let login = credential.map(|c| &c.login);
        self.with(|conn| {
            let n = conn
                .execute(
                    "UPDATE accounts SET login = ?1, password_hash = ?2 WHERE id = ?3",
                    params![
                        login.map(LoginName::as_str),
                        credential.map(|c| c.password_hash.as_str()),
                        id.as_str()
                    ],
                )
                .map_err(|e| map_account_write(e, id, login))?;
            if n == 0 {
                return Err(StoreError::AccountNotFound);
            }
            Ok(())
        })
    }

    fn set_disabled(&self, id: &AccountId, disabled: bool) -> Result<()> {
        self.with(|conn| {
            let n = conn
                .execute(
                    "UPDATE accounts SET disabled = ?1 WHERE id = ?2",
                    params![i64::from(disabled), id.as_str()],
                )
                .map_err(backend)?;
            if n == 0 {
                return Err(StoreError::AccountNotFound);
            }
            Ok(())
        })
    }
}

fn row_to_session(row: &Row<'_>) -> rusqlite::Result<SessionRecord> {
    let account: String = row.get(1)?;
    Ok(SessionRecord {
        id_hash: row.get(0)?,
        account: AccountId::parse(&account).ok_or_else(|| invalid("account_id"))?,
        csrf: row.get(2)?,
        created_at: row.get(3)?,
        expires_at: row.get(4)?,
    })
}

impl SessionStore for SqliteStore {
    fn add_session(&self, session: &SessionRecord) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !exists(
                &tx,
                "SELECT 1 FROM accounts WHERE id = ?1",
                session.account.as_str(),
            )? {
                return Err(StoreError::AccountNotFound);
            }
            if exists(
                &tx,
                "SELECT 1 FROM sessions WHERE id_hash = ?1",
                &session.id_hash,
            )? {
                return Err(StoreError::IdTaken("session".to_owned()));
            }
            tx.execute(
                "INSERT INTO sessions (id_hash, account_id, csrf, created_at, expires_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    session.id_hash,
                    session.account.as_str(),
                    session.csrf,
                    session.created_at,
                    session.expires_at
                ],
            )
            .map_err(backend)?;
            tx.commit().map_err(backend)
        })
    }

    fn session(&self, id_hash: &str, now: i64) -> Result<Option<SessionRecord>> {
        self.with(|conn| {
            conn.query_row(
                "SELECT id_hash, account_id, csrf, created_at, expires_at FROM sessions \
                 WHERE id_hash = ?1 AND expires_at > ?2",
                params![id_hash, now],
                row_to_session,
            )
            .optional()
            .map_err(backend)
        })
    }

    fn end_session(&self, id_hash: &str) -> Result<bool> {
        self.with(|conn| {
            conn.execute("DELETE FROM sessions WHERE id_hash = ?1", params![id_hash])
                .map(|n| n > 0)
                .map_err(backend)
        })
    }

    fn end_sessions_of(&self, id: &AccountId) -> Result<u64> {
        self.with(|conn| {
            conn.execute(
                "DELETE FROM sessions WHERE account_id = ?1",
                params![id.as_str()],
            )
            .map(|n| n as u64)
            .map_err(backend)
        })
    }

    fn purge_expired_sessions(&self, now: i64) -> Result<u64> {
        self.with(|conn| {
            conn.execute("DELETE FROM sessions WHERE expires_at <= ?1", params![now])
                .map(|n| n as u64)
                .map_err(backend)
        })
    }
}

const TOKEN_COLUMNS: &str =
    "id, hash, account_id, scope, label, created_at, expires_at, revoked_at";

fn row_to_token(row: &Row<'_>) -> rusqlite::Result<TokenRecord> {
    let id: String = row.get(0)?;
    let account: String = row.get(2)?;
    let scope: String = row.get(3)?;
    Ok(TokenRecord {
        id: TokenId::parse(&id).ok_or_else(|| invalid("id"))?,
        hash: row.get(1)?,
        account: AccountId::parse(&account).ok_or_else(|| invalid("account_id"))?,
        scope: Scope::parse(&scope).ok_or_else(|| invalid("scope"))?,
        label: row.get(4)?,
        created_at: row.get(5)?,
        expires_at: row.get(6)?,
        revoked_at: row.get(7)?,
    })
}

impl TokenStore for SqliteStore {
    fn add_token(&self, token: &TokenRecord) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            if !exists(&tx, "SELECT 1 FROM accounts WHERE id = ?1", token.account.as_str())? {
                return Err(StoreError::AccountNotFound);
            }
            if exists(&tx, "SELECT 1 FROM api_tokens WHERE id = ?1", token.id.as_str())? {
                return Err(StoreError::IdTaken(token.id.to_string()));
            }
            if exists(&tx, "SELECT 1 FROM api_tokens WHERE hash = ?1", &token.hash)? {
                return Err(StoreError::IdTaken("token".to_owned()));
            }
            tx.execute(
                "INSERT INTO api_tokens (id, hash, account_id, scope, label, created_at, expires_at, revoked_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    token.id.as_str(),
                    token.hash,
                    token.account.as_str(),
                    token.scope.as_str(),
                    clean_label(&token.label),
                    token.created_at,
                    token.expires_at,
                    token.revoked_at
                ],
            )
            .map_err(backend)?;
            tx.commit().map_err(backend)
        })
    }

    fn token_by_hash(&self, hash: &str) -> Result<Option<TokenRecord>> {
        self.with(|conn| {
            conn.query_row(
                &format!("SELECT {TOKEN_COLUMNS} FROM api_tokens WHERE hash = ?1"),
                params![hash],
                row_to_token,
            )
            .optional()
            .map_err(backend)
        })
    }

    fn tokens_of(&self, account: &AccountId) -> Result<Vec<TokenRecord>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT {TOKEN_COLUMNS} FROM api_tokens WHERE account_id = ?1 ORDER BY created_at, rowid"
                ))
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![account.as_str()], row_to_token)
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(backend)
        })
    }

    fn revoke_token(&self, account: &AccountId, id: &TokenId, at: i64) -> Result<bool> {
        self.with(|conn| {
            conn.execute(
                "UPDATE api_tokens SET revoked_at = ?1 \
                 WHERE id = ?2 AND account_id = ?3 AND revoked_at IS NULL",
                params![at, id.as_str(), account.as_str()],
            )
            .map(|n| n > 0)
            .map_err(backend)
        })
    }
}
