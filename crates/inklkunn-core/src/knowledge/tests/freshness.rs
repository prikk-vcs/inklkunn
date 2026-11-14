//! Freshness (RFC 005 part 2 D-10; K-T9): which sources want a person's look again, with the claims that rely on
//! each, against a day the test fixes. Every date and every sentence is invented.

use std::sync::Arc;
use std::time::Duration;

use super::*;
use crate::knowledge::freshness::{
    DEFAULT_REVIEW_AFTER_DAYS, LastReview, MAX_RELYING, SourceToReview, sources_to_review,
};
use crate::knowledge::index as kix;
use crate::read::testing::World;
use crate::read::{HealthList, HealthRows, ReadConfig, ReadService};

/// 2026-11-15T12:00:00Z, the day the tests are held on.
const NOW: i64 = 1_794_744_000;
/// 2026-11-15 as a day number (the clock above, in whole days).
const TODAY: &str = "2026-11-15";

fn config() -> ReadConfig {
    ReadConfig {
        tip_ttl: Duration::from_secs(3600),
        ..ReadConfig::default()
    }
}

fn source(n: u8, availability: Availability, reviewed: Option<&str>) -> Source {
    let mut s = Source::new(source_id(n));
    s.kind = SourceKind::Secondary;
    s.availability = availability;
    s.last_reviewed = reviewed.map(|day| Reviewed {
        date: day.to_owned(),
        proposal: None,
    });
    s
}

/// A claim said at `p1#b1` that cites each of these sources.
fn claim_citing(n: u8, cited: &[(u8, Link, Bearing)]) -> Claim {
    let mut c = Claim::new(claim_id(n));
    c.kind = StatementKind::Observation;
    c.lifecycle = Lifecycle::Proposed;
    c.expressions = vec![Expression::new("p1", "b1").expect("an expression")];
    c.sources = cited
        .iter()
        .map(|(s, relation, bearing)| SourceLink {
            source: source_id(*s),
            relation: *relation,
            bearing: *bearing,
            note: None,
            judged_on: digest_of_block("Said. ^b1"),
        })
        .collect();
    c
}

fn world(claims: &[Claim], sources: &[Source]) -> World {
    let mut files: Vec<(String, String)> = vec![(
        "pages/One.md".to_owned(),
        "---\ntitle: One\nid: p1\n---\n\nSaid. ^b1\n".to_owned(),
    )];
    files.extend(claims.iter().map(|c| (claim_path(&c.id), c.to_yaml())));
    files.extend(sources.iter().map(|s| (source_path(&s.id), s.to_yaml())));
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    World::with(&refs, config())
}

fn index_of(w: &World) -> Arc<kix::KnowledgeIndex> {
    w.service.knowledge_index(&w.wiki, None).expect("an index")
}

fn ids(rows: &[SourceToReview]) -> Vec<String> {
    rows.iter().map(|r| r.source.id.clone()).collect()
}

fn listed(w: &World, now: i64, days: u32) -> Vec<SourceToReview> {
    sources_to_review(&index_of(w), now, days)
}

/// A service over the same wiki that reads the day from the fixed clock, as a serving process would read its own.
fn clocked(w: &World, days: u32) -> ReadService {
    World::service_over(&w.counting, &w.store, &w.layout, config())
        .with_clock(|| NOW)
        .with_review_after_days(days)
}

#[test]
fn the_default_is_three_years_and_the_day_is_the_clocks() {
    assert_eq!(DEFAULT_REVIEW_AFTER_DAYS, 1096);
    // 2023-11-15 is three years (1096 days, counting the leap day of 2024) before the day of the tests.
    let w = world(
        &[],
        &[source(1, Availability::Available, Some("2023-11-15"))],
    );
    assert!(listed(&w, NOW, DEFAULT_REVIEW_AFTER_DAYS).is_empty());
    // One day later on the clock, the same file is older than the setting.
    assert_eq!(
        ids(&listed(&w, NOW + 86_400, DEFAULT_REVIEW_AFTER_DAYS)),
        [source_id(1).as_str()]
    );
}

