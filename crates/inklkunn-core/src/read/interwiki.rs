//! The wiki's interwiki map, read from the default view's tip (RFC 007 D-8; handoff I-T4; correction W-4).
//!
//! **One wiki has one map.** It is `governance/interwiki.yaml` **at the tip of the default view**, for every view and every state,
//! as the other two governance files are (RFC 006 D-1): an alternative view cannot change `governance/`, and a permalink is a
//! page read at an old block, so neither has a map of its own. The same link in two views, or in a page and its permalink, leads
//! to the same place.
//!
//! ```text
//! the default view's tip ─▶ memory (one map per wiki, by the tip's state) ─▶ else ONE listing of `governance/` at its block
//!                                                                            and, where the file is there, ONE read of it
//! ```
//!
//! The map is **not part of a title index**: the index of a state (held in memory and in the derived store) has the pages and no
//! map, and the map is put on a copy that shares the pages ([`TitleIndex::with_shared_interwiki`](super::TitleIndex)) each time an
//! index is asked for. So an index held for a view that did not move never holds a map the default tip has replaced, and a map
//! change at the default tip is seen by the next read of every index. What was drawn with a map is kept under the map's name
//! ([`Interwiki::fingerprint`](crate::governance::Interwiki::fingerprint)): a rendered page, a held permalink and a link graph.
//!
//! **Reading is lenient and writing is not.** A file that is absent, too large, not an ordinary file or not sound is an empty
//! map here, so that every page is still shown, with its prefixed links dangling as before. The rules' loader judges the same file
//! and a fault of it makes every write refuse until it is repaired ([`GovernanceFile::Interwiki`]).
//!
//! **What cannot be read now is not an empty map.** A wiki with no default view (nothing in it yet: an import has not finished)
//! has no map. But when the default view's tip, its `governance/` or the file cannot be asked for (prikk busy, a timeout), the
//! read fails as every read does, *could not be read now*: a page is never shown with links resolved by another map than the
//! wiki's.

use std::sync::{Arc, PoisonError};

use inklkunn_prikk::Point;
use inklkunn_store::{WikiId, WikiRecord};

use super::rules::{Shape, shape_at};
use super::{ReadError, ReadService, Tip};
use crate::governance::{GOVERNANCE_DIR, GovernanceFile, Interwiki};

impl ReadService {
    /// The wiki's interwiki map: **the one of the default view's tip** (see the module's account), read once per default tip.
    ///
    /// A new default tip costs one listing of `governance/` and, where the file is there, one read of it (nothing when an index of
    /// that tip was just built from an extract, which holds the file); the map of a tip that was read is held until the tip moves,
    /// and the map of a tip whose file is the same as the last one's is the same `Arc`.
    pub fn interwiki(&self, wiki: &WikiRecord) -> Result<Arc<Interwiki>, ReadError> {
        match self.tip(wiki, None) {
            Ok(tip) => self.interwiki_at(wiki, &tip),
            // No default view yet: nothing has been written, and there is no map to apply.
            Err(ReadError::NoSuchView(_)) => Ok(Arc::new(Interwiki::empty())),
            Err(e) => Err(e),
        }
    }

    /// [`ReadService::interwiki`] for a tip of the default view that was already read.
    pub(super) fn interwiki_at(
        &self,
        wiki: &WikiRecord,
        tip: &Tip,
    ) -> Result<Arc<Interwiki>, ReadError> {
        if let Some(held) = self.held_interwiki(&wiki.id, tip.state_id()) {
            return Ok(held);
        }
        let read = self.read_interwiki_at(wiki, tip.block_id())?;
        Ok(self.hold_interwiki(&wiki.id, tip.state_id(), read))
    }

    /// The map held for the default view's state `state`, if it is the one held.
    fn held_interwiki(&self, wiki: &WikiId, state: &str) -> Option<Arc<Interwiki>> {
        let held = self
            .interwiki_maps
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        held.get(wiki)
            .filter(|(held_state, _)| held_state == state)
            .map(|(_, map)| Arc::clone(map))
    }

    /// Hold `read` as the wiki's map at the default view's state `state`. **The same content is the same `Arc`**: what is kept by
    /// identity (a link graph) is not rebuilt for a tip that left the map alone.
    pub(super) fn hold_interwiki(
        &self,
        wiki: &WikiId,
        state: &str,
        read: Interwiki,
    ) -> Arc<Interwiki> {
        let mut held = self
            .interwiki_maps
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let map = match held.get(wiki) {
            Some((_, previous)) if **previous == read => Arc::clone(previous),
            _ => Arc::new(read),
        };
        held.insert(wiki.clone(), (state.to_owned(), Arc::clone(&map)));
        map
    }

    /// The map this process last had for the wiki, **asking prikk nothing** (empty when it never had one): what a page that is served
    /// without asking ([`ReadService::stale_page`]) is looked up and keyed by.
    pub(super) fn last_known_interwiki(&self, wiki: &WikiId) -> Arc<Interwiki> {
        self.interwiki_maps
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(wiki)
            .map_or_else(|| Arc::new(Interwiki::empty()), |(_, map)| Arc::clone(map))
    }

    /// `governance/interwiki.yaml` at `block_id`: one listing of `governance/` (what the rules' loader asks too) and, when an
    /// ordinary file of at most [`MAX_INTERWIKI_BYTES`] is there, one read of it. **An empty map** for a file that is absent, not an
    /// ordinary file, too large, not UTF-8 or not sound (the rules' loader reports it, and every write refuses while it stands); an
    /// error for a repository that could not be asked.
    fn read_interwiki_at(&self, wiki: &WikiRecord, block_id: &str) -> Result<Interwiki, ReadError> {
        let point = Point::Block(block_id.to_owned());
        let listing = self
            .storage
            .tree(&self.layout.repo_dir(&wiki.id), &point, Some(GOVERNANCE_DIR))
            .map_err(|e| self.storage_error(wiki, e))?;
        match shape_at(&listing.entries, GovernanceFile::Interwiki) {
            Shape::File(entry) => self.interwiki_file_at(wiki, &point, entry),
            Shape::Absent | Shape::NotAFile => Ok(Interwiki::empty()),
        }
    }

    /// The interwiki map the links of every view and state are resolved with: the wiki's, **the one of the default view's tip**.
    ///
    /// `view` is **not used**: the map does not depend on it (correction W-4). The parameter stays so that a caller written for the
    /// map of a view's state needs no change; [`ReadService::interwiki`] is the same without it.
    pub fn interwiki_of(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
    ) -> Result<Interwiki, ReadError> {
        let _ = view;
        Ok(self.interwiki(wiki)?.as_ref().clone())
    }
}
