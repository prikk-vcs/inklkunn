//! The writes index on both stores (RFC 006 D-8; handoff G-T4, correction J-2): who wrote each path of a view's log, in the order
//! the view numbers its blocks, answered with a few indexed lookups. The same invariants on the memory store and on SQLite, what
//! only SQLite can show (reopen, a row this build cannot read), and that the derived store's `clear` drops it.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use inklkunn_store::{
    Appended, BlockWrites, DerivedStore, DisplayName, MAX_ABSENT_BLOCKS, MAX_MAYBE_WRITERS,
    MaybeWriter, MemoryStore, NewWiki, PathWrite, Slug, SqliteStore, WikiId, WikiStore, WriteKind,
    WritesState,
};

const VIEW: &str = "heads/main";

/// What the tests that build an index by hand do: append under the view's generation now, and say that it was appended.
trait AppendNow: DerivedStore {
    fn append_now(
        &self,
        wiki: &WikiId,
        view: &str,
        blocks: &[BlockWrites],
        state: &WritesState,
    ) -> inklkunn_store::Result<()> {
        let generation = self.writes_generation(wiki, view)?;
        let outcome = self.append_writes(wiki, view, blocks, state, generation)?;
        assert_eq!(outcome, Appended::Done);
        Ok(())
    }
}

impl<T: DerivedStore + ?Sized> AppendNow for T {}

fn wiki(store: &dyn WikiStore, slug: &str) -> WikiId {
    store
        .create(NewWiki::new(
            Slug::new(slug).expect("slug"),
            DisplayName::new(slug).expect("name"),
        ))
        .expect("create")
        .id
}

fn block_of(n: u64) -> String {
    format!("{n:064x}")
}

fn write(path: &str, kind: WriteKind) -> PathWrite {
    PathWrite {
        path: path.to_owned(),
        kind,
    }
}

fn block(n: u64, writes: Vec<PathWrite>) -> BlockWrites {
    BlockWrites {
        number: n,
        block: block_of(n),
        proposal: n.is_multiple_of(2).then(|| format!("proposal-{n}")),
        writes,
    }
}

fn state(head: u64, floor: u64, genesis: bool) -> WritesState {
    WritesState {
        head_number: head,
        head_block: block_of(head),
        floor_number: floor,
        genesis,
    }
}

