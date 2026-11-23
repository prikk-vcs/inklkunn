//! The policy page: the rules in sentences at the default and at other values, in both languages; why they are what they are
//! (the changes of the rules and the proposals that made them); the limits and what they lowered; the protections in force; the
//! approvals warning; the state when a file cannot be read; and the way to the people, the log and the appeals.

use inklkunn_core::governance::{
    Constitution, Entry, PlatformLimits, ReadFault, Refusal, Rules, read_policy,
};
use inklkunn_store::ProposalId;

use super::*;
use common::{block_id, state_id};

/// A policy with every key set, that is valid under the default limits.
pub(crate) const RICH: &str = "entry: contributor\nrungs:\n  accept: reviewer\n  moderate: reviewer\n  decide_appeal: maintainer\n  grant_up_to: contributor\nacceptance:\n  approvals: 1\n  self: never\nknowledge:\n  review_after: 400\npromotion:\n  contributor: \"A few accepted edits.\"\nprotect:\n  - namespace: Help\n    require: review\n    reason: \"Read first.\"\n  - path: Home\n    require:\n      rung: maintainer\n    reason: \"The front page.\"\n";

/// **Criterion 15, over the whole catalog**: no message of either language says a key of the rules' files as the files write it,
/// and none calls anything official. (The retired-words check holds *official* by the glossary; the keys are held here, beside the
/// page and the form that must never print one.)
#[test]
fn no_message_of_either_language_says_a_key_of_the_files_or_official() {
    use inklkunn_web::catalog::{EN_DEF, JA_DEF};
    for def in [&EN_DEF, &JA_DEF] {
        for entry in def.entries {
            for text in [entry.one, entry.other] {
                for raw in RAW_YAML {
                    assert!(!text.contains(raw), "{} says `{raw}`: {text}", entry.key);
                }
                assert!(!text.to_lowercase().contains("official"), "{}", entry.key);
                assert!(!text.contains("公式"), "{}", entry.key);
            }
        }
    }
}

