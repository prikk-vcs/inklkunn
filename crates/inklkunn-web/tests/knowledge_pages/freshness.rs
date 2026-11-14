//! The health list *Sources to review* (RFC 005 part 2 D-10; handoff 005 K-T9), through the real router: which
//! sources are listed and why, the claims that rely on each, the words in both languages, the setting, existence
//! neutrality, and that nothing on it ranks. The clock of these tests is fixed (2023-11-14), so the three-year
//! default puts the limit at 2020-11-13: a source reviewed that day is not yet old, one the day before is. Every
//! title and every sentence is invented.

use axum::http::{StatusCode, header};

use super::common::{
    APP, World, claim, claim_id, get, home_with, link, put, request, source, source_id,
};
use super::{JA, RANKING_WORDS, claim_uri};
use inklkunn_core::knowledge::{
    Availability, Bearing, Identity, Link, Reviewed, SourceKind, Standing,
};
use inklkunn_store::Visibility;

const SAID: &str = "The bridge opened in 1950.";
const LIST: &str = "/handbook/-/health/sources-to-review";

fn reviewed(day: &str) -> Option<Reviewed> {
    Some(Reviewed {
        date: day.to_owned(),
        proposal: None,
    })
}

fn rows(body: &str) -> Vec<String> {
    body.split("<tbody>")
        .nth(1)
        .and_then(|b| b.split("</tbody>").next())
        .unwrap_or("")
        .split("<tr>")
        .skip(1)
        .map(str::to_owned)
        .collect()
}

fn handbook(files: &[(&str, &str)]) -> World {
    World::new("handbook", "The Handbook", files)
}

/// Six sources, each in one of the states the list tells apart, and two claims that rely on some of them.
fn planted() -> World {
    let w = handbook(&[("pages/Home.md", &home_with(SAID))]);
    let mut unavailable = source(1, SourceKind::Secondary);
    unavailable.availability = Availability::Unavailable;
    unavailable.last_reviewed = reviewed("2023-10-01");
    unavailable.identity = Identity {
        title: Some("<b>The Bridge Survey</b>".to_owned()),
        author: Some("The Harbour Authority".to_owned()),
        date: None,
    };
    let mut never = source(2, SourceKind::Primary);
    never.identity.title = Some("The Ferry Timetable".to_owned());
    let mut old = source(3, SourceKind::Other);
    old.last_reviewed = reviewed("2020-11-12");
    old.identity.title = Some("The Tide Tables".to_owned());
    let mut limit = source(4, SourceKind::Other);
    limit.last_reviewed = reviewed("2020-11-13");
    limit.identity.title = Some("Not Yet Old".to_owned());
    let mut fine = source(5, SourceKind::Primary);
    fine.last_reviewed = reviewed("2023-11-01");
    fine.identity.title = Some("Looked At Lately".to_owned());
    let mut unknown = source(6, SourceKind::Primary);
    unknown.availability = Availability::Unknown;
    unknown.last_reviewed = reviewed("2023-11-01");
    unknown.identity.title = Some("Unknown But Fresh".to_owned());
    let mut first = claim(1, Standing::Contested, None, SAID);
    first.sources = vec![
        link(1, Link::Supports, Bearing::Direct, SAID),
        link(3, Link::Challenges, Bearing::Partial, SAID),
        link(4, Link::Supports, Bearing::Direct, SAID),
        link(5, Link::Supports, Bearing::Direct, SAID),
    ];
    let mut second = claim(2, Standing::Unknown, None, SAID);
    second.expressions =
        vec![inklkunn_core::knowledge::Expression::new("page-gone", "b9").expect("an expression")];
    second.sources = vec![link(3, Link::Supports, Bearing::Indirect, SAID)];
    put(
        &w,
        &home_with(SAID),
        &[first, second],
        &[unavailable, never, old, limit, fine, unknown],
    );
    w
}

