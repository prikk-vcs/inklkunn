//! The proposal service (RFC 004 D-1, D-3, D-8, D-10, D-11; internal design DM-3; external design CT-04): the
//! proposal as an object with a life, **before any page shows it**. Create, save and resume a draft, submit,
//! revise, comment, decide, decline, withdraw, supersede; the state machine ([`machine`]) with every
//! transition named; the quotas ([`quota`]) checked at the boundary; and the participation metrics
//! ([`metrics`]) computed from the records.
//!
//! **Not here** (handoff 004's later tasks): the merge (T2), the acceptance and its lease and recovery (T4,
//! T5), the routes and pages (T3), the inbox events and the rollback (T6), the write API (T7). A proposal
//! reaches *accepted* only through `ProposalService::record_accepted` (crate-private), which the acceptance
//! ([`crate::acceptance`], handoff 004 T4) calls once the signer has sealed; nothing in this module touches a
//! repository.
//!
//! **Every call is authorized first**, through the one [`Authorizer`], before any record is read: *propose*
//! on each page the change touches (or on the wiki, for a draft that touches none yet), *comment*, *review*
//! and *accept* on the proposal. A proposal is addressed with its wiki: one that belongs to another wiki is
//! as absent as one that does not exist, and a draft is absent to everyone but its author (PR-10). Only an
//! account acts on a proposal; the system principal and an anonymous visitor are `NotAllowed`.
//!
//! **Errors are typed facts** ([`ProposalError`]); each maps to one of D-12's words (*not allowed*,
//! *refused: the named limit*, the proposal states) and none carries prikk's text or a page's path. The words
//! themselves arrive with the pages that show them (handoff 004 T3).

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError};

use inklkunn_store::{
    AcceptanceStore, AccountId, AccountStore, Anchor, Base, CommentId, CommentRecord, DecisionId,
    DecisionKind, DecisionRecord, FileChange, ProposalChange, ProposalFilter, ProposalId,
    ProposalOp, ProposalRecord, ProposalState, ProposalStore, ProposalTransition, Rename,
    RevisionContent, RevisionRecord, StoreError, WikiRecord,
};

use crate::authorize::{Action, Authorizer, Principal, Resource};
use crate::notify::Notifier;
use crate::read::{ReadError, ReadService};

pub mod machine;
pub mod metrics;
pub mod quota;
pub mod rollback;

pub use machine::{Illegal, TRANSITIONS};
pub use metrics::{Metrics, Period, Ratio};
pub use quota::{QuotaConfig, QuotaLimit, QuotaRefusal, Quotas, Reset};

/// The longest summary, in characters (RFC 004 D-2; the CT-03 first line).
pub const MAX_SUMMARY_CHARS: usize = 120;
/// The most a proposal may hold, paths and texts together (RFC 004 D-2, PERF-4).
pub const MAX_PROPOSAL_BYTES: u64 = 2 * 1024 * 1024;
/// The most one page of a proposal may hold (RFC 004 D-2, PERF-4).
pub const MAX_PAGE_BYTES: usize = 1024 * 1024;
/// The longest repository path prikk accepts (ledger IS-3).
pub const MAX_PATH_BYTES: usize = 1024;
/// The most sources a proposal may name. A bound for the record's size; the RFC names none.
pub const MAX_SOURCES: usize = 20;
/// The longest source, in bytes. A bound for the record's size; the RFC names none.
pub const MAX_SOURCE_BYTES: usize = 2048;
/// The longest comment or decision text, in bytes. A bound for the record's size; the RFC names none.
pub const MAX_COMMENT_BYTES: usize = 16 * 1024;
/// The longest reason for a decline, a withdrawal or a request for a revision, in bytes. A bound for the
/// record's size; the RFC names none.
pub const MAX_REASON_BYTES: usize = 4096;

/// Where a new proposal's base comes from: the view's state when the form opens (RFC 004 D-1, D-2).
pub trait BaseSource: Send + Sync {
    /// The state id and the block id of the tip of `view` in the wiki. [`ReadService`] answers from
    /// `Storage::views` (and one `log --limit 1` for the block), so a base is always a pair prikk named
    /// together.
    fn base(&self, wiki: &WikiRecord, view: &str) -> std::result::Result<Base, BaseError>;
}

/// Why no base could be had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseError {
    /// The wiki has no open view of that name.
    NoSuchView(String),
    /// The repository could not be asked now (the text is for the operator).
    Unavailable(String),
}

impl BaseSource for ReadService {
    fn base(&self, wiki: &WikiRecord, view: &str) -> std::result::Result<Base, BaseError> {
        match self.tip(wiki, Some(view)) {
            Ok(tip) => Base::new(&tip.state_id, &tip.block_id).ok_or_else(|| {
                BaseError::Unavailable("the tip's ids are not state and block ids".to_owned())
            }),
            Err(ReadError::NoSuchView(v)) => Err(BaseError::NoSuchView(v)),
            Err(other) => Err(BaseError::Unavailable(other.to_string())),
        }
    }
}

