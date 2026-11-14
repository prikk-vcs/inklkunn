//! The knowledge layer's endpoints, as the document describes them (RFC 005 part 2 D-12; handoff 005 K-T11): the read
//! API gains a claim, a source, a page's paragraph-by-paragraph provenance and a comparison of two views; the write API
//! gains the curation proposal. Beside the others and held to the route table the same way (a path in one and not the
//! other fails the build of the document). Every one is authorized as the pages are (`Read`, or `Propose` for the write)
//! and existence-neutral: a wiki that is absent and one the caller may not read are one `not-found`, and a claim or a
//! source the wiki does not hold is the same `not-found` as an id that names nothing.

use inklkunn_core::Action;

use super::{Answer, Base, Doc, Kind, PAGE_PROBLEMS, Param, VIEW_ONLY, Write, text};
use crate::api::problem::Code;

/// The most bytes of a curation proposal's body: its summary, a note on each of up to 256 links and sources, as JSON.
pub(crate) const CURATION_BODY_LIMIT: usize = 64 * 1024;

const VIEW_A: Param = text(
    "a",
    100,
    false,
    "The first view to compare: a view's name (for example `main`). Without it, the wiki's default view.",
);

const VIEW_B: Param = Param {
    name: "b",
    kind: Kind::Text { max: 100 },
    required: true,
    missing: Code::InvalidParameter,
    description: "The second view to compare: a view's name. Required. The two views are treated alike: the comparison says where they differ and nothing about which is right.",
};

const COMPARE_PARAMS: &[Param] = &[VIEW_A, VIEW_B];

pub(crate) const CLAIM: Doc = Doc {
    path: "/{slug}/-/api/v1/claims/{id}",
    operation_id: "getClaim",
    tag: "knowledge",
    summary: "Get a claim: its kind, where it is said, its sources and its status",
    description: "A claim as the view's tip has it, from the derived knowledge index: what kind of statement it is, where it is said (the page and the paragraph's `^id`, and whether the paragraph is there now), each source it cites with how the source bears on it (`supports` or `challenges`, `direct`, `partial` or `indirect`: how directly it speaks to the claim, never how true either is) and whether that was judged on the wording the claim has now (`judged_on`: `current`, `earlier` or `none`), its relations to other claims and its community status with the proposal that decided it. No number, rank or verdict is in it. `status.proposal` is set only when the caller may read that proposal; `status.proposal_unlisted` says a proposal decided it and the caller may not. An id that names no claim of the view is `not-found`, as an id that is not an id.",
    query: VIEW_ONLY,
    answer: Answer::Schema("Claim"),
    write: None,
    problems: &[Code::InvalidParameter, Code::NoSuchView],
};

pub(crate) const SOURCE: Doc = Doc {
    path: "/{slug}/-/api/v1/sources/{id}",
    operation_id: "getSource",
    tag: "knowledge",
    summary: "Get a source: what it says of itself and the claims that cite it",
    description: "A source as the view's tip has it: its kind, whether it can be had as a person who checked says (`availability`), the day a person last reviewed it as written, the source that replaces it, and what it says of itself (title, author or publisher, date and where it is found, each as written and cut at 300 characters: untrusted text), with the claims that cite it (at most 50, `claims_total` counts all) and how it bears on each. Nothing is fetched: the wiki never follows a source's address. An id that names no source of the view is `not-found`.",
    query: VIEW_ONLY,
    answer: Answer::Schema("Source"),
    write: None,
    problems: &[Code::InvalidParameter, Code::NoSuchView],
};