#[tokio::test]
async fn the_list_names_each_source_a_person_should_look_at_and_why() {
    let w = planted();
    let a = get(&w, LIST).await;
    assert_eq!(a.status, StatusCode::OK);
    let body = a.body();
    let r = rows(&body);
    // Three rows: unavailable, never reviewed, older than the limit. Not the one reviewed on the limit's own day,
    // not the fresh ones, whatever claims rely on them.
    assert_eq!(r.len(), 3, "{body}");
    assert!(!body.contains("Not Yet Old"), "{body}");
    assert!(!body.contains("Looked At Lately"), "{body}");
    assert!(!body.contains("Unknown But Fresh"), "{body}");
    // By source id: the unavailable source first. Its title is a person's and is printed as letters.
    let first = r.first().expect("a row");
    assert!(
        first.contains("&#60;b&#62;The Bridge Survey&#60;/b&#62;"),
        "{body}"
    );
    assert!(!body.contains("<b>The Bridge Survey"), "{body}");
    assert!(first.contains("by The Harbour Authority"), "{body}");
    assert!(
        first.contains("A person who checked marked it as unavailable."),
        "{body}"
    );
    assert!(first.contains("Last reviewed on 2023-10-01."), "{body}");
    assert!(first.contains("1 claim cites it:"), "{body}");
    assert!(first.contains("(supports, directly)"), "{body}");
    assert!(
        first.contains(&format!("href=\"{}\"", claim_uri(1))),
        "{body}"
    );
    let second = r.get(1).expect("a row");
    assert!(second.contains("The Ferry Timetable"), "{body}");
    assert!(
        second.contains("No one has recorded looking at it."),
        "{body}"
    );
    assert!(second.contains("No claim cites it."), "{body}");
    let third = r.get(2).expect("a row");
    assert!(third.contains("The Tide Tables"), "{body}");
    assert!(
        third.contains("Last reviewed on 2020-11-12, more than 1096 days ago."),
        "{body}"
    );
    // Both claims that cite it, each named by the page it is said on or, when no page says it, by its id.
    assert!(third.contains("2 claims cite it:"), "{body}");
    // Each claim is named by its page **and its whole identifier** (K43).
    assert!(
        third.contains(
            "Home [claim <code>01930000-0000-7000-8000-000000000001</code>] (challenges, in part)"
        ),
        "{body}"
    );
    assert!(
        third.contains("A claim said on no page now, identifier <code>01930000-0000-7000-8000-000000000002</code> (supports, indirectly)"),
        "{body}"
    );
    assert!(
        third.contains(&format!("href=\"{}\"", claim_uri(2))),
        "{body}"
    );
    // What would resolve a row, and the setting the list is read under.
    assert!(
        body.contains(
            "Look at the source, then propose the day you looked and whether it can be had."
        ),
        "{body}"
    );
    assert!(
        body.contains("A source is listed when it was last reviewed more than 1096 days ago."),
        "{body}"
    );
    // The page links nowhere outside the wiki: the wiki fetches nothing.
    let main = body.get(body.find("<main").expect("main")..body.find("</main>").expect("main"));
    let main = main.expect("main");
    assert!(
        !main.contains("http://") && !main.contains("https://"),
        "{main}"
    );
}

#[tokio::test]
async fn the_list_is_the_health_index_s_and_says_so_when_there_is_nothing_to_list() {
    let w = handbook(&[("pages/Home.md", &home_with(SAID))]);
    let index = get(&w, "/handbook/-/health").await.body();
    assert!(
        index.contains("href=\"/handbook/-/health/sources-to-review\""),
        "{index}"
    );
    assert!(index.contains("Sources to review"), "{index}");
    let a = get(&w, LIST).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.body().contains("No source needs a look."), "{}", a.body());
    assert!(!a.body().contains("<table"));
    // Sources that need no look leave the list empty too.
    let mut fine = source(1, SourceKind::Primary);
    fine.last_reviewed = reviewed("2023-11-01");
    put(
        &w,
        &home_with(SAID),
        &[{
            let mut c = claim(1, Standing::Unknown, None, SAID);
            c.sources = vec![link(1, Link::Supports, Bearing::Direct, SAID)];
            c
        }],
        &[fine],
    );
    assert!(
        get(&w, LIST)
            .await
            .body()
            .contains("No source needs a look.")
    );
}

#[tokio::test]
async fn the_list_is_said_in_japanese_too() {
    let w = planted();
    let a = request(&w, APP, "GET", LIST, JA).await;
    assert_eq!(a.status, StatusCode::OK);
    let body = a.body();
    for says in [
        "見直す出典",
        "確認した人が、利用できないと記しています。",
        "確認したと記録した人はまだいません。",
        "最後の確認は 2020-11-12 で、1096 日を超えて前です。",
        "最後の確認から 1096 日を超えた出典が挙げられます。",
        "出典を確かめたうえで、確かめた日と、利用できるかどうかを提案してください。",
    ] {
        assert!(body.contains(says), "{says}: {body}");
    }
    assert_eq!(rows(&body).len(), 3);
}

