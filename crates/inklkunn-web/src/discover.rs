//! Discovery (RFC 003 D-10; requirements IX-6, RD-2; wiki-commons chapter 11): what lets a tool find out what a
//! wiki offers from any of its pages, and what a crawler is told.
//!
//! - **The link relations of a page's head** ([`Page::wiki_head`]): `canonical`, `license`, `search`,
//!   `alternate` for the feed of the wiki, and, on a page, `alternate` for its Markdown source (on the content
//!   origin) and for its feed, `describedby` for its JSON-LD, `bookmark` and `version-history`.
//! - **schema.org JSON-LD (META-12) as a separate document.** The page policy is `default-src 'none'` and
//!   carries no `script-src`, and the only way to put JSON-LD inside an HTML page is a `<script>` element.
//!   The policy is not weakened for it: the description is served at `-/describe/<Title>` as
//!   `application/ld+json` and each page links to it with `rel="describedby"`.
//! - **The site description document** (`-/site.json`, wiki-commons API-1), **the OpenSearch description**,
//!   **`-/sitemap.xml`** for a public wiki and **`/robots.txt`** for the server.
//! - **`noindex`** for every wiki that is not public, on the page (`<meta>`) and in the header
//!   (`X-Robots-Tag`).
//!
//! `robots.txt` names the sitemaps of **public wikis only**: a wiki that is not public must not be named by
//! anything an anonymous visitor can read (existence neutrality, threat model INV-4).

use std::fmt::Write as _;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use inklkunn_core::read::PageOutcome;
use inklkunn_store::{Visibility, WikiRecord};

use crate::catalog::Key;
use crate::config::Origin;
use crate::feed::escape as xml;
use crate::pages::{self, Page, PageState};
use crate::present::WikiLinks;
use crate::url::encode_title;
use crate::wiki::{blocking, open};

/// The two origins, for the addresses that must be absolute: a feed, a sitemap, a link to the Markdown source
/// on the content origin.
#[derive(Debug, Clone)]
pub(crate) struct Origins {
    app: Origin,
    content: Origin,
}

impl Origins {
    pub(crate) fn new(app: Origin, content: Origin) -> Self {
        Origins { app, content }
    }

    /// An address on the application origin; `path` starts with `/`.
    pub(crate) fn app(&self, path: &str) -> String {
        format!("{}{path}", self.app)
    }

    /// An address on the content origin; `path` starts with `/`.
    pub(crate) fn content(&self, path: &str) -> String {
        format!("{}{path}", self.content)
    }

    pub(crate) fn app_origin(&self) -> &Origin {
        &self.app
    }

    pub(crate) fn content_origin(&self) -> &Origin {
        &self.content
    }
}

/// One `<link>` of a page's head.
#[derive(Debug, Clone)]
pub(crate) struct HeadLink {
    pub rel: &'static str,
    pub href: String,
    pub media_type: Option<&'static str>,
    pub title: Option<String>,
}

/// What a page is about, for its head.
pub(crate) enum Subject<'a> {
    /// A surface of the wiki with no address of its own worth naming (a notice).
    Wiki,
    /// A surface of the wiki: `path` is what follows `/<slug>/` (`-/about`).
    Surface(&'a str),
    /// A page, in a view, at a block.
    Page {
        title: &'a str,
        view: Option<&'a str>,
        block: Option<&'a str>,
    },
}

/// The address of an SPDX license, if the expression is one identifier.
fn license_href(license: &str) -> Option<String> {
    let simple = !license.is_empty()
        && license.len() <= 64
        && license
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'));
    simple.then(|| format!("https://spdx.org/licenses/{license}.html"))
}

