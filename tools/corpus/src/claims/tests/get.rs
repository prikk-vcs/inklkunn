use std::collections::BTreeSet;

use serde_json::json;

use crate::claims::get::get;
use crate::claims::parse_revisions;
use crate::claims::{deleted_path, raw_path};

use super::support::{Scripted, Tmp, param};

/// `n` pinned revisions `1001, 1002, …` of one article, as a revisions file.
fn pinned(lang: &str, title: &str, n: u64) -> String {
    (1..=n)
        .map(|i| format!("{lang}\t{title}\t{}\t{}\n", 1000 + i, 999 + i))
        .collect()
}

/// A fake API: revision `1010` is reported missing, `1020` has its text hidden; every other revision has text
/// "Sentence of revision N." The URLs it was asked are in the returned fetcher.
fn fake_api() -> Scripted {
    Scripted::new(|url| {
        let ids: Vec<u64> = param(url, "revids")
            .unwrap()
            .split('|')
            .map(|s| s.parse().unwrap())
            .collect();
        assert!(ids.len() <= 50);
        assert_eq!(param(url, "rvprop").as_deref(), Some("ids|content"));
        let mut bad = serde_json::Map::new();
        let mut revisions = Vec::new();
        for id in ids {
            match id {
                1010 => {
                    bad.insert(id.to_string(), json!({"revid": id, "missing": true}));
                }
                1020 => {
                    revisions.push(json!({"revid": id, "slots": {"main": {"texthidden": true}}}))
                }
                _ => revisions.push(json!({
                    "revid": id,
                    "slots": {"main": {"content": format!("Sentence of revision {id}.")}}
                })),
            }
        }
        Ok(json!({"query": {"badrevids": bad, "pages": [{"title": "Old bridge", "revisions": revisions}]}}).to_string())
    })
}

#[test]
fn get_reads_exactly_the_pinned_ids_in_batches_of_fifty_and_counts_the_deleted() {
    let cache = Tmp::new("get");
    let articles = parse_revisions(&pinned("en", "Old bridge", 120)).unwrap();
    let mut f = fake_api();
    let report = get(&articles, cache.path(), &mut f).unwrap();
    assert_eq!(f.urls.len(), 3, "50 + 50 + 20");
    let asked: BTreeSet<u64> = f
        .urls
        .iter()
        .flat_map(|u| {
            param(u, "revids")
                .unwrap()
                .split('|')
                .map(|s| s.parse::<u64>().unwrap())
                .collect::<Vec<_>>()
        })
        .collect();
    let pinned_ids: BTreeSet<u64> = (1001..=1120).collect();
    assert_eq!(
        asked, pinned_ids,
        "no id outside the list is asked for, none is left out"
    );

    let counts = report
        .languages
        .get("en")
        .unwrap()
        .get("Old bridge")
        .unwrap();
    assert_eq!(
        (counts.requested, counts.fetched, counts.deleted_upstream),
        (120, 118, 2)
    );
    let a = articles.first().unwrap();
    assert_eq!(
        std::fs::read_to_string(raw_path(cache.path(), "en", &a.key, 1001)).unwrap(),
        "Sentence of revision 1001."
    );
    assert!(
        !raw_path(cache.path(), "en", &a.key, 1010).exists(),
        "a deleted revision is skipped, never replaced"
    );
    assert!(deleted_path(cache.path(), "en", &a.key, 1010).exists());
    assert!(deleted_path(cache.path(), "en", &a.key, 1020).exists());
}

#[test]
fn a_revision_already_cached_is_not_requested_again() {
    let cache = Tmp::new("get-again");
    let articles = parse_revisions(&pinned("en", "Old bridge", 60)).unwrap();
    let first = get(&articles, cache.path(), &mut fake_api()).unwrap();
    let mut silent = Scripted::new(|_| Err("no request expected".to_owned()));
    let again = get(&articles, cache.path(), &mut silent).unwrap();
    assert!(silent.urls.is_empty());
    assert_eq!(
        first.languages, again.languages,
        "the report is the same, deleted ones included"
    );

    // A longer list asks only for the new ids.
    let more = parse_revisions(&pinned("en", "Old bridge", 70)).unwrap();
    let mut f = fake_api();
    get(&more, cache.path(), &mut f).unwrap();
    assert_eq!(f.urls.len(), 1);
    assert_eq!(
        param(f.urls.first().unwrap(), "revids")
            .unwrap()
            .split('|')
            .count(),
        10
    );
}

#[test]
fn an_id_the_answer_leaves_out_is_counted_deleted_not_replaced() {
    let cache = Tmp::new("get-silent");
    let articles = parse_revisions(&pinned("ja", "古い橋", 3)).unwrap();
    // The API answers for two of the three and says nothing of the other.
    let mut f = Scripted::new(|_| {
        Ok(json!({"query": {"pages": [{"revisions": [
            {"revid": 1001, "slots": {"main": {"content": "一つ目。"}}},
            {"revid": 1003, "slots": {"main": {"content": "三つ目。"}}}]}]}})
        .to_string())
    });
    let report = get(&articles, cache.path(), &mut f).unwrap();
    let c = report.languages.get("ja").unwrap().get("古い橋").unwrap();
    assert_eq!((c.requested, c.fetched, c.deleted_upstream), (3, 2, 1));
    assert!(
        f.urls
            .first()
            .unwrap()
            .starts_with("https://ja.wikipedia.org/")
    );
}
