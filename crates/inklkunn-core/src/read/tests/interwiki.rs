//! The interwiki map in the read path (RFC 007 D-8; handoff I-T4; correction W-4): a link whose prefix the wiki's
//! `governance/interwiki.yaml` has is an external link with the template's `{title}` filled percent-encoded, a prefix the map lacks is
//! dangling as it was, **the map is the wiki's and is read from the default view's tip** (an alternative view and a permalink resolve
//! the same link the same way), a state is served from the derived store whether or not the repository has a map file, and a file
//! that does not read is no map for the links and a fault for the rules.

use super::super::testing::{Env, World, block_id, page, state_id};
use super::history::{change, op};
use super::*;
use crate::governance::{GovernanceFile, Refusal};

const MAP: &str = "wp: https://en.wikipedia.org/wiki/{title}\nsearch: \"https://example.org/find?q={title}&lang=en\"\n";

fn world() -> World {
    let w = World::pages(&[(
        "Home",
        &[],
        "A [[wp:Rust (language)]] link, a [[search:a/b c]] one, an [[nope:Thing]] one and [[Other]].\n",
    )]);
    w.fake().derive_changes();
    w
}

fn put_map(w: &World, text: &str) {
    w.fake()
        .set_files(&[("governance/interwiki.yaml".to_owned(), text.to_owned())]);
    w.service.forget_wiki(&w.wiki);
}

#[test]
fn a_mapped_prefix_is_an_external_link_with_the_title_percent_encoded() {
    let w = world();
    put_map(&w, MAP);
    let view = read_page(&w, "Home");
    let html = view.html.as_str();
    assert!(
        html.contains("href=\"https://en.wikipedia.org/wiki/Rust%20%28language%29\""),
        "{html}"
    );
    assert!(
        html.contains("href=\"https://example.org/find?q=a/b%20c&amp;lang=en\""),
        "{html}"
    );
    // External, as every outside link is: no follow, user-generated.
    assert_eq!(html.matches("wikilink-interwiki").count(), 2, "{html}");
    assert_eq!(html.matches("rel=\"nofollow ugc\"").count(), 2, "{html}");
    assert_eq!(
        view.links.interwiki,
        ["search:a/b c", "wp:Rust (language)"].map(str::to_owned)
    );
}

#[test]
fn a_prefix_the_map_lacks_stays_dangling_as_it_was() {
    let w = world();
    put_map(&w, MAP);
    let view = read_page(&w, "Home");
    let html = view.html.as_str();
    assert!(html.contains("wikilink-missing"), "{html}");
    assert!(
        html.contains("href=\"/w/-/create?title=nope%3AThing\""),
        "an invitation to create the page, as for any title: {html}"
    );
    assert!(
        view.links.dangling.iter().any(|t| t == "nope:Thing"),
        "{:?}",
        view.links
    );
    assert!(!view.links.interwiki.iter().any(|t| t.starts_with("nope")));
}

#[test]
fn a_wiki_with_no_map_resolves_every_prefix_as_before() {
    let w = world();
    let view = read_page(&w, "Home");
    assert!(!view.html.as_str().contains("wikilink-interwiki"));
    assert!(view.links.interwiki.is_empty());
    assert!(
        view.links
            .dangling
            .iter()
            .any(|t| t == "wp:Rust (language)")
    );
    assert!(w.service.interwiki_of(&w.wiki, None).unwrap().is_empty());
    assert!(
        w.service
            .title_index(&w.wiki, None)
            .unwrap()
            .interwiki()
            .is_empty()
    );
}

#[test]
fn a_change_to_the_map_changes_the_links_at_the_next_tip_and_taking_the_file_away_takes_them_away() {
    let w = world();
    put_map(&w, MAP);
    let first = read_page(&w, "Home");
    assert!(first.html.as_str().contains("en.wikipedia.org"));
    put_map(&w, "wp: https://de.wikipedia.org/wiki/{title}\n");
    let second = read_page(&w, "Home");
    assert!(second.html.as_str().contains("de.wikipedia.org"));
    assert!(!second.html.as_str().contains("en.wikipedia.org"));
    w.fake().change_files(&[], &["governance/interwiki.yaml"]);
    w.service.forget_wiki(&w.wiki);
    let third = read_page(&w, "Home");
    assert!(!third.html.as_str().contains("wikilink-interwiki"));
}

