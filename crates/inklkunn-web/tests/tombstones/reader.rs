//! **Who wrote the text decides whether it is hidden** (RFC 006 D-8; INV-13; handoff G-T4, corrections J-1, J-3, J-5), through the
//! real router: the pages a page includes are read at the same block and each one is checked; a difference between two later
//! blocks that both keep a hidden text is hidden on both sides; a change that wrote twenty-five pages hides the twenty-fifth;
//! the same bytes written independently are nobody's but their writer's; a page moved without touching its bytes is still its
//! writer's and a move with an edit is the mover's; and a hide on the proposal revision that was sealed hides the change's text.
//! The records are the in-memory store's and the repository is a double. Every text that must never appear carries `ZEBRA`.

use axum::http::StatusCode;
use inklkunn_store::{
    AccountId, Actor, GovernanceStore, HiddenItem, HiddenItemKind, HiddenParts, ReasonCategory,
    Role, TombstoneAct, TombstoneActId, TombstoneEvent, TombstoneEventId,
};

use super::{HIDING, env, nothing_hidden_in};
use crate::common::{
    Answer, World, add_person, block_id, diff_of, get_as, op, person, post_form, service_text,
    shown, with_accounts,
};

fn page(title: &str, body: &str) -> String {
    format!("---\ntitle: {title}\n---\n\n{body}\n")
}

const DATA: &str = "Call ZEBRAPERSONAL on 555-0100.";

struct Wiki {
    w: World,
    maintainer: AccountId,
    reader: AccountId,
}

fn wiki(files: &[(&str, &str)]) -> Wiki {
    let w = with_accounts(World::new("handbook", "The Handbook", files));
    let (maintainer, reader) = (person(4), person(6));
    add_person(&w, &maintainer, "Mika Ito", Some(Role::Maintainer));
    add_person(&w, &reader, "Olu Reader", Some(Role::Observer));
    Wiki {
        w,
        maintainer,
        reader,
    }
}

impl Wiki {
    /// The change numbered `n` was made: its message and what it did.
    fn made(&self, n: u64, summary: &str, did: Vec<inklkunn_prikk::Operation>) {
        self.w.fake.say(n, &service_text(summary));
        self.w.fake.show(n, shown(did));
    }