/// What a proposer sends: the text of a draft or of a revision.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProposalEdit {
    /// "What and why": one line, at most [`MAX_SUMMARY_CHARS`] characters; required to submit.
    pub summary: String,
    /// Where the change comes from.
    pub sources: Vec<String>,
    /// The pages: each one's full new text, or its removal.
    pub files: Vec<FileChange>,
    /// The declared renames.
    pub renames: Vec<Rename>,
    /// A minor change.
    pub minor: bool,
    /// Machine-assisted content.
    pub automated: bool,
    /// The view's state the text was written against, captured when the form opened. `None`: for a new
    /// proposal, the tip now; for a draft or a revision, the base it already has (the text is an edit of the
    /// text that base produced, so it is never re-based silently).
    pub base: Option<Base>,
}

impl ProposalEdit {
    /// Every path the edit touches: the pages and both ends of each rename.
    fn touched(&self) -> BTreeSet<&str> {
        let files = self.files.iter().map(|f| f.path.as_str());
        let renames = self
            .renames
            .iter()
            .flat_map(|r| [r.from.as_str(), r.to.as_str()]);
        files.chain(renames).collect()
    }
}

/// A comment to add.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewComment {
    /// The text.
    pub body: String,
    /// The comment it replies to.
    pub parent: Option<CommentId>,
    /// The block it is about.
    pub anchor: Option<Anchor>,
}

/// What was wrong with what a person sent. Each is a plain fact; none repeats a path or a text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalid {
    /// A submitted proposal needs its summary.
    SummaryMissing,
    /// The summary is over [`MAX_SUMMARY_CHARS`].
    SummaryTooLong,
    /// The summary is more than one line, or holds a control character.
    SummaryNotOneLine,
    /// More than [`MAX_SOURCES`] sources.
    TooManySources,
    /// A source is empty, over [`MAX_SOURCE_BYTES`], or more than one line.
    BadSource,
    /// A submitted proposal must change a page or rename one.
    NothingProposed,
    /// The proposal is over [`MAX_PROPOSAL_BYTES`].
    TooLarge,
    /// A page is over [`MAX_PAGE_BYTES`].
    PageTooLarge,
    /// A path is empty, absolute, over [`MAX_PATH_BYTES`], holds a control character or a `..` part.
    BadPath,
    /// The same page is named twice.
    DuplicatePath,
    /// A rename moves a page to itself.
    RenameToSelf,
    /// The base is not a pair of state and block ids.
    BadBase,
    /// A reason is required and is empty.
    ReasonMissing,
    /// The reason is over [`MAX_REASON_BYTES`].
    ReasonTooLong,
    /// A comment must say something.
    CommentEmpty,
    /// The text is over [`MAX_COMMENT_BYTES`].
    CommentTooLong,
    /// The anchor names no block and quotes nothing, or one of its parts is out of bounds.
    BadAnchor,
    /// A proposal cannot supersede itself.
    SupersedeSelf,
}

/// Why a call to the service was refused. Every variant is one plain fact (RFC 004 D-6: failure is honest).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProposalError {
    /// *Not allowed*: the principal may not do this, or is not the person it is for (only the proposer
    /// withdraws).
    NotAllowed,
    /// There is no such proposal in the wiki, or the principal may not see it (a draft is its author's alone).
    NotFound,
    /// The act is not one the proposal's state allows ([`machine::TRANSITIONS`]).
    NotInThisState {
        /// The act.
        op: ProposalOp,
        /// The state the proposal is in.
        state: ProposalState,
    },
    /// What was sent is not acceptable.
    Invalid(Invalid),
    /// *Refused: (the named limit)* (D-11).
    Quota(QuotaRefusal),
    /// This reviewer has decided on this revision already; a decision is not changed, the next revision is
    /// decided on again.
    AlreadyDecided,
    /// The decision is on a revision that is no longer the latest.
    StaleRevision {
        /// The latest revision.
        latest: u32,
    },
    /// The proposal that would supersede this one is not one that can (it is a draft, or it ended other than
    /// by being accepted).
    SupersederNotEligible {
        /// Its state.
        state: ProposalState,
    },
    /// The proposal changed while the call was being made (another act got there first); read it again.
    Changed,
    /// The wiki has no open view of that name.
    NoSuchView(String),
    /// The repository could not be asked for its tip now (the text is for the operator).
    BaseUnavailable(String),
    /// The store failed (the text is for the operator).
    Store(StoreError),
}

