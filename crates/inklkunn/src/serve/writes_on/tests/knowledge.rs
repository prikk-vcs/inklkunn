#![allow(clippy::expect_used, clippy::unwrap_used)]
//! R-2 (0.3.0 readiness), end to end: the knowledge layer of RFC 005 part 2 on the real server over a real prikk,
//! a real key store and a real SQLite store, behind a real socket, driven the way U6 drives it (the same harness:
//! the invitation machinery, the sign-in page, forms read off the pages with their CSRF tokens). Nobody is
//! given a header-named identity and no record is written but by the server.
//!
//! One story is told, and each assertion names the acceptance criterion of RFC 005 part 2 ("What `done` means")
//! it carries as `AC D-n` in its message:
//!
//! - a contributor proposes a claim with a source through the forms, and while the proposal is held nobody
//!   reads a claim (D-3); a reviewer accepts it; a reader with no account sees the claim and its source (D-1,
//!   D-2, D-7);
//! - *Why is this here?* answers for the claim's paragraph and for an ordinary one (D-8);
//! - a reviewer creates a view; a change of wording, of status and of sources lands in it; *compare views* shows
//!   the three differences; and the claim reads as judged on an earlier wording in the view that rewrote it
//!   (D-1, D-9);
//! - three proposals written against one tip: a status, a second source and a status again; the first two merge
//!   and the third overlaps and writes nothing; the status stays what the accepted proposal made it (D-3, D-4);
//! - a file the claim rules refuse is refused at acceptance, by name, and nothing is written (D-5);
//! - *Sources to review* lists a source after the operator's `review_after` and not one reviewed today (D-10);
//! - a thread is written and is in the export, and the exported bundle validates against wiki-commons' schema
//!   with the extension declared (D-11, D-12).
//!
//! Needs prikk and prints `skipping: prikk not on PATH` without it.

use super::api::{API, api, json_field, json_quote};
use super::*;

// The core's own checker and the copy of wiki-commons' schema its export tests use: one checker, one schema.
#[path = "../../../../../inklkunn-core/tests/support/json_schema.rs"]
mod json_schema;

/// wiki-commons' bundle manifest schema, the copy the export tests of the core check against.
const MANIFEST_SCHEMA: &str =
    include_str!("../../../../../inklkunn-core/tests/fixtures/bundle-manifest.schema.json");

/// The paragraph of the starter page that is made a claim: part 4 of *Help*, the one under *Reading*.
const CLAIMED: usize = 3;

/// A source as the form asks for it.
pub(super) struct Cited<'a> {
    kind: &'a str,
    title: &'a str,
    author: &'a str,
    date: &'a str,
    reference: &'a str,
    bearing: &'a str,
    note: &'a str,
}

pub(super) const SURVEY: Cited = Cited {
    kind: "secondary",
    title: "A survey of wiki link syntax",
    author: "R. Example",
    date: "2019",
    reference: "https://example.org/link-syntax",
    bearing: "direct",
    note: "chapter 2",
};

pub(super) const FIELD_NOTES: Cited = Cited {
    kind: "observational",
    title: "Field notes on red links",
    author: "S. Example",
    date: "2021",
    reference: "https://example.org/red-links",
    bearing: "partial",
    note: "the second table",
};

/// What a reader reads of a page: between `<main` and `</main>`, without the page's frame.
pub(super) fn main_of(page: &Reply) -> &str {
    let after = page.body.split_once("<main").map_or("", |p| p.1);
    after.split_once("</main>").map_or(after, |p| p.0)
}

/// The words a page about knowledge must never use as a verdict (D-7), found as whole words in what a reader reads
/// (the page's own paragraphs, which *Why is this here?* quotes, are not the product's words).
fn retired_words_in(page: &Reply) -> Vec<&'static str> {
    let mut text = main_of(page).to_ascii_lowercase();
    while let Some((before, after)) = text.split_once("<p class=\"why-excerpt\">") {
        let rest = after.split_once("</p>").map_or("", |p| p.1);
        text = format!("{before}{rest}");
    }
    // The words a reader reads, not the markup's own (`aria-hidden="true"`).
    let mut plain = String::new();
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                plain.push(' ');
            }
            c if !in_tag => plain.push(c),
            _ => {}
        }
    }
    let words: Vec<&str> = plain
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|w| !w.is_empty())
        .collect();
    [
        "true",
        "false",
        "verified",
        "proven",
        "canonical",
        "official",
        "adopted",
    ]
    .into_iter()
    .filter(|w| words.contains(w))
    .collect()
}