fn invariants(store: &(impl DerivedStore + WikiStore)) {
    let w = wiki(store, "writes");
    let other = wiki(store, "other");
    assert_eq!(store.writes_state(&w, VIEW).unwrap(), None);
    assert!(
        store
            .indexed_block(&w, VIEW, &block_of(1))
            .unwrap()
            .is_none()
    );
    assert!(store.last_writes(&w, VIEW, "a", 9).unwrap().last.is_none());

    // 1 creates a and b; 2 edits a; 3 moves b to c (a pure rename); 4 removes a; 5 makes a again; 6 may have written anything.
    store
        .append_now(
            &w,
            VIEW,
            &[
                block(
                    1,
                    vec![write("a", WriteKind::Wrote), write("b", WriteKind::Wrote)],
                ),
                block(2, vec![write("a", WriteKind::Wrote)]),
                block(
                    3,
                    vec![
                        write("b", WriteKind::Deleted),
                        write(
                            "c",
                            WriteKind::Carried {
                                from: "b".to_owned(),
                            },
                        ),
                    ],
                ),
                block(4, vec![write("a", WriteKind::Deleted)]),
                block(5, vec![write("a", WriteKind::Wrote)]),
            ],
            &state(5, 1, true),
        )
        .unwrap();
    assert_eq!(
        store.writes_state(&w, VIEW).unwrap(),
        Some(state(5, 1, true))
    );
    // The blocks are found by id and by number, with the proposal their message named.
    let found = store
        .indexed_block(&w, VIEW, &block_of(2))
        .unwrap()
        .unwrap();
    assert_eq!(
        (found.number, found.proposal.as_deref()),
        (2, Some("proposal-2"))
    );
    assert_eq!(
        store.indexed_block_at(&w, VIEW, 3).unwrap().unwrap().block,
        block_of(3)
    );
    assert!(store.indexed_block_at(&w, VIEW, 6).unwrap().is_none());

    // The newest certain row of a path at or before a block.
    let last = |path: &str, at: u64| {
        store
            .last_writes(&w, VIEW, path, at)
            .unwrap()
            .last
            .map(|l| (l.number, l.kind))
    };
    assert_eq!(last("a", 1), Some((1, WriteKind::Wrote)));
    assert_eq!(last("a", 3), Some((2, WriteKind::Wrote)));
    assert_eq!(last("a", 4), Some((4, WriteKind::Deleted)));
    assert_eq!(last("a", 9), Some((5, WriteKind::Wrote)));
    assert_eq!(
        last("c", 3),
        Some((
            3,
            WriteKind::Carried {
                from: "b".to_owned()
            }
        ))
    );
    assert_eq!(last("c", 2), None, "nothing is at c before the move");
    assert_eq!(last("b", 3), Some((3, WriteKind::Deleted)));
    assert_eq!(last("b", 2), Some((1, WriteKind::Wrote)));
    assert_eq!(last("nothing", 5), None);
    // Another wiki and another view have none of it.
    assert!(
        store
            .last_writes(&other, VIEW, "a", 5)
            .unwrap()
            .last
            .is_none()
    );
    assert!(
        store
            .last_writes(&w, "heads/other", "a", 5)
            .unwrap()
            .last
            .is_none()
    );

    // The later changes that wrote a path: a write or a carry into it, oldest first, one more than the limit when there are more.
    let later = |path: &str, after: u64, limit: usize| {
        store
            .later_writes(&w, VIEW, path, after, limit)
            .unwrap()
            .into_iter()
            .map(|l| (l.number, l.block))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        later("a", 1, 10),
        vec![(2, block_of(2)), (5, block_of(5))],
        "the removal at 4 is no write"
    );
    assert_eq!(later("a", 1, 1), vec![(2, block_of(2)), (5, block_of(5))]);
    assert_eq!(later("a", 5, 10), Vec::new());
    assert_eq!(later("c", 0, 10), vec![(3, block_of(3))]);

    // A block that may have written anything, and one that cannot be attributed, are listed for every path after the last certain row.
    store
        .append_now(
            &w,
            VIEW,
            &[
                block(6, vec![write("", WriteKind::Maybe)]),
                block(7, vec![write("a", WriteKind::Wrote)]),
                block(8, vec![write("", WriteKind::Opaque)]),
            ],
            &state(8, 1, true),
        )
        .unwrap();
    let at6 = store.last_writes(&w, VIEW, "a", 6).unwrap();
    assert_eq!(at6.last.map(|l| l.number), Some(5));
    assert_eq!(
        at6.maybes,
        vec![MaybeWriter {
            number: 6,
            opaque: false
        }]
    );
    // After a certain write of the path, the earlier maybe is no longer among those that may have written it.
    let at7 = store.last_writes(&w, VIEW, "a", 7).unwrap();
    assert_eq!(at7.last.as_ref().map(|l| l.number), Some(7));
    assert!(at7.maybes.is_empty(), "{at7:?}");
    let at8 = store.last_writes(&w, VIEW, "a", 8).unwrap();
    assert_eq!(
        at8.maybes,
        vec![MaybeWriter {
            number: 8,
            opaque: true
        }]
    );
    // Another path has not been written since 1, so every block after it that may have is among them.
    let b8 = store.last_writes(&w, VIEW, "b", 8).unwrap();
    assert_eq!(b8.last.map(|l| l.number), Some(3));
    assert_eq!(
        b8.maybes,
        vec![
            MaybeWriter {
                number: 6,
                opaque: false
            },
            MaybeWriter {
                number: 8,
                opaque: true
            }
        ]
    );

    // Correction L-6: the blocks after one that may have written a path (a path's own maybe and the block-wide ones, a block once),
    // and where a file was moved to with its bytes (found by the path it came from, one lookup).
    let maybes = |path: &str, after: u64| {
        store
            .later_maybes(&w, VIEW, path, after)
            .unwrap()
            .into_iter()
            .map(|m| (m.number, m.opaque))
            .collect::<Vec<_>>()
    };
    assert_eq!(maybes("a", 1), vec![(6, false), (8, true)]);
    assert_eq!(maybes("a", 6), vec![(8, true)], "after is exclusive");
    assert_eq!(maybes("a", 8), Vec::new());
    assert_eq!(maybes("nowhere", 0), vec![(6, false), (8, true)]);
    let moves = |path: &str, after: u64, limit: usize| {
        store
            .later_moves(&w, VIEW, path, after, limit)
            .unwrap()
            .into_iter()
            .map(|m| (m.number, m.block, m.to))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        moves("b", 1, 10),
        vec![(3, block_of(3), "c".to_owned())],
        "block 3 moved b to c"
    );
    assert_eq!(moves("b", 3, 10), Vec::new(), "after is exclusive");
    assert_eq!(moves("a", 0, 10), Vec::new(), "a was written, never moved");
    assert_eq!(
        moves("c", 0, 10),
        Vec::new(),
        "c is where it went, not where it came from"
    );
    assert!(
        store
            .later_moves(&other, VIEW, "b", 0, 10)
            .unwrap()
            .is_empty(),
        "another wiki's index is not this one's"
    );

    // A block already there keeps what it has (a second reader built the same), and the state only moves forward.
    store
        .append_now(
            &w,
            VIEW,
            &[block(2, vec![write("zzz", WriteKind::Wrote)])],
            &state(2, 1, true),
        )
        .unwrap();
    assert!(
        store
            .last_writes(&w, VIEW, "zzz", 9)
            .unwrap()
            .last
            .is_none()
    );
    assert_eq!(
        store.writes_state(&w, VIEW).unwrap(),
        Some(state(8, 1, true))
    );

    // Too many blocks that may have written a path are said to be more than are listed, never cut silently.
    let many: Vec<BlockWrites> = (10..10 + u64::try_from(MAX_MAYBE_WRITERS).unwrap() + 5)
        .map(|n| block(n, vec![write("", WriteKind::Maybe)]))
        .collect();
    let head = many.last().map_or(0, |b| b.number);
    store
        .append_now(&w, VIEW, &many, &state(head, 1, true))
        .unwrap();
    let crowded = store.last_writes(&w, VIEW, "a", head).unwrap();
    assert_eq!(crowded.maybes.len(), MAX_MAYBE_WRITERS);
    assert!(crowded.more);
    // The list of later ones holds one more than the bound, so that a caller can say *at least*.
    assert_eq!(
        store.later_maybes(&w, VIEW, "a", 9).unwrap().len(),
        MAX_MAYBE_WRITERS + 1
    );
    // A file moved several times is listed oldest first, with one more than the limit asked for.
    let base = head + 1;
    let moved: Vec<BlockWrites> = (0..3)
        .map(|n| {
            block(
                base + n,
                vec![write(
                    &format!("m{n}"),
                    WriteKind::Carried {
                        from: "moved".to_owned(),
                    },
                )],
            )
        })
        .collect();
    store
        .append_now(&w, VIEW, &moved, &state(base + 2, 1, true))
        .unwrap();
    let listed: Vec<String> = store
        .later_moves(&w, VIEW, "moved", 0, 2)
        .unwrap()
        .into_iter()
        .map(|m| m.to)
        .collect();
    assert_eq!(listed, ["m0", "m1", "m2"], "two asked for, and one more");

    // Dropping a view's index drops that view's alone; `clear` drops the wiki's, and counts each view once.
    store
        .append_now(
            &w,
            "heads/second",
            &[block(1, vec![write("a", WriteKind::Wrote)])],
            &state(1, 1, true),
        )
        .unwrap();
    store
        .append_now(
            &other,
            VIEW,
            &[block(1, vec![write("a", WriteKind::Wrote)])],
            &state(1, 1, true),
        )
        .unwrap();
    store.reset_writes(&w, "heads/second").unwrap();
    assert!(store.writes_state(&w, "heads/second").unwrap().is_none());
    assert!(store.writes_state(&w, VIEW).unwrap().is_some());
    assert_eq!(store.clear(&w).unwrap(), 1);
    assert!(store.writes_state(&w, VIEW).unwrap().is_none());
    assert!(
        store
            .indexed_block(&w, VIEW, &block_of(1))
            .unwrap()
            .is_none()
    );
    assert!(store.last_writes(&w, VIEW, "a", 9).unwrap().last.is_none());
    assert!(
        store.writes_state(&other, VIEW).unwrap().is_some(),
        "another wiki's is kept"
    );
}

