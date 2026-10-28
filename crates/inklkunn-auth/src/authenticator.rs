//! The pieces together: a credential in, a [`Principal`] out (RFC 002 D-1).
//!
//! **Sign-in is constant-work.** Whatever the sign-in name and password are, a password sign-in that gets
//! past the throttles does exactly one Argon2id verification: against the account's hash when there is one,
//! against a dummy hash made at start when there is not (no such account, a name no account could have, an
//! account with no password), and the answer for every one of those, and for a wrong password, a disabled
//! account and a correct password for one, is the same [`AuthError::BadCredentials`] after the same work.
//! The one verification takes a permit from a bounded gate first. The throttles are consulted **before** any
//! hashing, so a locked name or address costs a map lookup, and a correct password is refused while locked.
//!
//! Nothing here can be asked to *make* a principal from nothing: [`SignedIn`], [`SessionPrincipal`] and the
//! OpenID Connect [`VerifiedIdentity`] are `#[non_exhaustive]` or have no public constructor, so code outside
//! this crate can read them and cannot forge them.

use std::sync::Arc;
use std::time::Duration;

use inklkunn_core::Principal;
use inklkunn_store::{
    Account, AccountId, AccountStore, DisplayName, DisplayNameError, ExternalIdentity,
    LocalCredential, LoginName, LoginNameError, SessionStore, StoreError, TokenId, TokenStore,
};

use crate::error::AuthError;
use crate::gate::WorkGate;
use crate::hashing::{
    Argon2idHasher, PASSWORD_MAX_BYTES, PasswordHasher, PasswordRefusal, check_new_password,
};
use crate::oidc::VerifiedIdentity;
use crate::proxy::{ClientAddress, ProxyPolicy};
use crate::random;
use crate::session::{CsrfToken, NewSession, SESSION_TTL_SECS, Sessions};
use crate::throttle::{Throttle, ThrottlePolicy};
use crate::token::{IssueError, IssuedToken, TokenSummary, Tokens};

/// How the authenticator is set up.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// How long a session lasts, in seconds.
    pub session_ttl_secs: i64,
    /// How many password verifications may run at once.
    pub hash_permits: usize,
    /// How long a sign-in waits for a permit before the service says it is busy.
    pub gate_wait: Duration,
    /// Failures per sign-in name.
    pub login_throttle: ThrottlePolicy,
    /// Failures per client address.
    pub address_throttle: ThrottlePolicy,
    /// Which peers are trusted proxies.
    pub proxies: ProxyPolicy,
}

impl Default for AuthConfig {
    fn default() -> Self {
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        AuthConfig {
            session_ttl_secs: SESSION_TTL_SECS,
            hash_permits: cores.clamp(1, 4),
            gate_wait: Duration::from_secs(5),
            login_throttle: ThrottlePolicy::PER_LOGIN,
            address_throttle: ThrottlePolicy::PER_ADDRESS,
            proxies: ProxyPolicy::none(),
        }
    }
}

/// A successful sign-in: who, and the session to put in a cookie.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SignedIn {
    /// The principal for the rest of the request.
    pub principal: Principal,
    /// The account.
    pub account: AccountId,
    /// The new session: its cookie and its CSRF token.
    pub session: NewSession,
}

/// A request's session, found and checked.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SessionPrincipal {
    /// The principal for the request.
    pub principal: Principal,
    /// The account.
    pub account: AccountId,
    /// The token every write made with this session must present.
    pub csrf: CsrfToken,
    /// When the session ends.
    pub expires_at: i64,
}

/// Why an account could not be made or changed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AccountError {
    /// The sign-in name is not acceptable.
    Login(LoginNameError),
    /// The display name is not acceptable.
    DisplayName(DisplayNameError),
    /// The password is not acceptable.
    Password(PasswordRefusal),
    /// Another account has the sign-in name.
    LoginTaken,
    /// Another account is linked to that external identity.
    IdentityTaken,
    /// No such account.
    NotFound,
    /// Hashing, a store or the random source failed, or the service is busy.
    Auth(AuthError),
}

