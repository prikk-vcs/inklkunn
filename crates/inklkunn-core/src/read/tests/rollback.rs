//! The inverse of a change (handoff 004 T6; RFC 004 D-8): the pages as they were before it, exactly, written
//! against the undone block; a rollback of a rollback restores the change; what cannot be undone says why.

use inklkunn_prikk::storage::PathRef;
use inklkunn_prikk::{Change, Operation, Patch};
use inklkunn_store::FileChange;

use super::super::testing::{World, block_id, page, state_id};
use super::*;

fn op(kind: &str, paths: &[&str]) -> Operation {
    Operation {
        kind: kind.to_owned(),
        paths: paths
            .iter()
            .map(|p| PathRef {
                path: Some((*p).to_owned()),
                unresolved_node_id: None,
            })
            .collect(),
        content: Default::default(),
    }
}

fn shows(ops: Vec<Operation>) -> inklkunn_prikk::Result<Change> {
    Ok(Change {
        schema_version: "show-report-v1".to_owned(),
        patches: vec![Patch {
            patch_id: "p".repeat(64),
            queued: false,
            operations: ops,
        }],
    })
}

fn text(title: &str, body: &str) -> String {
    page(&[&format!("title: {title}")], body)
}

fn pair(path: &str, text: String) -> (String, String) {
    (path.to_owned(), text)
}

/// Version 1: `Home` and `Guide`. Then one block per call below, in order.
fn world() -> World {
    let w = World::pages_with(
        &[("Home", &[], "Home v1.\n"), ("Guide", &[], "Guide v1.\n")],
        ttl_zero(),
    );
    w.fake().say(1, "Create the wiki");
    w.fake().show(
        1,
        shows(vec![
            op("create-file", &["pages/Home.md"]),
            op("create-file", &["pages/Guide.md"]),
        ]),
    );
    w
}

/// Seal the next block: write and remove files, with the message and the operations `show` will answer.
fn seal(
    w: &World,
    message: &str,
    write: &[(String, String)],
    remove: &[&str],
    ops: Vec<Operation>,
) -> u64 {
    w.fake().change_files(write, remove);
    describe(w, message, ops)
}

/// Say what the block the fake is at now is: its message and the operations `show` answers.
fn describe(w: &World, message: &str, ops: Vec<Operation>) -> u64 {
    let version = u64::from_str_radix(&w.fake().state_id(), 16).expect("a version");
    w.fake().say(version, message);
    w.fake().show(version, shows(ops));
    version
}

fn plan(w: &World, version: u64) -> RollbackOutcome {
    w.service.forget_wiki(&w.wiki);
    w.service
        .rollback_plan(&w.wiki, None, &block_id(version))
        .expect("a plan")
}

fn the_plan(outcome: RollbackOutcome) -> RollbackPlan {
    match outcome {
        RollbackOutcome::Plan(plan) => *plan,
        other => panic!("expected a plan, got {other:?}"),
    }
}

