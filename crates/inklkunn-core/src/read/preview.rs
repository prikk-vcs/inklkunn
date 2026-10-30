//! What a proposal's pages need of the read path (RFC 004 D-2, D-3; handoff 004 T3): a page's source **at a
//! block** (the base a proposal was written against, which is not the tip any more once the view moved) and a
//! page's **preview** (source text that is not in the repository, drawn exactly as the page would be and
//! sanitized by the same pass).
//!
//! Both are reads. Neither writes, neither caches: a preview is the text one person is typing, and the base
//! of a proposal is read once per page view.

use inklkunn_markup::path::title_to_path;
use inklkunn_prikk::{Point, StorageError};
use inklkunn_store::{DerivedHeading, IndexEntry, WikiRecord};

use super::{ReadError, ReadService, SafeHtml, Tip};
use crate::read::Presentation;

/// A page drawn from text that is not in the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preview {
    /// The page's body, sanitized.
    pub html: SafeHtml,
    /// The page's headings.
    pub toc: Vec<DerivedHeading>,
}

impl ReadService {
    /// The file at `path` as it was at the block `block_id`, as text. `Ok(None)` when there was no such file
    /// then, or it is not text. A file larger than a page may be is an error (it could not be read), not a
    /// missing file.
    pub fn source_at(
        &self,
        wiki: &WikiRecord,
        block_id: &str,
        path: &str,
    ) -> Result<Option<String>, ReadError> {
        let repo = self.layout.repo_dir(&wiki.id);
        let point = Point::Block(block_id.to_owned());
        match self
            .storage
            .read(&repo, &point, path, self.config.page_max_bytes)
        {
            Ok(bytes) => Ok(String::from_utf8(bytes).ok()),
            Err(refused @ StorageError::Refused { .. }) => {
                // Either there was no such file then, or it is over the size read: `stat` tells.
                match self.storage.stat(&repo, &point, path) {
                    Err(StorageError::Refused { .. }) => Ok(None),
                    Ok(_) => Err(self.storage_error(wiki, refused)),
                    Err(e) => Err(self.storage_error(wiki, e)),
                }
            }
            Err(e) => Err(self.storage_error(wiki, e)),
        }
    }

    /// Draw `text` as the page titled `title` stored at `path` would be drawn at the tip of the view (the
    /// default view when `None`): its links resolved against the tip's titles, its includes read at the tip,
    /// and the result through the sanitizer. A text whose frontmatter is not readable is
    /// [`ReadError::PageUnreadable`]; nothing is kept.
    pub fn preview(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        path: &str,
        title: &str,
        text: &str,
        presentation: &dyn Presentation,
    ) -> Result<Preview, ReadError> {
        title_to_path(title).map_err(ReadError::InvalidTitle)?;
        let tip: Tip = self.tip(wiki, view)?;
        let index = self.index_at(wiki, &tip)?;
        let entry = IndexEntry {
            path: path.to_owned(),
            title: title.to_owned(),
            aliases: Vec::new(),
            redirect: None,
            page_id: None,
            kind: "page".to_owned(),
            problem: None,
        };
        let row = self.render_text(wiki, &tip, &index, &entry, presentation, text)?;
        Ok(Preview {
            html: self.sanitizer.clean(&row.html),
            toc: row.toc,
        })
    }
}