#[test]
fn both_stores_hold_the_writes_index_invariants() {
    invariants(&MemoryStore::new());
    invariants(&SqliteStore::open_in_memory().expect("open"));
}

/// **The generation of a view's index moves when the index is dropped, and only then** (correction L-9): a reader that read it before
/// its rows and reads it again after them sees that the index it read is gone.
fn generations(store: &(impl DerivedStore + WikiStore)) {
    let w = wiki(store, "generation");
    let other = wiki(store, "elsewhere");
    let at = |wiki: &WikiId, view: &str| store.writes_generation(wiki, view).unwrap();
    assert_eq!(at(&w, VIEW), 0, "a view never dropped is at 0");
    let built = |wiki: &WikiId, view: &str| {
        store
            .append_now(
                wiki,
                view,
                &[block(1, vec![write("a", WriteKind::Wrote)])],
                &state(1, 1, true),
            )
            .unwrap();
    };
    built(&w, VIEW);
    built(&w, "heads/second");
    built(&other, VIEW);
    // Building and reading move nothing.
    assert_eq!(at(&w, VIEW), 0);
    store.last_writes(&w, VIEW, "a", 1).unwrap();
    assert_eq!(at(&w, VIEW), 0);
    // A reset moves that view's generation and no other's.
    store.reset_writes(&w, VIEW).unwrap();
    assert_eq!(at(&w, VIEW), 1);
    assert_eq!(at(&w, "heads/second"), 0);
    assert_eq!(at(&other, VIEW), 0);
    // The generation outlives the index it counts: built again, the view is still at 1, and a second reset is the next.
    built(&w, VIEW);
    assert_eq!(at(&w, VIEW), 1);
    store.reset_writes(&w, VIEW).unwrap();
    assert_eq!(at(&w, VIEW), 2);
    // `clear` moves each view of the wiki that had an index, and no other wiki's.
    built(&w, VIEW);
    store.clear(&w).unwrap();
    assert_eq!(at(&w, VIEW), 3);
    assert_eq!(at(&w, "heads/second"), 1);
    assert_eq!(at(&other, VIEW), 0, "another wiki's is kept");
}

