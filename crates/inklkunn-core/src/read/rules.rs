//! The rules of a wiki, read from the tip of its default view (RFC 006 D-1, D-5; handoff G-T1 part C).
//!
//! **Derived, never authoritative** (CM-7). The two governance files are in the repository; what this builds from them is
//! the [`Rules`] the [`Authorizer`](crate::Authorizer) decides with, kept like the knowledge index is: in memory by the tip,
//! in the derived store by `(wiki, tip)` ([`GovernanceSnapshotStore`]), and rebuilt from the repository when neither has
//! it, so that clearing the derived store changes nothing but the time the next decision takes.
//!
//! ```text
//! the default view's tip (held a few seconds; the write path forgets it after a seal)
//!   ─▶ memory ▶ the derived row ▶ BUILD: one `tree` of `governance/` at the tip's block,
//!                                         one `read` of each of the two files that is there,
//!                                         the walk of the view's history for the block that last changed one
//! ```
//!
//! **Only the default view's tip** is ever read (D-1; threat model C-3e). A proposal in another view that touches
//! `governance/` is refused at acceptance, and a governance file that is in another view is read by nobody.
//!
//! **Fail closed on every shape.** A governance file that is not there has its defaults; anything else at its place (a
//! directory, a link, a device, or a file whose mode says it is none) is a read fault naming the path
//! ([`Refusal::NotAFile`]), as a file that cannot be read is, and never the defaults ([`shape_at`]).
//!
//! **What a read costs.** A decision that the rules cannot change (a read, an anonymous caller, an absent wiki) does not ask
//! for them. For the others: nothing while the tip is held; one `branch list` and one `log` at a new tip; and at a tip whose
//! rules are not stored, one `tree` and, where a file is there, one `cat` of it and the walk for its block. The walk looks
//! at the newest [`WALK_BLOCKS`] blocks of the view, one `show` each, until it finds a block that changed a governance file
//! or meets the tip of the previous snapshot (what a block changed never changes, so each block is asked about once per
//! process). **A tip with no governance file has the defaults and no walk**: that is every wiki that has never written one.
//!
//! **The block.** [`Rules::block`] is the block that last changed one of the two files, and **it does not move while the
//! rules do not**. The rules of a new tip are built by walking back from the tip only until the walk meets a block that
//! changed a governance file or the tip of the wiki's *previous* snapshot (the one this process built or read last, else the
//! newest row of the derived store): in the second case no block since changed a governance file, and when the two read the
//! same the new rules name the block the previous ones did, however many blocks of pages came between and however old the
//! policy is. **Only a cold build can name a later block than the one that set the rules**: when there is no previous
//! snapshot (the derived store was cleared), or its tip is not behind this one within the window (a view put back, a wiki not
//! looked at for more than [`WALK_BLOCKS`] blocks), the walk of the newest [`WALK_BLOCKS`] blocks that finds no block that
//! changed a governance file names the **oldest block it looked at**: no block since it changed one, so the rules are the
//! same *as of* it, and the name is true, and the next tip, inheriting it, names the same one again. A tip with no governance
//! file names no block (the defaults), even for a wiki that once had one and removed it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use inklkunn_prikk::{Entry, Point};
use inklkunn_store::{
    GovernanceFault, GovernanceSnapshot, GovernanceSnapshotStore, WikiId, WikiRecord,
};

use super::{Cause, ReadError, ReadService, Tip};
use crate::authorize::{RulesSource, SourceError};
use crate::governance::{
    Constitution, GovernanceFile, PlatformLimits, Policy, ReadFault, Refusal, Rules,
    read_constitution, read_policy,
};

/// How many of the newest blocks of the default view the walk for the rules' block looks at.
pub const WALK_BLOCKS: u64 = 256;

/// The most blocks whose answer *did this block change a governance file?* is remembered, over all wikis.
const MAX_REMEMBERED_BLOCKS: usize = 20_000;

