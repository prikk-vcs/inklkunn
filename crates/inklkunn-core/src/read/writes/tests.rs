//! Who wrote a path at a block (RFC 006 D-8; handoff G-T4, corrections J-1, J-2, J-5): the rule, the rows a block's operations come to,
//! the index's cost, and what is hidden when nothing can be settled. The repository is the read tests' double; the moderation
//! records are the memory store's, written directly (the acts that write them are the moderation service's own tests).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use inklkunn_prikk::storage::PathRef;
use inklkunn_prikk::{Change, Operation, Patch};
use inklkunn_store::{
    Account, AccountId, AccountStore, Actor, DerivedStore, DisplayName, GovernanceStore,
    HiddenItem, HiddenItemKind, HiddenParts, ReasonCategory, Role, TombstoneAct, TombstoneActId,
    TombstoneEvent, TombstoneEventId,
};

use super::super::testing::{World, block_id};
use super::*;
use crate::read::{BlockText, ReadConfig};
use crate::tombstone::Tombstones;

const SECRET: &str = "---\ntitle: Foo\n---\n\nZEBRA personal data.\n";

fn foo(body: &str) -> String {
    format!("---\ntitle: Foo\n---\n\n{body}\n")
}

fn bar(body: &str) -> String {
    format!("---\ntitle: Bar\n---\n\n{body}\n")
}

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

/// A world whose blocks show what they changed, with a read service that asks the memory store what moderation hides.
struct Rig {
    w: World,
    service: ReadService,
}

fn rig(files: &[(&str, &str)]) -> Rig {
    let w = World::with(files, config());
    w.fake().derive_changes();
    w.store
        .add_account(&Account {
            id: moderator().account,
            display_name: DisplayName::new("A moderator").unwrap(),
            local: None,
            created_at: 0,
            disabled: false,
        })
        .unwrap();
    let service = service_of(&w);
    Rig { w, service }
}

fn config() -> ReadConfig {
    ReadConfig {
        tip_ttl: Duration::from_secs(3600),
        ..ReadConfig::default()
    }
}

fn service_of(w: &World) -> ReadService {
    World::service_over(&w.counting, &w.store, &w.layout, config())
        .with_tombstones(Arc::new(Tombstones::new(w.store.clone(), w.store.clone())))
}

impl Rig {
    /// The moderator hid the content of the change numbered `n`.
    fn hide(&self, n: u64) -> TombstoneEventId {
        let id = TombstoneEventId::new();
        self.w
            .store
            .record_tombstone_event(&TombstoneEvent {
                id: id.clone(),
                act_id: TombstoneActId::new(),
                wiki: self.w.wiki.id.clone(),
                at: 1_700_000_000,
                act: TombstoneAct::Hide,
                item: HiddenItem {
                    kind: HiddenItemKind::PageRevision,
                    id: block_id(n),
                    reference: None,
                },
                parts: CONTENT,
                category: Some(ReasonCategory::PersonalData),
                note: None,
                affected: None,
                by: moderator(),
                policy_block: None,
                of_event: None,
                appeal: None,
                declined_transition: None,
            })
            .unwrap();
        id
    }

    /// The moderator restored what the hide `hide` hid.
    fn unhide(&self, hide: &TombstoneEventId, n: u64) {
        self.w
            .store
            .record_tombstone_event(&TombstoneEvent {
                id: TombstoneEventId::new(),
                act_id: TombstoneActId::new(),
                wiki: self.w.wiki.id.clone(),
                at: 1_700_000_100,
                act: TombstoneAct::Unhide,
                item: HiddenItem {
                    kind: HiddenItemKind::PageRevision,
                    id: block_id(n),
                    reference: None,
                },
                parts: CONTENT,
                category: None,
                note: None,
                affected: None,
                by: moderator(),
                policy_block: None,
                of_event: Some(hide.clone()),
                appeal: None,
                declined_transition: None,
            })
            .unwrap();
    }

    fn text(&self, n: u64, path: &str) -> BlockText {
        self.service
            .text_at(&self.w.wiki, None, &block_id(n), path)
            .expect("a read")
    }

    fn is_hidden(&self, n: u64, path: &str) -> bool {
        matches!(self.text(n, path), BlockText::Hidden(_))
    }

    fn shows(&self, n: u64, path: &str) -> bool {
        matches!(self.text(n, path), BlockText::Text(_))
    }

