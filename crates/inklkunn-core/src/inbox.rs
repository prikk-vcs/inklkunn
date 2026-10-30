//! The inbox and watching (RFC 004 D-9; requirements PR-11, RD-8; handoff 004 T6): what a person is told, as
//! the person who opens it, and what they follow.
//!
//! **Rendering re-authorizes.** An event is ids and a kind ([`crate::notify`]); this module is where it
//! becomes words, and it is read *as the principal who asks*, at the moment they ask. What the event is about
//! — the wiki's name, the proposal's summary, the person who acted — is read only if that principal may read
//! the proposal (or the wiki) now. An event about something they can no longer read (the wiki became
//! restricted, their role was taken away, the wiki is gone) is shown as the bare fact that something happened
//! somewhere they cannot see, naming nothing. A reader of the inbox never learns, from an old event, what
//! they were later stopped from reading.
//!
//! **An account is needed.** An inbox belongs to an account and so does a watch: an anonymous visitor and the
//! system have neither ([`InboxError::NeedsAccount`]).
//!
//! **Watching** a page or a namespace follows it from the moment you ask, and needs the right to read what you
//! follow. A watch is a page's path or a namespace's path prefix (`pages/Guides/`), the forms the history
//! reports changes in, so that matching a change is a comparison of strings. The number of watches an account
//! may keep is bounded ([`MAX_WATCHES`]).

use std::sync::Arc;

use inklkunn_markup::path::{PAGES_DIR, PathError, title_to_path};
use inklkunn_store::{
    AccountId, AccountStore, InboxEvent, InboxEventId, InboxEventKind, InboxStore, ProposalId,
    ProposalState, ProposalStore, StoreError, WatchRecord, WatchTarget, WikiRecord, WikiStore,
};

use crate::authorize::{Action, Authorizer, Principal, Resource};

/// The most events one look at the inbox returns (the newest).
pub const INBOX_LIMIT: usize = 100;
/// The most watches one account keeps.
pub const MAX_WATCHES: usize = 500;

/// Why a call was refused. Each is a plain fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboxError {
    /// Only an account has an inbox and watches.
    NeedsAccount,
    /// The principal may not read what it asked to follow (or there is nothing there to follow): the same
    /// answer for a page that is not there and a wiki that cannot be read.
    NotAllowed,
    /// The title cannot name a page or a namespace.
    BadTitle(PathError),
    /// The account already follows [`MAX_WATCHES`] things.
    TooManyWatches,
    /// The store failed (the text is for the operator).
    Store(StoreError),
}

impl std::fmt::Display for InboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InboxError::NeedsAccount => write!(f, "an account is needed"),
            InboxError::NotAllowed => write!(f, "not allowed"),
            InboxError::BadTitle(e) => write!(f, "the title cannot be followed: {e}"),
            InboxError::TooManyWatches => write!(f, "the account follows as many as it may"),
            InboxError::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for InboxError {}

impl From<StoreError> for InboxError {
    fn from(e: StoreError) -> Self {
        InboxError::Store(e)
    }
}

type Result<T> = std::result::Result<T, InboxError>;

fn account_of(principal: &Principal) -> Result<&AccountId> {
    match principal {
        Principal::Account(a) => Ok(a.id()),
        Principal::Anonymous | Principal::System(_) => Err(InboxError::NeedsAccount),
    }
}

/// The proposal an event is about, as the reader may see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposalLine {
    /// The proposal.
    pub id: ProposalId,
    /// Its summary line. Untrusted text.
    pub summary: String,
    /// Where it is now.
    pub state: ProposalState,
}

/// What an event is about, as far as the person looking may be told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum About {
    /// They may read it: the wiki, the proposal, the person who acted.
    Readable {
        /// The wiki's address.
        slug: String,
        /// The wiki's name. Untrusted text.
        wiki: String,
        /// The proposal, when the event is about one.
        proposal: Option<ProposalLine>,
        /// The display name of the person who did it, when a person did. Untrusted text.
        actor: Option<String>,
    },
    /// They may not read it now (or it is gone): nothing is named.
    Unreadable,
}