#[test]
fn an_alternative_view_forked_before_a_map_change_follows_the_new_map() {
    // The view is forked while the wiki has no map at all, and keeps the files it was forked with.
    let w = world();
    w.fake().add_view("heads/draft");
    let draft = |w: &World| {
        found(
            w.service
                .page(&w.wiki, Some("heads/draft"), "Home", &Env::EN),
        )
    };
    let before = draft(&w);
    assert!(!before.html.as_str().contains("wikilink-interwiki"));
    // The default view gets a map: the draft's own tree has none, and its links are the wiki's.
    put_map(&w, MAP);
    let after = draft(&w);
    assert!(
        after
            .html
            .as_str()
            .contains("href=\"https://en.wikipedia.org/wiki/Rust%20%28language%29\""),
        "{}",
        after.html.as_str()
    );
    assert_eq!(
        after.at.state_id(),
        before.at.state_id(),
        "the view did not move"
    );
    assert!(!after.cached, "a row drawn without the map is not read back");
    // And the map changes again: the draft's index, held for its tip, never holds the map of an earlier default tip.
    put_map(&w, "wp: https://de.wikipedia.org/wiki/{title}\n");
    let later = draft(&w);
    assert!(later.html.as_str().contains("de.wikipedia.org"));
    assert!(!later.html.as_str().contains("en.wikipedia.org"));
    let index = w
        .service
        .title_index(&w.wiki, Some("heads/draft"))
        .expect("the draft's index");
    assert_eq!(
        index.interwiki().get("wp"),
        Some("https://de.wikipedia.org/wiki/{title}")
    );
    // The wiki's map is one: the default view and the draft agree, and `interwiki_of` says it for any view.
    assert_eq!(
        w.service
            .interwiki_of(&w.wiki, Some("heads/draft"))
            .unwrap(),
        w.service.interwiki_of(&w.wiki, None).unwrap()
    );
}

#[test]
fn a_permalink_of_an_old_state_follows_the_current_map() {
    let w = World::pages_with(
        &[("Home", &[], "A [[wp:Rust (language)]] link.\n")],
        ttl_zero(),
    );
    w.fake().say(1, "Create the wiki");
    w.fake()
        .show(1, change(vec![op("create-file", &["pages/Home.md"])]));
    let permalink = |w: &World| match w
        .service
        .page_at(&w.wiki, None, &block_id(1), "Home", &Env::EN)
        .expect("the permalink")
    {
        AtOutcome::Found { view, .. } => view,
        other => panic!("expected the page at the block, got {other:?}"),
    };
    // Before there was a map: the link dangles.
    assert!(!permalink(&w).html.as_str().contains("wikilink-interwiki"));
    // A map arrives in a later block: the old state's page has its link.
    w.fake()
        .set_files(&[("governance/interwiki.yaml".to_owned(), MAP.to_owned())]);
    w.fake().say(2, "A map");
    w.fake().show(
        2,
        change(vec![op("create-file", &["governance/interwiki.yaml"])]),
    );
    let seen = permalink(&w);
    assert!(
        seen.html
            .as_str()
            .contains("href=\"https://en.wikipedia.org/wiki/Rust%20%28language%29\""),
        "{}",
        seen.html.as_str()
    );
    assert_eq!(seen.at.block_id(), block_id(1));
    // The map changes: the held render of the permalink is not read back.
    w.fake().set_files(&[(
        "governance/interwiki.yaml".to_owned(),
        "wp: https://de.wikipedia.org/wiki/{title}\n".to_owned(),
    )]);
    w.fake().say(3, "Another map");
    w.fake().show(
        3,
        change(vec![op("edit-text", &["governance/interwiki.yaml"])]),
    );
    let current = permalink(&w);
    assert!(current.html.as_str().contains("de.wikipedia.org"));
    assert!(!current.html.as_str().contains("en.wikipedia.org"));
    // A page at a base is read with the current map too.
    let at = w
        .service
        .title_index_at_base(&w.wiki, None, &state_id(1), &block_id(1))
        .expect("the index at the first state");
    assert_eq!(
        at.interwiki().get("wp"),
        Some("https://de.wikipedia.org/wiki/{title}")
    );
}

