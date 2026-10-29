//! `Accept`: the one way into a wiki's history (RFC 002 D-4; internal design SGN-3; threat model C-1a,
//! C-1b, INV-1). One operation brings an **accepted** proposal into the repository; a second creates a
//! view. Neither takes a key, a seed path or a signing primitive, and neither can be asked to sign anything
//! that is not bound to a recorded act.
//!
//! The trait lives here, in the crate that implements it and enforces it, and `inklkunn-core` re-exports it
//! (a lower crate may not depend on core; the internal design calls it "the signer's trait"). A caller holds
//! an [`AcceptanceToken`], an id, and nothing else: the signer **reads the record the id names itself**
//! through an [`AcceptanceLedger`], so a caller cannot invent the facts a seal is bound to. In RFC 004 the
//! ledger is the service store's acceptance records; here it is a trait, and tests give it a double.

use inklkunn_store::{ProposalId, WikiId};

use crate::message::{Actor, BlockId, ImportSource, Kind, Message};

/// The id of an acceptance record in state *pending seal* (RFC 004's records; the guard is RFC 002 D-4's).
/// Holding one proves nothing by itself: the signer re-reads the record the id names and refuses unless it
/// is pending seal and the message matches.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AcceptanceToken(String);

impl AcceptanceToken {
    /// Name an acceptance record.
    pub fn new(record_id: impl Into<String>) -> Self {
        AcceptanceToken(record_id.into())
    }

    /// The record id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The id of a recorded view-creation act (RFC 002 D-4, SGN-4): `create_view` is bound to one as `accept`
/// is bound to an acceptance.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ViewCreationAct(String);

impl ViewCreationAct {
    /// Name a view-creation act.
    pub fn new(record_id: impl Into<String>) -> Self {
        ViewCreationAct(record_id.into())
    }

    /// The record id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One file of a prepared change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedFile {
    /// Repository-relative path under `pages/`, `attachments/`, `knowledge/` or `governance/`, or the
    /// manifest `wiki-bundle.yaml`. **The signer validates it** before anything is written (ASCII, prikk's
    /// path rules, the allowed roots); this type carries, it does not guarantee.
    pub path: std::path::PathBuf,
    /// The bytes to write; `None` removes the file.
    pub content: Option<Vec<u8>>,
}

/// The SHA-256 of a prepared change, in a canonical form (corrections handoff C5): what an acceptance record
/// says was accepted, and what the signer recomputes over what it is about to write. The message binds the
/// proposal and the people; this binds the **files**, so a change altered between the record and the call is
/// refused before anything is written.
///
/// The encoding is `"inklkunn-change/1\0"`, then the files **sorted by path** (a path may appear once), each as
/// a tag (`W` write or `D` delete), the length and bytes of the path, and for a write the length and bytes of the
/// content; then the renames **in the order given** (the order is meaning: a chain `a → b`, `b → c` is not
/// `b → c`, `a → b`), each as the length and bytes of both paths. Lengths are 8 bytes, big endian. The message
/// is not part of it (the record binds that separately).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChangeDigest([u8; 32]);

impl ChangeDigest {
    /// A digest from its 32 bytes (what a record stores).
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        ChangeDigest(bytes)
    }

    /// The 32 bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lowercase hex, 64 characters.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

impl std::fmt::Debug for ChangeDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ChangeDigest({})", self.to_hex())
    }
}

impl std::fmt::Display for ChangeDigest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// A change ready to be written: files, declared renames, and the CT-03 message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedChange {
    /// Files written or removed. A renamed page's *new* path is listed here when its content changes too.
    pub files: Vec<PreparedFile>,
    /// `(from, to)` renames, declared with `prikk mv` so identity survives.
    pub renames: Vec<(std::path::PathBuf, std::path::PathBuf)>,
    /// The CT-03 message text, exactly as [`Message::to_text`] writes it.
    pub message: String,
}