#[tokio::test]
async fn two_sources_that_say_the_same_of_themselves_are_two_rows_that_differ_in_both_languages() {
    // K43: the list names a source as the claim's page does, whole identifier included, and each claim by its page and
    // its whole identifier.
    let w = handbook(&[("pages/Home.md", &home_with(SAID))]);
    let twin = |n: u8| {
        let mut s = source(n, SourceKind::Primary);
        s.identity = Identity {
            title: Some("The Bridge Survey".to_owned()),
            author: Some("The Harbour Authority".to_owned()),
            date: Some("Spring 1950".to_owned()),
        };
        s
    };
    let mut c = claim(1, Standing::Contested, None, SAID);
    c.sources = vec![
        link(1, Link::Supports, Bearing::Direct, SAID),
        link(2, Link::Supports, Bearing::Direct, SAID),
    ];
    put(&w, &home_with(SAID), &[c], &[twin(1), twin(2)]);
    for (a, words, claimed) in [
        (get(&w, LIST).await, "Its identifier is", "[claim"),
        (request(&w, APP, "GET", LIST, JA).await, "識別子:", "［主張"),
    ] {
        let body = a.body();
        let r = rows(&body);
        assert_eq!(r.len(), 2, "{body}");
        for (n, row) in (1..=2).zip(&r) {
            assert!(
                row.contains(&format!("{words} <code>{}</code>", source_id(n))),
                "{body}"
            );
            assert!(
                row.contains(&format!("{claimed} <code>{}</code>", claim_id(1))),
                "{body}"
            );
        }
        assert_ne!(r.first(), r.get(1), "{body}");
    }
}

#[tokio::test]
async fn a_wiki_that_may_not_be_read_has_no_such_list() {
    let w = World::with_visibility(
        "secret",
        "Secret",
        Visibility::Restricted,
        &[("pages/Home.md", &home_with("Hidden."))],
    );
    let mut s = source(1, SourceKind::Primary);
    s.identity.title = Some("Hidden Source".to_owned());
    put(&w, &home_with("Hidden."), &[], &[s]);
    let real = get(&w, "/secret/-/health/sources-to-review").await;
    let absent = get(&w, "/no-such-wiki/-/health/sources-to-review").await;
    assert_eq!(real.status, StatusCode::NOT_FOUND);
    assert_eq!(real.status, absent.status);
    assert_eq!(
        real.body().replace("secret", "X"),
        absent.body().replace("no-such-wiki", "X")
    );
    assert_eq!(
        real.headers.get(header::CONTENT_LANGUAGE),
        absent.headers.get(header::CONTENT_LANGUAGE)
    );
    assert!(!real.body().contains("Hidden"));
}

#[tokio::test]
async fn nothing_on_the_list_ranks_a_source() {
    let w = planted();
    for headers in [&[][..], JA] {
        let page = request(&w, APP, "GET", LIST, headers).await.body();
        // What the page says, without the header and the footer.
        let start = page.find("<main").expect("a main landmark");
        let end = page.find("</main>").expect("the end of main");
        let body = page.get(start..end).expect("main").to_owned();
        let lower = body.to_lowercase();
        // Only the English page is held to the English words.
        if headers.is_empty() {
            for word in RANKING_WORDS {
                let whole = lower
                    .split(|c: char| !c.is_ascii_alphabetic())
                    .any(|w| w == *word);
                assert!(!whole, "the page says {word:?}: {body}");
            }
        }
        // No number is shown as a rank, no meter or progress element, no colour for a state.
        assert!(!body.contains("<meter") && !body.contains("<progress"));
        assert!(!body.contains("style="), "{body}");
    }
}

// ------------------------------------------------------------------------------------ a day that has not come, and pages (K40)

#[tokio::test]
async fn a_review_on_a_day_that_has_not_come_is_listed_with_its_own_reason_in_both_languages() {
    let w = handbook(&[("pages/Home.md", &home_with(SAID))]);
    let mut ahead = source(1, SourceKind::Primary);
    ahead.last_reviewed = reviewed("2999-01-01");
    ahead.identity.title = Some("The Mistyped Year".to_owned());
    let mut fine = source(2, SourceKind::Primary);
    fine.last_reviewed = reviewed("2023-11-14");
    fine.identity.title = Some("Looked At Today".to_owned());
    put(&w, &home_with(SAID), &[], &[ahead, fine]);
    let body = get(&w, LIST).await.body();
    let r = rows(&body);
    assert_eq!(r.len(), 1, "{body}");
    let row = r.first().expect("a row");
    assert!(row.contains("The Mistyped Year"), "{body}");
    assert!(
        row.contains("Recorded as reviewed on 2999-01-01, a day that has not come."),
        "{body}"
    );
    // It is not said to have been last reviewed, nor to be old.
    assert!(!row.contains("Last reviewed on"), "{body}");
    assert!(!row.contains("days ago"), "{body}");
    assert!(!body.contains("Looked At Today"), "{body}");
    let ja = request(&w, APP, "GET", LIST, JA).await.body();
    assert!(
        ja.contains("確認した日として 2999-01-01 と記録されていますが、その日はまだ来ていません。"),
        "{ja}"
    );
}

