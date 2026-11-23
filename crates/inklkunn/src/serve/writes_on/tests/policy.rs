#![allow(clippy::expect_used, clippy::unwrap_used)]
//! RFC 006 acceptance criterion 2 **through the policy form**, end to end (handoff G-T7): **the product's own wiring**. The real
//! server (the binary's `prepare`) over a real prikk. A maintainer opens the form and sends `acceptance.approvals: 1` (as a
//! sentence and a control, never as YAML), accepts it, and from the next decision on a page proposal is refused for want of an
//! approval until a second person approves; the record of that acceptance names the block of the new rules, and the policy page
//! says the new rule in a sentence and lists the change with the proposal that made it. A second change through the form is a
//! chain of two, newest first.

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