#[test]
fn both_stores_move_the_generation_of_a_view_when_its_index_is_dropped_and_only_then() {
    generations(&MemoryStore::new());
    generations(&SqliteStore::open_in_memory().expect("open"));
}

/// **An append is to the index the builder began with, and the store says so** (correction Q-2): the generation the builder read is
/// compared with the view's inside the step that appends. An index dropped between the two (by a reset, or by `clear`) is not
/// appended to: **nothing is written, state included**, and the answer is [`Appended::Dropped`]. A drop of one view, or of another
/// wiki's, is no reason to refuse.
fn appends_only_to_the_index_the_builder_began_with(store: &(impl DerivedStore + WikiStore)) {
    let w = wiki(store, "dropped");
    let other = wiki(store, "elsewhere");
    let blocks = |from: u64, to: u64| -> Vec<BlockWrites> {
        (from..=to)
            .map(|n| block(n, vec![write("a", WriteKind::Wrote)]))
            .collect()
    };
    let append = |wiki: &WikiId, view: &str, from: u64, to: u64, generation: u64| {
        store
            .append_writes(
                wiki,
                view,
                &blocks(from, to),
                &state(to, from, from == 1),
                generation,
            )
            .unwrap()
    };
    let generation = |wiki: &WikiId, view: &str| store.writes_generation(wiki, view).unwrap();

    // Under the generation the view has, an append is made.
    assert_eq!(append(&w, VIEW, 1, 2, 0), Appended::Done);
    assert_eq!(
        store.writes_state(&w, VIEW).unwrap(),
        Some(state(2, 1, true))
    );
    // The builder read the generation, then the index was dropped, then it appended what it built from the log it read: refused,
    // and nothing is left of what it brought (no block, no state).
    store.reset_writes(&w, VIEW).unwrap();
    assert_eq!(generation(&w, VIEW), 1);
    assert_eq!(append(&w, VIEW, 3, 5, 0), Appended::Dropped);
    assert_eq!(
        store.writes_state(&w, VIEW).unwrap(),
        None,
        "no state under a floor of an index that is gone"
    );
    for n in 1..=5 {
        assert!(
            store.indexed_block_at(&w, VIEW, n).unwrap().is_none(),
            "block {n} was not appended"
        );
    }
    assert_eq!(generation(&w, VIEW), 1, "a refused append moves nothing");
    // A builder that began after the drop appends.
    assert_eq!(append(&w, VIEW, 1, 5, 1), Appended::Done);
    assert_eq!(
        store.writes_state(&w, VIEW).unwrap(),
        Some(state(5, 1, true))
    );
    // `clear` drops it too.
    store.clear(&w).unwrap();
    assert_eq!(append(&w, VIEW, 6, 6, 1), Appended::Dropped);
    assert_eq!(store.writes_state(&w, VIEW).unwrap(), None);
    // Only the view that was dropped refuses: another view of the wiki, and another wiki's view, are at their own generations.
    assert_eq!(append(&w, "heads/second", 1, 1, 0), Appended::Done);
    assert_eq!(append(&other, VIEW, 1, 1, 0), Appended::Done);
    // A generation that is not the view's (a builder of a later one cannot exist) is refused as well, rather than trusted.
    assert_eq!(append(&other, VIEW, 2, 2, 7), Appended::Dropped);
    assert!(store.indexed_block_at(&other, VIEW, 2).unwrap().is_none());
}

