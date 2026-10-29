//! The stored file on the content origin, and discovery (RFC 003 D-3, D-10; handoff 003 T6): `-/raw/` answers
//! byte for byte and only on the content origin; the link relations of a page's head; the JSON-LD description
//! as a document of its own (the page policy allows no script, so none is added for it); the site description,
//! the OpenSearch description, the sitemap and `robots.txt`; and `noindex` for a wiki that is not public. The
//! repository is an in-memory double, so these run in both test runs.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;
mod json;
#[path = "../changes_feeds/xml.rs"]
mod xml;

use common::{APP, CONTENT, World, get, let_read, page, request, with_accounts};
use inklkunn_store::{AccountId, Visibility};

/// Exactly what the double stores for Home: no final newline, a carriage return, text outside ASCII.
const HOME: &str = "---\r\ntitle: Home\r\ndescription: \"Start here, \\\"friend\\\"\"\r\naliases: [Start]\r\ntags: [intro, 日本語]\r\nlang: en\r\n---\r\n\r\nWelcome — see [[Guide]].\r\nno newline at the end";

fn files() -> Vec<(String, String)> {
    vec![
        ("pages/Home.md".to_owned(), HOME.to_owned()),
        (
            "pages/Guide.md".to_owned(),
            page(&["title: Guide", "aliases: [How-to]"], "The guide.\n"),
        ),
        (
            "pages/Old Name.md".to_owned(),
            page(
                &["title: Old Name", "kind: redirect", "redirect: Guide"],
                "",
            ),
        ),
        (
            "pages/Guides/日本語.md".to_owned(),
            page(&["title: Guides/日本語"], "x\n"),
        ),
    ]
}

fn world(visibility: Visibility, slug: &str, name: &str) -> World {
    let owned = files();
    let borrowed: Vec<(&str, &str)> = owned
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    World::with_visibility(slug, name, visibility, &borrowed)
}

fn handbook() -> World {
    world(Visibility::Public, "handbook", "The Handbook")
}

// ------------------------------------------------------------------------------------ the raw source

#[tokio::test]
async fn the_raw_source_is_the_stored_file_byte_for_byte_on_the_content_origin() {
    let w = handbook();
    let a = request(&w, CONTENT, "GET", "/handbook/-/raw/Home", &[]).await;
    assert_eq!(a.status, 200);
    assert_eq!(a.bytes, HOME.as_bytes(), "byte for byte, CRLF and all");
    assert_eq!(
        a.header("content-type"),
        Some("text/markdown; charset=UTF-8")
    );
    // Inert: the content origin's policy, no sniffing, and not for an index.
    let csp = a.header("content-security-policy").unwrap();
    assert!(
        csp.contains("sandbox") && csp.contains("default-src 'none'"),
        "{csp}"
    );
    assert_eq!(a.header("x-content-type-options"), Some("nosniff"));
    assert_eq!(a.header("x-robots-tag"), Some("noindex, nofollow"));
    // Validators: a tag, and a date in the HTTP form.
    let etag = a.header("etag").unwrap();
    assert!(etag.starts_with('"') && etag.ends_with('"'), "{etag}");
    let modified = a.header("last-modified").unwrap();
    assert!(
        modified.ends_with(" GMT") && modified.contains(','),
        "{modified}"
    );
    // It says where the page is, and what to call the file; it says the title, not a storage path.
    assert_eq!(
        a.header("link"),
        Some("<http://localhost:8080/handbook/Home>; rel=\"canonical\"")
    );
    let disposition = a.header("content-disposition").unwrap();
    assert!(
        disposition.contains("filename*=UTF-8''Home.md"),
        "{disposition}"
    );
    assert!(!disposition.contains("pages/"), "{disposition}");

    // A hierarchy, a title outside ASCII, an alias and a redirect lead to the files they lead to.
    let nested = request(
        &w,
        CONTENT,
        "GET",
        "/handbook/-/raw/Guides/%E6%97%A5%E6%9C%AC%E8%AA%9E",
        &[],
    )
    .await;
    assert_eq!(nested.status, 200);
    assert_eq!(nested.bytes, files().get(3).unwrap().1.as_bytes());
    let alias = request(&w, CONTENT, "GET", "/handbook/-/raw/How-to", &[]).await;
    assert_eq!(alias.bytes, files().get(1).unwrap().1.as_bytes());
    let redirect = request(&w, CONTENT, "GET", "/handbook/-/raw/Old%20Name", &[]).await;
    assert_eq!(redirect.bytes, files().get(1).unwrap().1.as_bytes());
    // HEAD says the same and sends nothing.
    let head = request(&w, CONTENT, "HEAD", "/handbook/-/raw/Home", &[]).await;
    assert_eq!(head.status, 200);
    assert_eq!(head.header("etag"), Some(etag));
    assert!(head.bytes.is_empty());
}