/// What the service remembers about the rules: the rules at each wiki's last-seen tip, the snapshot built or read last for
/// each wiki (the row a new tip's rules are compared with), the lock of each wiki's build, and the blocks known not to have
/// changed a governance file.
#[derive(Default)]
pub(super) struct RulesState {
    held: Mutex<HashMap<WikiId, (String, Arc<Rules>)>>,
    previous: Mutex<HashMap<WikiId, (String, GovernanceSnapshot)>>,
    builds: Mutex<HashMap<WikiId, Arc<Mutex<()>>>>,
    touching: Mutex<HashMap<(WikiId, String), bool>>,
}

impl RulesState {
    /// Forget the rules held for the tip (a write path calls it after a seal). **The last snapshot is kept**: it is what the
    /// next tip's rules are compared with.
    pub(super) fn forget(&self, wiki: &WikiId) {
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(wiki);
    }

    /// Forget everything derived in memory for the wiki, the last snapshot included (the derived store is cleared with it).
    pub(super) fn clear(&self, wiki: &WikiId) {
        self.forget(wiki);
        self.previous
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(wiki);
    }

    /// The snapshot built or read last for the wiki, with the state it is of.
    fn previous(&self, wiki: &WikiId) -> Option<(String, GovernanceSnapshot)> {
        self.previous
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(wiki)
            .cloned()
    }

    fn remember(&self, wiki: &WikiId, state: &str, snapshot: &GovernanceSnapshot) {
        self.previous
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(wiki.clone(), (state.to_owned(), snapshot.clone()));
    }

    fn held(&self, wiki: &WikiId, tip: &str) -> Option<Arc<Rules>> {
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(wiki)
            .filter(|(held_tip, _)| held_tip == tip)
            .map(|(_, rules)| Arc::clone(rules))
    }

    fn hold(&self, wiki: &WikiId, tip: &str, rules: &Arc<Rules>) {
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(wiki.clone(), (tip.to_owned(), Arc::clone(rules)));
    }

    fn lock_of(&self, wiki: &WikiId) -> Arc<Mutex<()>> {
        let mut builds = self.builds.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(builds.entry(wiki.clone()).or_default())
    }

    fn touching(&self, wiki: &WikiId, block: &str) -> Option<bool> {
        self.touching
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&(wiki.clone(), block.to_owned()))
            .copied()
    }

    fn remember_touching(&self, wiki: &WikiId, block: &str, touches: bool) {
        let mut known = self.touching.lock().unwrap_or_else(PoisonError::into_inner);
        if known.len() >= MAX_REMEMBERED_BLOCKS {
            known.clear();
        }
        known.insert((wiki.clone(), block.to_owned()), touches);
    }
}

impl ReadService {
    /// The same, keeping the derived governance snapshot in `store` (RFC 006 D-5): the rules at a tip are stored by
    /// `(wiki, tip)`, dropped with the rest of the derived rows and built again from the repository. **Without one the rules
    /// are held in memory only** and read from the repository again by a process that starts.
    #[must_use]
    pub fn with_governance_snapshots(mut self, store: Arc<dyn GovernanceSnapshotStore>) -> Self {
        self.governance = Some(store);
        self
    }

    /// The same, reading the rules under the operator's limits (RFC 006 D-3): a policy above one is read as if it were at
    /// it, with a [`Lowered`](crate::governance::Lowered) saying so.
    #[must_use]
    pub fn with_platform_limits(mut self, limits: PlatformLimits) -> Self {
        self.limits = limits;
        self
    }

    /// What the operator allows any wiki.
    pub fn platform_limits(&self) -> &PlatformLimits {
        &self.limits
    }