impl std::fmt::Display for ProposalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProposalError::NotAllowed => write!(f, "not allowed"),
            ProposalError::NotFound => write!(f, "no such proposal"),
            ProposalError::NotInThisState { op, state } => {
                write!(f, "`{op}` is not possible while the proposal is {state}")
            }
            ProposalError::Invalid(why) => write!(f, "not acceptable: {why:?}"),
            ProposalError::Quota(q) => write!(
                f,
                "refused: {} (allowed {}, now {}, asked {})",
                q.limit.name(),
                q.allowed,
                q.current,
                q.asked
            ),
            ProposalError::AlreadyDecided => write!(f, "already decided on this revision"),
            ProposalError::StaleRevision { latest } => {
                write!(f, "revision {latest} is the latest")
            }
            ProposalError::SupersederNotEligible { state } => {
                write!(f, "a {state} proposal cannot supersede another")
            }
            ProposalError::Changed => write!(f, "the proposal changed meanwhile"),
            ProposalError::NoSuchView(v) => write!(f, "the wiki has no open view `{v}`"),
            ProposalError::BaseUnavailable(e) => write!(f, "the tip could not be read: {e}"),
            ProposalError::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ProposalError {}

impl From<StoreError> for ProposalError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::NotFound => ProposalError::NotFound,
            StoreError::StateChanged => ProposalError::Changed,
            other => ProposalError::Store(other),
        }
    }
}

impl From<Illegal> for ProposalError {
    fn from(i: Illegal) -> Self {
        match i.state {
            Some(state) => ProposalError::NotInThisState { op: i.op, state },
            // An act on a proposal that is not there is refused before the table is asked.
            None => ProposalError::NotFound,
        }
    }
}

impl From<BaseError> for ProposalError {
    fn from(e: BaseError) -> Self {
        match e {
            BaseError::NoSuchView(v) => ProposalError::NoSuchView(v),
            BaseError::Unavailable(why) => ProposalError::BaseUnavailable(why),
        }
    }
}

impl From<Invalid> for ProposalError {
    fn from(i: Invalid) -> Self {
        ProposalError::Invalid(i)
    }
}

type Result<T> = std::result::Result<T, ProposalError>;

/// The account behind a principal: only an account proposes, reviews, comments or decides.
fn account_of(principal: &Principal) -> Result<&AccountId> {
    match principal {
        Principal::Account(a) => Ok(a.id()),
        Principal::Anonymous | Principal::System(_) => Err(ProposalError::NotAllowed),
    }
}

fn viewer_of(principal: &Principal) -> Option<&AccountId> {
    match principal {
        Principal::Account(a) => Some(a.id()),
        Principal::Anonymous | Principal::System(_) => None,
    }
}

/// The state `op` leaves a proposal in, by the one table.
fn next(state: Option<ProposalState>, op: ProposalOp) -> Result<ProposalState> {
    Ok(machine::apply(state, op)?)
}

/// A path prikk would take as a relative path inside the repository.
fn path_ok(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PATH_BYTES
        && !path.starts_with('/')
        && !path.chars().any(char::is_control)
        && !path.split('/').any(|part| part == ".." || part.is_empty())
}

/// How strictly an edit is checked: a draft may be unfinished, a submission may not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Finish {
    Draft,
    Submit,
}

fn one_line(s: &str) -> bool {
    !s.chars().any(char::is_control)
}

/// Check the words, sources and pages of an edit (or of a stored draft being submitted).
fn validate(
    summary: &str,
    sources: &[String],
    content: &RevisionContent,
    finish: Finish,
) -> std::result::Result<(), Invalid> {
    let summary = summary.trim();
    if summary.chars().count() > MAX_SUMMARY_CHARS {
        return Err(Invalid::SummaryTooLong);
    }
    if !one_line(summary) {
        return Err(Invalid::SummaryNotOneLine);
    }
    if finish == Finish::Submit && summary.is_empty() {
        return Err(Invalid::SummaryMissing);
    }
    if sources.len() > MAX_SOURCES {
        return Err(Invalid::TooManySources);
    }
    if sources
        .iter()
        .any(|s| s.trim().is_empty() || s.len() > MAX_SOURCE_BYTES || !one_line(s))
    {
        return Err(Invalid::BadSource);
    }
    if Base::new(&content.base.state_id, &content.base.block_id).is_none() {
        return Err(Invalid::BadBase);
    }
    let mut seen = BTreeSet::new();
    for f in &content.files {
        if !path_ok(&f.path) {
            return Err(Invalid::BadPath);
        }
        if !seen.insert(f.path.as_str()) {
            return Err(Invalid::DuplicatePath);
        }
        if f.text.as_ref().is_some_and(|t| t.len() > MAX_PAGE_BYTES) {
            return Err(Invalid::PageTooLarge);
        }
    }
    let mut froms = BTreeSet::new();
    let mut tos = BTreeSet::new();
    for r in &content.renames {
        if !path_ok(&r.from) || !path_ok(&r.to) {
            return Err(Invalid::BadPath);
        }
        if r.from == r.to {
            return Err(Invalid::RenameToSelf);
        }
        if !froms.insert(r.from.as_str()) || !tos.insert(r.to.as_str()) {
            return Err(Invalid::DuplicatePath);
        }
    }
    if content.bytes() > MAX_PROPOSAL_BYTES {
        return Err(Invalid::TooLarge);
    }
    if finish == Finish::Submit && content.files.is_empty() && content.renames.is_empty() {
        return Err(Invalid::NothingProposed);
    }
    Ok(())
}

