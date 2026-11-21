//! `Recover` for the real signer (RFC 004 D-7; handoff 004 T5): the four operations, each behind the wiki's lent write
//! lease and a recorded act that names it. See [`crate::recover`] for the discipline.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use inklkunn_prikk::storage::{HistoryBlock, Operation, PatchMessage, QueuedPatch};
use inklkunn_store::WikiId;

use super::{Signer, is_view_ref};
use crate::accept::{
    AcceptanceToken, ChangeDigest, ExpectedMessage, PreparedChange, PreparedFile, ViewCreationAct,
};
use crate::locks::{self, WriteGuard};
use crate::message::Message;
use crate::recover::text::{ACCEPTANCE_LOCK_KINDS, Discarded};
use crate::recover::{Recover, RecoverError, RecoveryOp, RecoveryToken, RetirementExcuse};
use crate::worktree::{self, Applied};
use crate::writes::{SignerError, parse_status_text};

/// The most bytes of one file recovery reads back out of the history to restore a worktree, and out of a worktree to
/// check a queued patch's files: the bound of one acceptance.
const MAX_FILE_BYTES: u64 = worktree::MAX_TOTAL_BYTES;
/// The most renames whose order is tried against a recorded digest. prikk lists a patch's renames sorted, not in the
/// order they were declared in, and the digest binds the declared order; more than this and the order cannot be
/// proven, so the patch is not sealed by recovery.
const MAX_RENAMES_TRIED: usize = 7;

mod discard;

fn mismatch<T>(why: &str) -> Result<T, RecoverError> {
    Err(RecoverError::DoesNotMatch(why.to_owned()))
}

impl Signer {
    /// The guard every recovery operation starts with: the lease is lent to this thread (nothing is taken: a
    /// recovery that waited for a lock could be waiting on its own lease), and the recorded act `act` is the wiki's,
    /// open, and names `op`. Only then does anything else happen.
    fn recovering(
        &self,
        wiki: &WikiId,
        act: &RecoveryToken,
        op: &RecoveryOp,
    ) -> Result<WriteGuard<'_>, RecoverError> {
        if !locks::is_lent(&self.locks, wiki) {
            return Err(RecoverError::NoLease);
        }
        let held = self.write_lock(wiki).map_err(RecoverError::from)?;
        let facts = self
            .ledger
            .recovery(act)
            .map_err(|e| RecoverError::Unavailable(e.to_string()))?
            .ok_or(RecoverError::NotRecorded)?;
        if facts.wiki != *wiki || !facts.allows(op) {
            return Err(RecoverError::NotRecorded);
        }
        Ok(held)
    }
}

impl Signer {
    /// The newest block of `view` is the change the record `expected` describes: it holds exactly one patch, whose
    /// message is the record's, and its parent is the block the change was made on, for every acceptance (Q7, W8).
    /// Returns the block and its one patch.
    fn published_is_the_acts(
        &self,
        repo: &Path,
        view: &str,
        expected: &ExpectedMessage,
    ) -> Result<(HistoryBlock, PatchMessage), RecoverError> {
        let w = &self.writes;
        let Some(tip) = w.tip(repo, view)? else {
            return mismatch("the view has no block");
        };
        let [published] = tip.patch_messages.as_slice() else {
            return mismatch("the newest block of the view does not hold exactly one patch");
        };
        let published = published.clone();
        let message = Message::parse(&published.message).map_err(|_| {
            RecoverError::DoesNotMatch(
                "the published patch's message is not a patch message".to_owned(),
            )
        })?;
        if !expected.matches(&message) {
            return mismatch("the published patch's message is not the one the record describes");
        }
        let pinned = &expected.pinned_tip;
        let blocks = w.view_blocks(repo, view, 2)?;
        if blocks.get(1).map(|(block, _)| block.as_str()) != Some(pinned.as_str()) {
            return mismatch(
                "the published block was not made on the block the change was made on",
            );
        }
        Ok((tip, published))
    }

