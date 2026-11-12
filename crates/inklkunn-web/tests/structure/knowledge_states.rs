//! Every state of what a reader sees of the knowledge layer (handoff 005 K-T6; RFC 005 part 2 D-7), reached through
//! the real router over an in-memory repository: a page with a claimed paragraph (one claim, and two in one
//! paragraph), a claim's page in each of its states (sources of each relation and bearing, judged on an earlier
//! wording, each status, a deciding proposal this wiki does not hold, no source, no paragraph, a source whose file
//! cannot be read, in a view, in a view that is not there), an address that names no claim, and (K-T7) the page
//! *Why is this here?* leads to: a page's paragraphs with the changes that wrote them (people as the messages record
//! them, in a view, with a claim, with a proposal taken in and its review and sources, with one the records do not
//! tie to the change), a long history read only as far as the history page reads, a history nothing could be read
//! of, a page with no paragraphs, one with more paragraphs than are followed, and each page the wiki already has a
//! word for (no such page, two pages, no such view, a title that names nothing, prikk busy). Each is a page the
//! structural rules run on, in both languages.

use inklkunn_core::knowledge::{Bearing, Link, SourceKind, Standing};
use inklkunn_core::proposal::{NewComment, ProposalEdit};
use inklkunn_core::{IdentityAuthority, Principal};
use inklkunn_store::{DecisionKind, FileChange, Role};

use super::common::{
    World, accept_by_hand, add_person, busy, claim, home_with, link, op, page, person,
    proposal_message, put, script_history, shown, source,
};
use super::wiki_states::Scenario;

const SAID: &str = "The bridge opened in 1950.";
const PROPOSAL: &str = "01930000-aaaa-7000-8000-000000000001";

fn with_status(w: &World, standing: Standing) {
    put(
        w,
        &home_with(SAID),
        &[claim(1, standing, Some(PROPOSAL), SAID)],
        &[],
    );
}

/// A claim with a source of every relation and bearing, so that each phrase is a page the rules see.
fn sourced(w: &World) {
    let mut c = claim(1, Standing::Contested, Some(PROPOSAL), SAID);
    let mut sources = Vec::new();
    let mut n = 0;
    let kinds = [
        SourceKind::Primary,
        SourceKind::Secondary,
        SourceKind::Observational,
        SourceKind::Computational,
        SourceKind::Anecdotal,
        SourceKind::Other,
    ];
    for relation in [Link::Supports, Link::Challenges] {
        for bearing in [Bearing::Direct, Bearing::Partial, Bearing::Indirect] {
            n += 1;
            c.sources.push(link(n, relation, bearing, SAID));
        }
    }
    for (kind, n) in kinds.into_iter().zip(1..) {
        sources.push(source(n, kind));
    }
    put(w, &home_with(SAID), &[c], &sources);
}

fn reworded(w: &World) {
    let mut c = claim(1, Standing::Accepted, Some(PROPOSAL), SAID);
    c.sources = vec![link(1, Link::Supports, Bearing::Direct, SAID)];
    put(
        w,
        &home_with("The bridge was opened in 1950."),
        &[c],
        &[source(1, SourceKind::Primary)],
    );
}

fn two_in_a_paragraph(w: &World) {
    put(
        w,
        &home_with(SAID),
        &[
            claim(1, Standing::Contested, None, SAID),
            claim(2, Standing::Unresolved, None, SAID),
        ],
        &[],
    );
}

fn unreadable_source(w: &World) {
    let mut c = claim(1, Standing::Unknown, None, SAID);
    c.sources = vec![link(9, Link::Challenges, Bearing::Partial, SAID)];
    put(w, &home_with(SAID), &[c], &[]);
}

fn no_paragraph(w: &World) {
    put(
        w,
        "---\ntitle: Home\nid: page-home\n---\n\nThe bridge opened in 1950.\n",
        &[claim(1, Standing::Contested, None, SAID)],
        &[],
    );
}

const CLAIM: &str = "/handbook/-/claim/01930000-0000-7000-8000-000000000001";