/// A reason, trimmed; required and bounded.
fn clean_reason(reason: &str) -> std::result::Result<String, Invalid> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(Invalid::ReasonMissing);
    }
    if reason.len() > MAX_REASON_BYTES {
        return Err(Invalid::ReasonTooLong);
    }
    Ok(reason.to_owned())
}

fn valid_anchor(a: &Anchor) -> bool {
    let bounded = |s: &str, max: usize| s.len() <= max;
    let block_ok = a
        .block_id
        .as_ref()
        .is_none_or(|b| !b.is_empty() && bounded(b, 128));
    let quote_ok = a.quote.as_ref().is_none_or(|q| {
        !q.exact.is_empty()
            && bounded(&q.exact, 2048)
            && bounded(&q.prefix, 512)
            && bounded(&q.suffix, 512)
    });
    path_ok(&a.page) && block_ok && quote_ok && (a.block_id.is_some() || a.quote.is_some())
}

/// The proposal service.
pub struct ProposalService {
    proposals: Arc<dyn ProposalStore>,
    authorizer: Authorizer,
    bases: Arc<dyn BaseSource>,
    quotas: Quotas,
    clock: fn() -> i64,
    /// Serializes the checks of the quotas with the creation they guard, so that two requests cannot both
    /// find room for the last open proposal.
    admission: Mutex<()>,
    /// Who is told what (RFC 004 D-9), when there is someone to tell.
    notifier: Option<Arc<Notifier>>,
}

impl std::fmt::Debug for ProposalService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProposalService")
    }
}

impl ProposalService {
    /// A service over these records, authorizing through `authorizer`, taking bases from `bases`, limited by
    /// `quotas`.
    pub fn new(
        proposals: Arc<dyn ProposalStore>,
        acceptances: Arc<dyn AcceptanceStore>,
        accounts: Arc<dyn AccountStore>,
        authorizer: Authorizer,
        bases: Arc<dyn BaseSource>,
        quotas: QuotaConfig,
    ) -> Self {
        let quotas = Quotas::new(quotas, proposals.clone(), acceptances, accounts);
        ProposalService {
            proposals,
            authorizer,
            bases,
            quotas,
            clock: inklkunn_store::record::now_secs,
            admission: Mutex::new(()),
            notifier: None,
        }
    }

    /// The same, telling people what happens to their proposals (RFC 004 D-9): a review, a decline, a
    /// request for a revision, a withdrawal, a supersession, a reply, a mention. Telling is best effort and
    /// never changes the act ([`crate::notify`]).
    #[must_use]
    pub fn with_notifier(mut self, notifier: Arc<Notifier>) -> Self {
        self.notifier = Some(notifier);
        self
    }

    /// Tell, if there is a notifier.
    fn notify(&self, tell: impl FnOnce(&Notifier)) {
        if let Some(notifier) = &self.notifier {
            tell(notifier);
        }
    }

    /// The same, reading the time from `clock` (Unix seconds): tests pin it.
    #[must_use]
    pub fn with_clock(mut self, clock: fn() -> i64) -> Self {
        self.quotas = self.quotas.with_clock(clock);
        self.clock = clock;
        self
    }

    /// The limits, for the acceptance (the accepted-bytes quota, the queue bound).
    pub fn quotas(&self) -> &Quotas {
        &self.quotas
    }

    /// The tip to write a new proposal against: what a form captures when it opens (RFC 004 D-2).
    pub fn capture_base(&self, wiki: &WikiRecord, view: Option<&str>) -> Result<Base> {
        Ok(self.bases.base(wiki, view.unwrap_or(&wiki.default_view))?)
    }

    // --- authorization and loading

    fn allow(&self, principal: &Principal, action: Action, resource: &Resource) -> Result<()> {
        if self
            .authorizer
            .authorize(principal, action, resource)
            .is_allowed()
        {
            Ok(())
        } else {
            Err(ProposalError::NotAllowed)
        }
    }

    fn proposal_resource(wiki: &WikiRecord, id: &ProposalId) -> Resource {
        Resource::Proposal {
            wiki: wiki.id.clone(),
            id: id.to_string(),
        }
    }

    /// *Propose* on every page a change touches (on the wiki when it touches none yet).
    fn allow_pages(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        paths: &BTreeSet<&str>,
    ) -> Result<()> {
        if paths.is_empty() {
            return self.allow(
                principal,
                Action::Propose,
                &Resource::Wiki {
                    wiki: wiki.id.clone(),
                },
            );
        }
        for path in paths {
            self.allow(
                principal,
                Action::Propose,
                &Resource::Page {
                    wiki: wiki.id.clone(),
                    path: (*path).to_owned(),
                },
            )?;
        }
        Ok(())
    }