#[tokio::test]
async fn the_application_origin_refuses_the_raw_source_and_serves_no_file_bytes() {
    let w = handbook();
    for uri in [
        "/handbook/-/raw/Home",
        "/handbook/-/raw/Guides/%E6%97%A5%E6%9C%AC%E8%AA%9E",
    ] {
        let a = get(&w, uri).await;
        assert_eq!(a.status, 404, "{uri}");
        assert!(a.body().contains("<h1>Not found</h1>"), "{uri}");
        assert!(!a.body().contains("Welcome"), "{uri}: no file bytes");
        assert!(a.header("etag").is_none(), "{uri}");
        // The application's policy, never the content origin's.
        assert!(
            !a.header("content-security-policy")
                .unwrap()
                .contains("sandbox")
        );
    }
    // And the content origin serves nothing of the application.
    for uri in [
        "/handbook/-/changes",
        "/handbook/Home",
        "/handbook/-/search?q=x",
    ] {
        let a = request(&w, CONTENT, "GET", uri, &[]).await;
        assert_eq!(a.status, 404, "{uri}");
        assert_eq!(a.header("content-type"), Some("text/plain; charset=utf-8"));
    }
}

#[tokio::test]
async fn a_conditional_request_gets_a_304_and_a_changed_page_a_new_tag() {
    let w = handbook();
    let first = request(&w, CONTENT, "GET", "/handbook/-/raw/Home", &[]).await;
    let etag = first.header("etag").unwrap().to_owned();
    let modified = first.header("last-modified").unwrap().to_owned();

    for (name, value) in [
        ("if-none-match", etag.as_str()),
        ("if-none-match", &format!("W/{etag}")),
        ("if-none-match", &format!("\"other\", {etag}")),
        ("if-none-match", "*"),
        ("if-modified-since", modified.as_str()),
    ] {
        let a = request(&w, CONTENT, "GET", "/handbook/-/raw/Home", &[(name, value)]).await;
        assert_eq!(a.status, 304, "{name}: {value}");
        assert!(a.bytes.is_empty());
        assert_eq!(a.header("etag"), Some(etag.as_str()));
        assert_eq!(a.header("last-modified"), Some(modified.as_str()));
        assert_eq!(a.header("x-content-type-options"), Some("nosniff"));
    }
    let other = request(
        &w,
        CONTENT,
        "GET",
        "/handbook/-/raw/Home",
        &[("if-none-match", "\"something-else\"")],
    )
    .await;
    assert_eq!(other.status, 200);
    // The tag wins over the date.
    let both = request(
        &w,
        CONTENT,
        "GET",
        "/handbook/-/raw/Home",
        &[
            ("if-none-match", "\"something-else\""),
            ("if-modified-since", &modified),
        ],
    )
    .await;
    assert_eq!(both.status, 200);

    // The page changes: the tag does, and the old tag fetches the new file.
    w.fake
        .set("pages/Home.md", &page(&["title: Home"], "Rewritten.\n"));
    let changed = request(
        &w,
        CONTENT,
        "GET",
        "/handbook/-/raw/Home",
        &[("if-none-match", &etag)],
    )
    .await;
    assert_eq!(changed.status, 200);
    assert_ne!(changed.header("etag"), Some(etag.as_str()));
    assert!(changed.body().contains("Rewritten."));
}

