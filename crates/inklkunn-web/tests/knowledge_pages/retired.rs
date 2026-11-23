//! A retired claim and a retired source (RFC 006 D-10; handoff G-T6; acceptance criterion 11), through the real router over an in-memory
//! repository: the claim leaves the paragraph's link and keeps its page with *retired* and the reason; a retired source is marked on
//! every claim that cites it and leaves *Sources to review*; and a claim or a source of 0.3.0 that says nothing of this is the page it
//! was. Every sentence is invented.

use super::common::{World, claim, get, home_with, link, put, request, source};
use super::{APP, JA, claim_uri, main_of};
use inklkunn_core::knowledge::{
    Bearing, Identity, Lifecycle, Link, SourceKind, SourceLife, Standing,
};

const SAID: &str = "The bridge opened in 1950.";
const LIST: &str = "/handbook/-/health/sources-to-review";

fn handbook() -> World {
    World::new(
        "handbook",
        "The Handbook",
        &[("pages/Home.md", &home_with(SAID))],
    )
}

/// A claim cited by two sources, one retired with a reason and one active.
fn planted(claim_reason: Option<&str>, retired_claim: bool) -> World {
    let w = handbook();
    let mut c = claim(1, Standing::Contested, None, SAID);
    if retired_claim {
        c.lifecycle = Lifecycle::Retired;
        c.retired_reason = claim_reason.map(str::to_owned);
    }
    c.sources = vec![
        link(1, Link::Supports, Bearing::Direct, SAID),
        link(2, Link::Challenges, Bearing::Partial, SAID),
    ];
    let mut old = source(1, SourceKind::Primary);
    old.identity = Identity {
        title: Some("The Bridge Survey".to_owned()),
        author: None,
        date: None,
    };
    old.lifecycle = SourceLife::Retired;
    old.retired_reason = Some("A second edition replaced it.".to_owned());
    let mut kept = source(2, SourceKind::Secondary);
    kept.identity.title = Some("The Ferry Timetable".to_owned());
    put(&w, &home_with(SAID), &[c], &[old, kept]);
    w
}

#[tokio::test]
async fn a_retired_claim_leaves_the_paragraphs_link_and_keeps_its_page_with_its_reason() {
    let w = planted(Some("The survey was withdrawn."), true);
    let home = get(&w, "/handbook/Home").await;
    assert_eq!(home.status, 200);
    assert!(
        !home.body().contains("Sources and views"),
        "a retired claim is not offered beside its paragraph: {}",
        home.body()
    );
    assert!(!home.body().contains("/-/claim/"), "{}", home.body());
    let page = get(&w, &claim_uri(1)).await;
    assert_eq!(page.status, 200, "{}", page.body());
    let main = main_of(&page);
    assert!(main.contains("This claim is retired."), "{main}");
    assert!(
        main.contains("Why it was retired: The survey was withdrawn."),
        "{main}"
    );
    // It keeps its sources, and where it is said.
    assert!(main.contains("The Bridge Survey"), "{main}");
    assert!(main.contains("/handbook/Home#"), "{main}");
    // The reason is plain text, whatever it holds.
    let w = planted(Some("<b>x</b> & more"), true);
    let page = get(&w, &claim_uri(1)).await;
    assert!(
        page.body()
            .contains("Why it was retired: &lt;b&gt;x&lt;/b&gt; &amp; more")
            || page.body().contains("&#60;b&#62;x"),
        "{}",
        page.body()
    );
    assert!(!page.body().contains("<b>x</b>"));
    // In Japanese, with the glossary's word.
    let ja = request(&w, APP, "GET", &claim_uri(1), JA).await;
    assert!(ja.body().contains("役目を終えました"), "{}", ja.body());
}

#[tokio::test]
async fn a_claim_retired_before_the_reason_existed_says_so() {
    let w = planted(None, true);
    let page = get(&w, &claim_uri(1)).await;
    assert!(page.body().contains("This claim is retired."));
    assert!(
        page.body().contains("No reason was recorded."),
        "{}",
        page.body()
    );
}

#[tokio::test]
async fn a_claim_that_is_not_retired_is_the_page_it_was() {
    let w = planted(None, false);
    let home = get(&w, "/handbook/Home").await;
    assert!(home.body().contains("Sources and views"), "{}", home.body());
    let page = get(&w, &claim_uri(1)).await;
    assert!(
        !page.body().contains("This claim is retired"),
        "{}",
        page.body()
    );
    assert!(
        !page.body().contains("Why it was retired"),
        "{}",
        page.body()
    );
}

#[tokio::test]
async fn a_retired_source_is_marked_on_the_claim_that_cites_it_and_leaves_the_sources_to_review() {
    let w = planted(None, false);
    let page = get(&w, &claim_uri(1)).await;
    let main = main_of(&page);
    assert!(
        main.contains("This source is retired: A second edition replaced it."),
        "{main}"
    );
    // Exactly one source is marked, and the other is as it was.
    assert_eq!(main.matches("This source is retired").count(), 1, "{main}");
    assert!(main.contains("The Ferry Timetable"), "{main}");
    // Nobody checked either; only the one in use is to be reviewed.
    let list = get(&w, LIST).await;
    assert_eq!(list.status, 200, "{}", list.body());
    assert!(
        list.body().contains("The Ferry Timetable"),
        "{}",
        list.body()
    );
    assert!(
        !list.body().contains("The Bridge Survey"),
        "{}",
        list.body()
    );
    // A retired source without a reason says so too.
    let w = handbook();
    let mut c = claim(1, Standing::Unknown, None, SAID);
    c.sources = vec![link(1, Link::Supports, Bearing::Direct, SAID)];
    let mut s = source(1, SourceKind::Primary);
    s.lifecycle = SourceLife::Retired;
    put(&w, &home_with(SAID), &[c], &[s]);
    let page = get(&w, &claim_uri(1)).await;
    assert!(
        page.body()
            .contains("This source is retired. No reason was recorded."),
        "{}",
        page.body()
    );
}
