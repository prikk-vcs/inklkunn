#![allow(clippy::expect_used, clippy::unwrap_used)]
//! The writes index built in the background, end to end on a real prikk (0.4.0 readiness correction P-5): a server that is started
//! while a content hide is in force builds the index of who wrote each path **by itself**, with no reader, and an old version of a
//! page then answers with its tombstone and with its text at once, without a single `show`. (What a step does is the core's own
//! tests; what the worker does with wikis and failures is `serve/index_warm/tests.rs`; this is the whole of it over a real
//! repository.)

use inklkunn_store::DerivedStore;

use super::measure::{Counting, HELP_TITLE, HIDDEN_WORDS, edit_page, hide_content, permalink};
use super::*;

#[test]
fn a_server_started_with_a_content_hide_in_force_builds_the_index_by_itself_with_no_reader() {
    let Some(mut world) = World::new("writes-background") else {
        return;
    };
    let real = world.settings.prikk.clone();
    let counting = Counting::install(&mut world, &real);
    let running = Running::start(&world);
    let mut maya = join(
        &world,
        &running,
        "maya",
        "Maya Maintainer",
        Role::Maintainer,
    );
    let mut noor = join(
        &world,
        &running,
        "noor",
        "Noor Contributor",
        Role::Contributor,
    );
    // Eight accepted edits of the one page; the third is hidden, and the newest is the tip.
    let (mut hidden_proposal, mut hidden_block) = (String::new(), String::new());
    for n in 1..=8 {
        let proposal = edit_page(&mut noor, &mut maya, HELP_TITLE, None, n, true);
        if n == 3 {
            hidden_proposal = proposal;
            hidden_block = world.blocks().first().cloned().unwrap();
        }
    }
    let tip = world.blocks().first().cloned().unwrap();
    hide_content(&mut maya, &counting, &hidden_proposal);
    let state = |world: &World| {
        world
            .store
            .writes_state(&world.wiki.id, &world.wiki.default_view)
            .unwrap()
    };
    // The index is dropped (and the server stopped), as an operator's refresh would: nothing is built, and a hide is in force.
    running.services.read.clear_derived(&world.wiki).unwrap();
    assert!(state(&world).is_none());
    running.stop();

    // The server starts again: it says that it builds the index, and builds it with no request at all.
    let running = Running::start(&world);
    assert!(
        running
            .notes
            .iter()
            .any(|n| n.contains("handbook") && n.contains("built in the background")),
        "{:?}",
        running.notes
    );
    counting.take();
    let end = Instant::now() + Duration::from_secs(120);
    while !state(&world).is_some_and(|s| s.head_block == tip) {
        assert!(
            Instant::now() < end,
            "the index did not reach the tip by itself: {:?}",
            state(&world)
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    let built = counting.take();
    assert!(built.of("show") > 0, "the build was a build: {built:?}");
    assert_eq!(
        running.services.read.reader_advances(),
        0,
        "no request had any part in it"
    );

    // An old version answers with its tombstone, the newest with its text, and neither needs a `show`.
    let mut anonymous = running.browser();
    let old = anonymous.get(&permalink(&hidden_block));
    assert_eq!(old.status, 200, "{}", old.body);
    assert!(old.body.contains(HIDDEN_WORDS), "{}", old.body);
    let newest = anonymous.get(&permalink(&tip));
    assert_eq!(newest.status, 200, "{}", newest.body);
    assert!(!newest.body.contains(HIDDEN_WORDS));
    assert_eq!(counting.take().of("show"), 0, "the reads built nothing");
    running.stop();
}