#[test]
fn a_map_change_at_the_default_tip_is_seen_by_the_next_read_of_every_held_index() {
    let w = world();
    w.fake().add_view("heads/draft");
    put_map(&w, MAP);
    // Held: the draft's index, and the graph of the draft.
    let held = w.service.title_index(&w.wiki, Some("heads/draft")).unwrap();
    assert_eq!(held.interwiki().len(), 2);
    let graph = w.service.link_graph(&w.wiki, Some("heads/draft")).unwrap();
    assert_eq!(
        graph.wanted_backlinks("nope:Thing").len(),
        1,
        "a prefix the map lacks is a wanted page"
    );
    assert!(graph.wanted_backlinks("wp:Rust (language)").is_empty());
    put_map(&w, "nope: https://nope.example.org/{title}\n");
    let again = w.service.title_index(&w.wiki, Some("heads/draft")).unwrap();
    assert_eq!(again.interwiki().len(), 1);
    assert_eq!(
        again.interwiki().get("nope"),
        Some("https://nope.example.org/{title}")
    );
    let default = w.service.title_index(&w.wiki, None).unwrap();
    assert_eq!(default.interwiki(), again.interwiki());
    // The graph held for the draft's unmoved tip was drawn with the old map: it is not read back.
    let graph = w.service.link_graph(&w.wiki, Some("heads/draft")).unwrap();
    assert!(graph.wanted_backlinks("nope:Thing").is_empty());
    assert_eq!(graph.wanted_backlinks("wp:Rust (language)").len(), 1);
}

#[test]
fn the_map_is_read_once_per_default_tip_and_a_tip_that_leaves_it_alone_keeps_the_same_map() {
    let w = World::pages_with(&[("Home", &[], "Text.\n")], ttl_zero());
    put_map(&w, MAP);
    let first = w.service.interwiki(&w.wiki).expect("the map");
    let (second, calls) = w.counted(|| w.service.interwiki(&w.wiki));
    let second = second.expect("the map");
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(calls.read, 0, "the file is not read again for the same tip");
    // A page changes and the map does not: a new tip, a new listing, the same map.
    w.fake()
        .set("pages/Home.md", &page(&["title: Home"], "Changed.\n"));
    w.service.forget_wiki(&w.wiki);
    let third = w.service.interwiki(&w.wiki).expect("the map");
    assert!(
        Arc::ptr_eq(&first, &third),
        "the same content is the same map"
    );
}

#[test]
fn a_wiki_with_no_default_view_has_no_map_and_a_read_that_cannot_be_made_is_no_empty_map() {
    let w = world();
    put_map(&w, MAP);
    // A wiki whose default view is not in the repository (nothing written yet): no map, and no failure.
    let mut unwritten = w.wiki.clone();
    unwritten.default_view = "heads/not-yet".to_owned();
    assert!(w.service.interwiki(&unwritten).unwrap().is_empty());
    // The default view is there but its file cannot be read now: the read fails, as every read does, and is not an empty map.
    let restarted = World::service_over(&w.counting, &w.store, &w.layout, ReadConfig::default());
    w.fake()
        .fail_read(Some(inklkunn_prikk::StorageError::Unavailable {
            verb: "cat",
            reason: inklkunn_prikk::Unavailable::Busy,
        }));
    assert!(matches!(
        restarted.interwiki(&w.wiki),
        Err(ReadError::CouldNotBeReadNow(_))
    ));
    // Nothing of the failure was kept: the next ask reads it.
    w.fake().fail_read(None);
    assert_eq!(restarted.interwiki(&w.wiki).unwrap().len(), 2);
}

#[test]
fn an_index_the_derived_store_holds_serves_a_state_that_has_a_map_with_no_extract() {
    // The stored row holds the pages and the map is the wiki's, read beside it: a service that starts needs no extract for a state
    // that has a map file (the rebuild of every such state, which the map once cost, is gone).
    let w = world();
    put_map(&w, MAP);
    let built = w.service.title_index(&w.wiki, None).expect("built");
    assert_eq!(built.interwiki().len(), 2);
    let restarted = World::service_over(&w.counting, &w.store, &w.layout, ReadConfig::default());
    let before = w.counting.inner().materialized();
    let again = restarted
        .title_index(&w.wiki, None)
        .expect("from the store");
    assert_eq!(w.counting.inner().materialized(), before, "no extract");
    assert_eq!(again.interwiki(), built.interwiki());
    let view = found(restarted.page(&w.wiki, None, "Home", &Env::EN));
    assert!(view.html.as_str().contains("en.wikipedia.org"));
    assert_eq!(w.counting.inner().materialized(), before, "still none");
    // A state with none: the stored row is used, with no extract and no map, as before.
    let bare = world();
    bare.service.title_index(&bare.wiki, None).expect("built");
    let restarted = World::service_over(
        &bare.counting,
        &bare.store,
        &bare.layout,
        ReadConfig::default(),
    );
    let before = bare.counting.inner().materialized();
    let index = restarted
        .title_index(&bare.wiki, None)
        .expect("from the store");
    assert_eq!(bare.counting.inner().materialized(), before, "no extract");
    assert!(index.interwiki().is_empty());
}

