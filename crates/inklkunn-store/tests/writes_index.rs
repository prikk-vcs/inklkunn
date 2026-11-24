//! The writes index on both stores (RFC 006 D-8; handoff G-T4, correction J-2): who wrote each path of a view's log, in the order
//! the view numbers its blocks, answered with a few indexed lookups. The same invariants on the memory store and on SQLite, what
//! only SQLite can show (reopen, a row this build cannot read), and that the derived store's `clear` drops it.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use inklkunn_store::{
    BlockWrites, DerivedStore, DisplayName, MAX_MAYBE_WRITERS, MaybeWriter, MemoryStore, NewWiki,
    PathWrite, Slug, SqliteStore, WikiId, WikiStore, WriteKind, WritesState,
};

const VIEW: &str = "heads/main";

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
        .append_writes(
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
        .append_writes(
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
        .append_writes(
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
        .append_writes(&w, VIEW, &many, &state(head, 1, true))
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
        .append_writes(&w, VIEW, &moved, &state(base + 2, 1, true))
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
        .append_writes(
            &w,
            "heads/second",
            &[block(1, vec![write("a", WriteKind::Wrote)])],
            &state(1, 1, true),
        )
        .unwrap();
    store
        .append_writes(
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
            .append_writes(
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
        .append_writes(
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
        let result = store.append_writes(&w, VIEW, &[block(2, vec![bad])], &state(2, 1, true));
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
