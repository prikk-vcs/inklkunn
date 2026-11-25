//! **The writes index built in the background** (RFC 006 D-8; 0.4.0 readiness correction P-5): after a content hide, and at
//! start-up while one is in force, the index of who wrote each path ([`ReadService::background_step`]) is built toward the tip of
//! each view without a reader having to walk the history to make it, so that an old version of a page answers with its text or its
//! tombstone and not *cannot be checked yet*.
//!
//! `inklkunn-core` starts no thread it cannot stop and leaves the how to the binary ([`WritesBackground`]). The how here is **one
//! worker thread** and a **queue of wikis**:
//!
//! - **One task per wiki.** A wiki that is asked for while it is queued or being built is the same task (a set of the wikis in
//!   flight), so two triggers start one builder. The worker takes the wikis **in turn**, one step each ([`Build::step`]: a bounded
//!   number of `show`s of one view), so that a wiki with a long history does not keep another waiting, and **there is one prikk
//!   process of this kind at a time, whatever the number of wikis**.
//! - **Niced** (the box is shared, and a `show` is a process): between two steps the worker **pauses** for the time the step took
//!   (at least [`Pacing::floor`]), so that it uses at most about half of one core, and **yields to readers**: a request that built
//!   something of an index since the last step is a reader that is working, and the worker waits a little longer for it
//!   ([`Pacing::yield_pause`], at most [`Pacing::max_yields`] times in a row, so that it is never starved). **A pause is a whole
//!   pause**: a wiki asked for during it is queued (once: the set of wikis in flight) and taken up after it, never instead of it, so
//!   that traffic in many wikis does not cut the worker's rest short.
//! - **Bounded.** A step is bounded by the core; a task by [`Pacing::max_steps`]; the worker's pauses are the queue's own timeouts, so
//!   a new wiki is queued during a pause, and the worker **ends when the last sender is dropped** (the server is going down: the
//!   step in flight finishes, and no other begins).
//! - **Resumable.** The index is derived and kept by the core; a task that ends for any reason loses nothing, and the next one goes on.
//!   A step that fails ends the wiki's task and is **said once** to the operator (until a step of that wiki succeeds again); a hide
//!   asks again at once, and a reader's ask is heard once a pause ([`Pacing::retry_after`]) has passed, so that a reader's *try
//!   again in a little while* is true and a failing prikk is not asked for at every request.
//! - **Never panics, never poisons.** A step that panics is a failure like another.
//! - **Not joined at exit.** The worker's thread is detached: the server does not wait for it when it goes down, and a step in flight
//!   is cut off with the process. That is safe because a step does nothing that a cut can half-do: the core's append to the index is
//!   **one transaction** (it is all there or not at all, and the next task goes on from the index as it was), and the `show` the step
//!   waits for only **reads**.

use std::collections::{HashMap, HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};
use std::time::{Duration, Instant};

use inklkunn_core::read::{AskedFor, BackgroundStep, ReadService, WritesBackground};
use inklkunn_store::{WikiId, WikiRecord};

/// What the worker builds with: the core's step and the count of the readers' own builds.
pub(crate) trait Build: Send + Sync {
    /// One step of the background build of the wiki's index.
    fn step(&self, wiki: &WikiRecord) -> BackgroundStep;
    /// How many times a request built something of an index (see [`ReadService::reader_advances`]).
    fn reader_advances(&self) -> u64;
}

impl Build for ReadService {
    fn step(&self, wiki: &WikiRecord) -> BackgroundStep {
        self.background_step(wiki)
    }

    fn reader_advances(&self) -> u64 {
        ReadService::reader_advances(self)
    }
}

/// How the worker paces itself.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pacing {
    /// The least the worker waits between two steps.
    pub floor: Duration,
    /// What the worker waits between two steps as a share of the time the step took, in percent (100: as long as the step took, so a
    /// busy worker uses half of a core).
    pub share_percent: u32,
    /// How long the worker waits once more when a reader built something since the last step.
    pub yield_pause: Duration,
    /// The most such waits in a row.
    pub max_yields: u32,
    /// How long after a failure a **reader's** ask is not heard (a hide's always is).
    pub retry_after: Duration,
    /// How long after a task ended with nothing left to do a reader's ask is not heard.
    pub reader_every: Duration,
    /// The most steps one task takes before it ends (the next ask goes on).
    pub max_steps: usize,
}

