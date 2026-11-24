//! A view's tip, held for a few seconds, and the store size that goes with it (RFC 003 D-2; handoff 003 T3).
//!
//! **The tip** is the RefState id of a view (`branch list`), which prikk changes whenever the view moves;
//! every cache is keyed by it, **and every read is pinned to the block that state id belongs to**: `branch
//! list` does not name the block, so the first time a state id is met one `log --limit 1` asks for the pair,
//! and the pair (state id, block id) is what the tip is. A page and an index are then read *at the block*
//! (`cat --ref <block id>`, `extract --ref <block id>`), so a write between two of a request's calls cannot
//! put newer content under an older key (the key is the content). Asking prikk for it on every request would make `branch list` the cost of a
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

use inklkunn_prikk::{Point, SizeHints, Views, cost};
use inklkunn_store::{WikiId, WikiRecord};

use super::{Cause, ReadError, ReadService};

/// Only this module can say it: the field that keeps a [`Tip`] from being written as a literal anywhere else.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Seal;

/// **One view's tip, as [`ReadService::tip`] read it**, and nothing else is one. The content of a view's tip cannot be hidden
/// (RFC 006 D-8), so a read *at a tip* is unchecked, and that is only true of a block the moderation rules (the tip check) keep so.
/// A `Tip` therefore **can be made only here**: the one private constructor is `ReadService::tip`'s, and a block that is any other
/// place of a view's history is a [`BlockPoint`], whose every read goes through the one reader of a page's text at a block
/// ([`ReadService::text_at`]; handoff G-T4, correction J-3). A scan of the sources fails when a `Tip {` literal is written outside
/// this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tip {
    view: String,
    state_id: String,
    block_id: String,
    seal: Seal,
}

impl Tip {
    /// The view's full ref name (`heads/main`).
    pub fn view(&self) -> &str {
        &self.view
    }

    /// The view's RefState id at the time it was read: the key of everything derived from it.
    pub fn state_id(&self) -> &str {
        &self.state_id
    }

    /// The block that state id belongs to (64 hex): what every read of this tip is pinned to. **Empty** for the tip of a page served
    /// stale, which names no block.
    pub fn block_id(&self) -> &str {
        &self.block_id
    }

    /// A tip whose block is not known: what a stale page is read at (it is read from the cache, not from prikk). **Its block id
    /// is empty, so it names no block and no read can be made at it.**
    pub(super) fn unpinned(view: &str, state_id: &str) -> Tip {
        Tip {
            view: view.to_owned(),
            state_id: state_id.to_owned(),
            block_id: String::new(),
            seal: Seal,
        }
    }

    /// A tip for a test of what is built from tips, which has no repository to read one from. **Test builds only**: the product has
    /// no way to make a tip but [`ReadService::tip`].
    #[cfg(test)]
    pub(crate) fn for_tests(view: &str, state_id: &str, block_id: &str) -> Tip {
        Tip {
            view: view.to_owned(),
            state_id: state_id.to_owned(),
            block_id: block_id.to_owned(),
            seal: Seal,
        }
    }

    /// This tip as a place to read from: the plain form of a block, which a read that is not at a tip is made at.
    pub fn point(&self) -> BlockPoint {
        BlockPoint {
            view: self.view.clone(),
            state_id: self.state_id.clone(),
            block_id: self.block_id.clone(),
        }
    }
}

/// **A block of a view's history that is not known to be its tip**: a proposal's base, a change's permalink, the block an
/// acceptance has just read. Anyone can name one. **Every read of a page's or a file's text through it is checked** by the one
/// reader ([`ReadService::text_at`]), which answers *hidden* when the change that last wrote the path has its content hidden (handoff
/// G-T4, correction J-3): the text, the transclusions of a rendered page, the extract an index is built from, the knowledge and
/// title indexes at a base, and the hunks of a diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockPoint {
    view: String,
    state_id: String,
    block_id: String,
}

impl BlockPoint {
    /// The block `block_id` of `view`'s history, as the state `state_id` of the view named it.
    pub fn new(
        view: impl Into<String>,
        state_id: impl Into<String>,
        block_id: impl Into<String>,
    ) -> Self {
        BlockPoint {
            view: view.into(),
            state_id: state_id.into(),
            block_id: block_id.into(),
        }
    }

