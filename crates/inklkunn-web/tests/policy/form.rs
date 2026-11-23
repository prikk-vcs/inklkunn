//! The form that proposes a change to the rules: filled from the rules in force, each control round-trips, what it writes is the
//! canonical text the validator accepts, each refusal is said by its name beside its control, a change to the rules is never mixed
//! with a page, an unchanged form makes nothing, the way back to the defaults, the repair, and who may use it.

use inklkunn_core::governance::draft::PolicyDraft;
use inklkunn_core::governance::{
    GovernanceFile, PlatformLimits, Policy, ReadFault, Refusal, Rules, validate_constitution,
    validate_policy,
};
use inklkunn_store::{ProposalFilter, ProposalStore};

use super::page::RICH;
use super::*;

const POLICY_FILE: &str = "governance/policy.yaml";
const CONSTITUTION_FILE: &str = "governance/constitution.yaml";

fn canonical(change: impl FnOnce(&mut PolicyDraft)) -> String {
    let mut draft = PolicyDraft::of(&Policy::default());
    change(&mut draft);
    validate_policy(&draft.to_text(), &PlatformLimits::default())
        .expect("a policy")
        .to_yaml()
}

fn proposals_made(e: &Env) -> usize {
    e.w.store
        .proposals(&e.w.wiki.id, &ProposalFilter::default(), None)
        .unwrap()
        .len()
}

fn value<'a>(fields: &'a [(String, String)], name: &str) -> &'a str {
    fields
        .iter()
        .find(|(n, _)| n == name)
        .map_or("(no such field)", |(_, v)| v.as_str())
}