impl PreparedChange {
    /// The digest of this change's files and renames (see [`ChangeDigest`]).
    pub fn digest(&self) -> ChangeDigest {
        use sha2::{Digest, Sha256};
        use std::os::unix::ffi::OsStrExt;
        fn put(h: &mut Sha256, bytes: &[u8]) {
            h.update((bytes.len() as u64).to_be_bytes());
            h.update(bytes);
        }
        let mut h = Sha256::new();
        h.update(b"inklkunn-change/1\0");
        let mut files: Vec<&PreparedFile> = self.files.iter().collect();
        files.sort_by(|a, b| {
            a.path
                .as_os_str()
                .as_bytes()
                .cmp(b.path.as_os_str().as_bytes())
        });
        h.update((files.len() as u64).to_be_bytes());
        for f in files {
            match &f.content {
                Some(bytes) => {
                    h.update(b"W");
                    put(&mut h, f.path.as_os_str().as_bytes());
                    put(&mut h, bytes);
                }
                None => {
                    h.update(b"D");
                    put(&mut h, f.path.as_os_str().as_bytes());
                }
            }
        }
        h.update((self.renames.len() as u64).to_be_bytes());
        for (from, to) in &self.renames {
            put(&mut h, from.as_os_str().as_bytes());
            put(&mut h, to.as_os_str().as_bytes());
        }
        ChangeDigest(h.finalize().into())
    }
}

/// What a sealed change leaves behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SealedChange {
    /// The sealed block.
    pub block_id: String,
    /// The patches it seals.
    pub patch_ids: Vec<String>,
}

/// What could not be confirmed after a change was sealed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unconfirmed {
    /// The sealed block could not be read back: its ids are not known.
    BlockNotRead,
    /// The newest block is not the change that was written: something else sealed in between. Which block is
    /// this acceptance's is not known.
    NotOurs,
    /// A read of the sequence (the block read back, or `verify`) was stopped by its memory ceiling
    /// (threat model C-9a). This is **not** a failed verification and says nothing against the change: the
    /// repository could not be checked for want of memory.
    CeilingExceeded,
    /// `verify` could not run to a verdict.
    VerifyCouldNotRun,
    /// `verify` ran and the repository does not verify. With prikk 0.54.1 this is also what **every** change
    /// reports once a maintainer key has been retired (ledger PK-34): the change is sealed and the older
    /// history cannot be verified.
    VerifyFailed,
}

/// Why an acceptance was refused or did not complete. **Each variant says what state the repository is in**,
/// so a caller never has to read text to decide between "nothing happened", "a patch is queued" and "the
/// change is sealed".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceptError {
    /// The token names no acceptance record in state *pending seal* for this wiki and view (or no recorded
    /// view-creation act). Nothing was done.
    NotPending,
    /// The message does not name the token's proposal and acceptor (RFC 002 D-5), or is not a message.
    /// Nothing was done.
    MessageMismatch,
    /// The prepared files are not the ones the acceptance record says were accepted (their digest differs).
    /// Nothing was done.
    ChangeMismatch,
    /// Refused before anything was committed, with the worktree as it was found (it may be on another view,
    /// if a switch had been made); the text is for operators. Nothing was committed or sealed.
    Refused(String),
    /// Another act is writing this wiki (another thread, or another process such as the command line beside a
    /// running server): **nothing was done.** Retrying later is safe; the record stays pending.
    Busy {
        /// The process that holds the wiki's write lock, when it could be read.
        holder: Option<u32>,
    },
    /// The call could not run, and nothing was committed or sealed; the worktree is as it was found. The text
    /// is for operators.
    Unavailable(String),
    /// A call was stopped by its memory ceiling (threat model C-9a) **before anything was committed**; the
    /// worktree is as it was found. Not a refusal: nothing is wrong with the change, and a retry after the
    /// ceiling is raised (or the repository is smaller) can succeed.
    CeilingExceeded {
        /// The prikk verb that was stopped.
        verb: String,
        /// The ceiling it ran under, in bytes.
        bytes: u64,
    },
    /// A failure before `commit` could not be undone: **the worktree holds uncommitted changes** that a
    /// recovery must restore before anything else is accepted. Nothing was committed or sealed.
    WorktreeDirty(String),
    /// `commit` succeeded and `seal` did not: **one patch is queued, nothing is sealed.** Sealing it completes
    /// the acceptance (RFC 004's recovery); retrying the acceptance would commit the change a second time.
    Queued {
        /// The step that failed.
        step: &'static str,
        /// What failed, for operators.
        detail: String,
        /// The memory ceiling that stopped the step, in bytes, when that is what happened (C-9a): the seal was
        /// not refused, it ran out of memory, and the same seal under a higher ceiling completes the change.
        ceiling: Option<u64>,
    },
    /// **The change is sealed**, but something afterwards could not be confirmed. `sealed` carries the block
    /// when it was read back. A retry of the acceptance would seal it again: the caller records the block (or
    /// looks for it) instead.
    SealedUnconfirmed {
        /// What was sealed, when it is known.
        sealed: Option<SealedChange>,
        /// What could not be confirmed.
        why: Unconfirmed,
        /// For operators.
        detail: String,
    },
    /// **The call completed**, and `sealed` carries its ids when it was an acceptance, **but the temporary
    /// directory of decrypted seeds could not be removed.** The operator removes the directories named
    /// `inklkunn-seeds-*` under the seed directory.
    SeedsNotRemoved {
        /// What the call sealed, when it was an acceptance.
        sealed: Option<SealedChange>,
    },
}