    /// The view's full ref name.
    pub fn view(&self) -> &str {
        &self.view
    }

    /// The view's RefState id at this block: the key of what is derived from it.
    pub fn state_id(&self) -> &str {
        &self.state_id
    }

    /// The block's id.
    pub fn block_id(&self) -> &str {
        &self.block_id
    }
}

/// A place a block's tree is extracted at and an index is built from: a tip, or any other block. **Not public**: what the service
/// hands out of a place that is not a tip passes the one reader first.
pub(super) trait Place {
    /// The RefState id the derived rows are keyed by.
    fn state_id(&self) -> &str;
    /// The block.
    fn block_id(&self) -> &str;
}

impl Place for Tip {
    fn state_id(&self) -> &str {
        &self.state_id
    }
    fn block_id(&self) -> &str {
        &self.block_id
    }
}

impl Place for BlockPoint {
    fn state_id(&self) -> &str {
        &self.state_id
    }
    fn block_id(&self) -> &str {
        &self.block_id
    }
}

/// Where a page was read: at a view's tip, or at another block of its history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadAt {
    /// The view's tip.
    Tip(Tip),
    /// A block of the view's history (a permalink).
    Block(BlockPoint),
}

impl ReadAt {
    /// The view's full ref name.
    pub fn view(&self) -> &str {
        match self {
            ReadAt::Tip(tip) => &tip.view,
            ReadAt::Block(point) => point.view(),
        }
    }

    /// The RefState id.
    pub fn state_id(&self) -> &str {
        match self {
            ReadAt::Tip(tip) => &tip.state_id,
            ReadAt::Block(point) => point.state_id(),
        }
    }

    /// The block's id (empty for the tip of a page served stale).
    pub fn block_id(&self) -> &str {
        match self {
            ReadAt::Tip(tip) => &tip.block_id,
            ReadAt::Block(point) => point.block_id(),
        }
    }

    /// The tip, when this is one.
    pub fn tip(&self) -> Option<&Tip> {
        match self {
            ReadAt::Tip(tip) => Some(tip),
            ReadAt::Block(_) => None,
        }
    }
}

impl From<Tip> for ReadAt {
    fn from(tip: Tip) -> Self {
        ReadAt::Tip(tip)
    }
}

impl From<BlockPoint> for ReadAt {
    fn from(point: BlockPoint) -> Self {
        ReadAt::Block(point)
    }
}

/// A view and one RefState id of it.
type ViewState = (String, String);

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
    /// The block each `(view, state id)` of a wiki belongs to, learned once per state id.
    blocks: Mutex<HashMap<WikiId, HashMap<ViewState, String>>>,
    /// The last answer's `(view, state id)` pairs whatever its age, and kept when the hold is forgotten: what
    /// a page could be served from, labelled out of date, when prikk cannot be asked (OP-03).
    last: Mutex<HashMap<WikiId, Vec<ViewState>>>,
    /// `(fingerprint of the views the figure was walked at, bytes)`.
    sizes: Mutex<HashMap<WikiId, (String, u64)>>,
    hints: Option<SizeHints>,
}

impl Tips {
    pub(super) fn new(ttl: Duration, hints: Option<SizeHints>) -> Self {
        Tips {
            ttl,
            held: Mutex::new(HashMap::new()),
            blocks: Mutex::new(HashMap::new()),
            last: Mutex::new(HashMap::new()),
            sizes: Mutex::new(HashMap::new()),
            hints,
        }
    }

    /// The held answer for `wiki`, if it is younger than the hold: `(view, state id)` of the view, or `None`
    /// inside the answer when the view is not one of the open ones.
    pub(super) fn fresh(&self, wiki: &WikiId, view: &str) -> Option<Option<ViewState>> {
        let held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        let h = held.get(wiki).filter(|h| h.at.elapsed() < self.ttl)?;
        Some(
            h.views
                .iter()
                .find(|(name, _)| name == view)
                .map(|(name, state)| (name.clone(), state.clone())),
        )
    }

    /// The state id `view` had the last time prikk was asked, however long ago, even if the hold was forgotten
    /// since. `None` when this process has never seen the view.
    pub(super) fn last_known(&self, wiki: &WikiId, view: &str) -> Option<String> {
        let last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        last.get(wiki)?
            .iter()
            .find(|(name, _)| name == view)
            .map(|(_, state)| state.clone())
    }