impl Default for Pacing {
    fn default() -> Self {
        Pacing {
            floor: Duration::from_millis(200),
            share_percent: 100,
            yield_pause: Duration::from_secs(1),
            max_yields: 3,
            retry_after: Duration::from_secs(30),
            reader_every: Duration::from_secs(5),
            max_steps: 4_096,
        }
    }
}

/// The wikis in flight, and the wikis a reader's ask is not heard for, until when.
#[derive(Default)]
struct Shared {
    /// A wiki in this set is queued or being built: asking for it again is the same task.
    queued: HashSet<WikiId>,
    /// A wiki a reader's ask is not heard for until the time given.
    quiet: HashMap<WikiId, Instant>,
    /// The worker has no task and waits for a wiki to be asked for (what a test waits for: with nothing queued, nothing is left).
    waiting: bool,
}

type Log = Arc<dyn Fn(&str) + Send + Sync>;

/// The background build of the writes indexes: asked by the hide, the start and the readers ([`WritesBackground`]).
pub(crate) struct IndexWarmer {
    /// `None` when the worker could not be started (nothing is built in the background; the requests build what they need).
    queue: Option<Sender<WikiRecord>>,
    shared: Arc<Mutex<Shared>>,
    /// What the worker builds with, set once the service exists ([`IndexWarmer::bind`]); weak, so that the service ending ends the worker.
    builder: Arc<OnceLock<Weak<dyn Build>>>,
}

impl std::fmt::Debug for IndexWarmer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("IndexWarmer")
    }
}

impl IndexWarmer {
    /// The background build, its worker started and waiting for the service to be [`bound`](IndexWarmer::bind). `log` hears what
    /// the operator should know.
    pub(crate) fn start(pacing: Pacing, log: Log) -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let builder: Arc<OnceLock<Weak<dyn Build>>> = Arc::new(OnceLock::new());
        let (queue, jobs) = channel::<WikiRecord>();
        let worker = Worker {
            jobs,
            shared: shared.clone(),
            builder: builder.clone(),
            pacing,
            log: log.clone(),
        };
        let started = std::thread::Builder::new()
            .name("inklkunn-writes-index".to_owned())
            .spawn(move || worker.run());
        let queue = match started {
            Ok(_) => Some(queue),
            Err(e) => {
                log(&format!(
                    "the writes index cannot be built in the background ({}): requests build what they need",
                    e.kind()
                ));
                None
            }
        };
        IndexWarmer {
            queue,
            shared,
            builder,
        }
    }

    /// The service the worker builds with. Held weakly: the worker ends when the service does.
    pub(crate) fn bind(&self, builder: Weak<dyn Build>) {
        let _ = self.builder.set(builder);
    }
}

impl WritesBackground for IndexWarmer {
    fn ask(&self, wiki: &WikiRecord, why: AskedFor) {
        let Some(queue) = &self.queue else {
            return;
        };
        let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        if why == AskedFor::Reader
            && shared
                .quiet
                .get(&wiki.id)
                .is_some_and(|until| *until > Instant::now())
        {
            return;
        }
        if shared.queued.insert(wiki.id.clone()) && queue.send(wiki.clone()).is_err() {
            // The worker is gone (the server is going down).
            shared.queued.remove(&wiki.id);
        }
    }
}

