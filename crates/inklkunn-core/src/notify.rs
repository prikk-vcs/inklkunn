//! Who is told what (RFC 004 D-9; requirement PR-11; handoff 004 T6): the events that involve a person —
//! their proposal reviewed, accepted, declined or needing a revision; a reply or a mention; a page they watch
//! changed — made when the act that causes them happens, and handed to the [`Channel`]s.
//!
//! **An event carries ids and a kind, never content** (D-9: "events never carry content the recipient could
//! not read"). Two walls stand behind that:
//!
//! 1. *Delivery.* Nobody is told about a proposal or a page they may not read now: the recipient is checked
//!    against the same rule every read goes through ([`Authorizer`]), and a person who cannot read the wiki
//!    gets no event at all.
//! 2. *Rendering.* The inbox ([`crate::inbox`]) reads what an event is about as the person who opens it, and
//!    shows an event about something they can no longer read as a bare fact, so a wiki that became
//!    restricted after the event was made is not read through its inbox entries.
//!
//! **Telling is best effort and never changes the act.** A proposal that was declined stays declined if the
//! inbox record of it could not be written; the failure is counted ([`Notifier::failures`]) so that an
//! operator sees it, and nothing is retried silently. **Acceptance-time events** — the proposal accepted
//! ([`Notifier::accepted`]) and the watched pages that changed ([`Notifier::page_changed`]) — are made by the
//! acceptance after the seal, and only then: a watch is told about a change that is in the history, never
//! about a proposal that is merely proposed.

mod channel;
mod mention;

use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use inklkunn_store::{
    AccountId, AccountStore, CommentRecord, InboxEvent, InboxEventId, InboxEventKind, InboxStore,
    ProposalId, ProposalRecord, ProposalStore, RoleStore, WatchRecord, WatchTarget, WikiRecord,
};

use crate::authorize::{Action, Authorizer, Resource};

pub use channel::{Channel, ChannelError, InApp};
pub use mention::{MAX_MENTIONS, mentions};

/// Makes the events and hands them to the channels.
pub struct Notifier {
    channels: Vec<Arc<dyn Channel>>,
    inbox: Arc<dyn InboxStore>,
    proposals: Arc<dyn ProposalStore>,
    roles: Arc<dyn RoleStore>,
    accounts: Arc<dyn AccountStore>,
    authorizer: Authorizer,
    clock: fn() -> i64,
    failures: AtomicU64,
}

impl std::fmt::Debug for Notifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Notifier")
    }
}

impl Notifier {
    /// A notifier whose one channel is the in-app inbox over `inbox`. `proposals` is read for who took part
    /// in a proposal, `roles` for who may accept, `accounts` for the accounts a mention names.
    pub fn new(
        inbox: Arc<dyn InboxStore>,
        proposals: Arc<dyn ProposalStore>,
        roles: Arc<dyn RoleStore>,
        accounts: Arc<dyn AccountStore>,
        authorizer: Authorizer,
    ) -> Self {
        let in_app: Arc<dyn Channel> = Arc::new(InApp::new(inbox.clone()));
        Notifier {
            channels: vec![in_app],
            inbox,
            proposals,
            roles,
            accounts,
            authorizer,
            clock: inklkunn_store::record::now_secs,
            failures: AtomicU64::new(0),
        }
    }

    /// The same, reading the time from `clock` (Unix seconds): tests pin it.
    #[must_use]
    pub fn with_clock(mut self, clock: fn() -> i64) -> Self {
        self.clock = clock;
        self
    }

    /// The same, with these channels **instead of** the in-app one (a test double, or a later channel).
    #[must_use]
    pub fn with_channels(mut self, channels: Vec<Arc<dyn Channel>>) -> Self {
        self.channels = channels;
        self
    }

    /// How many events could not be delivered since the process started (a channel refused, a record could
    /// not be read): what an operator watches. Never a reason to fail the act that caused the event.
    pub fn failures(&self) -> u64 {
        self.failures.load(Ordering::Relaxed)
    }

    fn failed(&self) {
        self.failures.fetch_add(1, Ordering::Relaxed);
    }

