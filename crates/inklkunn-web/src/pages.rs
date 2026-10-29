//! The pages of this release and the layout every later page extends (RFC 003 D-6; WEB-09; INC-1 … INC-4).
//!
//! * [`Page`] is what a handler holds while it answers: the language, and where the footer's language buttons
//!   return to. It is an extractor, so a handler asks for it and gets the negotiated language (the cookie,
//!   else `Accept-Language`, else English; a preview language only ever by the cookie, see
//!   [`Lang::negotiate`](crate::catalog::Lang::negotiate)) without reading a header itself.
//! * `templates/base.html` is the one layout: a skip link first, a banner, one `main`, a footer, `lang` and
//!   `dir` on `<html>`, one stylesheet, no script. A page extends it and fills `title` and `body`; it holds no
//!   text of its own: every word is a catalog message (`tests/catalog_checks.rs` fails on a literal).
//! * The pages that exist so far are the language switcher (`/-/lang`), the notices (not found, a refused
//!   form) and the plain answers that are not pages (a misdirected request, a page that could not be drawn).
//!   Page content is the next task's.
//!
//! The switcher is a form per language that posts to `/-/lang`: no script, no link that changes anything. It
//! sets the language cookie and returns to the page the visitor was on, and only to a local address.

use std::collections::HashMap;
use std::convert::Infallible;

use askama::Template;
use axum::extract::rejection::{FormRejection, QueryRejection};
use axum::extract::{Form, FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::catalog::{self, Key, Lang, PRODUCT, Rich, T};
use crate::{lang, style};

/// The address of the language switcher: the page, and the form it and every footer post to.
pub const LANGUAGE_PATH: &str = "/-/lang";

/// The most bytes a language form may carry: two short fields.
pub(crate) const FORM_LIMIT: usize = 4096;

/// What the pages need to know about the site, shared by every handler.
#[derive(Clone, Debug)]
pub(crate) struct PageState {
    /// Whether the application origin is `https` (then the language cookie is `Secure`).
    pub https: bool,
    /// What the wiki pages are built on; `None` serves no wiki at all (every wiki address is then the generic
    /// not-found page, exactly as for a wiki that does not exist).
    pub services: Option<std::sync::Arc<crate::services::Services>>,
    /// The two origins, for the addresses that must be absolute (feeds, the sitemap, links to the content
    /// origin).
    pub origins: std::sync::Arc<crate::discover::Origins>,
}

/// One language the switcher offers.
#[derive(Clone, Debug)]
pub struct Choice {
    /// The language's tag, the value its form posts.
    pub tag: &'static str,
    /// Is it the page's own language?
    pub current: bool,
    /// Is it a preview, not yet read by a fluent reviewer?
    pub preview: bool,
    /// What the button says: the language's name in itself, marked with its `lang`, and "preview" where it is
    /// one.
    pub label: Rich,
}

/// What a handler holds while it answers a page request.
#[derive(Clone, Debug)]
pub struct Page {
    t: T,
    pub(crate) back: String,
    pub(crate) switcher: bool,
    /// The link relations of the head (a wiki's pages carry them, see `discover`).
    pub(crate) head: Vec<crate::discover::HeadLink>,
    /// A page of a wiki that is not public: `noindex`, in the head and in a header.
    pub(crate) noindex: bool,
    /// The site's origins, when the page was built for a request.
    pub(crate) origins: Option<std::sync::Arc<crate::discover::Origins>>,
}

impl Page {
    /// A page in the language of `t`, whose footer language buttons return to `back` (a local address; any
    /// other is replaced by the start page).
    pub fn new(t: T, back: &str) -> Page {
        Page {
            t,
            back: lang::safe_back(back).unwrap_or("/").to_owned(),
            switcher: true,
            head: Vec::new(),
            noindex: false,
            origins: None,
        }
    }

    /// The same page without the footer's language buttons (the language page and the refusals have their own
    /// way to choose).
    #[must_use]
    pub fn without_switcher(mut self) -> Page {
        self.switcher = false;
        self
    }

    /// The page's messages.
    pub fn t(&self) -> T {
        self.t
    }

    pub(crate) fn get(&self, key: Key) -> String {
        self.t.get(key)
    }

    /// A page title: the message with `{product}` filled.
    pub(crate) fn title(&self, key: Key) -> String {
        self.t.msg(key).product().text()
    }

    pub(crate) fn lang_tag(&self) -> &'static str {
        self.t.lang().tag()
    }

    pub(crate) fn dir(&self) -> &'static str {
        self.t.lang().dir()
    }

    pub(crate) fn product(&self) -> &'static str {
        PRODUCT
    }

    pub(crate) fn stylesheet(&self) -> &'static str {
        style::path()
    }

    pub(crate) fn language_path(&self) -> &'static str {
        LANGUAGE_PATH
    }

    /// The `<link>` elements of the head.
    pub(crate) fn head_links(&self) -> &[crate::discover::HeadLink] {
        &self.head
    }

    /// Does the page ask not to be indexed?
    pub(crate) fn noindex(&self) -> bool {
        self.noindex
    }

    pub(crate) fn footer_line(&self) -> String {
        self.t
            .msg(Key::LayoutFooter)
            .product()
            .with("version", self.t.version())
            .text()
    }

    /// The languages the switcher offers, English first, the page's own marked.
    pub fn languages(&self) -> Vec<Choice> {
        catalog::languages()
            .iter()
            .map(|def| {
                let preview = def.review.is_preview();
                let label = if preview {
                    self.t
                        .msg(Key::LanguagePreviewChoice)
                        .in_lang("name", def.tag, def.endonym)
                        .html()
                } else {
                    catalog::in_lang(def.tag, def.endonym)
                };
                Choice {
                    tag: def.tag,
                    current: def.tag == self.lang_tag(),
                    preview,
                    label,
                }
            })
            .collect()
    }
}