#[test]
fn both_stores_refuse_an_append_to_an_index_that_was_dropped_since_the_builder_began() {
    appends_only_to_the_index_the_builder_began_with(&MemoryStore::new());
    appends_only_to_the_index_the_builder_began_with(&SqliteStore::open_in_memory().expect("open"));
}

/// **What is found not in a view's log is kept with the view's tip** (correction L-1): it holds while the view is at that tip, is
/// dropped with the index, is bounded, and one view's is not another's.
fn absences(store: &(impl DerivedStore + WikiStore)) {
    let w = wiki(store, "absence");
    let other = wiki(store, "elsewhere");
    let absent = |view: &str, b: u64, tip: u64| {
        store
            .block_absent(&w, view, &block_of(b), &block_of(tip))
            .unwrap()
    };
    let note = |view: &str, b: u64, tip: u64| {
        store
            .note_block_absent(&w, view, &block_of(b), &block_of(tip))
            .unwrap();
    };
    assert!(!absent(VIEW, 9, 5));
    note(VIEW, 9, 5);
    assert!(absent(VIEW, 9, 5));
    // Only while the view is where it was.
    assert!(!absent(VIEW, 9, 6), "the view moved: asked again");
    assert!(!absent(VIEW, 8, 5), "another block");
    assert!(!absent("heads/second", 9, 5), "another view");
    assert!(
        !store
            .block_absent(&other, VIEW, &block_of(9), &block_of(5))
            .unwrap(),
        "another wiki"
    );
    // Noted again at the view's next tip, it holds there (and no longer at the old one).
    note(VIEW, 9, 6);
    assert!(absent(VIEW, 9, 6));
    assert!(!absent(VIEW, 9, 5));
    // Dropped with the index of its view, and not with another view's.
    note("heads/second", 9, 5);
    store.reset_writes(&w, VIEW).unwrap();
    assert!(!absent(VIEW, 9, 6));
    assert!(absent("heads/second", 9, 5));
    store.clear(&w).unwrap();
    assert!(!absent("heads/second", 9, 5), "`clear` drops the wiki's");
    // A view keeps at most so many, the oldest dropped.
    let more = u64::try_from(MAX_ABSENT_BLOCKS).unwrap() + 3;
    for n in 1..=more {
        note(VIEW, n, 1);
    }
    assert!(!absent(VIEW, 1, 1), "the oldest went");
    assert!(!absent(VIEW, 3, 1), "the oldest went");
    assert!(absent(VIEW, 4, 1));
    assert!(absent(VIEW, more, 1));
}

