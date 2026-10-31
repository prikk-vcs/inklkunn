//! Where an acceptance stopped because the wiki and the proposal changed the same text (RFC 004 D-4 step 3,
//! D-5): both texts side by side, so the proposer revises with both in front of them. An overlap is never
//! resolved by the service. The records are the acceptance's (`ActStore::overlaps_of`); they are shown on a
//! proposal that *needs a revision*, for the revision they were found for, and on no other.

use inklkunn_store::{OverlapRecord, OverlapText, ProposalRecord, ProposalState};

use super::Ctx;
use crate::catalog::{Key, T};

/// One overlapping place: where, and the text on each side.
pub(crate) struct OverlapRow {
    pub(crate) place: String,
    pub(crate) theirs: String,
    pub(crate) ours: String,
}

/// The section of overlaps.
pub(crate) struct OverlapsView {
    pub(crate) heading: String,
    pub(crate) intro: String,
    pub(crate) caption: String,
    pub(crate) col_where: String,
    pub(crate) col_theirs: String,
    pub(crate) col_ours: String,
    pub(crate) rows: Vec<OverlapRow>,
}

/// The overlaps to show on the proposal, if its state and its latest revision have any.
pub(crate) fn view(
    ctx: &Ctx,
    t: T,
    record: &ProposalRecord,
    latest: Option<u32>,
) -> Option<OverlapsView> {
    if record.state != ProposalState::NeedsRevision {
        return None;
    }
    let found = match ctx.services.acts.overlaps_of(&record.id) {
        Ok(found) => found,
        Err(error) => {
            ctx.log(&format!(
                "the overlaps of a proposal could not be read: {error}"
            ));
            return None;
        }
    };
    let found: OverlapRecord = found
        .into_iter()
        .rev()
        .find(|o| Some(o.revision) == latest)?;
    if found.texts.is_empty() {
        return None;
    }
    Some(OverlapsView {
        heading: t.get(Key::OverlapHeading),
        intro: t.get(Key::OverlapIntro),
        caption: t.get(Key::OverlapCaption),
        col_where: t.get(Key::OverlapColWhere),
        col_theirs: t.get(Key::OverlapColTheirs),
        col_ours: t.get(Key::OverlapColOurs),
        rows: found.texts.iter().map(|o| row(t, o)).collect(),
    })
}

fn row(t: T, o: &OverlapText) -> OverlapRow {
    let title = inklkunn_markup::path::path_to_title(&o.page).unwrap_or_else(|| o.page.clone());
    let side = |text: &Option<String>| text.clone().unwrap_or_else(|| t.get(Key::OverlapNothing));
    OverlapRow {
        place: t
            .msg(Key::OverlapRow)
            .with("page", title)
            .with("place", place(t, &o.place))
            .text(),
        theirs: side(&o.theirs),
        ours: side(&o.ours),
    }
}

/// A place the acceptance recorded (`frontmatter`, `frontmatter <key>`, `block ^<id>`, `block <n>`, `block`,
/// `page`, `path`) in words. A description this build does not know is the plainest one: a part of the page.
fn place(t: T, described: &str) -> String {
    match described {
        "frontmatter" => t.get(Key::OverlapPlaceTop),
        "page" => t.get(Key::OverlapPlacePage),
        "path" => t.get(Key::OverlapPlacePath),
        other => {
            if let Some(key) = other.strip_prefix("frontmatter ") {
                t.msg(Key::OverlapPlaceKey).with("key", key).text()
            } else if let Some(id) = other.strip_prefix("block ^") {
                t.msg(Key::OverlapPlaceNamed).with("id", id).text()
            } else if let Some(n) = other
                .strip_prefix("block ")
                .and_then(|n| n.parse::<usize>().ok())
            {
                t.msg(Key::OverlapPlaceNumber).with("number", n).text()
            } else {
                t.get(Key::OverlapPlacePart)
            }
        }
    }
}
