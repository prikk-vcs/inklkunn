//! A claim's thread on its page (RFC 005 part 2 D-11; requirement KN-7; handoff 005 K-T10), through the real router:
//! *Discussion of this claim* with the one sentence that a discussion never changes a claim, the comments in order and
//! in pages, a form for who may comment on a proposal and for nobody else, the post (CSRF, same site, the comment limit,
//! the words of every refusal), every text printed as letters, existence neutrality, and that nothing a comment says
//! changes the claim. No prikk is needed. Every name and sentence is invented.

use axum::http::{StatusCode, header};
use inklkunn_core::knowledge::Standing;
use inklkunn_store::{AccountId, Role, Visibility};

use super::common::{
    APP, Answer, World, add_person, claim, get, get_as, home_with, person, post_form, put, request,
    send_body, with_accounts,
};
use super::{JA, claim_uri};

const SAID: &str = "The bridge opened in 1950.";
const PROPOSAL: &str = "01930000-aaaa-7000-8000-000000000001";

struct Cast {
    w: World,
    questioner: AccountId,
    contributor: AccountId,
    reviewer: AccountId,
    observer: AccountId,
}

fn cast() -> Cast {
    let w = World::new(
        "handbook",
        "The Handbook",
        &[("pages/Home.md", &home_with(SAID))],
    );
    put(
        &w,
        &home_with(SAID),
        &[
            claim(1, Standing::Contested, Some(PROPOSAL), SAID),
            claim(2, Standing::Unknown, None, SAID),
        ],
        &[],
    );
    let w = with_accounts(w);
    let c = Cast {
        questioner: person(1),
        contributor: person(2),
        reviewer: person(3),
        observer: person(4),
        w,
    };
    add_person(&c.w, &c.questioner, "Aiko Tanaka", Some(Role::Questioner));
    add_person(&c.w, &c.contributor, "Ben Okoye", Some(Role::Contributor));
    add_person(&c.w, &c.reviewer, "Rin Sato", Some(Role::Reviewer));
    add_person(&c.w, &c.observer, "Olu Reader", Some(Role::Observer));
    c
}

fn discuss(n: u8) -> String {
    format!("/handbook/-/claim/{}/discuss", super::common::claim_id(n))
}

async fn say(c: &Cast, n: u8, who: &AccountId, fields: &[(&str, &str)]) -> Answer {
    post_form(&c.w, &discuss(n), Some(who), fields).await
}

async fn page(c: &Cast, uri: &str, who: Option<&AccountId>) -> String {
    let a = get_as(&c.w, uri, who).await;
    assert_eq!(a.status, StatusCode::OK, "{uri}");
    a.body()
}

fn location(a: &Answer) -> String {
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    a.header("location").unwrap().to_owned()
}

fn thread_of(body: &str) -> &str {
    let start = body
        .find("<section class=\"claim-thread\"")
        .unwrap_or_default();
    let end = body[start..]
        .find("</section>")
        .map_or(body.len(), |e| start + e);
    body.get(start..end).unwrap_or_default()
}

// ------------------------------------------------------------------------------------ the page

#[tokio::test]
async fn a_claim_page_has_a_discussion_labelled_so_and_says_in_one_sentence_it_changes_nothing() {
    let c = cast();
    let body = page(&c, &claim_uri(1), None).await;
    let thread = thread_of(&body);
    assert!(
        thread.contains("<h2 id=\"claim-thread\">Discussion of this claim</h2>"),
        "{thread}"
    );
    assert!(
        thread.contains(
            "A discussion never changes a claim: only a proposal that people accept does."
        ),
        "{thread}"
    );
    assert!(
        thread.contains("Nobody has discussed this claim yet."),
        "{thread}"
    );
    // Signed out, and an observer: the thread is read and there is no form.
    assert!(!thread.contains("<form"), "{thread}");
    let observer = page(&c, &claim_uri(1), Some(&c.observer)).await;
    assert!(!thread_of(&observer).contains("<form"));
    // The Japanese page says it in Japanese, with the label.
    let ja = request(&c.w, APP, "GET", &claim_uri(1), JA).await.body();
    assert!(thread_of(&ja).contains("この主張についての議論"), "{ja}");
    assert!(
        thread_of(&ja).contains("議論が主張を変えることはありません。"),
        "{ja}"
    );
}

