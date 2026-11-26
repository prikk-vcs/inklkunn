//! The interwiki map in the read path (RFC 007 D-8; handoff I-T4): a link whose prefix the state's `governance/interwiki.yaml` has
//! is an external link with the template's `{title}` filled percent-encoded, a prefix the map lacks is dangling as it was, the map is
//! of the state (a page at a base is read with the links it had), it survives the derived store, and a file that does not read is
//! no map for the links and a fault for the rules.

use super::super::testing::{Env, World};
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
fn the_map_is_of_the_state_so_a_change_to_it_changes_the_links_at_the_next_tip() {
    let w = world();
    put_map(&w, MAP);
    let first = read_page(&w, "Home");
    assert!(first.html.as_str().contains("en.wikipedia.org"));
    let first_block = w.fake().block_id();
    put_map(&w, "wp: https://de.wikipedia.org/wiki/{title}\n");
    let second = read_page(&w, "Home");
    assert!(second.html.as_str().contains("de.wikipedia.org"));
    assert!(!second.html.as_str().contains("en.wikipedia.org"));
    // The map the state at the first block had is the one the index at that base has.
    let at = w
        .service
        .title_index_at_base(
            &w.wiki,
            None,
            &crate::read::testing::state_id(2),
            &first_block,
        )
        .expect("the index at the first state");
    assert_eq!(
        at.interwiki().get("wp"),
        Some("https://en.wikipedia.org/wiki/{title}")
    );
    // Taking the file away takes the links away.
    w.fake().change_files(&[], &["governance/interwiki.yaml"]);
    w.service.forget_wiki(&w.wiki);
    let third = read_page(&w, "Home");
    assert!(!third.html.as_str().contains("wikilink-interwiki"));
}

#[test]
fn an_index_the_derived_store_holds_stands_in_only_for_a_state_that_has_no_map() {
    // A state with a map: the stored row holds the pages and not the map, so a service that starts builds it again (one extract), and
    // has the map.
    let w = world();
    put_map(&w, MAP);
    let built = w.service.title_index(&w.wiki, None).expect("built");
    assert_eq!(built.interwiki().len(), 2);
    let restarted = World::service_over(&w.counting, &w.store, &w.layout, ReadConfig::default());
    let before = w.counting.inner().materialized();
    let again = restarted.title_index(&w.wiki, None).expect("built again");
    assert_eq!(w.counting.inner().materialized(), before + 1, "one extract");
    assert_eq!(again.interwiki(), built.interwiki());
    let view = found(restarted.page(&w.wiki, None, "Home", &Env::EN));
    assert!(view.html.as_str().contains("en.wikipedia.org"));
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
