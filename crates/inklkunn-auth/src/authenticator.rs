//! The pieces together: a credential in, a [`Principal`] out (RFC 002 D-1).
//!
//! **Sign-in is constant-work.** Whatever the sign-in name and password are, a password sign-in that gets
//! past the throttles does exactly one Argon2id verification: against the account's hash when there is a
//! usable one, against a dummy hash made at start when there is not (no such account, a name no account could
//! have, an account with no password, a stored string that cannot be verified), and the answer for every one
//! of those, and for a wrong password, a disabled account and a correct password for one, is the same
//! [`AuthError::BadCredentials`] after the same work. The one verification takes a permit from a bounded gate
//! first.
//!
//! **The throttles are counted before the work, not after it**: an attempt is checked
//! against the lock and counted in one step, so a burst of simultaneous guesses is held to the limit as a
//! sequence is, and a locked name or address costs a map lookup and a correct password is refused while
//! locked. An attempt that turns out right gives its count back (an address keeps what others failed), and one
//! that was never judged (the service was busy, a store failed) gives it back too.
//!
//! **What cannot be made outside this crate.** [`SignedIn`] and [`SessionPrincipal`] are `#[non_exhaustive]`,
//! [`NewSession`] and the OpenID Connect
//! [`VerifiedIdentity`] have no public constructor, the session and token stores
//! are reachable only through this type, and a [`ClientAddress`] comes only from the trusted-proxy rule. A
//! [`Principal`] is `inklkunn-core`'s type, and core's gate (not the type system) keeps web sources from naming
//! how one is made.
//!
//! **Blocking.** Everything that hashes or waits for a permit is a plain function: call it from
//! `spawn_blocking`, never from an async worker.

use std::sync::Arc;
use std::time::Duration;

use inklkunn_core::{IdentityAuthority, Principal};
use inklkunn_store::{
    Account, AccountId, AccountStore, AuditEvent, AuditOutcome, AuditSink, DisplayName,
    DisplayNameError, ExternalIdentity, InvitationStore, LocalCredential, LoginName,
    LoginNameError, Role, SessionStore, StoreError, TokenId, TokenStore, WikiId,
};

use crate::error::AuthError;
use crate::gate::WorkGate;
use crate::hashing::{
    Argon2idHasher, PASSWORD_MAX_BYTES, PasswordHasher, PasswordRefusal, check_new_password,
};
use crate::invitation::{Invitations, InviteError, IssuedInvitation, JoinPolicy};
use crate::oidc::{
    BeginSignIn, Landing, OidcError, OidcProvider, OidcPurpose, Purpose, clear_state_cookie,
};
use crate::proxy::{ClientAddress, ProxyPolicy};
use crate::random;
use crate::session::{CsrfToken, NewSession, SESSION_TTL_SECS, SessionSummary, Sessions};
use crate::throttle::{Begin, Throttle, ThrottlePolicy};
use crate::token::{IssueError, IssuedToken, TokenSummary, Tokens};

/// The shortest and longest a session may be configured to last.
const SESSION_TTL_RANGE: std::ops::RangeInclusive<i64> = 60..=90 * 24 * 3600;

/// How the authenticator is set up.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// How long a session lasts, in seconds (a minute to ninety days).
    pub session_ttl_secs: i64,
    /// How many password verifications may run at once (at least one).
    pub hash_permits: usize,
    /// How long a sign-in waits for a permit before the service says it is busy. A waiter holds a thread, so
    /// this stays short.
    pub gate_wait: Duration,
    /// Failures per sign-in name.
    pub login_throttle: ThrottlePolicy,
    /// Failures per client address.
    pub address_throttle: ThrottlePolicy,
    /// How many OpenID Connect sign-ins one client address may start.
    pub oidc_start_throttle: ThrottlePolicy,
    /// Which peers are trusted proxies.
    pub proxies: ProxyPolicy,
    /// Who may make an account for themselves. **Off unless the operator says otherwise.**
    pub join: JoinPolicy,
    /// How many accounts one client address may start making.
    pub join_throttle: ThrottlePolicy,
}