    /// The files of the newest block `tip`, read from the block (the worktree cannot be asked while a publication is
    /// unfinished), are the ones that were accepted. Returns their size, for the call that follows.
    fn published_files_are_the_acts(
        &self,
        repo: &Path,
        view: &str,
        tip: &HistoryBlock,
        published: &PatchMessage,
        expected: &ExpectedMessage,
    ) -> Result<u64, RecoverError> {
        let w = &self.writes;
        let change = w.change(repo, &tip.block_id)?;
        let [patch] = change.patches.as_slice() else {
            return mismatch("the newest block of the view does not hold exactly one patch");
        };
        if patch.patch_id != published.patch_id {
            return mismatch("the newest block's patch is not the one its message belongs to");
        }
        let at_the_tip = |rel: &str| {
            w.cat(repo, view, rel, MAX_FILE_BYTES)
                .map_err(RecoverError::from)
        };
        let Some(change_bytes) =
            change_matches(&patch.operations, &at_the_tip, &expected.change_digest)?
        else {
            return mismatch("the published block's files are not the ones that were accepted");
        };
        Ok(change_bytes)
    }
}

/// The bytes of a file of the worktree, if it is a regular file (a link is refused: a patch's files are files).
fn read_worktree(repo: &Path, rel: &str) -> Result<Vec<u8>, RecoverError> {
    let path = repo.join(rel);
    let meta = std::fs::symlink_metadata(&path).map_err(|e| {
        RecoverError::Unavailable(format!(
            "a file of the worktree could not be read ({})",
            e.kind()
        ))
    })?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return mismatch(
            "a file of the queued patch is not a regular file of a size one acceptance may carry",
        );
    }
    std::fs::read(&path).map_err(|e| {
        RecoverError::Unavailable(format!(
            "a file of the worktree could not be read ({})",
            e.kind()
        ))
    })
}

/// The files and renames of a patch, read from its operations and `read` (the worktree for a queued patch, which,
/// once the patch is committed, is exactly the patch's result; the view's tip for a patch already published), and
/// whether they are the files `expected` binds. An operation this build does not know is a patch it cannot vouch for.
fn change_matches(
    operations: &[Operation],
    read: &dyn Fn(&str) -> Result<Vec<u8>, RecoverError>,
    expected: &ChangeDigest,
) -> Result<Option<u64>, RecoverError> {
    let mut files: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
    let mut renames: Vec<(String, String)> = Vec::new();
    let path_of = |p: &str| {
        worktree::repo_path(Path::new(p)).map_err(|why| RecoverError::DoesNotMatch(why.to_owned()))
    };
    for op in operations {
        let paths: Vec<&str> = op.paths.iter().filter_map(|p| p.path.as_deref()).collect();
        if paths.len() != op.paths.len() {
            return mismatch("an operation of the queued patch names a node that has no path");
        }
        match (op.kind.as_str(), paths.as_slice()) {
            ("delete-node", [p]) => {
                files.insert(path_of(p)?, None);
            }
            ("create-file" | "edit-text", [p]) => {
                let rel = path_of(p)?;
                let bytes = read(&rel)?;
                files.insert(rel, Some(bytes));
            }
            ("rename-path", [from, to]) => renames.push((path_of(from)?, path_of(to)?)),
            _ => return mismatch("the queued patch holds an operation this build does not know"),
        }
    }
    let bytes: u64 = files
        .values()
        .flatten()
        .map(|b| u64::try_from(b.len()).unwrap_or(u64::MAX))
        .fold(0, u64::saturating_add);
    let prepared = |renames: &[(String, String)]| PreparedChange {
        files: files
            .iter()
            .map(|(path, content)| PreparedFile {
                path: PathBuf::from(path),
                content: content.clone(),
            })
            .collect(),
        renames: renames
            .iter()
            .map(|(a, b)| (PathBuf::from(a), PathBuf::from(b)))
            .collect(),
        message: String::new(),
    };
    if renames.len() > MAX_RENAMES_TRIED {
        return mismatch("the queued patch has more renames than their order can be proven for");
    }
    // The digest binds the order the renames were declared in, and prikk lists them sorted: try every order.
    let mut found = false;
    permute(&mut renames, 0, &mut |order| {
        if !found && prepared(order).digest() == *expected {
            found = true;
        }
    });
    Ok(found.then_some(bytes))
}