pub(crate) const PROVENANCE: Doc = Doc {
    path: "/{slug}/-/api/v1/provenance/{*title}",
    operation_id: "getProvenance",
    tag: "knowledge",
    summary: "Say, for each paragraph of a page, which change last changed it and which first wrote it",
    description: "*Why is this here?* as data: the page's paragraphs in order (at most 500; `left_out` counts the rest), each with the change that last changed it and the one that first wrote it, as numbers of `changes`. A paragraph with an `^id` is followed through that marker (`basis`: `anchor`); any other is traced by matching its text (`text`), and what was found is the earliest wording found, not what happened. `reach` says how far back the history was read: `whole`, `bounded` (the history page's bounds ended the reading), `spent` (the work bound did) or `unfollowed` (an earlier version could not be read). The changes are the ones the paragraphs name, newest first, each with what its message says of the people (as the history endpoints do); a change made from a proposal the acceptance records tie to it, which the caller may read, also carries that proposal's id, summary and the sources given with it. It is `pages/{title}/provenance` of the plan, at `provenance/{title}` because `pages/{title}` takes a title with a `/` in it, as `history/{title}` does.",
    query: VIEW_ONLY,
    answer: Answer::Schema("PageProvenance"),
    write: None,
    problems: PAGE_PROBLEMS,
};

pub(crate) const COMPARE_VIEWS: Doc = Doc {
    path: "/{slug}/-/api/v1/views/compare",
    operation_id: "compareViews",
    tag: "knowledge",
    summary: "Compare two views of the wiki: the claims and the pages where they differ",
    description: "Where two views differ, claim by claim (a claim only one has, or that both have and word, status or sources differ) and page by page (added, removed, modified, with the lines changed). Only differences are stated, and none is a verdict: the two sides are `a` and `b` in the order given. At most 200 claims and 200 pages are listed; `claims_omitted` and `pages_omitted` count the rest. A claim only one view has is `only-a` or `only-b`, and one whose file the other view holds unreadable is `unreadable-in-a` or `unreadable-in-b`, which is not the same as having none. Two names that are not two open views of the wiki are `no-such-view`; the same view twice is `invalid-parameter`.",
    query: COMPARE_PARAMS,
    answer: Answer::Schema("ViewComparison"),
    write: None,
    problems: &[Code::InvalidParameter, Code::NoSuchView],
};

pub(crate) const PROPOSE_CURATION: Doc = Doc {
    path: "/{slug}/-/api/v1/proposals/curation",
    operation_id: "proposeCuration",
    tag: "proposals",
    summary: "Propose a curation of a claim: its status, its links to sources, and the sources it cites that were checked",
    description: "Opens a curation proposal (RFC 005 part 2 D-3): a proposal of its own, with no page text, and the only kind that sets a claim's community status, how a source bears on it (`relation`, `bearing`) or that a link goes (`remove`), the day a person last checked a source (`reviewed`) and whether it can be had (`availability`) and, for a source that was checked, who and what it is (`title`, `author`, `date`), where it is found (`reference`) and which source replaces it (`superseded_by`). It is a proposal like any other: held for review, accepted by someone who may, and the status it sets names it. Members left out are left as the base has them, in a source as in the body (`reviewed: \"\"` clears the day and `availability: unknown` clears the answer; left out they stay); a member that equals what the base has changes nothing, and each part of what a source is (`title`, `author`, `date`, `reference`, `superseded_by`) is compared and replaced on its own, and a body that changes nothing is `unacceptable`. `confirm: true` judges the sources and the status again on the wording the claim has in the base (the re-confirmation). Whatever is set is judged on the wording the claim has in the base, so the claim must be said in a paragraph (`no-wording`). A `reviewed` day that has not come (after the current date in UTC+14) is refused before any proposal exists, as `day-not-come`, naming the member (`sources[i].reviewed`) and the `source`; a day the base already has is left as it is. The write names the view's tip in `If-Match`, as a new proposal does. `submit: false` keeps a draft; `submit: true` sends it for review at once. Scope `propose`; the quotas of proposals refuse by name.",
    query: &[],
    answer: Answer::Schema("Proposal"),
    write: Some(Write {
        action: Action::Propose,
        request: "NewCuration",
        base: Base::View,
        success: 201,
        limit: CURATION_BODY_LIMIT,
    }),
    problems: &[
        Code::Unacceptable,
        Code::NoSuchView,
        Code::RefusedLimit,
        Code::NotInThisState,
    ],
};