#[test]
fn a_review_exactly_as_old_as_the_setting_is_not_listed_and_one_day_older_is() {
    let day = |days: i64| {
        // The day `days` before TODAY, written as the file writes it.
        let at = NOW - days * 86_400;
        let (y, m, d) = civil(at.div_euclid(86_400));
        format!("{y:04}-{m:02}-{d:02}")
    };
    let w = world(
        &[],
        &[
            source(1, Availability::Available, Some(&day(30))),
            source(2, Availability::Available, Some(&day(31))),
            source(3, Availability::Available, Some(&day(29))),
        ],
    );
    // A setting of 30 days: 30 days ago is the limit and is not older; 31 is.
    assert_eq!(ids(&listed(&w, NOW, 30)), [source_id(2).as_str()]);
    // A setting of 1 day.
    assert_eq!(listed(&w, NOW, 1).len(), 3);
    // A setting of 31 days lists none.
    assert!(listed(&w, NOW, 31).is_empty());
}

/// The civil date of a day number (the test's own, so it is not the code's arithmetic read back).
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[test]
fn a_source_marked_unavailable_is_listed_however_recently_it_was_reviewed() {
    let w = world(
        &[],
        &[
            source(1, Availability::Unavailable, Some(TODAY)),
            source(2, Availability::Available, Some(TODAY)),
            source(3, Availability::Unknown, Some(TODAY)),
        ],
    );
    let rows = listed(&w, NOW, DEFAULT_REVIEW_AFTER_DAYS);
    assert_eq!(ids(&rows), [source_id(1).as_str()]);
    let row = rows.first().expect("a row");
    assert!(row.unavailable);
    assert_eq!(
        row.review,
        LastReview::On {
            day: TODAY.to_owned(),
            overdue: false
        }
    );
    assert_eq!(row.reasons(), ["unavailable"]);
}

#[test]
fn a_source_never_reviewed_is_listed_as_never_reviewed() {
    let w = world(&[], &[source(1, Availability::Available, None)]);
    let rows = listed(&w, NOW, DEFAULT_REVIEW_AFTER_DAYS);
    assert_eq!(rows.len(), 1);
    let row = rows.first().expect("a row");
    assert_eq!(row.review, LastReview::Never);
    assert!(!row.unavailable);
    assert_eq!(row.reasons(), ["never-reviewed"]);
}

#[test]
fn an_old_unavailable_source_is_listed_once_with_both_reasons() {
    let w = world(
        &[],
        &[source(1, Availability::Unavailable, Some("2001-01-01"))],
    );
    let rows = listed(&w, NOW, DEFAULT_REVIEW_AFTER_DAYS);
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows.first().expect("a row").reasons(),
        ["unavailable", "overdue"]
    );
}

#[test]
fn a_review_of_today_is_not_listed_until_it_is_older_than_the_setting() {
    let w = world(&[], &[source(1, Availability::Available, Some(TODAY))]);
    // Even at a setting of one day, today's review is not older than it.
    assert!(listed(&w, NOW, 1).is_empty());
    assert!(listed(&w, NOW, DEFAULT_REVIEW_AFTER_DAYS).is_empty());
    // A day later, today's review is one day old: not more than a day.
    assert!(listed(&w, NOW + 86_400, 1).is_empty());
    // Two days later it is older than the setting.
    assert_eq!(
        ids(&listed(&w, NOW + 2 * 86_400, 1)),
        [source_id(1).as_str()]
    );
}

#[test]
fn a_day_that_has_not_come_is_listed_on_its_own_reason_never_hidden() {
    // A person cannot have looked on a day that has not come; a mistyped year would otherwise keep the source out of
    // the list for years (K40).
    let w = world(
        &[],
        &[
            source(1, Availability::Available, Some("2999-01-01")),
            source(2, Availability::Available, Some("2026-11-17")),
            source(3, Availability::Available, Some(TODAY)),
            source(4, Availability::Unavailable, Some("2999-01-01")),
            // The date in UTC+14 is 2026-11-16 at this moment: a reviewer in Kiritimati may have written it (K49).
            source(5, Availability::Available, Some("2026-11-16")),
        ],
    );
    let rows = listed(&w, NOW, DEFAULT_REVIEW_AFTER_DAYS);
    // The day after the date in the earliest timezone is a day that has not come; that date and today are not.
    assert_eq!(
        ids(&rows),
        [
            source_id(1).as_str(),
            source_id(2).as_str(),
            source_id(4).as_str()
        ]
    );
    let first = rows.first().expect("a row");
    assert_eq!(
        first.review,
        LastReview::Ahead {
            day: "2999-01-01".to_owned()
        }
    );
    assert_eq!(first.reasons(), ["review-in-future"]);
    // The reason is told with the other: the source is marked unavailable too.
    assert_eq!(
        rows.get(2).expect("a row").reasons(),
        ["unavailable", "review-in-future"]
    );
    // It does not matter how short the setting is: the day is not "old", it has not come.
    assert_eq!(listed(&w, NOW, 1).len(), 3);
    // When the day has come it is read as any other: tomorrow's review, a day later, is today's.
    let later = listed(&w, NOW + 86_400, DEFAULT_REVIEW_AFTER_DAYS);
    assert_eq!(
        ids(&later),
        [source_id(1).as_str(), source_id(4).as_str()],
        "the day of source 2 has come, and it is not old"
    );
}