/// One line of the inbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The event.
    pub id: InboxEventId,
    /// What happened.
    pub kind: InboxEventKind,
    /// When (Unix seconds).
    pub at: i64,
    /// Not yet read.
    pub unread: bool,
    /// What it is about.
    pub about: About,
}

/// One thing a person follows, as they may be told of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Followed {
    /// The wiki's address.
    pub slug: String,
    /// The wiki's name. Untrusted text.
    pub wiki: String,
    /// What is followed.
    pub target: WatchTarget,
}

/// The inbox and the watches of accounts.
pub struct Inbox {
    events: Arc<dyn InboxStore>,
    proposals: Arc<dyn ProposalStore>,
    wikis: Arc<dyn WikiStore>,
    accounts: Arc<dyn AccountStore>,
    authorizer: Authorizer,
    clock: fn() -> i64,
}

impl std::fmt::Debug for Inbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Inbox")
    }
}

/// The watch for a page, by its title: the page's path.
pub fn page_target(title: &str) -> std::result::Result<WatchTarget, PathError> {
    title_to_path(title).map(WatchTarget::Page)
}

/// The watch for the pages under a title (`Guides` follows `Guides/Setup`): the path prefix they share.
pub fn namespace_target(title: &str) -> std::result::Result<WatchTarget, PathError> {
    // A page below the title has the title's path without its extension as a directory.
    let path = title_to_path(&format!("{}/x", title.trim().trim_end_matches('/')))?;
    let prefix = path
        .strip_suffix("x.md")
        .filter(|p| p.starts_with(PAGES_DIR))
        .map(str::to_owned)
        .ok_or(PathError::Empty)?;
    Ok(WatchTarget::Namespace(prefix))
}

impl Inbox {
    /// The inbox over these records.
    pub fn new(
        events: Arc<dyn InboxStore>,
        proposals: Arc<dyn ProposalStore>,
        wikis: Arc<dyn WikiStore>,
        accounts: Arc<dyn AccountStore>,
        authorizer: Authorizer,
    ) -> Self {
        Inbox {
            events,
            proposals,
            wikis,
            accounts,
            authorizer,
            clock: inklkunn_store::record::now_secs,
        }
    }

    /// The same, reading the time from `clock` (Unix seconds): tests pin it.
    #[must_use]
    pub fn with_clock(mut self, clock: fn() -> i64) -> Self {
        self.clock = clock;
        self
    }

    fn may_read(&self, principal: &Principal, resource: &Resource) -> bool {
        self.authorizer
            .authorize(principal, Action::Read, resource)
            .is_allowed()
    }

    /// What `event` is about, for `principal`: read now, through the same rule as every other read.
    fn about(&self, principal: &Principal, event: &InboxEvent) -> About {
        let viewer = account_of(principal).ok();
        let resource = match &event.proposal {
            Some(id) => Resource::Proposal {
                wiki: event.wiki.clone(),
                id: id.to_string(),
            },
            None => Resource::Wiki {
                wiki: event.wiki.clone(),
            },
        };
        if !self.may_read(principal, &resource) {
            return About::Unreadable;
        }
        let Ok(wiki) = self.wikis.get(&event.wiki) else {
            return About::Unreadable;
        };
        let proposal = match &event.proposal {
            Some(id) => match self.proposals.proposal(id, viewer) {
                Ok(Some(p)) if p.wiki == event.wiki => Some(ProposalLine {
                    id: p.id,
                    summary: p.summary,
                    state: p.state,
                }),
                // A proposal that is not there for this reader is not named.
                _ => return About::Unreadable,
            },
            None => None,
        };
        let actor = event
            .actor
            .as_ref()
            .and_then(|a| self.accounts.account(a).ok().flatten())
            .map(|a| a.display_name.as_str().to_owned());
        About::Readable {
            slug: wiki.slug.as_str().to_owned(),
            wiki: wiki.display_name.as_str().to_owned(),
            proposal,
            actor,
        }
    }

    /// The account's newest events (at most [`INBOX_LIMIT`]), newest first, each shown as far as the account
    /// may be told of it now.
    pub fn entries(&self, principal: &Principal, unread_only: bool) -> Result<Vec<Entry>> {
        let account = account_of(principal)?;
        Ok(self
            .events
            .inbox(account, unread_only, INBOX_LIMIT)?
            .into_iter()
            .map(|event| Entry {
                about: self.about(principal, &event),
                id: event.id,
                kind: event.kind,
                at: event.created_at,
                unread: event.read_at.is_none(),
            })
            .collect())
    }