/// [`change_matches`] for a queued patch: its files are read from the worktree.
fn queued_change_matches(
    repo: &Path,
    patch: &QueuedPatch,
    expected: &ChangeDigest,
) -> Result<Option<u64>, RecoverError> {
    change_matches(&patch.operations, &|rel| read_worktree(repo, rel), expected)
}

/// Call `f` with every ordering of `items[from..]` (Heap's method, in place).
fn permute<T>(items: &mut [T], from: usize, f: &mut impl FnMut(&[T])) {
    if from + 1 >= items.len() {
        f(items);
        return;
    }
    for i in from..items.len() {
        items.swap(from, i);
        permute(items, from + 1, f);
        items.swap(from, i);
    }
}

impl Recover for Signer {
    fn seal_queued(
        &self,
        wiki: &WikiId,
        view: &str,
        act: &RecoveryToken,
        acceptance: &AcceptanceToken,
    ) -> Result<(), RecoverError> {
        if !is_view_ref(view) {
            return Err(RecoverError::Refused("a view is `heads/<name>`".to_owned()));
        }
        let op = RecoveryOp::SealQueued {
            acceptance: acceptance.as_str().to_owned(),
            view: view.to_owned(),
        };
        let _held = self.recovering(wiki, act, &op)?;
        // Sealing makes a block: while a key retirement is unfinished nothing new is sealed (W1).
        match self.seal_pause(wiki) {
            Ok(None) => {}
            Ok(Some(sentence)) => return Err(RecoverError::RetirementUnfinished(sentence)),
            Err(why) => return Err(RecoverError::Unavailable(why)),
        }
        // The record the change was accepted under: pending, the wiki's, on this view.
        let expected = self
            .ledger
            .pending(acceptance)
            .map_err(|e| RecoverError::Unavailable(e.to_string()))?
            .ok_or(RecoverError::NotRecorded)?;
        if expected.wiki != *wiki || expected.view != view {
            return Err(RecoverError::NotRecorded);
        }
        let repo = self.root.repo_dir(wiki);
        let w = &self.writes;
        // What the repository holds: one queued patch, for this view, nothing torn, nothing interrupted.
        let status = w.status_full(&repo)?;
        if status.trailing_partial_wal_bytes != 0
            || status.provisional_worktree.is_some()
            || status.interrupted_materialization.is_some()
        {
            return mismatch(
                "the repository has a torn log or an interrupted state besides the queue",
            );
        }
        let [patch] = status.queue.patches.as_slice() else {
            return mismatch("the queue does not hold exactly one patch");
        };
        if status.queue.target_ref.as_deref() != Some(view) {
            return mismatch("the queued patch is for another view");
        }
        // It is the change the record describes: the message, the base, the files.
        let message = Message::parse(&patch.message).map_err(|_| {
            RecoverError::DoesNotMatch(
                "the queued patch's message is not a patch message".to_owned(),
            )
        })?;
        if !expected.matches(&message) {
            return mismatch("the queued patch's message is not the one the record describes");
        }
        // The base is enforced for every acceptance (Q7, W8).
        let pinned = &expected.pinned_tip;
        let tip = w.tip(&repo, view)?.map(|b| b.block_id);
        if tip.as_deref() != Some(pinned.as_str()) {
            return mismatch("the view is not at the block the change was made on");
        }
        if !w.worktree_clean(&repo)? {
            return mismatch("the worktree is not the queued patch's result");
        }
        let Some(change_bytes) = queued_change_matches(&repo, patch, &expected.change_digest)?
        else {
            return mismatch("the queued patch's files are not the ones that were accepted");
        };
        // Seal alone: the patch is already committed, and committing again would make it twice.
        let seeds = self.seeds(wiki);
        let sealed = w.seal(&repo, &seeds, view, change_bytes);
        match (sealed, seeds.left_over()) {
            (Ok(()), false) => Ok(()),
            (Ok(()), true) => Err(RecoverError::SeedsNotRemoved),
            (Err(e), _) => Err(e.into()),
        }
    }

    fn verify_after_retirement(
        &self,
        wiki: &WikiId,
    ) -> Result<Option<RetirementExcuse>, RecoverError> {
        Ok(self.retirement_explains(&self.root.repo_dir(wiki), wiki)?)
    }

