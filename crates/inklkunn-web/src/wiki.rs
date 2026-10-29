//! A wiki's own pages (RFC 003 D-1, D-5; handoff 003 T4): the front door, a page, the page that is not there,
//! the verification page, the about page and the list of wikis.
//!
//! **Every handler begins the same way** ([`open`]): who is asking, which wiki the slug names, and whether that
//! principal may read it. A wiki that does not exist and a wiki the principal may not read produce **the same
//! answer** (the generic not-found page, byte for byte), and cost the same to ask about, so an address cannot be
//! used to find out which wikis exist (RFC 002 D-2; threat model INV-4). Nothing of a wiki is read before that.
//!
//! **The badge is the record's.** It is read from the wiki record fetched for this request, never from a cache
//! (RFC 003 D-5; INV-5, INV-6); a page served stale says *Could not be checked now* whatever the record says,
//! because the server could not ask just now and must not repeat an old verdict as if it were current.
//!
//! The blocking calls (the stores, prikk) run on a blocking thread; templates render on the async one.

use std::fmt;
use std::sync::Arc;

use askama::Template;
use askama::filters::HtmlSafe;
use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use inklkunn_core::read::Via;
use inklkunn_core::read::{
    PageOutcome, PageProblem, Presentation, ReadError, RedirectProblem, RedirectedBy,
};
use inklkunn_core::{Action, CheckOutcome, NotChecked, PageView, Resource, SafeHtml};
use inklkunn_store::{Slug, VerificationState, WikiId, WikiRecord};

use crate::catalog::{Key, Rich, T};
use crate::discover::Subject;
use crate::pages::{self, Page, PageState, answer, render_form_refused};
use crate::present::WikiLinks;
use crate::services::{MIN_RECHECK_SECS, Services};
use crate::time::utc_minutes;
use crate::url::{page_address, view_ref};

// ------------------------------------------------------------------------------------ the shared start

/// The wiki a request is about, if the principal may read it. `None` is the one answer for "no such wiki" and
/// "not yours to read".
pub(crate) fn open(services: &Services, headers: &HeaderMap, slug: &str) -> Option<WikiRecord> {
    let principal = services.identity.principal(headers);
    let record = Slug::new(slug)
        .ok()
        .and_then(|slug| services.wikis.by_slug(&slug).ok());
    // The same lookups and the same decision whether the wiki is there or not.
    let id = record
        .as_ref()
        .map_or_else(WikiId::new, |record| record.id.clone());
    let decision =
        services
            .authorizer
            .authorize(&principal, Action::Read, &Resource::Wiki { wiki: id });
    record.filter(|_| decision.is_allowed())
}

/// Run `work` on a blocking thread (the stores and prikk block).
pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    tokio::task::spawn_blocking(work).await.ok()
}

pub(crate) fn render_failed(page: &Page) -> Response {
    pages::plain(
        StatusCode::INTERNAL_SERVER_ERROR,
        page.t().lang(),
        page.get(Key::PageRenderFailed),
    )
}

/// The one answer for a wiki that is not there (or not readable).
pub(crate) fn generic_not_found(page: &Page) -> Response {
    answer(StatusCode::NOT_FOUND, page, pages::render_not_found(page))
}

// ------------------------------------------------------------------------------------ view models

/// HTML that has passed the sanitizer, as a template prints it: unescaped, because it was made safe already.
pub(crate) struct Body(pub(crate) SafeHtml);

impl fmt::Display for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl HtmlSafe for Body {}

/// What every page of a wiki shows above its heading: the wiki's name and the addresses of its own pages.
pub(crate) struct Shell {
    pub(crate) name: String,
    pub(crate) front: String,
    pub(crate) about: String,
    pub(crate) verification: String,
    pub(crate) changes: String,
    pub(crate) search: String,
    pub(crate) health: String,
    pub(crate) wikis: &'static str,
}

const WIKIS_PATH: &str = "/-/wikis";