#[tokio::test]
async fn whoever_may_comment_on_a_proposal_is_offered_the_form_and_nobody_else() {
    let c = cast();
    for (who, offered) in [
        (&c.questioner, true),
        (&c.contributor, true),
        (&c.reviewer, true),
        (&c.observer, false),
    ] {
        let body = page(&c, &claim_uri(1), Some(who)).await;
        let thread = thread_of(&body);
        assert_eq!(thread.contains("<form"), offered, "{who}");
        if offered {
            assert!(
                thread.contains("Add a comment to this discussion"),
                "{thread}"
            );
            assert!(
                thread.contains(&format!("action=\"{}\"", discuss(1))),
                "{thread}"
            );
            assert!(
                thread.contains("name=\"csrf\"") || thread.contains("type=\"hidden\""),
                "{thread}"
            );
        }
    }
}

#[tokio::test]
async fn a_comment_is_recorded_shown_in_order_with_its_number_and_never_changes_the_claim() {
    let c = cast();
    let claim_before = page(&c, &claim_uri(1), None).await;
    let home_before = get(&c.w, "/handbook/Home").await.body();
    let history_before = get(&c.w, "/handbook/-/changes").await.body();
    let status_of = |body: &str| {
        let at = body.find("claim-status").unwrap();
        body[at..body[at..].find("</p>").map_or(body.len(), |e| at + e)].to_owned()
    };
    let a = say(
        &c,
        1,
        &c.questioner,
        &[("body", "Where was this measured?")],
    )
    .await;
    let to = location(&a);
    assert!(to.starts_with(&claim_uri(1)) && to.contains("#k-"), "{to}");
    let reply_to = to.split_once("#k-").unwrap().1.to_owned();
    // A reply: the form opens with the comment it answers, and the post carries it.
    let form = page(
        &c,
        &format!("{}?reply={reply_to}", claim_uri(1)),
        Some(&c.contributor),
    )
    .await;
    assert!(thread_of(&form).contains("Replying to comment 1"), "{form}");
    assert!(
        thread_of(&form).contains(&format!("name=\"parent\" value=\"{reply_to}\"")),
        "{form}"
    );
    say(
        &c,
        1,
        &c.contributor,
        &[("body", "At the north gauge."), ("parent", &reply_to)],
    )
    .await;
    say(&c, 1, &c.reviewer, &[("body", "Thank you.")]).await;
    // Another claim has its own discussion.
    say(&c, 2, &c.reviewer, &[("body", "A different claim.")]).await;

    let body = page(&c, &claim_uri(1), None).await;
    let thread = thread_of(&body);
    assert!(thread.contains("3 comments"), "{thread}");
    let at = |needle: &str| {
        thread
            .find(needle)
            .unwrap_or_else(|| panic!("{needle}: {thread}"))
    };
    assert!(
        at("Where was this measured?") < at("At the north gauge.")
            && at("At the north gauge.") < at("Thank you.")
    );
    assert!(
        thread.contains("Comment 1 · ") && thread.contains("Comment 3 · "),
        "{thread}"
    );
    assert!(thread.contains("In reply to comment 1."), "{thread}");
    assert!(
        thread.contains("<strong>Aiko Tanaka</strong>")
            && thread.contains("<strong>Rin Sato</strong>")
    );
    assert!(!thread.contains("A different claim."), "{thread}");
    assert!(
        page(&c, &claim_uri(2), None)
            .await
            .contains("A different claim.")
    );
    // Nothing a comment said touched the claim: its status, its wording and the repository are as they were.
    let after = page(&c, &claim_uri(1), None).await;
    assert_eq!(status_of(&claim_before), status_of(&after));
    assert!(
        after.contains("Contested") || after.contains("contested"),
        "{after}"
    );
    let before_section = claim_before
        .split("<section class=\"claim-thread\"")
        .next()
        .unwrap();
    let after_section = after
        .split("<section class=\"claim-thread\"")
        .next()
        .unwrap();
    assert_eq!(
        before_section, after_section,
        "everything above the discussion is the page it was"
    );
    assert_eq!(
        home_before,
        get(&c.w, "/handbook/Home").await.body(),
        "the page the claim is said on is the page it was"
    );
    assert_eq!(
        history_before,
        get(&c.w, "/handbook/-/changes").await.body(),
        "no change was made to the history"
    );
}

#[tokio::test]
async fn every_text_is_printed_as_letters() {
    let c = cast();
    say(
        &c,
        1,
        &c.questioner,
        &[(
            "body",
            "<script>alert(1)</script> & <img src=x onerror=alert(2)>",
        )],
    )
    .await;
    let body = page(&c, &claim_uri(1), None).await;
    let thread = thread_of(&body);
    assert!(
        !thread.contains("<script>") && !thread.contains("<img"),
        "{thread}"
    );
    assert!(
        thread.contains("&#60;script&#62;alert(1)&#60;/script&#62;"),
        "{thread}"
    );
    assert!(thread.contains("&#38;"), "{thread}");
}

// ------------------------------------------------------------------------------------ the post

