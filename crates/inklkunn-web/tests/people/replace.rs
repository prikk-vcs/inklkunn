//! Replacing a knowledge file the wiki cannot read, or retiring it (RFC 006 D-10; handoff G-T6; acceptance criterion 11): the form the
//! list of files that cannot be read leads to, **for the `moderate` rung and nobody else**, the proposal it makes (a curation proposal
//! that writes a file the wiki reads in the file's place, with the identifier its name gives), and the rung asked of whoever sends any
//! such proposal. The roles are those of `people`: an admin, a maintainer, a contributor and an observer, a wiki under the default rules
//! and one under rules that hold the rung that moderates at admin.

use inklkunn_core::knowledge::{File, Lifecycle, claim_path, read_file};
use inklkunn_core::{IdentityAuthority, Principal, ProposalError};
use inklkunn_store::{FileChange, ProposalId};

use super::*;

const BROKEN: &str = "01930000-0000-7000-8000-0000000000aa";

fn unreadable_path() -> String {
    format!("knowledge/claims/{BROKEN}.yaml")
}

/// The wiki holds a claim file it cannot read (`secret` is not a key of the format), and its words are nowhere on any page.
fn with_an_unreadable_file(e: &Env) {
    e.w.fake
        .set(&unreadable_path(), "kind: observation\nsecret: words\n");
}

fn form_uri() -> String {
    format!(
        "/handbook/-/knowledge/replace?file={}",
        inklkunn_markup::percent_encode(&unreadable_path())
    )
}