/// Every id of the shape of a UUID that follows `marker` in `html`, in order of appearance, once each.
pub(super) fn ids_after(html: &str, marker: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for chunk in html.split(marker).skip(1) {
        let id: String = chunk
            .chars()
            .take_while(|c| c.is_ascii_hexdigit() || *c == '-')
            .collect();
        if id.len() == 36 && !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// The text of the paragraph (`<p ...>` to `</p>`) of `html` that holds `needle`, without its opening tag.
fn paragraph_with<'a>(html: &'a str, needle: &str) -> &'a str {
    let at = html.find(needle).expect("the page has the marker");
    let open = html
        .get(..at)
        .unwrap()
        .rfind("<p")
        .expect("inside a paragraph");
    let inner = html.get(open..).unwrap();
    let inner = inner.split_once('>').expect("the tag ends").1;
    inner.split_once("</p>").map_or(inner, |p| p.0)
}

/// A reviewer's approval and the maintainer's acceptance of a proposal: the accept page's answer.
pub(super) fn approve_and_accept(maya: &mut Person, id: &str) -> Reply {
    let page = maya.browser.get(&proposal_page(id));
    let mut form = form_to(&page, &format!("{}/decide", proposal_page(id)));
    form.set("body", "Read, and it says what it says.");
    let decided = maya.browser.submit(&form, Some(("op", "approve")));
    assert_eq!(decided.status, 303, "{}\n{}", decided.head, decided.body);
    let page = maya.browser.get(&proposal_page(id));
    let form = form_to(&page, &format!("{}/accept", proposal_page(id)));
    maya.browser.submit(&form, None)
}

pub(super) fn signed_into_history(accepted: &Reply) {
    assert_eq!(accepted.status, 200, "{}\n{}", accepted.head, accepted.body);
    assert!(
        accepted.body.contains("Signed into the history"),
        "{}",
        accepted.body
    );
}

/// *Attach a source* (or *Challenge this*) to part 4 of *Help*, in `view`, through the form; the id of the proposal.
pub(super) fn attach(
    person: &mut Person,
    view: Option<&str>,
    relation: &str,
    cited: &Cited,
    summary: &str,
) -> String {
    let mut uri = format!(
        "/handbook/-/proposals/new?page=Help&part=block&ordinal={CLAIMED}&attach={relation}"
    );
    if let Some(view) = view {
        uri.push_str(&format!("&view={view}"));
    }
    let page = person.browser.get(&uri);
    assert_eq!(page.status, 200, "{uri}: {}", page.body);
    let mut form = form_to(&page, "/handbook/-/proposals/new");
    form.set("a_kind", cited.kind)
        .set("a_title", cited.title)
        .set("a_author", cited.author)
        .set("a_date", cited.date)
        .set("a_ref_form", "url")
        .set("a_ref", cited.reference)
        .set("a_bearing", cited.bearing)
        .set("a_note", cited.note)
        .set("summary", summary);
    // A paragraph that is a claim already is not asked what kind of statement it is.
    if form.fields.iter().any(|f| f.name == "a_claim_kind") {
        form.set("a_claim_kind", "observation");
    }
    let sent = person.browser.submit(&form, Some(("op", "submit")));
    assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
    proposal_id(&sent.location())
}

/// Rewrite part 4 of *Help* in `view` through the ordinary part form.
fn reword(
    person: &mut Person,
    view: Option<&str>,
    edit: impl Fn(&str) -> String,
    summary: &str,
) -> String {
    let mut uri = format!("/handbook/-/proposals/new?page=Help&part=block&ordinal={CLAIMED}");
    if let Some(view) = view {
        uri.push_str(&format!("&view={view}"));
    }
    let page = person.browser.get(&uri);
    assert_eq!(page.status, 200, "{uri}: {}", page.body);
    let mut form = form_to(&page, "/handbook/-/proposals/new");
    let text = edit(form.get("p0_text"));
    form.set("p0_text", &text).set("summary", summary);
    let sent = person.browser.submit(&form, Some(("op", "submit")));
    assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
    proposal_id(&sent.location())
}