/// Start-up's resume: ask for the background build of each wiki in which a content hide is in force (`resume` does the asking and
/// says whether it did), and say it in **a line** each: what is resumed, and **what could not be looked at** (the wiki list, or one
/// wiki's hide). Nothing here fails the start: a wiki that could not be looked at is built when a hide or a reader asks. The lines
/// are the start-up notes (the same place as the server's other start-up lines).
pub(crate) fn resume_notes<E: std::fmt::Display, F: std::fmt::Display>(
    wikis: Result<Vec<WikiRecord>, E>,
    mut resume: impl FnMut(&WikiRecord) -> Result<bool, F>,
) -> Vec<String> {
    let wikis = match wikis {
        Ok(wikis) => wikis,
        Err(e) => {
            return vec![format!(
                "the wikis could not be listed to resume the background build of the writes index ({}); it is built when a hide or a reader asks for it",
                one_line(&e)
            )];
        }
    };
    let mut notes = Vec::new();
    for wiki in wikis {
        match resume(&wiki) {
            Ok(true) => notes.push(format!(
                "a content hide is in force in wiki {}: its index of who wrote each page is built in the background",
                wiki.slug
            )),
            Ok(false) => {}
            Err(e) => notes.push(format!(
                "whether a content hide is in force in wiki {} could not be looked at ({}); its writes index is built when a hide or a reader asks for it",
                wiki.slug,
                one_line(&e)
            )),
        }
    }
    notes
}

