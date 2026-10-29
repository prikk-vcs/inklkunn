//! A page's past for the API (handoff 003 T8): the history walked by cursor, and a diff whose points may be
//! left out. Both are the page views' own reads ([`ReadService::page_history`], [`ReadService::page_diff`]);
//! this file only turns the history's position into a cursor and fills in the points a request did not name.

use inklkunn_store::{IndexEntry, WikiRecord};

use super::{ApiError, Cursor, Position};
use crate::read::{ChangeOutcome, DiffOutcome, HistoryOutcome, PageHistory, ReadService};

/// One step of a page's history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryAnswer {
    /// The step: at most [`HISTORY_PAGE_ROWS`](crate::read::HISTORY_PAGE_ROWS) rows, and where the next begins.
    /// A step can hold no row and still have a next: it looked at as many changes as one request may, and none
    /// touched the page.
    Page {
        /// The history, as the page view reads it.
        history: Box<PageHistory>,
        /// Where the next step begins; `None` when the history is read to its end.
        next: Option<Cursor>,
    },
    /// Several pages match the title and none is nearer.
    Ambiguous {
        /// Every candidate.
        candidates: Vec<IndexEntry>,
    },
}

impl ReadService {
    /// One step of the history of the page titled `title`, newest change first, after the position `after`
    /// names (a cursor from this method's own previous answer; any other is [`ApiError::InvalidCursor`]).
    pub fn api_history(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        title: &str,
        after: Option<&Cursor>,
    ) -> Result<HistoryAnswer, ApiError> {
        let after = match after.map(Cursor::position) {
            Some(Some(Position::Number(number))) => Some(number),
            Some(_) => return Err(ApiError::InvalidCursor),
            None => None,
        };
        Ok(match self.page_history(wiki, view, title, after)? {
            HistoryOutcome::Page(history) => {
                let next = history.older.map(Cursor::after_number);
                HistoryAnswer::Page { history, next }
            }
            HistoryOutcome::Ambiguous { candidates } => HistoryAnswer::Ambiguous { candidates },
        })
    }

    /// What changed in the page titled `title` between two changes of the view's history. The points a request
    /// leaves out are filled in: `to` is the view's tip, `from` is the change before `to`; a `to` that is the
    /// first change of the view has no change before it, so the answer is [`DiffOutcome::NoSuchChange`]. The
    /// rest is [`ReadService::page_diff`], unchanged: one `diff`, always with the page's path. (Filling in
    /// `from` costs one `show`.)
    pub fn api_diff(
        &self,
        wiki: &WikiRecord,
        view: Option<&str>,
        from: Option<&str>,
        to: Option<&str>,
        title: &str,
    ) -> Result<DiffOutcome, ApiError> {
        inklkunn_markup::path::title_to_path(title)
            .map_err(crate::read::ReadError::InvalidTitle)?;
        let to = match to {
            Some(to) => to.to_owned(),
            None => self.tip(wiki, view)?.block_id,
        };
        let from = match from {
            Some(from) => from.to_owned(),
            None => match self.change(wiki, view, &to)? {
                ChangeOutcome::Found(detail) => match detail.info.parent {
                    Some(parent) => parent,
                    None => return Ok(DiffOutcome::NoSuchChange),
                },
                ChangeOutcome::NoSuchChange => return Ok(DiffOutcome::NoSuchChange),
            },
        };
        Ok(self.page_diff(wiki, view, &from, &to, title)?)
    }
}
