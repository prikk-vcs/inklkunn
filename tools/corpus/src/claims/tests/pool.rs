use std::collections::BTreeMap;

use crate::claims::events::{Event, Kind};
use crate::claims::pool::{draw, render_item, shares, write_packets};
use crate::claims::{converted_path, slug, write_file};

use super::support::{Tmp, event};

/// `n` eligible events of an article (kinds alternate between changed and replaced), plus noise that is not
/// eligible (added and removed).
fn article_events(lang: &str, article: &str, n: u64) -> Vec<Event> {
    let mut out = Vec::new();
    for i in 0..n {
        let kind = if i % 3 == 2 {
            Kind::Replaced
        } else {
            Kind::Changed
        };
        out.push(event(
            kind,
            lang,
            article,
            10 + i,
            (Some(0), Some(0)),
            Some("0-0"),
        ));
    }
    out.push(event(
        Kind::Added,
        lang,
        article,
        900,
        (None, Some(1)),
        None,
    ));
    out.push(event(
        Kind::Removed,
        lang,
        article,
        901,
        (Some(1), None),
        Some("0-1"),
    ));
    out
}

fn corpus() -> Vec<Event> {
    let mut e = Vec::new();
    for (lang, article, n) in [
        ("en", "A", 10),
        ("en", "B", 3),
        ("en", "C", 10),
        ("ja", "D", 10),
        ("ja", "E", 10),
    ] {
        e.extend(article_events(lang, article, n));
    }
    e
}

fn count_by(
    pool: &[crate::claims::pool::Drawn],
    f: impl Fn(&crate::claims::pool::PoolItem) -> String,
) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for d in pool {
        *m.entry(f(&d.item)).or_default() += 1;
    }
    m
}

#[test]
fn shares_are_equal_and_a_short_article_gives_its_surplus_to_the_others() {
    assert_eq!(shares(&[10, 10, 10], 9), [3, 3, 3]);
    assert_eq!(
        shares(&[10, 3, 10], 10),
        [4, 3, 3],
        "the remainder goes to the first that can take it"
    );
    assert_eq!(shares(&[1, 1, 10], 9), [1, 1, 7]);
    assert_eq!(shares(&[2, 2], 10), [2, 2], "not enough events: everything");
    assert_eq!(shares(&[5, 5], 0), [0, 0]);
}

#[test]
fn the_pool_is_balanced_between_languages_with_equal_shares_per_article() {
    let pool = draw(&corpus(), 7, 20, true).unwrap();
    assert_eq!(pool.len(), 20);
    let langs = count_by(&pool, |p| p.lang.clone());
    assert_eq!((langs.get("en"), langs.get("ja")), (Some(&10), Some(&10)));
    let by_article = count_by(&pool, |p| p.article.clone());
    // English: article B has only 3 eligible events, so A and C share the rest as 4 + 3.
    assert_eq!(by_article.get("B"), Some(&3));
    assert_eq!(by_article.get("A"), Some(&4));
    assert_eq!(by_article.get("C"), Some(&3));
    assert_eq!(
        (by_article.get("D"), by_article.get("E")),
        (Some(&5), Some(&5))
    );
}

#[test]
fn an_odd_size_gives_the_extra_item_to_english() {
    let pool = draw(&corpus(), 1, 21, true).unwrap();
    let langs = count_by(&pool, |p| p.lang.clone());
    assert_eq!((langs.get("en"), langs.get("ja")), (Some(&11), Some(&10)));
}

#[test]
fn only_events_with_an_old_and_a_new_text_are_drawn() {
    let pool = draw(&corpus(), 3, 30, true).unwrap();
    assert!(
        pool.iter()
            .all(|d| matches!(d.item.kind, Kind::Changed | Kind::Replaced))
    );
    assert!(pool.iter().all(|d| d.item.rev < 900));
}

#[test]
fn the_draw_is_deterministic_for_a_seed_and_numbers_items_from_one() {
    let a = draw(&corpus(), 42, 20, true).unwrap();
    let b = draw(&corpus(), 42, 20, true).unwrap();
    assert_eq!(a, b);
    let numbers: Vec<u64> = a.iter().map(|d| d.item.item).collect();
    assert_eq!(numbers, (1..=20).collect::<Vec<_>>());
    let c = draw(&corpus(), 43, 20, true).unwrap();
    assert_ne!(a, c, "another seed draws another pool");
}

#[test]
fn the_order_labellers_see_is_not_by_article_unless_asked() {
    let shuffled = draw(&corpus(), 5, 20, true).unwrap();
    let grouped = draw(&corpus(), 5, 20, false).unwrap();
    let switches = |p: &[crate::claims::pool::Drawn]| {
        p.windows(2)
            .filter(|w| w.first().map(|d| &d.item.article) != w.get(1).map(|d| &d.item.article))
            .count()
    };
    assert_eq!(switches(&grouped), 4, "five articles in a row each");
    assert!(
        switches(&shuffled) > 8,
        "interleaved: {}",
        switches(&shuffled)
    );
}

#[test]
fn a_language_with_too_few_events_is_an_error_not_a_smaller_pool() {
    let err = draw(&corpus(), 1, 100, true).unwrap_err();
    assert!(err.contains("fewer than"), "{err}");
}