    /// Build the whole index (a long history takes a few calls, each within its bound).
    fn warm(&self) {
        for _ in 0..200 {
            if self.service.warm_writes(&self.w.wiki).expect("a build") {
                return;
            }
        }
        panic!("the index was not built");
    }
}

const CONTENT: HiddenParts = HiddenParts {
    content: true,
    summary: false,
    author: false,
};

/// The moderator: an account the store knows, acting at the admin rung.
fn moderator() -> Actor {
    thread_local! {
        static MODERATOR: AccountId = AccountId::new();
    }
    Actor {
        account: MODERATOR.with(Clone::clone),
        rung: Role::Admin,
    }
}

// ------------------------------------------------------------------------------------ what a block did

fn touch(kind: TouchKind, path: &str, patch: usize) -> Touch {
    Touch {
        kind,
        path: path.to_owned(),
        patch,
    }
}

fn rows(touches: Vec<Touch>) -> Vec<(String, WriteKind)> {
    rows_of(&Touches::Known(Arc::new(touches)), false)
        .into_iter()
        .map(|w| (w.path, w.kind))
        .collect()
}

fn carried(from: &str) -> WriteKind {
    WriteKind::Carried {
        from: from.to_owned(),
    }
}

#[test]
fn a_creation_an_edit_and_a_removal_are_what_a_block_wrote_and_removed() {
    let got = rows(vec![
        touch(TouchKind::Create, "a", 0),
        touch(TouchKind::Edit, "b", 0),
        touch(TouchKind::Delete, "c", 0),
    ]);
    assert_eq!(
        got,
        vec![
            ("a".to_owned(), WriteKind::Wrote),
            ("b".to_owned(), WriteKind::Wrote),
            ("c".to_owned(), WriteKind::Deleted),
        ]
    );
}

#[test]
fn a_rename_that_touches_no_bytes_carries_the_writer_and_a_rename_with_an_edit_is_a_write() {
    let rename = |from: &str| TouchKind::Rename {
        from: from.to_owned(),
    };
    // Moved, nothing edited: carried from the old path, and the old path is gone.
    assert_eq!(
        rows(vec![touch(rename("a"), "b", 0)]),
        vec![
            ("a".to_owned(), WriteKind::Deleted),
            ("b".to_owned(), carried("a"))
        ]
    );
    // Moved and edited in the same patch (an edit names the path its node has when the patch is done): written.
    assert_eq!(
        rows(vec![
            touch(TouchKind::Edit, "b", 0),
            touch(rename("a"), "b", 0)
        ]),
        vec![
            ("a".to_owned(), WriteKind::Deleted),
            ("b".to_owned(), WriteKind::Wrote)
        ]
    );
    // Edited in a patch after the one that moved it: written too.
    assert_eq!(
        rows(vec![
            touch(rename("a"), "b", 0),
            touch(TouchKind::Edit, "b", 1)
        ]),
        vec![
            ("a".to_owned(), WriteKind::Deleted),
            ("b".to_owned(), WriteKind::Wrote)
        ]
    );
    // Moved twice in a block: carried from where it began, and neither of the paths it passed is left.
    assert_eq!(
        rows(vec![touch(rename("a"), "b", 0), touch(rename("b"), "c", 0)]),
        vec![
            ("a".to_owned(), WriteKind::Deleted),
            ("b".to_owned(), WriteKind::Deleted),
            ("c".to_owned(), carried("a"))
        ]
    );
    // A file made in this block and moved in it is this block's.
    assert_eq!(
        rows(vec![
            touch(TouchKind::Create, "a", 0),
            touch(rename("a"), "b", 0)
        ]),
        vec![
            ("a".to_owned(), WriteKind::Deleted),
            ("b".to_owned(), WriteKind::Wrote)
        ]
    );
}

#[test]
fn a_path_removed_and_made_again_in_a_block_is_written_and_a_change_of_mode_is_nothing() {
    assert_eq!(
        rows(vec![
            touch(TouchKind::Delete, "a", 0),
            touch(TouchKind::Create, "a", 0),
            touch(TouchKind::Other, "b", 0),
        ]),
        vec![("a".to_owned(), WriteKind::Wrote)]
    );
}

