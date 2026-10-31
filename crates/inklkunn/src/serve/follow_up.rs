//! What the server asks for after a change was signed into a history, **off the request path** (RFC 004 D-4
//! step 6; handoff 004 T4): the title index of the new tip is warmed, so that the first reader after an acceptance
//! does not pay for building it, and the wiki's repository is verified (OP-06), so that the badge on its pages
//! comes from a fresh check.
//!
//! `inklkunn-core` starts no thread it cannot stop and leaves the how to the binary ([`AfterSeal`]). The how here
//! is **one worker thread** fed by a **bounded queue**: a request that was just answered never waits for it, a
//! flood of acceptances cannot make threads (there is one) or memory (there are [`QUEUE_BOUND`] jobs at most; a
//! job asked for while the same job for the same wiki is already waiting is the same job, and one asked for
//! when the queue is full is dropped, because the next acceptance asks again and a page that needs the index
//! builds it itself). The worker ends when the last sender is dropped.

use std::collections::HashSet;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex, PoisonError};

use inklkunn_core::{AfterSeal, ReadService, Verifier};
use inklkunn_store::WikiRecord;

/// The most jobs that wait at once, over all wikis.
pub(crate) const QUEUE_BOUND: usize = 16;

/// What can be asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    /// Build the title index of the wiki's tip.
    WarmTitleIndex,
    /// Verify the wiki's repository and record the verdict.
    Verify,
}

/// One thing to do for one wiki.
#[derive(Debug, Clone)]
pub(crate) struct Job {
    pub kind: Kind,
    pub wiki: WikiRecord,
}

/// The follow-ups of an acceptance, run one at a time on one thread.
pub(crate) struct FollowUp {
    /// `None`: the worker could not be started, and nothing is asked for (the pages build what they need).
    queue: Option<SyncSender<Job>>,
    /// The jobs that wait: a job is in this set from the moment it is queued until the worker takes it.
    waiting: Arc<Mutex<HashSet<(Kind, String)>>>,
}

impl std::fmt::Debug for FollowUp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FollowUp")
    }
}

impl FollowUp {
    /// The follow-ups over the server's read service and verifier. `log` hears what the operator should know.
    pub(crate) fn new(
        read: Arc<ReadService>,
        verifier: Arc<Verifier>,
        log: Arc<dyn Fn(&str) + Send + Sync>,
    ) -> Self {
        Self::with_runner(move |job: &Job| match job.kind {
            Kind::WarmTitleIndex => {
                if let Err(e) = read.title_index(&job.wiki, None) {
                    log(&format!(
                        "the title index of wiki {} could not be warmed after a change ({e}); the next page builds it",
                        job.wiki.slug
                    ));
                }
            }
            Kind::Verify => {
                let checked = verifier.check(&job.wiki);
                if let Some(why) = checked.operator {
                    log(&format!(
                        "the verification of wiki {} after a change did not complete: {why}",
                        job.wiki.slug
                    ));
                }
            }
        })
    }

    /// The same, doing each job with `run` (a test counts them).
    pub(crate) fn with_runner(run: impl Fn(&Job) + Send + 'static) -> Self {
        let waiting: Arc<Mutex<HashSet<(Kind, String)>>> = Arc::new(Mutex::new(HashSet::new()));
        let (queue, jobs) = sync_channel::<Job>(QUEUE_BOUND);
        let taken = waiting.clone();
        let worker = std::thread::Builder::new()
            .name("inklkunn-after-seal".to_owned())
            .spawn(move || {
                for job in jobs {
                    // Taken: a change signed while this one runs asks again, and is not lost.
                    taken
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .remove(&(job.kind, job.wiki.id.as_str().to_owned()));
                    run(&job);
                }
            });
        FollowUp {
            queue: worker.ok().map(|_| queue),
            waiting,
        }
    }

    /// Ask for `kind` of `wiki`: never blocks, never waits for the work.
    fn ask(&self, kind: Kind, wiki: &WikiRecord) {
        let Some(queue) = &self.queue else {
            return;
        };
        let key = (kind, wiki.id.as_str().to_owned());
        let mut waiting = self.waiting.lock().unwrap_or_else(PoisonError::into_inner);
        if !waiting.insert(key.clone()) {
            return;
        }
        let job = Job {
            kind,
            wiki: wiki.clone(),
        };
        if queue.try_send(job).is_err() {
            waiting.remove(&key);
        }
    }
}

impl AfterSeal for FollowUp {
    fn warm_title_index(&self, wiki: &WikiRecord) {
        self.ask(Kind::WarmTitleIndex, wiki);
    }

    fn enqueue_verification(&self, wiki: &WikiRecord) {
        self.ask(Kind::Verify, wiki);
    }
}

#[cfg(test)]
mod tests;
