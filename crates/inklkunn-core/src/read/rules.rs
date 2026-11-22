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
//! **What a read costs.** A decision that the rules cannot change (a read, an anonymous caller, an absent wiki) does not ask
//! for them. For the others: nothing while the tip is held; one `branch list` and one `log` at a new tip; and at a tip whose
//! rules are not stored, one `tree` and, where a file is there, one `cat` of it and the walk for its block. The walk looks
//! at the newest [`WALK_BLOCKS`] blocks of the view, one `show` each until a block that changed a governance file is found
//! (what a block changed never changes, so each block is asked about once per process). **A tip with no governance file has
//! the defaults and no walk**: that is every wiki that has never written one.
//!
//! **The block.** [`Rules::block`] is the block that last changed one of the two files. When the walk's window ends before
//! it found one (a policy older than the window), the block is the **oldest block the walk looked at**: no block since it
//! changed a governance file, so the rules are the same *as of* it, and the name stays true and stable. A tip with no
//! governance file names no block (the defaults), even for a wiki that once had one and removed it.

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

/// What the service remembers about the rules: the rules at each wiki's last-seen tip, the lock of each wiki's build, and the
/// blocks known not to have changed a governance file.
#[derive(Default)]
pub(super) struct RulesState {
    held: Mutex<HashMap<WikiId, (String, Arc<Rules>)>>,
    builds: Mutex<HashMap<WikiId, Arc<Mutex<()>>>>,
    touching: Mutex<HashMap<(WikiId, String), bool>>,
}

impl RulesState {
    pub(super) fn forget(&self, wiki: &WikiId) {
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(wiki);
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
    pub fn rules_of(&self, wiki: &WikiRecord) -> Result<Arc<Rules>, ReadError> {
        let tip = self.tip(wiki, None)?;
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
        if let Some(rules) = self.stored_rules(wiki, &tip) {
            self.rules.hold(&wiki.id, &tip.state_id, &rules);
            return Ok(rules);
        }
        let snapshot = self.build_snapshot(wiki, &tip)?;
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
        self.rules.hold(&wiki.id, &tip.state_id, &rules);
        Ok(rules)
    }

    /// The rules the derived store has for the tip. A row that cannot be read back, or read as rules, is a miss: the right
    /// answer to a damaged derived row is to derive it again.
    fn stored_rules(&self, wiki: &WikiRecord, tip: &Tip) -> Option<Arc<Rules>> {
        let store = self.governance.as_ref()?;
        match store.governance_snapshot(&wiki.id, &tip.state_id) {
            Ok(Some(snapshot)) => rules_from(&snapshot, &self.limits).map(Arc::new),
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
            let Some(entry) = listing
                .entries
                .iter()
                .find(|e| e.kind == "file" && e.path == file.path())
            else {
                continue;
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
            snapshot.policy_block = Some(self.last_governance_block(wiki, tip)?);
        }
        Ok(snapshot)
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

    /// The block that last changed a governance file, by the walk of the default view's newest [`WALK_BLOCKS`] blocks (see the
    /// module's account of what is named when the window ends first).
    fn last_governance_block(&self, wiki: &WikiRecord, tip: &Tip) -> Result<String, ReadError> {
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
        let mut oldest = tip.block_id.clone();
        for block in history.blocks.iter().skip(start) {
            oldest.clone_from(&block.block_id);
            if self.changed_governance(wiki, &repo, &block.block_id)? {
                return Ok(block.block_id.clone());
            }
        }
        Ok(oldest)
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
            .any(|path| GovernanceFile::of_path(path).is_some());
        self.rules.remember_touching(&wiki.id, block, touches);
        Ok(touches)
    }
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