/// An error as one line (a start-up note is a line).
fn one_line(e: &dyn std::fmt::Display) -> String {
    e.to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A wiki being built.
struct Task {
    wiki: WikiRecord,
    steps: usize,
}

/// The worker thread's own state.
struct Worker {
    jobs: Receiver<WikiRecord>,
    shared: Arc<Mutex<Shared>>,
    builder: Arc<OnceLock<Weak<dyn Build>>>,
    pacing: Pacing,
    log: Log,
}

/// What one step of a task came to.
enum Outcome {
    Idle,
    Progress,
    Failed(String),
}

impl Worker {
    fn shared(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.shared.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn run(self) {
        // The wikis with a task, in the order they are taken in turn; the wikis a step of which failed (so the operator is told once).
        let mut active: VecDeque<Task> = VecDeque::new();
        let mut failing: HashSet<WikiId> = HashSet::new();
        let mut seen_readers = 0_u64;
        let mut yields = 0_u32;
        loop {
            if active.is_empty() {
                self.shared().waiting = true;
                let asked = self.jobs.recv();
                self.shared().waiting = false;
                match asked {
                    Ok(wiki) => active.push_back(Task { wiki, steps: 0 }),
                    // The last sender is gone: the server is going down.
                    Err(_) => return,
                }
            }
            while let Ok(wiki) = self.jobs.try_recv() {
                active.push_back(Task { wiki, steps: 0 });
            }
            let Some(mut task) = active.pop_front() else {
                continue;
            };
            let Some(builder) = self.builder.get().and_then(Weak::upgrade) else {
                // Nothing to build with (not bound yet, or the service is gone): the task is dropped, and the next ask makes it again.
                self.shared().queued.remove(&task.wiki.id);
                continue;
            };
            // Yield to the readers that are building: one wait, and then the step is taken (a reader's request is bounded, so it ends).
            let readers = builder.reader_advances();
            if readers != seen_readers && yields < self.pacing.max_yields {
                seen_readers = readers;
                yields += 1;
                active.push_front(task);
                drop(builder);
                if self.wait(self.pacing.yield_pause, &mut active) {
                    return;
                }
                continue;
            }
            yields = 0;
            seen_readers = readers;
            let began = Instant::now();
            let outcome = self.step(builder.as_ref(), &task.wiki);
            drop(builder);
            task.steps += 1;
            // The task that is to take its next turn, behind the wikis that were asked for meanwhile.
            let mut next_turn: Option<Task> = None;
            match outcome {
                Outcome::Progress => {
                    failing.remove(&task.wiki.id);
                    if task.steps >= self.pacing.max_steps {
                        // A task of this length ends; the next ask goes on from where the index is.
                        self.shared().queued.remove(&task.wiki.id);
                    } else {
                        next_turn = Some(task);
                    }
                }
                Outcome::Idle => {
                    // (A failing wiki is a failing wiki until the look after the task, `finish`, succeeds too.)
                    next_turn = self.finish(task, &mut failing);
                }
                Outcome::Failed(why) => self.failed(&task.wiki, &why, &mut failing),
            }
            while let Ok(wiki) = self.jobs.try_recv() {
                active.push_back(Task { wiki, steps: 0 });
            }
            active.extend(next_turn);
            // Niced: as long as the step took, at least the floor, and nothing when there is nothing more to do.
            if !active.is_empty() {
                let took = began.elapsed();
                let share = took
                    .saturating_mul(self.pacing.share_percent)
                    .checked_div(100)
                    .unwrap_or_default();
                if self.wait(share.max(self.pacing.floor), &mut active) {
                    return;
                }
            }
        }
    }

    /// One step, as the outcome: a panic is a failure like another.
    fn step(&self, builder: &dyn Build, wiki: &WikiRecord) -> Outcome {
        match catch_unwind(AssertUnwindSafe(|| builder.step(wiki))) {
            Ok(BackgroundStep::Idle) => Outcome::Idle,
            Ok(BackgroundStep::Progress) => Outcome::Progress,
            Ok(BackgroundStep::Failed(e)) => Outcome::Failed(e.to_string()),
            Err(_) => Outcome::Failed("the step panicked".to_owned()),
        }
    }

    /// A step failed: the wiki's task ends, a reader's ask is not heard until [`Pacing::retry_after`] has passed (a hide's always is),
    /// and the operator is told **once** (until a step of that wiki succeeds again).
    fn failed(&self, wiki: &WikiRecord, why: &str, failing: &mut HashSet<WikiId>) {
        {
            let mut shared = self.shared();
            shared.queued.remove(&wiki.id);
            shared
                .quiet
                .insert(wiki.id.clone(), Instant::now() + self.pacing.retry_after);
        }
        if failing.insert(wiki.id.clone()) {
            (self.log)(&format!(
                "the writes index of wiki {} could not be built in the background ({why}); it is tried again at the next hide, the next start, or when a reader needs it",
                wiki.slug
            ));
        }
    }

    /// A task found nothing left to do. It **leaves the set, and is looked at once more**: an ask that came after the last look and
    /// before the leaving was heard as the same task, and the look finds what it asked for (a hide is recorded before it asks). A
    /// look that fails is a failed step like another ([`Worker::failed`]: said once, and the retry pause is taken).
    /// `Some` when the look found more to do.
    fn finish(&self, mut task: Task, failing: &mut HashSet<WikiId>) -> Option<Task> {
        {
            let mut shared = self.shared();
            shared.queued.remove(&task.wiki.id);
            shared.quiet.insert(
                task.wiki.id.clone(),
                Instant::now() + self.pacing.reader_every,
            );
        }
        let builder = self.builder.get().and_then(Weak::upgrade)?;
        let outcome = self.step(builder.as_ref(), &task.wiki);
        drop(builder);
        match outcome {
            Outcome::Progress => {
                failing.remove(&task.wiki.id);
            }
            Outcome::Idle => {
                failing.remove(&task.wiki.id);
                return None;
            }
            Outcome::Failed(why) => {
                self.failed(&task.wiki, &why, failing);
                return None;
            }
        }
        // More to do after all: back in the set (unless a newer ask is already queued for it, which is the same task).
        let mut shared = self.shared();
        shared.quiet.remove(&task.wiki.id);
        if !shared.queued.insert(task.wiki.id.clone()) {
            return None;
        }
        task.steps += 1;
        Some(task)
    }

    /// Wait **the whole of** `pause`, queuing the wikis asked for meanwhile (they are taken up after the pause, not instead of it: a
    /// wiki asked for every few milliseconds must not cut the pause short). `true` when the worker is to end.
    fn wait(&self, pause: Duration, active: &mut VecDeque<Task>) -> bool {
        let deadline = Instant::now() + pause;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.jobs.recv_timeout(left) {
                Ok(wiki) => active.push_back(Task { wiki, steps: 0 }),
                Err(RecvTimeoutError::Timeout) => return false,
                Err(RecvTimeoutError::Disconnected) => return true,
            }
        }
    }
}

#[cfg(test)]
mod tests;