#[test]
fn an_operation_that_cannot_be_read_and_a_block_too_large_to_list_may_have_written_anything_and_a_merge_cannot_be_attributed()
 {
    let got = rows(vec![
        touch(TouchKind::Edit, "a", 0),
        touch(TouchKind::Unreadable, "", 0),
    ]);
    assert_eq!(
        got,
        vec![
            ("a".to_owned(), WriteKind::Wrote),
            (String::new(), WriteKind::Maybe)
        ]
    );
    let too_large: Vec<_> = rows_of(&Touches::TooLarge, false)
        .into_iter()
        .map(|w| (w.path, w.kind))
        .collect();
    assert_eq!(too_large, vec![(String::new(), WriteKind::Maybe)]);
    let opaque: Vec<_> = rows_of(
        &Touches::Known(Arc::new(vec![touch(TouchKind::Edit, "a", 0)])),
        true,
    )
    .into_iter()
    .map(|w| (w.path, w.kind))
    .collect();
    assert_eq!(opaque, vec![(String::new(), WriteKind::Opaque)]);
}

// ------------------------------------------------------------------------------------ the rule

/// B (2) writes the personal data to `Foo`, C (3) edits `Bar` only, D (4) takes the data out of `Foo`.
fn story() -> Rig {
    let r = rig(&[
        ("pages/Foo.md", &foo("Foo as first written.")),
        ("pages/Bar.md", &bar("Bar as first written.")),
    ]);
    r.w.fake().set("pages/Foo.md", SECRET);
    r.w.fake().set("pages/Bar.md", &bar("Bar, edited."));
    r.w.fake()
        .set("pages/Foo.md", &foo("Foo without the data."));
    r
}

#[test]
fn the_text_is_hidden_exactly_where_the_change_that_last_wrote_it_is_hidden() {
    let r = story();
    // Nothing is hidden yet: every block's text reads, and the index is not even built.
    let (_, calls) = r.w.counted(|| assert!(r.shows(3, "pages/Foo.md")));
    assert_eq!(
        calls.change, 0,
        "nothing hidden, nothing indexed: {calls:?}"
    );
    let hide = r.hide(2);
    // B and C show what B wrote, so both are hidden; the page as first written, the page after D, and C's own page are not.
    assert!(r.is_hidden(2, "pages/Foo.md"));
    assert!(
        r.is_hidden(3, "pages/Foo.md"),
        "C left the page as B wrote it"
    );
    assert!(r.shows(1, "pages/Foo.md"));
    assert!(r.shows(4, "pages/Foo.md"));
    assert!(r.shows(3, "pages/Bar.md"));
    // `hidden_text` says the same, with the mark of B's hiding.
    let mark = r
        .service
        .hidden_text(&r.w.wiki, None, &block_id(3), "pages/Foo.md")
        .unwrap()
        .expect("hidden");
    assert_eq!(mark.event, hide);
    // An unhide is in force at once, and a hide again.
    r.unhide(&hide, 2);
    assert!(r.shows(3, "pages/Foo.md"));
    assert!(r.shows(2, "pages/Foo.md"));
    r.hide(2);
    assert!(r.is_hidden(3, "pages/Foo.md"));
}

#[test]
fn bytes_that_another_change_wrote_independently_are_not_hidden() {
    let r = rig(&[
        ("pages/Foo.md", &foo("Foo as first written.")),
        ("pages/Bar.md", &bar("Bar as first written.")),
    ]);
    // B writes the data to Foo; C writes the very same bytes to Bar; D rewrites Foo without it; E writes the data to Foo again.
    r.w.fake().set("pages/Foo.md", SECRET);
    r.w.fake().set("pages/Bar.md", SECRET);
    r.w.fake()
        .set("pages/Foo.md", &foo("Foo without the data."));
    r.w.fake().set("pages/Foo.md", SECRET);
    r.hide(2);
    assert!(r.is_hidden(2, "pages/Foo.md"));
    assert!(
        r.shows(3, "pages/Bar.md"),
        "the same bytes at another path, written by another change, are that change's"
    );
    assert!(
        r.shows(5, "pages/Foo.md"),
        "the same bytes written again by a later change are that change's"
    );
    // And a hide of C hides C's page and not B's.
    let r2 = rig(&[("pages/Foo.md", &foo("x")), ("pages/Bar.md", &bar("y"))]);
    r2.w.fake().set("pages/Foo.md", SECRET);
    r2.w.fake().set("pages/Bar.md", SECRET);
    r2.hide(3);
    assert!(r2.shows(2, "pages/Foo.md"));
    assert!(r2.is_hidden(3, "pages/Bar.md"));
}

