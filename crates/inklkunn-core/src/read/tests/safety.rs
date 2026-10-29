//! What the read path refuses to let through: hostile markup in everything a page carries, a cached row
//! altered in the database, transclusion cycles, and a title that cannot name a page.

use inklkunn_prikk::{StorageError, Unavailable};
use inklkunn_store::PageKey;

use super::super::testing::{Env, World, findings, page};
use super::*;

// ------------------------------------------------------------------------------------ the sanitizer

const HOSTILE: &[&str] = &[
    "<script>alert(1)</script>",
    "<img src=x onerror=alert(2)>",
    "<a href=\"javascript:alert(3)\">click</a>",
    "<a href=\"data:text/html;base64,PHNjcmlwdD5hbGVydCg0KTwvc2NyaXB0Pg==\">click</a>",
    "<iframe src=\"https://evil.example/\"></iframe>",
    "<p onclick=\"alert(5)\">x</p>",
];

/// No element, attribute, class or address that is live or that the profile does not write. Text that merely
/// *mentions* an attack (an escaped title in a data attribute, a percent-encoded address) is not one.
fn assert_inert(what: &str, html: &str) {
    let live = findings(html);
    assert!(live.is_empty(), "{what}: {live:?} in {html}");
    for marker in ["<script", "<iframe", "<object", "<embed"] {
        assert!(
            !html.to_lowercase().contains(marker),
            "{what}: {marker} in {html}"
        );
    }
}

#[test]
fn hostile_markup_in_a_page_body_a_callout_title_an_alias_and_a_description_is_neutralized() {
    let attack = HOSTILE.join(" ");
    let body = format!(
        "Body: {attack}\n\n> [!NOTE] Title: {attack}\n> inside\n\n[[{attack}]]\n\n[[{attack}|label]]\n"
    );
    let alias = HOSTILE.first().copied().unwrap_or("");
    let w = World::pages(&[
        (
            "Victim",
            &[
                &format!(
                    "aliases: [\"{}\", \"<img src=x onerror=alert(2)>\"]",
                    alias.replace('"', "'")
                ),
                &format!("description: \"{}\"", attack.replace('"', "'")),
            ],
            &body,
        ),
        (
            "Linker",
            &[],
            "[[<img src=x onerror=alert(2)>]] and ![[<script>alert(1)</script>]]\n",
        ),
    ]);

    // The body and the callout title: through the renderer and the sanitizer.
    let view = read_page(&w, "Victim");
    assert_inert("a page body and a callout title", view.html.as_str());
    assert!(
        view.html.as_str().contains("Body:"),
        "the text around the attack stays"
    );
    assert!(
        view.html.as_str().contains("class=\"callout"),
        "{}",
        view.html
    );

    // The alias reaches the page's HTML as link text and a target the renderer escaped; a link written
    // with it resolves, and still nothing is live.
    let linker = read_page(&w, "Linker");
    assert_inert("a link by an alias", linker.html.as_str());
    let by_alias = found(
        w.service
            .page(&w.wiki, None, "<img src=x onerror=alert(2)>", &Env::EN),
    );
    assert_eq!(by_alias.meta.title, "Victim");
    assert_inert("a page reached by an alias", by_alias.html.as_str());

    // The alias and the description are plain text in the view: a surface escapes them, and turned into
    // HTML by the sanitizer's own text conversion they are inert whatever they say.
    let sanitizer = w.service.sanitizer();
    for text in view
        .meta
        .aliases
        .iter()
        .map(String::as_str)
        .chain(view.meta.description.as_deref())
        .chain(by_alias.redirected_from.as_ref().map(|r| r.title.as_str()))
    {
        assert!(!text.is_empty());
        let as_html = sanitizer.text(text);
        assert!(!as_html.as_str().contains('<'), "{as_html}");
        assert_inert("a text turned into HTML", as_html.as_str());
        // And it is also what the page's own sanitizer makes of it when it is (wrongly) taken for HTML.
        assert_inert("a text taken for HTML", sanitizer.clean(text).as_str());
    }
    assert!(
        view.meta
            .description
            .as_deref()
            .is_some_and(|d| d.contains("<script>")),
        "kept verbatim as data"
    );
}

