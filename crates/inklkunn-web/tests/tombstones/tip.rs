//! **What the tip rule can promise** (RFC 006 D-8; handoff G-T4, correction H-5). The content of a view's tip cannot be hidden, and
//! the rule is a comparison of bytes: (i) **a page moved with its bytes is still the tip's text**, in whichever view it is, so
//! hiding the change that wrote it is refused; (ii) **lines that survive**: when a later change fixed a typo and nothing else, the
//! tip no longer holds the change's file whole, the hide is allowed, and the form says how many of the lines the change added the
//! current page still has, that hiding the change does not hide them, and that a hide covers this change and not text a later
//! change writes again. The records are the in-memory store's and the repository is a double.

use axum::http::StatusCode;
use inklkunn_store::{AccountId, Role};

use crate::common::{
    Answer, World, add_person, block_id, get_as, op, person, post_form, service_text, shown,
    with_accounts,
};

const FOO: &str = "pages/Foo.md";
const BAZ: &str = "pages/Baz.md";

fn foo(body: &str) -> String {
    format!("---\ntitle: Foo\n---\n\n{body}\n")
}

fn baz(body: &str) -> String {
    format!("---\ntitle: Baz\n---\n\n{body}\n")
}

struct Wiki {
    w: World,
    maintainer: AccountId,
}

fn wiki(first: &str) -> Wiki {
    let w = with_accounts(World::new("handbook", "The Handbook", &[(FOO, first)]));
    let maintainer = person(4);
    add_person(&w, &maintainer, "Mika Ito", Some(Role::Maintainer));
    w.fake.say(1, "Create the handbook");
    w.fake.show(1, shown(vec![op("create-file", &[FOO])]));
    Wiki { w, maintainer }
}

impl Wiki {
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
}

#[tokio::test]
async fn a_page_moved_with_its_bytes_is_still_the_tip_and_its_text_cannot_be_hidden() {
    let s = wiki(&foo("Foo as first written."));
    // B writes the text; C moves the page with its bytes and is the tip.
    s.w.fake.set(FOO, &foo("Call the number on file."));
    s.w.fake.say(2, &service_text("Add the contact"));
    s.w.fake.show(2, shown(vec![op("edit-text", &[FOO])]));
    s.w.fake.rename(FOO, BAZ);
    s.w.fake.say(3, &service_text("Move the page"));
    s.w.fake
        .show(3, shown(vec![op("rename-path", &[FOO, BAZ])]));
    // The bytes of `Foo` at B are not at `Foo` at the tip, and are at `Baz`.
    let refused = s.hide(2).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.body());
    assert!(
        refused.body().contains("current text of a page"),
        "{}",
        refused.body()
    );
    let form = s.form(2).await;
    assert!(
        form.body().contains("current text of a page"),
        "{}",
        form.body()
    );
    // The change that moved it holds the same bytes: also the tip's.
    let moved = s.hide(3).await;
    assert_eq!(moved.status, StatusCode::CONFLICT, "{}", moved.body());
}

#[tokio::test]
async fn a_page_moved_in_another_view_is_still_that_views_tip() {
    let s = wiki(&foo("Foo as first written."));
    s.w.fake.set(FOO, &foo("Call the number on file."));
    s.w.fake.say(2, &service_text("Add the contact"));
    s.w.fake.show(2, shown(vec![op("edit-text", &[FOO])]));
    s.w.fake.rename(FOO, BAZ);
    s.w.fake.say(3, &service_text("Move the page"));
    s.w.fake
        .show(3, shown(vec![op("rename-path", &[FOO, BAZ])]));
    // The default view goes on: it changes the page, so its tip holds nothing of B's file. The view `alt` stays at the move.
    s.w.fake.set(BAZ, &baz("Something else entirely."));
    s.w.fake.say(4, &service_text("Rewrite it"));
    s.w.fake.show(4, shown(vec![op("edit-text", &[BAZ])]));
    s.w.fake.add_view_at("alt", 3);
    let refused = s.hide(2).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.body());
    assert!(refused.body().contains("alt"), "{}", refused.body());
}

