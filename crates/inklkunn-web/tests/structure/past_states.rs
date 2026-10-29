//! Every state of the pages about a page's past (handoff 003 T5), reached through the real router over an
//! in-memory repository with a scripted history: a page's history (with rows, in a view, empty, partly read,
//! ambiguous, prikk busy, no such view), a change (from a proposal, made by the service, sealed by someone
//! else, too large, not there), a difference (modified, new, removed, unchanged, no lines, not a hunk, not
//! there) and a permalink (the page, a point before it existed, no such change). Each is a page the structural
//! rules run on, in both languages.

use inklkunn_prikk::{StorageError, Unavailable};

use super::common::{World, busy, diff_of, op, page, script_history, shown};
use super::wiki_states::Scenario;

const B1: &str = "00000000000000000000000000000000000000000000000000000000b10c0001";
const B2: &str = "00000000000000000000000000000000000000000000000000000000b10c0002";
const B3: &str = "00000000000000000000000000000000000000000000000000000000b10c0003";
const B4: &str = "00000000000000000000000000000000000000000000000000000000b10c0004";

fn past(w: &World) {
    script_history(w);
}

/// The history, then a long run of changes to the guide, so that a history of `Home` stops before its end.
fn long(w: &World) {
    script_history(w);
    for v in 5..=44_u64 {
        w.fake.set_files(&[(
            "pages/Guide.md",
            &page(&["title: Guide"], &format!("v{v}.\n")),
        )]);
        w.fake.say(v, "Edit the guide");
        w.fake
            .show(v, shown(vec![op("edit-text", &["pages/Guide.md"])]));
    }
}

fn diff_with(w: &World, status: &str, hunks: &[&str]) {
    script_history(w);
    w.fake
        .diff_says(Ok(diff_of("pages/Home.md", status, hunks, 1, 2)));
}

