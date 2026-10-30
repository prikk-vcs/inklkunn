//! What a proposal changes, as a page shows it (RFC 004 D-3; WEB-04): per page its status, the information at
//! the top of the page if that changed, then each part that differs with its lines (the history's diff
//! markup, `linediff`'s word marks included), a count of the parts that did not change, and, on a proposal's
//! page, a link to comment on a part and the comments that landed on it.
//!
//! Every line is text from a proposal and is printed by a template, which escapes it; nothing here assembles
//! markup.

use super::changes::{How, PageDelta, PartDelta, Status, delta, hunk};
use crate::catalog::{Key, T};
use crate::history::{HunkTpl, hunk_view};

/// A page to compare: where it is, what it is called, and its text on each side (`None`: not there).
pub(crate) struct PageInput<'a> {
    pub(crate) path: &'a str,
    pub(crate) title: String,
    pub(crate) old: Option<&'a str>,
    pub(crate) new: Option<&'a str>,
}

/// One comment, as the page shows it.
pub(crate) struct CommentView {
    pub(crate) id: String,
    /// How deep a reply is nested (0: a comment of its own).
    pub(crate) depth: usize,
    pub(crate) author: String,
    pub(crate) meta: String,
    pub(crate) body: String,
    /// Where the comment is anchored, when it is and it is not shown beside the part.
    pub(crate) location: Option<String>,
    /// The text an anchored comment was about.
    pub(crate) quote: Option<String>,
    /// Said of a comment whose anchor matches nothing any more.
    pub(crate) orphaned: Option<String>,
    pub(crate) reply: Option<LinkView>,
}

/// A link.
pub(crate) struct LinkView {
    pub(crate) href: String,
    pub(crate) label: String,
}

/// One part that differs.
pub(crate) struct PartView {
    pub(crate) id: String,
    pub(crate) heading: String,
    pub(crate) hunks: Vec<HunkTpl>,
    pub(crate) truncated: Option<String>,
    pub(crate) comment: Option<LinkView>,
    pub(crate) comments: Vec<CommentView>,
}

/// One page of a proposal.
pub(crate) struct PageChangeView {
    pub(crate) id: String,
    pub(crate) heading: String,
    pub(crate) status: String,
    pub(crate) info: Option<PartView>,
    pub(crate) parts: Vec<PartView>,
    pub(crate) unchanged: Option<String>,
}

/// All of it.
pub(crate) struct ChangesView {
    pub(crate) pages: Vec<PageChangeView>,
    pub(crate) added_label: String,
    pub(crate) removed_label: String,
    pub(crate) refinement: Option<String>,
    /// There is nothing to show: every page is the page as it is.
    pub(crate) nothing: Option<String>,
}

/// What a proposal's page adds to the changes: where a part is commented on, and the comments that landed
/// beside it.
pub(crate) trait Beside {
    /// The link that comments on the `ordinal`-th part of the `index`-th page.
    fn comment_link(&self, index: usize, ordinal: usize) -> Option<LinkView>;
    /// The comments that landed on the `ordinal`-th part of the page at `path`.
    fn comments_at(&self, path: &str, ordinal: usize) -> Vec<CommentView>;
}

/// No links and no comments: the changes a form shows.
pub(crate) struct Alone;

impl Beside for Alone {
    fn comment_link(&self, _: usize, _: usize) -> Option<LinkView> {
        None
    }
    fn comments_at(&self, _: &str, _: usize) -> Vec<CommentView> {
        Vec::new()
    }
}

fn hunks_of(t: T, old: &str, new: &str) -> (Vec<HunkTpl>, bool) {
    let (view, truncated) = hunk(old, new);
    (vec![hunk_view(t, &view)], truncated)
}

fn marked(hunks: &[HunkTpl]) -> bool {
    hunks
        .iter()
        .any(|h| h.lines.iter().any(|l| l.segments.iter().any(|s| s.marked)))
}

fn part_view(
    t: T,
    id: String,
    heading: String,
    part: Option<&PartDelta>,
    info: Option<(&str, &str)>,
) -> PartView {
    let (old, new) = match (part, info) {
        (Some(p), _) => (
            p.old.as_deref().unwrap_or(""),
            p.new.as_deref().unwrap_or(""),
        ),
        (None, Some((a, b))) => (a, b),
        (None, None) => ("", ""),
    };
    let (hunks, truncated) = hunks_of(t, old, new);
    PartView {
        id,
        heading,
        hunks,
        truncated: truncated.then(|| {
            t.msg(Key::DiffTruncated)
                .with("limit", inklkunn_core::read::MAX_DIFF_LINES)
                .text()
        }),
        comment: None,
        comments: Vec::new(),
    }
}

/// The changes of `inputs`, shown for `beside`.
pub(crate) fn build(t: T, inputs: &[PageInput<'_>], beside: &dyn Beside) -> ChangesView {
    let mut pages = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        let d: PageDelta = delta(input.old, input.new);
        if d.status == Status::Unchanged {
            continue;
        }
        let page_id = format!("c{index}");
        let status = t.get(match d.status {
            Status::Added => Key::ChangesStatusAdded,
            Status::Removed => Key::ChangesStatusRemoved,
            Status::Modified | Status::Unchanged => Key::ChangesStatusModified,
        });
        let info = d.info.as_ref().map(|(a, b)| {
            part_view(
                t,
                format!("{page_id}-info"),
                t.get(Key::ChangesInfoHeading),
                None,
                Some((a, b)),
            )
        });
        let parts = d
            .parts
            .iter()
            .enumerate()
            .map(|(k, p)| {
                let number = p.ordinal.or(p.old_ordinal).map_or(0, |o| o + 1);
                let key = match p.how {
                    How::Changed => Key::ChangesPartChanged,
                    How::Added => Key::ChangesPartAdded,
                    How::Removed => Key::ChangesPartRemoved,
                };
                let mut view = part_view(
                    t,
                    format!("{page_id}-p{k}"),
                    t.msg(key).with("number", number).text(),
                    Some(p),
                    None,
                );
                if let Some(ordinal) = p.ordinal {
                    view.comment = beside.comment_link(index, ordinal);
                    view.comments = beside.comments_at(input.path, ordinal);
                }
                view
            })
            .collect();
        pages.push(PageChangeView {
            id: page_id,
            heading: t
                .msg(Key::ChangesPageHeading)
                .with("title", &input.title)
                .text(),
            status,
            info,
            parts,
            unchanged: (d.unchanged > 0).then(|| {
                t.msg(Key::ChangesUnchangedParts)
                    .n(d.unchanged as u64)
                    .text()
            }),
        });
    }
    let refinement = pages
        .iter()
        .flat_map(|p| p.info.iter().chain(p.parts.iter()))
        .any(|p| marked(&p.hunks))
        .then(|| t.get(Key::DiffRefinement));
    ChangesView {
        nothing: pages.is_empty().then(|| t.get(Key::ProposalChangesNone)),
        pages,
        added_label: t.get(Key::DiffAdded),
        removed_label: t.get(Key::DiffRemoved),
        refinement,
    }
}

/// The fragment a part's block is addressed by on a proposal's page, for the comment that answers it.
pub(crate) fn on_value(index: usize, ordinal: usize) -> String {
    format!("{index}-{ordinal}")
}

/// Read what [`on_value`] wrote.
pub(crate) fn on_parse(value: &str) -> Option<(usize, usize)> {
    let (a, b) = value.split_once('-')?;
    Some((a.parse().ok()?, b.parse().ok()?))
}
