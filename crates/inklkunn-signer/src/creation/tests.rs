#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

fn layout(tag: &str) -> (RootLayout, PathBuf) {
    let dir = std::env::temp_dir().join(format!("inklkunn-marks-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    (RootLayout::new(&dir), dir)
}

#[test]
fn a_mark_is_made_durable_before_anything_else_and_removed_only_by_finish() {
    let (l, dir) = layout("finish");
    let id = WikiId::new();
    assert!(!l.root().join(id.shard()).exists());
    let mark = CreationMark::begin(&l, &id).unwrap();
    assert!(l.creation_mark(&id).is_file());
    // Dropping a mark (the process going away) leaves it for the sweep.
    drop(mark);
    assert!(l.creation_mark(&id).is_file());
    // Finishing a held mark removes it.
    let again = find_unfinished(&l).unwrap();
    assert_eq!(again.dead.len(), 1);
    for m in again.dead {
        m.finish().unwrap();
    }
    assert!(!l.creation_mark(&id).exists());
    assert!(find_unfinished(&l).unwrap().dead.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_running_creation_is_live_and_a_second_one_for_the_same_wiki_is_refused() {
    let (l, dir) = layout("live");
    let id = WikiId::new();
    let mark = CreationMark::begin(&l, &id).unwrap();
    let found = find_unfinished(&l).unwrap();
    assert!(found.dead.is_empty(), "a live creation was claimed as dead");
    assert_eq!(found.live, vec![id.clone()]);
    let err = CreationMark::begin(&l, &id).unwrap_err();
    assert!(matches!(err, SignerError::Unavailable(_)), "{err:?}");
    assert!(l.creation_mark(&id).is_file(), "the live mark survived");
    mark.finish().unwrap();
    assert!(!l.creation_mark(&id).exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_swept_mark_is_held_so_that_nothing_else_claims_it() {
    let (l, dir) = layout("held");
    let id = WikiId::new();
    drop(CreationMark::begin(&l, &id).unwrap());
    let first = find_unfinished(&l).unwrap();
    assert_eq!(first.dead.len(), 1);
    // While the first sweep holds it, a second sweep sees it as live.
    let second = find_unfinished(&l).unwrap();
    assert!(second.dead.is_empty());
    assert_eq!(second.live, vec![id]);
    let _ = std::fs::remove_dir_all(&dir);
}
