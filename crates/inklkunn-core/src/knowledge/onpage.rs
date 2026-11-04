//! What the knowledge layer holds on a page, for the forms that propose a change to it (RFC 005 part 2 D-3).
//!
//! Read from the derived knowledge index of the view's tip (one question to a cached index, no repository call
//! for a page that holds no claim) and, for the one claim a form works on, from its file at the proposal's base.
//! Never a second source of truth: the files are.

use std::collections::BTreeSet;

use inklkunn_store::WikiRecord;

use super::compose::page_id_of;
use super::{Claim, ClaimId, File, claim_path, read_file};
use crate::read::{ReadError, ReadService};

/// The most claims one page is asked about. A page that holds more is a page whose claims the forms hold the
/// first of; the rest stay as written in the text (a marker never hidden is never lost).
pub const MAX_CLAIMS_ON_A_PAGE: usize = 500;

/// The `^id`s of the blocks of `page_text` that are a claim's expression at the tip of `view`: the markers a form
/// holds. Empty for a page with no `id` in its information block (no claim can name it).
pub fn claim_blocks(
    reads: &ReadService,
    wiki: &WikiRecord,
    view: Option<&str>,
    page_text: &str,
) -> Result<BTreeSet<String>, ReadError> {
    let Some(page_id) = page_id_of(page_text) else {
        return Ok(BTreeSet::new());
    };
    let index = reads.knowledge_index(wiki, view)?;
    Ok(index
        .rows()
        .anchors
        .iter()
        .filter(|a| a.page_id == page_id)
        .take(MAX_CLAIMS_ON_A_PAGE)
        .map(|a| a.block.clone())
        .collect())
}

/// The claims said at the block `block` of the page `page_id`, read from their files as they were at `base_block`
/// (the base a proposal is written against). A claim the tip names and the base does not have, or whose file the
/// base holds unreadable, is not among them.
pub fn claims_at(
    reads: &ReadService,
    wiki: &WikiRecord,
    view: Option<&str>,
    base_block: &str,
    page_id: &str,
    block: &str,
) -> Result<Vec<Claim>, ReadError> {
    let index = reads.knowledge_index(wiki, view)?;
    let mut out = Vec::new();
    for anchor in index.claims_at(page_id, block) {
        let Some(id) = ClaimId::parse(&anchor.claim) else {
            continue;
        };
        let path = claim_path(&id);
        let Some(text) = reads.source_at(wiki, base_block, &path)? else {
            continue;
        };
        if let Ok(File::Claim(claim)) = read_file(&path, text.as_bytes()) {
            out.push(*claim);
        }
    }
    Ok(out)
}

/// The claims a proposal's own knowledge files hold (`files`: path and text), which the tip does not have yet.
pub fn claims_in(files: &[(String, String)]) -> Vec<Claim> {
    files
        .iter()
        .filter_map(|(path, text)| match read_file(path, text.as_bytes()) {
            Ok(File::Claim(claim)) => Some(*claim),
            _ => None,
        })
        .collect()
}

/// The `^id`s of the blocks of the page `page_id` that these claims say.
pub fn blocks_said(claims: &[Claim], page_id: &str) -> BTreeSet<String> {
    claims
        .iter()
        .flat_map(|c| c.expressions.iter())
        .filter(|e| e.page_id == page_id)
        .map(|e| e.block.clone())
        .collect()
}