/// `n` sources nobody reviewed, titled by their number.
fn crowd(n: u8) -> World {
    let w = handbook(&[("pages/Home.md", &home_with(SAID))]);
    let sources: Vec<_> = (1..=n)
        .map(|i| {
            let mut s = source(i, SourceKind::Other);
            s.identity.title = Some(format!("Survey number {i}"));
            s
        })
        .collect();
    put(&w, &home_with(SAID), &[], &sources);
    w
}

/// The address of the link whose words are `words`, in `body`.
fn link_to(body: &str, words: &str) -> Option<String> {
    body.split("<a href=\"").skip(1).find_map(|rest| {
        let (href, after) = rest.split_once("\">")?;
        after.starts_with(words).then(|| href.replace("&amp;", "&"))
    })
}

#[tokio::test]
async fn the_list_is_shown_fifty_rows_at_a_time_with_a_cursor_and_the_way_back() {
    let w = crowd(55);
    let first = get(&w, LIST).await.body();
    assert_eq!(rows(&first).len(), 50, "{first}");
    assert!(first.contains("Showing sources 1 to 50 of 55."), "{first}");
    assert!(
        link_to(&first, "Show the first sources").is_none(),
        "{first}"
    );
    let more = link_to(&first, "Show the next sources").expect("a link to the next rows");
    assert!(more.starts_with(&format!("{LIST}?after=")), "{more}");
    // The next page is the five that remain, with the way back and no way on.
    let second = get(&w, &more).await;
    assert_eq!(second.status, StatusCode::OK);
    let second = second.body();
    assert_eq!(rows(&second).len(), 5, "{second}");
    assert!(
        second.contains("Showing sources 51 to 55 of 55."),
        "{second}"
    );
    assert!(
        link_to(&second, "Show the next sources").is_none(),
        "{second}"
    );
    assert_eq!(
        link_to(&second, "Show the first sources").as_deref(),
        Some(LIST)
    );
    // No row is on both pages, and every source is on one.
    let titles = |body: &str| -> Vec<String> {
        (1..=55)
            .map(|i| format!("Survey number {i}”"))
            .filter(|t| body.contains(t.as_str()))
            .collect()
    };
    let (a, b) = (titles(&first), titles(&second));
    assert_eq!((a.len(), b.len()), (50, 5));
    assert!(a.iter().all(|t| !b.contains(t)));
    // A cursor that is not one this service gave out is no place to continue from: the list begins.
    let lost = get(&w, &format!("{LIST}?after=not-a-cursor")).await.body();
    assert_eq!(rows(&lost).len(), 50, "{lost}");
    assert!(lost.contains("Showing sources 1 to 50 of 55."), "{lost}");
}

#[tokio::test]
async fn a_list_of_one_page_says_nothing_of_pages() {
    let w = crowd(50);
    let body = get(&w, LIST).await.body();
    assert_eq!(rows(&body).len(), 50);
    assert!(!body.contains("Showing sources"), "{body}");
    assert!(!body.contains("Show the next sources"), "{body}");
    assert!(!body.contains("Show the first sources"), "{body}");
    let ja = crowd(51);
    let body = request(&ja, APP, "GET", LIST, JA).await.body();
    assert!(
        body.contains("51 件のうち 1 件目から 50 件目までを表示しています。"),
        "{body}"
    );
    assert!(body.contains("続きの出典を表示"), "{body}");
}

// ------------------------------------------------------------------------------------ the cursor and the numbering (K50, U-i)

/// The sources `from` to `to`, unreviewed and titled by their number.
fn sources_between(from: u8, to: u8) -> Vec<inklkunn_core::knowledge::Source> {
    (from..=to)
        .map(|i| {
            let mut s = source(i, SourceKind::Other);
            s.identity.title = Some(format!("Survey number {i}"));
            s
        })
        .collect()
}

/// The sources `from` to `to`, as `sources_between` makes them, but looked at lately: they are not listed.
fn looked_at(from: u8, to: u8) -> Vec<inklkunn_core::knowledge::Source> {
    sources_between(from, to)
        .into_iter()
        .map(|mut s| {
            s.last_reviewed = reviewed("2023-11-01");
            s
        })
        .collect()
}

