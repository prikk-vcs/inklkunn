//! The interwiki map of a state, and where an index of the derived store may not stand in for it (RFC 007 D-8; handoff I-T4).
//!
//! **The map is part of the state.** A title index is of one state of one view, and the map it resolves links with is that
//! state's own `governance/interwiki.yaml`: a page at a permalink is read with the links it had, and an alternative view (which
//! cannot change `governance/`, RFC 006 D-1) carries the map it was made from. Nothing is held beside the index: the map is in
//! it ([`TitleIndex::interwiki`](super::TitleIndex::interwiki)), found by the build in the extract it already reads. An index that
//! came from the derived store (a row that holds pages and not the map) is used only for a state that has no map file
//! ([`ReadService::has_interwiki_at`]); one that has is built again.
//!
//! **Reading is lenient and writing is not.** A file that is absent, too large, not an ordinary file or not sound is an empty
//! map here, so that every page is still shown, with its prefixed links dangling as before. The rules' loader judges the same file
//! and a fault of it makes every write refuse until it is repaired ([`GovernanceFile::Interwiki`](crate::governance)).

use inklkunn_prikk::Point;
use inklkunn_store::WikiRecord;

use super::{ReadError, ReadService};
use crate::governance::{GOVERNANCE_DIR, INTERWIKI_PATH, Interwiki};

impl ReadService {
    /// Does the state at `block_id` have anything at the place of `governance/interwiki.yaml`: one listing of `governance/`, which
    /// is what the rules' loader asks too. **No file's bytes are read here** (the one reader of a block's content is `text_at`, and
    /// the governance files are read by the rules' loader and by the extract of a build): an index that came from the derived store
    /// holds pages and not the map, and a state that has a file there is **built again from the extract**, which reads it, so
    /// that the derived row never has to hold what it does not (and no stored row changes for the map). A state with none, which is
    /// every wiki that has no map, costs the listing and nothing more.
    pub(super) fn has_interwiki_at(
        &self,
        wiki: &WikiRecord,
        block_id: &str,
    ) -> Result<bool, ReadError> {
        let listing = self
            .storage
            .tree(
                &self.layout.repo_dir(&wiki.id),
                &Point::Block(block_id.to_owned()),
                Some(GOVERNANCE_DIR),
            )
            .map_err(|e| self.storage_error(wiki, e))?;
        let below = format!("{INTERWIKI_PATH}/");
        Ok(listing
            .entries
            .iter()
            .any(|e| e.path == INTERWIKI_PATH || e.path.starts_with(&below)))
    }

    /// The interwiki map the links of the view's tip are resolved with: the map of the state the title index is of.
    pub fn interwiki_of(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
    ) -> Result<Interwiki, ReadError> {
        Ok(self.title_index(wiki, view)?.interwiki().clone())
    }
}