#[test]
fn a_cached_row_that_was_altered_in_the_database_is_sanitized_on_the_way_out() {
    let w = World::pages(&[("Home", &[], "Plain.\n")]);
    read_page(&w, "Home");
    // Someone with the database writes an attack into the cached HTML (threat model T-10).
    let tip = w.fake().state_id();
    let key = PageKey {
        wiki: &w.wiki.id,
        tip: &tip,
        path: "pages/Home.md",
        lang: "en",
        presentation: "/w",
    };
    let mut row = inklkunn_store::DerivedStore::page(&*w.store, &key)
        .expect("store")
        .expect("cached");
    row.html = format!("<p>Plain.</p>{}", HOSTILE.join(""));
    inklkunn_store::DerivedStore::put_page(&*w.store, &key, &row).expect("put");
    let view = read_page(&w, "Home");
    assert!(view.cached, "it came from the cache");
    assert_inert("a cached row", view.html.as_str());
    assert!(view.html.as_str().contains("Plain."));
}

// ------------------------------------------------------------------------------------ transclusion

#[test]
fn a_transclusion_cycle_is_refused_with_the_cycle_named_and_the_page_still_renders() {
    let w = World::pages(&[
        ("A", &[], "Start of A.\n\n![[B]]\n\nEnd of A.\n"),
        ("B", &[], "Start of B.\n\n![[A]]\n\nEnd of B.\n"),
    ]);
    let (view, calls) = w.counted(|| read_page(&w, "A"));
    let html = view.html.as_str();
    assert!(html.contains("A → B → A"), "the cycle is named: {html}");
    assert!(
        html.contains("data-transclusion-refusal=\"cycle\""),
        "{html}"
    );
    assert!(
        html.contains("Not included, it would include itself"),
        "the catalog's words: {html}"
    );
    for line in ["Start of A.", "End of A.", "Start of B.", "End of B."] {
        assert!(html.contains(line), "{line}: {html}");
    }
    assert_eq!(view.links.transcluded, ["pages/B.md"]);
    // The page, plus each page it includes: one `cat` each (and the index and the tip, this first time).
    assert_eq!(calls.read, 2);
    assert_eq!(calls.total(), 5);
    // The cached page names it too.
    let again = read_page(&w, "A");
    assert!(again.cached);
    assert_eq!(again.html, view.html);
}

#[test]
fn an_included_page_that_cannot_be_asked_for_is_not_a_page_to_keep() {
    let w = World::pages(&[("Home", &[], "![[Part]]\n"), ("Part", &[], "x\n")]);
    // The host reads; the included page's read fails.
    let first = read_page(&w, "Home");
    assert!(first.html.as_str().contains("class=\"transclusion\""));
    w.fake()
        .set("pages/Part.md", &page(&["title: Part"], "y\n"));
    w.service.forget_wiki(&w.wiki);
    w.fake().fail_read(Some(StorageError::Unavailable {
        verb: "cat",
        reason: Unavailable::Busy,
    }));
    let e = w
        .service
        .page(&w.wiki, None, "Home", &Env::EN)
        .expect_err("a read that failed");
    assert!(
        matches!(e, ReadError::CouldNotBeReadNow(Cause::Storage(_))),
        "{e:?}"
    );
    // Nothing was cached for the new tip.
    w.fake().fail_read(None);
    let recovered = read_page(&w, "Home");
    assert!(!recovered.cached);
    assert!(recovered.html.as_str().contains('y'));
}

// ------------------------------------------------------------------------------------ titles

#[test]
fn a_title_whose_path_exceeds_either_cap_is_refused_before_any_prikk_call() {
    let w = World::pages(&[("Home", &[], "x\n")]);
    let over_component = "x".repeat(256);
    let over_path = vec!["ab"; 400].join("/");
    let encoded = "編".repeat(29);
    for (what, title) in [
        ("a 256-byte component", over_component.as_str()),
        ("a 1025-byte path", over_path.as_str()),
        ("an encoded component of 261 bytes", encoded.as_str()),
        ("an empty title", ""),
        ("an empty component", "a//b"),
        ("a dot component", "a/../b"),
    ] {
        let (page, c1) = w.counted(|| w.service.page(&w.wiki, None, title, &Env::EN));
        assert!(
            matches!(page, Err(ReadError::InvalidTitle(_))),
            "{what}: {page:?}"
        );
        let (look, c2) = w.counted(|| w.service.lookup(&w.wiki, None, title));
        assert!(matches!(look, Err(ReadError::InvalidTitle(_))), "{what}");
        assert_eq!(c1, Calls::default(), "{what}: nothing else saw it");
        assert_eq!(c2, Calls::default(), "{what}");
    }
    assert_eq!(w.calls(), Calls::default());
    // The edge that is allowed is allowed.
    let ok = "x".repeat(255);
    let (out, _) = w.counted(|| w.service.page(&w.wiki, None, &ok, &Env::EN));
    assert!(matches!(out, Ok(PageOutcome::Missing { .. })), "{out:?}");
}