#[tokio::test]
async fn the_page_of_a_wiki_with_no_rules_says_so_and_lists_the_default_rules_in_sentences() {
    let e = env();
    let a = as_(&e, &e.contributor, PAGE).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let main = main_of(&a);
    for want in [
        "<h1>The rules of this wiki</h1>",
        "follows the default rules",
        "Who may accept a proposal",
        "maintainer or above",
        "Who may comment",
        "questioner or above",
        "Approvals needed before a proposal is accepted",
        "Accepting your own proposal",
        "while the wiki has fewer than 3 maintainers",
        "A source is due for review after",
        "1096 days, as this server is set",
        "No protection is in force.",
        "No change to the rules is recorded.",
        "An admin of this wiki can always change these rules without any approval",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    assert_plain_words("the policy page", &a.body());
}

#[tokio::test]
async fn the_page_says_each_key_of_a_wiki_with_rules_of_its_own() {
    let e = env_under(RICH);
    let a = as_(&e, &e.contributor, PAGE).await;
    let main = main_of(&a);
    for want in [
        "These are the rules as set by the change",
        "reviewer or above",
        "contributor", // the role everyone holds
        "1 approval from someone other than the proposer",
        "never: a proposal is accepted by someone other than its proposer",
        "400 days",
        "A few accepted edits.",
        // The namespace says what it covers and what it does not, in the one display of the protections.
        "On the pages under Help/ (not the page Help).",
        "A change here is accepted only after someone other than the proposer has approved it.",
        "On the page Home.",
        "A change here is accepted only by maintainer or a higher role.",
        "Reason: The front page.",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    // The block that set them is linked, by its first characters.
    assert!(
        main.contains(&format!(
            "<a href=\"/handbook/-/change/{}\">{}</a>",
            common::POLICY_BLOCK,
            &common::POLICY_BLOCK[..12]
        )),
        "{main}"
    );
    assert_plain_words("the policy page", &a.body());
}

#[tokio::test]
async fn the_page_is_in_japanese_with_the_japanese_words_and_no_raw_key() {
    let e = env_under(RICH);
    let a = as_ja(&e, Some(&e.contributor), PAGE).await;
    assert_eq!(a.status, StatusCode::OK);
    let main = main_of(&a);
    for want in [
        "このウィキのルール",
        "提案を受け入れられる人",
        "レビュアー 以上",
        "A few accepted edits.",
        "ページ Home が対象です。",
        "Help/ の下にあるページが対象です（ページ Help 自体は含みません）。",
        "ルールがこうなっている理由",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    assert!(!main.contains("The rules of this wiki"), "{main}");
    assert_plain_words("the Japanese policy page", &a.body());
}

#[tokio::test]
async fn a_protection_that_asks_a_review_says_that_a_lone_maintainer_cannot_propose_and_accept_it()
{
    let small =
        env_under("protect:\n  - path: Home\n    require: review\n    reason: \"Read first.\"\n");
    let main = main_of(&as_(&small, &small.contributor, PAGE).await);
    assert!(
        main.contains(
            "a lone maintainer cannot both propose and accept a change to a protected page"
        ),
        "{main}"
    );
    // Where no one accepts their own proposal, there is nothing to add.
    let never = env_under(
        "acceptance:\n  self: never\nprotect:\n  - path: Home\n    require: review\n    reason: \"Read first.\"\n",
    );
    let main = main_of(&as_(&never, &never.contributor, PAGE).await);
    assert!(!main.contains("a lone maintainer"), "{main}");
}

#[tokio::test]
async fn a_limit_that_lowers_the_role_everyone_holds_is_said_beside_the_limits() {
    let policy = read_policy(b"entry: contributor\n").unwrap();
    let limits = PlatformLimits {
        entry_max: Entry::Observer,
        ..PlatformLimits::default()
    };
    let rules = Rules::read(
        Some(common::POLICY_BLOCK.to_owned()),
        policy,
        Constitution::default(),
        &limits,
    );
    assert_eq!(rules.lowered().len(), 1);
    let e = env_rules(rules);
    let main = main_of(&as_(&e, &e.contributor, PAGE).await);
    assert!(
        main.contains("The rules say that everyone who signs in holds contributor, but this server allows at most observer. So observer is what applies."),
        "{main}"
    );
    // A policy within the limits says nothing of the kind.
    let plain = env_under(RICH);
    let main = main_of(&as_(&plain, &plain.contributor, PAGE).await);
    assert!(!main.contains("this server allows at most"), "{main}");
}

#[tokio::test]
async fn the_limits_of_the_server_are_listed() {
    let e = env();
    let main = main_of(&as_(&e, &e.contributor, PAGE).await);
    assert!(main.contains("Limits of this server"), "{main}");
    assert!(
        main.contains("The highest role the rules may give everyone who signs in: "),
        "{main}"
    );
    assert!(
        main.contains("The longest a temporary protection or restriction may last: 90 days"),
        "{main}"
    );
}

#[tokio::test]
async fn the_role_everyone_holds_has_no_effect_in_a_restricted_wiki_and_the_page_says_so() {
    let w = under(&base(Visibility::Restricted), "entry: contributor\n");
    let e = cast(w);
    let main = main_of(&as_(&e, &e.observer, PAGE).await);
    assert!(
        main.contains("This has no effect in this wiki: only people who were granted a role here can read it or take part."),
        "{main}"
    );
    let public = env_under("entry: contributor\n");
    let main = main_of(&as_(&public, &public.observer, PAGE).await);
    assert!(!main.contains("This has no effect in this wiki"), "{main}");
}

#[tokio::test]
async fn the_approvals_warning_is_on_the_page_when_more_are_asked_than_people_can_give() {
    // Two reviewers or above hold a role (a maintainer and an admin and a reviewer: three), so ask for five.
    let e = env_under("acceptance:\n  approvals: 5\n");
    let main = main_of(&as_(&e, &e.contributor, PAGE).await);
    assert!(
        main.contains("not enough people can give them yet"),
        "{main}"
    );
    let ok = env_under("acceptance:\n  approvals: 1\n");
    let main = main_of(&as_(&ok, &ok.contributor, PAGE).await);
    assert!(!main.contains("not enough people can give"), "{main}");
}

#[tokio::test]
async fn a_file_that_cannot_be_read_is_said_with_the_file_and_the_line_and_the_default_rules_are_all_that_is_listed()
 {
    let fault = ReadFault::from_stored(
        inklkunn_core::governance::GovernanceFile::Policy,
        Some(3),
        Refusal::WrongShape,
    );
    let e = env_rules(Rules::faulted(Some(common::POLICY_BLOCK.to_owned()), fault));
    let a = as_(&e, &e.contributor, PAGE).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let main = main_of(&a);
    for want in [
        "The rules cannot be read",
        "governance/policy.yaml, line 3: ",
        "every change is refused",
        "An admin of the wiki can propose a change that repairs only the rules",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    // Where something that is not a file stands in its place, what stands there is listed, as on the page of every refusal.
    let fault = ReadFault::from_stored(
        inklkunn_core::governance::GovernanceFile::Policy,
        None,
        Refusal::NotAFile,
    )
    .with_found(vec!["governance/policy.yaml/a.txt".to_owned()], 2);
    let e = env_rules(Rules::faulted(None, fault));
    let main = main_of(&as_(&e, &e.contributor, PAGE).await);
    assert!(
        main.contains("governance/policy.yaml/a.txt, and 2 more"),
        "{main}"
    );
    assert_plain_words("the unreadable page", &main);
}

/// The message of a change made by `proposal`, as the service writes it.
pub(crate) fn message(
    id: &ProposalId,
    summary: &str,
    by: &AccountId,
    accepted_by: &AccountId,
) -> String {
    use inklkunn_core::message::{Actor, BlockId, Message, Summary};
    use inklkunn_store::DisplayName;
    let actor = |id: &AccountId, name: &str| Actor::Account {
        id: id.clone(),
        name: DisplayName::new(name).expect("name"),
    };
    Message::proposal(
        Summary::new(summary).expect("summary"),
        id.clone(),
        actor(by, "Aiko Tanaka"),
        actor(accepted_by, "Mika Ito"),
        BlockId::new(&block_id(1)).expect("block"),
    )
    .expect("a message")
    .to_text()
}

/// The repository took a change of `file` to `text` (none: removed) as its change number `version` (the double numbers them from
/// 1, and the first is the wiki's own), sealed from `proposal`.
pub(crate) fn changed_rules(
    e: &Env,
    version: u64,
    proposal: &ProposalId,
    summary: &str,
    file: &str,
    text: Option<&str>,
) {
    match text {
        Some(text) => e.w.fake.set_files(&[(file, text)]),
        None => e.w.fake.remove(file),
    }
    e.w.fake.say(
        version,
        &message(proposal, summary, &e.contributor, &e.maintainer),
    );
    e.w.fake.touches(&block_id(version), &[file]);
}

#[tokio::test]
async fn the_history_lists_each_change_of_the_rules_newest_first_with_its_proposal_and_what_it_did()
{
    let e = env();
    // Two proposals, each made through the form's own route, then taken into the history by hand.
    let first = a_proposal(&e, "Ask a review of Home").await;
    let second = a_proposal(&e, "Let reviewers accept").await;
    changed_rules(
        &e,
        2,
        &first,
        "Ask a review of Home",
        "governance/policy.yaml",
        Some("protect:\n  - path: Home\n    require: review\n    reason: \"Read first.\"\n"),
    );
    changed_rules(
        &e,
        3,
        &second,
        "Let reviewers accept",
        "governance/policy.yaml",
        Some(
            "rungs:\n  accept: reviewer\nprotect:\n  - path: Home\n    require: review\n    reason: \"Read first.\"\n",
        ),
    );
    let a = as_(&e, &e.contributor, PAGE).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let main = main_of(&a);
    let (newer, older) = (
        main.find("Let reviewers accept")
            .expect("the newer proposal"),
        main.find("Ask a review of Home")
            .expect("the older proposal"),
    );
    assert!(newer < older, "newest first: {main}");
    for want in [
        format!("/handbook/-/proposals/{second}"),
        format!("/handbook/-/proposals/{first}"),
        "Who may accept a proposal: maintainer or above → reviewer or above".to_owned(),
        "Protection added: The page Home: a review by someone other than the proposer first. Reason: Read first.".to_owned(),
    ] {
        assert!(main.contains(&want), "{want}: {main}");
    }
    assert!(
        !main.contains("No change to the rules is recorded."),
        "{main}"
    );
}

/// A held proposal of the contributor, to be named by a change of the rules.
async fn a_proposal(e: &Env, summary: &str) -> ProposalId {
    let a = post_form(
        &e.w,
        "/handbook/-/proposals/new",
        Some(&e.contributor),
        &[
            ("op", "submit"),
            ("pages", "1"),
            ("renames", "0"),
            ("part", "whole"),
            ("base_state", &state_id(1)),
            ("base_block", &block_id(1)),
            ("p0_path", "pages/Home.md"),
            ("p0_title", "Home"),
            ("p0_text", &home_with("The bridge opened in 1951.")),
            ("summary", summary),
            ("sources", ""),
        ],
    )
    .await;
    proposal_of(&a)
}

#[tokio::test]
async fn a_change_that_made_the_rules_go_back_to_the_defaults_and_one_that_only_repaired_are_said()
{
    let e = env();
    let proposal = a_proposal(&e, "Start with rules").await;
    changed_rules(
        &e,
        2,
        &proposal,
        "Start with rules",
        "governance/policy.yaml",
        Some("entry: observer\n"),
    );
    changed_rules(
        &e,
        3,
        &proposal,
        "Back to defaults",
        "governance/policy.yaml",
        None,
    );
    let main = main_of(&as_(&e, &e.contributor, PAGE).await);
    assert!(
        main.contains(
            "This removes a file of the rules: the default rules apply again for what it set."
        ),
        "{main}"
    );
    assert!(
        main.contains("The role everyone who signs in holds: observer → none: people hold only the role they are granted"),
        "{main}"
    );
}

#[tokio::test]
async fn the_page_links_the_people_the_log_the_appeals_and_the_form_that_changes_the_rules() {
    let e = env();
    let main = main_of(&as_(&e, &e.contributor, PAGE).await);
    for href in [
        "/handbook/-/policy/edit",
        "/handbook/-/people",
        "/handbook/-/moderation",
        "/handbook/-/appeals/mine",
    ] {
        assert!(main.contains(&format!("href=\"{href}\"")), "{href}: {main}");
    }
    // The people who decide appeals are also sent to them; a visitor is not offered one's own.
    assert!(!main.contains("href=\"/handbook/-/appeals\""), "{main}");
    let admin = main_of(&as_(&e, &e.admin, PAGE).await);
    assert!(admin.contains("href=\"/handbook/-/appeals\""), "{admin}");
    let visitor = main_of(&common::get(&e.w, PAGE).await);
    assert!(!visitor.contains("/appeals/mine"), "{visitor}");
    assert!(
        visitor.contains("href=\"/handbook/-/moderation\""),
        "{visitor}"
    );
}

#[tokio::test]
async fn a_temporary_protection_is_listed_with_the_role_that_set_it_and_the_rules_it_did_it_under()
{
    let e = env();
    let a = post_form(
        &e.w,
        "/handbook/-/protect",
        Some(&e.maintainer),
        &[
            ("target", "page"),
            ("title", "Home"),
            ("require", "review"),
            ("reason", "Vandalism this week."),
            ("days", "7"),
            ("act", "set"),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let main = main_of(&as_(&e, &e.contributor, PAGE).await);
    for want in [
        "Temporarily, until ",
        "Vandalism this week.",
        "Put in place by a person holding maintainer, under the default rules.",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    let ja = main_of(&as_ja(&e, Some(&e.contributor), PAGE).await);
    assert!(
        ja.contains("メンテナー の人が設けました。適用されたルール: 既定のルール。"),
        "{ja}"
    );
}

#[tokio::test]
async fn a_wiki_the_visitor_may_not_read_has_no_page_of_the_rules_and_no_form_byte_for_byte() {
    let w = under(&base(Visibility::Restricted), RICH);
    let stranger = person(9);
    add_person(&w, &stranger, "Sue Stranger", None);
    let absent = {
        let none = base(Visibility::Public);
        get_as(&none, "/nowhere/-/policy", None).await
    };
    for uri in [PAGE, EDIT, "/handbook/-/api/v1/policy"] {
        for who in [None, Some(&stranger)] {
            let there = get_as(&w, uri, who).await;
            let gone = get_as(&w, &uri.replace("handbook", "no-such-wiki"), who).await;
            assert_eq!(there.status, gone.status, "{uri}");
            assert_eq!(there.status, absent.status, "{uri}");
            assert_eq!(
                there.body().replace("handbook", "no-such-wiki"),
                gone.body(),
                "{uri} as {who:?}: the same page, byte for byte"
            );
        }
    }
}
