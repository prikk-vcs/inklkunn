#![allow(clippy::expect_used, clippy::unwrap_used)]
//! What an acceptance writes: the revision as it is, or re-authored against a moved tip, or the overlaps that
//! stop it; and the `^id` a comment's block is given (CM-5).

use std::path::Path;

use inklkunn_store::{Anchor, Base, CommentId, CommentRecord, FileChange, Quote, Rename};

use super::*;
use crate::read::testing::{Fake, block_id, state_id};

fn page(id: &str, body: &str) -> String {
    format!("---\ntitle: {id}\nid: {id}\n---\n\n{body}")
}

/// A revision made against version 1 of the fake.
fn revision(files: &[(&str, Option<String>)], renames: &[(&str, &str)]) -> RevisionContent {
    RevisionContent {
        base: Base::new(&state_id(1), &block_id(1)).unwrap(),
        files: files
            .iter()
            .map(|(p, t)| FileChange {
                path: (*p).to_owned(),
                text: t.clone(),
            })
            .collect(),
        renames: renames
            .iter()
            .map(|(a, b)| Rename {
                from: (*a).to_owned(),
                to: (*b).to_owned(),
            })
            .collect(),
        minor: false,
        automated: false,
    }
}

fn run(fake: &Fake, content: &RevisionContent) -> Prepared {
    prepare(fake, Path::new("/r"), content, &fake.block_id()).unwrap_or_else(|e| panic!("{e:?}"))
}

type Written = Vec<(String, Option<String>)>;