pub(super) const SCENARIOS: &[Scenario] = &[
    // --- a page with claimed paragraphs
    Scenario {
        id: "claim-paragraph",
        identity: "wiki-page-home",
        status: 200,
        warm: &[],
        setup: sourced,
        method: "GET",
        uri: "/handbook/Home",
        headers: &[],
    },
    Scenario {
        id: "claim-paragraph-two-claims",
        identity: "wiki-page-home",
        status: 200,
        warm: &[],
        setup: two_in_a_paragraph,
        method: "GET",
        uri: "/handbook/Home",
        headers: &[],
    },
    // --- a claim's page
    Scenario {
        id: "claim-page",
        identity: "claim-page",
        status: 200,
        warm: &[],
        setup: sourced,
        method: "GET",
        uri: CLAIM,
        headers: &[],
    },
    Scenario {
        id: "claim-page-earlier-wording",
        identity: "claim-page",
        status: 200,
        warm: &[],
        setup: reworded,
        method: "GET",
        uri: CLAIM,
        headers: &[],
    },
    Scenario {
        id: "claim-page-accepted",
        identity: "claim-page",
        status: 200,
        warm: &[],
        setup: |w| with_status(w, Standing::Accepted),
        method: "GET",
        uri: CLAIM,
        headers: &[],
    },
    Scenario {
        id: "claim-page-unresolved",
        identity: "claim-page",
        status: 200,
        warm: &[],
        setup: |w| with_status(w, Standing::Unresolved),
        method: "GET",
        uri: CLAIM,
        headers: &[],
    },
    Scenario {
        id: "claim-page-not-decided",
        identity: "claim-page",
        status: 200,
        warm: &[],
        setup: |w| with_status(w, Standing::Unknown),
        method: "GET",
        uri: CLAIM,
        headers: &[],
    },
    Scenario {
        id: "claim-page-unreadable-source",
        identity: "claim-page",
        status: 200,
        warm: &[],
        setup: unreadable_source,
        method: "GET",
        uri: CLAIM,
        headers: &[],
    },
    Scenario {
        id: "claim-page-no-paragraph",
        identity: "claim-page",
        status: 200,
        warm: &[],
        setup: no_paragraph,
        method: "GET",
        uri: CLAIM,
        headers: &[],
    },
    Scenario {
        id: "claim-page-in-a-view",
        identity: "claim-page",
        status: 200,
        warm: &[],
        setup: sourced,
        method: "GET",
        uri: "/handbook/-/claim/01930000-0000-7000-8000-000000000001?view=main",
        headers: &[],
    },
    Scenario {
        id: "claim-page-no-such-view",
        identity: "wiki-no-such-view",
        status: 404,
        warm: &[],
        setup: sourced,
        method: "GET",
        uri: "/handbook/-/claim/01930000-0000-7000-8000-000000000001?view=nope",
        headers: &[],
    },
    Scenario {
        id: "claim-none",
        identity: "claim-none",
        status: 404,
        warm: &[],
        setup: sourced,
        method: "GET",
        uri: "/handbook/-/claim/01930000-0000-7000-8000-0000000000ff",
        headers: &[],
    },
    Scenario {
        id: "claim-none-not-an-id",
        identity: "claim-none",
        status: 404,
        warm: &[],
        setup: sourced,
        method: "GET",
        uri: "/handbook/-/claim/nothing",
        headers: &[],
    },
    // --- Why is this here? (K-T7)
    Scenario {
        id: "why-page",
        identity: "why-home",
        status: 200,
        warm: &[],
        setup: script_history,
        method: "GET",
        uri: "/handbook/-/why/Home",
        headers: &[],
    },
    Scenario {
        id: "why-page-in-a-view",
        identity: "why-home",
        status: 200,
        warm: &[],
        setup: script_history,
        method: "GET",
        uri: "/handbook/-/why/Home?view=main",
        headers: &[],
    },
    Scenario {
        id: "why-page-nothing-listed",
        identity: "why-home",
        status: 200,
        warm: &[],
        setup: |_| {},
        method: "GET",
        uri: "/handbook/-/why/Home",
        headers: &[],
    },
    Scenario {
        id: "why-page-with-a-claim",
        identity: "why-home",
        status: 200,
        warm: &[],
        setup: with_a_claim,
        method: "GET",
        uri: "/handbook/-/why/Home",
        headers: &[],
    },
    Scenario {
        id: "why-page-proposal-taken-in",
        identity: "why-home",
        status: 200,
        warm: &[],
        setup: |w| taken_in(w, true),
        method: "GET",
        uri: "/handbook/-/why/Home",
        headers: &[],
    },
    Scenario {
        id: "why-page-proposal-not-tied-to-the-change",
        identity: "why-home",
        status: 200,
        warm: &[],
        setup: |w| taken_in(w, false),
        method: "GET",
        uri: "/handbook/-/why/Home",
        headers: &[],
    },
    Scenario {
        id: "why-page-read-as-far-as-the-history-page-reads",
        identity: "why-home",
        status: 200,
        warm: &[],
        setup: long_home,
        method: "GET",
        uri: "/handbook/-/why/Home",
        headers: &[],
    },
    Scenario {
        id: "why-page-no-change-reached",
        identity: "why-home",
        status: 200,
        warm: &[],
        setup: long_guide,
        method: "GET",
        uri: "/handbook/-/why/Home",
        headers: &[],
    },
    Scenario {
        id: "why-page-work-spent",
        identity: "why-home",
        status: 200,
        warm: &[],
        setup: heavy_home,
        method: "GET",
        uri: "/handbook/-/why/Home",
        headers: &[],
    },
    Scenario {
        id: "why-page-work-spent-before-any-change",
        identity: "why-home",
        status: 200,
        warm: &[],
        setup: heaviest_home,
        method: "GET",
        uri: "/handbook/-/why/Home",
        headers: &[],
    },
    Scenario {
        id: "why-page-no-paragraphs",
        identity: "why-home",
        status: 200,
        warm: &[],
        setup: |w| w.fake.set("pages/Home.md", "---\ntitle: Home\n---\n"),
        method: "GET",
        uri: "/handbook/-/why/Home",
        headers: &[],
    },
    Scenario {
        id: "why-page-more-paragraphs-than-are-followed",
        identity: "why-home",
        status: 200,
        warm: &[],
        setup: |w| {
            let body: String = (0..430).map(|n| format!("Paragraph {n}.\n\n")).collect();
            w.fake.set("pages/Home.md", &page(&["title: Home"], &body));
        },
        method: "GET",
        uri: "/handbook/-/why/Home",
        headers: &[],
    },
    Scenario {
        id: "why-page-another-page",
        identity: "why-guide",
        status: 200,
        warm: &[],
        setup: script_history,
        method: "GET",
        uri: "/handbook/-/why/Guide",
        headers: &[],
    },
    Scenario {
        id: "why-page-no-such-page",
        identity: "wiki-missing",
        status: 404,
        warm: &[],
        setup: |_| {},
        method: "GET",
        uri: "/handbook/-/why/Not%20Written",
        headers: &[],
    },
    Scenario {
        id: "why-page-two-pages",
        identity: "wiki-ambiguous",
        status: 200,
        warm: &[],
        setup: |_| {},
        method: "GET",
        uri: "/handbook/-/why/Twin",
        headers: &[],
    },
    Scenario {
        id: "why-page-no-such-view",
        identity: "wiki-no-such-view",
        status: 404,
        warm: &[],
        setup: |_| {},
        method: "GET",
        uri: "/handbook/-/why/Home?view=nope",
        headers: &[],
    },
    Scenario {
        id: "why-page-invalid-title",
        identity: "wiki-invalid-title",
        status: 404,
        warm: &[],
        setup: |_| {},
        method: "GET",
        uri: "/handbook/-/why/A%2F%2FB",
        headers: &[],
    },
    Scenario {
        id: "why-page-unavailable",
        identity: "wiki-unavailable",
        status: 503,
        warm: &[],
        setup: |w| w.fake.fail_views(Some(busy())),
        method: "GET",
        uri: "/handbook/-/why/Home",
        headers: &[],
    },
];

