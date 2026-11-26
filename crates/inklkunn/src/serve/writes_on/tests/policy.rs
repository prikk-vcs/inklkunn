#![allow(clippy::expect_used, clippy::unwrap_used)]
//! RFC 006 acceptance criterion 2 **through the policy form**, end to end (handoff G-T7): **the product's own wiring**. The real
//! server (the binary's `prepare`) over a real prikk. A maintainer opens the form and sends `acceptance.approvals: 1` (as a
//! sentence and a control, never as YAML), accepts it, and from the next decision on a page proposal is refused for want of an
//! approval until a second person approves; the record of that acceptance names the block of the new rules, and the policy page
//! says the new rule in a sentence and lists the change with the proposal that made it. A second change through the form is a
//! chain of two, newest first.
//!
//! **The interwiki map through the same form** (RFC 007 D-8, W-6): a row added, accepted by the signed change, and the policy page,
//! the form and the site description say the new map; a row changed and one added; the last removed, and the wiki has no map.

use super::*;

const FORM: &str = "/handbook/-/policy/edit";
const POLICY: &str = "/handbook/-/policy";

/// The first change linked on `page` after the words `after`.
fn change_after(page: &str, after: &str) -> String {
    let at = page
        .find(after)
        .unwrap_or_else(|| panic!("no `{after}` in {page}"));
    let rest = page.get(at..).unwrap_or_default();
    let (_, tail) = rest
        .split_once("href=\"/handbook/-/change/")
        .unwrap_or_else(|| panic!("no change after `{after}` in {page}"));
    tail.split('"').next().unwrap_or_default().to_owned()
}

