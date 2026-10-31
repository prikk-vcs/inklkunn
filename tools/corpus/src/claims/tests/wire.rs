use std::time::Duration;

use crate::claims::wire::{
    Fetcher, Limited, MAX_BATCH, RateLimiter, USER_AGENT, api_url, fetch_batch, get_params,
    parse_batch, parse_plan_page, pct_encode, plan_params,
};

use super::support::{FakeClock, Scripted, param};

/// Every token of a URL's query: names and values, split at `&`, `=` and `|`.
fn tokens(url: &str) -> Vec<String> {
    let query = url.split_once('?').unwrap().1;
    query.split(['&', '=', '|']).map(str::to_owned).collect()
}

const FORBIDDEN: [&str; 7] = [
    "user",
    "userid",
    "comment",
    "parsedcomment",
    "tags",
    "ip",
    "timestamp",
];

#[test]
fn requests_ask_for_no_editor_identity_comment_tag_or_time() {
    let cont = vec![("rvcontinue".to_owned(), "20010101|1".to_owned())];
    let plan = api_url("en", &plan_params("Old bridge", 30, &cont)).unwrap();
    let get = api_url("ja", &get_params(&[11, 12, 13], &[])).unwrap();
    for url in [&plan, &get] {
        let toks = tokens(url);
        for bad in FORBIDDEN {
            assert!(!toks.iter().any(|t| t == bad), "{bad} appears in {url}");
        }
    }
    assert_eq!(param(&plan, "rvprop").as_deref(), Some("ids"));
    assert_eq!(param(&get, "rvprop").as_deref(), Some("ids|content"));
    assert_eq!(param(&get, "rvslots").as_deref(), Some("main"));
    assert_eq!(param(&plan, "rvdir").as_deref(), Some("older"));
    assert_eq!(param(&plan, "rvlimit").as_deref(), Some("30"));
    assert_eq!(param(&plan, "rvcontinue").as_deref(), Some("20010101|1"));
    assert_eq!(param(&get, "revids").as_deref(), Some("11|12|13"));
}

#[test]
fn the_request_builder_encodes_titles_and_refuses_other_hosts() {
    let url = api_url("ja", &plan_params("橋 & 塔", 5, &[])).unwrap();
    assert!(url.starts_with("https://ja.wikipedia.org/w/api.php?"));
    assert!(!url.contains(' ') && !url.contains('橋'));
    assert_eq!(param(&url, "titles").as_deref(), Some("橋 & 塔"));
    assert_eq!(pct_encode("a b|c"), "a%20b%7Cc");
    assert!(api_url("evil.example/x#", &[]).is_err());
}

#[test]
fn a_fetch_names_at_most_fifty_ids() {
    let ids: Vec<u64> = (1..=80).collect();
    let url = api_url("en", &get_params(&ids, &[])).unwrap();
    assert_eq!(param(&url, "revids").unwrap().split('|').count(), MAX_BATCH);
}

#[test]
fn the_user_agent_names_the_project_and_a_contact() {
    assert!(USER_AGENT.starts_with("inklkunn-corpus/0.1"));
    assert!(USER_AGENT.contains("RFC 005") && USER_AGENT.contains("contact:"));
}

#[test]
fn the_rate_limiter_lets_one_request_through_per_gap() {
    let mut clock = FakeClock::default();
    let mut limiter = RateLimiter::new(Duration::from_secs(1));
    limiter.wait(&mut clock);
    assert!(
        clock.slept.borrow().is_empty(),
        "the first request waits for nothing"
    );
    limiter.wait(&mut clock);
    limiter.wait(&mut clock);
    assert_eq!(
        *clock.slept.borrow(),
        [Duration::from_secs(1), Duration::from_secs(1)]
    );
    clock.now += Duration::from_secs(5);
    limiter.wait(&mut clock);
    assert_eq!(
        clock.slept.borrow().len(),
        2,
        "after a long pause there is nothing to wait for"
    );
    clock.now += Duration::from_millis(400);
    limiter.wait(&mut clock);
    assert_eq!(
        clock.slept.borrow().last(),
        Some(&Duration::from_millis(600))
    );
}

#[test]
fn a_limited_fetcher_waits_before_every_request() {
    let clock = FakeClock::default();
    let slept = clock.slept.clone();
    let inner = Scripted::new(|_| Ok("{}".to_owned()));
    let mut f = Limited::new(inner, clock, Duration::from_secs(1));
    for _ in 0..4 {
        f.fetch("https://en.wikipedia.org/w/api.php?x=1").unwrap();
    }
    // Four requests at one instant: three gaps of a second.
    assert_eq!(slept.borrow().len(), 3);
}

#[test]
fn a_plan_answer_is_read_in_formatversion_2() {
    let json = r#"{"continue":{"rvcontinue":"2020|9","continue":"||"},"query":{"pages":[{"pageid":1,"title":"T",
        "revisions":[{"revid":30,"parentid":20},{"revid":20,"parentid":10}]}]}}"#;
    let page = parse_plan_page(json).unwrap();
    assert_eq!(page.revisions, [(30, 20), (20, 10)]);
    assert!(!page.missing);
    let next = page.next.unwrap();
    assert!(next.contains(&("rvcontinue".to_owned(), "2020|9".to_owned())));
    assert!(next.contains(&("continue".to_owned(), "||".to_owned())));
    let missing = parse_plan_page(r#"{"query":{"pages":[{"title":"T","missing":true}]}}"#).unwrap();
    assert!(missing.missing && missing.revisions.is_empty());
    assert!(parse_plan_page(r#"{"error":{"code":"maxlag","info":"x"}}"#).is_err());
}

#[test]
fn a_fetch_answer_separates_text_from_deleted_and_hidden_revisions() {
    let json = r#"{"batchcomplete":true,"query":{
        "badrevids":{"7":{"revid":7,"missing":true}},
        "pages":[{"pageid":1,"title":"T","revisions":[
          {"revid":5,"parentid":4,"slots":{"main":{"contentmodel":"wikitext","content":"First text."}}},
          {"revid":6,"parentid":5,"slots":{"main":{"texthidden":true}}}]}]}}"#;
    let b = parse_batch(json).unwrap();
    assert_eq!(b.texts.get(&5).map(String::as_str), Some("First text."));
    assert!(b.deleted.contains(&6) && b.deleted.contains(&7));
    assert!(!b.texts.contains_key(&6));
}

#[test]
fn a_fetch_follows_continue_and_merges_the_pages() {
    let mut n = 0;
    let mut f = Scripted::new(move |url| {
        n += 1;
        Ok(if n == 1 {
            assert!(param(url, "rvcontinue").is_none());
            r#"{"continue":{"rvcontinue":"x|1","continue":"||"},"query":{"pages":[{"revisions":[
               {"revid":1,"slots":{"main":{"content":"one"}}}]}]}}"#
                .to_owned()
        } else {
            assert_eq!(param(url, "rvcontinue").as_deref(), Some("x|1"));
            r#"{"query":{"pages":[{"revisions":[{"revid":2,"slots":{"main":{"content":"two"}}}]}]}}"#
                .to_owned()
        })
    });
    let b = fetch_batch(&mut f, "en", &[1, 2]).unwrap();
    assert_eq!(b.texts.len(), 2);
    assert_eq!(f.urls.len(), 2);
}
