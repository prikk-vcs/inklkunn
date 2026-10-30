//! Recent changes and the Atom feeds through the real router (RFC 003 D-7; handoff 003 T6): the changes of all the
//! views with the people from the message, the pages touched, the filters and the words that say a list is
//! filtered; the feeds validated against the structure of Atom by a reader of their own, not by eye; and the
//! rule that a wiki the reader may not read is not there, for the page and for the feed alike. The repository is
//! an in-memory double, so these run in both test runs.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;
mod xml;

use common::{
    Answer, World, block_id, get, import_message, let_read, page, proposal_message, request,
    with_accounts,
};
use inklkunn_store::{AccountId, Visibility};

const FILES: &[(&str, &str)] = &[];

fn files() -> Vec<(&'static str, String)> {
    vec![
        ("pages/Home.md", page(&["title: Home"], "Welcome.\n")),
        (
            "pages/Guides/Setup.md",
            page(&["title: Guides/Setup"], "Set up.\n"),
        ),
        (
            "pages/Guides/Usage.md",
            page(&["title: Guides/Usage"], "Use.\n"),
        ),
        ("pages/Notes.md", page(&["title: Notes"], "Notes.\n")),
        (
            "pages/Draft Chapter.md",
            page(&["title: Draft Chapter"], "Draft.\n"),
        ),
    ]
}

/// A wiki with five changes: one newest on `main`, an import, one made by hand, and one only in the `draft`
/// view.
fn world(visibility: Visibility, slug: &str, name: &str) -> World {
    let owned = files();
    let borrowed: Vec<(&str, &str)> = owned.iter().map(|(p, t)| (*p, t.as_str())).collect();
    let w = World::with_visibility(slug, name, visibility, &borrowed);
    let _ = FILES;
    w.fake.tip_message(&proposal_message(
        "Update the Home page",
        "Aiko Tanaka",
        "Ben Okoye",
    ));
    w.fake.touches(&block_id(1), &["pages/Home.md"]);
    w.fake.push_change(
        "heads/main",
        &block_id(101),
        &import_message("Import the handbook"),
    );
    w.fake.touches(
        &block_id(101),
        &["pages/Guides/Setup.md", "pages/Guides/Usage.md"],
    );
    w.fake
        .push_change("heads/main", &block_id(102), "Created by hand");
    w.fake.touches(&block_id(102), &["pages/Notes.md"]);
    w.fake.add_view("draft");
    w.fake.push_change(
        "heads/draft",
        &block_id(103),
        &proposal_message("Draft the new chapter", "Aiko Tanaka", "Aiko Tanaka"),
    );
    w.fake.touches(&block_id(103), &["pages/Draft Chapter.md"]);
    w
}

fn handbook() -> World {
    world(Visibility::Public, "handbook", "The Handbook")
}

fn order(body: &str, needles: &[&str]) -> bool {
    let at: Vec<Option<usize>> = needles.iter().map(|n| body.find(n)).collect();
    at.iter().all(Option::is_some)
        && at
            .windows(2)
            .all(|w| w.first().unwrap() < w.get(1).unwrap())
}

