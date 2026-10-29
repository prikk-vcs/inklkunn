//! The derived cache and the index it serves: identical pages after a rebuild (ENF-5), what a view
//! carries, aliases and redirects, the cache's keys, the tips it keeps and the index in the store.

use std::sync::Arc;
use std::time::Duration;

use inklkunn_prikk::{StorageError, Unavailable};
use inklkunn_store::PageKey;

use super::super::testing::{Env, World, page};
use super::*;

// ------------------------------------------------------------------------------------ the derived cache

/// Pages that use the whole path: links of every kind, an alias, a redirect, a transclusion, headings,
/// block ids, callouts, a table, a code block, hostile text, other languages.
fn varied() -> World {
    World::pages(&[
        (
            "Home",
            &["description: The front page", "tags: [start, 日本語]"],
            "# Welcome\n\nSee [[Guide]], [[How-to]], [[Old Name]], [[Missing]], [[Home#Welcome]] and [[#Welcome]]. ^intro\n\n\
             ![[Glossary]]\n\n> [!NOTE] A note\n> Body ^n1\n\n| a | b |\n|---|---|\n| 1 | 2 |\n^t1\n\n```rust\nfn main() {}\n```\n^c1\n\n\
             - [ ] task ^i1\n- [x] done\n\nText with #tag and :kbd[Ctrl]{} and a [link](https://example.org).\n",
        ),
        (
            "Guide",
            &["aliases: [How-to, 手引き]"],
            "## Steps\n\nGuide text. [[Home]]\n",
        ),
        ("Old Name", &["kind: redirect", "redirect: Guide"], ""),
        ("Glossary", &[], "# Terms\n\nA *term*. ^term\n"),
        ("日本語", &["lang: ja"], "日本語のページ。[[Guide]]\n"),
    ])
}

#[test]
fn the_cache_cleared_and_every_view_rebuilt_yields_identical_pages() {
    let w = varied();
    let titles = ["Home", "Guide", "Glossary", "日本語", "How-to", "Old Name"];
    let render_all = |env: &Env| -> Vec<PageView> {
        titles
            .iter()
            .map(|t| found(w.service.page(&w.wiki, None, t, env)))
            .collect()
    };

    // The alias and the redirect lead to the page `Guide` already rendered: four renderings, two shared.
    let first = render_all(&Env::EN);
    assert_eq!(first.iter().filter(|v| !v.cached).count(), 4);
    let hits = render_all(&Env::EN);
    assert!(hits.iter().all(|v| v.cached));
    assert_eq!(
        strip(&first),
        strip(&hits),
        "a hit is the page that was rendered"
    );

    // Clear everything derived, in the store and in memory (ENF-5), and rebuild every view.
    let removed = w.service.clear_derived(&w.wiki).expect("clear");
    assert!(removed > 0, "there was something to clear");
    let rebuilt = render_all(&Env::EN);
    assert_eq!(
        rebuilt.iter().filter(|v| !v.cached).count(),
        4,
        "all derived again"
    );
    assert_eq!(
        strip(&first),
        strip(&rebuilt),
        "identical HTML, metadata, links and headings"
    );
    assert_eq!(
        first.iter().map(|v| v.html.as_str()).collect::<Vec<_>>(),
        rebuilt.iter().map(|v| v.html.as_str()).collect::<Vec<_>>()
    );

    // A restart: another service over the same repository and a store with nothing in it.
    let fresh = Arc::new(inklkunn_store::MemoryStore::new());
    let restarted = ReadService::new(
        w.counting.clone(),
        fresh.clone(),
        fresh,
        w.layout.clone(),
        ReadConfig::default(),
        None,
    );
    let again: Vec<PageView> = titles
        .iter()
        .map(|t| found(restarted.page(&w.wiki, None, t, &Env::EN)))
        .collect();
    assert_eq!(strip(&first), strip(&again));

    // The Japanese catalog is another rendering, with its own words, and equally reproducible.
    let ja = render_all(&Env::JA);
    let home = ja.first().expect("home");
    assert!(
        home.html.as_str().contains("Glossary から"),
        "{}",
        home.html
    );
    w.service.clear_derived(&w.wiki).expect("clear");
    assert_eq!(strip(&ja), strip(&render_all(&Env::JA)));
}