impl std::fmt::Display for AccountError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AccountError::Login(e) => e.fmt(f),
            AccountError::DisplayName(e) => e.fmt(f),
            AccountError::Password(e) => e.fmt(f),
            AccountError::LoginTaken => f.write_str("another account has that sign-in name"),
            AccountError::IdentityTaken => {
                f.write_str("that external identity is linked to another account")
            }
            AccountError::NotFound => f.write_str("no such account"),
            AccountError::Auth(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for AccountError {}

impl From<AuthError> for AccountError {
    fn from(e: AuthError) -> Self {
        AccountError::Auth(e)
    }
}

impl From<StoreError> for AccountError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::LoginTaken(_) => AccountError::LoginTaken,
            StoreError::IdentityTaken => AccountError::IdentityTaken,
            StoreError::AccountNotFound => AccountError::NotFound,
            other => AccountError::Auth(other.into()),
        }
    }
}

/// The authenticator.
pub struct Authenticator {
    accounts: Arc<dyn AccountStore>,
    sessions: Sessions,
    tokens: Tokens,
    hasher: Arc<dyn PasswordHasher>,
    gate: WorkGate,
    /// What a sign-in with no password to check against verifies against, so the work is the same.
    dummy_hash: String,
    by_login: Throttle,
    by_address: Throttle,
    proxies: ProxyPolicy,
}

/// The throttle key for a sign-in name as typed: trimmed, ASCII case folded, at most 128 characters (so a
/// huge name cannot make a huge key).
fn login_key(typed: &str) -> String {
    typed
        .trim()
        .to_ascii_lowercase()
        .chars()
        .take(128)
        .collect()
}

impl Authenticator {
    /// An authenticator over one store that holds accounts, sessions and tokens.
    pub fn new<S>(store: Arc<S>, config: AuthConfig) -> Result<Self, AuthError>
    where
        S: AccountStore + SessionStore + TokenStore + 'static,
    {
        Self::build(store, config, Arc::new(Argon2idHasher::new()))
    }

    /// As [`Authenticator::new`] with a different hasher: a cheap one, or one that counts. Never available
    /// to a product build.
    #[cfg(any(test, feature = "test-knobs"))]
    pub fn with_hasher<S>(
        store: Arc<S>,
        config: AuthConfig,
        hasher: Arc<dyn PasswordHasher>,
    ) -> Result<Self, AuthError>
    where
        S: AccountStore + SessionStore + TokenStore + 'static,
    {
        Self::build(store, config, hasher)
    }

    fn build<S>(
        store: Arc<S>,
        config: AuthConfig,
        hasher: Arc<dyn PasswordHasher>,
    ) -> Result<Self, AuthError>
    where
        S: AccountStore + SessionStore + TokenStore + 'static,
    {
        let dummy_hash = hasher.hash(&random::token()?)?;
        Ok(Authenticator {
            accounts: store.clone(),
            sessions: Sessions::new(store.clone(), config.session_ttl_secs),
            tokens: Tokens::new(store),
            hasher,
            gate: WorkGate::new(config.hash_permits, config.gate_wait),
            dummy_hash,
            by_login: Throttle::new(config.login_throttle),
            by_address: Throttle::new(config.address_throttle),
            proxies: config.proxies,
        })
    }

    /// The client address to count for a connection from `peer` that carried `forwarded_for` (the
    /// `X-Forwarded-For` values joined with `, `): the peer unless it is a declared trusted proxy.
    pub fn client_address(
        &self,
        peer: std::net::IpAddr,
        forwarded_for: Option<&str>,
    ) -> ClientAddress {
        self.proxies.client(peer, forwarded_for)
    }

