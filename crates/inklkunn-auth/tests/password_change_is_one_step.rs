//! A password change is one store step (corrections H5), seen through the authenticator: when the store's step
//! fails, the old password, every session and every token are as they were, and the person is told the truth
//! (*unavailable*, not *changed*). The store here is the in-memory one with its one-step method made to fail;
//! everything else is delegated, so a change made in separate calls (credential, then sessions, then tokens)
//! would leave the password changed behind the failure and fail this test.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use common::{ALICE_PASSWORD, Counting, NOW, config, id, ip};
use inklkunn_auth::{AccountError, AuthError, Authenticator};
use inklkunn_store::{
    Account, AccountId, AccountStore, CredentialStore, DisplayName, ExternalIdentity,
    InvitationRecord, InvitationStore, LocalCredential, LoginName, MemoryStore, Result, RoleGrant,
    Scope, SessionRecord, SessionStore, StoreError, TokenId, TokenRecord, TokenStore,
};

/// The in-memory store, whose one-step credential change fails while `failing` is set.
struct Flaky {
    inner: MemoryStore,
    failing: AtomicBool,
}

impl AccountStore for Flaky {
    fn add_account(&self, a: &Account) -> Result<()> {
        self.inner.add_account(a)
    }
    fn account(&self, id: &AccountId) -> Result<Option<Account>> {
        self.inner.account(id)
    }
    fn account_by_login(&self, login: &LoginName) -> Result<Option<Account>> {
        self.inner.account_by_login(login)
    }
    fn account_by_identity(&self, identity: &ExternalIdentity) -> Result<Option<Account>> {
        self.inner.account_by_identity(identity)
    }
    fn link_identity(&self, id: &AccountId, identity: &ExternalIdentity, at: i64) -> Result<()> {
        self.inner.link_identity(id, identity, at)
    }
    fn add_account_with_identity(
        &self,
        a: &Account,
        identity: &ExternalIdentity,
        at: i64,
    ) -> Result<()> {
        self.inner.add_account_with_identity(a, identity, at)
    }
    fn set_local_credential(&self, id: &AccountId, c: Option<&LocalCredential>) -> Result<()> {
        self.inner.set_local_credential(id, c)
    }
    fn set_disabled(&self, id: &AccountId, disabled: bool) -> Result<()> {
        self.inner.set_disabled(id, disabled)
    }
    fn set_display_name(&self, id: &AccountId, name: &DisplayName) -> Result<()> {
        self.inner.set_display_name(id, name)
    }
}

impl SessionStore for Flaky {
    fn add_session(&self, s: &SessionRecord) -> Result<()> {
        self.inner.add_session(s)
    }
    fn session(&self, id_hash: &str, now: i64) -> Result<Option<SessionRecord>> {
        self.inner.session(id_hash, now)
    }
    fn end_session(&self, id_hash: &str) -> Result<bool> {
        self.inner.end_session(id_hash)
    }
    fn end_sessions_of(&self, id: &AccountId) -> Result<u64> {
        self.inner.end_sessions_of(id)
    }
    fn sessions_of(&self, id: &AccountId, now: i64) -> Result<Vec<SessionRecord>> {
        self.inner.sessions_of(id, now)
    }
    fn purge_expired_sessions(&self, now: i64) -> Result<u64> {
        self.inner.purge_expired_sessions(now)
    }
    fn keep_newest_sessions(&self, id: &AccountId, keep: usize, now: i64) -> Result<u64> {
        self.inner.keep_newest_sessions(id, keep, now)
    }
}

impl TokenStore for Flaky {
    fn add_token(&self, t: &TokenRecord) -> Result<()> {
        self.inner.add_token(t)
    }
    fn token_by_hash(&self, hash: &str) -> Result<Option<TokenRecord>> {
        self.inner.token_by_hash(hash)
    }
    fn tokens_of(&self, account: &AccountId) -> Result<Vec<TokenRecord>> {
        self.inner.tokens_of(account)
    }
    fn revoke_token(&self, account: &AccountId, id: &TokenId, at: i64) -> Result<bool> {
        self.inner.revoke_token(account, id, at)
    }
    fn revoke_tokens_of(&self, account: &AccountId, at: i64) -> Result<u64> {
        self.inner.revoke_tokens_of(account, at)
    }
}

impl InvitationStore for Flaky {
    fn add_invitation(&self, i: &InvitationRecord) -> Result<()> {
        self.inner.add_invitation(i)
    }
    fn invitation_by_hash(&self, hash: &str) -> Result<Option<InvitationRecord>> {
        self.inner.invitation_by_hash(hash)
    }
    fn redeem_invitation(
        &self,
        hash: &str,
        account: &Account,
        now: i64,
    ) -> Result<Option<RoleGrant>> {
        self.inner.redeem_invitation(hash, account, now)
    }
    fn redeem_invitation_with_identity(
        &self,
        hash: &str,
        account: &Account,
        identity: &ExternalIdentity,
        now: i64,
    ) -> Result<Option<RoleGrant>> {
        self.inner
            .redeem_invitation_with_identity(hash, account, identity, now)
    }
}

impl CredentialStore for Flaky {
    fn change_credential(
        &self,
        id: &AccountId,
        credential: &LocalCredential,
        session: &SessionRecord,
        at: i64,
    ) -> Result<()> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(StoreError::Backend("the disk is gone".into()));
        }
        self.inner.change_credential(id, credential, session, at)
    }
}

const NEW: &str = "a different password altogether";

#[test]
fn a_failure_of_the_store_step_leaves_the_old_password_the_sessions_and_the_tokens() {
    let store = Arc::new(Flaky {
        inner: MemoryStore::new(),
        failing: AtomicBool::new(false),
    });
    let auth = Authenticator::with_hasher(store.clone(), config(), Counting::new())
        .expect("authenticator");
    let alice = auth
        .register_local("alice", "Alice", ALICE_PASSWORD, NOW)
        .unwrap();
    let c = ip("203.0.113.4");
    let here = auth.sign_in("alice", ALICE_PASSWORD, &c, NOW).unwrap();
    let there = auth.sign_in("alice", ALICE_PASSWORD, &c, NOW + 1).unwrap();
    let token = auth
        .issue_token(&id(&alice), Scope::Read, "t", 3600, NOW)
        .unwrap();

    store.failing.store(true, Ordering::SeqCst);
    let failed = auth.change_own_password(&id(&alice), ALICE_PASSWORD, NEW, &c, NOW + 2);
    assert!(
        matches!(failed, Err(AccountError::Auth(AuthError::Backend(_)))),
        "{failed:?}: the person is told it is unavailable, not that the password changed"
    );

    // Nothing changed: both sessions are live, the token works, the old password signs in, the new does not.
    for session in [&here, &there] {
        assert!(
            auth.principal_for_session(session.session.secret().as_str(), NOW + 3)
                .unwrap()
                .is_some()
        );
    }
    assert!(
        auth.principal_for_token(token.secret.as_str(), NOW + 3)
            .is_ok()
    );
    assert!(auth.sign_in("alice", NEW, &c, NOW + 4).is_err());
    assert!(auth.sign_in("alice", ALICE_PASSWORD, &c, NOW + 5).is_ok());

    // And once the store is back, the same change goes through.
    store.failing.store(false, Ordering::SeqCst);
    auth.change_own_password(&id(&alice), ALICE_PASSWORD, NEW, &c, NOW + 6)
        .unwrap();
    assert!(auth.sign_in("alice", NEW, &c, NOW + 7).is_ok());
}