#[tokio::test]
async fn a_comment_that_is_empty_or_not_allowed_or_not_confirmed_says_so_in_words() {
    let c = cast();
    let a = say(&c, 1, &c.questioner, &[("body", "   ")]).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        a.body().contains("A comment needs some words."),
        "{}",
        a.body()
    );
    assert!(a.body().contains("Back to the claim"), "{}", a.body());
    let a = say(&c, 1, &c.observer, &[("body", "Hello")]).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    assert!(a.body().contains("<h1>Not allowed</h1>"));
    let a = post_form(&c.w, &discuss(1), None, &[("body", "Hello")]).await;
    assert!(
        a.body().contains("Proposing needs an account"),
        "{}",
        a.body()
    );
    // A signed-in session that does not present its token, and a form from another site, are refused before anything
    // is looked up.
    let no_token = send_body(
        &c.w,
        "POST",
        &discuss(1),
        Some(&c.questioner),
        &[
            ("sec-fetch-site", "same-origin"),
            (super::common::NO_TOKEN, ""),
        ],
        "body=Hello".to_owned(),
    )
    .await;
    assert_eq!(no_token.status, StatusCode::FORBIDDEN);
    let cross = send_body(
        &c.w,
        "POST",
        &discuss(1),
        Some(&c.questioner),
        &[("sec-fetch-site", "cross-site")],
        "body=Hello&csrf=test-session-token".to_owned(),
    )
    .await;
    assert_eq!(cross.status, StatusCode::FORBIDDEN);
    assert!(
        cross
            .body()
            .contains("The form could not be confirmed as coming from a page of this site"),
        "{}",
        cross.body()
    );
    // Nothing was written by any of them.
    assert!(
        page(&c, &claim_uri(1), None)
            .await
            .contains("Nobody has discussed this claim yet.")
    );
}

