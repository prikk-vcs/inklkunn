//! **The later changes that still hold a change's text** (RFC 006 D-8, RR-15; handoff G-T4, correction J-4). Hiding a change hides
//! the change, not what a later change wrote again, so the moderation form lists up to ten later changes of each page the change
//! wrote whose text still has most of the lines the change added, says that hiding the change does not hide them, and says what it
//! could not check (a bound, a later change hidden itself, an index that does not reach the change yet). It names a page's path and
//! a date and links to the page as that change left it: never the text, never a name. The records are the in-memory store's and the
//! repository is a double.

use axum::http::StatusCode;
use inklkunn_store::{AccountId, Role};

use crate::common::{
    Answer, World, add_person, block_id, get_as, op, person, post_form, service_text, shown,
    with_accounts,
};

const FOO: &str = "pages/Foo.md";

fn foo(body: &str) -> String {
    format!("---\ntitle: Foo\n---\n\n{body}\n")
}

/// The page with the lines of `0..count` and one line of its own.
fn lines(count: usize, own: usize) -> String {
    let mut all: Vec<String> = (0..count).map(|n| format!("Added line {n}.")).collect();
    all.push(format!("ZEBRA own line {own}."));
    foo(&all.join("\n\n"))
}

struct Wiki {
    w: World,
    maintainer: AccountId,
    /// The number of the last block.
    last: u64,
}

fn wiki() -> Wiki {
    let w = with_accounts(World::new(
        "handbook",
        "The Handbook",
        &[(FOO, &foo("First."))],
    ));
    let maintainer = person(4);
    add_person(&w, &maintainer, "Mika Ito", Some(Role::Maintainer));
    w.fake.say(1, "Create the handbook");
    w.fake.show(1, shown(vec![op("create-file", &[FOO])]));
    Wiki {
        w,
        maintainer,
        last: 1,
    }
}

impl Wiki {
    /// A change of the page: the next block.
    fn write(&mut self, text: &str) -> u64 {
        self.w.fake.set(FOO, text);
        self.last += 1;
        self.w.fake.say(self.last, &service_text("Edit the page"));
        self.w
            .fake
            .show(self.last, shown(vec![op("edit-text", &[FOO])]));
        self.last
    }

    async fn form(&self, n: u64) -> Answer {
        get_as(
            &self.w,
            &format!(
                "/handbook/-/moderate?kind=page-revision&block={}",
                block_id(n)
            ),
            Some(&self.maintainer),
        )
        .await
    }

    async fn hide(&self, n: u64) -> Answer {
        post_form(
            &self.w,
            "/handbook/-/moderate",
            Some(&self.maintainer),
            &[
                ("kind", "page-revision"),
                ("block", block_id(n).as_str()),
                ("part_content", "1"),
                ("category", "personal-data"),
                ("note", ""),
            ],
        )
        .await
    }
}

const INTRO: &str = "These later changes to the page still hold most of the lines it added, and hiding this change does not hide them.";

#[tokio::test]
async fn the_form_lists_the_later_changes_that_hold_most_of_the_lines_with_a_date_and_a_link() {
    let mut s = wiki();
    let change = s.write(&lines(6, 0));
    let holds = s.write(&lines(4, 1)); // four of the seven lines the change added
    s.write(&lines(3, 2)); // three of seven is not most
    s.write(&foo("Something else entirely."));
    let form = s.form(change).await;
    assert_eq!(form.status, StatusCode::OK, "{}", form.body());
    let body = form.body();
    assert!(body.contains(INTRO), "{body}");
    assert!(body.contains("pages/Foo.md, first seen "), "{body}");
    assert!(body.contains(" UTC"), "{body}");
    // The link is the page as that change left it (the permalink form), and only that change is listed.
    let link = format!("/handbook/-/at/{}/Foo", block_id(holds));
    assert!(body.contains(&link), "{body}");
    assert_eq!(
        body.matches("pages/Foo.md, first seen ").count(),
        1,
        "{body}"
    );
    // The form tells the moderator how to decide, and never says the text or a name of who wrote the later change.
    assert!(!body.contains("ZEBRA"), "{body}");
    assert!(!body.contains("could not be checked"), "{body}");
}

#[tokio::test]
async fn nothing_is_said_when_no_later_change_holds_the_lines() {
    let mut s = wiki();
    let change = s.write(&lines(6, 0));
    s.write(&lines(2, 1));
    s.write(&foo("Something else entirely."));
    let body = s.form(change).await.body();
    assert!(!body.contains(INTRO), "{body}");
    assert!(!body.contains("first seen"), "{body}");
}