    fn verify_beside_cleanup(
        &self,
        wiki: &WikiId,
    ) -> Result<Option<RetirementExcuse>, RecoverError> {
        Ok(self.retirement_explains_beside_cleanup(&self.root.repo_dir(wiki), wiki)?)
    }

    fn unfinished_retirement(&self, wiki: &WikiId) -> Result<Option<String>, RecoverError> {
        Ok(self.unfinished_retirement_note(wiki))
    }

    fn complete_publication(
        &self,
        wiki: &WikiId,
        view: &str,
        act: &RecoveryToken,
        acceptance: &AcceptanceToken,
    ) -> Result<(), RecoverError> {
        if !is_view_ref(view) {
            return Err(RecoverError::Refused("a view is `heads/<name>`".to_owned()));
        }
        let op = RecoveryOp::CompletePublication {
            acceptance: acceptance.as_str().to_owned(),
            view: view.to_owned(),
        };
        let _held = self.recovering(wiki, act, &op)?;
        let expected = self
            .ledger
            .pending(acceptance)
            .map_err(|e| RecoverError::Unavailable(e.to_string()))?
            .ok_or(RecoverError::NotRecorded)?;
        if expected.wiki != *wiki || expected.view != view {
            return Err(RecoverError::NotRecorded);
        }
        let repo = self.root.repo_dir(wiki);
        let w = &self.writes;
        // 1. prikk says exactly this: one error, a publication whose cleanup is pending, and nothing else. As the plan
        //    does (P13, Q6), the publication-trust findings a retirement recorded are not "something else": the
        //    signer's own question (`verify` fails by publication trust alone, all of it recorded) says which.
        let doctor = w.doctor(&repo)?;
        // While the cleanup is pending `verify` fails by the ref publication as well, so the plain question says no;
        // the question beside a cleanup (W7) is asked after it, and only a yes from either takes findings out.
        let excuse = match self.retirement_explains(&repo, wiki) {
            Ok(Some(excuse)) => Ok(Some(excuse)),
            _ => self.retirement_explains_beside_cleanup(&repo, wiki),
        };
        let doctor = match excuse {
            Ok(Some(excuse)) => {
                doctor
                    .without_excused_publication_findings(|kind, id| excuse.names(kind, id))
                    .0
            }
            _ => doctor,
        };
        if !doctor.is_only_cleanup_pending() {
            return mismatch(
                "prikk's doctor does not report exactly one pending cleanup of a publication",
            );
        }
        // 2. The block published is the change the record describes: the one patch, its message, its base.
        let (tip, published) = self.published_is_the_acts(&repo, view, &expected)?;
        // Nothing else is queued (Q2). `status --format json` refuses in this state up to prikk 0.57.0, but the plain
        // text of `status` reads on every prikk, and states the queue's size: exactly one patch, on this view, in a log with one record and no torn
        // byte. The queue's **content** is checked as well, by what prikk reports and what was read above: `verify`
        // reports this code (`doctor` names its finding) only when the queue holds exactly the patches of the block
        // the ref points at (ledger PK-44), and that tip block was required to hold exactly the one published patch.
        // So the queue is that patch, and no other; the size check below is kept beside it.
        let queue = parse_status_text(&w.status_text(&repo)?).map_err(|why| {
            RecoverError::Unavailable(format!("the queue could not be read: {why}"))
        })?;
        if queue.queued != 1
            || queue.target.as_deref() != Some(view)
            || queue.wal_records != 1
            || queue.torn_bytes != 0
        {
            return mismatch(
                "the repository's queue is not exactly the one patch that was published (more is queued, or the log is torn)",
            );
        }
        // Where the JSON `status` can be read (it cannot up to prikk 0.57.0 in the state this step is for; 0.58.0 reads
        // it), the patch itself, and the block the queue is already sealed in (maintenance M51): the block prikk names
        // must be the tip that was read above, and a status that reads an ordinary queue while `doctor` names a
        // pending cleanup is two answers of prikk's that disagree. Nothing else of `already_sealed` is read.
        if let Ok(status) = w.status_full(&repo) {
            if status.trailing_partial_wal_bytes != 0
                || status.provisional_worktree.is_some()
                || status.interrupted_materialization.is_some()
                || status
                    .queue
                    .patches
                    .iter()
                    .any(|q| q.patch_id != published.patch_id)
            {
                return mismatch(
                    "the repository has a torn log, an interrupted state or a queued patch that is not the published one",
                );
            }
            if status
                .queue
                .already_sealed
                .as_ref()
                .is_none_or(|sealed| sealed.block_id != tip.block_id)
            {
                return mismatch(
                    "prikk's status does not say that the queue is already sealed in the block that was published",
                );
            }
        }
        // Its files are the ones that were accepted, read from the block (the worktree cannot be asked yet).
        let change_bytes =
            self.published_files_are_the_acts(&repo, view, &tip, &published, &expected)?;
        // 3. `seal` once more: the ref is published, so it makes no block and drains what the kill left.
        let seeds = self.seeds(wiki);
        let sealed = w.seal(&repo, &seeds, view, change_bytes);
        match (sealed, seeds.left_over()) {
            (Ok(()), false) => {}
            (Ok(()), true) => return Err(RecoverError::SeedsNotRemoved),
            (Err(e), _) => return Err(e.into()),
        }
        // 4. It did what was meant and nothing more: the same tip, and no pending cleanup.
        if w.tip(&repo, view)?.map(|b| b.block_id) != Some(tip.block_id.clone()) {
            return mismatch("the view's tip is not the published block after the seal");
        }
        if w.doctor(&repo)?
            .diagnostics
            .iter()
            .any(|d| d.is_cleanup_pending())
        {
            return mismatch("prikk still reports the cleanup as pending after the seal");
        }
        Ok(())
    }