#[test]
fn both_stores_keep_the_blocks_found_not_in_a_views_log_with_its_tip_and_bound_them() {
    absences(&MemoryStore::new());
    absences(&SqliteStore::open_in_memory().expect("open"));
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> TempDir {
    let p = std::env::temp_dir().join(format!("inklkunn-store-w-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp");
    TempDir(p)
}

#[test]
fn the_writes_index_persists_across_reopen_and_a_code_this_build_does_not_know_is_not_a_block_that_wrote_nothing()
 {
    let tmp = temp("reopen");
    let db = tmp.0.join("store.sqlite");
    let w = {
        let store = SqliteStore::open(&db).expect("open");
        let w = wiki(&store, "persisted");
        store
            .append_now(
                &w,
                VIEW,
                &[block(1, vec![write("a", WriteKind::Wrote)])],
                &state(1, 1, true),
            )
            .unwrap();
        w
    };
    let store = SqliteStore::open(&db).expect("reopen");
    assert_eq!(
        store.writes_state(&w, VIEW).unwrap(),
        Some(state(1, 1, true))
    );
    assert_eq!(
        store
            .last_writes(&w, VIEW, "a", 1)
            .unwrap()
            .last
            .map(|l| l.kind),
        Some(WriteKind::Wrote)
    );
    // A row of a kind a newer build wrote (the schema's own check refuses it here, so the row is made past the check) is read as
    // one that cannot be attributed.
    let raw = rusqlite::Connection::open(&db).expect("raw open");
    raw.execute_batch(
        "PRAGMA ignore_check_constraints = ON; \
         UPDATE path_writes SET kind = 'moved-by-magic', from_path = NULL WHERE path = ''; \
         INSERT INTO path_writes (wiki_id, view, path, number, kind, from_path) \
             SELECT wiki_id, view, '', 2, 'moved-by-magic', NULL FROM path_writes LIMIT 1; \
         INSERT INTO write_blocks (wiki_id, view, number, block_id, proposal) \
             SELECT wiki_id, view, 2, 'ff', NULL FROM write_blocks LIMIT 1;",
    )
    .unwrap();
    let got = store.last_writes(&w, VIEW, "a", 2).unwrap();
    assert_eq!(
        got.maybes,
        vec![MaybeWriter {
            number: 2,
            opaque: true
        }]
    );
}

fn refuses_what_is_not_a_row(store: &(impl DerivedStore + WikiStore)) {
    let w = wiki(store, "shape");
    store
        .append_now(
            &w,
            VIEW,
            &[block(1, vec![write("a", WriteKind::Wrote)])],
            &state(1, 1, true),
        )
        .unwrap();
    // A row about every path is a *may have written* or an *opaque* one; an opaque row is of no one path; a carried row says where
    // it came from; a block has an id. Nothing is written when any is refused (and never silently dropped).
    for (what, bad) in [
        ("a certain row of no path", write("", WriteKind::Wrote)),
        ("a deleted row of no path", write("", WriteKind::Deleted)),
        ("an opaque row of a path", write("a", WriteKind::Opaque)),
        (
            "a carried row from nowhere",
            write(
                "b",
                WriteKind::Carried {
                    from: String::new(),
                },
            ),
        ),
    ] {
        let generation = store.writes_generation(&w, VIEW).unwrap();
        let result = store.append_writes(
            &w,
            VIEW,
            &[block(2, vec![bad])],
            &state(2, 1, true),
            generation,
        );
        assert!(result.is_err(), "{what} is refused");
        assert_eq!(
            store.writes_state(&w, VIEW).unwrap(),
            Some(state(1, 1, true)),
            "{what}: nothing was written"
        );
        assert!(
            store.indexed_block_at(&w, VIEW, 2).unwrap().is_none(),
            "{what}"
        );
    }
}

#[test]
fn both_stores_refuse_what_is_not_a_row_and_drop_nothing_silently() {
    refuses_what_is_not_a_row(&MemoryStore::new());
    refuses_what_is_not_a_row(&SqliteStore::open_in_memory().expect("open"));
}