#[test]
fn a_day_has_not_come_only_when_it_is_after_the_date_in_utc_plus_14() {
    use crate::knowledge::freshness::day_has_not_come;
    // 2026-11-15T09:59:59Z is 23:59:59 on 2026-11-15 in UTC+14; one second later it is 2026-11-16 there.
    let before = NOW - 7_201;
    let after = NOW - 7_200;
    assert!(!day_has_not_come("2026-11-15", before));
    assert!(day_has_not_come("2026-11-16", before));
    assert!(!day_has_not_come("2026-11-16", after));
    assert!(day_has_not_come("2026-11-17", after));
    // At midnight UTC itself the reviewers of Japan, Australia and Kiritimati are already on the new date.
    let midnight = NOW - 12 * 3_600;
    assert!(!day_has_not_come("2026-11-15", midnight));
    assert!(!day_has_not_come("2026-11-14", midnight));
    // A text that is not a day has not "not come" (it is refused as no day).
    assert!(!day_has_not_come("next week", NOW));
    assert!(!day_has_not_come("", NOW));
    // The service says it by its clock.
    let w = world(&[], &[]);
    let service = clocked(&w, DEFAULT_REVIEW_AFTER_DAYS);
    assert!(!service.day_has_not_come("2026-11-16"));
    assert!(service.day_has_not_come("2026-11-17"));
}

#[test]
fn the_kept_list_follows_the_date_in_utc_plus_14_as_well_as_the_utc_day() {
    // One source reviewed on 2026-11-16: it is a day that has not come until 10:00 UTC on 2026-11-15, then it has.
    let w = world(
        &[],
        &[source(1, Availability::Available, Some("2026-11-16"))],
    );
    let index = index_of(&w);
    let kept = |now: i64| {
        crate::knowledge::freshness::sources_to_review_kept(&index, now, DEFAULT_REVIEW_AFTER_DAYS)
    };
    let before = kept(NOW - 7_201);
    assert_eq!(before.len(), 1);
    // The same UTC day, the next second: the list is another one, and the source is not listed.
    let after = kept(NOW - 7_200);
    assert!(after.is_empty());
    assert!(!Arc::ptr_eq(&before, &after));
}

#[test]
fn each_listed_source_names_the_claims_that_rely_on_it_with_how_they_say_it_bears() {
    let claims = [
        claim_citing(1, &[(1, Link::Supports, Bearing::Direct)]),
        claim_citing(
            2,
            &[
                (1, Link::Challenges, Bearing::Partial),
                (2, Link::Supports, Bearing::Indirect),
            ],
        ),
        claim_citing(3, &[(2, Link::Supports, Bearing::Direct)]),
    ];
    let w = world(
        &claims,
        &[
            source(1, Availability::Available, None),
            source(2, Availability::Available, Some(TODAY)),
        ],
    );
    let rows = listed(&w, NOW, DEFAULT_REVIEW_AFTER_DAYS);
    // Source 2 is fresh and is not listed, though claims cite it.
    assert_eq!(ids(&rows), [source_id(1).as_str()]);
    let row = rows.first().expect("a row");
    assert_eq!(row.claims_total, 2);
    let got: Vec<(&str, &str, &str)> = row
        .claims
        .iter()
        .map(|c| (c.claim.as_str(), c.relation.as_str(), c.bearing.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            (claim_id(1).as_str(), "supports", "direct"),
            (claim_id(2).as_str(), "challenges", "partial"),
        ]
    );
}