#[test]
fn a_stale_page_of_a_state_that_has_a_map_still_asks_prikk_nothing() {
    let w = world();
    put_map(&w, MAP);
    let fresh = read_page(&w, "Home");
    w.service.forget_wiki(&w.wiki);
    w.fake()
        .fail_views(Some(inklkunn_prikk::StorageError::Unavailable {
            verb: "branch",
            reason: inklkunn_prikk::Unavailable::Busy,
        }));
    let (stale, calls) = w.counted(|| w.service.stale_page(&w.wiki, None, "Home", &Env::EN));
    let stale = stale.expect("the last tip's page, with its links as they were rendered");
    assert_eq!(stale.html, fresh.html);
    assert_eq!(calls.total(), 0, "{calls:?}");
}

#[test]
fn a_map_that_does_not_read_is_no_map_for_the_links_and_a_fault_for_the_rules() {
    for (text, refusal) in [
        (
            "wp: http://en.wikipedia.org/wiki/{title}\n",
            Refusal::TemplateScheme,
        ),
        (
            "wp: https://en.wikipedia.org/wiki/\n",
            Refusal::TemplatePlaceholder,
        ),
        (
            "wp: &anchor https://en.wikipedia.org/wiki/{title}\n",
            Refusal::WrongShape,
        ),
        (
            "bad_prefix: https://en.wikipedia.org/wiki/{title}\n",
            Refusal::UnknownKey,
        ),
    ] {
        let w = world();
        put_map(&w, text);
        // Reading goes on: the page is shown, its prefixed links dangling.
        let view = read_page(&w, "Home");
        assert!(!view.html.as_str().contains("wikilink-interwiki"), "{text}");
        assert!(view.links.interwiki.is_empty());
        // The wiki refuses writes, and says which file and why.
        let rules = w.service.rules_of(&w.wiki).expect("the rules");
        let fault = rules.fault().expect("a fault");
        assert_eq!(fault.file(), GovernanceFile::Interwiki, "{text}");
        assert_eq!(fault.refusal(), refusal, "{text}");
        assert_eq!(fault.line(), Some(1));
        assert!(
            fault
                .to_string()
                .starts_with("governance/interwiki.yaml, line 1: "),
            "{fault}"
        );
        assert!(rules.block().is_some());
    }
}

#[test]
fn a_directory_where_the_map_belongs_is_a_fault_and_never_the_defaults() {
    let w = world();
    put_map(&w, "x: https://example.org/{title}\n");
    w.fake().set_files(&[(
        "governance/interwiki.yaml/inner.txt".to_owned(),
        "x\n".to_owned(),
    )]);
    w.fake().change_files(&[], &["governance/interwiki.yaml"]);
    w.service.forget_wiki(&w.wiki);
    let rules = w.service.rules_of(&w.wiki).expect("the rules");
    let fault = rules.fault().expect("a fault");
    assert_eq!(fault.file(), GovernanceFile::Interwiki);
    assert_eq!(fault.refusal(), Refusal::NotAFile);
    assert_eq!(fault.found(), ["governance/interwiki.yaml/inner.txt"]);
}

#[test]
fn a_sound_map_is_no_fault_and_moves_the_block_that_set_the_rules() {
    let w = world();
    put_map(&w, MAP);
    let block = w.fake().block_id();
    let rules = w.service.rules_of(&w.wiki).expect("the rules");
    assert!(rules.fault().is_none());
    assert_eq!(rules.block(), Some(block.as_str()));
    // The map is not a rule of who may do what: the policy and the constitution are the defaults.
    assert_eq!(*rules.policy(), crate::governance::Policy::default());
    // And a later change of the map is a later block.
    put_map(&w, "wp: https://de.wikipedia.org/wiki/{title}\n");
    let newer = w.service.rules_of(&w.wiki).expect("the rules");
    assert_eq!(newer.block(), Some(w.fake().block_id().as_str()));
    assert_ne!(newer.block(), Some(block.as_str()));
}

#[test]
fn a_map_larger_than_a_file_may_be_is_no_map_for_the_links_and_a_fault_for_the_rules() {
    let w = world();
    let big = format!("# {}\n", "x".repeat(crate::governance::MAX_INTERWIKI_BYTES));
    put_map(&w, &big);
    let view = read_page(&w, "Home");
    assert!(!view.html.as_str().contains("wikilink-interwiki"));
    let rules = w.service.rules_of(&w.wiki).expect("the rules");
    assert_eq!(rules.fault().map(|f| f.refusal()), Some(Refusal::TooLong));
}