impl Page {
    /// This page with the head a page of `wiki` carries: the link relations that apply to `subject`, and
    /// `noindex` when the wiki is not public. A page built without the site's origins carries none.
    #[must_use]
    pub(crate) fn wiki_head(&self, wiki: &WikiRecord, subject: Subject<'_>) -> Page {
        let mut page = self.clone();
        page.noindex = wiki.visibility != Visibility::Public;
        let Some(origins) = self.origins.clone() else {
            return page;
        };
        let t = self.t();
        let base = format!("/{}", wiki.slug);
        let name = wiki.display_name.as_str();
        let mut head = Vec::new();
        let mut link = |rel, href: String, media_type, title: Option<String>| {
            head.push(HeadLink {
                rel,
                href,
                media_type,
                title,
            });
        };
        match &subject {
            Subject::Wiki => {}
            Subject::Surface(path) => link(
                "canonical",
                origins.app(&format!("{base}/{path}")),
                None,
                None,
            ),
            Subject::Page { title, view, .. } => {
                let at = match view {
                    Some(v) => format!("{base}/~{v}/{}", encode_title(title)),
                    None => format!("{base}/{}", encode_title(title)),
                };
                link("canonical", origins.app(&at), None, None);
            }
        }
        if let Some(href) = license_href(&wiki.default_license) {
            link("license", href, None, None);
        }
        link(
            "search",
            origins.app(&format!("{base}/-/opensearch.xml")),
            Some("application/opensearchdescription+xml"),
            Some(t.msg(Key::HeadSearch).with("wiki", name).text()),
        );
        link(
            "alternate",
            origins.app(&format!("{base}/-/changes.atom")),
            Some("application/atom+xml"),
            Some(t.msg(Key::HeadFeedWiki).with("wiki", name).text()),
        );
        link(
            "describedby",
            origins.app(&format!("{base}/-/site.json")),
            Some("application/json"),
            None,
        );
        if let Subject::Page { title, block, .. } = &subject {
            let enc = encode_title(title);
            link(
                "alternate",
                origins.content(&format!("{base}/-/raw/{enc}")),
                Some("text/markdown"),
                Some(t.msg(Key::HeadMarkdown).with("page", *title).text()),
            );
            link(
                "alternate",
                origins.app(&format!(
                    "{base}/-/changes.atom?page={}",
                    query_escape(title)
                )),
                Some("application/atom+xml"),
                Some(t.msg(Key::HeadFeedPage).with("page", *title).text()),
            );
            link(
                "describedby",
                origins.app(&format!("{base}/-/describe/{enc}")),
                Some("application/ld+json"),
                None,
            );
            link(
                "version-history",
                origins.app(&format!("{base}/-/history/{enc}")),
                None,
                None,
            );
            if let Some(block) = block {
                link(
                    "bookmark",
                    origins.app(&format!("{base}/-/at/{block}/{enc}")),
                    None,
                    None,
                );
            }
        }
        page.head = head;
        page
    }
}

/// A value for a query string: everything but unreserved characters percent-encoded.
pub(crate) fn query_escape(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

// ------------------------------------------------------------------------------------ the documents

/// `text` as a JSON string, quotes included.
pub(crate) fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || matches!(c, '\u{2028}' | '\u{2029}') => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_list(items: &[String]) -> String {
    let parts: Vec<String> = items.iter().map(|i| json_string(i)).collect();
    format!("[{}]", parts.join(","))
}

fn document(media_type: &'static str, body: String) -> Response {
    let mut res = (StatusCode::OK, body).into_response();
    res.headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(media_type));
    res
}

/// The answer for a document of a wiki that is not there, or not readable: the same sentence, as text.
fn no_document(page: &Page) -> Response {
    pages::plain(
        StatusCode::NOT_FOUND,
        page.t().lang(),
        page.get(Key::NotFoundBody),
    )
}

/// The wiki a request is about, if its principal may read it; and the services.
pub(crate) async fn resolve(
    state: &PageState,
    headers: HeaderMap,
    slug: String,
) -> Option<(Arc<crate::services::Services>, WikiRecord)> {
    let services = state.services.clone()?;
    let for_work = services.clone();
    let wiki = blocking(move || open(&for_work, &headers, &slug))
        .await
        .flatten()?;
    Some((services, wiki))
}