/// What the form sends, as owned texts.
type Fields = Vec<(&'static str, String)>;

fn fields(choice: &str, text: &str, reason: &str, summary: &str) -> Fields {
    vec![
        ("file", unreadable_path()),
        ("base_state", common::state_id(2)),
        ("base_block", common::block_id(2)),
        ("choice", choice.to_owned()),
        ("text", text.to_owned()),
        ("reason", reason.to_owned()),
        ("summary", summary.to_owned()),
    ]
}

/// A post of the form as `who`.
async fn send(e: &Env, who: &AccountId, sent: &Fields) -> Answer {
    let borrowed: Vec<(&str, &str)> = sent.iter().map(|(k, v)| (*k, v.as_str())).collect();
    post_form(&e.w, "/handbook/-/knowledge/replace", Some(who), &borrowed).await
}

fn files_of(e: &Env, location: &str) -> Vec<(String, Option<String>)> {
    let id = location
        .rsplit('/')
        .next()
        .and_then(ProposalId::parse)
        .expect("a proposal id");
    let principal = Principal::session(&IdentityAuthority::for_tests(), e.maintainer.clone());
    e.w.services
        .proposals
        .revisions(&principal, &e.w.wiki, &id)
        .unwrap()
        .pop()
        .map(|r| r.content.files)
        .expect("a revision")
        .into_iter()
        .map(|f| (f.path, f.text))
        .collect()
}

// ------------------------------------------------------------------------------------ the list leads to the form

#[tokio::test]
async fn the_list_of_files_that_cannot_be_read_leads_whoever_may_moderate_to_the_form() {
    let e = env();
    with_an_unreadable_file(&e);
    let list = "/handbook/-/health/knowledge-files";
    for who in [&e.admin, &e.maintainer] {
        let body = as_(&e, who, list).await.body();
        assert!(body.contains(&unreadable_path()), "{body}");
        assert!(body.contains("Replace it or retire it"), "{body}");
        assert!(
            body.contains("/handbook/-/knowledge/replace?file=knowledge/claims"),
            "{body}"
        );
    }
    // Everyone is told who may: the sentence that replaced *propose a corrected file, or propose to remove it*.
    for body in [
        as_(&e, &e.contributor, list).await.body(),
        as_(&e, &e.observer, list).await.body(),
        common::get(&e.w, list).await.body(),
    ] {
        assert!(
            body.contains("Someone who may moderate this wiki can replace it with a file that can be read, or retire it."),
            "{body}"
        );
        assert!(!body.contains("Replace it or retire it"), "{body}");
        assert!(!body.contains("knowledge/replace"), "{body}");
        assert!(!body.contains("propose to remove it"), "{body}");
    }
}

#[tokio::test]
async fn the_form_is_the_moderate_rungs_and_is_for_a_file_the_wiki_cannot_read() {
    let e = env();
    with_an_unreadable_file(&e);
    let a = as_(&e, &e.maintainer, &form_uri()).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let body = a.body();
    assert!(
        body.contains("<h1>Replace a file that cannot be read</h1>"),
        "{body}"
    );
    assert!(body.contains(&unreadable_path()), "{body}");
    // What the file held is not shown: nothing of it is read.
    assert!(!body.contains("secret"), "{body}");
    for name in ["choice", "text", "reason", "summary", "csrf"] {
        assert!(body.contains(&format!("name=\"{name}\"")), "{name}: {body}");
    }
    // Others are refused, in the words of a refusal; an account is needed.
    for who in [&e.contributor, &e.observer, &e.reviewer] {
        let a = as_(&e, who, &form_uri()).await;
        assert_eq!(a.status, StatusCode::FORBIDDEN, "{}", a.body());
        assert!(!a.body().contains("name=\"choice\""), "{}", a.body());
    }
    assert_eq!(
        common::get(&e.w, &form_uri()).await.status,
        StatusCode::FORBIDDEN
    );
    // A file the wiki can read, a page, a file of no family and nothing at all: there is nothing to replace.
    for file in [
        claim_path(
            &inklkunn_core::knowledge::ClaimId::parse("01930000-0000-7000-8000-0000000000bb")
                .unwrap(),
        ),
        "pages/Home.md".to_owned(),
        "knowledge/other.yaml".to_owned(),
        String::new(),
    ] {
        let uri = format!(
            "/handbook/-/knowledge/replace?file={}",
            inklkunn_markup::percent_encode(&file)
        );
        let a = as_(&e, &e.maintainer, &uri).await;
        assert_eq!(a.status, StatusCode::NOT_FOUND, "{file}: {}", a.body());
        assert!(
            a.body().contains("there is nothing to replace"),
            "{file}: {}",
            a.body()
        );
    }
}

// ------------------------------------------------------------------------------------ the proposal

#[tokio::test]
async fn retiring_a_file_that_cannot_be_read_writes_a_retired_claim_with_the_id_its_name_gives() {
    let e = env();
    with_an_unreadable_file(&e);
    let a = send(
        &e,
        &e.maintainer,
        &fields(
            "retire",
            "",
            "  It held nothing anyone can read.  ",
            "Retire the unreadable claim",
        ),
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let to = a.header("location").expect("a location").to_owned();
    let files = files_of(&e, &to);
    let [(path, Some(text))] = files.as_slice() else {
        panic!("{files:?}");
    };
    assert_eq!(*path, unreadable_path());
    let File::Claim(claim) = read_file(path, text.as_bytes()).expect("a file the wiki reads")
    else {
        panic!("a claim");
    };
    assert_eq!(
        claim.id.as_str(),
        BROKEN,
        "the identifier is the file's name"
    );
    assert_eq!(claim.lifecycle, Lifecycle::Retired);
    assert_eq!(
        claim.retired_reason.as_deref(),
        Some("It held nothing anyone can read.")
    );
    // The proposal says what it is, and what the file is that it replaces.
    let page = as_(&e, &e.maintainer, &to).await.body();
    assert!(page.contains("This is a curation proposal"), "{page}");
    assert!(
        page.contains("Replaces or removes a claim file that cannot be read"),
        "{page}"
    );
    assert!(!page.contains("secret"), "{page}");
}

#[tokio::test]
async fn writing_the_file_again_is_judged_by_the_wikis_own_reader_and_what_is_wrong_is_said_on_the_form()
 {
    let e = env();
    with_an_unreadable_file(&e);
    let good = "kind: observation\nlifecycle: proposed\ncommunity_status:\n  status: unknown\nexpressions: []\nrelations: []\nsources: []\n";
    let a = send(
        &e,
        &e.maintainer,
        &fields("write", good, "", "Write the claim again"),
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let files = files_of(&e, a.header("location").expect("a location"));
    assert_eq!(files, [(unreadable_path(), Some(good.to_owned()))]);
    // A text the wiki cannot read either: refused on the form, by the rule it breaks, with what was typed still in it.
    let bad = "kind: observation\nsecret: words\n";
    let a = send(&e, &e.maintainer, &fields("write", bad, "", "Try again")).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());
    let body = a.body();
    assert!(
        body.contains("The file you wrote cannot be read either:"),
        "{body}"
    );
    assert!(
        body.contains("the file has a key the format does not have") || body.contains("key"),
        "{body}"
    );
    assert!(body.contains("Try again"), "{body}");
    // A retirement without a reason, no choice, and no summary: each is said on the form.
    for (what, expected) in [
        (
            fields("retire", "", "  ", "Retire it"),
            "Say why you are retiring it",
        ),
        (
            fields("", "", "x", "Nothing chosen"),
            "Choose what to do with the file.",
        ),
        (fields("retire", "", "A reason", ""), "summary"),
    ] {
        let a = send(&e, &e.maintainer, &what).await;
        assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());
        assert!(
            a.body().to_lowercase().contains(&expected.to_lowercase()),
            "{expected}: {}",
            a.body()
        );
    }
}

#[tokio::test]
async fn whoever_may_not_moderate_is_refused_the_act_and_the_proposal_wherever_it_is_made() {
    let e = env();
    with_an_unreadable_file(&e);
    for who in [&e.contributor, &e.observer, &e.reviewer] {
        let a = send(&e, who, &fields("retire", "", "A reason", "Retire it")).await;
        assert_eq!(a.status, StatusCode::FORBIDDEN, "{}", a.body());
    }
    // Any other way to propose the same is refused as the form is: a curation proposal that writes over the unreadable file, made by
    // someone who only proposes, is not one that can be sent; made by the `moderate` rung, it is.
    let edit = || {
        inklkunn_core::proposal::ProposalEdit {
        summary: "Replace it".to_owned(),
        files: vec![FileChange {
            path: unreadable_path(),
            text: Some(
                "kind: observation\nlifecycle: proposed\ncommunity_status:\n  status: unknown\nexpressions: []\nrelations: []\nsources: []\n"
                    .to_owned(),
            ),
        }],
        ..Default::default()
    }
    };
    let principal =
        |who: &AccountId| Principal::session(&IdentityAuthority::for_tests(), who.clone());
    assert!(matches!(
        e.w.services.proposals.create_and_submit(
            &principal(&e.contributor),
            &e.w.wiki,
            None,
            edit()
        ),
        Err(ProposalError::NotAllowed)
    ));
    assert!(
        e.w.services
            .proposals
            .create_and_submit(&principal(&e.maintainer), &e.w.wiki, None, edit())
            .is_ok()
    );
    // Taking the file away is the same act.
    let removal = inklkunn_core::proposal::ProposalEdit {
        summary: "Remove it".to_owned(),
        files: vec![FileChange {
            path: unreadable_path(),
            text: None,
        }],
        ..Default::default()
    };
    assert!(matches!(
        e.w.services.proposals.create_and_submit(
            &principal(&e.contributor),
            &e.w.wiki,
            None,
            removal
        ),
        Err(ProposalError::NotAllowed)
    ));
}

#[tokio::test]
async fn a_wiki_whose_rules_hold_the_moderate_rung_at_admin_asks_it_of_a_maintainer_too() {
    let e = env_under("rungs:\n  moderate: admin\n");
    with_an_unreadable_file(&e);
    let a = as_(&e, &e.maintainer, &form_uri()).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN, "{}", a.body());
    let a = send(
        &e,
        &e.maintainer,
        &fields("retire", "", "A reason", "Retire it"),
    )
    .await;
    assert_eq!(a.status, StatusCode::FORBIDDEN, "{}", a.body());
    let a = send(&e, &e.admin, &fields("retire", "", "A reason", "Retire it")).await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    // The list leads the admin to the form, and not the maintainer.
    let list = "/handbook/-/health/knowledge-files";
    assert!(
        as_(&e, &e.admin, list)
            .await
            .body()
            .contains("Replace it or retire it")
    );
    assert!(
        !as_(&e, &e.maintainer, list)
            .await
            .body()
            .contains("Replace it or retire it")
    );
}