    async fn hide(&self, n: u64) -> Answer {
        let a = post_form(
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
        .await;
        assert_eq!(a.status, StatusCode::OK, "{}", a.body());
        a
    }

    async fn unhide_all(&self) {
        let log = self
            .w
            .store
            .tombstone_log(&self.w.wiki.id, None, 50)
            .expect("the log");
        for hide in log.iter().filter(|e| e.act == TombstoneAct::Hide) {
            let a = post_form(
                &self.w,
                "/handbook/-/moderate/unhide",
                Some(&self.maintainer),
                &[("hide", hide.id.as_str())],
            )
            .await;
            assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
        }
    }

    fn viewers(&self) -> Vec<Option<AccountId>> {
        vec![
            None,
            Some(self.reader.clone()),
            Some(self.maintainer.clone()),
        ]
    }

    /// `uri` as every viewer: it answers and says the tombstone, and nothing of the text.
    async fn says_hidden(&self, uri: &str) -> Vec<Answer> {
        let mut all = Vec::new();
        for who in self.viewers() {
            let a = get_as(&self.w, uri, who.as_ref()).await;
            assert_eq!(a.status, StatusCode::OK, "{uri} as {who:?}: {}", a.body());
            nothing_hidden_in(&a, uri);
            assert!(
                a.body().contains(HIDING),
                "{uri} as {who:?} says no tombstone: {}",
                a.body()
            );
            all.push(a);
        }
        all
    }

    async fn says(&self, uri: &str, words: &str) {
        for who in self.viewers() {
            let a = get_as(&self.w, uri, who.as_ref()).await;
            assert_eq!(a.status, StatusCode::OK, "{uri} as {who:?}: {}", a.body());
            assert!(
                a.body().contains(words),
                "{uri} as {who:?} does not say `{words}`: {}",
                a.body()
            );
            assert!(!a.body().contains(HIDING), "{uri}: {}", a.body());
        }
    }
}

fn at(n: u64, title: &str) -> String {
    format!("/handbook/-/at/{}/{title}", block_id(n))
}

// ------------------------------------------------------------------------------------ transclusion

/// 1 creates `Foo`, which includes `Contact`, and `Contact`; 2 (B) writes the data to `Contact`; 3 (C) edits `Foo` only; 4 (D) takes the
/// data out of `Contact`.
fn includes() -> Wiki {
    let with_include = |body: &str| page("Foo", &format!("{body}\n\n![[Contact]]"));
    let wiki = wiki(&[
        ("pages/Foo.md", &with_include("Foo as first written.")),
        ("pages/Contact.md", &page("Contact", "Nobody to call.")),
    ]);
    wiki.made(
        1,
        "Create the handbook",
        vec![
            op("create-file", &["pages/Foo.md"]),
            op("create-file", &["pages/Contact.md"]),
        ],
    );
    wiki.w.fake.set("pages/Contact.md", &page("Contact", DATA));
    wiki.made(
        2,
        "Add the contact",
        vec![op("edit-text", &["pages/Contact.md"])],
    );
    wiki.w
        .fake
        .set("pages/Foo.md", &with_include("Foo, edited."));
    wiki.made(3, "Edit the foo", vec![op("edit-text", &["pages/Foo.md"])]);
    wiki.w
        .fake
        .set("pages/Contact.md", &page("Contact", "Moved to the desk."));
    wiki.made(
        4,
        "Take the contact out",
        vec![op("edit-text", &["pages/Contact.md"])],
    );
    wiki
}

#[tokio::test]
async fn a_page_that_includes_a_page_a_hidden_change_wrote_is_hidden_at_its_permalink() {
    let w = includes();
    // The permalink of Foo at C embeds Contact as B wrote it, and it is held (rendered once).
    let before = get_as(&w.w, &at(3, "Foo"), None).await;
    assert!(before.body().contains("ZEBRAPERSONAL"), "{}", before.body());
    w.hide(2).await;
    // Foo's own text is C's, and it says that what it includes is hidden: the held render is asked again, and the include is read at
    // the same block through the same reader.
    w.says_hidden(&at(3, "Foo")).await;
    // At D, Contact is D's own: the page includes what is there.
    w.says(&at(4, "Foo"), "Moved to the desk.").await;
    // At A, nothing hidden is written yet.
    w.says(&at(1, "Foo"), "Nobody to call.").await;
    // An unhide gives it back, and a hide again takes it away at once.
    w.unhide_all().await;
    w.says(&at(3, "Foo"), "ZEBRAPERSONAL").await;
    w.hide(2).await;
    w.says_hidden(&at(3, "Foo")).await;
}

#[tokio::test]
async fn a_page_that_includes_a_hidden_page_is_hidden_when_it_was_never_rendered_before_the_hiding()
{
    // No held render to be asked again: the include is read, at the block, through the reader, and it says hidden.
    let w = includes();
    w.hide(2).await;
    w.says_hidden(&at(3, "Foo")).await;
    w.says(&at(4, "Foo"), "Moved to the desk.").await;
}

#[tokio::test]
async fn a_hide_builds_the_index_of_who_wrote_each_path_and_a_wiki_with_nothing_hidden_builds_none()
{
    use inklkunn_store::DerivedStore;
    let w = includes();
    let default_view = w.w.wiki.default_view.clone();
    let state = |w: &Wiki| {
        w.w.store
            .writes_state(&w.w.wiki.id, &default_view)
            .expect("the derived store")
    };
    // Reading the whole history of a wiki in which nothing is hidden indexes nothing.
    for n in 1..=4 {
        let a = get_as(&w.w, &at(n, "Foo"), None).await;
        assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    }
    assert!(state(&w).is_none(), "nothing is hidden: nothing is indexed");
    // The moderator's hide builds it toward the tip at once, so the reads after it find it built.
    w.hide(2).await;
    let built = state(&w).expect("the hide built the index");
    assert_eq!(built.head_number, 4);
    assert_eq!(built.head_block, block_id(4));
}

#[tokio::test]
async fn a_page_included_at_a_block_before_it_was_written_does_not_fail_the_permalink() {
    // Contact is made after Foo, which includes it: at Foo's first block there is no such page, and the embed stays what it was.
    let wiki = wiki(&[("pages/Foo.md", &page("Foo", "Foo.\n\n![[Contact]]"))]);
    wiki.made(
        1,
        "Create the foo",
        vec![op("create-file", &["pages/Foo.md"])],
    );
    wiki.w
        .fake
        .set("pages/Contact.md", &page("Contact", "Called later."));
    wiki.made(
        2,
        "Add the contact",
        vec![op("create-file", &["pages/Contact.md"])],
    );
    // And a later change rewrites it, so that no tip holds what the hidden change wrote.
    wiki.w
        .fake
        .set("pages/Contact.md", &page("Contact", "Called again."));
    wiki.made(
        3,
        "Rewrite the contact",
        vec![op("edit-text", &["pages/Contact.md"])],
    );
    let a = get_as(&wiki.w, &at(1, "Foo"), None).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    wiki.hide(2).await;
    let a = get_as(&wiki.w, &at(1, "Foo"), None).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    assert!(!a.body().contains(HIDING), "{}", a.body());
}

// ------------------------------------------------------------------------------------ diffs

#[tokio::test]
async fn a_difference_between_two_later_blocks_that_both_keep_the_hidden_text_is_hidden_on_both_sides()
 {
    let wiki = wiki(&[
        ("pages/Foo.md", &page("Foo", "Foo as first written.")),
        ("pages/Bar.md", &page("Bar", "Bar as first written.")),
    ]);
    wiki.made(
        1,
        "Create",
        vec![
            op("create-file", &["pages/Foo.md"]),
            op("create-file", &["pages/Bar.md"]),
        ],
    );
    wiki.w.fake.set("pages/Foo.md", &page("Foo", DATA));
    wiki.made(
        2,
        "Add the contact",
        vec![op("edit-text", &["pages/Foo.md"])],
    );
    wiki.w
        .fake
        .set("pages/Bar.md", &page("Bar", "Bar, edited."));
    wiki.made(3, "Edit the bar", vec![op("edit-text", &["pages/Bar.md"])]);
    wiki.w
        .fake
        .set("pages/Bar.md", &page("Bar", "Bar, edited again."));
    wiki.made(
        4,
        "Edit the bar again",
        vec![op("edit-text", &["pages/Bar.md"])],
    );
    wiki.w
        .fake
        .set("pages/Foo.md", &page("Foo", "Foo without the contact."));
    wiki.made(
        5,
        "Take the contact out",
        vec![op("edit-text", &["pages/Foo.md"])],
    );
    // What `diff` would say of Foo from C to D is nothing (the page did not change); the text of both sides is B's.
    wiki.w.fake.diff_says(Ok(diff_of(
        "pages/Foo.md",
        "modified",
        &["@@ -3,1 +3,1 @@\n-Call ZEBRAPERSONAL on 555-0100.\n+Call ZEBRAPERSONAL on 555-0101.\n"],
        3,
        4,
    )));
    wiki.hide(2).await;
    let uri = format!("/handbook/-/diff/{}/{}/Foo", block_id(3), block_id(4));
    for a in wiki.says_hidden(&uri).await {
        assert!(
            a.body()
                .contains("The text of the earlier change is hidden"),
            "{}",
            a.body()
        );
        assert!(
            a.body().contains("The text of the later change is hidden"),
            "{}",
            a.body()
        );
    }
    assert!(
        wiki.w.fake.diffs_asked().is_empty(),
        "no difference is asked of a text that is hidden: {:?}",
        wiki.w.fake.diffs_asked()
    );
    // The API says the same.
    let api = format!(
        "/handbook/-/api/v1/diff?from={}&to={}&path=Foo",
        block_id(3),
        block_id(4)
    );
    let a = get_as(&wiki.w, &api, None).await;
    nothing_hidden_in(&a, &api);
    assert!(a.body().contains("\"hidden\""), "{}", a.body());
}

// ------------------------------------------------------------------------------------ who wrote it

#[tokio::test]
async fn a_change_that_wrote_twenty_five_pages_hides_the_twenty_fifth_on_every_surface() {
    let wiki = wiki(&[("pages/Home.md", &page("Home", "Hi."))]);
    wiki.made(1, "Create", vec![op("create-file", &["pages/Home.md"])]);
    let files: Vec<(String, String)> = (0..25)
        .map(|i| (format!("pages/P{i}.md"), page(&format!("P{i}"), DATA)))
        .collect();
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    wiki.w.fake.set_files(&borrowed);
    let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    wiki.made(
        2,
        "A directory of contacts",
        paths.iter().map(|p| op("create-file", &[p])).collect(),
    );
    wiki.w.fake.set("pages/Home.md", &page("Home", "Hi again."));
    wiki.made(
        3,
        "Edit the home",
        vec![op("edit-text", &["pages/Home.md"])],
    );
    // A later change rewrites every one of them, so that no tip holds what the hidden change wrote.
    let rewritten: Vec<(String, String)> = (0..25)
        .map(|i| {
            (
                format!("pages/P{i}.md"),
                page(&format!("P{i}"), "Rewritten."),
            )
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = rewritten
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    wiki.w.fake.set_files(&borrowed);
    wiki.made(
        4,
        "Rewrite the directory",
        paths.iter().map(|p| op("edit-text", &[p])).collect(),
    );
    for i in [0, 24] {
        let before = get_as(&wiki.w, &at(3, &format!("P{i}")), None).await;
        assert!(
            before.body().contains("ZEBRAPERSONAL"),
            "P{i}: {}",
            before.body()
        );
    }
    wiki.hide(2).await;
    for i in 0..25 {
        wiki.says_hidden(&at(3, &format!("P{i}"))).await;
    }
    wiki.says(&at(3, "Home"), "Hi again.").await;
}

#[tokio::test]
async fn bytes_written_independently_by_another_change_are_not_hidden() {
    let wiki = wiki(&[
        ("pages/Foo.md", &page("Foo", "Foo as first written.")),
        ("pages/Bar.md", &page("Bar", "Bar as first written.")),
    ]);
    wiki.made(
        1,
        "Create",
        vec![
            op("create-file", &["pages/Foo.md"]),
            op("create-file", &["pages/Bar.md"]),
        ],
    );
    wiki.w.fake.set("pages/Foo.md", &page("Foo", DATA));
    wiki.made(
        2,
        "Add the contact",
        vec![op("edit-text", &["pages/Foo.md"])],
    );
    // The very same bytes at another path, written by another change; and the same bytes at the same path again, by a later one.
    wiki.w.fake.set("pages/Bar.md", &page("Foo", DATA));
    wiki.made(
        3,
        "Say it elsewhere",
        vec![op("edit-text", &["pages/Bar.md"])],
    );
    wiki.w
        .fake
        .set("pages/Foo.md", &page("Foo", "Foo without the contact."));
    wiki.made(4, "Take it out", vec![op("edit-text", &["pages/Foo.md"])]);
    // And the other page is rewritten too, so that no tip holds these bytes anywhere.
    wiki.w
        .fake
        .set("pages/Bar.md", &page("Bar", "Bar, rewritten."));
    wiki.made(
        5,
        "Rewrite the bar",
        vec![op("edit-text", &["pages/Bar.md"])],
    );
    wiki.hide(2).await;
    wiki.says_hidden(&at(2, "Foo")).await;
    wiki.says(&at(3, "Bar"), "ZEBRAPERSONAL").await;
    // And the other way: hide the second writer, and the first one's page is still its own.
    wiki.unhide_all().await;
    wiki.hide(3).await;
    wiki.says(&at(2, "Foo"), "ZEBRAPERSONAL").await;
    wiki.says_hidden(&at(3, "Bar")).await;
}

#[tokio::test]
async fn a_page_moved_without_touching_its_bytes_is_hidden_and_a_move_with_an_edit_is_not() {
    for edited in [false, true] {
        let wiki = wiki(&[
            ("pages/Foo.md", &page("Foo", "Foo as first written.")),
            ("pages/Bar.md", &page("Bar", "Bar as first written.")),
        ]);
        wiki.made(
            1,
            "Create",
            vec![
                op("create-file", &["pages/Foo.md"]),
                op("create-file", &["pages/Bar.md"]),
            ],
        );
        wiki.w.fake.set("pages/Foo.md", &page("Foo", DATA));
        wiki.made(
            2,
            "Add the contact",
            vec![op("edit-text", &["pages/Foo.md"])],
        );
        // C moves Foo to Moved (the double moves the bytes; the change says it did, and whether it edited them).
        let moved = if edited {
            page("Moved", "Edited on the way.")
        } else {
            page("Foo", DATA)
        };
        wiki.w
            .fake
            .apply(&[("pages/Moved.md", moved.as_str())], &["pages/Foo.md"]);
        let mut did = vec![op("rename-path", &["pages/Foo.md", "pages/Moved.md"])];
        if edited {
            did.push(op("edit-text", &["pages/Moved.md"]));
        }
        wiki.made(3, "Move the foo", did);
        // D (4) rewrites the moved page, so that no tip holds what B wrote.
        wiki.w
            .fake
            .set("pages/Moved.md", &page("Moved", "Rewritten."));
        wiki.made(
            4,
            "Rewrite the moved page",
            vec![op("edit-text", &["pages/Moved.md"])],
        );
        wiki.hide(2).await;
        if edited {
            wiki.says(&at(3, "Moved"), "Edited on the way.").await;
        } else {
            wiki.says_hidden(&at(3, "Moved")).await;
        }
        wiki.says(&at(4, "Moved"), "Rewritten.").await;
        wiki.says_hidden(&at(2, "Foo")).await;
    }
}

// ------------------------------------------------------------------------------------ one source of truth (J-5)

#[tokio::test]
async fn a_hide_on_the_proposal_revision_that_was_sealed_hides_the_text_of_the_change_it_became() {
    let e = env().await;
    // The page revision has no hide of its own: the content of the accepted proposal's revision 1 (sealed as change 2) was hidden before the
    // acceptance, which the record of the proposal's revision keeps.
    let maintainer = e.maintainer.clone();
    e.w.store
        .record_tombstone_event(&TombstoneEvent {
            id: TombstoneEventId::new(),
            act_id: TombstoneActId::new(),
            wiki: e.w.wiki.id.clone(),
            at: 1_700_000_000,
            act: TombstoneAct::Hide,
            item: HiddenItem {
                kind: HiddenItemKind::ProposalRevision,
                id: e.accepted.to_string(),
                reference: Some("1".to_owned()),
            },
            parts: HiddenParts {
                content: true,
                summary: false,
                author: false,
            },
            category: Some(ReasonCategory::PersonalData),
            note: None,
            affected: None,
            by: Actor {
                account: maintainer,
                rung: Role::Maintainer,
            },
            policy_block: None,
            of_event: None,
            appeal: None,
            declined_transition: None,
        })
        .expect("a hide");
    let at_2 = get_as(&e.w, &format!("/handbook/-/at/{}/Home", block_id(2)), None).await;
    assert_eq!(at_2.status, StatusCode::OK, "{}", at_2.body());
    nothing_hidden_in(&at_2, "the permalink of the sealed change");
    assert!(at_2.body().contains(HIDING), "{}", at_2.body());
    // The change after it wrote the page its own way.
    let at_3 = get_as(&e.w, &format!("/handbook/-/at/{}/Home", block_id(3)), None).await;
    assert!(
        at_3.body().contains("Plain tip paragraph."),
        "{}",
        at_3.body()
    );
}