    /// The proposal as `viewer` may see it, in this wiki; anything else is absent.
    fn load(
        &self,
        wiki: &WikiRecord,
        id: &ProposalId,
        viewer: Option<&AccountId>,
    ) -> Result<ProposalRecord> {
        match self.proposals.proposal(id, viewer)? {
            Some(p) if p.wiki == wiki.id => Ok(p),
            _ => Err(ProposalError::NotFound),
        }
    }

    fn latest_revision(&self, id: &ProposalId, viewer: &AccountId) -> Result<RevisionRecord> {
        self.proposals
            .revisions(id, Some(viewer))?
            .pop()
            .ok_or(ProposalError::Changed)
    }

    fn content_of(&self, edit: &ProposalEdit, base: Base) -> RevisionContent {
        RevisionContent {
            base,
            files: edit.files.clone(),
            renames: edit.renames.clone(),
            minor: edit.minor,
            automated: edit.automated,
        }
    }

    /// The edit's own base, checked, or `fallback`.
    fn base_of(edit: &ProposalEdit, fallback: impl FnOnce() -> Result<Base>) -> Result<Base> {
        match &edit.base {
            Some(b) => {
                Base::new(&b.state_id, &b.block_id).ok_or(ProposalError::Invalid(Invalid::BadBase))
            }
            None => fallback(),
        }
    }

    // --- the life of a proposal

    /// Open a proposal as a draft, visible to its author only. The base is the one the edit names, else the
    /// tip of `view` now. Checks *propose* on every page, the text, and the quotas of open proposals and of
    /// the hour, in one critical section with the creation.
    pub fn create(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        view: Option<&str>,
        edit: ProposalEdit,
    ) -> Result<ProposalRecord> {
        self.create_as(principal, wiki, view, edit, None)
    }

    /// [`ProposalService::create`], and, for a rollback (`undoes` is the block it undoes), the note that it
    /// is one, written in the same step as the proposal.
    fn create_as(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        view: Option<&str>,
        edit: ProposalEdit,
        undoes: Option<&str>,
    ) -> Result<ProposalRecord> {
        let author = account_of(principal)?;
        self.allow_pages(principal, wiki, &edit.touched())?;
        let view = view.unwrap_or(&wiki.default_view).to_owned();
        let base = Self::base_of(&edit, || Ok(self.bases.base(wiki, &view)?))?;
        let content = self.content_of(&edit, base);
        validate(&edit.summary, &edit.sources, &content, Finish::Draft)?;
        let state = next(None, ProposalOp::Create)?;

        let _admission = self
            .admission
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.quotas.check_open(&wiki.id, author)?;
        self.quotas.check_opening_rate(&wiki.id, author)?;
        let now = (self.clock)();
        let record = ProposalRecord {
            id: ProposalId::new(),
            wiki: wiki.id.clone(),
            view,
            state,
            author: author.clone(),
            summary: edit.summary.trim().to_owned(),
            sources: edit.sources,
            superseded_by: None,
            created_at: now,
            updated_at: now,
            submitted_at: None,
            closed_at: None,
        };
        let created = ProposalTransition {
            proposal: record.id.clone(),
            op: ProposalOp::Create,
            from: None,
            to: state,
            actor: author.clone(),
            reason: None,
            at: now,
        };
        match undoes {
            Some(undoes) => self
                .proposals
                .create_rollback_proposal(&record, &content, &created, undoes)?,
            None => self
                .proposals
                .create_proposal(&record, &content, &created)?,
        }
        Ok(record)
    }

    /// Save the draft's working copy (resuming it later is [`ProposalService::draft`]). Only its author, and
    /// only while it is a draft.
    pub fn save_draft(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
        edit: ProposalEdit,
    ) -> Result<ProposalRecord> {
        let author = account_of(principal)?;
        self.allow_pages(principal, wiki, &edit.touched())?;
        let p = self.load(wiki, id, Some(author))?;
        if p.author != *author {
            return Err(ProposalError::NotAllowed);
        }
        next(Some(p.state), ProposalOp::SaveDraft)?;
        let current = self
            .proposals
            .draft(id, Some(author))?
            .ok_or(ProposalError::Changed)?;
        let base = Self::base_of(&edit, || Ok(current.base.clone()))?;
        let content = self.content_of(&edit, base);
        validate(&edit.summary, &edit.sources, &content, Finish::Draft)?;
        let now = (self.clock)();
        self.proposals
            .save_draft(id, edit.summary.trim(), &edit.sources, &content, now)?;
        self.load(wiki, id, Some(author))
    }

