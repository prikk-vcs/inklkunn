//! The in-memory accounts, sessions and tokens: the reference the SQLite store must agree with.

use std::sync::PoisonError;

use crate::identity::{
    Account, AccountStore, ExternalIdentity, LocalCredential, LoginName, SessionRecord,
    SessionStore, TokenRecord, TokenStore, looks_like_an_argon2id_hash,
};
use crate::record::{AccountId, TokenId};
use crate::{Result, StoreError};

use super::MemoryStore;

/// Everything identity-shaped, behind one lock so that a check and the change that follows it are one step.
#[derive(Debug, Default)]
pub(super) struct Identities {
    accounts: Vec<Account>,
    links: Vec<(ExternalIdentity, AccountId)>,
    sessions: Vec<SessionRecord>,
    tokens: Vec<TokenRecord>,
}

impl Identities {
    fn login_taken(&self, login: &LoginName, except: Option<&AccountId>) -> bool {
        self.accounts
            .iter()
            .any(|a| Some(&a.id) != except && a.local.as_ref().is_some_and(|l| l.login == *login))
    }

    fn account_mut(&mut self, id: &AccountId) -> Result<&mut Account> {
        self.accounts
            .iter_mut()
            .find(|a| a.id == *id)
            .ok_or(StoreError::AccountNotFound)
    }
}

impl MemoryStore {
    fn identities(&self) -> std::sync::MutexGuard<'_, Identities> {
        self.identities
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

impl AccountStore for MemoryStore {
    fn add_account(&self, account: &Account) -> Result<()> {
        if let Some(local) = &account.local
            && !looks_like_an_argon2id_hash(&local.password_hash)
        {
            return Err(StoreError::InvalidCredential);
        }
        let mut d = self.identities();
        if d.accounts.iter().any(|a| a.id == account.id) {
            return Err(StoreError::IdTaken(account.id.to_string()));
        }
        if let Some(local) = &account.local
            && d.login_taken(&local.login, None)
        {
            return Err(StoreError::LoginTaken(local.login.to_string()));
        }
        d.accounts.push(account.clone());
        Ok(())
    }

    fn account(&self, id: &AccountId) -> Result<Option<Account>> {
        Ok(self
            .identities()
            .accounts
            .iter()
            .find(|a| a.id == *id)
            .cloned())
    }

    fn account_by_login(&self, login: &LoginName) -> Result<Option<Account>> {
        Ok(self
            .identities()
            .accounts
            .iter()
            .find(|a| a.local.as_ref().is_some_and(|l| l.login == *login))
            .cloned())
    }

    fn account_by_identity(&self, identity: &ExternalIdentity) -> Result<Option<Account>> {
        let d = self.identities();
        let Some((_, id)) = d.links.iter().find(|(i, _)| i == identity) else {
            return Ok(None);
        };
        Ok(d.accounts.iter().find(|a| a.id == *id).cloned())
    }

    fn link_identity(&self, id: &AccountId, identity: &ExternalIdentity, _at: i64) -> Result<()> {
        let mut d = self.identities();
        if !d.accounts.iter().any(|a| a.id == *id) {
            return Err(StoreError::AccountNotFound);
        }
        match d.links.iter().find(|(i, _)| i == identity) {
            Some((_, owner)) if owner == id => Ok(()),
            Some(_) => Err(StoreError::IdentityTaken),
            None => {
                d.links.push((identity.clone(), id.clone()));
                Ok(())
            }
        }
    }

    fn set_local_credential(
        &self,
        id: &AccountId,
        credential: Option<&LocalCredential>,
    ) -> Result<()> {
        let mut d = self.identities();
        // The account is checked first, as the SQLite store does, so the two answer alike.
        d.account_mut(id)?;
        if let Some(c) = credential {
            if !looks_like_an_argon2id_hash(&c.password_hash) {
                return Err(StoreError::InvalidCredential);
            }
            if d.login_taken(&c.login, Some(id)) {
                return Err(StoreError::LoginTaken(c.login.to_string()));
            }
        }
        d.account_mut(id)?.local = credential.cloned();
        Ok(())
    }

