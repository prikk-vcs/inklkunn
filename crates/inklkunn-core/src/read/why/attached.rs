//! The sources a change attached (RFC 005 part 2 D-8: *the sources attached in that change*; maintenance M56).
//!
//! **Derived from the change and the files it wrote, and nothing else.** A source is attached by a change that
//! *creates* a file under `knowledge/sources/`: the paths come from the change's own record (the one the history
//! page reads; a block it inspected costs no second `show`), and each file is read at that very block, so what is
//! said is what the change wrote, not what the source says now. A file that cannot be read as a source (not
//! UTF-8, over the size read, a text that holds a bidirectional control, an id that is not one, a field the format
//! refuses) is **counted, never shown**: the same reading that refuses it at acceptance refuses it here.
//!
//! **Bounded.** A request names how many files it may read for one change; the rest are counted. A change too
//! large for prikk's report to list is said to be (what it attached is not known), never read as having none.

use inklkunn_store::WikiRecord;

use super::super::text::FileAt;
use super::super::{ReadError, ReadService};
use crate::knowledge::{Family, File, MAX_FILE_BYTES, family_of, read_file};
use crate::tombstone::Mark;

/// A source file a change created, as the change wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedSource {
    /// The source's id (the file's name).
    pub id: String,
    /// Where it is found, as written: untrusted text, never fetched and never made a link.
    pub reference: Option<String>,
}

/// What a change attached.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attached {
    /// The source files it created that were read, in the order the change says them.
    pub sources: Vec<AttachedSource>,
    /// How many of them were read and could not be read as a source.
    pub unreadable: usize,
    /// How many more it created than were read in this request.
    pub omitted: usize,
    /// The change is too large to list what it did: what it attached is not known.
    pub too_large: bool,
    /// **What the change attached is hidden** (RFC 006 D-8; H-4): the text of the source files it created is a hidden change's, so
    /// none of it is read or said, and this is the hiding. `sources` and `unreadable` do not count those files.
    pub hidden: Option<Mark>,
}

impl ReadService {
    /// The source files the change `block_id` created, read at that block: at most `limit` of them are read, the
    /// rest are counted in [`Attached::omitted`].
    pub fn attached_sources(
        &self,
        wiki: &WikiRecord,
        block_id: &str,
        limit: usize,
    ) -> Result<Attached, ReadError> {
        let Some(created) = self.created_paths(wiki, block_id)? else {
            return Ok(Attached {
                too_large: true,
                ..Attached::default()
            });
        };
        let wanted: Vec<&String> = created
            .iter()
            .filter(|path| family_of(path) == Some(Family::Source))
            .collect();
        let mut out = Attached {
            omitted: wanted.len().saturating_sub(limit),
            ..Attached::default()
        };
        for path in wanted.into_iter().take(limit) {
            let bytes = match self.file_at(wiki, None, block_id, path, MAX_FILE_BYTES as u64)? {
                FileAt::Held(bytes) => bytes,
                // No such file at the block, or one over the size a knowledge file may have.
                FileAt::Missing | FileAt::TooLarge(_) => {
                    out.unreadable += 1;
                    continue;
                }
                // The change that wrote it has its content hidden: what it attached is not quoted (RFC 006 D-8).
                FileAt::Hidden(mark) => {
                    out.hidden.get_or_insert(*mark);
                    continue;
                }
            };
            match read_file(path, &bytes) {
                Ok(File::Source(source)) => out.sources.push(AttachedSource {
                    id: source.id.to_string(),
                    reference: source.reference.map(|r| r.value),
                }),
                Ok(File::Claim(_)) | Err(_) => out.unreadable += 1,
            }
        }
        Ok(out)
    }
}