impl Shell {
    pub(crate) fn of(wiki: &WikiRecord) -> Shell {
        let base = format!("/{}", wiki.slug);
        Shell {
            name: wiki.display_name.as_str().to_owned(),
            about: format!("{base}/-/about"),
            verification: format!("{base}/-/verification"),
            changes: format!("{base}/-/changes"),
            search: format!("{base}/-/search"),
            health: format!("{base}/-/health"),
            front: base,
            wikis: WIKIS_PATH,
        }
    }
}

pub(crate) struct TocItem {
    level: u8,
    id: String,
    text: String,
}

pub(crate) struct Choice {
    pub(crate) label: String,
    pub(crate) href: String,
}

/// The badge: the four labels, said in the sentence the catalog gives, linked to the verification page.
pub(crate) fn badge(t: T, wiki: &WikiRecord, stale: bool) -> Rich {
    let key = if stale {
        Key::AssuranceCouldNotBeCheckedNow
    } else {
        match wiki.verification.state {
            VerificationState::Verified => Key::AssuranceVerified,
            VerificationState::Failed => Key::AssuranceVerificationFailed,
            VerificationState::CouldNotCheck => Key::AssuranceCouldNotBeCheckedNow,
            VerificationState::Unknown => Key::AssuranceNotCheckedYet,
        }
    };
    t.msg(Key::BadgeSentence)
        .link(
            "state",
            t.get(key),
            format!("/{}/-/verification", wiki.slug),
        )
        .html()
}

// ------------------------------------------------------------------------------------ the templates

#[derive(Template)]
#[template(path = "wiki_page.html")]
struct PageTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    view_note: Option<String>,
    notes: Vec<String>,
    stale: Option<String>,
    contents_label: String,
    toc: Vec<TocItem>,
    body_lang: Option<String>,
    body: Body,
    license: String,
    badge: Rich,
    history_href: String,
    banner: Option<Banner>,
    propose: bool,
}

/// What stands above a page that is not the page as it is now (a permalink, handoff 003 T5): sentences with
/// their links, built from catalog messages.
pub(crate) struct Banner {
    pub(crate) lines: Vec<Rich>,
}

/// How a page differs from a plain page view: its own `<title>` and a banner (a permalink).
pub(crate) struct Variant {
    pub(crate) title: Option<String>,
    pub(crate) banner: Option<Banner>,
}

#[derive(Template)]
#[template(path = "wiki_notice.html")]
struct NoticeTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    paragraphs: Vec<String>,
    choices: Vec<Choice>,
    propose: bool,
}

struct CheckView {
    lines: Vec<String>,
    stages: Vec<String>,
}

#[derive(Template)]
#[template(path = "verification.html")]
struct VerificationTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    badge: Rich,
    state_sentence: String,
    checked_at: Option<String>,
    result: Option<CheckView>,
    action: String,
}

#[derive(Template)]
#[template(path = "about.html")]
struct AboutTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    license: String,
    view: String,
    badge: Rich,
    preview: bool,
}

struct WikiItem {
    name: String,
    href: String,
}

#[derive(Template)]
#[template(path = "wikis.html")]
struct WikisTpl<'a> {
    page: &'a Page,
    wikis: Vec<WikiItem>,
}

// ------------------------------------------------------------------------------------ small helpers

pub(crate) fn notice(
    page: &Page,
    wiki: &WikiRecord,
    status: StatusCode,
    heading: Key,
    paragraphs: Vec<String>,
    choices: Vec<Choice>,
    propose: bool,
) -> Response {
    let shell = Shell::of(wiki);
    let page = &page.wiki_head(wiki, Subject::Wiki);
    let rendered = NoticeTpl {
        page,
        shell: &shell,
        title: page
            .t()
            .msg(Key::TitlePage)
            .with("page", page.get(heading))
            .with("wiki", wiki.display_name.as_str())
            .product()
            .text(),
        heading: page.get(heading),
        paragraphs,
        choices,
        propose,
    }
    .render();
    answer(status, page, rendered)
}

