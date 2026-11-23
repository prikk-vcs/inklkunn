//! The addresses of moderation, under the reserved prefix `-/` so that no page title can shadow them, and the names of the
//! fields a form and a handler agree on. Code, not words.

use inklkunn_core::tombstone::Item;
use inklkunn_store::{CommentId, ProposalId};

use crate::url::encode_title;

/// `/<slug>/-/moderation`: the moderation log.
pub(crate) fn log(slug: &str) -> String {
    format!("/{slug}/-/moderation")
}

/// `/<slug>/-/moderation?before=<id>`: the log, from the entries older than one.
pub(crate) fn log_before(slug: &str, before: &str) -> String {
    format!("/{slug}/-/moderation?before={before}")
}

/// `/<slug>/-/moderate`: the form that hides, and the act.
pub(crate) fn form(slug: &str) -> String {
    format!("/{slug}/-/moderate")
}

/// `/<slug>/-/moderate/unhide`: the act that restores what a hide hid.
pub(crate) fn unhide(slug: &str) -> String {
    format!("/{slug}/-/moderate/unhide")
}

/// `/<slug>/-/moderate/show`: the act that shows hidden content.
pub(crate) fn show(slug: &str) -> String {
    format!("/{slug}/-/moderate/show")
}

/// The names of the fields and of the query.
pub(crate) mod fields {
    /// What kind of item: one of the four values below.
    pub(crate) const KIND: &str = "kind";
    pub(crate) const KIND_PAGE_REVISION: &str = "page-revision";
    pub(crate) const KIND_PROPOSAL_REVISION: &str = "proposal-revision";
    pub(crate) const KIND_PROPOSAL_COMMENT: &str = "proposal-comment";
    pub(crate) const KIND_CLAIM_COMMENT: &str = "claim-comment";
    /// A change's block id.
    pub(crate) const BLOCK: &str = "block";
    /// The view a change is read in, when it is not the default one.
    pub(crate) const VIEW: &str = "view";
    /// A proposal's id.
    pub(crate) const PROPOSAL: &str = "proposal";
    /// A revision's number.
    pub(crate) const SEQ: &str = "seq";
    /// A comment's id.
    pub(crate) const COMMENT: &str = "comment";
    /// A claim's id.
    pub(crate) const CLAIM: &str = "claim";
    /// The parts to hide: each is `1` when chosen.
    pub(crate) const PART_CONTENT: &str = "part_content";
    pub(crate) const PART_SUMMARY: &str = "part_summary";
    pub(crate) const PART_AUTHOR: &str = "part_author";
    /// The reason category.
    pub(crate) const CATEGORY: &str = "category";
    /// The note.
    pub(crate) const NOTE: &str = "note";
    /// The hide an act restores or shows.
    pub(crate) const HIDE: &str = "hide";
}

/// The address of the form for an item: the query that names it. **The one place an item is written into an address.**
pub(crate) fn item_query(item: &Item) -> String {
    use fields as f;
    match item {
        Item::PageRevision { block, view } => {
            let mut q = format!("{}={}&{}={block}", f::KIND, f::KIND_PAGE_REVISION, f::BLOCK);
            if let Some(view) = view {
                q.push_str(&format!("&{}={}", f::VIEW, encode_title(view)));
            }
            q
        }
        Item::ProposalRevision { proposal, seq } => format!(
            "{}={}&{}={proposal}&{}={seq}",
            f::KIND,
            f::KIND_PROPOSAL_REVISION,
            f::PROPOSAL,
            f::SEQ
        ),
        Item::ProposalComment { proposal, comment } => format!(
            "{}={}&{}={proposal}&{}={comment}",
            f::KIND,
            f::KIND_PROPOSAL_COMMENT,
            f::PROPOSAL,
            f::COMMENT
        ),
        Item::ClaimComment { claim, comment } => format!(
            "{}={}&{}={claim}&{}={comment}",
            f::KIND,
            f::KIND_CLAIM_COMMENT,
            f::CLAIM,
            f::COMMENT
        ),
    }
}

/// `/<slug>/-/moderate?<the item>`: the form for an item.
pub(crate) fn form_for(slug: &str, item: &Item) -> String {
    format!("{}?{}", form(slug), item_query(item))
}

/// The item a form's fields (or a query) name, if they name one.
pub(crate) fn item_of(get: &dyn Fn(&str) -> Option<String>) -> Option<Item> {
    use fields as f;
    match get(f::KIND)?.as_str() {
        f::KIND_PAGE_REVISION => Some(Item::PageRevision {
            block: get(f::BLOCK)?,
            view: get(f::VIEW).filter(|v| !v.is_empty()),
        }),
        f::KIND_PROPOSAL_REVISION => Some(Item::ProposalRevision {
            proposal: ProposalId::parse(&get(f::PROPOSAL)?)?,
            seq: get(f::SEQ)?.parse().ok()?,
        }),
        f::KIND_PROPOSAL_COMMENT => Some(Item::ProposalComment {
            proposal: ProposalId::parse(&get(f::PROPOSAL)?)?,
            comment: CommentId::parse(&get(f::COMMENT)?)?,
        }),
        f::KIND_CLAIM_COMMENT => Some(Item::ClaimComment {
            claim: get(f::CLAIM)?,
            comment: CommentId::parse(&get(f::COMMENT)?)?,
        }),
        _ => None,
    }
}