#[tokio::test]
async fn more_than_ten_are_ten_and_a_count() {
    let mut s = wiki();
    let change = s.write(&lines(6, 0));
    for own in 1..=12 {
        s.write(&lines(6, own));
    }
    let body = s.form(change).await.body();
    assert_eq!(
        body.matches("pages/Foo.md, first seen ").count(),
        10,
        "{body}"
    );
    assert!(body.contains("and 2 more"), "{body}");
}

#[tokio::test]
async fn a_later_change_that_is_hidden_itself_is_skipped_and_said() {
    let mut s = wiki();
    let change = s.write(&lines(6, 0));
    let first = s.write(&lines(6, 1));
    let second = s.write(&lines(5, 2));
    s.write(&foo("Something else entirely."));
    let hidden = s.hide(first).await;
    assert_eq!(hidden.status, StatusCode::OK, "{}", hidden.body());
    let body = s.form(change).await.body();
    assert!(body.contains(&block_id(second)), "{body}");
    assert!(!body.contains(&block_id(first)), "{body}");
    assert!(
        body.contains(
            "1 later change has its own text hidden, so it was not checked and is not listed"
        ),
        "{body}"
    );
}

#[tokio::test]
async fn later_changes_past_the_bound_are_counted_and_said() {
    let mut s = wiki();
    let change = s.write(&lines(6, 0));
    for own in 1..=60 {
        s.write(&lines(6, own));
    }
    let body = s.form(change).await.body();
    assert_eq!(
        body.matches("pages/Foo.md, first seen ").count(),
        10,
        "{body}"
    );
    assert!(body.contains("and 40 more"), "{body}");
    assert!(
        body.contains("10 more later changes could not be checked: at most 50 later changes of a page are read"),
        "{body}"
    );
}

#[tokio::test]
async fn an_index_that_does_not_reach_the_change_is_said_not_left_out() {
    let mut s = wiki();
    for own in 0..70 {
        s.write(&lines(6, own));
    }
    let change = s.write(&lines(7, 100));
    s.write(&lines(7, 101));
    // The first request builds one bound of the index from the oldest change; this one is further on.
    let body = s.form(change).await.body();
    assert!(
        body.contains("could not be listed now, because the record of who wrote what does not reach this change yet"),
        "{body}"
    );
    assert!(!body.contains(INTRO), "{body}");
}

#[tokio::test]
async fn pages_past_the_bound_are_counted_and_said() {
    let mut s = wiki();
    // A change that wrote twenty-one pages (of different sizes, so that no page is mistaken for a moved one).
    let pages: Vec<(String, String)> = (0..21)
        .map(|n| {
            (
                format!("pages/Q{n:02}.md"),
                foo(&format!("Same line.\n\n{}", "x".repeat(n + 1))),
            )
        })
        .collect();
    let refs: Vec<(&str, &str)> = pages
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    s.w.fake.set_files(&refs);
    s.last += 1;
    s.w.fake.say(s.last, &service_text("Add pages"));
    let ops = pages
        .iter()
        .map(|(p, _)| op("create-file", &[p.as_str()]))
        .collect();
    s.w.fake.show(s.last, shown(ops));
    let change = s.last;
    // A later change rewrites every one of them, so that no tip holds the change's files whole (the hide is allowed).
    let rewritten: Vec<(String, String)> = (0..21)
        .map(|n| {
            (
                format!("pages/Q{n:02}.md"),
                foo(&format!("Rewritten {}", "y".repeat(n + 1))),
            )
        })
        .collect();
    let refs: Vec<(&str, &str)> = rewritten
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    s.w.fake.set_files(&refs);
    s.last += 1;
    s.w.fake.say(s.last, &service_text("Rewrite pages"));
    let ops = rewritten
        .iter()
        .map(|(p, _)| op("edit-text", &[p.as_str()]))
        .collect();
    s.w.fake.show(s.last, shown(ops));
    let body = s.form(change).await.body();
    assert!(
        body.contains(
            "1 more page this change wrote could not be checked: at most 20 pages are looked at"
        ),
        "{body}"
    );
}

#[tokio::test]
async fn a_change_whose_text_cannot_be_hidden_lists_nothing_to_hide_with_it() {
    let mut s = wiki();
    let change = s.write(&lines(6, 0));
    // The page moves with its bytes (the tip holds them at the new path), and a later change writes the old path again.
    s.w.fake.rename(FOO, "pages/Baz.md");
    s.last += 1;
    s.w.fake.say(s.last, &service_text("Move the page"));
    s.w.fake.show(
        s.last,
        shown(vec![op("rename-path", &[FOO, "pages/Baz.md"])]),
    );
    s.write(&lines(5, 1));
    let body = s.form(change).await.body();
    assert!(body.contains("current text of a page"), "{body}");
    assert!(!body.contains(INTRO), "{body}");
    assert!(!body.contains("first seen"), "{body}");
}