#[tokio::test]
async fn nothing_to_serve_is_one_answer_whatever_the_reason() {
    let vault = world(Visibility::Restricted, "vault", "The Vault");
    let public = handbook();
    let asks = [
        // A wiki the reader may not read, a wiki that is not there, a title that leads nowhere, a title that
        // cannot be one.
        (&vault, "/vault/-/raw/Home"),
        (&public, "/vault/-/raw/Home"),
        (&public, "/handbook/-/raw/Nothing%20Here"),
        (&public, "/handbook/-/raw/"),
        (&public, "/handbook/-/raw/%FF"),
    ];
    let mut answers = Vec::new();
    for (w, uri) in asks {
        let a = request(w, CONTENT, "GET", uri, &[]).await;
        assert_eq!(a.status, 404, "{uri}");
        assert_eq!(a.header("content-type"), Some("text/plain; charset=utf-8"));
        assert!(a.header("etag").is_none() && a.header("last-modified").is_none());
        answers.push(a.bytes);
    }
    for pair in answers.windows(2) {
        assert_eq!(
            pair.first().unwrap(),
            pair.get(1).unwrap(),
            "the same sentence for every reason"
        );
    }
}

// ------------------------------------------------------------------------------------ the head

#[tokio::test]
async fn a_pages_head_carries_the_link_relations_and_no_script_and_the_policy_is_what_it_was() {
    let w = handbook();
    let a = get(&w, "/handbook/Home").await;
    let body = a.body();
    let head = body.split("</head>").next().unwrap();
    for expected in [
        "<link rel=\"canonical\" href=\"http://localhost:8080/handbook/Home\">",
        "<link rel=\"license\" href=\"https://spdx.org/licenses/CC-BY-SA-4.0.html\">",
        "<link rel=\"search\" href=\"http://localhost:8080/handbook/-/opensearch.xml\" type=\"application/opensearchdescription+xml\"",
        "<link rel=\"alternate\" href=\"http://localhost:8081/handbook/-/raw/Home\" type=\"text/markdown\" title=\"Home as Markdown\">",
        "<link rel=\"alternate\" href=\"http://localhost:8080/handbook/-/changes.atom?page=Home\" type=\"application/atom+xml\" title=\"Changes to Home\">",
        "<link rel=\"alternate\" href=\"http://localhost:8080/handbook/-/changes.atom\" type=\"application/atom+xml\" title=\"Recent changes to The Handbook\">",
        "<link rel=\"describedby\" href=\"http://localhost:8080/handbook/-/describe/Home\" type=\"application/ld+json\">",
        "<link rel=\"describedby\" href=\"http://localhost:8080/handbook/-/site.json\" type=\"application/json\">",
        "<link rel=\"version-history\" href=\"http://localhost:8080/handbook/-/history/Home\">",
        "<link rel=\"bookmark\" href=\"http://localhost:8080/handbook/-/at/",
    ] {
        assert!(head.contains(expected), "{expected}\n{head}");
    }
    // No script, anywhere: JSON-LD is a document of its own.
    assert!(!body.contains("<script"), "{body}");
    let csp = a.header("content-security-policy").unwrap();
    assert!(csp.starts_with("default-src 'none'"), "{csp}");
    assert!(!csp.contains("script-src"), "{csp}");
    // A public wiki is for indexing.
    assert!(!head.contains("name=\"robots\""));
    assert!(a.header("x-robots-tag").is_none());
    // A page of the wiki that is not a page has the wiki's relations and no page's.
    let about = get(&w, "/handbook/-/about").await.body();
    let about_head = about.split("</head>").next().unwrap();
    assert!(
        about_head.contains("rel=\"canonical\" href=\"http://localhost:8080/handbook/-/about\"")
    );
    assert!(!about_head.contains("text/markdown"));
    assert!(!about_head.contains("version-history"));
    // A page in another view is its own canonical address.
    let in_view = get(&w, "/handbook/~main/Home").await.body();
    assert!(
        in_view.contains("rel=\"canonical\" href=\"http://localhost:8080/handbook/~main/Home\"")
    );
}