#[test]
fn a_listed_source_no_claim_cites_is_listed_with_no_claims() {
    let w = world(&[], &[source(1, Availability::Unavailable, Some(TODAY))]);
    let rows = listed(&w, NOW, DEFAULT_REVIEW_AFTER_DAYS);
    let row = rows.first().expect("a row");
    assert!(row.claims.is_empty());
    assert_eq!(row.claims_total, 0);
}

#[test]
fn the_claims_beside_a_source_are_cut_and_the_whole_count_kept() {
    let n = MAX_RELYING + 3;
    let claims: Vec<Claim> = (1..=n)
        .map(|i| {
            let mut c = claim_citing(1, &[(1, Link::Supports, Bearing::Direct)]);
            c.id = ClaimId::parse(&format!("01930000-0000-7000-8000-{i:012x}")).expect("an id");
            c
        })
        .collect();
    let w = world(&claims, &[source(1, Availability::Unavailable, None)]);
    let rows = listed(&w, NOW, DEFAULT_REVIEW_AFTER_DAYS);
    let row = rows.first().expect("a row");
    assert_eq!(row.claims.len(), MAX_RELYING);
    assert_eq!(row.claims_total, n);
}

#[test]
fn a_review_day_the_index_cannot_read_is_never_reviewed_not_trusted() {
    for bad in [
        "",
        "2026-13-01",
        "2026-00-10",
        "2026-1-5",
        "yesterday",
        "2026-11-1x",
    ] {
        let mut rows = inklkunn_store::KnowledgeRows::default();
        rows.sources.push(inklkunn_store::KnowledgeSourceRow {
            id: source_id(1).as_str().to_owned(),
            kind: "other".to_owned(),
            availability: "available".to_owned(),
            last_reviewed: Some(bad.to_owned()),
            superseded_by: None,
            title: None,
            author: None,
            date: None,
            reference: None,
        });
        let index = kix::KnowledgeIndex::new("tip", rows);
        let listed = sources_to_review(&index, NOW, DEFAULT_REVIEW_AFTER_DAYS);
        assert_eq!(listed.len(), 1, "{bad:?}");
        assert_eq!(listed.first().expect("a row").review, LastReview::Never);
    }
}

// ------------------------------------------------------------------------------------ the cost of the list (K40)

#[test]
fn the_list_is_computed_once_for_a_tip_a_day_and_a_setting() {
    let w = world(
        &[claim_citing(1, &[(1, Link::Supports, Bearing::Direct)])],
        &[source(1, Availability::Unavailable, Some(TODAY))],
    );
    let index = index_of(&w);
    let kept = |now: i64, days: u32| {
        crate::knowledge::freshness::sources_to_review_kept(&index, now, days)
    };
    let first = kept(NOW, DEFAULT_REVIEW_AFTER_DAYS);
    // The same day (any moment of it) and setting: the list that was computed, not another.
    assert!(Arc::ptr_eq(
        &first,
        &kept(NOW + 60, DEFAULT_REVIEW_AFTER_DAYS)
    ));
    assert_eq!(first.len(), 1);
    // Another setting or another day is another list (the answer can differ), and it replaces the kept one.
    let strict = kept(NOW, 1);
    assert!(!Arc::ptr_eq(&first, &strict));
    assert!(Arc::ptr_eq(&strict, &kept(NOW, 1)));
    let tomorrow = kept(NOW + 86_400, 1);
    assert!(!Arc::ptr_eq(&strict, &tomorrow));
    // The answer is the one the plain function gives.
    assert_eq!(
        ids(&first),
        ids(&sources_to_review(&index, NOW, DEFAULT_REVIEW_AFTER_DAYS))
    );
}

#[test]
fn the_service_gives_one_list_to_every_reader_of_a_tip_and_a_day() {
    let w = world(
        &[],
        &[
            source(1, Availability::Unavailable, Some(TODAY)),
            source(2, Availability::Unavailable, Some(TODAY)),
            source(3, Availability::Unavailable, Some(TODAY)),
        ],
    );
    let service = clocked(&w, DEFAULT_REVIEW_AFTER_DAYS);
    let a = service
        .sources_to_review_page(&w.wiki, None, None, 2)
        .expect("a page");
    let b = service
        .sources_to_review_page(&w.wiki, None, a.next.as_ref(), 2)
        .expect("a page");
    // The two pages are of one list of one index, and the second begins where the first ended.
    assert!(Arc::ptr_eq(&a.index, &b.index));
    assert_eq!((a.offset, b.offset), (0, 2));
    assert_eq!((a.rows.len(), b.rows.len(), a.total), (2, 1, 3));
    assert!(b.next.is_none());
    assert!(Arc::ptr_eq(
        &crate::knowledge::freshness::sources_to_review_kept(
            &a.index,
            NOW,
            DEFAULT_REVIEW_AFTER_DAYS
        ),
        &crate::knowledge::freshness::sources_to_review_kept(
            &b.index,
            NOW,
            DEFAULT_REVIEW_AFTER_DAYS
        ),
    ));
}