#[test]
fn a_change_that_wrote_twenty_five_pages_hides_the_twenty_fifth_and_every_one_of_them_after_it() {
    let r = rig(&[("pages/Home.md", &page_of("Home", "Hi."))]);
    let files: Vec<(String, String)> = (0..25)
        .map(|i| (format!("pages/P{i}.md"), page_of(&format!("P{i}"), SECRET)))
        .collect();
    r.w.fake().set_files(&files);
    r.w.fake()
        .set("pages/Home.md", &page_of("Home", "Hi again."));
    r.hide(2);
    for i in 0..25 {
        assert!(r.is_hidden(2, &format!("pages/P{i}.md")), "P{i} at B");
        assert!(r.is_hidden(3, &format!("pages/P{i}.md")), "P{i} at C");
    }
    assert!(r.shows(3, "pages/Home.md"));
}

fn page_of(title: &str, body: &str) -> String {
    format!("---\ntitle: {title}\n---\n\n{body}\n")
}

#[test]
fn a_path_that_was_removed_and_written_again_is_the_later_changes_whatever_the_bytes() {
    let r = rig(&[("pages/Foo.md", &foo("x")), ("pages/Bar.md", &bar("y"))]);
    r.w.fake().set("pages/Foo.md", SECRET); // B (2)
    r.w.fake().change_files(&[], &["pages/Foo.md"]); // C (3) removes it
    r.w.fake().set("pages/Foo.md", SECRET); // D (4) writes the same bytes again
    r.hide(2);
    assert!(r.is_hidden(2, "pages/Foo.md"));
    assert!(matches!(r.text(3, "pages/Foo.md"), BlockText::Missing));
    assert!(r.shows(4, "pages/Foo.md"));
}

#[test]
fn a_page_moved_without_touching_its_bytes_is_still_its_writers_and_a_move_with_an_edit_is_the_movers()
 {
    for (edited, hidden_after_move) in [(false, true), (true, false)] {
        let r = rig(&[("pages/Foo.md", &foo("x")), ("pages/Bar.md", &bar("y"))]);
        r.w.fake().set("pages/Foo.md", SECRET); // B (2)
        // C (3) moves it.
        let moved = if edited {
            page_of("Moved", "Edited on the way.")
        } else {
            SECRET.to_owned()
        };
        r.w.fake()
            .change_files(&[("pages/Moved.md".to_owned(), moved)], &["pages/Foo.md"]);
        let mut ops = vec![op("rename-path", &["pages/Foo.md", "pages/Moved.md"])];
        if edited {
            ops.push(op("edit-text", &["pages/Moved.md"]));
        }
        r.w.fake().show(3, shows(ops));
        r.hide(2);
        assert_eq!(
            r.is_hidden(3, "pages/Moved.md"),
            hidden_after_move,
            "edited: {edited}"
        );
        // The path it left has nothing at C, and at B it is B's.
        assert!(matches!(r.text(3, "pages/Foo.md"), BlockText::Missing));
        assert!(r.is_hidden(2, "pages/Foo.md"));
        // A change after the move that edits the moved page makes it that change's.
        r.w.fake()
            .set("pages/Moved.md", &page_of("Moved", "Later."));
        // The view moved: the service is told, as a write does.
        r.service.forget_wiki(&r.w.wiki);
        assert!(r.shows(4, "pages/Moved.md"));
    }
}

#[test]
fn a_page_moved_without_touching_its_bytes_is_shown_when_the_change_that_wrote_it_is_not_hidden() {
    let r = rig(&[("pages/Foo.md", &foo("x")), ("pages/Bar.md", &bar("y"))]);
    r.w.fake().set("pages/Foo.md", &foo("Foo, written by B.")); // B (2)
    r.w.fake().set("pages/Bar.md", SECRET); // H (3), the one that is hidden
    // C (4) moves Foo, nothing of its bytes touched, and moves it again (the chain is followed to B).
    r.w.fake().change_files(
        &[("pages/Moved.md".to_owned(), foo("Foo, written by B."))],
        &["pages/Foo.md"],
    );
    r.w.fake().show(
        4,
        shows(vec![op("rename-path", &["pages/Foo.md", "pages/Moved.md"])]),
    );
    r.w.fake().change_files(
        &[("pages/Moved Again.md".to_owned(), foo("Foo, written by B."))],
        &["pages/Moved.md"],
    );
    r.w.fake().show(
        5,
        shows(vec![op(
            "rename-path",
            &["pages/Moved.md", "pages/Moved Again.md"],
        )]),
    );
    r.hide(3);
    assert!(r.is_hidden(3, "pages/Bar.md"));
    assert!(r.shows(4, "pages/Moved.md"), "B's, and B is not hidden");
    assert!(r.shows(5, "pages/Moved Again.md"), "B's, two moves on");
    // Hide B: the page is hidden at every place it was moved to.
    r.hide(2);
    assert!(r.is_hidden(4, "pages/Moved.md"));
    assert!(r.is_hidden(5, "pages/Moved Again.md"));
    assert!(r.is_hidden(2, "pages/Foo.md"));
}

