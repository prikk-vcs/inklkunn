//! What a comparison is held by (handoff 005 K12): the wiki and the two blocks, in the order asked; one entry for one
//! pair; the newest [`CACHED_COMPARISONS`] and no more; and a tip whose block is not known is not a key.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

fn tip(view: &str, block: &str) -> Tip {
    Tip {
        view: view.to_owned(),
        state_id: format!("state-{block}"),
        block_id: block.to_owned(),
    }
}

fn pair(wiki: &WikiId, a: &str, b: &str) -> Pair {
    Pair {
        wiki: wiki.clone(),
        a: a.to_owned(),
        b: b.to_owned(),
    }
}

#[test]
fn a_pair_is_the_wiki_and_the_two_blocks_in_the_order_asked() {
    let wiki = crate::read::testing::World::with(
        &[("pages/Home.md", "x\n")],
        crate::read::ReadConfig::default(),
    )
    .wiki;
    let (x, y) = (tip("heads/a", "x"), tip("heads/b", "y"));
    let ab = Pair::of(&wiki, &x, &y).expect("a pair");
    assert_eq!(ab, pair(&wiki.id, "x", "y"));
    assert_ne!(Some(ab), Pair::of(&wiki, &y, &x));
    // The same blocks under another view's name are the same pair: a comparison depends on blocks, not on names.
    assert_eq!(
        Pair::of(&wiki, &x, &y),
        Pair::of(&wiki, &tip("heads/other", "x"), &tip("heads/else", "y"))
    );
    // A block that is not known (a stale page's tip) is not a key, on either side.
    assert_eq!(Pair::of(&wiki, &tip("heads/a", ""), &y), None);
    assert_eq!(Pair::of(&wiki, &x, &tip("heads/b", "")), None);
}

#[test]
fn one_pair_is_one_entry_and_the_newest_stay() {
    let wiki = WikiId::new();
    let list: Mutex<Vec<(Pair, usize)>> = Mutex::new(Vec::new());
    hold(&list, pair(&wiki, "a", "b"), 1);
    hold(&list, pair(&wiki, "a", "b"), 2);
    assert_eq!(
        list.lock().unwrap().len(),
        1,
        "a pair held twice is held once"
    );
    assert_eq!(held(&list, &pair(&wiki, "a", "b")), Some(2));
    // Another wiki's blocks, or the same blocks the other way round, are other keys.
    assert_eq!(held(&list, &pair(&WikiId::new(), "a", "b")), None);
    assert_eq!(held(&list, &pair(&wiki, "b", "a")), None);
    // Past the bound the oldest go first, and the bound is exact.
    for n in 0..CACHED_COMPARISONS {
        hold(&list, pair(&wiki, "p", &n.to_string()), n);
    }
    assert_eq!(list.lock().unwrap().len(), CACHED_COMPARISONS);
    assert_eq!(
        held(&list, &pair(&wiki, "a", "b")),
        None,
        "the oldest is gone"
    );
    assert_eq!(
        held(
            &list,
            &pair(&wiki, "p", &(CACHED_COMPARISONS - 1).to_string())
        ),
        Some(CACHED_COMPARISONS - 1)
    );
    assert_eq!(
        held(&list, &pair(&wiki, "p", "0")),
        Some(0),
        "the oldest of those that stay is held"
    );
}