// ------------------------------------------------------------------------------------ the setups of the why page

/// The principal of an account that has a session (the test identity).
fn who(id: &inklkunn_store::AccountId) -> Principal {
    Principal::session(&IdentityAuthority::for_tests(), id.clone())
}

/// `Home` as the history left it, with a claim said in its first paragraph.
fn with_a_claim(w: &World) {
    let said = "The bridge opened in 1950.";
    w.fake.say(1, "Create the handbook");
    w.fake.show(
        1,
        shown(vec![
            op("create-file", &["pages/Home.md"]),
            op("create-file", &["pages/Guide.md"]),
        ]),
    );
    put(
        w,
        &home_with(said),
        &[claim(1, Standing::Contested, None, said)],
        &[],
    );
    w.fake.say(2, "Add a claim");
    w.fake.show(
        2,
        shown(vec![op("create-file", &["knowledge/claims/c.yaml"])]),
    );
    w.fake.set(
        "pages/Home.md",
        &home_with("The bridge was opened in 1950."),
    );
    w.fake.say(3, "Reword the claim");
    w.fake
        .show(3, shown(vec![op("edit-text", &["pages/Home.md"])]));
}

/// A proposal that changes `Home`, with a comment, an approval and a source, accepted and taken into the history
/// as version 2 (`sealed`), or only named by version 2's message (not `sealed`).
fn taken_in(w: &World, sealed: bool) {
    let (contributor, reviewer, maintainer) = (person(1), person(3), person(4));
    add_person(w, &contributor, "Aiko Tanaka", Some(Role::Contributor));
    add_person(w, &reviewer, "Rin Sato", Some(Role::Reviewer));
    add_person(w, &maintainer, "Mika Ito", Some(Role::Maintainer));
    let service = &w.services.proposals;
    let home = page(
        &["title: Home", "description: Start here"],
        "# Welcome\n\nSee [[Guide]] and [[Nowhere]].\n\n## Background\n\nMore.\n",
    );
    let better = home.replace("More.", "More, and better.");
    let draft = service
        .create(
            &who(&contributor),
            &w.wiki,
            None,
            ProposalEdit {
                summary: "Improve the background".to_owned(),
                sources: vec!["A book, page 12".to_owned()],
                files: vec![FileChange {
                    path: "pages/Home.md".to_owned(),
                    text: Some(better.clone()),
                }],
                ..ProposalEdit::default()
            },
        )
        .unwrap();
    service
        .submit(&who(&contributor), &w.wiki, &draft.id)
        .unwrap();
    service
        .comment(
            &who(&reviewer),
            &w.wiki,
            &draft.id,
            NewComment {
                body: "Looks fine to me".to_owned(),
                ..NewComment::default()
            },
        )
        .unwrap();
    service
        .decide(
            &who(&reviewer),
            &w.wiki,
            &draft.id,
            1,
            DecisionKind::Approve,
            "Ready to take in",
        )
        .unwrap();
    if sealed {
        accept_by_hand(w, &draft.id, &maintainer, &contributor);
    }
    w.fake.say(1, "Create the handbook");
    w.fake.show(
        1,
        shown(vec![
            op("create-file", &["pages/Home.md"]),
            op("create-file", &["pages/Guide.md"]),
        ]),
    );
    w.fake.set("pages/Home.md", &better);
    let message = proposal_message("Improve the background", "Aiko Tanaka", "Mika Ito")
        .replace("019a0000-0000-7000-8000-00000000aaaa", draft.id.as_str())
        .replace("019a0000-0000-7000-8000-0000000000a1", contributor.as_str())
        .replace("019a0000-0000-7000-8000-0000000000a2", maintainer.as_str());
    w.fake.say(2, &message);
    w.fake
        .show(2, shown(vec![op("edit-text", &["pages/Home.md"])]));
}

