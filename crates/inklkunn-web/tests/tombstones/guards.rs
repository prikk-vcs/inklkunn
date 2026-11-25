//! **What the moderation form refuses and says when a change cannot be checked** (RFC 006 D-8; handoff G-T4; the L round): a change
//! with an operation whose path could not be resolved is refused by name, in both languages (L-2); the later changes that may have
//! written the page, and the moves of the page past the bound, are said and never left out (L-6); and the preview of the form is the
//! core's to refuse to a person who may not moderate (L-7). The records are the in-memory store's and the repository is a double.

use axum::http::StatusCode;
use inklkunn_core::Principal;
use inklkunn_core::tombstone::{Item, ModerationError};
use inklkunn_prikk::Operation;
use inklkunn_prikk::storage::PathRef;
use inklkunn_store::{AccountId, Role};

use crate::common::{
    Answer, World, add_person, block_id, get_as, op, person, post_form, service_text, shown,
    with_accounts,
};

const FOO: &str = "pages/Foo.md";

fn foo(body: &str) -> String {
    format!("---\ntitle: Foo\n---\n\n{body}\n")
}

/// An operation whose path prikk could not resolve.
fn unresolved() -> Operation {
    Operation {
        kind: "edit-text".to_owned(),
        paths: vec![PathRef {
            path: None,
            unresolved_node_id: Some("n".repeat(8)),
        }],
    }
}

struct Wiki {
    w: World,
    maintainer: AccountId,
    reader: AccountId,
}

fn wiki() -> Wiki {
    let w = with_accounts(World::new(
        "handbook",
        "The Handbook",
        &[(FOO, &foo("Foo as first written."))],
    ));
    let (maintainer, reader) = (person(4), person(6));
    add_person(&w, &maintainer, "Mika Ito", Some(Role::Maintainer));
    add_person(&w, &reader, "Olu Reader", Some(Role::Observer));
    w.fake.say(1, "Create the handbook");
    w.fake.show(1, shown(vec![op("create-file", &[FOO])]));
    Wiki {
        w,
        maintainer,
        reader,
    }
}

impl Wiki {
    /// The next block: the page as `text`, and what `show` says it did.
    fn write(&self, n: u64, text: &str, did: Vec<Operation>) {
        self.w.fake.set(FOO, text);
        self.w.fake.say(n, &service_text("Edit the page"));
        self.w.fake.show(n, shown(did));
    }

    async fn form(&self, n: u64, lang: &str) -> Answer {
        let uri = format!(
            "/handbook/-/moderate?kind=page-revision&block={}",
            block_id(n)
        );
        crate::common::request(
            &self.w,
            crate::common::APP,
            "GET",
            &uri,
            &[
                ("x-test-account", self.maintainer.as_str()),
                ("cookie", &format!("inklkunn_lang={lang}")),
            ],
        )
        .await
    }

    async fn hide(&self, n: u64, parts: &[(&str, &str)]) -> Answer {
        let block = block_id(n);
        let mut fields: Vec<(&str, &str)> = vec![
            ("kind", "page-revision"),
            ("block", block.as_str()),
            ("category", "personal-data"),
            ("note", ""),
        ];
        fields.extend_from_slice(parts);
        post_form(
            &self.w,
            "/handbook/-/moderate",
            Some(&self.maintainer),
            &fields,
        )
        .await
    }
}

#[tokio::test]
async fn a_change_with_an_operation_on_no_resolved_path_cannot_have_its_text_hidden_and_the_form_says_so_in_both_languages()
 {
    let s = wiki();
    // Block 2 wrote the page and has an operation whose path could not be resolved; block 3 replaced the page, so no tip holds
    // what block 2 wrote at the path that was resolved: without the refusal the hide would be allowed.
    s.write(
        2,
        &foo("Call the number on file."),
        vec![op("edit-text", &[FOO]), unresolved()],
    );
    s.write(
        3,
        &foo("Something else entirely."),
        vec![op("edit-text", &[FOO])],
    );
    let en = s.form(2, "en").await;
    assert_eq!(en.status, StatusCode::OK, "{}", en.body());
    assert!(
        en.body().contains(
            "Some of what this change did could not be traced to a file, so it cannot be compared with the current text of the pages, and its text cannot be hidden. Its summary and its author can be hidden now."
        ),
        "{}",
        en.body()
    );
    let ja = s.form(2, "ja").await.body();
    assert!(
        ja.contains("どのファイルに対する操作なのかを特定できなかったものがあり"),
        "{ja}"
    );
    // The text's hide is refused, the form is not offered it, and the change is not a tip's all the same.
    let refused = s.hide(2, &[("part_content", "1")]).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.body());
    // The sentence promises the summary and the author can be hidden now: they can.
    let done = s
        .hide(2, &[("part_summary", "1"), ("part_author", "1")])
        .await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.body());
    // A change whose operations are all resolved and is not a tip's is not refused (the refusal is of the unresolved one).
    let plain = s.form(3, "en").await.body();
    assert!(!plain.contains("could not be traced"), "{plain}");
}