#[tokio::test]
async fn the_form_is_filled_from_the_rules_in_force_with_a_sentence_and_a_control_for_each_key() {
    let e = env_under(RICH);
    let a = as_(&e, &e.contributor, EDIT).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let main = main_of(&a);
    for want in [
        "<h1>Propose a change to the rules</h1>",
        "Anyone who may propose can send a change to the rules.",
        "Accepting a change to the rules takes: maintainer or above, with 1 approval from someone other than the proposer",
        "Who may accept a proposal</label>",
        "Approvals needed before a proposal is accepted</label>",
        "Before this wiki grants contributor</label>",
        "Protection 1</legend>",
        "Protection 2</legend>",
        "name=\"csrf\"",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    assert!(!main.contains("<script"), "{main}");
    let fields = form_fields(&main);
    for (name, want) in [
        ("who_accept", "reviewer"),
        ("who_moderate", "reviewer"),
        ("who_appeals", "maintainer"),
        ("who_grants", "contributor"),
        ("everyone", "contributor"),
        ("approvals", "1"),
        ("own_proposal", "never"),
        ("review_days", "400"),
        ("promo-contributor", "A few accepted edits."),
        ("rows", "2"),
        ("row-1-kind", "namespace"),
        ("row-1-title", "Help"),
        ("row-1-asks", "review"),
        ("row-1-reason", "Read first."),
        ("row-2-kind", "page"),
        ("row-2-title", "Home"),
        ("row-2-asks", "maintainer"),
        ("change_rung", "maintainer"),
        ("change_approvals", "0"),
        ("change_who_approvals", "0"),
    ] {
        assert_eq!(value(&fields, name), want, "{name}: {main}");
    }
    assert_plain_words("the policy form", &a.body());
    let ja = as_ja(&e, Some(&e.contributor), EDIT).await;
    assert_eq!(ja.status, StatusCode::OK);
    assert!(main_of(&ja).contains("ルールの変更を提案する"));
    assert_plain_words("the Japanese policy form", &ja.body());
}

#[tokio::test]
async fn an_unchanged_form_creates_nothing_and_says_so() {
    for e in [env(), env_under(RICH)] {
        let fields = opened(&e, &e.contributor, "send").await;
        let a = post_fields(&e.w, Some(&e.contributor), &fields).await;
        assert_eq!(a.status, StatusCode::OK, "{}", a.body());
        assert!(
            main_of(&a).contains("Nothing was changed, so no proposal was made."),
            "{}",
            a.body()
        );
        assert_eq!(proposals_made(&e), 0);
    }
}

#[tokio::test]
async fn each_control_changed_makes_a_proposal_whose_text_is_the_canonical_file() {
    type Change = fn(&mut PolicyDraft);
    let cases: Vec<(&str, &str, Change)> = vec![
        ("who_comment", "observer", |d| {
            d.comment = "observer".to_owned()
        }),
        ("who_propose", "questioner", |d| {
            d.propose = "questioner".to_owned()
        }),
        ("who_review", "contributor", |d| {
            d.review = "contributor".to_owned()
        }),
        ("who_view", "maintainer", |d| {
            d.create_view = "maintainer".to_owned()
        }),
        ("who_accept", "reviewer", |d| {
            d.accept = "reviewer".to_owned()
        }),
        ("who_moderate", "reviewer", |d| {
            d.moderate = "reviewer".to_owned()
        }),
        ("who_appeals", "maintainer", |d| {
            d.decide_appeal = "maintainer".to_owned()
        }),
        ("who_grants", "reviewer", |d| {
            d.grant_up_to = "reviewer".to_owned()
        }),
        ("everyone", "observer", |d| d.entry = "observer".to_owned()),
        ("approvals", "2", |d| d.approvals = 2),
        ("own_proposal", "never", |d| {
            d.self_acceptance = "never".to_owned()
        }),
        ("review_days", "30", |d| d.review_after = Some(30)),
        ("promo-reviewer", "Read ten pages.", |d| {
            d.promotion = vec![(Role::Reviewer, "Read ten pages.".to_owned())]
        }),
    ];
    for (field, posted, change) in cases {
        let e = env();
        let fields = with(&opened(&e, &e.maintainer, "send").await, field, posted);
        let a = post_fields(&e.w, Some(&e.maintainer), &fields).await;
        let id = proposal_of(&a);
        assert_eq!(
            files_of(&e.w, &id),
            vec![(POLICY_FILE.to_owned(), Some(canonical(change)))],
            "{field}"
        );
    }
}

#[tokio::test]
async fn the_constitutions_settings_are_written_to_its_own_file_and_a_policy_only_when_it_changed()
{
    let e = env();
    let fields = with(
        &with(&opened(&e, &e.admin, "send").await, "change_rung", "admin"),
        "change_approvals",
        "2",
    );
    let a = post_fields(&e.w, Some(&e.admin), &fields).await;
    let id = proposal_of(&a);
    let expected = validate_constitution(
        "policy_change:\n  rung: admin\n  approvals: 2\nconstitution_change:\n  approvals: 0\n",
    )
    .unwrap();
    assert_eq!(
        files_of(&e.w, &id),
        vec![(CONSTITUTION_FILE.to_owned(), Some(expected.to_yaml()))]
    );
    // Both at once: both files, constitution first.
    let e = env();
    let fields = with(
        &with(
            &opened(&e, &e.admin, "send").await,
            "change_who_approvals",
            "1",
        ),
        "approvals",
        "1",
    );
    let id = proposal_of(&post_fields(&e.w, Some(&e.admin), &fields).await);
    let paths: Vec<String> = files_of(&e.w, &id).into_iter().map(|(p, _)| p).collect();
    assert_eq!(paths, [CONSTITUTION_FILE, POLICY_FILE]);
}

#[tokio::test]
async fn protections_are_a_list_with_rows_added_and_removed_and_a_blank_row_is_no_protection() {
    let e = env();
    let opened = opened(&e, &e.maintainer, "add").await;
    let a = post_fields(&e.w, Some(&e.maintainer), &opened).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let fields = form_fields(&main_of(&a));
    assert_eq!(value(&fields, "rows"), "1");
    assert_eq!(value(&fields, "row-1-kind"), "page");
    assert_eq!(value(&fields, "row-1-asks"), "review");
    // Nothing was typed in the row: it is dropped, and the form is unchanged.
    let sent = with(&fields, "act", "send");
    let a = post_fields(&e.w, Some(&e.maintainer), &sent).await;
    assert!(main_of(&a).contains("Nothing was changed"), "{}", a.body());
    // A row filled in is a standing protection; a second add keeps what was typed.
    let filled = with(
        &with(
            &with(&sent, "row-1-title", "Help"),
            "row-1-kind",
            "namespace",
        ),
        "row-1-reason",
        "Read first.",
    );
    let again = post_fields(&e.w, Some(&e.maintainer), &with(&filled, "act", "add")).await;
    let after_add = form_fields(&main_of(&again));
    assert_eq!(value(&after_add, "rows"), "2");
    assert_eq!(value(&after_add, "row-1-title"), "Help");
    let removed = post_fields(
        &e.w,
        Some(&e.maintainer),
        &with(&after_add, "act", "remove-2"),
    )
    .await;
    assert_eq!(value(&form_fields(&main_of(&removed)), "rows"), "1");
    let id = proposal_of(&post_fields(&e.w, Some(&e.maintainer), &filled).await);
    let expected = canonical(|d| {
        d.protect = vec![inklkunn_core::governance::draft::ProtectDraft {
            namespace: true,
            title: "Help".to_owned(),
            require: "review".to_owned(),
            reason: "Read first.".to_owned(),
        }]
    });
    assert_eq!(
        files_of(&e.w, &id),
        vec![(POLICY_FILE.to_owned(), Some(expected))]
    );
}

/// A refusal of the validator is said by its sentence beside the control it concerns, and nothing is made.
async fn refused_beside(e: &Env, changes: &[(&str, &str)], control: &str, sentence: &str) {
    let mut fields = opened(e, &e.maintainer, "send").await;
    for (name, value) in changes {
        fields = with(&fields, name, value);
    }
    let a = post_fields(&e.w, Some(&e.maintainer), &fields).await;
    assert_eq!(
        a.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{control}: {}",
        a.body()
    );
    let main = main_of(&a);
    let at = main
        .find(&format!("id=\"c-{control}-problem\""))
        .unwrap_or_else(|| panic!("no problem beside {control}: {main}"));
    let said = main.get(at..).unwrap_or("");
    let said = said
        .get(..said.find("</p>").unwrap_or(said.len()))
        .unwrap_or("");
    assert!(
        said.contains(sentence),
        "{control}: `{sentence}` not in `{said}`"
    );
    assert_eq!(proposals_made(e), 0, "{control}");
    assert_plain_words("a refusal of the form", &a.body());
}

#[tokio::test]
async fn each_refusal_of_the_validator_is_said_by_its_sentence_beside_its_control() {
    let e = env();
    refused_beside(
        &e,
        &[("who_propose", "maintainer"), ("who_review", "reviewer")],
        "who_propose",
        "The role that may propose must not be above the role that may review",
    )
    .await;
    refused_beside(
        &e,
        &[("who_review", "maintainer"), ("who_accept", "reviewer")],
        "who_review",
        "The role that may review must not be above the role that may accept",
    )
    .await;
    refused_beside(
        &e,
        &[("who_moderate", "contributor")],
        "who_moderate",
        "The role that may moderate must be reviewer or above",
    )
    .await;
    refused_beside(
        &e,
        &[("approvals", "9")],
        "approvals",
        "outside what this setting allows",
    )
    .await;
    refused_beside(
        &e,
        &[("approvals", "many")],
        "approvals",
        "This must be a whole number.",
    )
    .await;
    refused_beside(
        &e,
        &[("review_days", "0")],
        "review_days",
        "outside what this setting allows",
    )
    .await;
    refused_beside(
        &e,
        &[("who_accept", "bogus")],
        "who_accept",
        "Choose one of the options.",
    )
    .await;
    refused_beside(
        &e,
        &[("everyone", "admin")],
        "everyone",
        "Choose one of the options.",
    )
    .await;
    refused_beside(
        &e,
        &[("promo-reviewer", "one line\nand another")],
        "promo-reviewer",
        "Write it on one line.",
    )
    .await;
    refused_beside(
        &e,
        &[("change_rung", "reviewer")],
        "change_rung",
        "Choose one of the options.",
    )
    .await;
    refused_beside(
        &e,
        &[("change_approvals", "-1")],
        "change_approvals",
        "outside what this setting allows",
    )
    .await;
    // A protection with no reason, one written as a page is not named, and one protected twice.
    let row = |n: usize, kind: &str, title: &str, reason: &str| -> Vec<(String, String)> {
        vec![
            (format!("row-{n}-kind"), kind.to_owned()),
            (format!("row-{n}-title"), title.to_owned()),
            (format!("row-{n}-asks"), "review".to_owned()),
            (format!("row-{n}-reason"), reason.to_owned()),
        ]
    };
    let with_rows = |rows: Vec<Vec<(String, String)>>| {
        let mut f = vec![("rows".to_owned(), rows.len().to_string())];
        f.extend(rows.into_iter().flatten());
        f
    };
    for (rows, control, sentence) in [
        (
            vec![row(1, "page", "Home", " ")],
            "row-1-reason",
            "This cannot be empty.",
        ),
        (
            vec![row(1, "page", "Help/", "Why.")],
            "row-1-title",
            "Write the title as the page is named",
        ),
        (
            vec![
                row(1, "page", "Home", "Why."),
                row(2, "page", "Home", "Why."),
            ],
            "row-2-title",
            "protected more than once",
        ),
        (
            // A blank row between two is not counted: the second row is the second the person sees.
            vec![
                row(1, "page", "Home", "Why."),
                row(2, "page", "", ""),
                row(3, "page", "Home", "Again."),
            ],
            "row-3-title",
            "protected more than once",
        ),
    ] {
        let mut fields = opened(&e, &e.maintainer, "send").await;
        for (name, v) in with_rows(rows) {
            fields = with(&fields, &name, &v);
        }
        let a = post_fields(&e.w, Some(&e.maintainer), &fields).await;
        assert_eq!(
            a.status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{control}: {}",
            a.body()
        );
        let main = main_of(&a);
        let at = main
            .find(&format!("id=\"c-{control}-problem\""))
            .unwrap_or_else(|| panic!("{control}: {main}"));
        assert!(
            main.get(at..).unwrap_or("").contains(sentence),
            "{control}: {sentence}"
        );
    }
    assert_eq!(proposals_made(&e), 0);
}

#[tokio::test]
async fn every_refusal_the_form_can_make_is_in_japanese_too() {
    let e = env();
    let mut fields = opened(&e, &e.maintainer, "send").await;
    fields = with(
        &with(&fields, "who_propose", "maintainer"),
        "who_review",
        "reviewer",
    );
    let borrowed: Vec<(&str, &str)> = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let body: String = borrowed
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                inklkunn_markup::percent_encode(k),
                inklkunn_markup::percent_encode(v)
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    let a = common::send_body(
        &e.w,
        "POST",
        EDIT,
        Some(&e.maintainer),
        &[
            ("sec-fetch-site", "same-origin"),
            ("cookie", "inklkunn_lang=ja"),
            ("x-csrf-token", common::TEST_CSRF),
        ],
        body,
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());
    assert!(
        main_of(&a).contains("提案できる役割は、レビューできる役割より上であってはなりません。"),
        "{}",
        a.body()
    );
}

#[tokio::test]
async fn going_back_to_the_defaults_removes_the_files_that_are_there() {
    let e = env_under(RICH);
    e.w.fake.set_files(&[
        (POLICY_FILE, "entry: observer\n"),
        (CONSTITUTION_FILE, "policy_change:\n  approvals: 1\n"),
    ]);
    let fields = opened(&e, &e.maintainer, "defaults").await;
    let id = proposal_of(&post_fields(&e.w, Some(&e.maintainer), &fields).await);
    assert_eq!(
        files_of(&e.w, &id),
        vec![
            (CONSTITUTION_FILE.to_owned(), None),
            (POLICY_FILE.to_owned(), None)
        ]
    );
    // With no file to remove there is nothing to go back to.
    let none = env();
    let fields = opened(&none, &none.maintainer, "defaults").await;
    let a = post_fields(&none.w, Some(&none.maintainer), &fields).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(
        main_of(&a).contains("already follows the default rules"),
        "{}",
        a.body()
    );
    assert_eq!(proposals_made(&none), 0);
}

#[tokio::test]
async fn a_change_to_the_rules_is_never_mixed_with_a_page() {
    let e = env();
    let mut fields = with(&opened(&e, &e.maintainer, "send").await, "approvals", "1");
    // Fields of a page's form, sent along: no part of the form reads them.
    for (k, v) in [
        ("p0_path", "pages/Home.md"),
        ("p0_text", "A page, changed alongside."),
        ("pages", "1"),
    ] {
        fields = with(&fields, k, v);
    }
    let id = proposal_of(&post_fields(&e.w, Some(&e.maintainer), &fields).await);
    let paths: Vec<String> = files_of(&e.w, &id).into_iter().map(|(p, _)| p).collect();
    assert_eq!(paths, [POLICY_FILE]);
    // The proposal service refuses a mix by name as well, whatever sends it (handoff G-T1 correction C-1).
}

#[tokio::test]
async fn who_may_open_and_send_the_form_is_who_may_propose_and_the_others_are_told_why() {
    let e = env();
    // Anyone who may propose.
    for who in [&e.contributor, &e.reviewer, &e.maintainer, &e.admin] {
        assert_eq!(as_(&e, who, EDIT).await.status, StatusCode::OK);
    }
    // A visitor is told that proposing needs an account.
    let visitor = common::get(&e.w, EDIT).await;
    assert_eq!(visitor.status, StatusCode::FORBIDDEN);
    assert!(visitor.body().contains("account"), "{}", visitor.body());
    // Someone who may not propose is told so, and nothing is made.
    for who in [&e.observer, &e.newcomer] {
        let a = as_(&e, who, EDIT).await;
        assert_eq!(a.status, StatusCode::FORBIDDEN, "{who}");
        let fields = with(&opened(&e, &e.maintainer, "send").await, "approvals", "1");
        let posted = post_fields(&e.w, Some(who), &fields).await;
        assert_eq!(posted.status, StatusCode::FORBIDDEN, "{}", posted.body());
    }
    assert_eq!(proposals_made(&e), 0);
    // A rule that asks more to propose is held to: the form follows the rung of the rules in force.
    let strict = env_under("rungs:\n  propose: maintainer\n  review: maintainer\n");
    assert_eq!(
        as_(&strict, &strict.contributor, EDIT).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        as_(&strict, &strict.maintainer, EDIT).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn a_post_needs_the_sessions_token_and_a_form_from_this_site() {
    let e = env();
    let fields = with(&opened(&e, &e.maintainer, "send").await, "approvals", "1");
    let body: String = fields
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                inklkunn_markup::percent_encode(k),
                inklkunn_markup::percent_encode(v)
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    // No token: refused, nothing made.
    let a = common::send_body(
        &e.w,
        "POST",
        EDIT,
        Some(&e.maintainer),
        &[("sec-fetch-site", "same-origin"), (common::NO_TOKEN, "")],
        body.clone(),
    )
    .await;
    assert!(a.status.is_client_error(), "{}", a.body());
    assert_ne!(a.status, StatusCode::SEE_OTHER);
    // From another site: refused.
    let b = common::send_body(
        &e.w,
        "POST",
        EDIT,
        Some(&e.maintainer),
        &[("sec-fetch-site", "cross-site")],
        body,
    )
    .await;
    assert_eq!(b.status, StatusCode::FORBIDDEN, "{}", b.body());
    assert_eq!(proposals_made(&e), 0);
}

#[tokio::test]
async fn while_the_rules_cannot_be_read_only_an_admin_is_offered_the_repair_and_it_names_the_fault()
{
    let fault = ReadFault::from_stored(GovernanceFile::Policy, Some(3), Refusal::WrongShape);
    let e = env_rules(Rules::faulted(Some(common::POLICY_BLOCK.to_owned()), fault));
    // Not an admin: told so, with the fault.
    let a = as_(&e, &e.maintainer, EDIT).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN, "{}", a.body());
    assert!(
        a.body()
            .contains("Only an admin of the wiki can propose the repair."),
        "{}",
        a.body()
    );
    assert!(
        a.body().contains("governance/policy.yaml, line 3"),
        "{}",
        a.body()
    );
    // An admin: the form, from the default rules, naming the fault.
    let a = as_(&e, &e.admin, EDIT).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let main = main_of(&a);
    assert!(
        main.contains("The rules cannot be read, so the form starts from the default rules."),
        "{main}"
    );
    assert!(main.contains("governance/policy.yaml, line 3"), "{main}");
    let fields = opened(&e, &e.admin, "send").await;
    // Sent unchanged, it writes the file that could not be read, in its canonical text, and nothing else.
    let id = proposal_of(&post_fields(&e.w, Some(&e.admin), &fields).await);
    assert_eq!(
        files_of(&e.w, &id),
        vec![(POLICY_FILE.to_owned(), Some(Policy::default().to_yaml()))]
    );
}

#[tokio::test]
async fn where_something_that_is_not_a_file_stands_the_repair_removes_what_was_found() {
    let fault = ReadFault::from_stored(GovernanceFile::Policy, None, Refusal::NotAFile)
        .with_found(vec!["governance/policy.yaml/a.txt".to_owned()], 0);
    let e = env_rules(Rules::faulted(None, fault));
    let fields = opened(&e, &e.admin, "send").await;
    let id = proposal_of(&post_fields(&e.w, Some(&e.admin), &fields).await);
    assert_eq!(
        files_of(&e.w, &id),
        vec![
            ("governance/policy.yaml/a.txt".to_owned(), None),
            (POLICY_FILE.to_owned(), Some(Policy::default().to_yaml())),
        ]
    );
}