#[tokio::test]
async fn the_changes_of_every_view_are_listed_with_their_people_pages_and_the_time_they_were_first_seen()
 {
    let w = handbook();
    let a = get(&w, "/handbook/-/changes").await;
    assert_eq!(a.status, 200, "{}", a.body());
    let body = a.body();
    // The change, the people from the message, the page it touched as a link.
    assert!(body.contains("Update the Home page"), "{body}");
    assert!(body.contains("Proposed by Aiko Tanaka"), "{body}");
    assert!(body.contains("Accepted by Ben Okoye"), "{body}");
    assert!(
        body.contains("<a href=\"/handbook/Home\">Home</a>"),
        "{body}"
    );
    // An import is the service's own; a change nobody recorded through the service says so.
    assert!(body.contains("Import the handbook"), "{body}");
    assert!(
        body.contains("Made by inklkunn, the wiki service"),
        "{body}"
    );
    assert!(body.contains("Created by hand"), "{body}");
    assert!(
        body.contains("Recorded outside this service, so the people are not known."),
        "{body}"
    );
    // A change only the draft view has is there, its page in that view.
    assert!(body.contains("Draft the new chapter"), "{body}");
    assert!(
        body.contains("<a href=\"/handbook/~draft/Draft%20Chapter\">Draft Chapter</a>"),
        "{body}"
    );
    // Newest first within the view, the default view before the others.
    assert!(
        order(
            &body,
            &[
                "Update the Home page",
                "Import the handbook",
                "Created by hand",
                "Draft the new chapter"
            ]
        ),
        "{body}"
    );
    // The time is what this server saw, and the page says so; the table is a table.
    assert!(body.contains("First seen here"), "{body}");
    assert!(
        body.contains("The history does not record when a change was made"),
        "{body}"
    );
    assert!(body.contains("<time datetime=\""), "{body}");
    assert!(
        body.contains("<caption>Changes, newest first</caption>"),
        "{body}"
    );
    // Nothing is filtered, so nothing says so.
    assert!(!body.contains("The list is filtered"), "{body}");
    // The feed is one link away.
    assert!(body.contains("href=\"/handbook/-/changes.atom\""), "{body}");
    // A page that can be read by anyone is cached by no one but the reader.
    assert!(a.header("cache-control").unwrap().contains("private"));
}

#[tokio::test]
async fn a_filter_narrows_the_list_and_the_page_says_it_is_filtered() {
    let w = handbook();
    for (query, shown, hidden, says) in [
        (
            "namespace=Guides",
            "Import the handbook",
            "Update the Home page",
            "Only pages under Guides",
        ),
        (
            "view=draft",
            "Draft the new chapter",
            "Import the handbook",
            "Only the view draft",
        ),
        (
            "automated=hide",
            "Update the Home page",
            "Import the handbook",
            "Changes made by the wiki service itself or with the help of a machine are hidden",
        ),
        (
            "automated=only",
            "Import the handbook",
            "Update the Home page",
            "Only changes made by the wiki service itself or with the help of a machine",
        ),
        (
            "page=Home",
            "Update the Home page",
            "Import the handbook",
            "Only changes to the page Home",
        ),
    ] {
        let a = get(&w, &format!("/handbook/-/changes?{query}")).await;
        assert_eq!(a.status, 200, "{query}");
        let body = a.body();
        assert!(body.contains(shown), "{query}: {body}");
        assert!(!body.contains(hidden), "{query}: {body}");
        assert!(body.contains("The list is filtered"), "{query}: {body}");
        assert!(body.contains(says), "{query}: {body}");
        assert!(body.contains("Show all changes"), "{query}: {body}");
        // The feed link carries the filter, so the feed is the list.
        assert!(
            body.contains(&format!(
                "href=\"/handbook/-/changes.atom?{}",
                query.split('&').next().unwrap()
            )),
            "{query}: {body}"
        );
    }
    // A page nothing changed: an empty list, said in words, still filtered.
    let a = get(&w, "/handbook/-/changes?page=Nowhere").await;
    assert!(a.body().contains("There are no changes to show."));
    assert!(a.body().contains("The list is filtered"));
}

#[tokio::test]
async fn the_pages_of_a_change_are_asked_for_once_and_a_failure_to_ask_is_said() {
    let w = handbook();
    let _ = get(&w, "/handbook/-/changes").await;
    let first = w.fake.shows();
    assert_eq!(first, 4, "one `show` for each change listed");
    let _ = get(&w, "/handbook/-/changes").await;
    assert_eq!(
        w.fake.shows(),
        first,
        "a change never changes: what it touched is remembered"
    );

    // A prikk that cannot answer `show`: the changes are listed without their pages, and the list says so
    // when a filter by page needed them.
    let w = handbook();
    w.fake.fail_show(true);
    let a = get(&w, "/handbook/-/changes").await;
    assert_eq!(a.status, 200);
    assert!(
        a.body()
            .contains("The pages of this change could not be read just now."),
        "{}",
        a.body()
    );
    let a = get(&w, "/handbook/-/changes?namespace=Guides").await;
    assert!(
        a.body()
            .contains("Some changes could not be matched to their pages just now"),
        "{}",
        a.body()
    );
}

