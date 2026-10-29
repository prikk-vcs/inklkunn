//! What the read service needs of a surface to write a page's HTML: the URL scheme and the catalog's words
//! (RFC 003 D-1, D-6). One value per request; its [`Presentation::key`] is part of the cache key, so two
//! requests that would write different links never share a cached page.

use inklkunn_core::read::Presentation;
use inklkunn_core::read::TransclusionRefusal;

use crate::catalog::{Key, T};
use crate::url::encode_title;

/// The links and the words of one wiki's pages in one view and one language.
#[derive(Debug, Clone)]
pub(crate) struct WikiLinks {
    t: T,
    slug: String,
    /// The view the address names (`draft` for `heads/draft`), or `None` for the default view.
    view: Option<String>,
}

impl WikiLinks {
    pub(crate) fn new(t: T, slug: &str, view: Option<&str>) -> Self {
        WikiLinks {
            t,
            slug: slug.to_owned(),
            view: view.map(str::to_owned),
        }
    }

    /// `/<slug>` or `/<slug>/~<view>`: where this wiki's pages are, in this view.
    pub(crate) fn base(&self) -> String {
        match &self.view {
            Some(view) => format!("/{}/~{view}", self.slug),
            None => format!("/{}", self.slug),
        }
    }
}

impl Presentation for WikiLinks {
    fn lang(&self) -> &str {
        self.t.lang().tag()
    }

    fn key(&self) -> String {
        self.base()
    }

    fn page_href(&self, title: &str) -> String {
        format!("{}/{}", self.base(), encode_title(title))
    }

    /// Nobody can create a page yet: proposals arrive with the next release (RFC 004), so a link to a page that
    /// does not exist is marked and goes nowhere. (A link that led to a form would be a dead end.)
    fn create_href(&self, _title: &str) -> Option<String> {
        None
    }

    fn transclusion_label(&self, title: &str) -> String {
        self.t
            .msg(Key::TransclusionFrom)
            .with("title", title)
            .text()
    }

    fn transclusion_notice(&self, refusal: &TransclusionRefusal) -> String {
        match refusal {
            TransclusionRefusal::Cycle { chain } => self
                .t
                .msg(Key::TransclusionCycle)
                .with("chain", chain.join(" → "))
                .text(),
            TransclusionRefusal::TooDeep { title, limit } => self
                .t
                .msg(Key::TransclusionTooDeep)
                .with("title", title)
                .with("limit", limit)
                .text(),
            TransclusionRefusal::OverBudget { title } => self
                .t
                .msg(Key::TransclusionOverBudget)
                .with("title", title)
                .text(),
        }
    }
}

#[cfg(test)]
mod tests;