#[tokio::test]
async fn a_wiki_that_is_not_public_asks_not_to_be_indexed_in_the_page_and_in_the_header() {
    let w = with_accounts(world(Visibility::Restricted, "vault", "The Vault"));
    let reader = AccountId::new();
    let_read(&w, &reader);
    let who = reader.to_string();
    let a = request(&w, APP, "GET", "/vault/Home", &[("x-test-account", &who)]).await;
    assert_eq!(a.status, 200, "{}", a.body());
    assert!(
        a.body()
            .contains("<meta name=\"robots\" content=\"noindex, nofollow\">")
    );
    assert_eq!(a.header("x-robots-tag"), Some("noindex, nofollow"));
    // The notice pages of it too.
    let missing = request(
        &w,
        APP,
        "GET",
        "/vault/Nothing",
        &[("x-test-account", &who)],
    )
    .await;
    assert_eq!(missing.status, 404);
    assert_eq!(missing.header("x-robots-tag"), Some("noindex, nofollow"));
    // The site description of a wiki that is not public names no sitemap; there is none.
    let site = request(
        &w,
        APP,
        "GET",
        "/vault/-/site.json",
        &[("x-test-account", &who)],
    )
    .await;
    assert_eq!(site.status, 200);
    assert!(json::parse(&site.body()).unwrap().get("sitemap").is_none());
    let sitemap = request(
        &w,
        APP,
        "GET",
        "/vault/-/sitemap.xml",
        &[("x-test-account", &who)],
    )
    .await;
    assert_eq!(sitemap.status, 404, "a sitemap is for public wikis");
}

// ------------------------------------------------------------------------------------ the documents

#[tokio::test]
async fn a_pages_schema_org_description_is_a_json_ld_document_of_its_own() {
    let w = handbook();
    let a = get(&w, "/handbook/-/describe/Home").await;
    assert_eq!(a.status, 200, "{}", a.body());
    assert_eq!(
        a.header("content-type"),
        Some("application/ld+json; charset=utf-8")
    );
    let doc = json::parse(&a.body()).unwrap_or_else(|e| panic!("{e}\n{}", a.body()));
    let text = |k: &str| doc.get(k).and_then(json::Json::text).map(str::to_owned);
    assert_eq!(text("@context").as_deref(), Some("https://schema.org"));
    assert_eq!(text("@type").as_deref(), Some("Article"));
    assert_eq!(text("headline").as_deref(), Some("Home"));
    assert_eq!(text("name").as_deref(), Some("Home"));
    assert_eq!(
        text("description").as_deref(),
        Some("Start here, \"friend\"")
    );
    assert_eq!(text("inLanguage").as_deref(), Some("en"));
    assert_eq!(
        text("url").as_deref(),
        Some("http://localhost:8080/handbook/Home")
    );
    assert_eq!(text("mainEntityOfPage"), text("url"));
    assert_eq!(
        text("license").as_deref(),
        Some("https://spdx.org/licenses/CC-BY-SA-4.0.html")
    );
    assert_eq!(
        doc.get("alternateName")
            .unwrap()
            .list()
            .unwrap()
            .first()
            .unwrap()
            .text(),
        Some("Start")
    );
    let tags: Vec<&str> = doc
        .get("keywords")
        .unwrap()
        .list()
        .unwrap()
        .iter()
        .filter_map(json::Json::text)
        .collect();
    assert_eq!(tags, ["intro", "日本語"]);
    assert_eq!(
        doc.get("isPartOf").unwrap().get("name").unwrap().text(),
        Some("The Handbook")
    );
    // Not for a page that is not there.
    assert_eq!(get(&w, "/handbook/-/describe/Nothing").await.status, 404);
}