#[test]
fn a_change_that_may_have_written_a_path_counts_with_the_one_before_it() {
    let r = rig(&[("pages/Foo.md", &foo("x")), ("pages/Bar.md", &bar("y"))]);
    r.w.fake().set("pages/Foo.md", SECRET); // B (2)
    r.w.fake().set("pages/Bar.md", &bar("z")); // C (3): one operation could not be read
    r.w.fake().set("pages/Bar.md", &bar("w")); // D (4)
    r.w.fake().show(
        3,
        shows(vec![
            op("edit-text", &["pages/Bar.md"]),
            Operation {
                kind: "edit-text".to_owned(),
                paths: vec![PathRef {
                    path: None,
                    unresolved_node_id: Some("node".to_owned()),
                }],
                content: Default::default(),
            },
        ]),
    );
    // B hidden: Foo at C may be C's or B's, so it is hidden; Bar at C is C's own write.
    let b = r.hide(2);
    assert!(r.is_hidden(3, "pages/Foo.md"));
    assert!(r.is_hidden(4, "pages/Foo.md"));
    assert!(r.shows(3, "pages/Bar.md"));
    // B restored and C hidden: C may have written any path, so Foo at C and after it is hidden, and Foo at B is not.
    r.unhide(&b, 2);
    assert!(r.shows(3, "pages/Foo.md"));
    r.hide(3);
    assert!(r.is_hidden(3, "pages/Foo.md"));
    assert!(r.is_hidden(4, "pages/Foo.md"));
    assert!(r.shows(2, "pages/Foo.md"));
}

#[test]
fn a_text_nobody_is_recorded_as_having_written_is_hidden_while_anything_is() {
    let r = rig(&[("pages/Foo.md", &foo("x")), ("pages/Bar.md", &bar("y"))]);
    // The first block's operations name only `Bar`: nobody wrote `Foo` as far as the record goes.
    r.w.fake()
        .show(1, shows(vec![op("create-file", &["pages/Bar.md"])]));
    r.w.fake().set("pages/Bar.md", &bar("z")); // B (2)
    assert!(
        r.shows(1, "pages/Foo.md"),
        "nothing is hidden: nothing is asked"
    );
    r.hide(2);
    assert!(r.is_hidden(1, "pages/Foo.md"), "a text with no writer");
    assert!(r.is_hidden(2, "pages/Foo.md"));
    assert!(r.shows(1, "pages/Bar.md"));
    // `hidden_text` agrees (it asks the repository whether anything is there).
    assert!(
        r.service
            .hidden_text(&r.w.wiki, None, &block_id(2), "pages/Foo.md")
            .unwrap()
            .is_some()
    );
    // A path nothing is at is not hidden.
    assert!(matches!(r.text(2, "pages/Nothing.md"), BlockText::Missing));
}

