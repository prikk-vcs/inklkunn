//! How the links of a page are written (RFC 003 D-1, D-6): what the renderer asks (`RenderLinks`) answered
//! from the title index, and what the reader's presentation supplies that the renderer cannot know (the URL
//! scheme, the catalog's words).
//!
//! The rendered HTML contains these answers, so the cache key contains them: [`Presentation::lang`] and
//! [`Presentation::key`] are two parts of it, and a wiki renamed or read in another language is another key,
//! never the old page with the old hrefs.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use inklkunn_markup::render::IMAGE_EXTENSIONS;
use inklkunn_markup::scan::Link;
use inklkunn_markup::{
    LinkHref, Page, RenderLinks, Transclusion, TransclusionRefusal, Via, nfc_trim,
};
use inklkunn_store::{DerivedLinks, IndexEntry};

use super::ReadError;
use super::index::{Found, Lookup, TitleIndex};

/// What the surface supplies for the page it is about to show: the language, and the URL scheme and words
/// that end up written into the page. The web layer implements it for the URL scheme of RFC 003 D-1 and the
/// message catalog; this crate knows neither.
pub trait Presentation: Send + Sync {
    /// The catalog language the words are in (a BCP 47 tag). Part of the cache key.
    fn lang(&self) -> &str;
    /// Everything else the written HTML depends on, as one string: the URL base of the wiki and the view the
    /// page is shown in. Part of the cache key, so it must differ whenever [`Presentation::page_href`]
    /// would.
    fn key(&self) -> String;
    /// The address of the page titled `title`.
    fn page_href(&self, title: &str) -> String;
    /// The address that offers to create the page titled `title` (the dangling link leads there, NAV-3);
    /// `None` for a link that is only marked.
    fn create_href(&self, title: &str) -> Option<String>;
    /// The provenance line of an included page, in the language: "From ‘Glossary’". Plain text.
    fn transclusion_label(&self, title: &str) -> String;
    /// The sentence for an embed that was not included, in the language. Plain text.
    fn transclusion_notice(&self, refusal: &TransclusionRefusal) -> String;
}

/// Reads the source of a page for an inclusion: `Ok(None)` when the source is not what a page is (the embed
/// then stays a link), an error when prikk could not be asked (the page being rendered is then not kept).
pub(super) type BodyReader<'a> = &'a dyn Fn(&IndexEntry) -> Result<Option<String>, ReadError>;

/// The links of one page, resolved against one tip's index.
pub(super) struct PageLinks<'a> {
    index: &'a TitleIndex,
    presentation: &'a dyn Presentation,
    from_path: &'a str,
    body_of: BodyReader<'a>,
    seen: RefCell<Seen>,
    bodies: RefCell<HashMap<String, Option<String>>>,
    failure: RefCell<Option<ReadError>>,
}

#[derive(Default)]
struct Seen {
    resolved: BTreeSet<(String, String)>,
    dangling: BTreeSet<String>,
    ambiguous: BTreeMap<String, Vec<String>>,
    interwiki: BTreeSet<String>,
}

impl<'a> PageLinks<'a> {
    pub(super) fn new(
        index: &'a TitleIndex,
        presentation: &'a dyn Presentation,
        from_path: &'a str,
        body_of: BodyReader<'a>,
    ) -> Self {
        PageLinks {
            index,
            presentation,
            from_path,
            body_of,
            seen: RefCell::new(Seen::default()),
            bodies: RefCell::new(HashMap::new()),
            failure: RefCell::new(None),
        }
    }

    /// The first failure to read an included page, if any: a page that was rendered around it is not a page
    /// to keep.
    pub(super) fn take_failure(&self) -> Option<ReadError> {
        self.failure.borrow_mut().take()
    }

    /// The links seen, sorted, with the paths of the pages included.
    pub(super) fn into_links(self, transcluded: Vec<String>) -> DerivedLinks {
        let seen = self.seen.into_inner();
        let mut transcluded = transcluded;
        transcluded.sort();
        transcluded.dedup();
        DerivedLinks {
            resolved: seen.resolved.into_iter().collect(),
            dangling: seen.dangling.into_iter().collect(),
            ambiguous: seen.ambiguous.into_iter().collect(),
            interwiki: seen.interwiki.into_iter().collect(),
            transcluded,
        }
    }

    fn page_link(&self, entry: &IndexEntry) -> LinkHref {
        LinkHref::Page {
            href: self.presentation.page_href(&entry.title),
            title: Some(entry.title.clone()),
        }
    }

    /// The body of an included page, read once per render.
    fn body(&self, entry: &IndexEntry) -> Option<String> {
        if let Some(known) = self.bodies.borrow().get(&entry.path) {
            return known.clone();
        }
        let body = match (self.body_of)(entry) {
            Ok(text) => text.and_then(|t| Page::parse(&t).ok()).map(|p| p.body),
            Err(e) => {
                self.failure.borrow_mut().get_or_insert(e);
                None
            }
        };
        self.bodies
            .borrow_mut()
            .insert(entry.path.clone(), body.clone());
        body
    }
}

impl RenderLinks for PageLinks<'_> {
    fn link(&self, link: &Link) -> LinkHref {
        let target = nfc_trim(&link.target);
        let resolution = self.index.resolver().resolve(&target, Some(self.from_path));
        let mut seen = self.seen.borrow_mut();
        match resolution.via {
            Via::Interwiki => {
                seen.interwiki.insert(target);
                LinkHref::External(resolution.url.unwrap_or_default())
            }
            Via::Dangling | Via::SamePage => {
                // A file named in an embed is not a page that is missing: no invitation to create it.
                let file = link.embed
                    && target.rsplit('.').next().is_some_and(|e| {
                        IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str())
                    });
                if file {
                    return LinkHref::Missing { href: None };
                }
                let href = self.presentation.create_href(&target);
                seen.dangling.insert(target);
                LinkHref::Missing { href }
            }
            Via::Ambiguous => {
                // The first candidate, as the profile says, and the ambiguity reported, never hidden.
                seen.ambiguous.insert(target, resolution.candidates.clone());
                match resolution.path.as_deref().and_then(|p| self.index.entry(p)) {
                    Some(entry) => self.page_link(entry),
                    None => LinkHref::Missing { href: None },
                }
            }
            _ => match resolution.path.as_deref().and_then(|p| self.index.entry(p)) {
                Some(entry) => {
                    seen.resolved.insert((target, entry.path.clone()));
                    self.page_link(entry)
                }
                None => LinkHref::Missing { href: None },
            },
        }
    }

    fn transclusion(&self, link: &Link) -> Option<Transclusion> {
        // Only a title that leads to a page is a transclusion; redirects are followed. A title that does not
        // lead to one stays the link, whose rendering records why.
        let Lookup::Found(Found { entry, .. }) = self.index.lookup(&link.target) else {
            return None;
        };
        if entry.problem.is_some() {
            return None;
        }
        Some(Transclusion {
            href: self.presentation.page_href(&entry.title),
            label: self.presentation.transclusion_label(&entry.title),
            title: entry.title,
            path: entry.path,
        })
    }

    /// Asked for only when the page is going to be included: this is the call to prikk.
    fn transclusion_body(&self, page: &Transclusion) -> Option<String> {
        self.body(self.index.entry(&page.path)?)
    }

    fn transclusion_notice(&self, refusal: &TransclusionRefusal) -> String {
        self.presentation.transclusion_notice(refusal)
    }
}

#[cfg(test)]
mod tests;