/// `GET /{slug}/-/site.json`: the site description document (wiki-commons API-1).
pub(crate) async fn site_description(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Response {
    let Some((_, wiki)) = resolve(&state, headers, slug).await else {
        return no_document(&page);
    };
    let origins = &state.origins;
    let base = format!("/{}", wiki.slug);
    let public = wiki.visibility == Visibility::Public;
    let mut doc = String::from("{");
    let _ = write!(
        doc,
        "\"format\":\"wiki-site-description\",\"version\":\"0.1\",\"name\":{},\"url\":{},",
        json_string(wiki.display_name.as_str()),
        json_string(&origins.app(&base))
    );
    let _ = write!(
        doc,
        "\"engine\":{{\"name\":\"inklkunn\",\"version\":{}}},",
        json_string(env!("CARGO_PKG_VERSION"))
    );
    let _ = write!(doc, "\"lang\":{},", json_list(&wiki.languages));
    let _ = write!(
        doc,
        "\"license\":{},\"markup\":{{\"profile\":\"portable-wiki-markdown\",\"native_format\":\"text/markdown\"}},",
        json_string(&wiki.default_license)
    );
    let _ = write!(
        doc,
        "\"feeds\":{{\"recent_changes\":{}}},\"search\":{},",
        json_string(&origins.app(&format!("{base}/-/changes.atom"))),
        json_string(&origins.app(&format!("{base}/-/opensearch.xml")))
    );
    if public {
        let _ = write!(
            doc,
            "\"sitemap\":{},",
            json_string(&origins.app(&format!("{base}/-/sitemap.xml")))
        );
    }
    doc.push_str("\"interwiki\":{}}");
    document("application/json; charset=utf-8", doc)
}

/// `GET /{slug}/-/describe/{*title}`: the schema.org description of a page (META-12), as JSON-LD.
pub(crate) async fn describe_page(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    Path((slug, title)): Path<(String, String)>,
) -> Response {
    let Some((services, wiki)) = resolve(&state, headers, slug).await else {
        return no_document(&page);
    };
    let t = page.t();
    let origins = state.origins.clone();
    let wiki_for_work = wiki.clone();
    let found = blocking(move || {
        let links = WikiLinks::new(t, wiki_for_work.slug.as_str(), None);
        services.read.page(&wiki_for_work, None, &title, &links)
    })
    .await;
    let Some(Ok(PageOutcome::Found(view))) = found else {
        return no_document(&page);
    };
    let base = format!("/{}", wiki.slug);
    let url = json_string(&origins.app(&format!("{base}/{}", encode_title(&view.meta.title))));
    let license = view
        .meta
        .license
        .clone()
        .unwrap_or_else(|| wiki.default_license.clone());
    let title = json_string(&view.meta.title);
    let mut parts = vec![
        "\"@context\":\"https://schema.org\"".to_owned(),
        "\"@type\":\"Article\"".to_owned(),
        format!("\"headline\":{title}"),
        format!("\"name\":{title}"),
    ];
    if !view.meta.aliases.is_empty() {
        parts.push(format!(
            "\"alternateName\":{}",
            json_list(&view.meta.aliases)
        ));
    }
    if let Some(id) = &view.meta.id {
        parts.push(format!("\"identifier\":{}", json_string(id)));
    }
    if let Some(description) = &view.meta.description {
        parts.push(format!("\"description\":{}", json_string(description)));
    }
    if !view.meta.tags.is_empty() {
        parts.push(format!("\"keywords\":{}", json_list(&view.meta.tags)));
    }
    if let Some(lang) = &view.meta.lang {
        parts.push(format!("\"inLanguage\":{}", json_string(lang)));
    }
    parts.push(format!(
        "\"license\":{}",
        json_string(&license_href(&license).unwrap_or(license))
    ));
    parts.push(format!(
        "\"creativeWorkStatus\":{}",
        json_string(&view.meta.status)
    ));
    parts.push(format!("\"url\":{url}"));
    parts.push(format!("\"mainEntityOfPage\":{url}"));
    parts.push(format!(
        "\"isPartOf\":{{\"@type\":\"WebSite\",\"name\":{},\"url\":{}}}",
        json_string(wiki.display_name.as_str()),
        json_string(&origins.app(&base))
    ));
    let doc = format!("{{{}}}", parts.join(","));
    document("application/ld+json; charset=utf-8", doc)
}

/// `GET /{slug}/-/opensearch.xml`: the OpenSearch description (wiki-commons API-7).
pub(crate) async fn opensearch(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Response {
    let Some((_, wiki)) = resolve(&state, headers, slug).await else {
        return no_document(&page);
    };
    let name = wiki.display_name.as_str();
    let short: String = name.chars().take(16).collect();
    let template = state.origins.app(&format!("/{}/-/search?q=", wiki.slug));
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <OpenSearchDescription xmlns=\"http://a9.com/-/spec/opensearch/1.1/\">\n\
         <ShortName>{}</ShortName>\n<Description>{}</Description>\n<InputEncoding>UTF-8</InputEncoding>\n\
         <Url type=\"text/html\" method=\"get\" template=\"{}{{searchTerms}}\"/>\n\
         </OpenSearchDescription>\n",
        xml(&short),
        xml(&page
            .t()
            .msg(Key::SearchOpenSearch)
            .with("wiki", name)
            .text()),
        xml(&template)
    );
    document("application/opensearchdescription+xml; charset=utf-8", body)
}

/// The most addresses a sitemap lists (the sitemap protocol's own limit).
const SITEMAP_MAX: usize = 50_000;

/// `GET /{slug}/-/sitemap.xml`: the pages of a **public** wiki's default view; any other wiki answers as if
/// it were not there.
pub(crate) async fn sitemap(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    Path(slug): Path<String>,
) -> Response {
    let Some((services, wiki)) = resolve(&state, headers, slug).await else {
        return no_document(&page);
    };
    if wiki.visibility != Visibility::Public {
        return no_document(&page);
    }
    let for_work = wiki.clone();
    let index = blocking(move || services.read.title_index(&for_work, None)).await;
    let Some(Ok(index)) = index else {
        return pages::plain(
            StatusCode::SERVICE_UNAVAILABLE,
            page.t().lang(),
            page.get(Key::UnavailableBody),
        );
    };
    let mut body = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for entry in index
        .entries()
        .iter()
        .filter(|e| e.kind == "page" && e.problem.is_none())
        .take(SITEMAP_MAX)
    {
        let loc = state
            .origins
            .app(&format!("/{}/{}", wiki.slug, encode_title(&entry.title)));
        let _ = writeln!(body, "<url><loc>{}</loc></url>", xml(&loc));
    }
    body.push_str("</urlset>\n");
    document("application/xml; charset=utf-8", body)
}

/// `GET /robots.txt`: what crawlers may fetch, and the sitemaps of the public wikis. A wiki that is not
/// public is not named: its pages say `noindex`, and anonymous visitors, crawlers included, are told it is not
/// there.
pub(crate) async fn robots(State(state): State<PageState>) -> Response {
    let mut body =
        String::from("User-agent: *\nAllow: /*/-/sitemap.xml$\nDisallow: /-/\nDisallow: /*/-/\n");
    if let Some(services) = state.services.clone() {
        let slugs = blocking(move || {
            services
                .wikis
                .list()
                .unwrap_or_default()
                .into_iter()
                .filter(|w| w.visibility == Visibility::Public)
                .map(|w| w.slug.to_string())
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        for slug in slugs {
            let _ = writeln!(
                body,
                "Sitemap: {}",
                state.origins.app(&format!("/{slug}/-/sitemap.xml"))
            );
        }
    }
    let mut res = (StatusCode::OK, body).into_response();
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    res
}

#[cfg(test)]
mod tests;