    /// The log named a newer state for `view` than the list held: hold that one, so that the cache of the
    /// tip the reads are about to fill is kept, not pruned as a past one.
    pub(super) fn moved(&self, wiki: &WikiId, view: &str, state: &str) {
        if let Some(pairs) = self
            .last
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(wiki)
        {
            for (name, current) in pairs {
                if name == view {
                    state.clone_into(current);
                }
            }
        }
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(h) = held.get_mut(wiki) {
            for (name, current) in &mut h.views {
                if name == view {
                    state.clone_into(current);
                }
            }
        }
    }

    /// The block `state` of `view` belongs to, when it has been asked for.
    pub(super) fn block(&self, wiki: &WikiId, view: &str, state: &str) -> Option<String> {
        let blocks = self.blocks.lock().unwrap_or_else(PoisonError::into_inner);
        blocks
            .get(wiki)?
            .get(&(view.to_owned(), state.to_owned()))
            .cloned()
    }

    /// Remember the block `state` of `view` belongs to.
    pub(super) fn remember_block(&self, wiki: &WikiId, view: &str, state: &str, block: &str) {
        let mut blocks = self.blocks.lock().unwrap_or_else(PoisonError::into_inner);
        blocks
            .entry(wiki.clone())
            .or_default()
            .insert((view.to_owned(), state.to_owned()), block.to_owned());
    }

    /// Hold a fresh `branch list` answer. When the views have moved since the store was last measured, the
    /// figure is dropped and walked again here, once, and the driver is told (the walk is the truth after a
    /// write; the hint is what spares the calls after it).
    pub(super) fn hold(&self, wiki: &WikiId, repo: &Path, views: &Views) {
        let held = Held::from(views);
        let fingerprint = held.fingerprint();
        self.last
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(wiki.clone(), held.views.clone());
        // Only the states the views have now can be asked for again.
        if let Some(known) = self
            .blocks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(wiki)
        {
            known
                .retain(|(view, state), _| held.views.iter().any(|(v, s)| v == view && s == state));
        }
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
        self.blocks
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

impl ReadService {
    /// The tip of `view` (the wiki's default view when `None`). Held for [`ReadConfig::tip_ttl`](super::ReadConfig::tip_ttl): a
    /// held answer makes no prikk call. **The one place a [`Tip`] is made.**
    pub fn tip(&self, wiki: &WikiRecord, view: Option<&str>) -> Result<Tip, ReadError> {
        let view = view.unwrap_or(&wiki.default_view);
        let repo = self.layout.repo_dir(&wiki.id);
        let (name, state) = match self.tips.fresh(&wiki.id, view) {
            Some(held) => held.ok_or_else(|| ReadError::NoSuchView(view.to_owned()))?,
            None => {
                let views = self
                    .storage
                    .views(&repo)
                    .map_err(|e| self.storage_error(wiki, e))?;
                self.tips.hold(&wiki.id, &repo, &views);
                views
                    .views
                    .iter()
                    .find(|v| v.name == view && !v.closed)
                    .map(|v| (v.name.clone(), v.state_id.clone()))
                    .ok_or_else(|| ReadError::NoSuchView(view.to_owned()))?
            }
        };
        if let Some(block_id) = self.tips.block(&wiki.id, &name, &state) {
            return Ok(Tip {
                view: name,
                state_id: state,
                block_id,
                seal: Seal,
            });
        }
        // `branch list` names the state, not the block: ask once, here, for the pair. If the view moved
        // since the list, the pair the log gives is the newer tip, whole.
        let newest = self
            .storage
            .history(&repo, &Point::View(name.clone()), Some(1))
            .map_err(|e| self.storage_error(wiki, e))?;
        let block = newest.blocks.into_iter().next().ok_or_else(|| {
            ReadError::CouldNotBeReadNow(Cause::Scratch(format!(
                "the log of `{name}` lists no block"
            )))
        })?;
        if block.ref_state_id != state {
            self.tips.moved(&wiki.id, &name, &block.ref_state_id);
        }
        self.tips
            .remember_block(&wiki.id, &name, &block.ref_state_id, &block.block_id);
        Ok(Tip {
            view: name,
            state_id: block.ref_state_id,
            block_id: block.block_id,
            seal: Seal,
        })
    }
}

#[cfg(test)]
mod tests;