#[tokio::test]
async fn a_refusal_that_is_not_not_found_leaves_a_change_unchecked_and_its_text_cannot_be_hidden() {
    // P-1 (a): the page a change added is not at the tip, and the `stat` that tells why refuses with something that is not *not found*:
    // the file may be there, so the change is not shown to be no tip's, and the hide is refused with the refusal for a change that
    // cannot be compared (it is allowed once the store says the file is not there).
    const NEW: &str = "pages/New.md";
    let s = wiki();
    s.w.fake.set(NEW, &foo("Call the number on file."));
    s.w.fake.say(2, &service_text("Add a page"));
    s.w.fake.show(2, shown(vec![op("create-file", &[NEW])]));
    s.w.fake.remove(NEW);
    s.w.fake.say(3, &service_text("Remove the page"));
    s.w.fake.show(3, shown(vec![op("delete-file", &[NEW])]));
    s.w.fake
        .fail_stat(Some(inklkunn_prikk::StorageError::Refused {
            verb: "cat",
            message: "the store is damaged".to_owned(),
            kind: None,
            reason: None,
        }));
    let form = s.form(2, "en").await.body();
    assert!(
        form.contains("cannot be compared with the current pages, so its text cannot be hidden"),
        "{form}"
    );
    // The Japanese says the same of both causes (Q-5): not *too large*.
    let ja = s.form(2, "ja").await.body();
    assert!(
        ja.contains("現在のページと照らし合わせられないため、本文は非表示にできません"),
        "{ja}"
    );
    assert!(!ja.contains("大きすぎて、ページの現在の本文"), "{ja}");
    let refused = s.hide(2, &[("part_content", "1")]).await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.body());
    s.w.fake.fail_stat(None);
    let done = s.hide(2, &[("part_content", "1")]).await;
    assert_eq!(done.status, StatusCode::OK, "{}", done.body());
}

#[tokio::test]
async fn later_changes_that_may_have_written_the_page_and_a_page_moved_past_the_bound_are_said_in_both_languages()
 {
    let s = wiki();
    let lines = |own: &str| {
        foo(&format!(
            "Added line one.\n\nAdded line two.\n\nAdded line three.\n\n{own}"
        ))
    };
    s.write(2, &lines("Own 2."), vec![op("edit-text", &[FOO])]);
    // Block 3: the record cannot say what it did, so it was not read, and the list says it may have written the page.
    s.write(3, &lines("Own 3."), vec![unresolved()]);
    // Blocks 4 ...: the page is moved, hop after hop, with its bytes untouched, further than the list follows.
    let mut at = FOO.to_owned();
    for hop in 0..=inklkunn_core::read::MAX_FOLLOWED_MOVES {
        let to = format!("pages/Moved{hop}.md");
        let n = 4 + hop as u64;
        s.w.fake.set_files(&[(to.as_str(), &lines("Own 3."))]);
        s.w.fake.remove(&at);
        s.w.fake.say(n, &service_text("Move the page"));
        s.w.fake.show(
            n,
            shown(vec![op("rename-path", &[at.as_str(), to.as_str()])]),
        );
        at = to;
    }
    let en = s.form(2, "en").await.body();
    assert!(
        en.contains("1 later change may have written this page, but the record of what it did cannot say so either way, so it was not checked and is not listed."),
        "{en}"
    );
    assert!(
        en.contains("This page was moved more than 8 times after this change, so the changes to the pages it was moved to after the last of those moves are not listed."),
        "{en}"
    );
    let ja = s.form(2, "ja").await.body();
    assert!(
        ja.contains("後の変更のうち、1 件は、このページを書き換えた可能性があります"),
        "{ja}"
    );
    assert!(
        ja.contains("このページは 8 回を超えて移動されたため"),
        "{ja}"
    );
}

#[tokio::test]
async fn the_core_refuses_the_preview_to_a_person_who_may_not_moderate_and_the_form_answers_as_it_did()
 {
    let s = wiki();
    s.write(
        2,
        &foo("Call the number on file."),
        vec![op("edit-text", &[FOO])],
    );
    s.write(
        3,
        &foo("Something else entirely."),
        vec![op("edit-text", &[FOO])],
    );
    let authority = inklkunn_core::IdentityAuthority::for_tests();
    let item = Item::PageRevision {
        block: block_id(2),
        view: None,
    };
    let before = s.w.fake.asked();
    // A reader (an observer) may read the wiki and may not moderate it: the core says so, and makes no read of the repository.
    let reader = Principal::session(&authority, s.reader.clone());
    assert_eq!(
        s.w.services
            .moderation
            .preview(&reader, &s.w.wiki, &item)
            .unwrap_err(),
        ModerationError::NotAllowed
    );
    assert_eq!(
        s.w.fake.asked().since(before),
        crate::common::Asked {
            shows: 0,
            cats: 0,
            logs: 0
        },
        "a refused preview asks the repository nothing"
    );
    // Nobody who is not signed in moderates either.
    assert_eq!(
        s.w.services
            .moderation
            .preview(&Principal::Anonymous, &s.w.wiki, &item)
            .unwrap_err(),
        ModerationError::NotAllowed
    );
    // The moderator is answered.
    let maintainer = Principal::session(&authority, s.maintainer.clone());
    assert!(
        s.w.services
            .moderation
            .preview(&maintainer, &s.w.wiki, &item)
            .is_ok()
    );
    // The page the reader is given is the one it was given before: *not allowed*.
    let page = get_as(
        &s.w,
        &format!(
            "/handbook/-/moderate?kind=page-revision&block={}",
            block_id(2)
        ),
        Some(&s.reader),
    )
    .await;
    assert_eq!(page.status, StatusCode::FORBIDDEN, "{}", page.body());
}