/// A page as a surface sees it, without the flag that says where it came from.
fn strip(views: &[PageView]) -> Vec<PageView> {
    views
        .iter()
        .cloned()
        .map(|mut v| {
            v.cached = false;
            v
        })
        .collect()
}

#[test]
fn a_view_carries_its_metadata_headings_links_and_where_it_came_from() {
    let w = varied();
    let home = read_page(&w, "Home");
    assert_eq!(home.meta.title, "Home");
    assert_eq!(home.meta.description.as_deref(), Some("The front page"));
    assert_eq!(home.meta.tags, ["start", "日本語"]);
    assert_eq!(home.meta.kind, "page");
    assert_eq!(home.meta.status, "stable");
    assert_eq!(home.path, "pages/Home.md");
    assert_eq!(home.via, Via::Exact);
    assert_eq!(home.redirected_from, None);
    assert_eq!(home.tip.view, "heads/main");
    assert_eq!(home.tip.state_id, w.fake().state_id());
    assert_eq!(
        home.toc
            .iter()
            .map(|h| (h.level, h.id.as_str()))
            .collect::<Vec<_>>(),
        [(1, "welcome")],
        "the page's own headings, not those of the page it includes"
    );
    assert_eq!(
        home.links.resolved,
        [
            // The embed is an edge too (the page includes it); sorted by target.
            ("Glossary".to_owned(), "pages/Glossary.md".to_owned()),
            ("Guide".to_owned(), "pages/Guide.md".to_owned()),
            ("Home".to_owned(), "pages/Home.md".to_owned()),
            ("How-to".to_owned(), "pages/Guide.md".to_owned()),
            ("Old Name".to_owned(), "pages/Old Name.md".to_owned()),
        ]
    );
    assert_eq!(home.links.dangling, ["Missing"]);
    assert_eq!(home.links.transcluded, ["pages/Glossary.md"]);
    let html = home.html.as_str();
    assert!(html.contains("<p id=\"^intro\">"), "{html}");
    assert!(html.contains("<table id=\"^t1\">"), "{html}");
    assert!(
        html.contains("<pre id=\"^c1\"><code class=\"language-rust\">"),
        "{html}"
    );
    assert!(html.contains("<li id=\"^i1\">"), "{html}");
    assert!(html.contains("class=\"transclusion\""), "{html}");
    assert!(html.contains("<h3 id=\"t1-terms\">Terms</h3>"), "{html}");
    assert!(html.contains("href=\"#welcome\""), "{html}");
}

#[test]
fn an_alias_and_a_redirect_answer_with_the_page_and_say_redirected_from() {
    let w = varied();
    let via_alias = read_page(&w, "How-to");
    assert_eq!(via_alias.meta.title, "Guide");
    assert_eq!(via_alias.path, "pages/Guide.md");
    assert_eq!(
        via_alias.redirected_from,
        Some(RedirectedFrom {
            title: "How-to".into(),
            by: RedirectedBy::Alias
        })
    );
    assert_eq!(via_alias.via, Via::Alias);
    let via_redirect = read_page(&w, "Old Name");
    assert_eq!(via_redirect.meta.title, "Guide");
    assert_eq!(
        via_redirect.redirected_from,
        Some(RedirectedFrom {
            title: "Old Name".into(),
            by: RedirectedBy::Redirect
        })
    );
    // They are the same cached page: the second asks prikk for nothing.
    let (_, c) = w.counted(|| read_page(&w, "手引き"));
    assert_eq!(c.total(), 0);
    // The title itself is not redirected from anywhere.
    assert_eq!(read_page(&w, "Guide").redirected_from, None);
}

