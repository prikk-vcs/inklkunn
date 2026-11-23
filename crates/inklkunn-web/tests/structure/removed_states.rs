//! The pages of a removed page (RFC 006 D-10; handoff G-T6), reached through the real router over an in-memory repository with a
//! scripted history: the page a removed page answers with (`410`) for a visitor who may not propose and for a person who may, and
//! the form that proposes restoring it, for each way back (a rollback, and the page's last text when later changes block it).
//! Each is a page the structural rules run on, in both languages.

use inklkunn_store::Role;

use super::common::{World, accept_by_hand, add_person, op, page, person, proposal_text, shown};
use super::wiki_states::Scenario;

const GUIDE: &str = "pages/Guide.md";
const HOME: &str = "pages/Home.md";
/// `person(1)`'s id, the header the test identity reads.
const CONTRIBUTOR: &str = "019a0000-0000-7000-8000-000000000101";

fn home(body: &str) -> String {
    page(&["title: Home", "description: Start here"], body)
}

/// Version 2 removes `Guide`, by a proposal this wiki keeps; `edits_home` makes the same change edit `Home` too.
fn removal(w: &World, edits_home: bool) {
    let author = person(1);
    add_person(w, &author, "Mika Tanaka", Some(Role::Contributor));
    add_person(w, &person(2), "Ren Sato", Some(Role::Maintainer));
    w.fake.say(1, "Create the handbook");
    w.fake.show(
        1,
        shown(vec![
            op("create-file", &[HOME]),
            op("create-file", &[GUIDE]),
        ]),
    );
    let record = w.hold_unjudged(
        &author,
        "Remove the guide: it is out of date",
        vec![inklkunn_store::FileChange {
            path: GUIDE.to_owned(),
            text: None,
        }],
    );
    accept_by_hand(w, &record.id, &person(2), &author);
    if edits_home {
        w.fake.apply(&[(HOME, &home("Tidied.\n"))], &[GUIDE]);
        w.fake.show(
            2,
            shown(vec![op("delete-node", &[GUIDE]), op("edit-text", &[HOME])]),
        );
    } else {
        w.fake.remove(GUIDE);
        w.fake.show(2, shown(vec![op("delete-node", &[GUIDE])]));
    }
    w.fake.say(
        2,
        &proposal_text(
            "Remove the guide: it is out of date",
            "Mika Tanaka",
            "Ren Sato",
        ),
    );
}

/// A later change to the same place of `Home`: the rollback of the removal would overlap it.
fn blocked(w: &World) {
    removal(w, true);
    w.fake.set(HOME, &home("Tidied again.\n"));
    w.fake.say(3, "Tidy the front page again");
    w.fake.show(3, shown(vec![op("edit-text", &[HOME])]));
}

fn clean(w: &World) {
    removal(w, false);
}

pub(super) const SCENARIOS: &[Scenario] = &[
    Scenario {
        id: "removed-page",
        identity: "removed-page",
        status: 410,
        warm: &[],
        setup: clean,
        method: "GET",
        uri: "/handbook/Guide",
        headers: &[],
    },
    Scenario {
        id: "removed-page-proposer",
        identity: "removed-page",
        status: 410,
        warm: &[],
        setup: clean,
        method: "GET",
        uri: "/handbook/Guide",
        headers: &[("x-test-account", CONTRIBUTOR)],
    },
    Scenario {
        id: "removed-page-last-text",
        identity: "removed-page",
        status: 410,
        warm: &[],
        setup: blocked,
        method: "GET",
        uri: "/handbook/Guide",
        headers: &[("x-test-account", CONTRIBUTOR)],
    },
    Scenario {
        id: "restore-form",
        identity: "restore-guide",
        status: 200,
        warm: &[],
        setup: clean,
        method: "GET",
        uri: "/handbook/-/restore/Guide",
        headers: &[("x-test-account", CONTRIBUTOR)],
    },
    Scenario {
        id: "restore-form-last-text",
        identity: "restore-guide",
        status: 200,
        warm: &[],
        setup: blocked,
        method: "GET",
        uri: "/handbook/-/restore/Guide",
        headers: &[("x-test-account", CONTRIBUTOR)],
    },
];