#[tokio::test]
async fn lines_that_survive_a_later_fix_are_counted_and_the_hide_is_allowed_with_the_sentence() {
    let s = wiki(&foo("Original line."));
    s.w.fake.set(
        FOO,
        &foo("Alpha line.\n\nBeta line.\n\nGamma line with the number 555-0100."),
    );
    s.w.fake.say(2, &service_text("Add three lines"));
    s.w.fake.show(2, shown(vec![op("edit-text", &[FOO])]));
    // C fixes one line and nothing else: the tip no longer holds the change's file whole.
    s.w.fake.set(
        FOO,
        &foo("Alpha line.\n\nBeta line.\n\nGamma line with the number removed."),
    );
    s.w.fake.say(3, &service_text("Take the number out"));
    s.w.fake.show(3, shown(vec![op("edit-text", &[FOO])]));
    let form = s.form(2).await;
    assert_eq!(form.status, StatusCode::OK, "{}", form.body());
    let body = form.body();
    assert!(
        body.contains(
            "The current page still contains 2 lines this change added. Hiding the change does not hide them; propose an edit that removes them."
        ),
        "{body}"
    );
    // A hide covers this change and not text that a later change writes again: said on the form of a change.
    assert!(
        body.contains(
            "A hiding covers this change. It does not cover text that a later change writes again."
        ),
        "{body}"
    );
    // The hide is allowed all the same.
    let done = s.hide(2).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.body());
    // A change whose lines are all gone says nothing of survivors: the form of the change that took the number out holds the tip.
    let tip = s.form(3).await;
    assert!(!tip.body().contains("still contains"), "{}", tip.body());
}

#[tokio::test]
async fn a_change_none_of_whose_lines_survive_says_nothing_of_survivors() {
    let s = wiki(&foo("Original line."));
    s.w.fake.set(FOO, &foo("Call 555-0100 today."));
    s.w.fake.say(2, &service_text("Add the contact"));
    s.w.fake.show(2, shown(vec![op("edit-text", &[FOO])]));
    s.w.fake.set(FOO, &foo("A different sentence altogether."));
    s.w.fake.say(3, &service_text("Replace it"));
    s.w.fake.show(3, shown(vec![op("edit-text", &[FOO])]));
    let form = s.form(2).await;
    assert!(!form.body().contains("still contains"), "{}", form.body());
    assert!(form.body().contains("A hiding covers this change"));
    let done = s.hide(2).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.body());
}

#[tokio::test]
async fn a_single_surviving_line_is_said_in_the_singular() {
    let s = wiki(&foo("Original line."));
    s.w.fake
        .set(FOO, &foo("Alpha line.\n\nCall 555-0100 today."));
    s.w.fake.say(2, &service_text("Add two lines"));
    s.w.fake.show(2, shown(vec![op("edit-text", &[FOO])]));
    s.w.fake.set(FOO, &foo("Alpha line.\n\nCall nobody."));
    s.w.fake.say(3, &service_text("Fix one"));
    s.w.fake.show(3, shown(vec![op("edit-text", &[FOO])]));
    let body = s.form(2).await.body();
    assert!(
        body.contains(
            "The current page still contains 1 line this change added. Hiding the change does not hide it; propose an edit that removes it."
        ),
        "{body}"
    );
}

#[tokio::test]
async fn a_page_too_long_for_the_count_says_at_least() {
    let s = wiki(&foo("Original line."));
    let long: String = (0..20_100)
        .map(|n| format!("Line number {n}.\n"))
        .collect::<Vec<_>>()
        .join("\n");
    s.w.fake.set(FOO, &foo(&long));
    s.w.fake.say(2, &service_text("Add a long text"));
    s.w.fake.show(2, shown(vec![op("edit-text", &[FOO])]));
    s.w.fake
        .set(FOO, &foo(&format!("{long}\nAn addition at the end.")));
    s.w.fake.say(3, &service_text("Add a line"));
    s.w.fake.show(3, shown(vec![op("edit-text", &[FOO])]));
    let body = s.form(2).await.body();
    // The count stopped at its bound (the three lines of the page's head are looked at too, and are not added).
    assert!(
        body.contains("The current page still contains at least 19,997 lines this change added."),
        "{}",
        body.lines()
            .filter(|l| l.contains("still contains"))
            .collect::<String>()
    );
}
