//! Creating an alternative view (RFC 005 part 2 D-9; RFC 002 D-4's second signer operation; internal design SGN-4,
//! WR-8; handoff 005 K-T8): **the same path as an acceptance, with its own record.**
//!
//! [`ProposalAcceptor::create_view`] does, in order:
//!
//! 0. `authorize(_, create-view, view)` **first**, before the lease, and the name is checked (a new view's name is
//!    [`views::new_name`], never changed or tidied).
//! 1. The wiki's write lease ([`Purpose::View`]), then the guard against an unfinished earlier act: a wiki that
//!    *needs recovery* refuses it.
//! 2. The wiki's views are read **now**, from prikk, never from a held answer. A record of an earlier creation that
//!    never ended is **settled** from what prikk says (the view is there: created; it is not: it failed), so that
//!    a process that died between the record and its end blocks nothing. The view to fork from must be open, the
//!    name free (compared without regard to case), and the wiki under [`views::MAX_VIEWS`].
//! 3. **The act is recorded** (name, the view it forks from, that view's tip, the kind, who) before the signer is
//!    asked. The signer reads it back through [`StoreLedger`](super::StoreLedger) and refuses unless it names this
//!    wiki, view and ref: a web handler cannot hand it a fact of its own (INV-4).
//! 4. `Accept::create_view`, inside the lease. An unfinished key retirement pauses it as it pauses a seal (W1: a
//!    new view is a new ref in a history the retirement recorded the views of).
//! 5. The answer is settled from prikk, not guessed: whatever the signer said, **if the view is there the act
//!    created it**, and if not, the act failed with a fixed word. The end is recorded once, the wiki's caches are
//!    forgotten, the lease is released.
//!
//! A view creation seals nothing and writes no block: nothing is left for recovery to complete, which is why a
//! holder that dies in the middle of one does not flag the wiki ([`Purpose::View`]).

use inklkunn_signer::accept::{AcceptError, ViewCreationAct};
use inklkunn_store::{
    StoreError, ViewActEnd, ViewActId, ViewActOutcome, ViewActRecord, ViewKind, WikiRecord,
};

use super::shared::{self, Blocked, TipError};
use super::{ProposalAcceptor, words};
use crate::authorize::{Action, Principal, Resource};
use crate::lease::{Busy, LeaseError, Purpose};
use crate::views::{self, MAX_VIEWS, NameFault};

/// A view that was created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewCreated {
    /// The record of the act.
    pub act: ViewActId,
    /// The new view's short name.
    pub name: String,
    /// The short name of the view it was forked from.
    pub forked_from: String,
    /// A temporary directory of decrypted seeds could not be removed (the view **is** created).
    pub seeds_left: bool,
}

/// Why a view was not created. Each variant says what state things are in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewFailure {
    /// *Not allowed*. Nothing was done.
    NotAllowed,
    /// This server's records do not keep a view's creation (its store cannot hold the act the signer is bound
    /// to), so it does not take the first step. Nothing was done.
    NotKept,
    /// The name is not one a new view may have. Nothing was done.
    InvalidName(NameFault),
    /// The view to fork from is not one of the wiki's open views. Nothing was done.
    NoSuchSource,
    /// A view of that name is there (names are compared without regard to case). Nothing was done.
    NameTaken,
    /// The wiki already has as many open views as it may. Nothing was done.
    TooMany {
        /// The bound.
        bound: usize,
    },
    /// *The wiki is busy*, with how many acts are ahead and about how long. Nothing was done.
    Busy(Busy),
    /// *The wiki needs recovery*: it refuses writes until recovery has looked. Nothing was done by this call.
    NeedsRecovery(inklkunn_store::RecoveryFlag),
    /// A change to the wiki's keys was begun and is not finished: the signer makes nothing new until the person who
    /// runs the wiki finishes it. Nothing was created; the act is recorded as failed.
    RetirementUnfinished(String),
    /// Failed: prikk or the signer refused, and the view is not there. The act is recorded as failed with `word`.
    Failed {
        /// The act.
        act: ViewActId,
        /// The fixed word ([`words`]).
        word: &'static str,
        /// What happened, for operators.
        detail: String,
    },
    /// The repository, the lease or the records could not be had before anything was created. Nothing was done.
    Unavailable(String),
    /// The signer was asked and the wiki's views could not be read afterwards, so it is not known whether the view
    /// is there. The act stays open; the next creation settles it from the repository.
    Uncertain(String),
    /// A store failed (the text is for operators).
    Store(StoreError),
}