impl std::fmt::Display for AcceptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcceptError::NotPending => write!(f, "the acceptance is not pending seal"),
            AcceptError::MessageMismatch => write!(
                f,
                "the message does not name the acceptance's proposal and acceptor"
            ),
            AcceptError::ChangeMismatch => write!(
                f,
                "the prepared files are not the ones that were accepted; nothing was done"
            ),
            AcceptError::Refused(m) => write!(f, "refused, nothing committed: {m}"),
            AcceptError::Busy { holder: Some(pid) } => write!(
                f,
                "the wiki is being written by process {pid}; nothing was done, try again when it is finished"
            ),
            AcceptError::Busy { holder: None } => write!(
                f,
                "the wiki is being written by another process; nothing was done, try again when it is finished"
            ),
            AcceptError::Unavailable(m) => write!(f, "could not run, nothing committed: {m}"),
            AcceptError::CeilingExceeded { verb, bytes } => write!(
                f,
                "`{verb}` was stopped by its memory ceiling ({bytes} bytes) before anything was committed; the worktree is as it was"
            ),
            AcceptError::WorktreeDirty(m) => write!(
                f,
                "failed before the commit and the worktree could not be put back; recovery must restore it: {m}"
            ),
            AcceptError::Queued {
                step,
                detail,
                ceiling,
            } => write!(
                f,
                "committed, `{step}` {}: one patch is queued and nothing is sealed; recovery seals it: {detail}",
                if ceiling.is_some() {
                    "was stopped by its memory ceiling"
                } else {
                    "failed"
                }
            ),
            AcceptError::SealedUnconfirmed { why, detail, .. } => write!(
                f,
                "the change is sealed but could not be confirmed ({}); do not accept it again: {detail}",
                match why {
                    Unconfirmed::BlockNotRead => "the block could not be read back",
                    Unconfirmed::NotOurs => "the newest block is not the change that was written",
                    Unconfirmed::CeilingExceeded => {
                        "a check was stopped by its memory ceiling; the change is not known to be bad"
                    }
                    Unconfirmed::VerifyCouldNotRun => "verification could not run",
                    Unconfirmed::VerifyFailed => "the repository did not verify afterwards",
                }
            ),
            AcceptError::SeedsNotRemoved { sealed } => write!(
                f,
                "the call completed{} but a temporary directory of decrypted seeds could not be removed; \
                 remove the directories named `inklkunn-seeds-*` under the seed directory",
                if sealed.is_some() {
                    " (the change is sealed)"
                } else {
                    ""
                }
            ),
        }
    }
}