    fn complete_ref_publication(
        &self,
        wiki: &WikiId,
        view: &str,
        act: &RecoveryToken,
        acceptance: &AcceptanceToken,
    ) -> Result<(), RecoverError> {
        if !is_view_ref(view) {
            return Err(RecoverError::Refused("a view is `heads/<name>`".to_owned()));
        }
        let op = RecoveryOp::CompleteRefPublication {
            acceptance: acceptance.as_str().to_owned(),
            view: view.to_owned(),
        };
        let _held = self.recovering(wiki, act, &op)?;
        // Completing appends a record signed by the maintainer key: while a key retirement is unfinished nothing new is
        // signed (W1), as nothing new is sealed.
        match self.seal_pause(wiki) {
            Ok(None) => {}
            Ok(Some(sentence)) => return Err(RecoverError::RetirementUnfinished(sentence)),
            Err(why) => return Err(RecoverError::Unavailable(why)),
        }
        // The record the change was accepted under: pending, the wiki's, on this view.
        let expected = self
            .ledger
            .pending(acceptance)
            .map_err(|e| RecoverError::Unavailable(e.to_string()))?
            .ok_or(RecoverError::NotRecorded)?;
        if expected.wiki != *wiki || expected.view != view {
            return Err(RecoverError::NotRecorded);
        }
        let repo = self.root.repo_dir(wiki);
        let w = &self.writes;
        // 1. prikk says exactly this: one interrupted publication, of this ref, and nothing else. (No retirement's
        //    record excuses anything here: with a pointer ahead of its log `verify` fails by the ref publication as
        //    well, so a finding of publication trust beside it is "something else".)
        if w.doctor(&repo)?.only_ref_lead() != Some(view) {
            return mismatch(
                "prikk's doctor does not report exactly one interrupted publication, and it is not this view's",
            );
        }
        // 2. The block the pointer leads to is the change the record describes: the one patch, its message, its base.
        let (tip, published) = self.published_is_the_acts(&repo, view, &expected)?;
        // 3. prikk's own plan: this ref, this block as its target and as the state the pointer leads with, the ref log's
        //    last record the state before it, and this block's own sequence next (so one transition, which is the
        //    block's own), with nothing to remove first.
        let seeds = self.seeds(wiki);
        let plan = match (w.ref_complete_plan(&repo, &seeds, view), seeds.left_over()) {
            (Ok(plan), false) => plan,
            (Ok(_), true) => return Err(RecoverError::SeedsNotRemoved),
            (Err(e), false) => return Err(e.into()),
            (Err(e), true) => return Err(note_leftover(e.into())),
        };
        if plan.ref_name != view
            || plan.kind != "Branch"
            || plan.target != tip.block_id
            || plan.leading_ref_state != tip.ref_state_id
            || tip.previous_ref_state_id.is_none()
            || plan.log_tip != tip.previous_ref_state_id
            || plan.sequence != tip.update_seq
            || plan.partial_tail_bytes != 0
        {
            return mismatch(
                "prikk's plan is not the one transition that publishes the block the change was made in, onto the ref log's last record",
            );
        }
        // 4. The queue is that patch and no other: the text of `status` reads on every prikk, and states the queue's size;
        //    where the JSON reads (0.58.0) it names the patch and the block the queue is already sealed in, and where it
        //    refuses it refuses with the integrity error this state makes (up to 0.57.0), and nothing else is read from it.
        let queue = parse_status_text(&w.status_text(&repo)?).map_err(|why| {
            RecoverError::Unavailable(format!("the queue could not be read: {why}"))
        })?;
        if queue.queued != 1
            || queue.target.as_deref() != Some(view)
            || queue.wal_records != 1
            || queue.torn_bytes != 0
        {
            return mismatch(
                "the repository's queue is not exactly the one patch that was published (more is queued, or the log is torn)",
            );
        }
        match w.status_full(&repo) {
            Ok(status) => {
                if status.trailing_partial_wal_bytes != 0
                    || status.provisional_worktree.is_some()
                    || status.interrupted_materialization.is_some()
                    || status.queue.count != 1
                    || status
                        .queue
                        .patches
                        .iter()
                        .any(|q| q.patch_id != published.patch_id)
                {
                    return mismatch(
                        "the repository has a torn log, an interrupted state or a queued patch that is not the published one",
                    );
                }
                if status
                    .queue
                    .already_sealed
                    .as_ref()
                    .is_none_or(|sealed| sealed.block_id != tip.block_id)
                {
                    return mismatch(
                        "prikk's status does not say that the queue is already sealed in the block that was published",
                    );
                }
            }
            Err(SignerError::Refused(why)) if why.starts_with("integrity error") => {}
            Err(e) => return Err(e.into()),
        }
        // 5. Its files are the ones that were accepted, read from the block.
        self.published_files_are_the_acts(&repo, view, &tip, &published, &expected)?;
        // 6. `ref complete`, once.
        let seeds = self.seeds(wiki);
        let done = w.ref_complete(&repo, &seeds, view);
        match (done, seeds.left_over()) {
            (Ok(()), false) => {}
            (Ok(()), true) => return Err(RecoverError::SeedsNotRemoved),
            (Err(e), false) => return Err(e.into()),
            (Err(e), true) => return Err(note_leftover(e.into())),
        }
        // 7. It did what was meant and nothing more: the same tip, and `doctor` no longer names the lead (it names the
        //    cleanup of the queue now, which is the step that follows).
        if w.tip(&repo, view)?.map(|b| b.block_id) != Some(tip.block_id.clone()) {
            return mismatch("the view's tip is not the published block after the step");
        }
        if w.doctor(&repo)?
            .diagnostics
            .iter()
            .any(|d| d.is_publication_lead())
        {
            return mismatch("prikk still reports an interrupted publication after the step");
        }
        Ok(())
    }