/// Forty more edits of `Home`, so that its history is read only as far as the history page reads.
fn long_home(w: &World) {
    script_history(w);
    for v in 5..=44_u64 {
        w.fake.set(
            "pages/Home.md",
            &page(
                &["title: Home", "description: Start here"],
                &format!("# Welcome\n\nEdit {v}.\n\nA paragraph that stays as it is.\n"),
            ),
        );
        w.fake.say(v, "Edit the home page");
        w.fake
            .show(v, shown(vec![op("edit-text", &["pages/Home.md"])]));
    }
}

/// Thirty edits of a page of about 600 KB: the work one request may spend is gone after a few of them.
fn heavy_home(w: &World) {
    script_history(w);
    for v in 5..=34_u64 {
        let body: String = (0..40_000)
            .map(|n| {
                if n == 3 {
                    format!("Paragraph {n} edit {v}.\n\n")
                } else {
                    format!("Paragraph {n}.\n\n")
                }
            })
            .collect();
        w.fake.set("pages/Home.md", &page(&["title: Home"], &body));
        w.fake.say(v, "Edit the home page");
        w.fake
            .show(v, shown(vec![op("edit-text", &["pages/Home.md"])]));
    }
}

/// One edit of a page whose two versions have two hundred stretches of five hundred paragraphs against five
/// hundred, none of whose text is unique: the work of the first step alone is more than a request may spend.
fn heaviest_home(w: &World) {
    script_history(w);
    for (v, word) in [(5_u64, "x"), (6, "y")] {
        let body: String = (0..200)
            .map(|n| format!("{}Anchor {n}.\n\n", format!("{word}\n\n").repeat(500)))
            .collect();
        w.fake.set("pages/Home.md", &page(&["title: Home"], &body));
        w.fake.say(v, "Edit the home page");
        w.fake
            .show(v, shown(vec![op("edit-text", &["pages/Home.md"])]));
    }
}

/// Forty changes to the guide, so that the changes one request reads do not reach `Home`'s.
fn long_guide(w: &World) {
    script_history(w);
    for v in 5..=44_u64 {
        w.fake.set(
            "pages/Guide.md",
            &page(&["title: Guide"], &format!("v{v}.\n")),
        );
        w.fake.say(v, "Edit the guide");
        w.fake
            .show(v, shown(vec![op("edit-text", &["pages/Guide.md"])]));
    }
}