fn no(outcome: RollbackOutcome) -> NoRollback {
    match outcome {
        RollbackOutcome::No(why) => why,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn the_plan_of_an_edit_restores_the_text_before_it_exactly_and_names_the_undone_block() {
    let w = world();
    // Multibyte text, CRLF line ends and a missing final newline survive byte for byte.
    let before = text(
        "Home",
        "日本語の段落。\r\n\r\nSecond paragraph, no final newline",
    );
    w.fake().set("pages/Home.md", &before);
    describe(
        &w,
        "Reword the home page",
        vec![op("edit-text", &["pages/Home.md"])],
    );
    let v = seal(
        &w,
        "Rewrite the home page",
        &[pair("pages/Home.md", text("Home", "Rewritten.\n"))],
        &[],
        vec![op("edit-text", &["pages/Home.md"])],
    );

    let plan = the_plan(plan(&w, v));
    assert_eq!(
        plan.files,
        vec![FileChange {
            path: "pages/Home.md".to_owned(),
            text: Some(before),
        }]
    );
    assert_eq!(plan.undoes, block_id(v));
    assert_eq!(plan.number, v);
    assert_eq!(plan.undone_summary, "Rewrite the home page");
    assert_eq!(plan.summary, "Undo: Rewrite the home page");
    // Written against the undone block itself: its state and its id.
    assert_eq!(plan.base.block_id, block_id(v));
    assert_eq!(plan.base.state_id, state_id(v));
}

#[test]
fn the_plan_of_a_creation_removes_the_page_and_of_a_removal_writes_it_again() {
    let w = world();
    let notes = text("Notes", "Notes.\n");
    let created = seal(
        &w,
        "Add notes",
        &[pair("pages/Notes.md", notes.clone())],
        &[],
        vec![op("create-file", &["pages/Notes.md"])],
    );
    let removed = seal(
        &w,
        "Remove the guide",
        &[],
        &["pages/Guide.md"],
        vec![op("delete-node", &["pages/Guide.md"])],
    );

    assert_eq!(
        the_plan(plan(&w, created)).files,
        vec![FileChange {
            path: "pages/Notes.md".to_owned(),
            text: None
        }],
        "the page the change created is removed"
    );
    assert_eq!(
        the_plan(plan(&w, removed)).files,
        vec![FileChange {
            path: "pages/Guide.md".to_owned(),
            text: Some(text("Guide", "Guide v1.\n")),
        }],
        "the page the change removed is written again, as it was"
    );
}

#[test]
fn the_plan_of_a_move_puts_the_page_back_where_it_was() {
    let w = world();
    let moved = seal(
        &w,
        "Rename Home to Start",
        &[pair("pages/Start.md", text("Start", "Home v1.\n"))],
        &["pages/Home.md"],
        vec![op("rename-path", &["pages/Home.md", "pages/Start.md"])],
    );
    let files = the_plan(plan(&w, moved)).files;
    assert_eq!(
        files,
        vec![
            FileChange {
                path: "pages/Home.md".to_owned(),
                text: Some(text("Home", "Home v1.\n")),
            },
            FileChange {
                path: "pages/Start.md".to_owned(),
                text: None,
            },
        ],
        "the old path is written as it was and the new one removed, in path order"
    );
}

#[test]
fn a_rollback_of_a_rollback_restores_the_change() {
    let w = world();
    let changed = text("Home", "Home, changed.\n");
    let edit = seal(
        &w,
        "Change the home page",
        &[pair("pages/Home.md", changed.clone())],
        &[],
        vec![op("edit-text", &["pages/Home.md"])],
    );
    // The rollback is proposed, accepted and sealed: the pages say what the plan said.
    let undo = the_plan(plan(&w, edit));
    let restored = undo.files.first().and_then(|f| f.text.clone()).unwrap();
    assert_eq!(restored, text("Home", "Home v1.\n"));
    let rolled_back = seal(
        &w,
        "Undo: Change the home page",
        &[pair("pages/Home.md", restored)],
        &[],
        vec![op("edit-text", &["pages/Home.md"])],
    );
    // Undoing the rollback gives the change back, exactly.
    let again = the_plan(plan(&w, rolled_back));
    assert_eq!(
        again.files,
        vec![FileChange {
            path: "pages/Home.md".to_owned(),
            text: Some(changed),
        }]
    );
    assert_eq!(again.undoes, block_id(rolled_back));
    assert_eq!(again.summary, "Undo: Undo: Change the home page");
}

#[test]
fn a_change_that_cannot_be_undone_says_why() {
    let w = world();
    // The first change of a view has nothing before it.
    assert_eq!(no(plan(&w, 1)), NoRollback::FirstChange);
    // A block the history does not have.
    w.service.forget_wiki(&w.wiki);
    assert_eq!(
        no(w.service
            .rollback_plan(&w.wiki, None, &block_id(99))
            .unwrap()),
        NoRollback::NoSuchChange
    );
    assert_eq!(
        no(w.service
            .rollback_plan(&w.wiki, None, "not-a-block")
            .unwrap()),
        NoRollback::NoSuchChange
    );
    // A file that is not a page.
    let attachment = seal(
        &w,
        "Add a picture",
        &[pair("files/logo.png", "png".to_owned())],
        &[],
        vec![op("create-file", &["files/logo.png"])],
    );
    assert_eq!(no(plan(&w, attachment)), NoRollback::NotOnlyPages);
    // An operation that is not an edit of text, a creation, a removal or a move.
    let linked = seal(
        &w,
        "Change a mode",
        &[pair("pages/Home.md", text("Home", "Mode.\n"))],
        &[],
        vec![op("set-mode", &["pages/Home.md"])],
    );
    assert_eq!(no(plan(&w, linked)), NoRollback::NotOnlyPages);
    // A path prikk could not name.
    let unnamed = seal(
        &w,
        "Unnamed",
        &[pair("pages/Home.md", text("Home", "Unnamed.\n"))],
        &[],
        vec![Operation {
            kind: "edit-text".to_owned(),
            paths: vec![PathRef {
                path: None,
                unresolved_node_id: Some("node".to_owned()),
            }],
            content: Default::default(),
        }],
    );
    assert_eq!(no(plan(&w, unnamed)), NoRollback::NotOnlyPages);
    // A page that is not text.
    w.fake().set_bytes("pages/Home.md", &[0xff, 0xfe, 0x00]);
    let binary = describe(&w, "Binary", vec![op("edit-text", &["pages/Home.md"])]);
    assert_eq!(no(plan(&w, binary)), NoRollback::NotText);
}

#[test]
fn a_change_that_left_the_pages_as_they_were_has_nothing_to_undo() {
    let w = world();
    let same = seal(
        &w,
        "Touch the home page",
        &[pair("pages/Home.md", text("Home", "Home v1.\n"))],
        &[],
        vec![op("edit-text", &["pages/Home.md"])],
    );
    assert_eq!(no(plan(&w, same)), NoRollback::NothingToUndo);
}

#[test]
fn a_change_that_touched_too_many_pages_is_left_to_a_person() {
    let w = world();
    let many: Vec<(String, String)> = (0..=MAX_ROLLBACK_PAGES)
        .map(|i| pair(&format!("pages/P{i}.md"), text(&format!("P{i}"), "x\n")))
        .collect();
    let paths: Vec<String> = many.iter().map(|(p, _)| p.clone()).collect();
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let big = seal(
        &w,
        "Add a great many",
        &many,
        &[],
        refs.iter().map(|p| op("create-file", &[p])).collect(),
    );
    assert_eq!(no(plan(&w, big)), NoRollback::TooLarge);
}

#[test]
fn a_long_summary_is_cut_to_a_line_the_message_allows() {
    let w = world();
    let long = "A".repeat(300);
    let v = seal(
        &w,
        &long,
        &[pair("pages/Home.md", text("Home", "Longer.\n"))],
        &[],
        vec![op("edit-text", &["pages/Home.md"])],
    );
    let plan = the_plan(plan(&w, v));
    assert_eq!(plan.summary.chars().count(), 120);
    assert!(plan.summary.starts_with("Undo: AAAA") && plan.summary.ends_with('…'));
    assert!(crate::message::Summary::new(&plan.summary).is_ok());
}