    fn complete_view_creation(
        &self,
        wiki: &WikiId,
        view: &str,
        act: &RecoveryToken,
        creation: &ViewCreationAct,
    ) -> Result<(), RecoverError> {
        if !is_view_ref(view) {
            return Err(RecoverError::Refused("a view is `heads/<name>`".to_owned()));
        }
        let op = RecoveryOp::CompleteViewCreation {
            creation: creation.as_str().to_owned(),
            view: view.to_owned(),
        };
        let _held = self.recovering(wiki, act, &op)?;
        // Completing appends a record signed by the maintainer key: while a key retirement is unfinished nothing new
        // is signed (W1), as a new view is refused.
        match self.seal_pause(wiki) {
            Ok(None) => {}
            Ok(Some(sentence)) => return Err(RecoverError::RetirementUnfinished(sentence)),
            Err(why) => return Err(RecoverError::Unavailable(why)),
        }
        // The record the creation was made under: open, the wiki's, for this view.
        let facts = self
            .ledger
            .view_creation(creation)
            .map_err(|e| RecoverError::Unavailable(e.to_string()))?
            .ok_or(RecoverError::NotRecorded)?;
        if facts.wiki != *wiki || facts.name != view {
            return Err(RecoverError::NotRecorded);
        }
        let repo = self.root.repo_dir(wiki);
        let w = &self.writes;
        // 1. prikk says exactly this: one interrupted publication, of this ref, and nothing else.
        if w.doctor(&repo)?.only_ref_lead() != Some(view) {
            return mismatch(
                "prikk's doctor does not report exactly one interrupted publication, and it is not this view's",
            );
        }
        // 2. prikk's own plan: the first publication of this ref, onto the block the act recorded as the view's start
        //    (the block its source pointed at when the act was written, under the lease; the source may have moved
        //    since, and a ref that points anywhere else is not what was recorded), with nothing to remove first.
        let seeds = self.seeds(wiki);
        let plan = match (w.ref_complete_plan(&repo, &seeds, view), seeds.left_over()) {
            (Ok(plan), false) => plan,
            (Ok(_), true) => return Err(RecoverError::SeedsNotRemoved),
            (Err(e), false) => return Err(e.into()),
            (Err(e), true) => return Err(note_leftover(e.into())),
        };
        if plan.ref_name != view
            || plan.kind != "Branch"
            || plan.target != facts.from_block
            || plan.log_tip.is_some()
            || plan.sequence != 1
            || plan.partial_tail_bytes != 0
        {
            return mismatch(
                "prikk's plan is not the first publication of this view onto the block the creation recorded",
            );
        }
        // 3. `ref complete`, once.
        let seeds = self.seeds(wiki);
        let done = w.ref_complete(&repo, &seeds, view);
        match (done, seeds.left_over()) {
            (Ok(()), false) => {}
            (Ok(()), true) => return Err(RecoverError::SeedsNotRemoved),
            (Err(e), false) => return Err(e.into()),
            (Err(e), true) => return Err(note_leftover(e.into())),
        }
        // 4. It did what was meant: `doctor` no longer names the lead (the block was checked before, and `ref complete`
        //    appends the record of the same state).
        if w.doctor(&repo)?
            .diagnostics
            .iter()
            .any(|d| d.lead_ref().is_some())
        {
            return mismatch("prikk still reports an interrupted publication after the step");
        }
        Ok(())
    }