#[tokio::test]
async fn what_an_author_wrote_cannot_break_out_of_the_json_ld() {
    let hostile = "\"}],\"@type\":\"Evil\",\"x\":[{\"y\":\"</script><script>alert(1)</script>\\";
    let w = World::new(
        "handbook",
        "The Handbook",
        &[(
            "pages/Hostile.md",
            &format!(
                "---\ntitle: \"{}\"\ndescription: \"{}\"\n---\n\nx\n",
                "Hostile \\\"title\\\"",
                hostile.replace('\\', "\\\\").replace('"', "\\\"")
            ),
        )],
    );
    let a = get(&w, "/handbook/-/describe/Hostile%20%22title%22").await;
    assert_eq!(a.status, 200, "{}", a.body());
    let doc = json::parse(&a.body()).unwrap_or_else(|e| panic!("{e}\n{}", a.body()));
    assert_eq!(
        doc.get("@type").unwrap().text(),
        Some("Article"),
        "no key was added by the text"
    );
    assert_eq!(doc.get("description").unwrap().text(), Some(hostile));
    assert!(doc.get("x").is_none());
    // The three characters of markup are written as escapes, as the API writes them (R10).
    let body = a.body();
    assert!(
        !body.contains(['<', '>', '&']),
        "markup characters in the JSON-LD: {body}"
    );
    assert!(
        body.contains("\\u003c/script\\u003e\\u003cscript\\u003e"),
        "{body}"
    );
}

#[tokio::test]
async fn the_site_description_writes_markup_characters_as_json_escapes() {
    let w = World::new("handbook", "Docs & <More>", &[]);
    let a = get(&w, "/handbook/-/site.json").await;
    assert_eq!(a.status, 200, "{}", a.body());
    let body = a.body();
    assert!(!body.contains(['<', '>', '&']), "{body}");
    assert!(body.contains("Docs \\u0026 \\u003cMore\\u003e"), "{body}");
    let doc = json::parse(&body).unwrap();
    assert_eq!(doc.get("name").unwrap().text(), Some("Docs & <More>"));
}

#[tokio::test]
async fn the_site_description_names_what_the_wiki_offers() {
    let w = handbook();
    let a = get(&w, "/handbook/-/site.json").await;
    assert_eq!(a.status, 200);
    assert_eq!(
        a.header("content-type"),
        Some("application/json; charset=utf-8")
    );
    let doc = json::parse(&a.body()).unwrap();
    let text = |d: &json::Json, k: &str| d.get(k).and_then(json::Json::text).map(str::to_owned);
    assert_eq!(
        text(&doc, "format").as_deref(),
        Some("wiki-site-description")
    );
    assert_eq!(text(&doc, "name").as_deref(), Some("The Handbook"));
    assert_eq!(
        text(&doc, "url").as_deref(),
        Some("http://localhost:8080/handbook")
    );
    assert_eq!(text(&doc, "license").as_deref(), Some("CC-BY-SA-4.0"));
    assert_eq!(
        text(doc.get("feeds").unwrap(), "recent_changes").as_deref(),
        Some("http://localhost:8080/handbook/-/changes.atom")
    );
    assert_eq!(
        text(&doc, "search").as_deref(),
        Some("http://localhost:8080/handbook/-/opensearch.xml")
    );
    assert_eq!(
        text(&doc, "sitemap").as_deref(),
        Some("http://localhost:8080/handbook/-/sitemap.xml")
    );
    assert_eq!(
        text(doc.get("engine").unwrap(), "name").as_deref(),
        Some("inklkunn")
    );
}