/// An index with `sources` unreviewed sources, each cited by `per_source` claims (so `sources * per_source` links),
/// and one place for every claim: the size of a new wiki that was filled in from a list.
fn crowded(sources: usize, per_source: usize) -> kix::KnowledgeIndex {
    let mut rows = inklkunn_store::KnowledgeRows::default();
    for s in 0..sources {
        rows.sources.push(inklkunn_store::KnowledgeSourceRow {
            id: format!("01930000-0000-7000-8000-{s:012x}"),
            kind: "other".to_owned(),
            availability: "available".to_owned(),
            last_reviewed: None,
            superseded_by: None,
            title: None,
            author: None,
            date: None,
            reference: None,
        });
    }
    let claims = sources * per_source;
    for c in 0..claims {
        let id = format!("01940000-0000-7000-8000-{c:012x}");
        rows.links.push(inklkunn_store::KnowledgeLinkRow {
            claim: id.clone(),
            source: format!("01930000-0000-7000-8000-{:012x}", c % sources),
            relation: "supports".to_owned(),
            bearing: "direct".to_owned(),
            judgement: "current".to_owned(),
        });
        rows.anchors.push(inklkunn_store::KnowledgeAnchorRow {
            claim: id,
            page_id: "p1".to_owned(),
            block: "b1".to_owned(),
            page_path: Some("pages/One.md".to_owned()),
            state: "present".to_owned(),
            digest: None,
        });
    }
    kix::KnowledgeIndex::new("tip", rows)
}

#[test]
fn the_list_of_two_thousand_sources_costs_what_it_returns_not_sources_times_links() {
    // 2,000 unreviewed sources, 200,000 links, 200,000 places: a pass over every link for each source is 4 * 10^8
    // comparisons; a map built once is 2 * 10^5 steps.
    let index = crowded(2_000, 100);
    let started = std::time::Instant::now();
    let list =
        crate::knowledge::freshness::sources_to_review_kept(&index, NOW, DEFAULT_REVIEW_AFTER_DAYS);
    let took = started.elapsed();
    assert_eq!(list.len(), 2_000);
    assert!(list.iter().all(|r| r.claims_total == 100));
    assert!(list.iter().all(|r| r.claims.len() == MAX_RELYING));
    assert!(
        took < Duration::from_secs(1),
        "the list of 2,000 sources took {took:?}"
    );
    // The lookups answer in what they return, in the rows' order.
    let source = index.sources().first().expect("a source").id.clone();
    let cited: Vec<&str> = index
        .claims_citing(&source)
        .map(|l| l.claim.as_str())
        .collect();
    assert_eq!(cited.len(), 100);
    let mut sorted = cited.clone();
    sorted.sort_unstable();
    assert_eq!(cited, sorted);
    let claim = index.rows().links.first().expect("a link").claim.clone();
    assert_eq!(index.anchors_of(&claim).count(), 1);
    assert_eq!(index.links_of(&claim).count(), 1);
    assert_eq!(index.anchors_of("no such claim").count(), 0);
    // A second ask costs nothing: it is the list that was kept.
    let again = std::time::Instant::now();
    let same =
        crate::knowledge::freshness::sources_to_review_kept(&index, NOW, DEFAULT_REVIEW_AFTER_DAYS);
    assert!(Arc::ptr_eq(&list, &same));
    assert!(again.elapsed() < Duration::from_millis(50));
}

// ------------------------------------------------------------------------------------ the list at the API's seam

fn page_of(
    service: &ReadService,
    w: &World,
    limit: usize,
    after: Option<&crate::read::Cursor>,
) -> crate::read::HealthPage {
    service
        .api_health(&w.wiki, None, HealthList::SourcesToReview, after, limit)
        .expect("a list")
}