impl std::fmt::Display for ViewFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewFailure::NotAllowed => write!(f, "not allowed"),
            ViewFailure::NotKept => write!(f, "this server keeps no record of a view's creation"),
            ViewFailure::InvalidName(why) => write!(f, "the name is not a view's: {why:?}"),
            ViewFailure::NoSuchSource => write!(f, "the view to fork from is not an open view"),
            ViewFailure::NameTaken => write!(f, "a view of that name is there"),
            ViewFailure::TooMany { bound } => {
                write!(f, "the wiki already has {bound} open views")
            }
            ViewFailure::Busy(b) => write!(
                f,
                "the wiki is busy ({} ahead, about {} s)",
                b.depth,
                b.estimate.as_secs()
            ),
            ViewFailure::NeedsRecovery(flag) => {
                write!(f, "the wiki needs recovery ({})", flag.reason.as_str())
            }
            ViewFailure::RetirementUnfinished(m) => write!(f, "refused, nothing created: {m}"),
            ViewFailure::Failed { word, detail, .. } => {
                write!(f, "failed ({word}); nothing was created: {detail}")
            }
            ViewFailure::Unavailable(why) => write!(f, "could not be done now: {why}"),
            ViewFailure::Uncertain(why) => {
                write!(f, "the view may or may not have been created: {why}")
            }
            ViewFailure::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ViewFailure {}

impl From<StoreError> for ViewFailure {
    fn from(e: StoreError) -> Self {
        ViewFailure::Store(e)
    }
}

impl ProposalAcceptor {
    /// Create the alternative view `name`, forked from the view `from` (the wiki's default view when `None`; short
    /// names, `draft` for `heads/draft`), of this `kind`, as `principal`. Blocks the calling thread (the lease
    /// waits its turn; prikk and the signer are asked); an async caller runs it on a blocking thread.
    pub fn create_view(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        name: &str,
        from: Option<&str>,
        kind: ViewKind,
    ) -> Result<ViewCreated, ViewFailure> {
        let p = &self.parts;
        // 0. Authorization, first: before the lease, before anything is read.
        let resource = Resource::View {
            wiki: wiki.id.clone(),
            name: format!("heads/{name}"),
        };
        let creator = match principal.account_id() {
            Some(account)
                if p.proposals
                    .authorizes(principal, Action::CreateView, &resource) =>
            {
                account.clone()
            }
            _ => return Err(ViewFailure::NotAllowed),
        };
        if !p.acts.keeps_view_acts() {
            return Err(ViewFailure::NotKept);
        }
        let name = views::new_name(name).map_err(ViewFailure::InvalidName)?;
        let from = from.map_or_else(|| views::short_name(&wiki.default_view), |f| f);

        // 1. The lease, and the guard against an unfinished earlier act.
        let lease = p
            .leases
            .acquire(&wiki.id, Purpose::View, None)
            .map_err(|e| match e {
                LeaseError::Busy(busy) => ViewFailure::Busy(busy),
                LeaseError::Unavailable(why) => ViewFailure::Unavailable(why),
            })?;
        shared::recovery_guard(&*p.acts, &*p.acceptances, wiki, &lease, (self.clock)()).map_err(
            |b| match b {
                Blocked::Flagged(flag) => ViewFailure::NeedsRecovery(flag),
                Blocked::Unavailable(why) => ViewFailure::Unavailable(why),
                Blocked::Store(e) => ViewFailure::Store(e),
            },
        )?;

        // 2. The views as prikk says they are now; earlier acts settled from them; then the checks.
        let repo = p.layout.repo_dir(&wiki.id);
        let open = open_views(self, wiki)?;
        self.settle_earlier_acts(wiki, &open)?;
        let source = format!("heads/{from}");
        let wanted = format!("heads/{name}");
        if open.iter().any(|v| v.eq_ignore_ascii_case(&wanted)) {
            return Err(ViewFailure::NameTaken);
        }
        if open.len() >= MAX_VIEWS {
            return Err(ViewFailure::TooMany { bound: MAX_VIEWS });
        }
        let from_block = shared::tip_block(&*p.storage, &repo, &source).map_err(|e| match e {
            TipError::NoSuchView => ViewFailure::NoSuchSource,
            TipError::Unavailable(why) => ViewFailure::Unavailable(why),
        })?;

        // 3. The record of the act, before the signer is asked.
        let act = ViewActRecord {
            id: ViewActId::new(),
            wiki: wiki.id.clone(),
            name: name.to_owned(),
            forked_from: from.to_owned(),
            from_block,
            kind,
            created_by: creator,
            created_at: (self.clock)(),
        };
        // A name held by an act that stands but is not among the open views belongs to a view that was closed: it is
        // taken, and stays so (prikk does not make a closed branch's name again either).
        p.acts.begin_view_act(&act).map_err(|e| match e {
            StoreError::AlreadyRecorded => ViewFailure::NameTaken,
            other => ViewFailure::Store(other),
        })?;

        // 4. The signer, on this thread, inside the lease.
        let answer = {
            let _scope = lease.scope();
            p.signer.create_view(
                &wiki.id,
                &wanted,
                &source,
                &ViewCreationAct::new(act.id.as_str()),
            )
        };

        // 5. Settled from the repository, not from the answer. If prikk cannot be asked now the act stays open and
        //    the caller is told it could not be done now; the next creation settles it from prikk.
        let seeds_left = matches!(answer, Err(AcceptError::SeedsNotRemoved { .. }));
        let word = failed_word(&answer);
        let created = match open_views(self, wiki) {
            Ok(now) => now.contains(&wanted),
            Err(ViewFailure::Unavailable(why)) => return Err(ViewFailure::Uncertain(why)),
            Err(other) => return Err(other),
        };
        // A lost end is not lost: the act is open, the view is there or not, and the next creation settles it.
        let _ = p.acts.finish_view_act(&ViewActEnd {
            act: act.id.clone(),
            outcome: if created {
                ViewActOutcome::Created
            } else {
                ViewActOutcome::Failed {
                    word: word.to_owned(),
                }
            },
            at: (self.clock)(),
        });
        p.reads.forget_wiki(wiki);
        if created {
            return Ok(ViewCreated {
                act: act.id,
                name: name.to_owned(),
                forked_from: from.to_owned(),
                seeds_left,
            });
        }
        Err(match answer {
            Err(AcceptError::RetirementUnfinished(sentence)) => {
                ViewFailure::RetirementUnfinished(sentence)
            }
            Err(AcceptError::Busy { .. }) => {
                ViewFailure::Unavailable("another process holds the wiki".to_owned())
            }
            Err(e) => ViewFailure::Failed {
                act: act.id,
                word,
                detail: e.to_string(),
            },
            // The signer said all was well and the view is not there: said as it is.
            Ok(()) => ViewFailure::Failed {
                act: act.id,
                word,
                detail: "the signer reported the view created and it is not among the views"
                    .to_owned(),
            },
        })
    }