pub(super) const SCENARIOS: &[Scenario] = &[
    // --- a page's history
    Scenario {
        id: "history",
        identity: "history-home",
        status: 200,
        warm: &[],
        setup: past,
        method: "GET",
        uri: "/handbook/-/history/Home",
        headers: &[],
    },
    Scenario {
        id: "history-in-a-view",
        identity: "history-home",
        status: 200,
        warm: &[],
        setup: past,
        method: "GET",
        uri: "/handbook/-/history/Home?view=main",
        headers: &[],
    },
    Scenario {
        id: "history-with-a-moved-page",
        identity: "history-guide",
        status: 200,
        warm: &[],
        setup: |w| {
            script_history(w);
            w.fake.set_files(&[(
                "pages/Manual.md",
                &page(&["title: Manual"], "Moved here.\n"),
            )]);
            w.fake.say(5, "Rename the guide");
            w.fake.show(
                5,
                shown(vec![op(
                    "rename-path",
                    &["pages/Guide.md", "pages/Manual.md"],
                )]),
            );
        },
        method: "GET",
        uri: "/handbook/-/history/Guide",
        headers: &[],
    },
    Scenario {
        id: "history-empty",
        identity: "history-empty",
        status: 200,
        warm: &[],
        setup: past,
        method: "GET",
        uri: "/handbook/-/history/Never%20Written",
        headers: &[],
    },
    Scenario {
        id: "history-partly-read",
        identity: "history-home",
        status: 200,
        warm: &[],
        setup: long,
        method: "GET",
        uri: "/handbook/-/history/Home",
        headers: &[],
    },
    Scenario {
        id: "history-too-large-a-change",
        identity: "history-guide",
        status: 200,
        warm: &[],
        setup: |w| {
            script_history(w);
            w.fake.show(
                3,
                Err(StorageError::Unavailable {
                    verb: "show",
                    reason: Unavailable::OutputExceeded,
                }),
            );
        },
        method: "GET",
        uri: "/handbook/-/history/Guide",
        headers: &[],
    },
    Scenario {
        id: "history-ambiguous",
        identity: "wiki-ambiguous",
        status: 200,
        warm: &[],
        setup: past,
        method: "GET",
        uri: "/handbook/-/history/Twin",
        headers: &[],
    },
    Scenario {
        id: "history-no-such-view",
        identity: "wiki-no-such-view",
        status: 404,
        warm: &[],
        setup: past,
        method: "GET",
        uri: "/handbook/-/history/Home?view=nope",
        headers: &[],
    },
    Scenario {
        id: "history-unavailable",
        identity: "wiki-unavailable",
        status: 503,
        warm: &[],
        setup: |w| {
            script_history(w);
            w.fake.fail_views(Some(busy()));
        },
        method: "GET",
        uri: "/handbook/-/history/Home",
        headers: &[],
    },
    // --- one change
    Scenario {
        id: "change-from-a-proposal",
        identity: "change-2",
        status: 200,
        warm: &[],
        setup: past,
        method: "GET",
        uri: "/handbook/-/change/00000000000000000000000000000000000000000000000000000000b10c0002",
        headers: &[],
    },
    Scenario {
        id: "change-by-the-service",
        identity: "change-3",
        status: 200,
        warm: &[],
        setup: past,
        method: "GET",
        uri: "/handbook/-/change/00000000000000000000000000000000000000000000000000000000b10c0003",
        headers: &[],
    },
    Scenario {
        id: "change-too-large",
        identity: "change-3",
        status: 200,
        warm: &[],
        setup: |w| {
            script_history(w);
            w.fake.show(
                3,
                Err(StorageError::Unavailable {
                    verb: "show",
                    reason: Unavailable::OutputExceeded,
                }),
            );
        },
        method: "GET",
        uri: "/handbook/-/change/00000000000000000000000000000000000000000000000000000000b10c0003",
        headers: &[],
    },
    Scenario {
        id: "change-sealed-by-someone-else",
        identity: "change-4",
        status: 200,
        warm: &[],
        setup: past,
        method: "GET",
        uri: "/handbook/-/change/00000000000000000000000000000000000000000000000000000000b10c0004",
        headers: &[],
    },
    Scenario {
        id: "change-the-first",
        identity: "change-1",
        status: 200,
        warm: &[],
        setup: past,
        method: "GET",
        uri: "/handbook/-/change/00000000000000000000000000000000000000000000000000000000b10c0001",
        headers: &[],
    },
    Scenario {
        id: "change-none",
        identity: "change-none",
        status: 404,
        warm: &[],
        setup: past,
        method: "GET",
        uri: "/handbook/-/change/eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        headers: &[],
    },
    // --- a page's difference
    Scenario {
        id: "diff",
        identity: "diff-home",
        status: 200,
        warm: &[],
        setup: past,
        method: "GET",
        uri: "/handbook/-/diff/00000000000000000000000000000000000000000000000000000000b10c0001/00000000000000000000000000000000000000000000000000000000b10c0002/Home",
        headers: &[],
    },
    Scenario {
        id: "diff-new-page",
        identity: "diff-home",
        status: 200,
        warm: &[],
        setup: |w| diff_with(w, "added", &["@@ -0,0 +1 @@\n+first\n"]),
        method: "GET",
        uri: "/handbook/-/diff/00000000000000000000000000000000000000000000000000000000b10c0001/00000000000000000000000000000000000000000000000000000000b10c0002/Home",
        headers: &[],
    },
    Scenario {
        id: "diff-removed-page",
        identity: "diff-home",
        status: 200,
        warm: &[],
        setup: |w| diff_with(w, "removed", &["@@ -1 +0,0 @@\n-gone\n"]),
        method: "GET",
        uri: "/handbook/-/diff/00000000000000000000000000000000000000000000000000000000b10c0001/00000000000000000000000000000000000000000000000000000000b10c0002/Home",
        headers: &[],
    },
    Scenario {
        id: "diff-no-lines",
        identity: "diff-home",
        status: 200,
        warm: &[],
        setup: |w| diff_with(w, "modified", &[]),
        method: "GET",
        uri: "/handbook/-/diff/00000000000000000000000000000000000000000000000000000000b10c0001/00000000000000000000000000000000000000000000000000000000b10c0002/Home",
        headers: &[],
    },
    Scenario {
        id: "diff-not-a-hunk",
        identity: "diff-home",
        status: 200,
        warm: &[],
        setup: |w| diff_with(w, "modified", &["this is not a unified hunk\n"]),
        method: "GET",
        uri: "/handbook/-/diff/00000000000000000000000000000000000000000000000000000000b10c0001/00000000000000000000000000000000000000000000000000000000b10c0002/Home",
        headers: &[],
    },
    Scenario {
        id: "diff-unchanged",
        identity: "diff-home",
        status: 200,
        warm: &[],
        setup: |w| {
            script_history(w);
            let mut none = diff_of("pages/Home.md", "modified", &[], 1, 2);
            none.entries.clear();
            w.fake.diff_says(Ok(none));
        },
        method: "GET",
        uri: "/handbook/-/diff/00000000000000000000000000000000000000000000000000000000b10c0001/00000000000000000000000000000000000000000000000000000000b10c0002/Home",
        headers: &[],
    },
    Scenario {
        id: "diff-none",
        identity: "change-none",
        status: 404,
        warm: &[],
        setup: past,
        method: "GET",
        uri: "/handbook/-/diff/nothing/nothing/Home",
        headers: &[],
    },
    // --- a page as it was (a permalink)
    Scenario {
        id: "at",
        identity: "at-home-2",
        status: 200,
        warm: &[],
        setup: past,
        method: "GET",
        uri: "/handbook/-/at/00000000000000000000000000000000000000000000000000000000b10c0002/Home",
        headers: &[],
    },
    Scenario {
        id: "at-in-a-view",
        identity: "at-home-2",
        status: 200,
        warm: &[],
        setup: past,
        method: "GET",
        uri: "/handbook/-/at/00000000000000000000000000000000000000000000000000000000b10c0002/Home?view=main",
        headers: &[],
    },
    Scenario {
        id: "at-before-the-page-existed",
        identity: "at-missing",
        status: 404,
        warm: &[],
        setup: |w| {
            script_history(w);
            w.fake
                .set_files(&[("pages/Fresh.md", &page(&["title: Fresh"], "New.\n"))]);
            w.fake.say(5, "Add Fresh");
            w.fake
                .show(5, shown(vec![op("create-file", &["pages/Fresh.md"])]));
        },
        method: "GET",
        uri: "/handbook/-/at/00000000000000000000000000000000000000000000000000000000b10c0001/Fresh",
        headers: &[],
    },
    Scenario {
        id: "at-none",
        identity: "change-none",
        status: 404,
        warm: &[],
        setup: past,
        method: "GET",
        uri: "/handbook/-/at/eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee/Home",
        headers: &[],
    },
];

// The block ids above are `block_id(n)` of the test repository; this keeps them so.
#[test]
fn the_block_ids_in_the_addresses_are_the_repositorys() {
    for (n, id) in [(1, B1), (2, B2), (3, B3), (4, B4)] {
        assert_eq!(super::common::block_id(n), id);
    }
}