#[test]
fn a_policy_changed_through_the_form_changes_who_may_accept_from_the_next_decision_and_the_record_names_the_new_block()
 {
    let Some(world) = World::new("policy") else {
        return;
    };
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
    let mut quinn = join(&world, &running, "quinn", "Quinn Reviewer", Role::Reviewer);
    let first_blocks = world.blocks();

    // Before: no rules of the wiki's own, said so.
    let before = maya.browser.get(POLICY);
    assert_eq!(before.status, 200, "{}", before.body);
    assert!(
        before.body.contains("follows the default rules"),
        "{}",
        before.body
    );

    // Maya opens the form and changes one sentence: a proposal needs one approval from someone other than its proposer.
    let page = maya.browser.get(FORM);
    assert_eq!(page.status, 200, "{}", page.body);
    assert!(
        page.body
            .contains("Approvals needed before a proposal is accepted")
    );
    let mut form = form_to(&page, FORM);
    assert_eq!(form.get("approvals"), "0");
    form.set("approvals", "1");
    let sent = maya.browser.submit(&form, Some(("act", "send")));
    assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
    let rules = proposal_id(&sent.location());
    assert_eq!(world.blocks(), first_blocks, "proposing writes nothing");

    // The proposal says the change as before and after sentences, never as the file.
    let shown = maya.browser.get(&proposal_page(&rules));
    assert_eq!(shown.status, 200, "{}", shown.body);
    for want in [
        "What this proposal changes in the rules",
        "Approvals needed before a proposal is accepted: none → 1 approval from someone other than the proposer",
        "Accepting this change takes: maintainer or above, with 0 approvals",
    ] {
        assert!(shown.body.contains(want), "{want}: {}", shown.body);
    }
    for raw in ["acceptance:", "rungs:", "approvals: 1"] {
        assert!(!shown.body.contains(raw), "{raw}: {}", shown.body);
    }

    // A maintainer accepts it, under the rules there are (a small wiki: a maintainer may accept their own proposal).
    let accept = format!("{}/accept", proposal_page(&rules));
    let signed = maya.browser.submit(&form_to(&shown, &accept), None);
    assert_eq!(signed.status, 200, "{}\n{}", signed.head, signed.body);
    assert_eq!(world.blocks().len(), first_blocks.len() + 1);
    let rules_block = world.blocks().first().cloned().expect("the new change");

    // From the next decision on, a page proposal needs an approval first: refused, by name, until a second person gives one.
    let page_one = propose_edit(&mut noor, "Help", "Say which brackets", clarify_reading);
    let hers = maya.browser.get(&proposal_page(&page_one));
    let accept_page = format!("{}/accept", proposal_page(&page_one));
    let held = maya.browser.submit(&form_to(&hers, &accept_page), None);
    assert_eq!(held.status, 409, "{}\n{}", held.head, held.body);
    assert!(
        held.body.contains("it needs more approvals"),
        "{}",
        held.body
    );
    assert_eq!(
        world.blocks().len(),
        first_blocks.len() + 1,
        "nothing was signed"
    );
    let theirs = quinn.browser.get(&proposal_page(&page_one));
    let mut decide = form_to(&theirs, &format!("{}/decide", proposal_page(&page_one)));
    decide.set("body", "Looks right to me.");
    let decided = quinn.browser.submit(&decide, Some(("op", "approve")));
    assert_eq!(decided.status, 303, "{}\n{}", decided.head, decided.body);
    let hers = maya.browser.get(&proposal_page(&page_one));
    let accepted = maya.browser.submit(&form_to(&hers, &accept_page), None);
    assert_eq!(accepted.status, 200, "{}\n{}", accepted.head, accepted.body);
    assert_eq!(world.blocks().len(), first_blocks.len() + 2);

    // The record of that acceptance names the block of the new rules, with its link; the acceptance of the rules themselves
    // names the rules it was made under, which were the defaults.
    let record = maya.browser.get(&proposal_page(&page_one));
    let named = format!(
        "Accepted by Maya Maintainer (maintainer), under the rules as of <a href=\"/handbook/-/change/{rules_block}\">{}</a>.",
        &rules_block[..12]
    );
    assert!(record.body.contains(&named), "{named}: {}", record.body);
    let first = maya.browser.get(&proposal_page(&rules));
    assert!(
        first
            .body
            .contains("Accepted by Maya Maintainer (maintainer), under the default rules."),
        "{}",
        first.body
    );
    // The reviewer's decision names the role she held and the same rules.
    assert!(
        record.body.contains(&format!(
            "Decided as reviewer, under the rules as of <a href=\"/handbook/-/change/{rules_block}\">"
        )),
        "{}",
        record.body
    );

    // The policy page says the new rule in a sentence and the change with the proposal that made it.
    let policy = noor.browser.get(POLICY);
    assert_eq!(policy.status, 200, "{}", policy.body);
    for want in [
        "1 approval from someone other than the proposer",
        "These are the rules as set by the change",
        &format!("href=\"/handbook/-/proposals/{rules}\""),
        "Approvals needed before a proposal is accepted: none → 1 approval from someone other than the proposer",
    ] {
        assert!(policy.body.contains(want), "{want}: {}", policy.body);
    }
    assert_eq!(
        change_after(&policy.body, "These are the rules as set by the change"),
        rules_block
    );
    for raw in ["acceptance:", "rungs:", "entry:"] {
        assert!(!policy.body.contains(raw), "{raw}: {}", policy.body);
    }
    assert!(!policy.body.to_lowercase().contains("official"));

    // A second change through the form: a chain of two, the newest first, each with its proposal.
    let page = maya.browser.get(FORM);
    let mut form = form_to(&page, FORM);
    assert_eq!(
        form.get("approvals"),
        "1",
        "the form is filled from the rules in force now"
    );
    form.set("review_days", "30");
    let sent = maya.browser.submit(&form, Some(("act", "send")));
    assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
    let second = proposal_id(&sent.location());
    // The rules ask one approval of a page and of a change to the rules alike: the page says so, and a reviewer gives it.
    let shown = maya.browser.get(&proposal_page(&second));
    assert!(
        shown.body.contains(
            "Accepting this change takes: maintainer or above, with 1 approval from someone other than the proposer"
        ),
        "{}",
        shown.body
    );
    let theirs = quinn.browser.get(&proposal_page(&second));
    let mut decide = form_to(&theirs, &format!("{}/decide", proposal_page(&second)));
    decide.set("body", "The interval is fine.");
    let decided = quinn.browser.submit(&decide, Some(("op", "approve")));
    assert_eq!(decided.status, 303, "{}\n{}", decided.head, decided.body);
    let shown = maya.browser.get(&proposal_page(&second));
    let signed = maya.browser.submit(
        &form_to(&shown, &format!("{}/accept", proposal_page(&second))),
        None,
    );
    assert_eq!(signed.status, 200, "{}\n{}", signed.head, signed.body);
    let policy = noor.browser.get(POLICY);
    let (newer, older) = (
        policy.body.find(&format!("/handbook/-/proposals/{second}")),
        policy.body.find(&format!("/handbook/-/proposals/{rules}")),
    );
    assert!(
        newer.is_some() && older.is_some() && newer < older,
        "newest first: {}",
        policy.body
    );
    assert!(
        policy.body.contains(
            "A source is due for review after: 1096 days, as this server is set → 30 days"
        ),
        "{}",
        policy.body
    );
    running.stop();
}

/// The form with the map rows `rows` typed (it is opened, a row is added for each, and what was typed is sent).
fn send_map(person: &mut Person, rows: &[(&str, &str)]) -> Reply {
    let mut page = person.browser.get(FORM);
    assert_eq!(page.status, 200, "{}", page.body);
    let have: usize = form_to(&page, FORM)
        .get("map_rows")
        .parse()
        .expect("a count");
    for _ in have..rows.len() {
        let form = form_to(&page, FORM);
        page = person.browser.submit(&form, Some(("act", "map-add")));
        assert_eq!(page.status, 200, "{}", page.body);
    }
    let mut form = form_to(&page, FORM);
    // Rows beyond what is wanted are taken away, last first, by their own button.
    for n in (rows.len() + 1..=have).rev() {
        let remove = format!("map-remove-{n}");
        page = person.browser.submit(&form, Some(("act", remove.as_str())));
        assert_eq!(page.status, 200, "{}", page.body);
        form = form_to(&page, FORM);
    }
    for (n, (prefix, template)) in rows.iter().enumerate() {
        form.set(&format!("map-{}-prefix", n + 1), prefix);
        form.set(&format!("map-{}-template", n + 1), template);
    }
    person.browser.submit(&form, Some(("act", "send")))
}