    /// The rules in force for the wiki: **those of the default view's tip** (RFC 006 D-1), from memory, else from the
    /// derived store, else built from the repository. An error is *could not be read now*, and the decision it was for is
    /// deny.
    ///
    /// The tip is **held for a few seconds** ([`ReadConfig::tip_ttl`](super::ReadConfig)), so this answers what the rules were
    /// a moment ago. That is right for a decision that is advisory or that a later one repeats; what *decides* an act that is
    /// about to be signed reads the rules of the tip it has just read from the repository ([`ReadService::rules_at`]).
    pub fn rules_of(&self, wiki: &WikiRecord) -> Result<Arc<Rules>, ReadError> {
        let tip = self.tip(wiki, None)?;
        self.rules_at(wiki, &tip)
    }

    /// The rules in force at `tip`, **a tip of the wiki's default view that the caller has just read from the repository**,
    /// never the held tip (handoff G-T1 correction C-2): from memory when the rules of this state are held, else from the
    /// derived store, else built from the repository. It is what a decision made under the write lease uses, so that the
    /// rules it judges by are the rules of the state the change is signed on, whatever the tip cache holds and whichever
    /// process sealed last.
    pub fn rules_at(&self, wiki: &WikiRecord, tip: &Tip) -> Result<Arc<Rules>, ReadError> {
        if let Some(rules) = self.rules.held(&wiki.id, &tip.state_id) {
            return Ok(rules);
        }
        // One build per wiki at a time; whoever waited finds the rules already there. Not the index's lock: an index build
        // can take a long while, and a decision must not wait behind it.
        let lock = self.rules.lock_of(&wiki.id);
        let _building = lock.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(rules) = self.rules.held(&wiki.id, &tip.state_id) {
            return Ok(rules);
        }
        if let Some(rules) = self.stored_rules(wiki, tip) {
            self.rules.hold(&wiki.id, &tip.state_id, &rules);
            return Ok(rules);
        }
        let snapshot = self.build_snapshot(wiki, tip)?;
        if let Some(store) = &self.governance
            && store
                .put_governance_snapshot(
                    &wiki.id,
                    &tip.state_id,
                    &snapshot,
                    inklkunn_store::record::now_secs(),
                )
                .is_err()
        {
            self.store_failures
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        let rules = Arc::new(rules_from(&snapshot, &self.limits).ok_or_else(|| {
            ReadError::CouldNotBeReadNow(Cause::Scratch(
                "the governance files just read could not be read again".to_owned(),
            ))
        })?);
        self.rules.remember(&wiki.id, &tip.state_id, &snapshot);
        self.rules.hold(&wiki.id, &tip.state_id, &rules);
        Ok(rules)
    }

    /// The rules the derived store has for the tip. A row that cannot be read back, or read as rules, is a miss: the right
    /// answer to a damaged derived row is to derive it again.
    fn stored_rules(&self, wiki: &WikiRecord, tip: &Tip) -> Option<Arc<Rules>> {
        let store = self.governance.as_ref()?;
        match store.governance_snapshot(&wiki.id, &tip.state_id) {
            Ok(Some(snapshot)) => {
                let rules = rules_from(&snapshot, &self.limits).map(Arc::new)?;
                self.rules.remember(&wiki.id, &tip.state_id, &snapshot);
                Some(rules)
            }
            Ok(None) => None,
            Err(_) => {
                self.store_failures
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                None
            }
        }
    }

    /// Read the two governance files at the tip's block, and find the block that last changed one.
    fn build_snapshot(
        &self,
        wiki: &WikiRecord,
        tip: &Tip,
    ) -> Result<GovernanceSnapshot, ReadError> {
        let repo = self.layout.repo_dir(&wiki.id);
        let point = Point::Block(tip.block_id.clone());
        let listing = self
            .storage
            .tree(&repo, &point, Some(crate::governance::GOVERNANCE_DIR))
            .map_err(|e| self.storage_error(wiki, e))?;
        let mut snapshot = GovernanceSnapshot::default();
        let mut present = false;
        for file in GovernanceFile::ALL {
            let entry = match shape_at(&listing.entries, file) {
                Shape::Absent => continue,
                Shape::NotAFile => {
                    // Something that is not an ordinary file is where the file belongs: read as no file, it would be read as the
                    // defaults, and the rules would say less than the repository does. It is a fault, as an unreadable file is.
                    present = true;
                    let fault = ReadFault::from_stored(file, None, Refusal::NotAFile);
                    snapshot.fault.get_or_insert_with(|| stored_fault(&fault));
                    continue;
                }
                Shape::File(entry) => entry,
            };
            present = true;
            let read = match self.read_governance_file(wiki, &point, file, entry)? {
                Ok(read) => read,
                Err(fault) => {
                    // The first file that cannot be read is the fault (the constitution is looked at first); the other file
                    // is not read as anything while one is broken.
                    snapshot.fault.get_or_insert_with(|| stored_fault(&fault));
                    continue;
                }
            };
            match read {
                Read::Constitution(c) => snapshot.constitution = Some(c.to_yaml()),
                Read::Policy(p) => snapshot.policy = Some(p.to_yaml()),
            }
        }
        if snapshot.fault.is_some() {
            // Nothing half read decides anything: the faulted rules are the defaults and the fault.
            snapshot.constitution = None;
            snapshot.policy = None;
        }
        if present {
            snapshot.policy_block = Some(self.governance_block(wiki, tip, &snapshot)?);
        }
        Ok(snapshot)
    }

    /// The snapshot built for the wiki last, with the state it is of, **other than the one the tip's own state would be**: the
    /// one in memory (what this process built or read last) or, when it has none, the newest the derived store holds. A row
    /// the store cannot answer or read back is none.
    fn previous_snapshot(
        &self,
        wiki: &WikiRecord,
        tip: &Tip,
    ) -> Option<(String, GovernanceSnapshot)> {
        let found = match self.rules.previous(&wiki.id) {
            Some(held) => Some(held),
            None => match self
                .governance
                .as_ref()?
                .latest_governance_snapshot(&wiki.id)
            {
                Ok(row) => row,
                Err(_) => {
                    self.store_failures
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    None
                }
            },
        };
        found.filter(|(state, _)| *state != tip.state_id)
    }

    /// The block that last changed a governance file, for the rules at `tip` that were just read as `snapshot`.
    ///
    /// **It walks back from the tip only until it meets a block that changed a governance file or the tip of the wiki's
    /// previous snapshot** ([`ReadService::previous_snapshot`]); when it meets the latter, no block since changed a governance
    /// file, and when the two read the same (the files and the fault) the answer is the one the previous snapshot already
    /// gave. The name of the block therefore does not move while the rules do not (two decisions under one policy name one
    /// block, however many blocks of pages came between), whatever the policy's age. What cannot be answered so (no previous
    /// snapshot, or the tip it is of is not behind this one in the window: a view put back, or a wiki that was not looked at
    /// for more than the window) is the **cold** walk of the newest [`WALK_BLOCKS`] blocks, which names the oldest block it
    /// looked at when the window ends before it found one: **only a cold build can name a later block than the one that set
    /// the rules**, and it names the same one again at the next tip.
    fn governance_block(
        &self,
        wiki: &WikiRecord,
        tip: &Tip,
        snapshot: &GovernanceSnapshot,
    ) -> Result<String, ReadError> {
        // A previous snapshot of a wiki with no governance names no block, and has nothing to inherit.
        let previous = self
            .previous_snapshot(wiki, tip)
            .filter(|(_, previous)| previous.policy_block.is_some());
        let stop = previous.as_ref().map(|(state, _)| state.as_str());
        let walked = self.walk_for_block(wiki, tip, stop)?;
        if let Some(block) = walked.changed {
            return Ok(block);
        }
        if !walked.met_previous {
            return Ok(walked.oldest);
        }
        match previous {
            Some((_, previous))
                if previous.constitution == snapshot.constitution
                    && previous.policy == snapshot.policy
                    && previous.fault == snapshot.fault =>
            {
                if let Some(block) = previous.policy_block {
                    return Ok(block);
                }
            }
            _ => {}
        }
        // The files differ though no block of the walk changed one (a view put back, a block whose change the history does
        // not list): the previous snapshot says nothing about these rules, and the walk is a cold one.
        let cold = self.walk_for_block(wiki, tip, None)?;
        Ok(cold.changed.unwrap_or(cold.oldest))
    }

    /// One governance file at the block: read, and judged. `Ok(Err(_))` is a file that cannot be read as rules; an `Err` is a
    /// repository that could not be asked.
    fn read_governance_file(
        &self,
        wiki: &WikiRecord,
        point: &Point,
        file: GovernanceFile,
        entry: &Entry,
    ) -> Result<Result<Read, ReadFault>, ReadError> {
        let max = u64::try_from(file.max_bytes()).unwrap_or(u64::MAX);
        if entry.size.is_some_and(|size| size > max) {
            return Ok(Err(ReadFault::from_stored(file, None, Refusal::TooLong)));
        }
        let bytes = self
            .storage
            .read(
                &self.layout.repo_dir(&wiki.id),
                point,
                file.path(),
                max.saturating_add(1),
            )
            .map_err(|e| self.storage_error(wiki, e))?;
        Ok(match file {
            GovernanceFile::Constitution => read_constitution(&bytes).map(Read::Constitution),
            GovernanceFile::Policy => read_policy(&bytes).map(Read::Policy),
        })
    }

    /// Walk back from the tip through the default view's newest [`WALK_BLOCKS`] blocks, one block at a time, until a block that
    /// changed a governance file is found (the tip's own block is looked at first) or, when `stop` names a state, the block
    /// that state is of is met: the blocks before it are not looked at. See [`Walked`].
    fn walk_for_block(
        &self,
        wiki: &WikiRecord,
        tip: &Tip,
        stop: Option<&str>,
    ) -> Result<Walked, ReadError> {
        let repo = self.layout.repo_dir(&wiki.id);
        let history = self
            .storage
            .history(&repo, &Point::View(tip.view.clone()), Some(WALK_BLOCKS))
            .map_err(|e| self.storage_error(wiki, e))?;
        // The view may have moved on since the tip was read: the walk starts at the tip's own block.
        let Some(start) = history
            .blocks
            .iter()
            .position(|b| b.block_id == tip.block_id)
        else {
            return Err(ReadError::CouldNotBeReadNow(Cause::Scratch(
                "the view moved while its governance files were read".to_owned(),
            )));
        };
        let mut walked = Walked {
            changed: None,
            met_previous: false,
            oldest: tip.block_id.clone(),
        };
        for (n, block) in history.blocks.iter().enumerate().skip(start) {
            // The block the previous snapshot is of: what changed in it is what that snapshot already says. (Never the tip's
            // own block: the walk is for a state that has no snapshot yet.)
            if n > start && stop == Some(block.ref_state_id.as_str()) {
                walked.met_previous = true;
                return Ok(walked);
            }
            walked.oldest.clone_from(&block.block_id);
            if self.changed_governance(wiki, &repo, &block.block_id)? {
                walked.changed = Some(block.block_id.clone());
                return Ok(walked);
            }
        }
        Ok(walked)
    }

    /// Did this block change one of the two governance files? A block never changes, so each is asked about once.
    fn changed_governance(
        &self,
        wiki: &WikiRecord,
        repo: &std::path::Path,
        block: &str,
    ) -> Result<bool, ReadError> {
        if let Some(known) = self.rules.touching(&wiki.id, block) {
            return Ok(known);
        }
        let change = self
            .storage
            .change(repo, block)
            .map_err(|e| self.storage_error(wiki, e))?;
        let touches = change
            .patches
            .iter()
            .flat_map(|patch| &patch.operations)
            .flat_map(|op| &op.paths)
            .filter_map(|p| p.path.as_deref())
            .any(|path| GovernanceFile::at_or_below(path).is_some());
        self.rules.remember_touching(&wiki.id, block, touches);
        Ok(touches)
    }
}

/// What the walk for the block that set the rules found.
struct Walked {
    /// The block that changed a governance file, when the walk met one first.
    changed: Option<String>,
    /// The walk met the block of the previous snapshot's tip before any block that changed a governance file: no block since
    /// changed one.
    met_previous: bool,
    /// The oldest block the walk looked at (the tip's own when it looked at no other): what the window names when it ends first.
    oldest: String,
}

/// What a listing of `governance/` has at the place of one of the two files.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Shape<'a> {
    /// Nothing at the place, and nothing below it: the file is not there (its defaults).
    Absent,
    /// An ordinary file.
    File(&'a Entry),
    /// Anything else: an entry of the path that is not an ordinary file (a directory, a link, a device: by its kind or by
    /// the type bits of its mode), or anything below the path (the listing names files, so a directory is told by what is in
    /// it). **Never skipped**: the wiki fails closed on it.
    NotAFile,
}

/// The `S_IFMT` bits of a POSIX mode, and the one an ordinary file has.
const MODE_TYPE_MASK: u32 = 0o170_000;
const MODE_REGULAR: u32 = 0o100_000;

/// What the listing has at the place of `file`. Bounded by the listing, which is read whole already: one pass over it.
pub(super) fn shape_at(entries: &[Entry], file: GovernanceFile) -> Shape<'_> {
    let below = format!("{}/", file.path());
    let mut found = Shape::Absent;
    for entry in entries {
        if entry.path.starts_with(&below) {
            return Shape::NotAFile;
        }
        if entry.path != file.path() {
            continue;
        }
        // A mode that carries no type bits says nothing about the type (the kind decides); one that does must be a file's.
        let typed = entry.mode.map(|mode| mode & MODE_TYPE_MASK);
        if entry.kind != "file" || typed.is_some_and(|t| t != 0 && t != MODE_REGULAR) {
            return Shape::NotAFile;
        }
        found = Shape::File(entry);
    }
    found
}