    fn set_disabled(&self, id: &AccountId, disabled: bool) -> Result<()> {
        self.identities().account_mut(id)?.disabled = disabled;
        Ok(())
    }
}

impl SessionStore for MemoryStore {
    fn add_session(&self, session: &SessionRecord) -> Result<()> {
        let mut d = self.identities();
        if !d.accounts.iter().any(|a| a.id == session.account) {
            return Err(StoreError::AccountNotFound);
        }
        if d.sessions.iter().any(|s| s.id_hash == session.id_hash) {
            return Err(StoreError::IdTaken("session".to_owned()));
        }
        d.sessions.push(session.clone());
        Ok(())
    }

    fn session(&self, id_hash: &str, now: i64) -> Result<Option<SessionRecord>> {
        Ok(self
            .identities()
            .sessions
            .iter()
            .find(|s| s.id_hash == id_hash && s.expires_at > now)
            .cloned())
    }

    fn end_session(&self, id_hash: &str) -> Result<bool> {
        let mut d = self.identities();
        let before = d.sessions.len();
        d.sessions.retain(|s| s.id_hash != id_hash);
        Ok(d.sessions.len() < before)
    }

    fn end_sessions_of(&self, id: &AccountId) -> Result<u64> {
        let mut d = self.identities();
        let before = d.sessions.len();
        d.sessions.retain(|s| s.account != *id);
        Ok((before - d.sessions.len()) as u64)
    }

    fn purge_expired_sessions(&self, now: i64) -> Result<u64> {
        let mut d = self.identities();
        let before = d.sessions.len();
        d.sessions.retain(|s| s.expires_at > now);
        Ok((before - d.sessions.len()) as u64)
    }
}

impl TokenStore for MemoryStore {
    fn add_token(&self, token: &TokenRecord) -> Result<()> {
        let mut d = self.identities();
        if !d.accounts.iter().any(|a| a.id == token.account) {
            return Err(StoreError::AccountNotFound);
        }
        if d.tokens.iter().any(|t| t.id == token.id) {
            return Err(StoreError::IdTaken(token.id.to_string()));
        }
        if d.tokens.iter().any(|t| t.hash == token.hash) {
            return Err(StoreError::IdTaken("token".to_owned()));
        }
        let mut stored = token.clone();
        stored.label = crate::identity::clean_label(&stored.label);
        d.tokens.push(stored);
        Ok(())
    }

    fn token_by_hash(&self, hash: &str) -> Result<Option<TokenRecord>> {
        Ok(self
            .identities()
            .tokens
            .iter()
            .find(|t| t.hash == hash)
            .cloned())
    }

    fn tokens_of(&self, account: &AccountId) -> Result<Vec<TokenRecord>> {
        let mut mine: Vec<TokenRecord> = self
            .identities()
            .tokens
            .iter()
            .filter(|t| t.account == *account)
            .cloned()
            .collect();
        // Oldest first by creation time, ties in the order they were stored: the order SQLite gives.
        mine.sort_by_key(|t| t.created_at);
        Ok(mine)
    }

    fn revoke_token(&self, account: &AccountId, id: &TokenId, at: i64) -> Result<bool> {
        let mut d = self.identities();
        match d
            .tokens
            .iter_mut()
            .find(|t| t.id == *id && t.account == *account && t.revoked_at.is_none())
        {
            Some(t) => {
                t.revoked_at = Some(at);
                Ok(true)
            }
            None => Ok(false),
        }
    }

    fn revoke_tokens_of(&self, account: &AccountId, at: i64) -> Result<u64> {
        let mut d = self.identities();
        let mut n = 0;
        for t in d
            .tokens
            .iter_mut()
            .filter(|t| t.account == *account && t.revoked_at.is_none())
        {
            t.revoked_at = Some(at);
            n += 1;
        }
        Ok(n)
    }
}