/// Accept the proposal `id` as `person`, and say how many blocks the wiki has then.
fn accept(world: &World, person: &mut Person, id: &str) {
    let before = world.blocks().len();
    let shown = person.browser.get(&proposal_page(id));
    assert_eq!(shown.status, 200, "{}", shown.body);
    let signed = person.browser.submit(
        &form_to(&shown, &format!("{}/accept", proposal_page(id))),
        None,
    );
    assert_eq!(signed.status, 200, "{}\n{}", signed.head, signed.body);
    assert_eq!(world.blocks().len(), before + 1);
}

#[test]
fn an_interwiki_map_changed_through_the_form_is_accepted_and_the_policy_page_the_form_and_the_site_follow_it()
 {
    let Some(world) = World::new("policy-map") else {
        return;
    };
    let running = Running::start(&world);
    let mut maya = join(
        &world,
        &running,
        "maya",
        "Maya Maintainer",
        Role::Maintainer,
    );
    let wiki = "https://en.wikipedia.org/wiki/{title}";
    let docs = "https://example.org/docs?page={title}";

    // No map yet: the page says so, and the form has no row.
    let before = maya.browser.get(POLICY);
    assert!(
        before.body.contains("This wiki has no interwiki map."),
        "{}",
        before.body
    );
    assert_eq!(form_to(&maya.browser.get(FORM), FORM).get("map_rows"), "0");

    // A rule that is broken is said beside its row and nothing is proposed.
    let refused = send_map(
        &mut maya,
        &[("wikipedia", "http://en.wikipedia.org/wiki/{title}")],
    );
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert!(
        refused
            .body
            .contains("An address template of the interwiki map is not an https address."),
        "{}",
        refused.body
    );
    let blocks = world.blocks();

    // One prefix, sent as a proposal: said as a sentence, never as the file; signed by the accepting change.
    let sent = send_map(&mut maya, &[("wikipedia", wiki)]);
    assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
    let first = proposal_id(&sent.location());
    assert_eq!(world.blocks(), blocks, "proposing writes nothing");
    let shown = maya.browser.get(&proposal_page(&first));
    assert!(
        shown
            .body
            .contains(&format!("Interwiki prefix added: wikipedia → {wiki}")),
        "{}",
        shown.body
    );
    assert!(!shown.body.contains("wikipedia: "), "{}", shown.body);
    accept(&world, &mut maya, &first);

    // The next sight of the policy page, the form and the site description is of the new map.
    let policy = maya.browser.get(POLICY);
    for want in [
        "<dt>wikipedia</dt>",
        wiki,
        "These are the rules as set by the change",
    ] {
        assert!(policy.body.contains(want), "{want}: {}", policy.body);
    }
    assert!(!policy.body.contains("This wiki has no interwiki map."));
    let form = form_to(&maya.browser.get(FORM), FORM);
    assert_eq!(form.get("map_rows"), "1");
    assert_eq!(form.get("map-1-prefix"), "wikipedia");
    assert_eq!(form.get("map-1-template"), wiki);
    let site = maya.browser.get("/handbook/-/site.json");
    assert!(
        site.body.contains("\"interwiki\":{\"wikipedia\":"),
        "{}",
        site.body
    );

    // Sent unchanged, the form makes nothing.
    let same = send_map(&mut maya, &[("wikipedia", wiki)]);
    assert_eq!(same.status, 200, "{}", same.body);
    assert!(same.body.contains("Nothing was changed"), "{}", same.body);

    // One changed and one added, in one proposal.
    let sent = send_map(
        &mut maya,
        &[
            ("docs", docs),
            ("wikipedia", "https://de.wikipedia.org/wiki/{title}"),
        ],
    );
    assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
    let second = proposal_id(&sent.location());
    accept(&world, &mut maya, &second);
    let policy = maya.browser.get(POLICY);
    for want in [
        "<dt>docs</dt>",
        "<dd>https://example.org/docs?page={title}</dd>",
        "<dd>https://de.wikipedia.org/wiki/{title}</dd>",
    ] {
        assert!(policy.body.contains(want), "{want}: {}", policy.body);
    }
    // The map shows what leads where now: the old address is in the history of the change, not in the map.
    assert!(
        !policy.body.contains(&format!("<dd>{wiki}</dd>")),
        "{}",
        policy.body
    );

    // The last row removed: the file is removed, and the wiki has no map again.
    let sent = send_map(&mut maya, &[]);
    assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
    let third = proposal_id(&sent.location());
    let shown = maya.browser.get(&proposal_page(&third));
    assert!(
        shown.body.contains("Interwiki prefix removed: docs"),
        "{}",
        shown.body
    );
    accept(&world, &mut maya, &third);
    let policy = maya.browser.get(POLICY);
    assert!(
        policy.body.contains("This wiki has no interwiki map."),
        "{}",
        policy.body
    );
    let site = maya.browser.get("/handbook/-/site.json");
    assert!(site.body.contains("\"interwiki\":{}"), "{}", site.body);
    running.stop();
}
