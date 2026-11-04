//! Every state of what a reader sees of the knowledge layer (handoff 005 K-T6; RFC 005 part 2 D-7), reached through
//! the real router over an in-memory repository: a page with a claimed paragraph (one claim, and two in one
//! paragraph), a claim's page in each of its states (sources of each relation and bearing, judged on an earlier
//! wording, each status, a deciding proposal this wiki does not hold, no source, no paragraph, a source whose file
//! cannot be read, in a view, in a view that is not there), an address that names no claim, and the page that
//! *Why is this here?* leads to (for a page, in a view, for a title no page has). Each is a page the structural
//! rules run on, in both languages.

use inklkunn_core::knowledge::{Bearing, Link, SourceKind, Standing};

use super::common::{World, claim, home_with, link, put, source};
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
    // --- Why is this here? (not available in this release)
    Scenario {
        id: "why-page",
        identity: "why-page",
        status: 501,
        warm: &[],
        setup: |_| {},
        method: "GET",
        uri: "/handbook/-/why/Home",
        headers: &[],
    },
    Scenario {
        id: "why-page-in-a-view",
        identity: "why-page",
        status: 501,
        warm: &[],
        setup: |_| {},
        method: "GET",
        uri: "/handbook/-/why/Home?view=main",
        headers: &[],
    },
    Scenario {
        id: "why-page-no-such-title",
        identity: "why-page",
        status: 501,
        warm: &[],
        setup: |_| {},
        method: "GET",
        uri: "/handbook/-/why/Nothing%20here",
        headers: &[],
    },
];