#[test]
fn a_packet_holds_the_two_texts_and_nothing_else() {
    let cache = Tmp::new("packets");
    let out = Tmp::new("packets-out");
    let title = "Old bridge";
    let page = |body: &str| format!("---\ntitle: {title}\n---\n{body}\n");
    let put = |rev: u64, body: &str| {
        write_file(
            &converted_path(cache.path(), "en", &slug(title), rev),
            page(body).as_bytes(),
        )
        .unwrap();
    };
    put(1, "The bridge opened in 1950.\n\nThe tower is tall.");
    put(2, "The bridge was opened in 1950.\n\nThe tower is tall.");
    put(
        3,
        "The bridge was opened in 1950.\n\nThe tower is very tall.",
    );
    let events = [
        event(
            Kind::Changed,
            "en",
            title,
            2,
            (Some(0), Some(0)),
            Some("0-0"),
        ),
        event(
            Kind::Changed,
            "en",
            title,
            3,
            (Some(1), Some(1)),
            Some("0-1"),
        ),
    ];
    let mut pool = Vec::new();
    for (n, e) in events.iter().enumerate() {
        pool.push(crate::claims::pool::Drawn {
            item: crate::claims::pool::PoolItem {
                item: n as u64 + 1,
                lang: e.lang.clone(),
                article: e.article.clone(),
                rev: e.rev,
                new_block: e.new_block,
                prev_block: e.prev_block,
                kind: e.kind,
            },
            parent_rev: e.parent_rev,
        });
    }
    write_packets(&pool, cache.path(), out.path(), 1).unwrap();
    let first = std::fs::read_to_string(out.path().join("packet-001.md")).unwrap();
    let second = std::fs::read_to_string(out.path().join("packet-002.md")).unwrap();
    assert!(first.contains("## Item 1") && first.contains("OLD:") && first.contains("NEW:"));
    assert!(
        first.contains("The bridge opened in 1950.")
            && first.contains("The bridge was opened in 1950.")
    );
    assert!(second.contains("## Item 2") && second.contains("The tower is very tall."));
    for packet in [&first, &second] {
        assert!(!packet.contains(title), "the article's name is not shown");
        for word in [
            "survived", "changed", "replaced", "chars", "rev", "kind", "block",
        ] {
            assert!(!packet.contains(word), "`{word}` in a packet");
        }
    }
    assert!(!out.path().join("packet-003.md").exists());
}

#[test]
fn a_text_with_backticks_gets_a_longer_fence() {
    let item = render_item(9, "plain", "has ``` inside");
    assert!(item.contains("````\nhas ``` inside\n````"));
    assert!(item.contains("```\nplain\n```"));
}

#[test]
fn a_snapshot_is_read_as_the_source_it_stands_for_and_nothing_else_changes() {
    use crate::claims::text::readable;
    let snap = |name: &str, src: &str| {
        format!(
            "<!-- wiki:snapshot kind=\"template\" name=\"{name}\" engine=\"mediawiki\" src=\"{src}\" -->*[template {name} not expanded]*<!-- /wiki:snapshot -->"
        )
    };
    let text = format!(
        "The tower sways by up to {} in the wind.{}",
        snap("convert", "{{convert|9|cm|in|abbr=on}}"),
        snap("cite web", "{{cite web|url=http://example.org/a |title=A}}")
    );
    assert_eq!(
        readable(&text),
        "The tower sways by up to {{convert|9|cm|in|abbr=on}} in the wind.{{cite web|url=http://example.org/a |title=A}}"
    );
    // Plain text, Japanese text and an unclosed snapshot are left exactly as they are.
    assert_eq!(readable("橋は1950年に開通した。"), "橋は1950年に開通した。");
    let open = "before <!-- wiki:snapshot kind=\"x\" src=\"{{a}}\" --> never closed";
    assert_eq!(readable(open), open);
    // A snapshot with no `src` stays whole (nothing to show instead).
    let bare = "<!-- wiki:snapshot kind=\"x\" -->*[x]*<!-- /wiki:snapshot -->";
    assert_eq!(readable(bare), "<!-- wiki:snapshot kind=\"x\" -->*[x]*");
}

#[test]
fn a_block_over_the_cap_as_a_labeller_reads_it_is_left_out_of_the_draw() {
    use crate::claims::pool::within_length;
    let cache = Tmp::new("cap");
    let title = "Old bridge";
    let long = format!("{} {}", "word".repeat(30), "end");
    let snapshot = format!(
        "<!-- wiki:snapshot kind=\"template\" name=\"t\" engine=\"mediawiki\" src=\"{{{{t}}}}\" -->*[{}]*<!-- /wiki:snapshot -->",
        "padding ".repeat(100)
    );
    let put = |rev: u64, body: &str| {
        write_file(
            &converted_path(cache.path(), "en", &slug(title), rev),
            format!("---\ntitle: {title}\n---\n{body}\n").as_bytes(),
        )
        .unwrap();
    };
    put(1, &format!("Short one.\n\n{long}\n\n{snapshot}"));
    put(2, &format!("Short two.\n\n{long} changed\n\n{snapshot} x"));
    let ev = |prev, new| {
        event(
            Kind::Changed,
            "en",
            title,
            2,
            (Some(prev), Some(new)),
            Some("0-0"),
        )
    };
    let kept = within_length(vec![ev(0, 0), ev(1, 1), ev(2, 2)], cache.path(), 150).unwrap();
    // The short pair is kept; the 120-character pair is kept; the snapshot block reads as `{{t}}` (5 characters
    // and its padding is the comment's, not the labeller's), so it is kept as well.
    assert_eq!(kept.len(), 3);
    let kept = within_length(vec![ev(0, 0), ev(1, 1), ev(2, 2)], cache.path(), 20).unwrap();
    assert_eq!(kept.len(), 2, "the long prose pair is out: {kept:?}");
    assert!(kept.iter().all(|e| e.prev_block != Some(1)));
}