impl FromRequestParts<PageState> for Page {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &PageState) -> Result<Page, Infallible> {
        let language = lang::negotiate_from_headers(&parts.headers);
        let mut page = Page::new(T::new(language), &lang::back_for(&parts.method, &parts.uri));
        page.origins = Some(state.origins.clone());
        Ok(page)
    }
}

/// A link a notice offers as its next step.
struct NoticeLink {
    href: &'static str,
    label: String,
}

/// A page with a heading, a sentence and perhaps one link: the shape of every answer that is only a message.
#[derive(Template)]
#[template(path = "notice.html")]
struct NoticeTpl<'a> {
    page: &'a Page,
    title: String,
    heading: String,
    body: String,
    link: Option<NoticeLink>,
}

/// The language page: a form per language, and a way back.
#[derive(Template)]
#[template(path = "language.html")]
struct LanguageTpl<'a> {
    page: &'a Page,
    next: &'a str,
    back_link: Option<&'a str>,
}

/// The *not found* page, as HTML (RFC 003 D-1: a missing route answers 404 with a next step in words).
pub fn render_not_found(page: &Page) -> Result<String, askama::Error> {
    NoticeTpl {
        page,
        title: page.title(Key::TitleNotFound),
        heading: page.get(Key::NotFoundHeading),
        body: page.get(Key::NotFoundBody),
        link: None,
    }
    .render()
}

/// The language page. `next` is where a choice returns to (a local address, checked by the caller); with none
/// a choice stays on this page.
pub fn render_language(page: &Page, next: Option<&str>) -> Result<String, askama::Error> {
    LanguageTpl {
        page,
        next: next.unwrap_or(LANGUAGE_PATH),
        back_link: next.filter(|n| !n.starts_with(LANGUAGE_PATH)),
    }
    .render()
}

fn render_language_refused(page: &Page) -> Result<String, askama::Error> {
    NoticeTpl {
        page,
        title: page.title(Key::TitleLanguageRefused),
        heading: page.get(Key::LanguageRefusedHeading),
        body: page.get(Key::LanguageRefusedBody),
        link: Some(NoticeLink {
            href: LANGUAGE_PATH,
            label: page.get(Key::LanguageRefusedLink),
        }),
    }
    .render()
}

pub(crate) fn render_form_refused(page: &Page) -> Result<String, askama::Error> {
    NoticeTpl {
        page,
        title: page.title(Key::TitleFormRefused),
        heading: page.get(Key::FormRefusedHeading),
        body: page.get(Key::FormRefusedBody),
        link: None,
    }
    .render()
}

/// A page answer: the HTML, the status, and the language it is in.
pub(crate) fn html(status: StatusCode, language: Lang, body: String) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CONTENT_LANGUAGE, language.tag()),
        ],
        body,
    )
        .into_response()
}

