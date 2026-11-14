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
    RevisionContent, RevisionRecord, Role, StoreError, WikiRecord,
};

use crate::authorize::{Action, Authorizer, Principal, Resource};
use crate::notify::Notifier;
use crate::read::{ReadError, ReadService};

pub mod claim_thread;
pub mod files;
pub mod machine;
pub mod metrics;
pub mod quota;
pub mod rollback;

pub use claim_thread::NewClaimComment;
pub use machine::{Illegal, TRANSITIONS};
pub use metrics::{
    ClaimFigures, Metrics, Period, REVIEW_RECORD_NOTE, REVIEW_RECORD_SCHEMA, Ratio,
    operator_metrics,
};
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

    /// Every file under `pages/` at `base`, as repository paths. What a path "absent at the base" and a case
    /// collision are judged against (P4).
    fn page_paths(
        &self,
        wiki: &WikiRecord,
        base: &Base,
    ) -> std::result::Result<BTreeSet<String>, BaseError>;

    /// The text of the file at `path` at `base`; `None` when there was no such file then, or it is not text.
    /// What a changed, removed or renamed page is judged against (P5).
    fn page_text(
        &self,
        wiki: &WikiRecord,
        base: &Base,
        path: &str,
    ) -> std::result::Result<Option<String>, BaseError>;

    /// The paths of the pages whose `id` is `id` at `base`: where an expression's page is when a new source link is
    /// judged on the wording of its paragraph (K25). **The default reads every page**, which is right and slow;
    /// [`ReadService`] answers from the title index of exactly that state.
    fn pages_with_id(
        &self,
        wiki: &WikiRecord,
        base: &Base,
        id: &str,
    ) -> std::result::Result<Vec<String>, BaseError> {
        let mut found = Vec::new();
        for path in self.page_paths(wiki, base)? {
            if self.page_text(wiki, base, &path)?.is_some_and(|text| {
                crate::knowledge::compose::page_id_of(&text).as_deref() == Some(id)
            }) {
                found.push(path);
            }
        }
        Ok(found)
    }
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

    fn page_paths(
        &self,
        wiki: &WikiRecord,
        base: &Base,
    ) -> std::result::Result<BTreeSet<String>, BaseError> {
        self.page_paths_at(wiki, &base.block_id)
            .map_err(|e| BaseError::Unavailable(e.to_string()))
    }

    fn page_text(
        &self,
        wiki: &WikiRecord,
        base: &Base,
        path: &str,
    ) -> std::result::Result<Option<String>, BaseError> {
        self.source_at(wiki, &base.block_id, path)
            .map_err(|e| BaseError::Unavailable(e.to_string()))
    }

    fn pages_with_id(
        &self,
        wiki: &WikiRecord,
        base: &Base,
        id: &str,
    ) -> std::result::Result<Vec<String>, BaseError> {
        let titles = self
            .title_index_at_base(wiki, None, &base.state_id, &base.block_id)
            .map_err(|e| BaseError::Unavailable(e.to_string()))?;
        Ok(titles
            .entries()
            .iter()
            .filter(|e| e.page_id.as_deref() == Some(id))
            .map(|e| e.path.clone())
            .collect())
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
    /// A page or either end of a rename is not a `.md` file inside `pages/` (P4).
    PathOutsidePages,
    /// A page that is not at the base has a path other than the one its title gives (P4).
    PathNotCanonical,
    /// A new path collides, on a case-insensitive file system, with a page that stays or with another new path;
    /// or a rename lands on a page that stays (P4).
    PathCollides,
    /// The information at the top of a page cannot be read, so the keys only an administrator may change cannot
    /// be told (P5).
    FrontUnreadable,
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
    /// The knowledge files set what only a curation proposal sets (a community status, a review date, an
    /// availability, a change to a source link a claim already has, the removal of a claim whose standing was
    /// decided) and the proposal is not one: the acceptance would refuse it, so it is refused when it is sent
    /// (RFC 005 part 2 D-3; handoff 005 corrections K7).
    KnowledgeCurated,
    /// A claim or source file changes something the proposal's page would not describe line by line, so whoever
    /// reviews it could not see what it does (RFC 005 part 1 H-1: trust never moves silently; handoff 005
    /// corrections K5).
    KnowledgeNotShown,
    /// A claim file adds a source link judged on a wording that no paragraph of the claim has once the proposal is made:
    /// the acceptance would refuse it, and a digest must never name wording that is not in the history (RFC 005 part 2
    /// D-3, *a new link*; handoff 005 corrections, third round, K25).
    KnowledgeUnjudged,
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
    /// The edit adds, changes or removes keys only an administrator of the wiki may change (they are named);
    /// removing or renaming a page that carries one counts (P5).
    OwnedKeys(Vec<&'static str>),
    /// The reviewer is the proposer: a proposal's own approve or request for changes is refused by name (ruling
    /// D9). A comment on one's own proposal stays.
    OwnProposal,
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
            ProposalError::OwnedKeys(keys) => write!(
                f,
                "refused: only an administrator may change {}",
                keys.join(", ")
            ),
            ProposalError::OwnProposal => {
                write!(f, "a proposer cannot decide on their own proposal")
            }
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

/// Every path a change touches: the pages and both ends of each rename.
fn touched_by(content: &RevisionContent) -> BTreeSet<&str> {
    let files = content.files.iter().map(|f| f.path.as_str());
    let renames = content
        .renames
        .iter()
        .flat_map(|r| [r.from.as_str(), r.to.as_str()]);
    files.chain(renames).collect()
}

/// The step that sends a draft for review: the transition and its working copy as revision 1. **The one place** it is
/// built, for [`ProposalService::submit`] and for the reconfirm step's proposal, which is made already sent.
fn submission(
    id: &ProposalId,
    from: ProposalState,
    to: ProposalState,
    actor: &AccountId,
    content: RevisionContent,
    at: i64,
) -> ProposalChange {
    ProposalChange {
        transition: ProposalTransition {
            proposal: id.clone(),
            op: ProposalOp::Submit,
            from: Some(from),
            to,
            actor: actor.clone(),
            reason: None,
            at,
        },
        text: None,
        revision: Some(RevisionRecord {
            proposal: id.clone(),
            seq: 1,
            content,
            created_at: at,
        }),
        superseded_by: None,
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
        // A file is a page, or a knowledge file in the place the knowledge layer reads (`knowledge/claims/…` and
        // `knowledge/sources/…`; its name and content are judged at acceptance, K-T2). Nothing else.
        if !files::is_page_path(&f.path) && crate::knowledge::family_of(&f.path).is_none() {
            return Err(Invalid::PathOutsidePages);
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
        if !files::is_page_path(&r.from) || !files::is_page_path(&r.to) {
            return Err(Invalid::PathOutsidePages);
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
    /// A claim's own thread (RFC 005 part 2 D-11), when the service keeps them.
    claim_threads: Option<claim_thread::Threads>,
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
            claim_threads: None,
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

    /// May `principal` do `action` to `resource`? The acceptance path's other acts (a view's creation) are
    /// authorized through the same authorizer as the proposals', so that there is one.
    pub(crate) fn authorizes(
        &self,
        principal: &Principal,
        action: Action,
        resource: &Resource,
    ) -> bool {
        self.authorizer
            .authorize(principal, action, resource)
            .is_allowed()
    }

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
        self.create_as(principal, wiki, view, edit, Origin::Plain)
    }

    /// [`ProposalService::create`] for the curation proposal of the **reconfirm step** (RFC 005 part 2 D-3; handoff 005
    /// K11): the note of the change it re-confirms (`change`, the acceptance that put it in the history) is written in
    /// the same step as the proposal, so that the curation proposal's acceptance can record which change it
    /// re-confirmed, and the page can say that one is already proposed. The store refuses a change that did not seal.
    ///
    /// **The proposal is made already held for review** (K26): it is checked as a submission is (the finished text, the
    /// files, the knowledge files, the wiki's hourly limit) *before* anything is written, and then written with its
    /// submission in one store transaction. A refusal leaves nothing behind, and there is no draft to hold the change's
    /// one open place; the record returned is the proposal as it is held, and is not submitted again.
    pub fn create_reconfirm(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        view: Option<&str>,
        edit: ProposalEdit,
        change: &inklkunn_store::AcceptanceId,
    ) -> Result<ProposalRecord> {
        self.create_as(principal, wiki, view, edit, Origin::Reconfirm(change))
    }

    /// The change a proposal re-confirms, when it is the curation proposal of a reconfirm step; `None` for any other.
    /// For whoever may read the proposal.
    pub fn reconfirm_of(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
    ) -> Result<Option<inklkunn_store::AcceptanceId>> {
        let viewer: Option<&AccountId> = self.readable(principal, wiki, id)?;
        self.load(wiki, id, viewer)?;
        Ok(self.proposals.reconfirm_of(id, viewer_of(principal))?)
    }

    /// The proposals made to re-confirm the change `change`, oldest first, as the principal may see them. Anyone who may
    /// read the wiki's proposals may ask.
    pub fn reconfirmations_of(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        change: &inklkunn_store::AcceptanceId,
    ) -> Result<Vec<ProposalId>> {
        self.allow(
            principal,
            Action::Read,
            &Resource::Wiki {
                wiki: wiki.id.clone(),
            },
        )?;
        Ok(self
            .proposals
            .reconfirmations_of(change, viewer_of(principal))?)
    }

    /// Write what an accepted change did to the claims it touched. **Called by the acceptance** (K3 b), under the lease,
    /// between the acceptance record and the signer; it does not authorize, because the acceptance already did.
    pub(crate) fn record_claim_reviews(
        &self,
        acceptance: &inklkunn_store::AcceptanceId,
        outcomes: &[inklkunn_store::ClaimOutcome],
    ) -> std::result::Result<(), StoreError> {
        self.proposals.record_claim_reviews(acceptance, outcomes)
    }

    /// Write that a curation acceptance re-confirms claims of an earlier change. Called by the acceptance only.
    pub(crate) fn record_reconfirmations(
        &self,
        by: &inklkunn_store::AcceptanceId,
        change: &inklkunn_store::AcceptanceId,
        claims: &[String],
    ) -> std::result::Result<(), StoreError> {
        self.proposals.record_reconfirmations(by, change, claims)
    }

    /// What the accepted change `change` did to the claims it touched, and whether each was re-confirmed since. Called
    /// by the acceptance and by the pages that offer the reconfirm step; the claims are the wiki's to read.
    pub fn claim_reviews_of(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        change: &inklkunn_store::AcceptanceId,
    ) -> Result<Vec<inklkunn_store::ClaimReview>> {
        self.allow(
            principal,
            Action::Read,
            &Resource::Wiki {
                wiki: wiki.id.clone(),
            },
        )?;
        let found = self.proposals.claim_reviews_of(change)?;
        // Another wiki's change is not found here.
        if let Some(first) = found.first() {
            self.load(wiki, &first.proposal, viewer_of(principal))?;
        }
        Ok(found)
    }

    /// [`ProposalService::create`], and, for a rollback (the block it undoes) or a reconfirm step (the change it
    /// re-confirms), the note that it is one, written in the same step as the proposal. A reconfirm step's proposal is
    /// made **already held for review** (K26): it is checked as a submission is, before anything is written, and then
    /// written with its submission in one store transaction, so that it is never a draft.
    fn create_as(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        view: Option<&str>,
        edit: ProposalEdit,
        origin: Origin<'_>,
    ) -> Result<ProposalRecord> {
        let author = account_of(principal)?;
        self.allow_pages(principal, wiki, &edit.touched())?;
        let view = view.unwrap_or(&wiki.default_view).to_owned();
        let base = Self::base_of(&edit, || Ok(self.bases.base(wiki, &view)?))?;
        let content = self.content_of(&edit, base);
        let id = ProposalId::new();
        let held = matches!(origin, Origin::Reconfirm(_));
        if held {
            self.allow(
                principal,
                Action::Propose,
                &Self::proposal_resource(wiki, &id),
            )?;
            self.judge_submission(principal, wiki, &edit.summary, &edit.sources, &content)?;
        } else {
            validate(&edit.summary, &edit.sources, &content, Finish::Draft)?;
            self.judge_files(principal, wiki, &content)?;
        }
        let state = next(None, ProposalOp::Create)?;

        let _admission = self
            .admission
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.quotas.check_open(&wiki.id, author)?;
        self.quotas.check_opening_rate(&wiki.id, author)?;
        if held {
            self.check_submission_rate(wiki, author)?;
        }
        let now = (self.clock)();
        let mut record = ProposalRecord {
            id,
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
        match origin {
            Origin::Plain => self
                .proposals
                .create_proposal(&record, &content, &created)?,
            Origin::Rollback(undoes) => self
                .proposals
                .create_rollback_proposal(&record, &content, &created, undoes)?,
            Origin::Reconfirm(change) => {
                let to = next(Some(state), ProposalOp::Submit)?;
                let sent = submission(&record.id, state, to, author, content, now);
                // The proposal as it is held: what a submission leaves of the record.
                record.state = to;
                record.submitted_at = Some(now);
                self.proposals
                    .create_held_reconfirm_proposal(&record, &created, &sent, change)?;
            }
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
        self.judge_files(principal, wiki, &content)?;
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
        let touched = touched_by(&draft);
        self.allow_pages(principal, wiki, &touched)?;
        self.judge_submission(principal, wiki, &p.summary, &p.sources, &draft)?;
        // The wiki-wide hourly limit counts what is *sent*, so it is asked here and not when a draft is opened;
        // in one critical section with the change, so that two sends cannot both find room for the last one.
        let _admission = self
            .admission
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.check_submission_rate(wiki, author)?;
        let now = (self.clock)();
        let change = submission(id, p.state, to, author, draft, now);
        Ok(self.proposals.change_proposal(&change)?)
    }

    /// What a submission is checked on, besides who may touch which page: the text finished, the files fit the wiki,
    /// the knowledge files hold. **Shared** by [`ProposalService::submit`] and the reconfirm step, which is made already
    /// sent, so that the two cannot differ.
    fn judge_submission(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        summary: &str,
        sources: &[String],
        content: &RevisionContent,
    ) -> Result<()> {
        validate(summary, sources, content, Finish::Submit)?;
        self.judge_files(principal, wiki, content)?;
        self.judge_knowledge(wiki, content)
    }

    /// The wiki-wide hourly limit on what is sent for review; the caller holds the admission lock. Reviewers and the
    /// roles above them are not bound by it and are not counted toward it.
    fn check_submission_rate(&self, wiki: &WikiRecord, author: &AccountId) -> Result<()> {
        let at_least_reviewer = |account: &AccountId| {
            self.authorizer
                .role_in(account, &wiki.id)
                .is_some_and(|role| role >= Role::Reviewer)
        };
        self.quotas
            .check_submission_rate(&wiki.id, at_least_reviewer(author), &|sender| {
                !at_least_reviewer(sender)
            })?;
        Ok(())
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
        self.judge_files(principal, wiki, &content)?;
        self.judge_knowledge(wiki, &content)?;
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
        let _admission = self
            .admission
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.quotas.check_comment_rate(&wiki.id, who)?;
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
        self.decide_touching(principal, wiki, id, revision, kind, body, &[])
    }

    /// [`ProposalService::decide`], and, for a *request changes* on a revision that touches claims (RFC 005 part 1 D-1
    /// d), the claims it touches (their ids), written **in the same step as the decision**: the decision lands with its
    /// claims or not at all, so that the participation metrics never miss a request for changes. Only a request for changes
    /// records any.
    #[allow(clippy::too_many_arguments)]
    pub fn decide_touching(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
        id: &ProposalId,
        revision: u32,
        kind: DecisionKind,
        body: &str,
        claims: &[String],
    ) -> Result<DecisionRecord> {
        let who = account_of(principal)?;
        self.allow(
            principal,
            Action::Review,
            &Self::proposal_resource(wiki, id),
        )?;
        let p = self.load(wiki, id, Some(who))?;
        next(Some(p.state), ProposalOp::Decide)?;
        // A review of one's own proposal would be recorded and not counted, which confuses everyone (ruling D9):
        // it is refused, by name. Commenting on one's own proposal stays.
        if p.author == *who {
            return Err(ProposalError::OwnProposal);
        }
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
        match self.proposals.add_decision_touching(&decision, claims) {
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
        let reviews = self.proposals.claim_reviews(&wiki.id)?;
        let touching = self.proposals.touching_decisions(&wiki.id)?;
        Ok(metrics::compute_with_reviews(
            &wiki.id, period, &proposals, &decisions, &reviews, &touching,
        ))
    }
}

/// Where a proposal comes from, when it is made as more than a proposal (written with it, in the same step).
#[derive(Clone, Copy)]
enum Origin<'a> {
    /// An ordinary proposal.
    Plain,
    /// A rollback: the block it undoes.
    Rollback(&'a str),
    /// The curation proposal of the reconfirm step: the change it re-confirms.
    Reconfirm(&'a inklkunn_store::AcceptanceId),
}

#[cfg(test)]
mod tests;