    fn discard_damaged(
        &self,
        wiki: &WikiId,
        act: &RecoveryToken,
    ) -> Result<Discarded, RecoverError> {
        let _held = self.recovering(wiki, act, &RecoveryOp::DiscardDamaged)?;
        Ok(self.writes.discard_damaged(&self.root.repo_dir(wiki))?)
    }

    fn discard_queued(
        &self,
        wiki: &WikiId,
        act: &RecoveryToken,
        patch_id: &str,
    ) -> Result<Discarded, RecoverError> {
        self.discard_queued_op(wiki, act, patch_id)
    }

    fn clear_locks(
        &self,
        wiki: &WikiId,
        act: &RecoveryToken,
        locks: &[String],
    ) -> Result<Vec<String>, RecoverError> {
        let _held = self.recovering(
            wiki,
            act,
            &RecoveryOp::ClearLocks {
                paths: locks.to_vec(),
            },
        )?;
        let repo = self.root.repo_dir(wiki);
        let own = std::fs::canonicalize(repo.join(".prikk")).map_err(|e| {
            RecoverError::Unavailable(format!(
                "the repository's store could not be found ({})",
                e.kind()
            ))
        })?;
        let held = self.writes.locks(&repo)?;
        // Everything is checked before anything is cleared.
        let mut to_clear: Vec<&str> = Vec::new();
        for want in locks {
            let Some(lock) = held.iter().find(|l| l.path == *want) else {
                continue; // no longer held: nothing to clear
            };
            if !ACCEPTANCE_LOCK_KINDS.contains(&lock.kind.as_str()) {
                return mismatch("a lock is of a kind an acceptance does not leave");
            }
            if !Path::new(&lock.path).starts_with(&own) {
                return mismatch("a lock is not inside the wiki's own store");
            }
            to_clear.push(&lock.path);
        }
        let mut cleared = Vec::new();
        for path in to_clear {
            self.writes.unlock(&repo, path)?;
            cleared.push(path.to_owned());
        }
        Ok(cleared)
    }