    /// An earlier creation that never ended is settled from what prikk says now: the view is there, so it was
    /// created; it is not, so it failed. Under the lease, so no creation of this process is between its record and
    /// its end.
    fn settle_earlier_acts(&self, wiki: &WikiRecord, open: &[String]) -> Result<(), ViewFailure> {
        let p = &self.parts;
        for entry in p.acts.view_acts(&wiki.id)? {
            if entry.end.is_some() {
                continue;
            }
            let there = open
                .iter()
                .any(|v| *v == format!("heads/{}", entry.act.name));
            p.acts.finish_view_act(&ViewActEnd {
                act: entry.act.id,
                outcome: if there {
                    ViewActOutcome::Created
                } else {
                    ViewActOutcome::Failed {
                        word: words::INTERRUPTED.to_owned(),
                    }
                },
                at: (self.clock)(),
            })?;
        }
        Ok(())
    }
}

/// The wiki's open views by full name, from prikk, now.
fn open_views(acceptor: &ProposalAcceptor, wiki: &WikiRecord) -> Result<Vec<String>, ViewFailure> {
    let repo = acceptor.parts.layout.repo_dir(&wiki.id);
    let views = acceptor
        .parts
        .storage
        .views(&repo)
        .map_err(|e| ViewFailure::Unavailable(e.to_string()))?;
    Ok(views
        .views
        .into_iter()
        .filter(|v| !v.closed)
        .map(|v| v.name)
        .collect())
}

/// The fixed word of a refusal that created nothing.
fn failed_word(answer: &Result<(), AcceptError>) -> &'static str {
    match answer {
        Err(AcceptError::RetirementUnfinished(_)) => words::RETIREMENT_UNFINISHED,
        Err(AcceptError::CeilingExceeded { .. }) => words::MEMORY_LIMIT,
        Err(AcceptError::Refused(_) | AcceptError::NotPending) => words::REFUSED,
        _ => words::UNAVAILABLE,
    }
}

#[cfg(test)]
mod tests;
