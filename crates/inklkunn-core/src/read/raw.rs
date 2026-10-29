//! A page's stored file, byte for byte (RFC 003 D-10; requirements RD-10, API-2): what `-/raw/<Title>` serves
//! from the content origin. Nothing here reads, renders, sanitizes or interprets the bytes: they are the file
//! at the view's tip, read at the tip's block, so the validator names the content.
//!
//! **The validators.** The `ETag` is the tip's block id and the file's path, so it is the same exactly as long
//! as the tip's block is, and different for another page of the same block. It is a strong validator in the
//! sense that matters (the same value means the same bytes), and it changes when an unrelated page changes
//! (the block moved): a cache revalidates more than it must and never serves less than the truth. The
//! `Last-Modified` is when this service first saw the tip's block (see [`super::changes`]: prikk's history
//! carries no time), which is a moment the representation was at the latest as it is now.

use inklkunn_prikk::Point;
use inklkunn_store::WikiRecord;

use super::index::Lookup;
use super::{ReadError, ReadService, Tip};

/// A page's stored file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPage {
    /// The tip it was read at.
    pub tip: Tip,
    /// The file's repository path (the storage path, labelled so wherever it is shown).
    pub path: String,
    /// The page's title.
    pub title: String,
    /// The file's bytes, exactly as stored.
    pub bytes: Vec<u8>,
    /// The entity tag, quoted.
    pub etag: String,
    /// When this service first saw the tip's block, in Unix seconds.
    pub last_modified: i64,
}

/// FNV-1a over the path, to keep the tag short and free of characters a header does not take.
fn path_tag(path: &str) -> u32 {
    path.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    })
}

impl ReadService {
    /// The stored file of the page `title` leads to at the view's tip (the default view when `None`): the page
    /// itself, or, for a redirect that leads nowhere, the redirect page's own file. `Ok(None)` when the title
    /// leads to no page or to several.
    pub fn raw(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        title: &str,
    ) -> Result<Option<RawPage>, ReadError> {
        inklkunn_markup::path::title_to_path(title).map_err(ReadError::InvalidTitle)?;
        let tip = self.tip(wiki, view)?;
        let index = self.index_at(wiki, &tip)?;
        let entry = match index.lookup(title) {
            Lookup::Found(found) => found.entry,
            Lookup::BrokenRedirect { from, .. } => from,
            Lookup::Ambiguous { .. } | Lookup::Missing => return Ok(None),
        };
        let bytes = self
            .storage
            .read(
                &self.layout.repo_dir(&wiki.id),
                &Point::Block(tip.block_id.clone()),
                &entry.path,
                self.config.page_max_bytes,
            )
            .map_err(|e| self.storage_error(wiki, e))?;
        let last_modified = self
            .derived
            .first_seen(
                &wiki.id,
                std::slice::from_ref(&tip.block_id),
                inklkunn_store::record::now_secs(),
            )
            .ok()
            .and_then(|seen| seen.first().copied())
            .unwrap_or_else(inklkunn_store::record::now_secs);
        Ok(Some(RawPage {
            etag: format!("\"{}-{:08x}\"", tip.block_id, path_tag(&entry.path)),
            title: entry.title,
            path: entry.path,
            bytes,
            last_modified,
            tip,
        }))
    }
}