#[tokio::test]
async fn an_edit_elsewhere_does_not_send_the_reader_back_to_the_first_rows() {
    let w = crowd(55);
    let first = get(&w, LIST).await.body();
    let more = link_to(&first, "Show the next sources").expect("a link to the next rows");
    // The wiki changes while the reader reads: the first source is looked at and no longer listed.
    put(&w, &home_with(SAID), &[], &looked_at(1, 1));
    let second = get(&w, &more).await.body();
    // The page continues after the last source the reader was shown (number 50), wherever that now stands.
    assert!(second.contains("Survey number 51”"), "{second}");
    assert!(second.contains("Survey number 55”"), "{second}");
    assert!(!second.contains("Survey number 50”"), "{second}");
    assert_eq!(rows(&second).len(), 5, "{second}");
    assert!(
        second.contains("Showing sources 50 to 54 of 54."),
        "{second}"
    );
}

#[tokio::test]
async fn a_cursor_past_the_end_says_there_are_no_more_sources_and_never_counts_backwards() {
    let w = crowd(55);
    let first = get(&w, LIST).await.body();
    let more = link_to(&first, "Show the next sources").expect("a link to the next rows");
    // The list is shorter now than when the link was made: five sources remain, and the reader had seen fifty.
    put(&w, &home_with(SAID), &[], &looked_at(6, 55));
    let past = get(&w, &more).await;
    assert_eq!(past.status, StatusCode::OK);
    let past = past.body();
    assert!(rows(&past).is_empty(), "{past}");
    assert!(past.contains("No more sources."), "{past}");
    assert!(!past.contains("Showing sources"), "{past}");
    assert!(!past.contains("No source needs a look."), "{past}");
    assert_eq!(
        link_to(&past, "Show the first sources").as_deref(),
        Some(LIST)
    );
    let ja = request(&w, APP, "GET", &more, JA).await.body();
    assert!(ja.contains("これ以上の出典はありません。"), "{ja}");
    // A list with nothing to review at all still says so in its own words.
    put(&w, &home_with(SAID), &[], &looked_at(1, 5));
    let none = get(&w, &more).await.body();
    assert!(none.contains("No source needs a look."), "{none}");
    assert!(!none.contains("No more sources."), "{none}");
}

#[tokio::test]
async fn two_claims_said_on_one_page_are_told_apart_by_a_count_that_goes_on_across_pages() {
    // 55 sources, each cited by one claim, and every claim is said on the page `Home`: the claims are told apart by a
    // count (Home, Home (2), …), and the second page goes on from where the first stopped (U-i).
    let w = crowd(55);
    let claims: Vec<_> = (1..=55u8)
        .map(|n| {
            let mut c = claim(n, Standing::Unknown, None, SAID);
            c.sources = vec![link(n, Link::Supports, Bearing::Direct, SAID)];
            c
        })
        .collect();
    put(&w, &home_with(SAID), &claims, &sources_between(1, 55));
    let first = get(&w, LIST).await.body();
    assert!(first.contains("Home [claim <code>"), "{first}");
    assert!(first.contains("Home (50) [claim <code>"), "{first}");
    let more = link_to(&first, "Show the next sources").expect("a link to the next rows");
    let second = get(&w, &more).await.body();
    assert!(second.contains("Home (51) [claim <code>"), "{second}");
    assert!(second.contains("Home (55) [claim <code>"), "{second}");
    // None of the five begins again at the page's bare name.
    assert!(!second.contains("Home [claim <code>"), "{second}");
}

#[tokio::test]
async fn a_claim_cited_on_a_later_page_keeps_the_number_the_list_gave_it() {
    // K63: the count belongs to the list, not to the page that reads it. Claim 1 is cited by the first source and by the
    // last, which is on the second page; it is `Home` there as on the first page, and the claim only the last source
    // cites is `Home (55)`.
    let w = crowd(55);
    let claims: Vec<_> = (1..=55u8)
        .map(|n| {
            let mut c = claim(n, Standing::Unknown, None, SAID);
            c.sources = vec![link(n, Link::Supports, Bearing::Direct, SAID)];
            if n == 1 {
                c.sources
                    .push(link(55, Link::Supports, Bearing::Partial, SAID));
            }
            c
        })
        .collect();
    put(&w, &home_with(SAID), &claims, &sources_between(1, 55));
    let first = get(&w, LIST).await.body();
    let more = link_to(&first, "Show the next sources").expect("a link to the next rows");
    let second = get(&w, &more).await.body();
    assert!(second.contains("Home [claim <code>"), "{second}");
    assert!(second.contains("Home (55) [claim <code>"), "{second}");
    assert!(!second.contains("Home (2) [claim <code>"), "{second}");
}