impl std::error::Error for AcceptError {}

/// The one way into the repository's history.
pub trait Accept: Send + Sync {
    /// Bring the worktree to `view`, apply `change`, commit under the wiki's author key, seal under its
    /// maintainer key, verify, and return the ids. Refuses unless `token` names an acceptance **pending
    /// seal for this wiki and view** and the message names its proposal and acceptor, **before anything is
    /// written**.
    fn accept(
        &self,
        wiki: &WikiId,
        view: &str,
        token: &AcceptanceToken,
        change: &PreparedChange,
    ) -> Result<SealedChange, AcceptError>;

    /// Create view `name` (`heads/<name>`) from the published ref `from`: a maintainer-signed publication
    /// bound to the recorded act `act`. Refuses unless the act is recorded and names this wiki, view and ref.
    fn create_view(
        &self,
        wiki: &WikiId,
        name: &str,
        from: &str,
        act: &ViewCreationAct,
    ) -> Result<(), AcceptError>;
}

/// The facts an acceptance record binds a seal to: everything in the message except its summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedMessage {
    /// The wiki the acceptance is for.
    pub wiki: WikiId,
    /// The view (`heads/<name>`) it accepts into.
    pub view: String,
    /// The kind of change.
    pub kind: Kind,
    /// The proposal, for a proposal or a rollback.
    pub proposal: Option<ProposalId>,
    /// Who proposed.
    pub proposed_by: Actor,
    /// Who accepted.
    pub accepted_by: Actor,
    /// The block the proposal was made against, for a proposal or a rollback.
    pub base: Option<BlockId>,
    /// Where an import came from.
    pub imported_from: Option<ImportSource>,
    /// The block the view's tip must still be when the change is written (an import is planned against the tip
    /// it saw; a message of kind `import` carries no `Base`, so the pin lives here, in the record). The signer
    /// reads the tip under the wiki's write lock, before it decrypts a seed or writes a byte, and refuses if
    /// the view has moved. `None` pins nothing (a proposal's `base` is the merge's business, not the signer's).
    pub pinned_tip: Option<BlockId>,
    /// The digest of the files that were accepted (RFC 004 D-4 records "the merged content's digest" on the
    /// acceptance record). The signer recomputes it over the change it is handed and refuses a mismatch
    /// before it writes anything.
    pub change_digest: ChangeDigest,
}

impl ExpectedMessage {
    /// Does `message` say exactly what the record says (the summary is the proposer's words and is not
    /// part of the binding)?
    pub fn matches(&self, message: &Message) -> bool {
        message.kind() == self.kind
            && message.proposal_id() == self.proposal.as_ref()
            && message.proposed_by() == &self.proposed_by
            && message.accepted_by() == &self.accepted_by
            && message.base() == self.base.as_ref()
            && message.imported_from() == self.imported_from.as_ref()
    }
}

/// What a recorded view-creation act binds a branch creation to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewCreationFacts {
    /// The wiki.
    pub wiki: WikiId,
    /// The new view, `heads/<name>`.
    pub name: String,
    /// The ref it starts from.
    pub from: String,
}

/// The ledger could not answer. The text is for operators.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerError(pub String);

impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the acceptance ledger could not answer: {}", self.0)
    }
}

impl std::error::Error for LedgerError {}

/// The records a seal is bound to (RFC 002 D-4; threat model C-1b). The signer reads them itself; a caller
/// only names them.
pub trait AcceptanceLedger: Send + Sync {
    /// The facts of the acceptance record `token` names, **only if it is in state *pending seal* with an
    /// open intent**; `None` for any other state or an unknown id.
    fn pending(&self, token: &AcceptanceToken) -> Result<Option<ExpectedMessage>, LedgerError>;
    /// The facts of the recorded view-creation act `act` names; `None` for an unknown id.
    fn view_creation(
        &self,
        act: &ViewCreationAct,
    ) -> Result<Option<ViewCreationFacts>, LedgerError>;
}

#[cfg(test)]
mod tests;