#[test]
fn a_block_whose_effect_is_not_listed_cannot_be_passed_by_a_text_while_anything_is_hidden() {
    use inklkunn_store::{BlockWrites, PathWrite, WritesState};
    let r = rig(&[("pages/Foo.md", &foo("x")), ("pages/Bar.md", &bar("y"))]);
    r.w.fake().set("pages/Bar.md", &bar("z")); // 2: the log says it is a merge (the index is made by hand)
    r.w.fake().set("pages/Bar.md", &bar("w")); // 3
    let row = |path: &str, kind: WriteKind| PathWrite {
        path: path.to_owned(),
        kind,
    };
    let block = |n: u64, writes: Vec<PathWrite>| BlockWrites {
        number: n,
        block: block_id(n),
        proposal: None,
        writes,
    };
    r.w.store
        .append_writes(
            &r.w.wiki.id,
            &r.w.wiki.default_view,
            &[
                block(
                    1,
                    vec![
                        row("pages/Foo.md", WriteKind::Wrote),
                        row("pages/Bar.md", WriteKind::Wrote),
                    ],
                ),
                block(2, vec![row("", WriteKind::Opaque)]),
                block(3, vec![row("pages/Bar.md", WriteKind::Wrote)]),
            ],
            &WritesState {
                head_number: 3,
                head_block: block_id(3),
                floor_number: 1,
                genesis: true,
            },
        )
        .unwrap();
    // Nothing is hidden: nothing is asked of the index.
    assert!(r.shows(2, "pages/Foo.md"));
    r.hide(3);
    // The text of Foo is block 1's, which is not hidden, but block 2 cannot be passed: nobody can say it did not write Foo.
    assert!(r.shows(1, "pages/Foo.md"));
    assert!(r.is_hidden(2, "pages/Foo.md"));
    assert!(r.is_hidden(3, "pages/Foo.md"));
    // Bar at 3 is block 3's own, which is the hidden one.
    assert!(r.is_hidden(3, "pages/Bar.md"), "block 3 is the hidden one");
}

#[test]
fn a_block_that_is_in_no_view_is_unattributed_and_so_hidden_while_anything_is() {
    let r = story();
    assert!(matches!(
        r.service
            .text_at(&r.w.wiki, None, &"c".repeat(64), "pages/Foo.md")
            .unwrap(),
        BlockText::Missing
    ));
    r.hide(2);
    let got = r
        .service
        .text_at(&r.w.wiki, None, &"c".repeat(64), "pages/Foo.md")
        .unwrap();
    assert!(matches!(got, BlockText::Hidden(_)), "{got:?}");
}

// ------------------------------------------------------------------------------------ the index

#[test]
fn the_index_is_built_again_after_the_derived_store_is_cleared() {
    let r = story();
    r.hide(2);
    r.warm();
    let state = || {
        r.w.store
            .writes_state(&r.w.wiki.id, &r.w.wiki.default_view)
            .unwrap()
    };
    assert_eq!(state().expect("built").head_number, 4);
    assert!(r.is_hidden(3, "pages/Foo.md"));
    // Everything derived is dropped, the index with it.
    r.service.clear_derived(&r.w.wiki).unwrap();
    assert!(state().is_none(), "the index went with the rest");
    assert!(
        r.w.store
            .indexed_block(&r.w.wiki.id, &r.w.wiki.default_view, &block_id(3))
            .unwrap()
            .is_none()
    );
    // The next reads build it again from the repository's own record of each block, and answer as before.
    assert!(r.is_hidden(3, "pages/Foo.md"));
    assert!(r.shows(4, "pages/Foo.md"));
    assert!(r.shows(1, "pages/Foo.md"));
    assert_eq!(state().expect("built again").head_number, 4);
}

#[test]
fn a_first_read_of_a_long_history_builds_what_it_may_says_hidden_for_what_it_could_not_reach_and_never_fails()
 {
    let r = rig(&[("pages/Foo.md", &foo("x")), ("pages/Bar.md", &bar("y"))]);
    for i in 0..150 {
        r.w.fake()
            .set("pages/Bar.md", &bar(&format!("revision {i}")));
    }
    r.w.fake().set("pages/Foo.md", SECRET); // 152
    r.hide(152);
    // Block 100 is further than one call builds to: its text is not attributed yet, and hidden, not an error.
    let (first, calls) = r.w.counted(|| r.text(100, "pages/Bar.md"));
    assert!(matches!(first, BlockText::Hidden(_)), "{first:?}");
    assert!(
        calls.change <= u64::try_from(MAX_INDEXED_SHOWS).unwrap(),
        "one call builds at most {MAX_INDEXED_SHOWS} blocks: {calls:?}"
    );
    // Each call goes on from where the last stopped; a few calls reach it, and then the text reads.
    let mut calls_needed = 1;
    loop {
        calls_needed += 1;
        if r.shows(100, "pages/Bar.md") {
            break;
        }
        assert!(calls_needed <= 4, "the build does not go on");
    }
    assert_eq!(calls_needed, 2, "the second call reaches block 100");
    assert!(r.is_hidden(152, "pages/Foo.md"));
}

