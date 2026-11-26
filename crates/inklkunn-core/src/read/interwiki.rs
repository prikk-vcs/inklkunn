//! The interwiki map of a state, read from the repository when no extract is at hand (RFC 007 D-8; handoff I-T4).
//!
//! **The map is part of the state.** A title index is of one state of one view, and the map it resolves links with is that
//! state's own `governance/interwiki.yaml`: a page at a permalink is read with the links it had, and an alternative view (which
//! cannot change `governance/`, RFC 006 D-1) carries the map it was made from. Nothing is held beside the index: the map is in
//! it ([`TitleIndex::interwiki`](super::TitleIndex::interwiki)), found by the build in the extract it already reads, or here
//! when the index came from the derived store (a row that holds pages and not the map).
//!
//! **Reading is lenient and writing is not.** A file that is absent, too large, not an ordinary file or not sound is an empty
//! map here, so that every page is still shown, with its prefixed links dangling as before. The rules' loader judges the same file
//! and a fault of it makes every write refuse until it is repaired ([`GovernanceFile::Interwiki`](crate::governance)).

use inklkunn_prikk::{Point, StorageError};
use inklkunn_store::WikiRecord;

use super::{ReadError, ReadService};
use crate::governance::{INTERWIKI_PATH, Interwiki, MAX_INTERWIKI_BYTES, read_interwiki};

impl ReadService {
    /// The interwiki map of the state at `block_id`: its `governance/interwiki.yaml`, or an empty map (see the module's account).
    /// An error is *the repository could not be asked*, never an empty map.
    pub(super) fn interwiki_at(
        &self,
        wiki: &WikiRecord,
        block_id: &str,
    ) -> Result<Interwiki, ReadError> {
        let max = u64::try_from(MAX_INTERWIKI_BYTES).map_or(u64::MAX, |n| n.saturating_add(1));
        let read = self.storage.read(
            &self.layout.repo_dir(&wiki.id),
            &Point::Block(block_id.to_owned()),
            INTERWIKI_PATH,
            max,
        );
        match read {
            Ok(bytes) => Ok(read_interwiki(&bytes).unwrap_or_default()),
            // Not there, or larger than a map may be: no map.
            Err(StorageError::Refused { .. }) => Ok(Interwiki::empty()),
            Err(e) => Err(self.storage_error(wiki, e)),
        }
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