#[test]
fn what_a_title_does_not_lead_to_is_an_outcome_not_an_error() {
    let w = World::pages(&[
        ("Home", &[], "x\n"),
        ("Loop A", &["redirect: Loop B"], ""),
        ("Loop B", &["redirect: Loop A"], ""),
        ("A/Same", &[], "x\n"),
        ("B/Same", &[], "x\n"),
    ]);
    let (out, calls) = w.counted(|| w.service.page(&w.wiki, None, "Nope", &Env::EN));
    assert_eq!(
        out.expect("read"),
        PageOutcome::Missing {
            title: "Nope".into()
        }
    );
    assert_eq!(
        calls.read, 0,
        "a title the index does not have costs no `cat`"
    );
    assert!(matches!(
        w.service
            .page(&w.wiki, None, "Loop A", &Env::EN)
            .expect("read"),
        PageOutcome::BrokenRedirect {
            problem: RedirectProblem::Loop,
            ..
        }
    ));
    assert!(matches!(
        w.service.page(&w.wiki, None, "Same", &Env::EN).expect("read"),
        PageOutcome::Ambiguous { candidates } if candidates.len() == 2
    ));
    // Lookup alone reads nothing.
    let (l, c) = w.counted(|| w.service.lookup(&w.wiki, None, "Home"));
    assert!(matches!(l.expect("lookup"), Lookup::Found(_)));
    assert_eq!(c.read, 0);
}

#[test]
fn a_presentation_that_writes_other_hrefs_is_another_cache_entry_never_the_old_html() {
    let w = World::pages(&[("Home", &[], "[[Guide]]\n"), ("Guide", &[], "x\n")]);
    let under_w = read_page(&w, "Home");
    assert!(under_w.html.as_str().contains("href=\"/w/Guide\""));
    // The wiki is renamed: its pages are written under another base, from the same tip.
    let renamed = Env {
        lang: "en",
        base: "/renamed",
    };
    let (out, c) = w.counted(|| w.service.page(&w.wiki, None, "Home", &renamed));
    let view = found(out);
    assert!(!view.cached);
    assert_eq!(c.read, 1);
    assert!(
        view.html.as_str().contains("href=\"/renamed/Guide\""),
        "{}",
        view.html
    );
    assert!(!view.html.as_str().contains("/w/Guide"));
    // Both are cached side by side.
    assert!(read_page(&w, "Home").html.as_str().contains("/w/Guide"));
}

// ------------------------------------------------------------------------------------ views and tips

#[test]
fn another_view_has_its_own_tip_and_an_unknown_view_is_not_found() {
    let w = World::pages(&[("Home", &[], "x\n")]);
    w.fake().add_view("heads/draft");
    let main = w.service.tip(&w.wiki, None).expect("tip");
    assert_eq!(main.view, "heads/main");
    let draft = w.service.tip(&w.wiki, Some("heads/draft")).expect("tip");
    assert_eq!(draft.view, "heads/draft");
    assert_ne!(main.state_id, draft.state_id);
    // One `branch list` answered both, and a view it did not list is known not to exist while held.
    assert_eq!(w.calls().views, 1);
    assert!(matches!(
        w.service.tip(&w.wiki, Some("heads/nope")),
        Err(ReadError::NoSuchView(v)) if v == "heads/nope"
    ));
    assert_eq!(w.calls().views, 1, "no call to learn that");
    // The tip is held for the hold, and asked for again after it.
    let short = World::pages_with(
        &[("Home", &[], "x\n")],
        ReadConfig {
            tip_ttl: Duration::from_millis(30),
            ..ReadConfig::default()
        },
    );
    short.service.tip(&short.wiki, None).expect("tip");
    short.service.tip(&short.wiki, None).expect("tip");
    assert_eq!(short.calls().views, 1);
    std::thread::sleep(Duration::from_millis(100));
    short.service.tip(&short.wiki, None).expect("tip");
    assert_eq!(short.calls().views, 2);
}