/// The curation form of `claim` in `view`, changed by `edit`, sent for review; the id of the proposal.
pub(super) fn curate(
    person: &mut Person,
    claim: &str,
    view: Option<&str>,
    edit: impl Fn(&mut Form),
    summary: &str,
) -> String {
    let mut uri = format!("/handbook/-/proposals/curate?claim={claim}");
    if let Some(view) = view {
        uri.push_str(&format!("&view={view}"));
    }
    let page = person.browser.get(&uri);
    assert_eq!(page.status, 200, "{uri}: {}", page.body);
    let mut form = form_to(&page, "/handbook/-/proposals/curate");
    edit(&mut form);
    form.set("summary", summary);
    let sent = person.browser.submit(&form, Some(("op", "submit")));
    assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
    proposal_id(&sent.location())
}

/// The index of the group of fields of the curation form that is for `source`.
fn group_of(form: &Form, source: &str) -> usize {
    (0..8)
        .find(|i| {
            form.fields
                .iter()
                .any(|f| f.name == format!("c{i}_source") && f.value == source)
        })
        .unwrap_or_else(|| panic!("the form has no source {source}"))
}

#[test]
fn the_knowledge_layer_works_end_to_end_on_a_real_prikk() {
    let Some(world) = World::new("knowledge") else {
        return;
    };
    let today = utc_minutes(now()).get(..10).unwrap().to_owned();
    let running = Running::start(&world);
    let mut anonymous = running.browser();
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
    let mut quinn = join(&world, &running, "quinn", "Quinn Reviewer", Role::Reviewer);
    let first_blocks = world.blocks();

    // ---- AC D-7: a wiki with no claim reads as it did: no control for a claim, one *Why is this here?* link
    let help = anonymous.get(HELP);
    assert_eq!(help.status, 200, "{}", help.body);
    assert_eq!(
        help.body.matches("class=\"claim-link\"").count(),
        0,
        "AC D-7: no paragraph carries a claim control before there is a claim\n{}",
        help.body
    );
    assert_eq!(
        help.body.matches(">Why is this here?</a>").count(),
        1,
        "AC D-7: one *Why is this here?* link among the page's tools\n{}",
        help.body
    );

    // ---- a contributor proposes a claim with a source through the forms (the claim's id is made by the form)
    let first = attach(&mut noor, None, "supports", &SURVEY, "Cite the link syntax");
    let held = page_text(&mut noor.browser, &proposal_page(&first));
    assert!(
        held.contains("Records the claim") && held.contains("A survey of wiki link syntax"),
        "the proposal's page names the claim and the source it records: {held}"
    );
    // AC D-3: while the proposal is held, no claim exists for any reader and nothing was written
    let help = anonymous.get(HELP);
    assert_eq!(
        help.body.matches("class=\"claim-link\"").count(),
        0,
        "AC D-3: an open proposal changed nothing a reader sees"
    );
    assert_eq!(
        world.blocks(),
        first_blocks,
        "AC D-3: an open proposal wrote nothing"
    );

    // ---- a reviewer accepts it; the reader with no account sees the claim and its source
    signed_into_history(&approve_and_accept(&mut maya, &first));
    assert_eq!(world.blocks().len(), first_blocks.len() + 1);
    let help = anonymous.get(HELP);
    assert_eq!(
        help.body.matches("class=\"claim-link\"").count(),
        1,
        "AC D-7: one quiet link\n{}",
        help.body
    );
    // ... and it is on part 4, the paragraph under *Reading* the claim was proposed on, and on no other.
    let paragraph = paragraph_with(&help.body, "class=\"claim-link\"");
    assert!(
        paragraph.starts_with("Pages link to each other with double brackets")
            && help
                .body
                .contains("<h3 id=\"h-reading\">Reading</h3>\n<p id=\"h-^k")
            && !paragraph.contains("A change is proposed"),
        "AC D-7: the link is on part {} of Help (the paragraph under Reading): {paragraph}",
        CLAIMED + 1
    );
    assert_eq!(
        help.body.matches(">Why is this here?</a>").count(),
        1,
        "AC D-7: still one *Why is this here?* link"
    );
    let claim = ids_after(&help.body, "/handbook/-/claim/")
        .into_iter()
        .next()
        .expect("the claim link names the claim");
    let claim_page = format!("/handbook/-/claim/{claim}");
    let seen = anonymous.get(&claim_page);
    assert_eq!(seen.status, 200, "{}", seen.body);
    let body = main_of(&seen);
    for needed in [
        "What kind of statement it is: an observation.",
        "A survey of wiki link syntax",
        "R. Example",
        "dated 2019",
        "supports, directly",
        "https://example.org/link-syntax",
        "This claim has not been decided in this view.",
        "Where it is said",
    ] {
        assert!(
            body.contains(needed),
            "AC D-1, D-2, D-7: the claim page says `{needed}`:\n{body}"
        );
    }
    assert!(
        !body.contains("Judged on an earlier wording") && !body.contains("Other views"),
        "AC D-1, D-7: a claim read as written, in the one view there is: {body}"
    );
    assert!(
        retired_words_in(&seen).is_empty(),
        "AC D-7: the claim page uses {:?}",
        retired_words_in(&seen)
    );
    let source_one = ids_after(&seen.body, "id=\"source-")
        .into_iter()
        .next()
        .expect("the source's line");

    // ---- AC D-12: the read API answers for the same claim, to the same reader
    let read = wire(
        running.port,
        "GET",
        &format!("/handbook/-/api/v1/claims/{claim}"),
        &[("Accept".into(), "application/json".into())],
        &[],
    );
    assert_eq!(read.status, 200, "AC D-12: {}\n{}", read.head, read.body);
    assert!(
        read.body.contains(&claim) && read.body.contains(&source_one),
        "AC D-12: the API names the claim and its source: {}",
        read.body
    );

    // ---- AC D-8: *Why is this here?* for the claim's paragraph and for an ordinary one
    let why = anonymous.get("/handbook/-/why/Help");
    assert_eq!(why.status, 200, "{}", why.body);
    let mine = main_of(&why);
    let claimed = mine
        .split("<li id=\"w-")
        .find(|chunk| chunk.contains(&claim))
        .expect("the claim's paragraph is on the page");
    assert!(
        claimed.contains("Pages link to each other")
            && claimed.contains("Latest change found in")
            && claimed.contains("Earliest wording found in")
            && claimed.contains("Sources and views"),
        "AC D-8: the claim's paragraph (traced by matching its text: the paragraph gained its `^id` in the very change \
         that made it a claim, so the `^id` chain is `tests/why_page/`'s), its changes and its way in to the claim: {claimed}"
    );
    let ordinary = mine
        .split("<li id=\"w-")
        .find(|chunk| chunk.contains("A change is proposed, reviewed, and accepted"))
        .expect("an ordinary paragraph is on the page");
    assert!(
        ordinary.contains("Earliest wording found in")
            && ordinary.contains("Traced by matching the text")
            && !ordinary.contains("Sources and views"),
        "AC D-8: an ordinary paragraph is traced and labelled as traced by matching the text: {ordinary}"
    );
    assert!(
        mine.contains("Accepted by Maya Maintainer")
            && mine.contains("Proposed by Noor Contributor")
            && mine.contains("Maya Maintainer approved")
            && mine.contains("Read, and it says what it says."),
        "AC D-8: the change's people and its review: {mine}"
    );
    assert!(
        retired_words_in(&why).is_empty(),
        "AC D-7: the *why* page uses {:?}",
        retired_words_in(&why)
    );

    // ---- a view: only a reviewer and above creates one; the contributor is offered nothing
    let refused = noor.browser.get("/handbook/-/views/new");
    assert_eq!(refused.status, 403, "{}", refused.body);
    let page = quinn.browser.get("/handbook/-/views/new");
    let mut form = form_to(&page, "/handbook/-/views/new");
    form.set("name", "reading")
        .check("kind", "interpretation")
        .set("from", "main");
    let made = quinn.browser.submit(&form, None);
    assert_eq!(made.status, 303, "{}\n{}", made.head, made.body);
    let views = page_text(&mut anonymous, "/handbook/-/views");
    assert!(
        views.contains("the view “reading”")
            && views.contains("Interpretation")
            && views.contains("A competing understanding, kept beside the default view.")
            && views.contains("Forked from the default view."),
        "AC D-9: the view is listed with its kind and where it was forked from, beside the default view: {views}"
    );
    // Before anything differs, the claim says no other view reads it differently.
    let seen = anonymous.get(&claim_page);
    assert!(
        main_of(&seen).contains("No other view reads this claim differently."),
        "AC D-9: {}",
        main_of(&seen)
    );

    // ---- a change of wording, of status and of sources lands in the view
    let reworded = reword(
        &mut noor,
        Some("reading"),
        |t| t.replacen("double brackets", "doubled square brackets", 1),
        "Read the brackets another way",
    );
    signed_into_history(&approve_and_accept(&mut maya, &reworded));
    let in_view = anonymous.get(&format!("{claim_page}?view=reading"));
    assert!(
        main_of(&in_view).contains("Judged on an earlier wording."),
        "AC D-1: in the view that rewrote the paragraph the source is shown as judged on an earlier wording: {}",
        main_of(&in_view)
    );
    assert!(
        !main_of(&anonymous.get(&claim_page)).contains("Judged on an earlier wording."),
        "AC D-1: and the default view, whose wording did not change, is not"
    );
    // One step judges again, on the wording the claim has now, everything that was judged on the earlier one.
    let contested = curate(
        &mut noor,
        &claim,
        Some("reading"),
        |f| {
            f.set("status", "contested").check("confirm", "1");
        },
        "Say that this is contested here, on the new wording",
    );
    signed_into_history(&approve_and_accept(&mut maya, &contested));
    assert!(
        !main_of(&anonymous.get(&format!("{claim_page}?view=reading")))
            .contains("Judged on an earlier wording."),
        "AC D-1: the reviewer's confirmation cleared it, in that one step"
    );
    let challenge = attach(
        &mut noor,
        Some("reading"),
        "challenges",
        &FIELD_NOTES,
        "A source that speaks against it",
    );
    signed_into_history(&approve_and_accept(&mut maya, &challenge));

    let in_view = anonymous.get(&format!("{claim_page}?view=reading"));
    let body = main_of(&in_view);
    assert!(
        body.contains("This claim is contested in this view.")
            && body.contains("challenges, in part")
            && body.contains("supports, directly"),
        "AC D-9: the claim as the view reads it: {body}"
    );
    let default = anonymous.get(&claim_page);
    let body = main_of(&default);
    assert!(
        body.contains("This claim has not been decided in this view.")
            && !body.contains("challenges")
            && body.contains("Other views")
            && body.contains("It is worded differently.")
            && body.contains(
                "Its status: not decided in the default view, contested in the view “reading”."
            )
            && body.contains("Its sources differ.")
            && body.contains("Compare with the view “reading”"),
        "AC D-9: the default view is unchanged and says where else the claim reads differently: {body}"
    );

    // ---- compare views: wording, status and sources
    let compared = anonymous.get("/handbook/-/views/compare?a=main&b=reading");
    assert_eq!(compared.status, 200, "{}", compared.body);
    let body = main_of(&compared);
    for needed in [
        "Comparing the default view and the view “reading”",
        "A claim said in Help",
        "It is worded differently.",
        "Its status: not decided in the default view, contested in the view “reading”.",
        "Its sources differ.",
        "Pages that differ",
        "How Help differs",
    ] {
        assert!(
            body.contains(needed),
            "AC D-9: compare views says `{needed}`:\n{body}"
        );
    }
    let page_diff = anonymous.get("/handbook/-/views/compare/Help?a=main&b=reading");
    assert_eq!(page_diff.status, 200, "{}", page_diff.body);
    assert!(
        main_of(&page_diff).contains("<mark>double</mark> brackets")
            && main_of(&page_diff).contains("<mark> doubled</mark> <mark>square</mark> brackets"),
        "AC D-9: the page's own difference is shown, line by line: {}",
        main_of(&page_diff)
    );
    for page in [&compared, &page_diff, &in_view, &default] {
        assert!(
            retired_words_in(page).is_empty(),
            "AC D-7, D-9: *canonical* and its kin appear nowhere a reader sees: {:?}",
            retired_words_in(page)
        );
    }

    // ---- three proposals written against one tip: a status, a second source and a status again
    let status_a = curate(
        &mut noor,
        &claim,
        None,
        |f| {
            f.set("status", "contested");
        },
        "Say that this is contested",
    );
    let more = attach(
        &mut quinn,
        None,
        "challenges",
        &FIELD_NOTES,
        "A source that speaks against it",
    );
    let status_b = curate(
        &mut quinn,
        &claim,
        None,
        |f| {
            f.set("status", "unresolved");
        },
        "Say that this is unresolved",
    );
    signed_into_history(&approve_and_accept(&mut maya, &status_a));
    let after_status = world.blocks();
    // AC D-4: the second was written before the first was accepted; its files merge key by key, no relation is lost
    signed_into_history(&approve_and_accept(&mut maya, &more));
    let after_source = world.blocks();
    assert_eq!(
        after_source.len(),
        after_status.len() + 1,
        "AC D-4: the merged proposal is one new block"
    );
    let seen = anonymous.get(&claim_page);
    let body = main_of(&seen);
    assert!(
        body.contains("This claim is contested in this view.")
            && body.contains("supports, directly")
            && body.contains("challenges, in part"),
        "AC D-3, D-4: the status the first proposal made and both sources, none lost: {body}"
    );
    // AC D-4: the third changes the status the first changed: an overlap, both texts, never resolved; nothing is written
    let overlap = approve_and_accept(&mut maya, &status_b);
    assert_eq!(overlap.status, 409, "{}\n{}", overlap.head, overlap.body);
    assert!(
        overlap.body.contains("Stopped: changes overlap"),
        "AC D-4: {}",
        overlap.body
    );
    assert_eq!(
        world.blocks(),
        after_source,
        "AC D-4: an overlap writes nothing"
    );
    let stopped = page_text(&mut quinn.browser, &proposal_page(&status_b));
    assert!(
        stopped.contains("Needs a revision") || stopped.contains("needs a revision"),
        "AC D-4: {stopped}"
    );
    // Both texts: the wiki's now (the accepted status) and the proposal's (the one it wanted), in the claim's file.
    let theirs = stopped.find("<td>status: contested\n");
    let ours = stopped.find("<td>status: unresolved\n");
    assert!(
        stopped.contains("Where the changes overlap")
            && stopped.contains(&format!("knowledge/claims/{claim}.yaml"))
            && theirs.is_some()
            && ours.is_some()
            && theirs < ours,
        "AC D-4: the overlap shows both texts, the wiki's first and the proposal's second: {stopped}"
    );
    let seen = anonymous.get(&claim_page);
    assert!(
        main_of(&seen).contains("This claim is contested in this view.")
            && !main_of(&seen).contains("unresolved"),
        "AC D-3: the status stays what an accepted proposal made it: {}",
        main_of(&seen)
    );
    let source_two = ids_after(&seen.body, "id=\"source-")
        .into_iter()
        .find(|id| *id != source_one)
        .expect("the second source's line");

    // ---- AC D-3: no path changes a claim or a source but an accepted proposal. The forms are proposals; a script's
    //      proposal may name pages only, so a knowledge file cannot be written raw, and nothing is recorded
    let auth = running.services.auth.clone().expect("sign-in is served");
    let token = auth
        .issue_token(&world.account_of("noor"), Scope::Propose, "r2", 3600, now())
        .unwrap()
        .secret
        .as_str()
        .to_owned();
    let tip = api(
        &running,
        "GET",
        &format!("{API}/raw/Help"),
        None,
        None,
        None,
    );
    assert_eq!(tip.status, 200, "{}", tip.body);
    let state = json_field(&tip.body, "state_id").expect("the tip's state id");
    let raw_claim = format!("knowledge/claims/{claim}.yaml");
    let body = format!(
        "{{\"summary\":\"Decide it by hand\",\"submit\":true,\"files\":[{{\"path\":{},\"text\":{}}}]}}",
        json_quote(&raw_claim),
        json_quote("kind: observation\ncommunity_status:\n  status: accepted\n")
    );
    let before = world.blocks();
    let made = api(
        &running,
        "POST",
        &format!("{API}/proposals"),
        Some(&token),
        Some(&format!("\"{state}\"")),
        Some(&body),
    );
    // This is the API's scope rule (a script's proposal names pages only), one of the paths D-3 closes; the core's
    // tests try the others.
    assert_eq!(
        made.status, 422,
        "the API's scope rule: {}\n{}",
        made.head, made.body
    );
    assert!(
        made.body.contains("path-outside-pages"),
        "the API's scope rule: a proposal names pages only: {}",
        made.body
    );
    assert_eq!(
        world.blocks(),
        before,
        "the API's scope rule: nothing was written"
    );
    let proposals = page_text(&mut noor.browser, "/handbook/-/proposals?state=all");
    assert!(
        proposals.contains("Cite the link syntax") && !proposals.contains("Decide it by hand"),
        "AC D-3: and nothing was recorded: {proposals}"
    );
    assert!(
        main_of(&anonymous.get(&claim_page)).contains("This claim is contested in this view."),
        "AC D-3: the status is what the accepted proposal made it"
    );

    // ---- AC D-10: *Sources to review* lists a source after `review_after`, not one reviewed today
    let listed = anonymous.get("/handbook/-/health/sources-to-review");
    assert_eq!(listed.status, 200, "{}", listed.body);
    assert_eq!(
        ids_after(main_of(&listed), "<code>")
            .into_iter()
            .filter(|id| *id == source_one || *id == source_two)
            .count(),
        2,
        "AC D-10: two sources, neither reviewed by anyone: {}",
        main_of(&listed)
    );
    assert!(
        main_of(&listed).contains("No one has recorded looking at it."),
        "AC D-10: {}",
        main_of(&listed)
    );
    let review = curate(
        &mut noor,
        &claim,
        None,
        |f| {
            let one = group_of(f, &source_one);
            f.set(&format!("c{one}_reviewed"), &today)
                .set(&format!("c{one}_availability"), "available");
            let two = group_of(f, &source_two);
            f.set(&format!("c{two}_reviewed"), "2022-03-01")
                .set(&format!("c{two}_availability"), "available");
        },
        "Say when each source was looked at",
    );
    signed_into_history(&approve_and_accept(&mut maya, &review));
    let listed = anonymous.get("/handbook/-/health/sources-to-review");
    let body = main_of(&listed);
    assert!(
        body.contains("Last reviewed on 2022-03-01, more than 1096 days ago.")
            && body.contains(&source_two)
            && !body.contains(&source_one)
            && !body.contains("No one has recorded looking at it."),
        "AC D-10: the source reviewed in March 2022 is listed with its reason, the one reviewed today is not: {body}"
    );
    assert!(
        body.contains(&claim) && body.contains("1 claim cites it:"),
        "AC D-10: with the claim that relies on it: {body}"
    );
    assert!(
        retired_words_in(&listed).is_empty(),
        "AC D-7: {:?}",
        retired_words_in(&listed)
    );

    // ---- AC D-11: a thread is written (a claim's own, append-only), and a reader sees it
    let page = noor.browser.get(&claim_page);
    let discuss = format!("{claim_page}/discuss");
    let mut form = form_to(&page, &discuss);
    form.set("body", "Where in chapter 2 is this said?");
    let said = noor.browser.submit(&form, None);
    assert_eq!(said.status, 303, "{}\n{}", said.head, said.body);
    let page = quinn.browser.get(&claim_page);
    let mut form = form_to(&page, &discuss);
    form.set("body", "Page 14, the third paragraph <b>of it</b>.");
    let replied = quinn.browser.submit(&form, None);
    assert_eq!(replied.status, 303, "{}\n{}", replied.head, replied.body);
    let seen = anonymous.get(&claim_page);
    let body = main_of(&seen);
    assert!(
        body.contains("Where in chapter 2 is this said?")
            && body.contains("Noor Contributor")
            && body.contains("Quinn Reviewer")
            && body.contains("Page 14, the third paragraph &#60;b&#62;of it&#60;/b&#62;."),
        "AC D-11: the thread, with the markup shown as letters: {body}"
    );
    let (first_comment, second_comment) = (
        body.find("Where in chapter 2 is this said?"),
        body.find("Page 14, the third paragraph"),
    );
    assert!(
        first_comment.is_some() && first_comment < second_comment,
        "AC D-11: the thread is in the order it was written (Noor's, then Quinn's): {body}"
    );
    assert!(
        body.contains("This claim is contested in this view."),
        "AC D-3, D-11: a thread never changes a claim: {body}"
    );

    // ---- AC D-12: the export, validated against wiki-commons' schema with the extension declared
    let before_export = world.blocks();
    let stopped = running.stop();
    assert!(
        !stopped.contains(SECRET),
        "no secret in the server's output (the server's secrets rule; not a criterion of D-12)"
    );
    let bundle = world.tmp.0.join("bundle");
    let (status, out, err) = world.cli(&[
        "wiki",
        "export",
        "handbook",
        "--bundle",
        bundle.to_str().unwrap(),
    ]);
    assert_eq!(status, 0, "AC D-12: {err}\n{out}");
    assert_eq!(world.blocks(), before_export, "an export writes nothing");
    let read = |path: &str| -> String {
        std::fs::read_to_string(bundle.join(path))
            .unwrap_or_else(|e| panic!("AC D-12: the bundle has no `{path}`: {e}"))
    };
    let manifest = read("wiki-bundle.yaml");
    assert!(
        manifest.contains(
            "\nextensions_used:\n  - name: inklkunn-knowledge\n    version: \"1\"\n    directory: extensions/inklkunn-knowledge\n"
        ),
        "AC D-12: the extension is declared: {manifest}"
    );
    let schema: serde_json::Value =
        serde_json::from_str(MANIFEST_SCHEMA).expect("the schema is JSON");
    let value = inklkunn_markup::frontmatter::parse_yaml(&manifest)
        .unwrap_or_else(|e| panic!("AC D-12: the manifest is not YAML the wiki reads: {e:?}"));
    let problems = json_schema::violations(&schema, &value.to_json());
    assert!(
        problems.is_empty(),
        "AC D-12: the manifest is not valid against wiki-commons' schema: {problems:#?}\n{manifest}"
    );
    for path in [
        format!("knowledge/claims/{claim}.yaml"),
        format!("knowledge/sources/{source_one}.yaml"),
        format!("knowledge/sources/{source_two}.yaml"),
    ] {
        assert!(
            bundle.join(&path).is_file(),
            "AC D-12: the knowledge files travel as files: {path}"
        );
    }
    let claim_file = read(&format!("knowledge/claims/{claim}.yaml"));
    assert!(
        [
            "kind: observation",
            "expressions:",
            "relations:",
            "community_status:",
            "contested"
        ]
        .iter()
        .all(|field| claim_file.contains(field))
            && [
                "relation: supports",
                "relation: challenges",
                "bearing: direct",
                "bearing: partial",
                "judged_on:"
            ]
            .iter()
            .all(|field| claim_file.contains(field)),
        "AC D-1, D-12: a claim's fields travel as the file has them: {claim_file}"
    );
    let thread = read(&format!(
        "extensions/inklkunn-knowledge/threads/{claim}.jsonl"
    ));
    assert_eq!(
        thread.lines().count(),
        2,
        "AC D-11: the thread is in the export, one line a comment: {thread}"
    );
    assert!(
        thread.contains("```text\\nWhere in chapter 2 is this said?\\n```")
            && thread.contains("Noor Contributor")
            && thread.contains("Quinn Reviewer"),
        "AC D-11: bodies as fenced text, authors named: {thread}"
    );
    let views_record = read("extensions/inklkunn-knowledge/views.jsonl");
    assert!(
        views_record.contains("\"name\":\"reading\"")
            && views_record.contains("\"kind\":\"interpretation\""),
        "AC D-9, D-12: the view's creation record travels: {views_record}"
    );
    let sums = read("SHA256SUMS");
    assert!(
        sums.contains(&format!(
            "extensions/inklkunn-knowledge/threads/{claim}.jsonl"
        )) && sums.contains("extensions/inklkunn-knowledge/views.jsonl"),
        "AC D-12: the extension's files are in the checksums: {sums}"
    );

    // ---- AC D-12 (the import) and D-5: another wiki is offered this bundle. The bundle brings what the people of the
    //      first wiki decided (a status, review days, availabilities), which is not what this wiki's people decided,
    //      so it is refused whole, by file and rule, never stripped; a claim file that is not one is refused by name
    let (status, out, err) = world.cli(&[
        "wiki",
        "create",
        "second",
        "--name",
        "Second",
        "--visibility",
        "public",
    ]);
    assert_eq!(status, 0, "{err}\n{out}");
    let folder = bundle.to_str().unwrap();
    let (status, out, err) = world.cli(&[
        "wiki",
        "import-folder",
        "second",
        folder,
        "--replace-existing",
        "--dry-run",
    ]);
    assert_eq!(status, 1, "AC D-12: {out}\n{err}");
    for path in [
        format!("knowledge/claims/{claim}.yaml"),
        format!("knowledge/sources/{source_one}.yaml"),
        format!("knowledge/sources/{source_two}.yaml"),
    ] {
        assert!(
            out.contains(&format!(
                "{path}: the change sets a community status, a review date or an availability"
            )),
            "AC D-12: the import refuses {path} by file and rule: {out}"
        );
    }
    assert!(
        err.contains("nothing was changed"),
        "AC D-12: the import is all or nothing: {err}"
    );
    std::fs::write(
        bundle.join(format!("knowledge/claims/{claim}.yaml")),
        "kind: [this is not a claim\nexpressions: 7\n",
    )
    .unwrap();
    let (status, out, err) = world.cli(&[
        "wiki",
        "import-folder",
        "second",
        folder,
        "--replace-existing",
    ]);
    assert_eq!(status, 1, "AC D-5: {out}\n{err}");
    assert!(
        out.contains(&format!("knowledge/claims/{claim}.yaml"))
            && err.contains("nothing was changed"),
        "AC D-5: a claim file that is not one is refused by name, and nothing is written: {out}\n{err}"
    );
}
