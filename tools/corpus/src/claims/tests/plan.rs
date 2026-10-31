use std::path::Path;

use serde_json::json;

use crate::claims::plan::plan;
use crate::claims::{
    Article, default_cache, parse_articles, parse_revisions, refuse_in_repository, slug,
};

use super::support::{Scripted, Tmp, param};

fn article(lang: &str, title: &str) -> Article {
    Article {
        lang: lang.to_owned(),
        title: title.to_owned(),
        reason: "invented".to_owned(),
    }
}

/// A fake API for one article with revisions `50, 40, 30, 20, 10` (newest first), two to a page.
fn fake_api(titles_asked: std::rc::Rc<std::cell::RefCell<Vec<String>>>) -> Scripted {
    let all: Vec<(u64, u64)> = vec![(50, 40), (40, 30), (30, 20), (20, 10), (10, 0)];
    Scripted::new(move |url| {
        titles_asked
            .borrow_mut()
            .push(param(url, "titles").unwrap());
        let start: usize = param(url, "rvcontinue").map_or(0, |s| s.parse().unwrap());
        let limit: usize = param(url, "rvlimit").unwrap().parse().unwrap();
        let take = limit.min(2);
        let page: Vec<_> = all
            .iter()
            .skip(start)
            .take(take)
            .map(|(id, p)| json!({"revid": id, "parentid": p}))
            .collect();
        let mut answer = json!({"query": {"pages": [{"title": "Old bridge", "revisions": page}]}});
        if start + take < all.len() {
            answer.as_object_mut().unwrap().insert(
                "continue".to_owned(),
                json!({"rvcontinue": (start + take).to_string(), "continue": "||"}),
            );
        }
        Ok(answer.to_string())
    })
}

#[test]
fn a_plan_pins_the_newest_n_revisions_oldest_first_following_continue() {
    let cache = Tmp::new("plan");
    let asked = Default::default();
    let mut f = fake_api(std::rc::Rc::clone(&asked));
    let text = plan(&[article("en", "Old bridge")], 3, cache.path(), &mut f).unwrap();
    assert_eq!(
        text,
        "en\tOld bridge\t30\t20\nen\tOld bridge\t40\t30\nen\tOld bridge\t50\t40\n"
    );
    assert_eq!(f.urls.len(), 2, "the first page held two of the three");
    assert_eq!(
        param(f.urls.get(1).unwrap(), "rvlimit").as_deref(),
        Some("1")
    );
    assert!(asked.borrow().iter().all(|t| t == "Old bridge"));
    // The output is exactly what `get` reads.
    let parsed = parse_revisions(&text).unwrap();
    assert_eq!(parsed.first().unwrap().revs.len(), 3);
}

#[test]
fn a_repeated_plan_asks_nothing_and_does_not_drift() {
    let cache = Tmp::new("plan-again");
    let list = [article("en", "Old bridge")];
    let first = plan(&list, 4, cache.path(), &mut fake_api(Default::default())).unwrap();
    let mut silent = Scripted::new(|_| Err("no request expected".to_owned()));
    let again = plan(&list, 4, cache.path(), &mut silent).unwrap();
    assert_eq!(first, again);
    assert!(silent.urls.is_empty());
}

#[test]
fn a_window_larger_than_the_history_returns_what_there_is() {
    let cache = Tmp::new("plan-short");
    let text = plan(
        &[article("en", "Old bridge")],
        50,
        cache.path(),
        &mut fake_api(Default::default()),
    )
    .unwrap();
    assert_eq!(text.lines().count(), 5);
}

#[test]
fn a_missing_article_and_a_zero_window_are_errors() {
    let cache = Tmp::new("plan-missing");
    let mut f = Scripted::new(|_| {
        Ok(r#"{"query":{"pages":[{"title":"Nothing","missing":true}]}}"#.to_owned())
    });
    let err = plan(&[article("en", "Nothing")], 3, cache.path(), &mut f).unwrap_err();
    assert!(err.contains("does not exist"), "{err}");
    assert!(plan(&[article("en", "X")], 0, cache.path(), &mut f).is_err());
}

#[test]
fn the_article_list_takes_comments_and_refuses_what_it_cannot_read() {
    let ok = "# the list\n\nen\tOld bridge\tdates were corrected\nja\t古い橋\t開通年が訂正された\n";
    let list = parse_articles(ok).unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list.get(1).unwrap().title, "古い橋");
    assert!(parse_articles("fr\tPont\tx\n").is_err(), "only en and ja");
    assert!(
        parse_articles("en\tOld bridge\n").is_err(),
        "the reason is required"
    );
    assert!(parse_articles("en\t\tbecause\n").is_err());
}

#[test]
fn keys_are_safe_file_names() {
    assert_eq!(slug("Old bridge (1950)/draft"), "Old_bridge_1950_draft");
    assert_eq!(slug("古い橋"), "古い橋");
    assert_eq!(slug(".."), "_");
    assert!(!slug("a/../b").contains('/'));
    assert!(slug(&"x".repeat(500)).chars().count() <= 80);
    let rows = "en\tA b\t1\t0\nen\tA_b\t2\t0\n";
    assert!(parse_revisions(rows).is_err(), "two titles with one key");
}

#[test]
fn the_cache_is_refused_inside_a_repository() {
    // This very repository: the crate's directory is inside a work tree (a clone or a worktree).
    let inside = Path::new(env!("CARGO_MANIFEST_DIR")).join("cache-that-does-not-exist");
    assert!(refuse_in_repository(&inside).is_err());
    let tmp = Tmp::new("repo");
    std::fs::create_dir_all(tmp.path().join("work").join(".git")).unwrap();
    let nested = tmp.path().join("work").join("deep").join("cache");
    let err = refuse_in_repository(&nested).unwrap_err();
    assert!(err.contains("inside a repository"), "{err}");
    // A worktree has a `.git` file instead of a directory.
    std::fs::create_dir_all(tmp.path().join("work").join("deep")).unwrap();
    std::fs::remove_dir_all(tmp.path().join("work").join(".git")).unwrap();
    std::fs::write(
        tmp.path().join("work").join("deep").join(".git"),
        "gitdir: x",
    )
    .unwrap();
    assert!(refuse_in_repository(&nested).is_err());
    let clean = Tmp::new("clean");
    assert!(refuse_in_repository(&clean.path().join("cache")).is_ok());
}

#[test]
fn the_default_cache_follows_the_xdg_rule() {
    let a = default_cache(Some("/xdg"), Some("/home/u")).unwrap();
    assert_eq!(a, Path::new("/xdg/inklkunn-corpus/claims"));
    let b = default_cache(None, Some("/home/u")).unwrap();
    assert_eq!(b, Path::new("/home/u/.cache/inklkunn-corpus/claims"));
    let c = default_cache(Some("relative"), Some("/home/u")).unwrap();
    assert_eq!(c, b, "a relative XDG_CACHE_HOME is ignored");
    assert!(default_cache(None, None).is_err());
}