#[tokio::test]
async fn a_restricted_account_is_refused_and_told_why() {
    let e = env();
    with_an_unreadable_file(&e);
    let a = post_form(
        &e.w,
        "/handbook/-/people/restrict",
        Some(&e.admin),
        &restrict_form(
            &e.maintainer.to_string(),
            "3",
            "posting the same link into every page",
        ),
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let a = as_(&e, &e.maintainer, &form_uri()).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN, "{}", a.body());
    assert!(
        a.body().contains("posting the same link into every page"),
        "{}",
        a.body()
    );
    let a = send(
        &e,
        &e.maintainer,
        &fields("retire", "", "A reason", "Retire it"),
    )
    .await;
    assert_eq!(a.status, StatusCode::FORBIDDEN, "{}", a.body());
    assert!(a.body().contains("You are restricted"), "{}", a.body());
}

#[tokio::test]
async fn the_sentences_that_said_not_offered_in_this_version_are_gone_in_both_languages() {
    use inklkunn_web::catalog::{EN, JA_DEF, Key, Lang, T};
    let en = T::new(EN);
    let ja = T::new(Lang::from_def(&JA_DEF));
    for t in [en, ja] {
        for key in [
            Key::KnowNotByForm,
            Key::KnowUnreadableByModerate,
            Key::HealthKnowledgeAction,
        ] {
            let text = t.get(key);
            assert!(!text.contains("in this version"), "{key:?}: {text}");
            assert!(!text.contains("このバージョンでは"), "{key:?}: {text}");
        }
    }
    // What replaced them says what is done: a removal is a retirement, and a file that cannot be read is the moderating rung's.
    assert!(
        en.get(Key::KnowNotByForm)
            .contains("retire it with a curation proposal")
    );
    assert!(
        en.get(Key::KnowUnreadableByModerate)
            .contains("someone who may moderate this wiki")
    );
    assert!(
        !en.get(Key::HealthKnowledgeAction)
            .contains("propose to remove it")
    );
}