    fn restore_worktree(
        &self,
        wiki: &WikiId,
        view: &str,
        act: &RecoveryToken,
    ) -> Result<usize, RecoverError> {
        if !is_view_ref(view) {
            return Err(RecoverError::Refused("a view is `heads/<name>`".to_owned()));
        }
        let _held = self.recovering(
            wiki,
            act,
            &RecoveryOp::RestoreWorktree {
                view: view.to_owned(),
            },
        )?;
        let repo = self.root.repo_dir(wiki);
        let w = &self.writes;
        // A queued patch makes the worktree its baseline, and what the worktree holds is that patch: never touched.
        if w.status_full(&repo)?.queue.count != 0 {
            return mismatch("a patch is queued; the worktree is its result and is not restored");
        }
        let first = w.worktree_full(&repo)?;
        if first.clean {
            return Ok(0);
        }
        let Some(reference) = first.reference.clone().filter(|r| is_view_ref(r)) else {
            return mismatch("the worktree is not a copy of a view");
        };
        // The step names what it restores: the record (the plan, the audit trail) says `view`, so a worktree that is a
        // copy of another view is refused, with nothing changed, rather than put back under the wrong name.
        if reference != view {
            return Err(RecoverError::DoesNotMatch(format!(
                "the worktree is a copy of {reference}, not of {view}, the view the step names"
            )));
        }
        let mut restored = 0_usize;
        // 1. Declared renames go back (a move back cancels the declaration: ledger PK-29).
        for d in &first.declarations {
            let (old, new) = (path_of(&d.old_path)?, path_of(&d.new_path)?);
            w.mv(&repo, &new, &old)?;
            restored += 1;
        }
        // 2. What still differs is put back from the tip, or taken away.
        let second = if first.declarations.is_empty() {
            first
        } else {
            w.worktree_full(&repo)?
        };
        if !second.declarations.is_empty() {
            return mismatch("a declared rename could not be moved back");
        }
        let mut applied = Applied::default();
        for c in &second.changes {
            let rel = path_of(&c.path)?;
            match c.kind.as_str() {
                "modified" | "missing" => {
                    let bytes = w.cat(&repo, &reference, &rel, MAX_FILE_BYTES)?;
                    applied
                        .write(&repo, &rel, &bytes)
                        .map_err(RecoverError::Unavailable)?;
                }
                "untracked" => applied
                    .delete(&repo, &rel)
                    .map_err(RecoverError::Unavailable)?,
                _ => {
                    return mismatch(
                        "the worktree has a change of a kind this build does not restore",
                    );
                }
            }
            restored += 1;
        }
        // 3. It is the baseline now, or this says it is not.
        if !w.worktree_clean(&repo)? {
            return mismatch("the worktree is still not the baseline after it was put back");
        }
        Ok(restored)
    }
}

/// Add to a recovery failure that the seeds' temporary directory is also still there (as an acceptance's failure says it,
/// handoff 005 K23): the failure is the news, and the directory is told beside it, not instead of it. An error that
/// carries no sentence (a lease, a ceiling) is returned as it is.
fn note_leftover(e: RecoverError) -> RecoverError {
    const NOTE: &str = "; also, a temporary directory of decrypted seeds could not be removed (the next start removes it)";
    match e {
        RecoverError::DoesNotMatch(m) => RecoverError::DoesNotMatch(format!("{m}{NOTE}")),
        RecoverError::Refused(m) => RecoverError::Refused(format!("{m}{NOTE}")),
        RecoverError::Unavailable(m) => RecoverError::Unavailable(format!("{m}{NOTE}")),
        other => other,
    }
}

fn path_of(p: &str) -> Result<String, RecoverError> {
    worktree::repo_path(Path::new(p)).map_err(|why| RecoverError::DoesNotMatch(why.to_owned()))
}