    // --- one event

    fn proposal_resource(wiki: &WikiRecord, id: &ProposalId) -> Resource {
        Resource::Proposal {
            wiki: wiki.id.clone(),
            id: id.to_string(),
        }
    }

    /// Tell `to` that `kind` happened to `proposal`, if `to` is not the one who did it and may read the
    /// proposal now. `true` when a channel took the event.
    fn tell(
        &self,
        wiki: &WikiRecord,
        to: &AccountId,
        kind: InboxEventKind,
        proposal: &ProposalId,
        actor: &AccountId,
    ) -> bool {
        if to == actor
            || !self.authorizer.account_may(
                to,
                Action::Read,
                &Self::proposal_resource(wiki, proposal),
            )
        {
            return false;
        }
        let event = InboxEvent {
            id: InboxEventId::new(),
            account: to.clone(),
            wiki: wiki.id.clone(),
            proposal: Some(proposal.clone()),
            kind,
            actor: Some(actor.clone()),
            created_at: (self.clock)(),
            read_at: None,
        };
        let mut taken = false;
        for channel in &self.channels {
            match channel.deliver(&event) {
                Ok(()) => taken = true,
                Err(_) => self.failed(),
            }
        }
        taken
    }

    /// Tell each of `to`, once; how many were told.
    fn tell_all(
        &self,
        wiki: &WikiRecord,
        to: &BTreeSet<AccountId>,
        kind: InboxEventKind,
        proposal: &ProposalId,
        actor: &AccountId,
    ) -> usize {
        to.iter()
            .filter(|who| self.tell(wiki, who, kind, proposal, actor))
            .count()
    }

    // --- the acts of the proposal service

    /// A reviewer decided on the proposal: its proposer is told. A draft is nobody's but its author's.
    pub fn reviewed(&self, wiki: &WikiRecord, proposal: &ProposalRecord, reviewer: &AccountId) {
        if proposal.submitted_at.is_some() {
            self.tell(
                wiki,
                &proposal.author,
                InboxEventKind::Reviewed,
                &proposal.id,
                reviewer,
            );
        }
    }

    /// An acceptor sent the proposal back for a revision: its proposer is told.
    pub fn needs_revision(&self, wiki: &WikiRecord, proposal: &ProposalRecord, by: &AccountId) {
        self.tell(
            wiki,
            &proposal.author,
            InboxEventKind::NeedsRevision,
            &proposal.id,
            by,
        );
    }

    /// An acceptor declined the proposal: its proposer is told.
    pub fn declined(&self, wiki: &WikiRecord, proposal: &ProposalRecord, by: &AccountId) {
        self.tell(
            wiki,
            &proposal.author,
            InboxEventKind::Declined,
            &proposal.id,
            by,
        );
    }

    /// The proposal was superseded by a later one: its proposer is told (unless they did it).
    pub fn superseded(&self, wiki: &WikiRecord, proposal: &ProposalRecord, by: &AccountId) {
        self.tell(
            wiki,
            &proposal.author,
            InboxEventKind::Superseded,
            &proposal.id,
            by,
        );
    }

    /// Everyone who took part in the proposal (reviewers and commenters) and everyone who may accept in the
    /// wiki: the people who held it.
    fn holders(&self, wiki: &WikiRecord, proposal: &ProposalRecord) -> BTreeSet<AccountId> {
        let mut who = BTreeSet::new();
        match self.proposals.decisions(&proposal.id, None) {
            Ok(decisions) => who.extend(decisions.into_iter().map(|d| d.reviewer)),
            Err(_) => self.failed(),
        }
        match self.proposals.comments(&proposal.id, None) {
            Ok(comments) => who.extend(comments.into_iter().map(|c| c.author)),
            Err(_) => self.failed(),
        }
        match self.roles.grants_in(&wiki.id) {
            Ok(grants) => who.extend(
                grants
                    .into_iter()
                    .filter(|g| g.role >= Action::Accept.minimum_role())
                    .map(|g| g.account),
            ),
            Err(_) => self.failed(),
        }
        who
    }