#[test]
fn the_list_is_the_eighth_with_its_own_slug_and_reads_the_injected_clock() {
    assert_eq!(HealthList::ALL.len(), 8);
    assert_eq!(HealthList::SourcesToReview.slug(), "sources-to-review");
    assert_eq!(
        HealthList::from_slug("sources-to-review"),
        Some(HealthList::SourcesToReview)
    );
    let w = world(
        &[claim_citing(1, &[(1, Link::Supports, Bearing::Direct)])],
        &[
            source(1, Availability::Available, Some("2001-01-01")),
            source(2, Availability::Available, Some(TODAY)),
            source(3, Availability::Unavailable, Some(TODAY)),
        ],
    );
    let service = clocked(&w, DEFAULT_REVIEW_AFTER_DAYS);
    assert_eq!(service.review_after_days(), DEFAULT_REVIEW_AFTER_DAYS);
    let page = page_of(&service, &w, 50, None);
    assert_eq!(page.total, 2);
    let HealthRows::SourcesToReview(rows) = &page.rows else {
        panic!("rows of the wrong list");
    };
    assert_eq!(
        ids(rows),
        [source_id(1).as_str(), source_id(3).as_str()],
        "by source id"
    );
    // The same list through the service's own method.
    assert_eq!(
        ids(&service.sources_to_review(&w.wiki, None).expect("rows").rows),
        ids(rows)
    );
    // With a setting short enough, the review of today is not old yet, and with the day moved on it is.
    let strict = World::service_over(&w.counting, &w.store, &w.layout, config())
        .with_clock(|| NOW + 3 * 86_400)
        .with_review_after_days(2);
    assert_eq!(
        ids(&strict.sources_to_review(&w.wiki, None).expect("rows").rows).len(),
        3
    );
}

#[test]
fn the_setting_is_kept_between_one_day_and_a_hundred_years() {
    let w = world(&[], &[]);
    let service = |days| clocked(&w, days).review_after_days();
    assert_eq!(service(0), 1);
    assert_eq!(service(7), 7);
    assert_eq!(
        service(u32::MAX),
        crate::knowledge::freshness::MAX_REVIEW_AFTER_DAYS
    );
}