    fn locked(&self, key: &str, client: &ClientAddress, now: i64) -> Result<(), AuthError> {
        let a = self.by_login.check(key, now);
        let b = self.by_address.check(client.as_key(), now);
        match (a, b) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(x), Ok(())) | (Ok(()), Err(x)) => Err(AuthError::Locked { until: x }),
            (Err(x), Err(y)) => Err(AuthError::Locked { until: x.max(y) }),
        }
    }

    fn failed(&self, key: &str, client: &ClientAddress, now: i64) {
        self.by_login.record_failure(key, now);
        self.by_address.record_failure(client.as_key(), now);
    }

    /// Sign in with a sign-in name and a password.
    ///
    /// **Blocking**: it waits for a permit and verifies an Argon2id hash. Call it from `spawn_blocking`.
    pub fn sign_in(
        &self,
        login: &str,
        password: &str,
        client: &ClientAddress,
        now: i64,
    ) -> Result<SignedIn, AuthError> {
        let key = login_key(login);
        self.locked(&key, client, now)?;
        if password.len() > PASSWORD_MAX_BYTES {
            // No password this long was ever accepted, so nothing is verified; it is a failure all the same.
            self.failed(&key, client, now);
            return Err(AuthError::BadCredentials);
        }
        let permit = self.gate.acquire().ok_or(AuthError::Busy)?;
        let account = match LoginName::parse(login.trim()) {
            Ok(name) => self.accounts.account_by_login(&name)?,
            Err(_) => None,
        };
        let (stored, usable) = match account
            .as_ref()
            .and_then(|a| a.local.as_ref().map(|l| (a, l)))
        {
            Some((a, local)) => (local.password_hash.as_str(), !a.disabled),
            None => (self.dummy_hash.as_str(), false),
        };
        let verified = self.hasher.verify(password, stored);
        drop(permit);
        match account {
            Some(account) if verified && usable => {
                self.by_login.record_success(&key);
                self.start(&account, now)
            }
            _ => {
                self.failed(&key, client, now);
                Err(AuthError::BadCredentials)
            }
        }
    }

    fn start(&self, account: &Account, now: i64) -> Result<SignedIn, AuthError> {
        let session = self.sessions.start(&account.id, now)?;
        Ok(SignedIn {
            principal: Principal::session(account.id.clone()),
            account: account.id.clone(),
            session,
        })
    }

    /// Sign in with an identity an OpenID Connect provider just verified. An identity no account is linked
    /// to signs nobody in; so does a disabled account. Both are [`AuthError::BadCredentials`].
    pub fn sign_in_oidc(
        &self,
        identity: &VerifiedIdentity,
        now: i64,
    ) -> Result<SignedIn, AuthError> {
        let account = self
            .accounts
            .account_by_identity(identity.identity())?
            .ok_or(AuthError::BadCredentials)?;
        if account.disabled {
            return Err(AuthError::BadCredentials);
        }
        self.start(&account, now)
    }

    /// The principal for a request's session cookie, if the session is live and its account is enabled.
    pub fn principal_for_session(
        &self,
        secret: &str,
        now: i64,
    ) -> Result<Option<SessionPrincipal>, AuthError> {
        let Some(session) = self.sessions.resolve(secret, now)? else {
            return Ok(None);
        };
        let Some(account) = self.accounts.account(&session.account)? else {
            return Ok(None);
        };
        if account.disabled {
            return Ok(None);
        }
        Ok(Some(SessionPrincipal {
            principal: Principal::session(account.id.clone()),
            account: account.id,
            csrf: session.csrf,
            expires_at: session.expires_at,
        }))
    }

    /// The principal for a presented API token: it carries the scope the token was issued with. Every way a
    /// token can fail to authenticate is [`AuthError::BadCredentials`].
    pub fn principal_for_token(&self, presented: &str, now: i64) -> Result<Principal, AuthError> {
        let verified = self.tokens.verify(presented, now)?;
        let account = self
            .accounts
            .account(&verified.account)?
            .ok_or(AuthError::BadCredentials)?;
        if account.disabled {
            return Err(AuthError::BadCredentials);
        }
        Ok(Principal::token(verified.account, verified.scope))
    }

    /// Sign out: end the session the cookie names.
    pub fn sign_out(&self, secret: &str) -> Result<(), AuthError> {
        self.sessions.end(secret).map(|_| ())
    }

    // ---- accounts: primitives for the administration surface, which authorizes before it calls ----

    /// Make an account that signs in with a name and a password.
    ///
    /// **Blocking** (it hashes). The caller has already been authorized to administer accounts.
    pub fn register_local(
        &self,
        login: &str,
        display_name: &str,
        password: &str,
        now: i64,
    ) -> Result<Account, AccountError> {
        let login = LoginName::parse(login).map_err(AccountError::Login)?;
        let display_name = DisplayName::new(display_name).map_err(AccountError::DisplayName)?;
        let password_hash = self.hash_new_password(password)?;
        let account = Account {
            id: AccountId::new(),
            display_name,
            local: Some(LocalCredential {
                login,
                password_hash,
            }),
            created_at: now,
            disabled: false,
        };
        self.accounts.add_account(&account)?;
        Ok(account)
    }

    /// Make an account with no password, to be reached by a linked external identity.
    pub fn register_external(&self, display_name: &str, now: i64) -> Result<Account, AccountError> {
        let account = Account {
            id: AccountId::new(),
            display_name: DisplayName::new(display_name).map_err(AccountError::DisplayName)?,
            local: None,
            created_at: now,
            disabled: false,
        };
        self.accounts.add_account(&account)?;
        Ok(account)
    }

    /// Link an external identity to an account (an administrative act).
    pub fn link_external(
        &self,
        account: &AccountId,
        identity: &ExternalIdentity,
        now: i64,
    ) -> Result<(), AccountError> {
        Ok(self.accounts.link_identity(account, identity, now)?)
    }

    /// Give an account a sign-in name and a new password, and **end all its sessions** (whoever held one
    /// before must sign in again). **Blocking.**
    pub fn set_password(
        &self,
        account: &AccountId,
        login: &str,
        password: &str,
    ) -> Result<(), AccountError> {
        let login = LoginName::parse(login).map_err(AccountError::Login)?;
        let password_hash = self.hash_new_password(password)?;
        self.accounts.set_local_credential(
            account,
            Some(&LocalCredential {
                login,
                password_hash,
            }),
        )?;
        self.sessions.end_all(account)?;
        Ok(())
    }

    /// Disable an account and end all its sessions. Its tokens stop working too (they are checked against
    /// the account), and its history stays attributed to it.
    pub fn disable(&self, account: &AccountId) -> Result<(), AccountError> {
        self.accounts.set_disabled(account, true)?;
        self.sessions.end_all(account)?;
        Ok(())
    }

    /// Enable an account again.
    pub fn enable(&self, account: &AccountId) -> Result<(), AccountError> {
        Ok(self.accounts.set_disabled(account, false)?)
    }

    fn hash_new_password(&self, password: &str) -> Result<String, AccountError> {
        check_new_password(password).map_err(AccountError::Password)?;
        let _permit = self.gate.acquire().ok_or(AuthError::Busy)?;
        Ok(self.hasher.hash(password)?)
    }

    // ---- tokens ----

    /// Issue an API token for an account (shown once). The account's own act; the web layer checks the
    /// session and the CSRF token first.
    pub fn issue_token(
        &self,
        account: &AccountId,
        scope: inklkunn_store::Scope,
        label: &str,
        lifetime_secs: i64,
        now: i64,
    ) -> Result<IssuedToken, IssueError> {
        self.tokens.issue(account, scope, label, lifetime_secs, now)
    }

    /// Revoke one of the account's tokens.
    pub fn revoke_token(
        &self,
        account: &AccountId,
        id: &TokenId,
        now: i64,
    ) -> Result<bool, AuthError> {
        self.tokens.revoke(account, id, now)
    }

    /// The account's tokens, without their secrets.
    pub fn tokens(&self, account: &AccountId) -> Result<Vec<TokenSummary>, AuthError> {
        self.tokens.list(account)
    }
}

impl std::fmt::Debug for Authenticator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Authenticator").finish_non_exhaustive()
    }
}