fn change(p: Prepared) -> (Written, Vec<(String, String)>, bool) {
    match p {
        Prepared::Change {
            files,
            renames,
            reauthored,
        } => (
            files
                .into_iter()
                .map(|f| {
                    (
                        f.path.to_str().unwrap().to_owned(),
                        f.content.map(|b| String::from_utf8(b).unwrap()),
                    )
                })
                .collect(),
            renames
                .into_iter()
                .map(|(a, b)| {
                    (
                        a.to_str().unwrap().to_owned(),
                        b.to_str().unwrap().to_owned(),
                    )
                })
                .collect(),
            reauthored,
        ),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_view_that_has_not_moved_gives_the_revision_as_it_is_and_reads_nothing() {
    let fake = Fake::new(&[("pages/A.md", "x")]);
    fake.fail_read(Some(StorageError::Driver {
        verb: "cat",
        detail: "no reads".to_owned(),
    }));
    let content = revision(
        &[("pages/A.md", Some("new".to_owned())), ("pages/B.md", None)],
        &[("pages/B.md", "pages/C.md")],
    );
    let (files, renames, reauthored) =
        change(prepare(&fake, Path::new("/r"), &content, &block_id(1)).unwrap());
    assert!(!reauthored);
    assert_eq!(
        files,
        vec![
            ("pages/A.md".to_owned(), Some("new".to_owned())),
            ("pages/B.md".to_owned(), None)
        ]
    );
    assert_eq!(
        renames,
        vec![("pages/B.md".to_owned(), "pages/C.md".to_owned())]
    );
}

#[test]
fn a_moved_view_writes_only_the_pages_whose_text_differs_at_the_tip() {
    let fake = Fake::new(&[
        ("pages/A.md", &page("a", "One.\n\nTwo.\n")),
        ("pages/B.md", &page("b", "Same.\n")),
    ]);
    // The tip took the second paragraph of A.
    fake.set("pages/A.md", &page("a", "One.\n\nZwei.\n"));
    // The revision edits A's first paragraph and B (back to what B already says at the tip is not a change).
    let content = revision(
        &[
            ("pages/A.md", Some(page("a", "Uno.\n\nTwo.\n"))),
            ("pages/B.md", Some(page("b", "Same.\n"))),
        ],
        &[],
    );
    let (files, renames, reauthored) = change(run(&fake, &content));
    assert!(reauthored && renames.is_empty());
    assert_eq!(
        files,
        vec![("pages/A.md".to_owned(), Some(page("a", "Uno.\n\nZwei.\n")))]
    );
}

#[test]
fn a_page_the_revision_removes_and_the_tip_left_alone_is_removed_and_a_page_the_tip_has_changed_is_an_overlap()
 {
    let fake = Fake::new(&[
        ("pages/A.md", &page("a", "x\n")),
        ("pages/B.md", &page("b", "y\n")),
    ]);
    fake.set("pages/C.md", &page("c", "z\n"));
    let content = revision(&[("pages/A.md", None)], &[]);
    let (files, _, reauthored) = change(run(&fake, &content));
    assert!(reauthored);
    assert_eq!(files, vec![("pages/A.md".to_owned(), None)]);
    // The tip edited what the revision removes: a person decides.
    fake.set("pages/A.md", &page("a", "x, edited\n"));
    match run(&fake, &content) {
        Prepared::Overlaps(texts) => {
            let one = texts.first().unwrap();
            assert_eq!(
                (one.page.as_str(), one.place.as_str()),
                ("pages/A.md", "page")
            );
            assert!(one.theirs.as_deref().unwrap().contains("x, edited"));
            assert_eq!(one.ours, None, "the revision removes it");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_rename_the_tip_can_still_carry_out_stays_a_rename_with_the_merged_text() {
    let fake = Fake::new(&[("pages/Old.md", &page("o", "One.\n\nTwo.\n"))]);
    fake.set("pages/Old.md", &page("o", "One.\n\nZwei.\n"));
    let content = revision(
        &[("pages/New.md", Some(page("o", "Uno.\n\nTwo.\n")))],
        &[("pages/Old.md", "pages/New.md")],
    );
    let (files, renames, reauthored) = change(run(&fake, &content));
    assert!(reauthored);
    assert_eq!(
        renames,
        vec![("pages/Old.md".to_owned(), "pages/New.md".to_owned())]
    );
    assert_eq!(
        files,
        vec![(
            "pages/New.md".to_owned(),
            Some(page("o", "Uno.\n\nZwei.\n"))
        )]
    );
}

#[test]
fn a_rename_alone_is_a_rename_and_writes_no_file() {
    let fake = Fake::new(&[
        ("pages/Old.md", &page("o", "One.\n")),
        ("pages/B.md", &page("b", "b\n")),
    ]);
    fake.set("pages/B.md", &page("b", "bb\n"));
    let content = revision(&[], &[("pages/Old.md", "pages/New.md")]);
    let (files, renames, _) = change(run(&fake, &content));
    assert!(files.is_empty());
    assert_eq!(
        renames,
        vec![("pages/Old.md".to_owned(), "pages/New.md".to_owned())]
    );
}

#[test]
fn a_tip_that_already_has_everything_is_nothing_to_change() {
    let fake = Fake::new(&[("pages/A.md", &page("a", "One.\n"))]);
    fake.set("pages/A.md", &page("a", "Uno.\n"));
    let content = revision(&[("pages/A.md", Some(page("a", "Uno.\n")))], &[]);
    assert!(matches!(run(&fake, &content), Prepared::NothingToChange));
}

#[test]
fn a_frontmatter_key_changed_on_both_sides_is_an_overlap_that_names_the_key() {
    let fake = Fake::new(&[("pages/A.md", "---\ntitle: A\nstatus: draft\n---\n\nx\n")]);
    fake.set("pages/A.md", "---\ntitle: A\nstatus: stable\n---\n\nx\n");
    let content = revision(
        &[(
            "pages/A.md",
            Some("---\ntitle: A\nstatus: review\n---\n\nx\n".to_owned()),
        )],
        &[],
    );
    match run(&fake, &content) {
        Prepared::Overlaps(texts) => {
            let one = texts.first().unwrap();
            assert_eq!(one.place, "frontmatter status");
            assert!(one.base.as_deref().unwrap().contains("draft"));
            assert!(one.theirs.as_deref().unwrap().contains("stable"));
            assert!(one.ours.as_deref().unwrap().contains("review"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_page_that_is_not_text_at_the_base_or_the_tip_is_unreadable_and_says_so() {
    let fake = Fake::new(&[("pages/A.md", "x")]);
    fake.set_bytes("pages/A.md", &[0xff, 0xfe, 0xfd]);
    let content = revision(&[("pages/A.md", Some("y".to_owned()))], &[]);
    assert!(matches!(
        prepare(&fake, Path::new("/r"), &content, &fake.block_id()),
        Err(PrepareError::Unreadable)
    ));
    fake.fail_read(Some(StorageError::Driver {
        verb: "cat",
        detail: "gone".to_owned(),
    }));
    assert!(matches!(
        prepare(&fake, Path::new("/r"), &content, &fake.block_id()),
        Err(PrepareError::Storage(_))
    ));
}

#[test]
fn the_accepted_bytes_are_every_path_and_every_text() {
    let files = vec![
        prepared_file("pages/A.md", Some("abc".to_owned())),
        prepared_file("pages/B.md", None),
    ];
    let renames = vec![(PathBuf::from("a"), PathBuf::from("bb"))];
    assert_eq!(change_bytes(&files, &renames), 10 + 3 + 10 + 3);
}

// --- CM-5

fn comment(page: &str, block_id: Option<&str>, quote: Option<&str>, ordinal: u32) -> CommentRecord {
    CommentRecord {
        id: CommentId::new(),
        proposal: inklkunn_store::ProposalId::new(),
        revision: 1,
        author: inklkunn_store::AccountId::new(),
        parent: None,
        body: "?".to_owned(),
        anchor: Some(Anchor {
            page: page.to_owned(),
            block_id: block_id.map(str::to_owned),
            quote: quote.map(|q| Quote {
                exact: q.to_owned(),
                prefix: String::new(),
                suffix: String::new(),
            }),
            ordinal,
        }),
        created_at: 1,
    }
}

fn file(path: &str, text: &str) -> PreparedFile {
    prepared_file(path, Some(text.to_owned()))
}

fn text_of(f: &PreparedFile) -> String {
    String::from_utf8(f.content.clone().unwrap()).unwrap()
}

#[test]
fn a_paragraph_that_a_comment_quotes_gets_its_id_at_the_end_of_its_last_line() {
    let before = page("a", "One.\n\nTwo\ntwo again.\n\nThree.\n");
    let mut files = vec![file("pages/A.md", &before)];
    let c = comment("pages/A.md", None, Some("two again"), 1);
    assert_eq!(attach_ids(&mut files, std::slice::from_ref(&c)), 1);
    let after = text_of(files.first().unwrap());
    let tail: String =
        c.id.as_str()
            .chars()
            .filter(|x| *x != '-')
            .rev()
            .take(8)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
    assert_eq!(
        after,
        before.replace("two again.", &format!("two again. ^c{tail}"))
    );
    // The blocks are the same blocks, and the one has its id.
    let (_, body, _) = inklkunn_markup::frontmatter::split(&after);
    let ids: Vec<_> = inklkunn_markup::blocks(body)
        .iter()
        .map(|b| b.id.map(str::to_owned))
        .collect();
    assert_eq!(ids, vec![None, Some(format!("c{tail}")), None]);
}

#[test]
fn a_table_and_a_code_block_get_a_line_of_their_own_and_a_heading_and_a_list_get_none() {
    let table = "| a | b |\n|---|---|\n| 1 | 2 |";
    let code = "```\nlet x = 1;\n```";
    let body = format!("# Title\n\n{table}\n\n{code}\n\n- item one\n- item two\n");
    let mut files = vec![file("pages/A.md", &page("a", &body))];
    let comments = [
        comment("pages/A.md", None, Some("| 1 | 2 |"), 1),
        comment("pages/A.md", None, Some("let x = 1;"), 2),
        comment("pages/A.md", None, Some("Title"), 0),
        comment("pages/A.md", None, Some("item two"), 3),
    ];
    assert_eq!(
        attach_ids(&mut files, &comments),
        2,
        "the table and the code block"
    );
    let after = text_of(files.first().unwrap());
    assert!(after.contains("| 1 | 2 |\n^c"), "{after}");
    assert!(after.contains("```\n^c"), "{after}");
    assert!(after.contains("# Title\n"), "{after}");
    assert!(after.ends_with("- item one\n- item two\n"), "{after}");
}

#[test]
fn a_comment_that_names_an_id_or_a_block_that_has_one_or_a_page_the_change_does_not_write_changes_nothing()
 {
    let with_id = page("a", "One. ^keep\n\nTwo.\n");
    let mut files = vec![file("pages/A.md", &with_id)];
    let comments = [
        comment("pages/A.md", Some("keep"), None, 0),
        comment("pages/A.md", None, Some("One."), 0),
        comment("pages/Elsewhere.md", None, Some("Two."), 0),
        CommentRecord {
            anchor: None,
            ..comment("pages/A.md", None, None, 0)
        },
        comment("pages/A.md", None, Some("not in the page"), 0),
    ];
    assert_eq!(attach_ids(&mut files, &comments), 0);
    assert_eq!(text_of(files.first().unwrap()), with_id);
    // A page the change removes has no text to give an id to.
    let mut gone = vec![prepared_file("pages/A.md", None)];
    assert_eq!(attach_ids(&mut gone, &comments), 0);
}

#[test]
fn a_quote_in_two_blocks_is_resolved_by_the_ordinal_and_left_alone_when_the_ordinal_does_not_decide()
 {
    let before = page("a", "Same words.\n\nSame words.\n");
    let mut files = vec![file("pages/A.md", &before)];
    assert_eq!(
        attach_ids(
            &mut files,
            &[comment("pages/A.md", None, Some("Same words"), 1)]
        ),
        1
    );
    let after = text_of(files.first().unwrap());
    let (_, body, _) = inklkunn_markup::frontmatter::split(&after);
    let ids: Vec<_> = inklkunn_markup::blocks(body)
        .iter()
        .map(|b| b.id.is_some())
        .collect();
    assert_eq!(ids, vec![false, true], "the second, as the ordinal says");
    let mut files = vec![file("pages/A.md", &before)];
    assert_eq!(
        attach_ids(
            &mut files,
            &[comment("pages/A.md", None, Some("Same words"), 7)]
        ),
        0,
        "no block is at ordinal 7"
    );
    assert_eq!(text_of(files.first().unwrap()), before);
}

#[test]
fn two_comments_on_one_block_give_it_one_id_and_ids_do_not_collide_with_the_pages() {
    let mut files = vec![file("pages/A.md", &page("a", "One. ^c1\n\nTwo.\n"))];
    let comments = [
        comment("pages/A.md", None, Some("Two."), 1),
        comment("pages/A.md", None, Some("Two."), 1),
    ];
    assert_eq!(attach_ids(&mut files, &comments), 1);
    let after = text_of(files.first().unwrap());
    assert_eq!(
        after.matches(" ^c").count(),
        2,
        "the old id and the one new id: {after}"
    );
}