#[test]
fn the_list_is_walked_in_pages_by_a_cursor_after_the_last_source_shown() {
    let sources: Vec<Source> = (1..=5u8)
        .map(|n| source(n, Availability::Unavailable, Some(TODAY)))
        .collect();
    let w = world(&[], &sources);
    let service = clocked(&w, DEFAULT_REVIEW_AFTER_DAYS);
    let first = page_of(&service, &w, 2, None);
    assert_eq!(first.total, 5);
    let second = page_of(&service, &w, 2, first.next.as_ref());
    let third = page_of(&service, &w, 2, second.next.as_ref());
    assert!(third.next.is_none());
    let all: Vec<String> = [&first, &second, &third]
        .into_iter()
        .flat_map(|p| match &p.rows {
            HealthRows::SourcesToReview(r) => ids(r),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(all.len(), 5);
    let mut sorted = all.clone();
    sorted.sort();
    assert_eq!(all, sorted);
    // A cursor of another list is not followed.
    assert!(
        service
            .api_health(&w.wiki, None, HealthList::Wanted, first.next.as_ref(), 2)
            .is_err()
    );
    // The cursor says the id it follows, and nothing of the tip or the day: the list continues after that source
    // on another day, too (K50).
    let cursor = first.next.as_ref().expect("a next page");
    assert_eq!(
        cursor.as_str(),
        format!("s1{}", source_id(2).as_str()),
        "after the last source shown"
    );
    let tomorrow =
        World::service_over(&w.counting, &w.store, &w.layout, config()).with_clock(|| NOW + 86_400);
    let next_day = tomorrow
        .sources_to_review_page(&w.wiki, None, Some(cursor), 2)
        .expect("a page");
    assert_eq!(
        ids(&next_day.rows),
        [source_id(3).as_str(), source_id(4).as_str()]
    );
    assert_eq!(next_day.offset, 2);
}

#[test]
fn an_edit_elsewhere_does_not_restart_the_walk() {
    let five: Vec<Source> = (1..=5u8)
        .map(|n| source(n, Availability::Unavailable, Some(TODAY)))
        .collect();
    let w = world(&[], &five);
    let service = clocked(&w, DEFAULT_REVIEW_AFTER_DAYS);
    let first = service
        .sources_to_review_page(&w.wiki, None, None, 2)
        .expect("a page");
    let cursor = first.next.expect("a next page");
    // Another tip of the wiki: source 1 is gone (it was looked at), source 6 is new.
    let edited: Vec<Source> = (2..=6u8)
        .map(|n| source(n, Availability::Unavailable, Some(TODAY)))
        .collect();
    let other = world(&[], &edited);
    let service = clocked(&other, DEFAULT_REVIEW_AFTER_DAYS);
    let second = service
        .sources_to_review_page(&other.wiki, None, Some(&cursor), 2)
        .expect("a page, not an invalid cursor");
    assert_eq!(
        ids(&second.rows),
        [source_id(3).as_str(), source_id(4).as_str()]
    );
    // The page says where it stands in the list as it is now: source 2 is its first row.
    assert_eq!((second.offset, second.total), (1, 5));
    // The pages before it are in hand, so what a page numbers is numbered as the earlier pages did.
    assert_eq!(second.whole.len(), 5);
    assert_eq!(
        ids(second.whole.get(..second.offset).unwrap_or_default()),
        [source_id(2).as_str()]
    );
}

#[test]
fn a_cursor_past_the_end_is_an_empty_page_not_an_error_and_not_a_wrong_count() {
    let five: Vec<Source> = (1..=5u8)
        .map(|n| source(n, Availability::Unavailable, Some(TODAY)))
        .collect();
    let w = world(&[], &five);
    let service = clocked(&w, DEFAULT_REVIEW_AFTER_DAYS);
    let cursor = service
        .sources_to_review_page(&w.wiki, None, None, 4)
        .expect("a page")
        .next
        .expect("a next page");
    // The wiki now lists three: the reader was shown up to source 4.
    let three: Vec<Source> = (1..=3u8)
        .map(|n| source(n, Availability::Unavailable, Some(TODAY)))
        .collect();
    let other = world(&[], &three);
    let page = clocked(&other, DEFAULT_REVIEW_AFTER_DAYS)
        .sources_to_review_page(&other.wiki, None, Some(&cursor), 4)
        .expect("a page");
    assert!(page.rows.is_empty() && page.next.is_none());
    assert_eq!((page.offset, page.total), (3, 3));
    // And exactly at the last source.
    let at_end =
        crate::read::Cursor::parse(&format!("s1{}", source_id(3).as_str())).expect("a cursor");
    let page = clocked(&other, DEFAULT_REVIEW_AFTER_DAYS)
        .sources_to_review_page(&other.wiki, None, Some(&at_end), 4)
        .expect("a page");
    assert!(page.rows.is_empty() && page.next.is_none());
}

#[test]
fn only_a_cursor_that_names_a_source_id_is_one() {
    use crate::read::Cursor;
    let id = source_id(2);
    assert!(Cursor::parse(&format!("s1{id}")).is_some());
    for bad in [
        "s1",
        "s1 ",
        "s1ABC",
        "s1../x",
        "s1%2e",
        &format!("s1{id}{id}{id}"),
    ] {
        assert!(Cursor::parse(bad).is_none(), "{bad:?}");
    }
    // A cursor of another list is refused by the sources' list.
    let w = world(&[], &[source(1, Availability::Unavailable, Some(TODAY))]);
    let service = clocked(&w, DEFAULT_REVIEW_AFTER_DAYS);
    let foreign = Cursor::parse("n12").expect("a cursor");
    assert!(matches!(
        service.sources_to_review_page(&w.wiki, None, Some(&foreign), 2),
        Err(crate::read::ApiError::InvalidCursor)
    ));
}

#[test]
fn nothing_in_the_freshness_code_can_reach_the_network_or_a_process() {
    // The boundary gates hold the crate's dependencies; this holds the file itself: it reads an index and a day.
    let text = include_str!("../freshness.rs");
    for word in [
        "egress",
        "fetch",
        "TcpStream",
        "reqwest",
        "std::process",
        "SystemTime",
        "now_secs",
    ] {
        assert!(
            !text
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .any(|l| l.contains(word)),
            "freshness.rs mentions {word}"
        );
    }
}