pub(crate) fn with_retry_after(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from_static("30"));
    response
}

pub(crate) fn services_of(state: &PageState) -> Option<Arc<Services>> {
    state.services.clone()
}

// ------------------------------------------------------------------------------------ the page itself

/// `GET /{slug}`: the front door. The wiki's start page is the page titled *Home*, and a wiki with none shows
/// its about page, which always exists.
pub(crate) async fn front(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    slug: Result<Path<String>, PathRejection>,
) -> Response {
    let Ok(Path(slug)) = slug else {
        return generic_not_found(&page);
    };
    let Some(services) = services_of(&state) else {
        return generic_not_found(&page);
    };
    let slug_for_work = slug.clone();
    let target = blocking(move || {
        let wiki = open(&services, &headers, &slug_for_work)?;
        let home = services.read.lookup(&wiki, None, "Home").ok();
        let has_home = matches!(home, Some(inklkunn_core::read::Lookup::Found(_)));
        Some(if has_home {
            format!("/{}/Home", wiki.slug)
        } else {
            format!("/{}/-/about", wiki.slug)
        })
    })
    .await
    .flatten();
    match target {
        Some(location) => redirect(&location),
        None => generic_not_found(&page),
    }
}

pub(crate) fn redirect(location: &str) -> Response {
    match HeaderValue::from_str(location) {
        Ok(value) => (StatusCode::SEE_OTHER, [(header::LOCATION, value)]).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// The page shown for a title that leads to no page: the 404 that invites it.
pub(crate) fn missing_page(page: &Page, wiki: &WikiRecord, title: &str) -> Response {
    present(
        page,
        Shown::Missing {
            wiki: Box::new(wiki.clone()),
            title: title.trim().to_owned(),
        },
    )
}

/// What a request for a page came to, before any template.
pub(crate) enum Shown {
    NotFound,
    Found {
        wiki: Box<WikiRecord>,
        view: Box<PageView>,
        asked: String,
        view_name: Option<String>,
    },
    Missing {
        wiki: Box<WikiRecord>,
        title: String,
    },
    Ambiguous {
        wiki: Box<WikiRecord>,
        title: String,
        candidates: Vec<(String, String)>,
    },
    BrokenRedirect {
        wiki: Box<WikiRecord>,
        from: String,
        problem: RedirectProblem,
    },
    Unreadable {
        wiki: Box<WikiRecord>,
        problem: PageProblem,
    },
    NoSuchView {
        wiki: Box<WikiRecord>,
        view: String,
    },
    InvalidTitle {
        wiki: Box<WikiRecord>,
    },
    Unavailable {
        wiki: Box<WikiRecord>,
    },
}

fn read_page(services: &Services, headers: &HeaderMap, t: T, slug: &str, raw_rest: &str) -> Shown {
    let Some(wiki) = open(services, headers, slug) else {
        return Shown::NotFound;
    };
    let wiki = Box::new(wiki);
    let Some(address) = page_address(raw_rest) else {
        return Shown::InvalidTitle { wiki };
    };
    let (view_name, view_full) = match &address.view {
        None => (None, None),
        Some(name) => match view_ref(name) {
            Some(full) => (Some(name.clone()), Some(full)),
            None => {
                return Shown::NoSuchView {
                    wiki,
                    view: name.clone(),
                };
            }
        },
    };
    let links = WikiLinks::new(t, wiki.slug.as_str(), view_name.as_deref());
    match services
        .read
        .page(&wiki, view_full.as_deref(), &address.title, &links)
    {
        Ok(PageOutcome::Found(view)) => Shown::Found {
            wiki,
            view,
            asked: address.title,
            view_name,
        },
        Ok(PageOutcome::Missing { title }) => Shown::Missing { wiki, title },
        Ok(PageOutcome::Ambiguous { candidates }) => Shown::Ambiguous {
            candidates: candidates
                .iter()
                .map(|c| (c.title.clone(), links.page_href(&c.title)))
                .collect(),
            wiki,
            title: address.title,
        },
        Ok(PageOutcome::BrokenRedirect { from, problem }) => Shown::BrokenRedirect {
            wiki,
            from: from.title,
            problem,
        },
        Err(ReadError::InvalidTitle(_)) => Shown::InvalidTitle { wiki },
        Err(ReadError::NoSuchView(view)) => Shown::NoSuchView {
            wiki,
            view: view.strip_prefix("heads/").unwrap_or(&view).to_owned(),
        },
        Err(ReadError::PageUnreadable(problem)) => Shown::Unreadable { wiki, problem },
        Err(ReadError::CouldNotBeReadNow(cause)) => {
            (services.log)(&format!(
                "a page of wiki `{}` could not be read now: {cause}",
                wiki.slug
            ));
            // The newest could not be asked for: the last version this server has, if it has one, says so.
            match services
                .read
                .stale_page(&wiki, view_full.as_deref(), &address.title, &links)
            {
                Some(view) => Shown::Found {
                    wiki,
                    view: Box::new(view),
                    asked: address.title,
                    view_name,
                },
                None => Shown::Unavailable { wiki },
            }
        }
    }
}

/// `GET /{slug}/{title}` and `GET /{slug}/~{view}/{title}`.
pub(crate) async fn view_page(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    // An address under the API's prefix that no endpoint took is answered in the API's own form, never as a
    // page that does not exist.
    if crate::api::is_api_path(uri.path()) {
        return crate::api::not_found(&headers);
    }
    let Some(services) = services_of(&state) else {
        return generic_not_found(&page);
    };
    // The raw path decides what is a view and what is a title (see `url`), and an address that cannot be
    // decoded is a page like any other refusal, not axum's bare 400.
    let (raw_slug, raw_rest) = uri
        .path()
        .strip_prefix('/')
        .and_then(|p| p.split_once('/'))
        .map(|(slug, rest)| (slug.to_owned(), rest.to_owned()))
        .unwrap_or_default();
    let Some(slug) = crate::url::decode(&raw_slug) else {
        return generic_not_found(&page);
    };
    let t = page.t();
    let slug_for_work = slug.clone();
    let shown =
        blocking(move || read_page(&services, &headers, t, &slug_for_work, &raw_rest)).await;
    let Some(shown) = shown else {
        return render_failed(&page);
    };
    present(&page, shown)
}

pub(crate) fn present(page: &Page, shown: Shown) -> Response {
    let t = page.t();
    match shown {
        Shown::NotFound => generic_not_found(page),
        Shown::Found {
            wiki,
            view,
            asked,
            view_name,
        } => found(page, &wiki, &view, &asked, view_name.as_deref()),
        Shown::Missing { wiki, title } => notice(
            page,
            &wiki,
            StatusCode::NOT_FOUND,
            Key::MissingHeading,
            vec![
                t.msg(Key::MissingBody)
                    .with("title", &title)
                    .with("wiki", wiki.display_name.as_str())
                    .text(),
                page.get(Key::MissingInvite),
            ],
            Vec::new(),
            true,
        ),
        Shown::Ambiguous {
            wiki,
            title,
            candidates,
        } => notice(
            page,
            &wiki,
            StatusCode::OK,
            Key::AmbiguousHeading,
            vec![t.msg(Key::AmbiguousBody).with("title", &title).text()],
            candidates
                .into_iter()
                .map(|(label, href)| Choice { label, href })
                .collect(),
            false,
        ),
        Shown::BrokenRedirect {
            wiki,
            from,
            problem,
        } => {
            let sentence = match problem {
                RedirectProblem::Dangling { target } => t
                    .msg(Key::RedirectDangling)
                    .with("title", &from)
                    .with("target", &target)
                    .text(),
                RedirectProblem::Loop => t.msg(Key::RedirectLoop).with("title", &from).text(),
                RedirectProblem::TooLong => t.msg(Key::RedirectTooLong).with("title", &from).text(),
            };
            notice(
                page,
                &wiki,
                StatusCode::NOT_FOUND,
                Key::RedirectHeading,
                vec![sentence],
                Vec::new(),
                false,
            )
        }
        Shown::Unreadable { wiki, problem } => {
            let sentence = match problem {
                PageProblem::NotUtf8 => page.get(Key::UnreadableNotText),
                PageProblem::Frontmatter(_) => page.get(Key::UnreadableInfo),
                PageProblem::FrontmatterTooLong => page.get(Key::UnreadableInfoLong),
            };
            notice(
                page,
                &wiki,
                StatusCode::UNPROCESSABLE_ENTITY,
                Key::UnreadableHeading,
                vec![sentence, page.get(Key::UnreadableAdvice)],
                Vec::new(),
                false,
            )
        }
        Shown::NoSuchView { wiki, view } => notice(
            page,
            &wiki,
            StatusCode::NOT_FOUND,
            Key::NoViewHeading,
            vec![t.msg(Key::NoViewBody).with("view", &view).text()],
            Vec::new(),
            false,
        ),
        Shown::InvalidTitle { wiki } => notice(
            page,
            &wiki,
            StatusCode::NOT_FOUND,
            Key::InvalidTitleHeading,
            vec![page.get(Key::InvalidTitleBody)],
            Vec::new(),
            false,
        ),
        Shown::Unavailable { wiki } => with_retry_after(notice(
            page,
            &wiki,
            StatusCode::SERVICE_UNAVAILABLE,
            Key::UnavailableHeading,
            vec![page.get(Key::UnavailableBody)],
            Vec::new(),
            false,
        )),
    }
}

fn found(
    page: &Page,
    wiki: &WikiRecord,
    view: &PageView,
    asked: &str,
    view_name: Option<&str>,
) -> Response {
    found_with(
        page,
        wiki,
        view,
        asked,
        view_name,
        Variant {
            title: None,
            banner: None,
        },
    )
}

/// A page, drawn: the page view, and the permalink, which differs in its title and its banner and offers no
/// proposal (a change is proposed against the page as it is).
pub(crate) fn found_with(
    page: &Page,
    wiki: &WikiRecord,
    view: &PageView,
    asked: &str,
    view_name: Option<&str>,
    variant: Variant,
) -> Response {
    let t = page.t();
    let shell = Shell::of(wiki);
    let mut notes = Vec::new();
    match &view.redirected_from {
        Some(from) if from.by == RedirectedBy::Alias => {
            notes.push(t.msg(Key::PageAliasFrom).with("title", &from.title).text())
        }
        Some(from) => notes.push(
            t.msg(Key::PageRedirectedFrom)
                .with("title", &from.title)
                .text(),
        ),
        None if !matches!(view.via, Via::Exact | Via::Alias) => {
            notes.push(t.msg(Key::PageMatchedLoosely).with("title", asked).text())
        }
        None => {}
    }
    let license = view
        .meta
        .license
        .clone()
        .unwrap_or_else(|| wiki.default_license.clone());
    let heading = view.meta.title.clone();
    let page = &page.wiki_head(
        wiki,
        Subject::Page {
            title: &heading,
            view: view_name,
            block: (!view.stale).then_some(view.tip.block_id.as_str()),
        },
    );
    let rendered = PageTpl {
        page,
        shell: &shell,
        title: variant.title.unwrap_or_else(|| {
            t.msg(Key::TitlePage)
                .with("page", &heading)
                .with("wiki", wiki.display_name.as_str())
                .product()
                .text()
        }),
        history_href: crate::url::history_path(wiki.slug.as_str(), view_name, &heading, None),
        propose: variant.banner.is_none(),
        banner: variant.banner,
        heading,
        view_note: view_name.map(|v| t.msg(Key::WikiViewInUse).with("view", v).text()),
        notes,
        stale: view.stale.then(|| page.get(Key::PageStale)),
        contents_label: page.get(Key::PageContents),
        toc: view
            .toc
            .iter()
            .map(|h| TocItem {
                level: h.level,
                id: h.id.clone(),
                text: h.text.clone(),
            })
            .collect(),
        body_lang: view.meta.lang.clone().filter(|l| l != page.lang_tag()),
        body: Body(view.html.clone()),
        license: t.msg(Key::PageLicense).with("license", license).text(),
        badge: badge(t, wiki, view.stale),
    }
    .render();
    answer(StatusCode::OK, page, rendered)
}

// ------------------------------------------------------------------------------------ about

/// `GET /{slug}/-/about`: the license, the default view and the accessibility statement.
pub(crate) async fn about(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    slug: Result<Path<String>, PathRejection>,
) -> Response {
    let Ok(Path(slug)) = slug else {
        return generic_not_found(&page);
    };
    let Some(services) = services_of(&state) else {
        return generic_not_found(&page);
    };
    let wiki = blocking(move || open(&services, &headers, &slug))
        .await
        .flatten();
    let Some(wiki) = wiki else {
        return generic_not_found(&page);
    };
    let t = page.t();
    let shell = Shell::of(&wiki);
    let page = page.wiki_head(&wiki, Subject::Surface("-/about"));
    let rendered = AboutTpl {
        page: &page,
        shell: &shell,
        title: t
            .msg(Key::TitleAbout)
            .with("wiki", wiki.display_name.as_str())
            .product()
            .text(),
        heading: t
            .msg(Key::AboutHeading)
            .with("wiki", wiki.display_name.as_str())
            .text(),
        license: t
            .msg(Key::AboutLicense)
            .with("license", &wiki.default_license)
            .text(),
        view: t
            .msg(Key::AboutView)
            .with(
                "view",
                wiki.default_view
                    .strip_prefix("heads/")
                    .unwrap_or(&wiki.default_view),
            )
            .text(),
        badge: badge(t, &wiki, false),
        preview: crate::catalog::languages()
            .iter()
            .any(|l| l.review.is_preview()),
    }
    .render();
    answer(StatusCode::OK, &page, rendered)
}

// ------------------------------------------------------------------------------------ verification

fn state_sentence(page: &Page, state: VerificationState) -> String {
    page.get(match state {
        VerificationState::Verified => Key::VerificationStateVerified,
        VerificationState::Failed => Key::VerificationStateFailed,
        VerificationState::CouldNotCheck => Key::VerificationStateCouldNot,
        VerificationState::Unknown => Key::VerificationStateUnknown,
    })
}

fn verification_page(page: &Page, wiki: &WikiRecord, result: Option<CheckView>) -> Response {
    let t = page.t();
    let shell = Shell::of(wiki);
    let page = &page.wiki_head(wiki, Subject::Surface("-/verification"));
    let rendered = VerificationTpl {
        page,
        shell: &shell,
        title: t
            .msg(Key::TitleVerification)
            .with("wiki", wiki.display_name.as_str())
            .product()
            .text(),
        badge: badge(t, wiki, false),
        state_sentence: state_sentence(page, wiki.verification.state),
        checked_at: wiki.verification.at.map(|at| {
            t.msg(Key::VerificationCheckedAt)
                .with("when", utc_minutes(at))
                .text()
        }),
        result,
        action: shell.verification.clone(),
    }
    .render();
    answer(StatusCode::OK, page, rendered)
}

/// `GET /{slug}/-/verification`: the last recorded state, in words, and a button to check again.
pub(crate) async fn verification(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    slug: Result<Path<String>, PathRejection>,
) -> Response {
    let Ok(Path(slug)) = slug else {
        return generic_not_found(&page);
    };
    let Some(services) = services_of(&state) else {
        return generic_not_found(&page);
    };
    let wiki = blocking(move || open(&services, &headers, &slug))
        .await
        .flatten();
    match wiki {
        Some(wiki) => verification_page(&page, &wiki, None),
        None => generic_not_found(&page),
    }
}

/// `POST /{slug}/-/verification`: run `verify` now, through the admission gate, and say what it found. A form
/// from another site is refused; a check that ran a moment ago is not repeated.
pub(crate) async fn check_now(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    slug: Result<Path<String>, PathRejection>,
) -> Response {
    let Ok(Path(slug)) = slug else {
        return generic_not_found(&page);
    };
    let page = page.without_switcher();
    let Some(services) = services_of(&state) else {
        return generic_not_found(&page);
    };
    if !crate::lang::from_this_site(&headers) {
        return answer(StatusCode::FORBIDDEN, &page, render_form_refused(&page));
    }
    let t = page.t();
    let done = blocking(move || {
        let wiki = open(&services, &headers, &slug)?;
        let now = (services.clock)();
        let recent = wiki
            .verification
            .at
            .is_some_and(|at| (0..MIN_RECHECK_SECS).contains(&(now - at)));
        if recent {
            return Some((wiki, vec![t.get(Key::VerificationRecent)], Vec::new(), true));
        }
        let checked = services.verifier.check(&wiki);
        if let Some(why) = &checked.operator {
            (services.log)(&format!(
                "a verification of wiki `{}` could not be completed: {why}",
                wiki.slug
            ));
        }
        let mut lines = Vec::new();
        let mut stages = Vec::new();
        match &checked.outcome {
            CheckOutcome::Verified => lines.push(t.get(Key::VerificationNowVerified)),
            CheckOutcome::Failed { stages: found } => {
                lines.push(t.get(Key::VerificationStateFailed));
                stages = found.iter().map(|s| s.stage.clone()).collect();
            }
            CheckOutcome::CouldNotBeChecked(why) => lines.push(t.get(match why {
                NotChecked::Busy => Key::VerificationBusy,
                NotChecked::Limit => Key::VerificationLimit,
                NotChecked::Unavailable => Key::VerificationUnavailable,
            })),
        }
        if !checked.recorded {
            lines.push(t.get(Key::VerificationNotSaved));
        }
        // The record as it stands now (what the badge will say), or the one we had if it cannot be read.
        let wiki = services.wikis.get(&wiki.id).unwrap_or(wiki);
        Some((wiki, lines, stages, false))
    })
    .await;
    match done {
        Some(Some((wiki, lines, stages, _))) => {
            verification_page(&page, &wiki, Some(CheckView { lines, stages }))
        }
        Some(None) => generic_not_found(&page),
        None => render_failed(&page),
    }
}

// ------------------------------------------------------------------------------------ the list of wikis

/// `GET /-/wikis`: the wikis the visitor may read, by name.
pub(crate) async fn wikis(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
) -> Response {
    let Some(services) = services_of(&state) else {
        return render_list(&page, Vec::new());
    };
    let items = blocking(move || {
        let principal = services.identity.principal(&headers);
        services
            .wikis
            .list()
            .unwrap_or_default()
            .into_iter()
            .filter(|w| {
                services
                    .authorizer
                    .authorize(
                        &principal,
                        Action::Read,
                        &Resource::Wiki { wiki: w.id.clone() },
                    )
                    .is_allowed()
            })
            .map(|w| WikiItem {
                name: w.display_name.as_str().to_owned(),
                href: format!("/{}", w.slug),
            })
            .collect::<Vec<_>>()
    })
    .await;
    match items {
        Some(items) => render_list(&page, items),
        None => render_failed(&page),
    }
}

fn render_list(page: &Page, wikis: Vec<WikiItem>) -> Response {
    answer(StatusCode::OK, page, WikisTpl { page, wikis }.render())
}