/// An answer that is not a page: one catalog sentence as plain text.
pub(crate) fn plain(status: StatusCode, language: Lang, body: String) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CONTENT_LANGUAGE, language.tag()),
        ],
        body,
    )
        .into_response()
}

/// Turn a rendered page into an answer. A template that cannot draw is a failure said in one sentence, never a
/// silent empty page.
pub(crate) fn answer(
    status: StatusCode,
    page: &Page,
    rendered: Result<String, askama::Error>,
) -> Response {
    match rendered {
        Ok(body) => {
            let mut res = html(status, page.t.lang(), body);
            if page.noindex {
                res.headers_mut().insert(
                    HeaderName::from_static("x-robots-tag"),
                    HeaderValue::from_static("noindex, nofollow"),
                );
            }
            res
        }
        Err(_) => plain(
            StatusCode::INTERNAL_SERVER_ERROR,
            page.t.lang(),
            page.get(Key::PageRenderFailed),
        ),
    }
}

/// Any address the router has no route for: 404, a page with a heading, a sentence and the layout.
pub(crate) async fn not_found(page: Page) -> Response {
    let rendered = render_not_found(&page);
    answer(StatusCode::NOT_FOUND, &page, rendered)
}

/// An address that exists but does not take the request's method (405): a page in the visitor's language.
/// (The `Allow` header is the router's own, set on the method router before this fallback answers.)
pub(crate) async fn method_not_allowed(page: Page) -> Response {
    let rendered = NoticeTpl {
        page: &page,
        title: page.title(Key::TitleMethodNotAllowed),
        heading: page.get(Key::MethodNotAllowedHeading),
        body: page.get(Key::MethodNotAllowedBody),
        link: None,
    }
    .render();
    answer(StatusCode::METHOD_NOT_ALLOWED, &page, rendered)
}

/// The content origin's 404: it serves no page (its policy allows no stylesheet and no form), so the same
/// sentence is plain text.
pub(crate) async fn plain_not_found(headers: HeaderMap) -> Response {
    let language = lang::negotiate_from_headers(&headers);
    plain(
        StatusCode::NOT_FOUND,
        language,
        T::new(language).get(Key::NotFoundBody),
    )
}

/// A request for a host this server does not answer for (421): plain text in the visitor's language. No page
/// is built, because no address on it can be trusted.
pub(crate) fn misdirected(headers: &HeaderMap) -> Response {
    let language = lang::negotiate_from_headers(headers);
    plain(
        StatusCode::MISDIRECTED_REQUEST,
        language,
        T::new(language).get(Key::MisdirectedBody),
    )
}

/// `GET /-/lang`: the language page. `?next=` names where a choice returns to; anything that is not a local
/// address is ignored.
pub(crate) async fn language_page(
    page: Page,
    query: Result<Query<HashMap<String, String>>, QueryRejection>,
) -> Response {
    let next = query
        .ok()
        .and_then(|Query(q)| q.get("next").cloned())
        .filter(|n| lang::safe_back(n).is_some());
    let page = page.without_switcher();
    let rendered = render_language(&page, next.as_deref());
    answer(StatusCode::OK, &page, rendered)
}

/// `POST /-/lang`: remember the visitor's language in a cookie (a language code and nothing else) and send
/// them back to the page they were on. A form from another site is refused (403), and so is a language this
/// build does not have, with a page that says so (it is never defaulted); the return address must be local.
pub(crate) async fn choose_language(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let page = page.without_switcher();
    if !lang::from_this_site(&headers) {
        let rendered = render_form_refused(&page);
        return answer(StatusCode::FORBIDDEN, &page, rendered);
    }
    let fields = match form {
        Ok(Form(fields)) => fields,
        Err(rejection) => {
            let rendered = render_language_refused(&page);
            return answer(rejection.status(), &page, rendered);
        }
    };
    let Some(chosen) = fields.get("lang").and_then(|tag| Lang::from_tag(tag)) else {
        let rendered = render_language_refused(&page);
        return answer(StatusCode::UNPROCESSABLE_ENTITY, &page, rendered);
    };
    let to = fields
        .get("next")
        .and_then(|n| lang::safe_back(n))
        .unwrap_or(LANGUAGE_PATH);
    let cookie = lang::language_cookie(chosen.tag(), state.https);
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, to),
            (header::SET_COOKIE, cookie.as_str()),
        ],
    )
        .into_response()
}

#[cfg(test)]
mod tests;