impl Default for AuthConfig {
    fn default() -> Self {
        let cores = std::thread::available_parallelism().map_or(1, usize::from);
        AuthConfig {
            session_ttl_secs: SESSION_TTL_SECS,
            hash_permits: cores.clamp(1, 4),
            gate_wait: Duration::from_secs(2),
            login_throttle: ThrottlePolicy::PER_LOGIN,
            address_throttle: ThrottlePolicy::PER_ADDRESS,
            oidc_start_throttle: ThrottlePolicy::OIDC_STARTS_PER_ADDRESS,
            proxies: ProxyPolicy::none(),
            join: JoinPolicy::Off,
            join_throttle: ThrottlePolicy::JOINS_PER_ADDRESS,
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

/// A successful OpenID Connect sign-in: the sign-in, and the page the callback must be answered with.
#[derive(Debug, Clone)]
#[non_exhaustive]
#[must_use]
pub struct OidcSignedIn {
    /// Who signed in and the new session (its secret is also in the landing's cookies).
    pub signed_in: SignedIn,
    /// Whether this sign-in made the account: a first sign-in where joining through the provider is open, or an
    /// invitation redeemed with the provider.
    pub account_created: bool,
    /// The answer to the callback. **Send it as it is**, not a redirect: see [`Landing`].
    pub landing: Landing,
}

/// A request's session, found and checked.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SessionPrincipal {
    /// The principal for the request.
    pub principal: Principal,
    /// The account.
    pub account: AccountId,
    /// The token every write made with this session must present (for the form to carry).
    pub csrf: CsrfToken,
    /// When the session ends.
    pub expires_at: i64,
}

/// What a state-changing request's session and CSRF token add up to. A write goes through
/// [`Authenticator::principal_for_write`], which cannot be answered without the token.
#[derive(Debug, Clone)]
#[must_use]
pub enum SessionCheck {
    /// A live session, and the request presented its CSRF token.
    Valid(SessionPrincipal),
    /// A live session, but the token is missing or wrong: refuse the write, and say the form expired.
    CsrfMismatch,
    /// No live session.
    SignedOut,
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
    /// The invitation is unknown, used or over (one answer for all three).
    InvitationUnusable,
    /// Joining without an invitation is not open on this server.
    JoinClosed,
    /// Hashing, a store or the random source failed, the credential did not authenticate, or the service is
    /// busy.
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
            AccountError::InvitationUnusable => {
                f.write_str("that invitation is unknown, used or over")
            }
            AccountError::JoinClosed => f.write_str("joining is not open on this server"),
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
            StoreError::InvitationUnusable => AccountError::InvitationUnusable,
            other => AccountError::Auth(other.into()),
        }
    }
}

/// Why an OpenID Connect sign-in did not complete.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum OidcSignInError {
    /// The provider flow failed; the reason is for the log (a person sees "sign-in failed").
    Oidc(OidcError),
    /// The account is disabled, this client address has started or made too many, or a store failed.
    Auth(AuthError),
    /// The provider vouched for the person, but no account here is linked to that identity and joining through
    /// the provider is not open. (The person is told so, and told to ask for an invitation: the identity is
    /// their own, so nothing about anyone else is given away.)
    NotLinked,
    /// The invitation's sign-in found that the person's provider identity already belongs to an account here.
    /// The invitation is **not** spent: the person signs in with that account instead.
    IdentityTaken,
    /// The invitation is unknown, used or over.
    InvitationUnusable,
}

impl std::fmt::Display for OidcSignInError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OidcSignInError::Oidc(e) => e.fmt(f),
            OidcSignInError::Auth(e) => e.fmt(f),
            OidcSignInError::NotLinked => f.write_str(
                "no account is linked to that identity and joining through the provider is not open",
            ),
            OidcSignInError::IdentityTaken => {
                f.write_str("that identity is linked to an account already")
            }
            OidcSignInError::InvitationUnusable => {
                f.write_str("that invitation is unknown, used or over")
            }
        }
    }
}

impl std::error::Error for OidcSignInError {}