#[test]
fn the_cost_of_a_read_is_the_same_with_no_change_hidden_with_ten_and_with_a_thousand() {
    let mut costs = Vec::new();
    for hidden in [0_u64, 10, 1000] {
        let r = rig(&[("pages/Foo.md", &foo("x")), ("pages/Bar.md", &bar("y"))]);
        // A thousand and more changes of other pages, and a change of Foo in the middle.
        for i in 0..1010 {
            r.w.fake()
                .set("pages/Bar.md", &bar(&format!("revision {i}")));
            if i == 500 {
                r.w.fake().set("pages/Foo.md", &foo("the middle"));
            }
        }
        // The changes that are hidden are changes of Bar (the first `hidden` of them, after block 1).
        for k in 0..hidden {
            r.hide(if k <= 500 { 2 + k } else { 3 + k });
        }
        r.warm();
        let read = || r.text(900, "pages/Foo.md");
        let (got, calls) = r.w.counted(read);
        // Foo at 900 is the middle change's, which is not among the hidden ones.
        assert!(
            matches!(got, BlockText::Text(_)),
            "{hidden} hidden: {got:?}"
        );
        costs.push(calls);
        // A hide or an unhide of another change changes nothing about the cost.
        let again = r.hide(1010);
        let (_, after) = r.w.counted(read);
        assert_eq!(after, calls, "{hidden} hidden, then one more");
        r.unhide(&again, 1010);
    }
    let [none, ten, thousand]: [crate::read::Calls; 3] = costs.try_into().expect("three");
    assert_eq!(none, ten, "0 and 10 hidden");
    assert_eq!(ten, thousand, "10 and 1000 hidden");
    assert_eq!(none.read, 1, "one `cat`: {none:?}");
    assert_eq!(none.change, 0);
    assert_eq!(none.history, 0);
    assert_eq!(none.materialize, 0);
}

#[test]
fn the_cost_of_a_read_is_the_same_on_a_page_with_three_revisions_and_with_three_thousand() {
    let mut costs = Vec::new();
    for revisions in [3_u64, 3000] {
        let r = rig(&[("pages/Foo.md", &foo("x")), ("pages/Bar.md", &bar("y"))]);
        for i in 0..revisions {
            r.w.fake()
                .set("pages/Foo.md", &foo(&format!("revision {i}")));
        }
        r.hide(2);
        r.warm();
        let at = revisions; // the one before the last
        let (got, calls) = r.w.counted(|| r.text(at, "pages/Foo.md"));
        assert!(matches!(got, BlockText::Text(_)), "{revisions}: {got:?}");
        costs.push(calls);
    }
    let [few, many]: [crate::read::Calls; 2] = costs.try_into().expect("two");
    assert_eq!(few, many);
    assert_eq!(few.read, 1);
    assert_eq!(few.change + few.history + few.materialize, 0);
}

#[test]
fn the_later_changes_that_wrote_a_path_are_one_lookup_away() {
    let r = rig(&[("pages/Foo.md", &foo("x")), ("pages/Bar.md", &bar("y"))]);
    r.w.fake().set("pages/Foo.md", SECRET); // 2
    r.w.fake().set("pages/Bar.md", &bar("z")); // 3
    r.w.fake().set("pages/Foo.md", &foo("again")); // 4
    r.w.fake().set("pages/Foo.md", &foo("and again")); // 5
    r.hide(2);
    r.warm();
    let (got, calls) = r.w.counted(|| {
        r.service
            .later_writers(&r.w.wiki, None, &block_id(2), "pages/Foo.md", 1)
            .unwrap()
    });
    assert!(got.settled);
    assert_eq!(
        got.writers
            .iter()
            .map(|w| w.block.clone())
            .collect::<Vec<_>>(),
        vec![block_id(4)]
    );
    assert!(got.more, "one more than the limit");
    assert_eq!(calls.total(), 0, "a lookup in the index: {calls:?}");
    let all = r
        .service
        .later_writers(&r.w.wiki, None, &block_id(2), "pages/Foo.md", 10)
        .unwrap();
    assert_eq!(all.writers.len(), 2);
    assert!(!all.more);
    // A block the index does not know says nothing.
    let none = r
        .service
        .later_writers(&r.w.wiki, None, &"d".repeat(64), "pages/Foo.md", 10)
        .unwrap();
    assert!(!none.settled);
}
