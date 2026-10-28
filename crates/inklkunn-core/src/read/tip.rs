//! A view's tip, held for a few seconds, and the store size that goes with it (RFC 003 D-2; handoff 003 T3).
//!
//! **The tip** is the RefState id of a view (`branch list`), which prikk changes whenever the view moves;
//! every cache is keyed by it. Asking prikk for it on every request would make `branch list` the cost of a
//! cache hit, so one answer is held for [`ReadConfig::tip_ttl`](super::ReadConfig::tip_ttl) and serves
//! every view of the wiki (the call lists them all). The price is staleness of at most that long: a page
//! changed a moment ago is shown as it was until the hold ends, which the write path shortens by calling
//! [`ReadService::forget_wiki`](super::ReadService::forget_wiki) after it seals.
//!
//! **The store size** *S* is what the driver costs a call from. The driver walks `.prikk` for it on every
//! call that follows *S*; here it is walked **once per change of the wiki's views** and handed to the driver
//! as a hint (`inklkunn_prikk::SizeHints`), keyed by the views' state ids: whenever any view moves, the
//! figure is dropped and walked again, so it is true after a write and cheap between writes.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use inklkunn_prikk::{SizeHints, Views, cost};
use inklkunn_store::WikiId;

/// One view and its tip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tip {
    /// The view's full ref name (`heads/main`).
    pub view: String,
    /// The view's RefState id at the time it was read: the key of everything derived from it.
    pub state_id: String,
}

/// What one `branch list` said, kept for a while.
#[derive(Debug, Clone)]
struct Held {
    at: Instant,
    /// `(view, state id)` of every open view, in prikk's order.
    views: Vec<(String, String)>,
}

impl Held {
    fn from(views: &Views) -> Self {
        Held {
            at: Instant::now(),
            views: views
                .views
                .iter()
                .filter(|v| !v.closed)
                .map(|v| (v.name.clone(), v.state_id.clone()))
                .collect(),
        }
    }

    /// One string that changes whenever any view moves, opens or closes.
    fn fingerprint(&self) -> String {
        self.views
            .iter()
            .map(|(name, state)| format!("{name}={state}"))
            .collect::<Vec<_>>()
            .join(";")
    }
}

/// The held answers and the store figures, per wiki.
#[derive(Debug)]
pub(super) struct Tips {
    ttl: Duration,
    held: Mutex<HashMap<WikiId, Held>>,
    /// `(fingerprint of the views the figure was walked at, bytes)`.
    sizes: Mutex<HashMap<WikiId, (String, u64)>>,
    hints: Option<SizeHints>,
}

impl Tips {
    pub(super) fn new(ttl: Duration, hints: Option<SizeHints>) -> Self {
        Tips {
            ttl,
            held: Mutex::new(HashMap::new()),
            sizes: Mutex::new(HashMap::new()),
            hints,
        }
    }

    /// The held answer for `wiki`, if it is younger than the hold.
    pub(super) fn fresh(&self, wiki: &WikiId, view: &str) -> Option<Option<Tip>> {
        let held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        let h = held.get(wiki).filter(|h| h.at.elapsed() < self.ttl)?;
        Some(
            h.views
                .iter()
                .find(|(name, _)| name == view)
                .map(|(name, state)| Tip {
                    view: name.clone(),
                    state_id: state.clone(),
                }),
        )
    }

    /// Hold a fresh `branch list` answer. When the views have moved since the store was last measured, the
    /// figure is dropped and walked again here, once, and the driver is told (the walk is the truth after a
    /// write; the hint is what spares the calls after it).
    pub(super) fn hold(&self, wiki: &WikiId, repo: &Path, views: &Views) {
        let held = Held::from(views);
        let fingerprint = held.fingerprint();
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(wiki.clone(), held);
        let Some(hints) = &self.hints else {
            return;
        };
        // Unchanged views and a figure the driver still trusts: nothing to walk. (A hint expires on its own,
        // which bounds what a change nobody announced can cost; then the walk is done again here.)
        let unchanged = {
            let sizes = self.sizes.lock().unwrap_or_else(PoisonError::into_inner);
            sizes
                .get(wiki)
                .is_some_and(|(seen, _)| *seen == fingerprint)
        };
        if unchanged && hints.get(repo).is_some() {
            return;
        }
        // The walk is outside every lock: it can take long on a large store, and another wiki's reads do not
        // wait for it. Two readers who both find the figure stale both walk; the figure is the same.
        let measured = cost::store_size(repo);
        let mut sizes = self.sizes.lock().unwrap_or_else(PoisonError::into_inner);
        match measured {
            Ok(bytes) => {
                hints.set(repo, bytes);
                sizes.insert(wiki.clone(), (fingerprint, bytes));
            }
            // Not measurable now: no hint, so the driver walks and says what is wrong itself.
            Err(_) => {
                hints.forget(repo);
                sizes.remove(wiki);
            }
        }
    }

    /// Every view's current state id, from the held answer (whatever its age).
    pub(super) fn current_tips(&self, wiki: &WikiId) -> Vec<String> {
        let held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        held.get(wiki)
            .map(|h| h.views.iter().map(|(_, state)| state.clone()).collect())
            .unwrap_or_default()
    }

    /// The store figure last walked for `wiki`.
    pub(super) fn store_bytes(&self, wiki: &WikiId) -> Option<u64> {
        let sizes = self.sizes.lock().unwrap_or_else(PoisonError::into_inner);
        sizes.get(wiki).map(|(_, bytes)| *bytes)
    }

    /// Forget what is held and measured for `wiki`: the next request asks prikk and walks.
    pub(super) fn forget(&self, wiki: &WikiId, repo: &Path) {
        self.held
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(wiki);
        self.sizes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(wiki);
        if let Some(hints) = &self.hints {
            hints.forget(repo);
        }
    }
}

#[cfg(test)]
mod tests;