    /// Send the draft for review: its working copy becomes revision 1 and the proposal is held for review.
    pub fn submit(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<ProposalRecord> {
        let author = account_of(principal)?;
        self.allow(
            principal,
            Action::Propose,
            &Self::proposal_resource(wiki, id),
        )?;
        let p = self.load(wiki, id, Some(author))?;
        if p.author != *author {
            return Err(ProposalError::NotAllowed);
        }
        let to = next(Some(p.state), ProposalOp::Submit)?;
        let draft = self
            .proposals
            .draft(id, Some(author))?
            .ok_or(ProposalError::Changed)?;
        let touched: BTreeSet<&str> = draft
            .files
            .iter()
            .map(|f| f.path.as_str())
            .chain(
                draft
                    .renames
                    .iter()
                    .flat_map(|r| [r.from.as_str(), r.to.as_str()]),
            )
            .collect();
        self.allow_pages(principal, wiki, &touched)?;
        validate(&p.summary, &p.sources, &draft, Finish::Submit)?;
        let now = (self.clock)();
        let change = ProposalChange {
            transition: ProposalTransition {
                proposal: id.clone(),
                op: ProposalOp::Submit,
                from: Some(p.state),
                to,
                actor: author.clone(),
                reason: None,
                at: now,
            },
            text: None,
            revision: Some(RevisionRecord {
                proposal: id.clone(),
                seq: 1,
                content: draft,
                created_at: now,
            }),
            superseded_by: None,
        };
        Ok(self.proposals.change_proposal(&change)?)
    }

    /// Add a revision to a proposal that is held for review or needs one: the proposal is held for review
    /// again. The revision supersedes the decisions on earlier revisions for the purpose of acceptance; they
    /// stay recorded. Only the proposer, within the revision quota.
    pub fn revise(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
        edit: ProposalEdit,
    ) -> Result<ProposalRecord> {
        let author = account_of(principal)?;
        self.allow_pages(principal, wiki, &edit.touched())?;
        let p = self.load(wiki, id, Some(author))?;
        if p.author != *author {
            return Err(ProposalError::NotAllowed);
        }
        let to = next(Some(p.state), ProposalOp::Revise)?;
        let previous = self.latest_revision(id, author)?;
        self.quotas
            .check_revisions(author, u64::from(previous.seq))?;
        let base = Self::base_of(&edit, || Ok(previous.content.base.clone()))?;
        let content = self.content_of(&edit, base);
        validate(&edit.summary, &edit.sources, &content, Finish::Submit)?;
        let now = (self.clock)();
        let change = ProposalChange {
            transition: ProposalTransition {
                proposal: id.clone(),
                op: ProposalOp::Revise,
                from: Some(p.state),
                to,
                actor: author.clone(),
                reason: None,
                at: now,
            },
            text: Some((edit.summary.trim().to_owned(), edit.sources)),
            revision: Some(RevisionRecord {
                proposal: id.clone(),
                seq: previous.seq + 1,
                content,
                created_at: now,
            }),
            superseded_by: None,
        };
        Ok(self.proposals.change_proposal(&change)?)
    }

    /// Comment on a proposal that is held for review or needs a revision, on its latest revision.
    pub fn comment(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
        new: NewComment,
    ) -> Result<CommentRecord> {
        let who = account_of(principal)?;
        self.allow(
            principal,
            Action::Comment,
            &Self::proposal_resource(wiki, id),
        )?;
        let p = self.load(wiki, id, Some(who))?;
        next(Some(p.state), ProposalOp::Comment)?;
        let body = new.body.trim();
        if body.is_empty() {
            return Err(Invalid::CommentEmpty.into());
        }
        if body.len() > MAX_COMMENT_BYTES {
            return Err(Invalid::CommentTooLong.into());
        }
        if new.anchor.as_ref().is_some_and(|a| !valid_anchor(a)) {
            return Err(Invalid::BadAnchor.into());
        }
        let revision = self.latest_revision(id, who)?.seq;
        let comment = CommentRecord {
            id: CommentId::new(),
            proposal: id.clone(),
            revision,
            author: who.clone(),
            parent: new.parent,
            body: body.to_owned(),
            anchor: new.anchor,
            created_at: (self.clock)(),
        };
        self.proposals.add_comment(
            &comment,
            &[ProposalState::HeldForReview, ProposalState::NeedsRevision],
        )?;
        self.notify(|n| n.commented(wiki, &p, &comment));
        Ok(comment)
    }

    /// Decide on a revision: approve it or ask for changes. One decision per reviewer per revision, bound to
    /// the revision that was reviewed; only on the latest, and only while the proposal is held for review.
    pub fn decide(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
        revision: u32,
        kind: DecisionKind,
        body: &str,
    ) -> Result<DecisionRecord> {
        let who = account_of(principal)?;
        self.allow(
            principal,
            Action::Review,
            &Self::proposal_resource(wiki, id),
        )?;
        let p = self.load(wiki, id, Some(who))?;
        next(Some(p.state), ProposalOp::Decide)?;
        let body = body.trim();
        if body.len() > MAX_COMMENT_BYTES {
            return Err(Invalid::CommentTooLong.into());
        }
        let latest = self.latest_revision(id, who)?.seq;
        if revision != latest {
            return Err(ProposalError::StaleRevision { latest });
        }
        let decision = DecisionRecord {
            id: DecisionId::new(),
            proposal: id.clone(),
            revision,
            reviewer: who.clone(),
            kind,
            body: body.to_owned(),
            created_at: (self.clock)(),
        };
        match self.proposals.add_decision(&decision) {
            Ok(()) => {
                self.notify(|n| n.reviewed(wiki, &p, who));
                Ok(decision)
            }
            Err(StoreError::AlreadyRecorded) => Err(ProposalError::AlreadyDecided),
            Err(e) => Err(e.into()),
        }
    }

    /// One step that ends a proposal or sends it back, with the reason a person gave.
    fn step(
        &self,
        wiki: &WikiRecord,
        id: &ProposalId,
        op: ProposalOp,
        actor: &AccountId,
        reason: &str,
    ) -> Result<ProposalRecord> {
        let reason = clean_reason(reason)?;
        let p = self.load(wiki, id, Some(actor))?;
        let to = next(Some(p.state), op)?;
        let change = ProposalChange {
            transition: ProposalTransition {
                proposal: id.clone(),
                op,
                from: Some(p.state),
                to,
                actor: actor.clone(),
                reason: Some(reason),
                at: (self.clock)(),
            },
            text: None,
            revision: None,
            superseded_by: None,
        };
        let after = self.proposals.change_proposal(&change)?;
        // Whether the proposal was ever sent for review (its record says) decides who held it.
        self.notify(|n| match op {
            ProposalOp::Decline => n.declined(wiki, &after, actor),
            ProposalOp::AskForRevision => n.needs_revision(wiki, &after, actor),
            ProposalOp::Withdraw => n.withdrawn(wiki, &after, actor),
            _ => {}
        });
        Ok(after)
    }

    /// Send a held proposal back to its proposer with a reason (a decline with changes). An acceptor's act.
    pub fn ask_for_revision(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
        reason: &str,
    ) -> Result<ProposalRecord> {
        let who = account_of(principal)?;
        self.allow(
            principal,
            Action::Accept,
            &Self::proposal_resource(wiki, id),
        )?;
        self.step(wiki, id, ProposalOp::AskForRevision, who, reason)
    }

    /// Decline a proposal, with a reason. An acceptor's act.
    pub fn decline(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
        reason: &str,
    ) -> Result<ProposalRecord> {
        let who = account_of(principal)?;
        self.allow(
            principal,
            Action::Accept,
            &Self::proposal_resource(wiki, id),
        )?;
        self.step(wiki, id, ProposalOp::Decline, who, reason)
    }

    /// Take a proposal back, with a reason. The proposer's act, a draft included.
    pub fn withdraw(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
        reason: &str,
    ) -> Result<ProposalRecord> {
        let who = account_of(principal)?;
        self.allow(
            principal,
            Action::Propose,
            &Self::proposal_resource(wiki, id),
        )?;
        let p = self.load(wiki, id, Some(who))?;
        if p.author != *who {
            return Err(ProposalError::NotAllowed);
        }
        self.step(wiki, id, ProposalOp::Withdraw, who, reason)
    }

    /// Name `old` as superseded by `new`. By the proposer of `new`, who is the proposer of `old` or an
    /// acceptor; `new` is held for review, needs a revision, or was accepted.
    pub fn supersede(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        old: &ProposalId,
        new: &ProposalId,
    ) -> Result<ProposalRecord> {
        let who = account_of(principal)?;
        self.allow(
            principal,
            Action::Propose,
            &Self::proposal_resource(wiki, new),
        )?;
        if old == new {
            return Err(Invalid::SupersedeSelf.into());
        }
        let superseder = self.load(wiki, new, Some(who))?;
        if superseder.author != *who {
            return Err(ProposalError::NotAllowed);
        }
        let old_record = self.load(wiki, old, Some(who))?;
        if old_record.author != *who {
            self.allow(
                principal,
                Action::Accept,
                &Self::proposal_resource(wiki, old),
            )?;
        }
        let to = next(Some(old_record.state), ProposalOp::Supersede)?;
        if !matches!(
            superseder.state,
            ProposalState::HeldForReview | ProposalState::NeedsRevision | ProposalState::Accepted
        ) {
            return Err(ProposalError::SupersederNotEligible {
                state: superseder.state,
            });
        }
        let change = ProposalChange {
            transition: ProposalTransition {
                proposal: old.clone(),
                op: ProposalOp::Supersede,
                from: Some(old_record.state),
                to,
                actor: who.clone(),
                reason: None,
                at: (self.clock)(),
            },
            text: None,
            revision: None,
            superseded_by: Some(new.clone()),
        };
        let after = self.proposals.change_proposal(&change)?;
        self.notify(|n| n.superseded(wiki, &after, who));
        Ok(after)
    }

    /// *Accept* on the proposal, **first and before anything else is read** (RFC 004 D-4: the acceptance
    /// authorizes before it takes the lease): the account that may accept it, or *not allowed*. Only an
    /// account accepts; the system principal's changes have no proposal.
    pub(crate) fn authorize_accept(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<AccountId> {
        let who = account_of(principal)?;
        self.allow(
            principal,
            Action::Accept,
            &Self::proposal_resource(wiki, id),
        )?;
        Ok(who.clone())
    }

    /// Record that a proposal's acceptance sealed: held for review becomes accepted. **Called by the
    /// acceptance (handoff 004 T4) after the signer has sealed, and by nothing else**: it does not
    /// authorize, because the acceptance already did, under the lease.
    pub(crate) fn record_accepted(
        &self,
        wiki: &WikiRecord,
        id: &ProposalId,
        acceptor: &AccountId,
    ) -> Result<ProposalRecord> {
        self.record_step(wiki, id, acceptor, ProposalOp::Accept, None)
    }

    /// Send a held proposal back to its proposer because the acceptance found overlaps (RFC 004 D-4 step 3):
    /// *needs a revision*, with `reason` (the fixed word of the outcome). Called by the acceptance, under the
    /// lease, after it recorded both texts; it does not authorize, because the acceptance already did.
    pub(crate) fn record_needs_revision(
        &self,
        wiki: &WikiRecord,
        id: &ProposalId,
        acceptor: &AccountId,
        reason: &str,
    ) -> Result<ProposalRecord> {
        self.record_step(wiki, id, acceptor, ProposalOp::AskForRevision, Some(reason))
    }

    fn record_step(
        &self,
        wiki: &WikiRecord,
        id: &ProposalId,
        actor: &AccountId,
        op: ProposalOp,
        reason: Option<&str>,
    ) -> Result<ProposalRecord> {
        let p = self.load(wiki, id, Some(actor))?;
        let to = next(Some(p.state), op)?;
        let change = ProposalChange {
            transition: ProposalTransition {
                proposal: id.clone(),
                op,
                from: Some(p.state),
                to,
                actor: actor.clone(),
                reason: reason.map(str::to_owned),
                at: (self.clock)(),
            },
            text: None,
            revision: None,
            superseded_by: None,
        };
        Ok(self.proposals.change_proposal(&change)?)
    }

    // --- reading

    fn readable<'a>(
        &self,
        principal: &'a Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<Option<&'a AccountId>> {
        self.allow(principal, Action::Read, &Self::proposal_resource(wiki, id))?;
        Ok(viewer_of(principal))
    }

    /// The proposal, if the principal may read the wiki and see it.
    pub fn get(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<ProposalRecord> {
        let viewer = self.readable(principal, wiki, id)?;
        self.load(wiki, id, viewer)
    }

    /// The wiki's proposals in the order they were opened, as the principal may see them.
    pub fn list(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        filter: &ProposalFilter,
    ) -> Result<Vec<ProposalRecord>> {
        self.allow(
            principal,
            Action::Read,
            &Resource::Wiki {
                wiki: wiki.id.clone(),
            },
        )?;
        Ok(self
            .proposals
            .proposals(&wiki.id, filter, viewer_of(principal))?)
    }

    /// The draft's working copy, for its author to resume.
    pub fn draft(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<RevisionContent> {
        let viewer = self.readable(principal, wiki, id)?;
        self.load(wiki, id, viewer)?;
        self.proposals
            .draft(id, viewer)?
            .ok_or(ProposalError::NotFound)
    }

    /// The revisions, oldest first.
    pub fn revisions(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<Vec<RevisionRecord>> {
        let viewer = self.readable(principal, wiki, id)?;
        self.load(wiki, id, viewer)?;
        Ok(self.proposals.revisions(id, viewer)?)
    }

    /// The comments, oldest first.
    pub fn comments(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<Vec<CommentRecord>> {
        let viewer = self.readable(principal, wiki, id)?;
        self.load(wiki, id, viewer)?;
        Ok(self.proposals.comments(id, viewer)?)
    }

    /// The decisions, oldest first, each with the revision it is bound to.
    pub fn decisions(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<Vec<DecisionRecord>> {
        let viewer = self.readable(principal, wiki, id)?;
        self.load(wiki, id, viewer)?;
        Ok(self.proposals.decisions(id, viewer)?)
    }

    /// The log of the proposal's steps, oldest first.
    pub fn transitions(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<Vec<ProposalTransition>> {
        let viewer = self.readable(principal, wiki, id)?;
        self.load(wiki, id, viewer)?;
        Ok(self.proposals.transitions(id, viewer)?)
    }

    /// The participation metrics of a wiki over a period (D-10), computed from the records. An
    /// administrator's: the figures are about people.
    pub fn metrics(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        period: Period,
    ) -> Result<Metrics> {
        self.allow(
            principal,
            Action::Admin,
            &Resource::Wiki {
                wiki: wiki.id.clone(),
            },
        )?;
        // No viewer: a draft is its author's alone and is in no figure.
        let proposals = self
            .proposals
            .proposals(&wiki.id, &ProposalFilter::default(), None)?;
        let decisions = self.proposals.wiki_decisions(&wiki.id)?;
        Ok(metrics::compute(&wiki.id, period, &proposals, &decisions))
    }
}

#[cfg(test)]
mod tests;