    /// The proposer took the proposal back: the people who held it are told. A proposal that was never sent
    /// for review was held by nobody.
    pub fn withdrawn(&self, wiki: &WikiRecord, proposal: &ProposalRecord, by: &AccountId) {
        if proposal.submitted_at.is_none() {
            return;
        }
        let holders = self.holders(wiki, proposal);
        self.tell_all(wiki, &holders, InboxEventKind::Withdrawn, &proposal.id, by);
    }

    /// A comment was added: the proposer is told of a comment on their proposal, the author of the comment
    /// it replies to is told of the reply, and each account the text mentions that may read the proposal is
    /// told of the mention. Each person is told once, by the first of those that applies to them.
    pub fn commented(&self, wiki: &WikiRecord, proposal: &ProposalRecord, comment: &CommentRecord) {
        let mut told: BTreeSet<AccountId> = BTreeSet::new();
        told.insert(comment.author.clone());
        let mut tell_once = |to: &AccountId, kind| {
            if told.insert(to.clone()) {
                self.tell(wiki, to, kind, &proposal.id, &comment.author);
            }
        };
        if let Some(parent) = &comment.parent {
            match self.proposals.comments(&proposal.id, None) {
                Ok(all) => {
                    if let Some(replied_to) = all.iter().find(|c| c.id == *parent) {
                        tell_once(&replied_to.author, InboxEventKind::Replied);
                    }
                }
                Err(_) => self.failed(),
            }
        }
        tell_once(&proposal.author, InboxEventKind::Replied);
        for login in mentions(&comment.body) {
            // A name that is nobody's, or that belongs to someone with no business here, is told nothing
            // and the writer is told nothing either way.
            match self.accounts.account_by_login(&login) {
                Ok(Some(account)) => tell_once(&account.id, InboxEventKind::Mentioned),
                Ok(None) => {}
                Err(_) => self.failed(),
            }
        }
    }

    // --- the acts of the acceptance (handoff 004 T4 calls these after the seal, and only then)

    /// The proposal was accepted and sealed into the history: its proposer is told.
    pub fn accepted(&self, wiki: &WikiRecord, proposal: &ProposalRecord, acceptor: &AccountId) {
        self.tell(
            wiki,
            &proposal.author,
            InboxEventKind::Accepted,
            &proposal.id,
            acceptor,
        );
    }

    /// The pages at `paths` changed in the history, by the accepted `proposal`: each account that watches
    /// one of them — the page itself, or a namespace it is in — and may read the page is told, once however
    /// many of the pages it watches, unless it is the one who accepted. **Call it after the seal, never
    /// before**: a watch is told what is in the history. How many were told is returned.
    pub fn page_changed(
        &self,
        wiki: &WikiRecord,
        proposal: &ProposalId,
        paths: &[String],
        acceptor: &AccountId,
    ) -> usize {
        let watches = match self.inbox.watches_in(&wiki.id) {
            Ok(watches) => watches,
            Err(_) => {
                self.failed();
                return 0;
            }
        };
        let mut told: BTreeSet<AccountId> = BTreeSet::new();
        for watch in &watches {
            if told.contains(&watch.account) {
                continue;
            }
            // The first page of the change this watch follows, and whether its owner may read that page.
            let readable = paths.iter().any(|path| {
                watch_covers(watch, path)
                    && self.authorizer.account_may(
                        &watch.account,
                        Action::Read,
                        &Resource::Page {
                            wiki: wiki.id.clone(),
                            path: path.clone(),
                        },
                    )
            });
            if readable
                && self.tell(
                    wiki,
                    &watch.account,
                    InboxEventKind::WatchedPageChanged,
                    proposal,
                    acceptor,
                )
            {
                told.insert(watch.account.clone());
            }
        }
        told.len()
    }
}

/// Does this watch follow the page at `path`?
pub fn watch_covers(watch: &WatchRecord, path: &str) -> bool {
    match &watch.target {
        WatchTarget::Page(page) => page == path,
        WatchTarget::Namespace(prefix) => path.starts_with(prefix.as_str()),
    }
}

#[cfg(test)]
mod tests;