/// The authenticator.
pub struct Authenticator {
    accounts: Arc<dyn AccountStore>,
    sessions: Sessions,
    tokens: Tokens,
    invitations: Invitations,
    hasher: Arc<dyn PasswordHasher>,
    gate: WorkGate,
    /// What a sign-in with no usable hash to check against verifies against, so the work is the same.
    dummy_hash: String,
    by_login: Throttle,
    by_address: Throttle,
    by_oidc_start: Throttle,
    by_join: Throttle,
    join: JoinPolicy,
    /// The store the single-step redemption goes through (the same store the other records are in).
    redemption: Arc<dyn InvitationStore>,
    proxies: ProxyPolicy,
    oidc: Option<OidcProvider>,
    audit: Option<Arc<dyn AuditSink>>,
    /// The right to say who a request is: this authenticator holds the process's only one, so no other code can
    /// build an account principal (corrections handoff C7).
    authority: IdentityAuthority,
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
    /// An authenticator over one store that holds accounts, sessions and tokens. It takes the process's
    /// [`IdentityAuthority`] (`IdentityAuthority::claim()`, once, in the binary) and keeps it: every principal
    /// that exists is built here.
    pub fn new<S>(
        store: Arc<S>,
        config: AuthConfig,
        authority: IdentityAuthority,
    ) -> Result<Self, AuthError>
    where
        S: AccountStore + SessionStore + TokenStore + InvitationStore + 'static,
    {
        Self::build(store, config, Arc::new(Argon2idHasher::new()), authority)
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
        S: AccountStore + SessionStore + TokenStore + InvitationStore + 'static,
    {
        Self::build(store, config, hasher, IdentityAuthority::for_tests())
    }

    fn build<S>(
        store: Arc<S>,
        config: AuthConfig,
        hasher: Arc<dyn PasswordHasher>,
        authority: IdentityAuthority,
    ) -> Result<Self, AuthError>
    where
        S: AccountStore + SessionStore + TokenStore + InvitationStore + 'static,
    {
        if !SESSION_TTL_RANGE.contains(&config.session_ttl_secs) {
            return Err(AuthError::Misconfigured(
                "a session lasts between a minute and ninety days",
            ));
        }
        if config.hash_permits == 0 {
            return Err(AuthError::Misconfigured(
                "at least one password verification must be allowed at a time",
            ));
        }
        let dummy_hash = hasher.hash(&random::token()?)?;
        Ok(Authenticator {
            accounts: store.clone(),
            sessions: Sessions::new(store.clone(), config.session_ttl_secs),
            tokens: Tokens::new(store.clone()),
            invitations: Invitations::new(store.clone()),
            redemption: store,
            hasher,
            gate: WorkGate::new(config.hash_permits, config.gate_wait),
            dummy_hash,
            by_login: Throttle::new(config.login_throttle),
            by_address: Throttle::new(config.address_throttle),
            by_oidc_start: Throttle::new(config.oidc_start_throttle),
            by_join: Throttle::new(config.join_throttle),
            join: config.join,
            proxies: config.proxies,
            oidc: None,
            audit: None,
            authority,
        })
    }

    /// Record the moment a name or an address becomes locked in the audit trail (no name, no address: the
    /// fact and its end). Without a sink a lockout is visible only as the refusals it causes.
    #[must_use]
    pub fn with_audit(mut self, sink: Arc<dyn AuditSink>) -> Self {
        self.audit = Some(sink);
        self
    }

    /// Enable OpenID Connect sign-in with this provider.
    #[must_use]
    pub fn with_oidc(mut self, provider: OidcProvider) -> Self {
        self.oidc = Some(provider);
        self
    }

    /// The client address to count for a connection from `peer` that carried `forwarded_for` (the
    /// `X-Forwarded-For` values joined with `, `): the peer unless it is a declared trusted proxy. When a
    /// trusted proxy's header cannot be read, [`ClientAddress::fell_back`] says so.
    pub fn client_address(
        &self,
        peer: std::net::IpAddr,
        forwarded_for: Option<&str>,
    ) -> ClientAddress {
        self.proxies.client(peer, forwarded_for)
    }

    fn note_lock(&self, what: &str, until: i64, now: i64) {
        if let Some(audit) = &self.audit {
            // A lockout is worth a line; failing to write that line must not change the answer given.
            let _ = audit.record(&AuditEvent::new(
                now,
                "anonymous",
                "sign-in-lock",
                what,
                AuditOutcome::Denied,
                &format!("until {until}"),
            ));
        }
    }

    /// Count this attempt against the client address and the sign-in name, or say which is locked. The
    /// address goes first, so a locked address costs the name nothing, and a name that turns out to be locked
    /// gives the address its attempt back.
    fn admit(&self, key: &str, client: &ClientAddress, now: i64) -> Result<(), AuthError> {
        match self.by_address.begin(client.as_key(), now) {
            Begin::Locked(until) => return Err(AuthError::Locked { until }),
            Begin::Admitted { locks_now } => {
                if let Some(until) = locks_now {
                    self.note_lock("address-throttle", until, now);
                }
            }
        }
        match self.by_login.begin(key, now) {
            Begin::Locked(until) => {
                self.by_address.refund(client.as_key());
                Err(AuthError::Locked { until })
            }
            Begin::Admitted { locks_now } => {
                if let Some(until) = locks_now {
                    self.note_lock("login-throttle", until, now);
                }
                Ok(())
            }
        }
    }

    fn give_back(&self, key: &str, client: &ClientAddress) {
        self.by_login.refund(key);
        self.by_address.refund(client.as_key());
    }

    /// Check a sign-in name and password. One verification, whatever they are; the same answer for every way
    /// to fail; counted before it is judged.
    fn verify_password(
        &self,
        login_name: &str,
        password: &str,
        client: &ClientAddress,
        now: i64,
    ) -> Result<Account, AuthError> {
        let key = login_key(login_name);
        self.admit(&key, client, now)?;
        if password.len() > PASSWORD_MAX_BYTES {
            // No password this long was ever accepted, so nothing is verified; it stays counted.
            return Err(AuthError::BadCredentials);
        }
        let Some(permit) = self.gate.acquire() else {
            self.give_back(&key, client);
            return Err(AuthError::Busy);
        };
        let account = match LoginName::parse(login_name.trim()) {
            Ok(name) => match self.accounts.account_by_login(&name) {
                Ok(found) => found,
                Err(e) => {
                    self.give_back(&key, client);
                    return Err(e.into());
                }
            },
            Err(_) => None,
        };
        let own = account
            .as_ref()
            .and_then(|a| a.local.as_ref().map(|l| (a, l)));
        let (stored, usable) = match own {
            Some((a, local)) if self.hasher.is_verifiable(&local.password_hash) => {
                (local.password_hash.as_str(), !a.disabled)
            }
            _ => (self.dummy_hash.as_str(), false),
        };
        let verified = self.hasher.verify(password, stored);
        drop(permit);
        match account {
            Some(account) if verified && usable => {
                self.by_login.clear(&key);
                self.by_address.refund(client.as_key());
                Ok(account)
            }
            _ => Err(AuthError::BadCredentials),
        }
    }

    /// Sign in with a sign-in name and a password.
    ///
    /// **Blocking**: it waits for a permit and verifies an Argon2id hash. Call it from `spawn_blocking`.
    ///
    /// A store failure after a correct password is reported as a backend failure, not as bad credentials: a
    /// person who typed the right password is told the truth about an outage. (Someone who guesses can learn
    /// that a password was right only by also arranging for the store to fail at that instant.)
    pub fn sign_in(
        &self,
        login_name: &str,
        password: &str,
        client: &ClientAddress,
        now: i64,
    ) -> Result<SignedIn, AuthError> {
        let account = self.verify_password(login_name, password, client, now)?;
        self.start(&account, now)
    }

    /// Check that whoever is at the keyboard knows this account's password (the re-authentication a sensitive
    /// act asks for), without starting a session. Throttled and constant-work exactly as [`Authenticator::sign_in`].
    /// The caller compares the account returned with the one it expects.
    ///
    /// **Blocking.**
    pub fn reauthenticate(
        &self,
        login_name: &str,
        password: &str,
        client: &ClientAddress,
        now: i64,
    ) -> Result<AccountId, AuthError> {
        self.verify_password(login_name, password, client, now)
            .map(|a| a.id)
    }

    fn start(&self, account: &Account, now: i64) -> Result<SignedIn, AuthError> {
        let session = self.sessions.start(&account.id, now)?;
        Ok(SignedIn {
            principal: Principal::session(&self.authority, account.id.clone()),
            account: account.id.clone(),
            session,
        })
    }

    /// Is an OpenID Connect provider configured? A page offers the provider's button only then.
    pub fn oidc_configured(&self) -> bool {
        self.oidc.is_some()
    }

    /// The configured provider's issuer, for a page to name it; `None` without one.
    pub fn oidc_issuer(&self) -> Option<&str> {
        self.oidc.as_ref().map(OidcProvider::issuer)
    }

    /// Start an OpenID Connect sign-in for `purpose`: spends one of the client address's sign-in starts (so an
    /// address cannot flood the logins in flight), then asks the provider. Send the browser to the returned URL
    /// and set the returned state cookie. A purpose that is an invitation must be usable now.
    ///
    /// **Blocking** (it may fetch the provider's discovery document).
    pub fn begin_oidc(
        &self,
        purpose: OidcPurpose<'_>,
        client: &ClientAddress,
        now: i64,
    ) -> Result<BeginSignIn, OidcSignInError> {
        let provider = self
            .oidc
            .as_ref()
            .ok_or(OidcSignInError::Oidc(OidcError::NotConfigured))?;
        match self.by_oidc_start.begin(client.as_key(), now) {
            Begin::Locked(until) => {
                return Err(OidcSignInError::Auth(AuthError::Locked { until }));
            }
            Begin::Admitted { locks_now } => {
                if let Some(until) = locks_now {
                    self.note_lock("oidc-start-throttle", until, now);
                }
            }
        }
        let started = self.start_oidc(provider, purpose, now);
        if started.is_err() {
            // Nothing was started: the start is given back.
            self.by_oidc_start.refund(client.as_key());
        }
        started
    }

    fn start_oidc(
        &self,
        provider: &OidcProvider,
        purpose: OidcPurpose<'_>,
        now: i64,
    ) -> Result<BeginSignIn, OidcSignInError> {
        if let OidcPurpose::Invitation { secret, .. } = purpose
            && !self
                .invitations
                .usable(secret, now)
                .map_err(OidcSignInError::Auth)?
        {
            return Err(OidcSignInError::InvitationUnusable);
        }
        provider
            .begin_sign_in(purpose.owned(), now)
            .map_err(OidcSignInError::Oidc)
    }

    /// Finish an OpenID Connect sign-in from the provider's callback: `state` and `code` are its query
    /// parameters and `cookie_header` the request's `Cookie` header (the state cookie is read from it). A
    /// callback this browser did not start and a state used twice end before any account is looked at; a token
    /// that fails a check ends with its reason for the log ([`OidcSignInError::Oidc`]).
    ///
    /// What happens to the identity the provider vouches for depends on what the sign-in was for (told to
    /// [`Authenticator::begin_oidc`]):
    ///
    /// * **sign in**: an account linked to the identity signs in (a disabled one does not). Where the join
    ///   setting lets a provider sign-in make accounts, an identity nobody linked makes one now (counted per
    ///   client address, the new-account quotas, no role beyond what the wiki gives everyone). Otherwise it
    ///   signs nobody in ([`OidcSignInError::NotLinked`]). **Never matched or linked by e-mail address**:
    ///   the identity is the provider's `iss` and `sub` and nothing else, so one person is one account.
    /// * **an invitation**: the account the invitation is for is made, bound to the identity (no password), the
    ///   role it named is granted and the invitation is spent, in one step. An identity that already has an
    ///   account spends nothing ([`OidcSignInError::IdentityTaken`]).
    ///
    /// **Answer a success with its [`OidcSignedIn::landing`] exactly as it is.** The session cookie is
    /// `SameSite=Strict`, and the callback is a navigation the provider started on another site: a redirect
    /// from it would reach the landing page *without* the session cookie, so the person would see the site
    /// signed out. The landing is a `200` page that moves the browser on with a request of this site's own,
    /// which does carry it. The landing also removes the one-shot state cookie. On an error, send
    /// [`clear_state_cookie`] with the refusal.
    ///
    /// **Blocking** (the token exchange is a request to the provider).
    pub fn finish_oidc(
        &self,
        state: &str,
        cookie_header: Option<&str>,
        code: &str,
        client: &ClientAddress,
        now: i64,
    ) -> Result<OidcSignedIn, OidcSignInError> {
        let provider = self
            .oidc
            .as_ref()
            .ok_or(OidcSignInError::Oidc(OidcError::NotConfigured))?;
        let (identity, purpose) = provider
            .finish_sign_in(state, cookie_header, code, now)
            .map_err(OidcSignInError::Oidc)?;
        let (account, created, next) = match purpose {
            Purpose::SignIn {
                next,
                after_joining,
            } => match self
                .accounts
                .account_by_identity(identity.identity())
                .map_err(|e| OidcSignInError::Auth(e.into()))?
            {
                Some(account) if account.disabled => {
                    return Err(OidcSignInError::Auth(AuthError::BadCredentials));
                }
                Some(account) => (account, false, next),
                None if self.join.lets_a_provider_sign_in_join() => {
                    let account = self.join_through_provider(identity.identity(), client, now)?;
                    (account, true, after_joining)
                }
                None => return Err(OidcSignInError::NotLinked),
            },
            Purpose::Invitation { secret, next } => {
                let account = self.redeem_with_provider(&secret, identity.identity(), now)?;
                (account, true, next)
            }
        };
        let signed_in = self.start(&account, now).map_err(OidcSignInError::Auth)?;
        let landing = Landing::new(
            next.as_deref(),
            vec![signed_in.session.set_cookie(now), clear_state_cookie()],
        );
        Ok(OidcSignedIn {
            signed_in,
            account_created: created,
            landing,
        })
    }

    /// An account with no password, named by a placeholder the person changes on their account page: the
    /// provider's claims are not read (not even a name), so nothing about the person is copied from it.
    fn external_account(&self, now: i64) -> Result<Account, OidcSignInError> {
        let id = AccountId::new();
        let mut tail: Vec<char> = id
            .as_str()
            .chars()
            .rev()
            .filter(char::is_ascii_hexdigit)
            .take(6)
            .collect();
        tail.reverse();
        let tail: String = tail.into_iter().collect();
        let display_name = DisplayName::new(&format!("member-{tail}"))
            .map_err(|_| OidcSignInError::Auth(AuthError::BadCredentials))?;
        Ok(Account {
            id,
            display_name,
            local: None,
            created_at: now,
            disabled: false,
        })
    }

    /// A first sign-in where joining through the provider is open: the account and its identity, in one step.
    fn join_through_provider(
        &self,
        identity: &ExternalIdentity,
        client: &ClientAddress,
        now: i64,
    ) -> Result<Account, OidcSignInError> {
        match self.by_join.begin(client.as_key(), now) {
            Begin::Locked(until) => {
                return Err(OidcSignInError::Auth(AuthError::Locked { until }));
            }
            Begin::Admitted { locks_now } => {
                if let Some(until) = locks_now {
                    self.note_lock("join-throttle", until, now);
                }
            }
        }
        let account = self.external_account(now)?;
        match self
            .accounts
            .add_account_with_identity(&account, identity, now)
        {
            Ok(()) => Ok(account),
            Err(StoreError::IdentityTaken) => {
                // Two first sign-ins of one person at once: the other one made it, and this one signs in.
                self.by_join.refund(client.as_key());
                self.accounts
                    .account_by_identity(identity)
                    .map_err(|e| OidcSignInError::Auth(e.into()))?
                    .filter(|a| !a.disabled)
                    .ok_or(OidcSignInError::Auth(AuthError::BadCredentials))
            }
            Err(e) => {
                self.by_join.refund(client.as_key());
                Err(OidcSignInError::Auth(e.into()))
            }
        }
    }

    /// An invitation redeemed with the provider's identity instead of a password.
    fn redeem_with_provider(
        &self,
        presented: &str,
        identity: &ExternalIdentity,
        now: i64,
    ) -> Result<Account, OidcSignInError> {
        let hash = Invitations::hash_of(presented).ok_or(OidcSignInError::InvitationUnusable)?;
        if !self
            .invitations
            .usable(presented, now)
            .map_err(OidcSignInError::Auth)?
        {
            return Err(OidcSignInError::InvitationUnusable);
        }
        // One person, one account: an identity that has one is told to sign in, and the link is not spent.
        if self
            .accounts
            .account_by_identity(identity)
            .map_err(|e| OidcSignInError::Auth(e.into()))?
            .is_some()
        {
            return Err(OidcSignInError::IdentityTaken);
        }
        let account = self.external_account(now)?;
        self.redemption
            .redeem_invitation_with_identity(&hash, &account, identity, now)
            .map_err(|e| match e {
                StoreError::InvitationUnusable => OidcSignInError::InvitationUnusable,
                StoreError::IdentityTaken => OidcSignInError::IdentityTaken,
                other => OidcSignInError::Auth(other.into()),
            })?;
        Ok(account)
    }

    /// The principal for a request's session cookie, if the session is live and its account is enabled. For
    /// a request that only reads; a request that changes anything uses [`Authenticator::principal_for_write`].
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
            principal: Principal::session(&self.authority, account.id.clone()),
            account: account.id,
            csrf: session.csrf,
            expires_at: session.expires_at,
        }))
    }

    /// The principal for a request that **changes something**: the session cookie and the CSRF token the
    /// request presented, checked together, so a handler cannot forget the second.
    pub fn principal_for_write(
        &self,
        secret: &str,
        presented_csrf: &str,
        now: i64,
    ) -> Result<SessionCheck, AuthError> {
        Ok(match self.principal_for_session(secret, now)? {
            None => SessionCheck::SignedOut,
            Some(s) if s.csrf.matches(presented_csrf) => SessionCheck::Valid(s),
            Some(_) => SessionCheck::CsrfMismatch,
        })
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
        Ok(Principal::token(
            &self.authority,
            verified.account,
            verified.scope,
        ))
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
        login_name: &str,
        display_name: &str,
        password: &str,
        now: i64,
    ) -> Result<Account, AccountError> {
        let account = self.new_local_account(login_name, display_name, password, now)?;
        self.accounts.add_account(&account)?;
        Ok(account)
    }

    /// An account with a name and a password, checked and hashed and not yet stored.
    fn new_local_account(
        &self,
        login_name: &str,
        display_name: &str,
        password: &str,
        now: i64,
    ) -> Result<Account, AccountError> {
        let login = LoginName::parse(login_name).map_err(AccountError::Login)?;
        let display_name = DisplayName::new(display_name).map_err(AccountError::DisplayName)?;
        let password_hash = self.hash_new_password(password)?;
        Ok(Account {
            id: AccountId::new(),
            display_name,
            local: Some(LocalCredential {
                login,
                password_hash,
            }),
            created_at: now,
            disabled: false,
        })
    }

    // ---- how accounts come into being (the sign-in handoff's U2) ----

    /// Who may join without an invitation (`INKLKUNN_JOIN`).
    pub fn join_policy(&self) -> JoinPolicy {
        self.join
    }

    /// Make an invitation (the operator's act; the caller has recorded it in the audit trail first). `grant` is
    /// the role in a wiki the new account starts with, if any. The secret is returned once.
    pub fn invite(
        &self,
        grant: Option<(WikiId, Role)>,
        lifetime_secs: i64,
        now: i64,
    ) -> Result<IssuedInvitation, InviteError> {
        self.invitations.issue(grant, lifetime_secs, now)
    }

    /// Can the person who opened this link still use it? A link that is malformed, unknown, used or over is one
    /// answer.
    pub fn invitation_usable(&self, presented: &str, now: i64) -> Result<bool, AuthError> {
        self.invitations.usable(presented, now)
    }

    /// The person who opened an invitation chooses a sign-in name and a password: the account is made, the
    /// invitation is used up and the role it named is granted in one step, and the person is signed in. The
    /// invitation is checked **before** anything is hashed (a stranger's bad link costs a lookup, not a
    /// verification), and a refusal (a taken name, a weak password) spends nothing: the person tries again with
    /// the same link.
    ///
    /// **Blocking** (it hashes).
    pub fn redeem_with_password(
        &self,
        presented: &str,
        login_name: &str,
        display_name: &str,
        password: &str,
        now: i64,
    ) -> Result<SignedIn, AccountError> {
        let hash = Invitations::hash_of(presented).ok_or(AccountError::InvitationUnusable)?;
        if !self.invitations.usable(presented, now)? {
            return Err(AccountError::InvitationUnusable);
        }
        let account = self.new_local_account(login_name, display_name, password, now)?;
        self.redemption.redeem_invitation(&hash, &account, now)?;
        Ok(self.start(&account, now)?)
    }

    /// Where joining is open to anyone (`INKLKUNN_JOIN=anyone`), a person makes an account with a sign-in name
    /// and a password, and is signed in. Never matched or linked to another account by e-mail address; no
    /// role; the new-account quotas apply. Counted per client address before any work is done.
    ///
    /// **Blocking** (it hashes).
    pub fn join_with_password(
        &self,
        login_name: &str,
        display_name: &str,
        password: &str,
        client: &ClientAddress,
        now: i64,
    ) -> Result<SignedIn, AccountError> {
        if !self.join.lets_anyone_join_with_a_password() {
            return Err(AccountError::JoinClosed);
        }
        match self.by_join.begin(client.as_key(), now) {
            Begin::Locked(until) => return Err(AuthError::Locked { until }.into()),
            Begin::Admitted { locks_now } => {
                if let Some(until) = locks_now {
                    self.note_lock("join-throttle", until, now);
                }
            }
        }
        let account = self.register_local(login_name, display_name, password, now)?;
        Ok(self.start(&account, now)?)
    }

    // ---- the account page ----

    /// Change the name readers see. The account's own act; the web layer checks the session and the CSRF token
    /// first.
    pub fn set_display_name(
        &self,
        account: &AccountId,
        display_name: &str,
    ) -> Result<(), AccountError> {
        let name = DisplayName::new(display_name).map_err(AccountError::DisplayName)?;
        Ok(self.accounts.set_display_name(account, &name)?)
    }

    /// The account's live sessions, newest first; `current_secret` is the session cookie of the asking request,
    /// which marks its own.
    pub fn sessions(
        &self,
        account: &AccountId,
        current_secret: Option<&str>,
        now: i64,
    ) -> Result<Vec<SessionSummary>, AuthError> {
        self.sessions.list(account, current_secret, now)
    }

    /// End one of the account's own sessions by its handle (*end this session*); `true` when it was theirs and
    /// live.
    pub fn end_session_of(
        &self,
        account: &AccountId,
        handle: &str,
        now: i64,
    ) -> Result<bool, AuthError> {
        self.sessions.end_one(account, handle, now)
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

    /// Change one's own password: the old one must authenticate (throttled and constant-work like a
    /// sign-in), the new one must be acceptable. Every session of the account ends and every API token is
    /// revoked, the one making the change included: they sign in again with the new password.
    ///
    /// **Blocking.**
    pub fn change_password(
        &self,
        login_name: &str,
        old_password: &str,
        new_password: &str,
        client: &ClientAddress,
        now: i64,
    ) -> Result<(), AccountError> {
        let account = self.verify_password(login_name, old_password, client, now)?;
        let Some(login) = account.local.as_ref().map(|l| l.login.clone()) else {
            return Err(AccountError::Auth(AuthError::BadCredentials));
        };
        let password_hash = self.hash_new_password(new_password)?;
        self.accounts.set_local_credential(
            &account.id,
            Some(&LocalCredential {
                login,
                password_hash,
            }),
        )?;
        self.end_credentials(&account.id, now)?;
        Ok(())
    }

    /// An administrator's reset: give an account a sign-in name and a new password **without proving the old
    /// one** (the caller has been authorized to do this and has re-authenticated). Every session ends and
    /// every API token is revoked. **Blocking.**
    pub fn reset_password(
        &self,
        account: &AccountId,
        login_name: &str,
        password: &str,
        now: i64,
    ) -> Result<(), AccountError> {
        let login = LoginName::parse(login_name).map_err(AccountError::Login)?;
        let password_hash = self.hash_new_password(password)?;
        self.accounts.set_local_credential(
            account,
            Some(&LocalCredential {
                login,
                password_hash,
            }),
        )?;
        self.end_credentials(account, now)?;
        Ok(())
    }

    /// Disable an account: it cannot sign in, its sessions end, and every API token is revoked (enabling it
    /// again does not bring them back). Its history stays attributed to it.
    pub fn disable(&self, account: &AccountId, now: i64) -> Result<(), AccountError> {
        self.accounts.set_disabled(account, true)?;
        self.end_credentials(account, now)?;
        Ok(())
    }

    /// Enable an account again. Its old sessions and tokens stay ended.
    pub fn enable(&self, account: &AccountId) -> Result<(), AccountError> {
        Ok(self.accounts.set_disabled(account, false)?)
    }

    fn end_credentials(&self, account: &AccountId, now: i64) -> Result<(), AuthError> {
        self.sessions.end_all(account)?;
        self.tokens.revoke_all(account, now)?;
        Ok(())
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

    /// The dummy hash a sign-in verifies against when it has nothing usable to check (tests compare its cost
    /// with a real hash's).
    #[cfg(any(test, feature = "test-knobs"))]
    pub fn dummy_hash_for_test(&self) -> &str {
        &self.dummy_hash
    }
}

impl std::fmt::Debug for Authenticator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Authenticator").finish_non_exhaustive()
    }
}