// ------------------------------------------------------------------------------------ feeds

fn feed_of(a: &Answer) -> xml::Element {
    assert_eq!(a.status, 200, "{}", a.body());
    assert_eq!(
        a.header("content-type"),
        Some("application/atom+xml; charset=utf-8")
    );
    assert_eq!(
        xml::atom_violations(&a.body()),
        Vec::<String>::new(),
        "{}",
        a.body()
    );
    xml::parse(&a.body()).unwrap()
}

#[tokio::test]
async fn the_feeds_of_the_wiki_a_namespace_and_a_page_validate_as_atom() {
    let w = handbook();
    let wiki = feed_of(&get(&w, "/handbook/-/changes.atom").await);
    assert_eq!(
        wiki.first("title").unwrap().text,
        "Recent changes · The Handbook"
    );
    let entries: Vec<_> = wiki.all("entry").collect();
    assert_eq!(entries.len(), 4);
    assert_eq!(
        entries.first().unwrap().first("title").unwrap().text,
        "Update the Home page"
    );
    // The people are the author; the page is the link; the pages touched are the summary.
    assert_eq!(
        entries
            .first()
            .unwrap()
            .first("author")
            .unwrap()
            .first("name")
            .unwrap()
            .text,
        "Ben Okoye"
    );
    let link = entries
        .first()
        .unwrap()
        .all("link")
        .find(|l| l.attr("rel") == Some("alternate"))
        .unwrap();
    assert_eq!(
        link.attr("href"),
        Some("http://localhost:8080/handbook/Home")
    );
    assert_eq!(
        entries.first().unwrap().first("summary").unwrap().text,
        "Pages: Home"
    );
    // Ids are the wiki's stable id and the change's, so they survive a rename of the address.
    assert!(
        entries
            .first()
            .unwrap()
            .first("id")
            .unwrap()
            .text
            .starts_with(&format!("urn:inklkunn:{}:", w.wiki.id))
    );
    // The feed knows its own address.
    let own = wiki
        .all("link")
        .find(|l| l.attr("rel") == Some("self"))
        .unwrap();
    assert_eq!(
        own.attr("href"),
        Some("http://localhost:8080/handbook/-/changes.atom")
    );

    let ns = feed_of(&get(&w, "/handbook/-/changes.atom?namespace=Guides").await);
    assert_eq!(
        ns.first("title").unwrap().text,
        "Recent changes under Guides · The Handbook"
    );
    assert_eq!(ns.all("entry").count(), 1);
    assert_ne!(
        ns.first("id").unwrap().text,
        wiki.first("id").unwrap().text,
        "each feed is its own feed"
    );

    let one = feed_of(&get(&w, "/handbook/-/changes.atom?page=Notes").await);
    assert_eq!(
        one.first("title").unwrap().text,
        "Changes to Notes · The Handbook"
    );
    assert_eq!(one.all("entry").count(), 1);
    let own = one
        .all("link")
        .find(|l| l.attr("rel") == Some("self"))
        .unwrap();
    assert_eq!(
        own.attr("href"),
        Some("http://localhost:8080/handbook/-/changes.atom?page=Notes")
    );

    // A feed with nothing in it is still a feed.
    let none = feed_of(&get(&w, "/handbook/-/changes.atom?page=Nowhere").await);
    assert_eq!(none.all("entry").count(), 0);
}

#[tokio::test]
async fn what_an_author_wrote_in_a_summary_or_a_name_is_text_in_the_page_and_in_the_feed() {
    let w = handbook();
    w.fake.tip_message(&proposal_message(
        "<script>alert(1)</script> & \"quotes\"",
        "<b>Mallory</b>",
        "Ben & <i>Okoye</i>",
    ));
    let a = get(&w, "/handbook/-/changes").await;
    let body = a.body();
    assert!(!body.contains("<script>alert(1)"), "{body}");
    assert!(!body.contains("<b>Mallory"), "{body}");
    assert!(
        body.contains("&#60;script&#62;alert(1)&#60;/script&#62;"),
        "{body}"
    );

    let feed = feed_of(&get(&w, "/handbook/-/changes.atom").await);
    let first = feed.all("entry").next().unwrap();
    // Escaped in the document, and the same text once the document is read.
    assert_eq!(
        first.first("title").unwrap().text,
        "<script>alert(1)</script> & \"quotes\""
    );
    assert_eq!(
        first.first("author").unwrap().first("name").unwrap().text,
        "Ben & <i>Okoye</i>"
    );
    assert!(
        !get(&w, "/handbook/-/changes.atom")
            .await
            .body()
            .contains("<script>")
    );
}