    /// How many events of the newest [`INBOX_LIMIT`] are unread: the figure a page header shows.
    pub fn unread(&self, principal: &Principal) -> Result<usize> {
        let account = account_of(principal)?;
        Ok(self.events.inbox(account, true, INBOX_LIMIT)?.len())
    }

    /// Mark one event read. `false` when the account has no such event (an event of someone else's inbox is
    /// not found, whoever asks).
    pub fn mark_read(&self, principal: &Principal, event: &InboxEventId) -> Result<bool> {
        let account = account_of(principal)?;
        Ok(self.events.mark_read(account, event, (self.clock)())?)
    }

    /// Mark everything unread (of the newest [`INBOX_LIMIT`]) read; how many were.
    pub fn mark_all_read(&self, principal: &Principal) -> Result<usize> {
        let account = account_of(principal)?;
        let now = (self.clock)();
        let mut marked = 0;
        for event in self.events.inbox(account, true, INBOX_LIMIT)? {
            if self.events.mark_read(account, &event.id, now)? {
                marked += 1;
            }
        }
        Ok(marked)
    }

    // --- watching

    fn resource_of(wiki: &WikiRecord, target: &WatchTarget) -> Resource {
        match target {
            WatchTarget::Page(path) => Resource::Page {
                wiki: wiki.id.clone(),
                path: path.clone(),
            },
            WatchTarget::Namespace(prefix) => Resource::Namespace {
                wiki: wiki.id.clone(),
                prefix: prefix.clone(),
            },
        }
    }

    /// Follow a page or a namespace of `wiki`. `true` when it is new, `false` when it was followed already.
    /// Needs the right to read it (the same answer, *not allowed*, for a wiki that cannot be read).
    pub fn watch(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        target: WatchTarget,
    ) -> Result<bool> {
        let account = account_of(principal)?;
        if !self.may_read(principal, &Self::resource_of(wiki, &target)) {
            return Err(InboxError::NotAllowed);
        }
        if self.events.watches_of(account)?.len() >= MAX_WATCHES {
            // Following the same thing again is no change, however many are followed.
            let already = self
                .events
                .watches_of(account)?
                .iter()
                .any(|w| w.wiki == wiki.id && w.target == target);
            if !already {
                return Err(InboxError::TooManyWatches);
            }
        }
        Ok(self.events.watch(&WatchRecord {
            account: account.clone(),
            wiki: wiki.id.clone(),
            target,
            created_at: (self.clock)(),
        })?)
    }

    /// Stop following. Needs only the account: a person who can no longer read a page may still stop being
    /// told about it. `true` when it was followed.
    pub fn unwatch(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        target: &WatchTarget,
    ) -> Result<bool> {
        let account = account_of(principal)?;
        Ok(self.events.unwatch(account, &wiki.id, target)?)
    }

    /// Whether the account follows this exactly.
    pub fn is_watching(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        target: &WatchTarget,
    ) -> Result<bool> {
        let account = account_of(principal)?;
        Ok(self
            .events
            .watches_of(account)?
            .iter()
            .any(|w| w.wiki == wiki.id && w.target == *target))
    }

    /// What the account follows, oldest first, as far as it may read the wikis: a watch in a wiki the
    /// account can no longer read is not listed (and no event of it is made).
    pub fn followed(&self, principal: &Principal) -> Result<Vec<Followed>> {
        let account = account_of(principal)?;
        let mut out = Vec::new();
        for watch in self.events.watches_of(account)? {
            let Ok(wiki) = self.wikis.get(&watch.wiki) else {
                continue;
            };
            if !self.may_read(principal, &Self::resource_of(&wiki, &watch.target)) {
                continue;
            }
            out.push(Followed {
                slug: wiki.slug.as_str().to_owned(),
                wiki: wiki.display_name.as_str().to_owned(),
                target: watch.target,
            });
        }
        Ok(out)
    }
}