#[test]
fn the_cache_holds_the_present_and_the_past_is_dropped_when_a_tip_moves() {
    let w = World::pages(&[("Home", &[], "v1\n"), ("Other", &[], "o\n")]);
    read_page(&w, "Home");
    read_page(&w, "Other");
    let old_tip = w.fake().state_id();
    assert!(
        inklkunn_store::DerivedStore::title_index(&*w.store, &w.wiki.id, &old_tip)
            .unwrap()
            .is_some()
    );

    w.fake()
        .set("pages/Home.md", &page(&["title: Home"], "v2\n"));
    w.service.forget_wiki(&w.wiki);
    let view = read_page(&w, "Home");
    assert!(view.html.as_str().contains("v2"));
    let new_tip = w.fake().state_id();
    assert_ne!(old_tip, new_tip);

    // The index of the tip the wiki left, and the pages cached at it, are gone; the new tip's are there.
    let derived: &dyn DerivedStore = &*w.store;
    assert_eq!(derived.title_index(&w.wiki.id, &old_tip).unwrap(), None);
    assert!(derived.title_index(&w.wiki.id, &new_tip).unwrap().is_some());
    let at = |tip: &str, path: &'static str| {
        derived
            .page(&PageKey {
                wiki: &w.wiki.id,
                tip,
                path,
                lang: "en",
                presentation: &cache_presentation(&Env::EN),
            })
            .unwrap()
    };
    assert!(
        at(&old_tip, "pages/Other.md").is_none(),
        "cached at the tip the wiki left"
    );
    assert!(at(&old_tip, "pages/Home.md").is_none());
    assert!(at(&new_tip, "pages/Home.md").is_some());
}

// ------------------------------------------------------------------------------------ the index store

#[test]
fn a_restart_finds_the_index_in_the_derived_store_and_does_not_extract_again() {
    let w = World::pages(&[("Home", &[], "x\n"), ("Guide", &[], "y\n")]);
    read_page(&w, "Home");
    assert_eq!(w.calls().materialize, 1);
    let restarted = World::service_over(&w.counting, &w.store, &w.layout, ReadConfig::default());
    let (view, c) = w.counted(|| found(restarted.page(&w.wiki, None, "Guide", &Env::EN)));
    assert!(view.html.as_str().contains('y'));
    assert_eq!(c.materialize, 0, "the index came from the store");
    assert_eq!((c.views, c.read), (1, 1));
}

#[test]
fn the_scratch_directory_is_empty_after_a_build_and_leftovers_are_swept() {
    let w = World::pages(&[("Home", &[], "x\n")]);
    let scratch = w.layout.scratch_dir(&w.wiki.id);
    // A build that died left a directory behind.
    std::fs::create_dir_all(scratch.join("index-1-0/pages")).expect("leftover");
    std::fs::write(scratch.join("index-1-0/pages/Junk.md"), "x").expect("junk");
    std::fs::write(scratch.join("keep-me"), "not ours").expect("other file");
    read_page(&w, "Home");
    let left: Vec<String> = std::fs::read_dir(&scratch)
        .expect("scratch")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        left,
        ["keep-me"],
        "the build removed its own tree and swept the dead one's"
    );
    // And when the build fails, nothing is left either.
    let failing = World::pages(&[("Home", &[], "x\n")]);
    failing
        .fake()
        .fail_materialize(Some(StorageError::Unavailable {
            verb: "extract",
            reason: Unavailable::TimedOut { seconds: 120 },
        }));
    assert!(
        failing
            .service
            .page(&failing.wiki, None, "Home", &Env::EN)
            .is_err()
    );
    let scratch = failing.layout.scratch_dir(&failing.wiki.id);
    assert_eq!(std::fs::read_dir(&scratch).expect("scratch").count(), 0);
}