#[tokio::test]
async fn a_foreign_message_with_control_characters_cannot_break_the_feed() {
    let w = handbook();
    w.fake
        .tip_message("Tab\there\u{7} bell and \u{1b}[31mred\u{1b}[0m");
    let feed = feed_of(&get(&w, "/handbook/-/changes.atom").await);
    assert!(feed.all("entry").next().is_some());
}

// ------------------------------------------------------------------------------------ access

#[tokio::test]
async fn a_wiki_the_reader_may_not_read_is_not_there_for_the_page_or_the_feed() {
    let vault = world(Visibility::Restricted, "vault", "The Vault");
    let nothing = world(Visibility::Public, "elsewhere", "Elsewhere");
    for uri in [
        "/{}/-/changes",
        "/{}/-/changes?namespace=Guides&automated=hide",
        "/{}/-/changes.atom",
        "/{}/-/changes.atom?page=Home",
    ] {
        let a = get(&vault, &uri.replace("{}", "vault")).await;
        let b = get(&nothing, &uri.replace("{}", "vault")).await;
        assert_eq!(a.status, 404, "{uri}");
        assert_eq!(a.status, b.status, "{uri}");
        assert_eq!(
            a.bytes, b.bytes,
            "{uri}: the same answer, byte for byte, whether or not it exists"
        );
        assert!(!a.body().contains("Update the Home page"), "{uri}");
        assert!(!a.body().contains("Vault"), "{uri}");
        assert!(!a.body().contains("urn:inklkunn"), "{uri}");
    }
    // Asking took no more from prikk for the one than for the other.
    assert_eq!(vault.fake.shows(), 0);
}

#[tokio::test]
async fn a_reader_of_a_restricted_wiki_gets_its_changes_and_the_feed_is_private_and_not_for_indexing()
 {
    let w = with_accounts(world(Visibility::Restricted, "vault", "The Vault"));
    let reader = AccountId::new();
    let_read(&w, &reader);
    let who = reader.to_string();
    let as_reader = [("x-test-account", who.as_str())];
    // Anonymous: not there. The reader: there.
    assert_eq!(get(&w, "/vault/-/changes").await.status, 404);
    assert_eq!(get(&w, "/vault/-/changes.atom").await.status, 404);
    let page = request(&w, common::APP, "GET", "/vault/-/changes", &as_reader).await;
    assert_eq!(page.status, 200);
    assert!(page.body().contains("Update the Home page"));
    assert!(page.body().contains("name=\"robots\" content=\"noindex"));
    assert_eq!(page.header("x-robots-tag"), Some("noindex, nofollow"));
    let feed = request(&w, common::APP, "GET", "/vault/-/changes.atom", &as_reader).await;
    assert_eq!(feed.status, 200);
    // Not for a shared cache, not for an index, whatever the reader subscribes with.
    assert!(feed.header("cache-control").unwrap().contains("private"));
    assert_eq!(feed.header("x-robots-tag"), Some("noindex, nofollow"));
    // A reader of the vault is no reader of the other wiki's secrets: its feed holds only its own ids.
    assert_eq!(
        xml::atom_violations(&feed.body()),
        Vec::<String>::new(),
        "{}",
        feed.body()
    );
    // And the public handbook's feed names nothing of the vault, to anyone.
    let handbook = world(Visibility::Public, "handbook", "The Handbook");
    let public = get(&handbook, "/handbook/-/changes.atom").await;
    assert!(public.header("x-robots-tag").is_none());
    assert!(!public.body().contains("vault"));
}