/// A governance file as it was read.
enum Read {
    Constitution(Constitution),
    Policy(Policy),
}

/// The fault as the derived row keeps it: the file, the line and the rule's stable name.
fn stored_fault(fault: &ReadFault) -> GovernanceFault {
    GovernanceFault {
        file: fault.file().path().to_owned(),
        line: fault.line().and_then(|l| u32::try_from(l).ok()),
        rule: fault.refusal().name().to_owned(),
    }
}

/// The rules a derived row says. `None` when the row cannot be read as rules (a file's text that no longer reads, a fault of
/// a file or a rule this build does not know): the caller derives it again.
fn rules_from(snapshot: &GovernanceSnapshot, limits: &PlatformLimits) -> Option<Rules> {
    if let Some(fault) = &snapshot.fault {
        let file = GovernanceFile::ALL
            .into_iter()
            .find(|f| f.path() == fault.file)?;
        let refusal = Refusal::from_name(&fault.rule)?;
        let line = fault.line.and_then(|l| usize::try_from(l).ok());
        return Some(Rules::faulted(
            snapshot.policy_block.clone(),
            ReadFault::from_stored(file, line, refusal),
        ));
    }
    if snapshot.policy_block.is_none()
        && snapshot.policy.is_none()
        && snapshot.constitution.is_none()
    {
        return Some(Rules::defaults());
    }
    let policy = match &snapshot.policy {
        Some(text) => read_policy(text.as_bytes()).ok()?,
        None => Policy::default(),
    };
    let constitution = match &snapshot.constitution {
        Some(text) => read_constitution(text.as_bytes()).ok()?,
        None => Constitution::default(),
    };
    Some(Rules::read(
        snapshot.policy_block.clone(),
        policy,
        constitution,
        limits,
    ))
}

impl RulesSource for ReadService {
    fn rules(&self, wiki: &WikiId) -> Result<Arc<Rules>, SourceError> {
        let record = self
            .wikis
            .get(wiki)
            .map_err(|e| SourceError(e.to_string()))?;
        self.rules_of(&record)
            .map_err(|e| SourceError(e.to_string()))
    }
}