#[tokio::test]
async fn the_opensearch_description_names_the_search_address() {
    let w = handbook();
    let a = get(&w, "/handbook/-/opensearch.xml").await;
    assert_eq!(a.status, 200);
    assert_eq!(
        a.header("content-type"),
        Some("application/opensearchdescription+xml; charset=utf-8")
    );
    let doc = xml::parse(&a.body()).unwrap_or_else(|e| panic!("{e}\n{}", a.body()));
    assert_eq!(doc.name, "OpenSearchDescription");
    assert_eq!(doc.first("ShortName").unwrap().text, "The Handbook");
    assert_eq!(
        doc.first("Description").unwrap().text,
        "Search the titles and aliases of The Handbook"
    );
    let url = doc.first("Url").unwrap();
    assert_eq!(url.attr("type"), Some("text/html"));
    assert_eq!(
        url.attr("template"),
        Some("http://localhost:8080/handbook/-/search?q={searchTerms}")
    );
}

#[tokio::test]
async fn the_sitemap_lists_the_pages_of_a_public_wiki_and_robots_txt_names_only_public_wikis() {
    let w = handbook();
    let a = get(&w, "/handbook/-/sitemap.xml").await;
    assert_eq!(a.status, 200);
    assert_eq!(
        a.header("content-type"),
        Some("application/xml; charset=utf-8")
    );
    let doc = xml::parse(&a.body()).unwrap_or_else(|e| panic!("{e}\n{}", a.body()));
    assert_eq!(
        doc.attr("xmlns"),
        Some("http://www.sitemaps.org/schemas/sitemap/0.9")
    );
    let locs: Vec<String> = doc
        .all("url")
        .map(|u| u.first("loc").unwrap().text.clone())
        .collect();
    assert_eq!(
        locs,
        [
            "http://localhost:8080/handbook/Guide",
            "http://localhost:8080/handbook/Guides/%E6%97%A5%E6%9C%AC%E8%AA%9E",
            "http://localhost:8080/handbook/Home",
        ],
        "pages in path order; a redirect is no page of its own"
    );

    // robots.txt: fixed rules, and the sitemaps of the wikis anyone may read.
    let r = get(&w, "/robots.txt").await;
    assert_eq!(r.status, 200);
    assert_eq!(r.header("content-type"), Some("text/plain; charset=utf-8"));
    let body = r.body();
    assert!(body.contains("User-agent: *"));
    assert!(body.contains("Disallow: /-/"));
    assert!(
        body.contains("Sitemap: http://localhost:8080/handbook/-/sitemap.xml"),
        "{body}"
    );

    // A wiki that is not public is not named by anything an anonymous visitor reads, and has no sitemap for one.
    let w = with_accounts(world(Visibility::Restricted, "vault", "The Vault"));
    let body = get(&w, "/robots.txt").await.body();
    assert!(!body.contains("vault"), "{body}");
    assert!(!body.contains("Sitemap:"), "{body}");
    assert_eq!(get(&w, "/vault/-/sitemap.xml").await.status, 404);
}

#[tokio::test]
async fn every_document_of_a_wiki_that_is_not_there_is_the_same_answer_as_one_that_is_not_readable()
{
    let vault = world(Visibility::Restricted, "vault", "The Vault");
    let nothing = handbook();
    for tail in [
        "-/site.json",
        "-/describe/Home",
        "-/opensearch.xml",
        "-/sitemap.xml",
        "-/search?q=home",
        "-/health",
        "-/health/orphans",
        "-/links/Home",
    ] {
        let a = get(&vault, &format!("/vault/{tail}")).await;
        let b = get(&nothing, &format!("/vault/{tail}")).await;
        assert_eq!(a.status, 404, "{tail}");
        assert_eq!(a.status, b.status, "{tail}");
        assert_eq!(a.bytes, b.bytes, "{tail}");
        assert!(!a.body().contains("Vault"), "{tail}");
    }
}