#[tokio::test]
async fn a_claim_the_wiki_does_not_hold_and_a_wiki_that_may_not_be_read_are_the_pages_they_always_were()
 {
    let c = cast();
    // A claim the wiki does not hold: the page every unknown claim gets; nothing is written.
    let none = get(
        &c.w,
        &format!("/handbook/-/claim/{}", super::common::claim_id(9)),
    )
    .await;
    let a = say(&c, 9, &c.questioner, &[("body", "Hello")]).await;
    assert_eq!(a.status, StatusCode::NOT_FOUND, "{}", a.body());
    let b = say(
        &c,
        1,
        &c.questioner,
        &[("body", "Hello"), ("parent", "not-a-comment")],
    )
    .await;
    assert_eq!(b.status, StatusCode::NOT_FOUND);
    let b = say(&c, 1, &c.questioner, &[("body", "Hello"), ("view", "../x")]).await;
    assert_eq!(b.status, StatusCode::NOT_FOUND);
    let b = say(&c, 1, &c.questioner, &[("body", "Hello"), ("view", "nope")]).await;
    assert_ne!(b.status, StatusCode::SEE_OTHER, "{}", b.body());
    let id = post_form(
        &c.w,
        "/handbook/-/claim/not-an-id/discuss",
        Some(&c.questioner),
        &[("body", "x")],
    )
    .await;
    assert_eq!(id.status, StatusCode::NOT_FOUND);
    assert_eq!(none.status, StatusCode::NOT_FOUND);
    // A reply to a comment that is not in the thread is not a reply.
    let stranger = say(&c, 2, &c.reviewer, &[("body", "On the second claim.")]).await;
    let other = location(&stranger).split_once("#k-").unwrap().1.to_owned();
    let a = say(
        &c,
        1,
        &c.reviewer,
        &[("body", "Misplaced"), ("parent", &other)],
    )
    .await;
    assert_eq!(a.status, StatusCode::NOT_FOUND, "{}", a.body());

    // A wiki the visitor may not read answers as a wiki that is not there, for the post as for the page.
    let w = with_accounts(World::with_visibility(
        "secret",
        "Secret",
        Visibility::Restricted,
        &[("pages/Home.md", &home_with("Hidden."))],
    ));
    put(
        &w,
        &home_with("Hidden."),
        &[claim(1, Standing::Contested, None, "Hidden.")],
        &[],
    );
    add_person(&w, &person(1), "Aiko Tanaka", Some(Role::Questioner));
    add_person(&w, &person(9), "Stranger", None);
    for path in [
        format!("/secret/-/claim/{}/discuss", super::common::claim_id(1)),
        "/secret/-/claim/nothing/discuss".to_owned(),
    ] {
        let absent_path = path.replace("secret", "no-such-wiki");
        let real = post_form(&w, &path, Some(&person(9)), &[("body", "Hello")]).await;
        let absent = post_form(&w, &absent_path, Some(&person(9)), &[("body", "Hello")]).await;
        assert_eq!(real.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(real.status, absent.status);
        assert_eq!(
            real.body().replace("secret", "X"),
            absent.body().replace("no-such-wiki", "X"),
            "{path}"
        );
        assert_eq!(
            real.headers.get(header::CONTENT_LANGUAGE),
            absent.headers.get(header::CONTENT_LANGUAGE)
        );
        assert!(!real.body().contains("Hidden") && !real.body().contains("Secret"));
    }
}

#[tokio::test]
async fn the_comment_limit_of_proposals_is_the_one_a_claim_thread_is_held_to() {
    let c = cast();
    // The limit is 60 an hour (RFC 004 D-11); the person who writes 61 comments is told which limit, and when.
    let mut refused = None;
    for n in 0..70 {
        let a = say(&c, 1, &c.reviewer, &[("body", &format!("Remark {n}"))]).await;
        if a.status != StatusCode::SEE_OTHER {
            refused = Some((n, a));
            break;
        }
    }
    let (n, a) = refused.expect("the limit was reached");
    assert_eq!(n, 60, "sixty comments an hour");
    assert_eq!(a.status, StatusCode::TOO_MANY_REQUESTS, "{}", a.body());
    assert!(a.body().contains("<h1>Refused</h1>"), "{}", a.body());
    assert!(
        a.body()
            .contains("The limit is 60. The count now is 60, and this asked for 1."),
        "{}",
        a.body()
    );
    // Somebody else's limit is their own.
    assert_eq!(
        say(&c, 1, &c.contributor, &[("body", "Mine")]).await.status,
        StatusCode::SEE_OTHER
    );
    // The thread says how many it holds, in pages of fifty: the first page ends and leads on.
    let body = page(&c, &claim_uri(1), None).await;
    let thread = thread_of(&body);
    assert!(thread.contains("61 comments"), "{thread}");
    assert!(thread.contains("Show the next comments"), "{thread}");
    assert!(!thread.contains("Show the first comments"), "{thread}");
    let next = thread
        .split("Show the next comments")
        .next()
        .unwrap()
        .rsplit("href=\"")
        .next()
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .replace("&#38;", "&");
    assert!(next.contains("after="), "{next}");
    let second = page(&c, &next, None).await;
    let thread2 = thread_of(&second);
    assert!(
        thread2.contains("Comment 51 · ") && thread2.contains("Comment 61 · "),
        "{thread2}"
    );
    assert!(!thread2.contains("Comment 50 · "), "{thread2}");
    assert!(thread2.contains("Show the first comments"), "{thread2}");
    assert!(!thread2.contains("Show the next comments"), "{thread2}");
    // A comment to continue from that is not in the thread is the first page, not an error.
    let bogus = page(
        &c,
        &format!(
            "{}?after=019a0000-0000-7000-8000-00000000ffff",
            claim_uri(1)
        ),
        None,
    )
    .await;
    assert!(thread_of(&bogus).contains("Comment 1 · "));
    // A new comment leads to the page that has it: the last.
    let last = location(&say(&c, 1, &c.contributor, &[("body", "The last word.")]).await);
    assert!(last.contains("after=") && last.contains("#k-"), "{last}");
    let url = last.split('#').next().unwrap();
    assert!(page(&c, url, None).await.contains("The last word."));
}

#[tokio::test]
async fn a_claim_read_in_a_view_has_the_one_discussion_and_the_form_stays_in_the_view() {
    let c = cast();
    let uri = format!("{}?view=main", claim_uri(1));
    let body = page(&c, &uri, Some(&c.questioner)).await;
    let thread = thread_of(&body);
    // The form posts to the claim's discussion and carries the view, so that the comment is checked against it.
    assert!(thread.contains("name=\"view\" value=\"main\""), "{thread}");
    let a = say(
        &c,
        1,
        &c.questioner,
        &[("body", "In the view."), ("view", "main")],
    )
    .await;
    let to = location(&a);
    assert!(
        to.starts_with(&format!("{}?view=main", claim_uri(1))),
        "{to}"
    );
    // A claim is one claim in every view: the discussion is the same in the default view.
    assert!(page(&c, &claim_uri(1), None).await.contains("In the view."));
    assert!(page(&c, &uri, None).await.contains("In the view."));
    // A view the wiki does not have is a refusal in words, and nothing is written.
    let a = say(
        &c,
        1,
        &c.questioner,
        &[("body", "Nowhere."), ("view", "nonesuch")],
    )
    .await;
    assert_ne!(a.status, StatusCode::SEE_OTHER);
    assert!(!page(&c, &claim_uri(1), None).await.contains("Nowhere."));
}
