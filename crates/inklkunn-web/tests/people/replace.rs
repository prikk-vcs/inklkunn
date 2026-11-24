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
async fn the_list_of_files_that_cannot_be_read_loses_a_file_when_what_the_form_proposes_is_in_the_history()
 {
    // Criterion 11, the end of it (readiness gap 6), from the page: the file is on the list; the form proposes the file that is read
    // in its place and the list is unchanged while the proposal waits; once the repository holds what the proposal writes (what the
    // acceptance seals: `core`'s `the_list_of_files_that_cannot_be_read_holds_a_file_until_its_replacement_is_signed_and_not_after`),
    // the list has no file, in the page and in the API.
    for (choice, text, summary) in [
        ("retire", "", "Retire the unreadable claim"),
        (
            "write",
            "kind: observation\nlifecycle: proposed\ncommunity_status:\n  status: unknown\nexpressions: []\nrelations: []\nsources: []\n",
            "Write the claim again",
        ),
    ] {
        let e = env();
        with_an_unreadable_file(&e);
        let page = "/handbook/-/health/knowledge-files";
        let api = "/handbook/-/api/v1/health?list=knowledge-files";
        for uri in [page, api] {
            let body = as_(&e, &e.maintainer, uri).await.body();
            assert!(body.contains(&unreadable_path()), "{choice}: {uri}: {body}");
        }
        let a = send(
            &e,
            &e.maintainer,
            &fields(choice, text, "It held nothing anyone can read.", summary),
        )
        .await;
        assert_eq!(a.status, StatusCode::SEE_OTHER, "{choice}: {}", a.body());
        // Waiting changes nothing.
        let to = a.header("location").expect("a location").to_owned();
        for uri in [page, api] {
            let body = as_(&e, &e.maintainer, uri).await.body();
            assert!(body.contains(&unreadable_path()), "{choice}: {uri}: {body}");
        }
        // The repository holds what the proposal writes.
        let files = files_of(&e, &to);
        let [(path, Some(written))] = files.as_slice() else {
            panic!("{choice}: {files:?}");
        };
        assert_eq!(*path, unreadable_path());
        e.w.fake.set(path, written);
        for uri in [page, api] {
            let body = as_(&e, &e.maintainer, uri).await.body();
            assert!(
                !body.contains(&unreadable_path()),
                "{choice}: {uri} still lists the file that is read now: {body}"
            );
        }
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

// ------------------------------------------------------------------------------------ the kinds of file (S-2)

/// A file the wiki cannot read, by the way it cannot: its bytes, and the sentence the form says why in.
struct Unreadable {
    name: &'static str,
    bytes: Vec<u8>,
    why: &'static str,
}

/// Not UTF-8; over what a knowledge file may be; over even what a page read allows (the bound the old decision stumbled on: the
/// read was an error, so not even a moderator could replace the file); and not in the format.
fn kinds() -> Vec<Unreadable> {
    vec![
        Unreadable {
            name: "not UTF-8",
            bytes: [&[0xff, 0xfe, 0x00][..], b"SECRETBYTES"].concat(),
            why: "Why the wiki cannot read it: The file is not UTF-8 text.",
        },
        Unreadable {
            name: "over what a knowledge file may be",
            bytes: vec![b'a'; inklkunn_core::knowledge::MAX_FILE_BYTES + 10],
            why: "The file is larger than the 65536 bytes a knowledge file may have.",
        },
        Unreadable {
            name: "over what a page read allows",
            bytes: vec![b'a'; (1 << 20) + 10],
            why: "The file is larger than the 65536 bytes a knowledge file may have.",
        },
        Unreadable {
            name: "not in the format",
            bytes: b"kind: observation\nsecret: words\n".to_vec(),
            why: "Why the wiki cannot read it: The file has a key the format does not have.",
        },
    ]
}

fn with_unreadable(e: &Env, kind: &Unreadable) {
    e.w.fake.set_bytes(&unreadable_path(), &kind.bytes);
}

const READABLE: &str = "kind: observation\nlifecycle: proposed\ncommunity_status:\n  status: unknown\nexpressions: []\nrelations: []\nsources: []\n";

fn principal_of(who: &AccountId) -> Principal {
    Principal::session(&IdentityAuthority::for_tests(), who.clone())
}

fn replace_edit(text: Option<&str>) -> inklkunn_core::proposal::ProposalEdit {
    inklkunn_core::proposal::ProposalEdit {
        summary: "Replace it".to_owned(),
        files: vec![FileChange {
            path: unreadable_path(),
            text: text.map(str::to_owned),
        }],
        ..Default::default()
    }
}

#[tokio::test]
async fn whoever_only_proposes_cannot_replace_or_remove_a_file_of_any_kind_the_wiki_cannot_read() {
    for kind in kinds() {
        let e = env();
        with_unreadable(&e, &kind);
        for edit in [replace_edit(Some(READABLE)), replace_edit(None)] {
            let result = e.w.services.proposals.create_and_submit(
                &principal_of(&e.contributor),
                &e.w.wiki,
                None,
                edit,
            );
            assert!(
                matches!(result, Err(ProposalError::NotAllowed)),
                "{}: {result:?}",
                kind.name
            );
        }
        // And the people the rules hold below the rung are refused as the form refuses them.
        let a = send(
            &e,
            &e.contributor,
            &fields("retire", "", "A reason", "Retire it"),
        )
        .await;
        assert_eq!(
            a.status,
            StatusCode::FORBIDDEN,
            "{}: {}",
            kind.name,
            a.body()
        );
    }
}

#[tokio::test]
async fn a_moderator_replaces_or_retires_a_file_of_every_kind_and_the_form_says_why_and_never_what_it_held()
 {
    for kind in kinds() {
        let e = env();
        with_unreadable(&e, &kind);
        let list = as_(&e, &e.maintainer, "/handbook/-/health/knowledge-files")
            .await
            .body();
        assert!(list.contains(&unreadable_path()), "{}: {list}", kind.name);
        let form = as_(&e, &e.maintainer, &form_uri()).await;
        assert_eq!(
            form.status,
            StatusCode::OK,
            "{}: {}",
            kind.name,
            form.body()
        );
        let body = form.body();
        assert!(body.contains(&unreadable_path()), "{}: {body}", kind.name);
        assert!(body.contains(kind.why), "{}: {body}", kind.name);
        // Nothing of the bytes: not the readable words of a non-text file, not the key of a file that does not parse.
        assert!(!body.contains("SECRETBYTES"), "{}", kind.name);
        assert!(!body.contains("secret"), "{}", kind.name);
        // The replacement is written as it always was, and retiring is too.
        let a = send(
            &e,
            &e.maintainer,
            &fields("write", READABLE, "", "Write it again"),
        )
        .await;
        assert_eq!(
            a.status,
            StatusCode::SEE_OTHER,
            "{}: {}",
            kind.name,
            a.body()
        );
        assert_eq!(
            files_of(&e, a.header("location").expect("a location")),
            [(unreadable_path(), Some(READABLE.to_owned()))],
            "{}",
            kind.name
        );
        let a = send(
            &e,
            &e.maintainer,
            &fields("retire", "", "Nothing of it can be read", "Retire it"),
        )
        .await;
        assert_eq!(
            a.status,
            StatusCode::SEE_OTHER,
            "{}: {}",
            kind.name,
            a.body()
        );
        // The service takes the removal of such a file from the rung too; and a change that is not a curation proposal (a page
        // beside the file) is not what the rung is for.
        let removal = e.w.services.proposals.create_and_submit(
            &principal_of(&e.maintainer),
            &e.w.wiki,
            None,
            replace_edit(None),
        );
        assert!(removal.is_ok(), "{}: {removal:?}", kind.name);
        let mut beside_a_page = replace_edit(Some(READABLE));
        beside_a_page.files.push(FileChange {
            path: "pages/Home.md".to_owned(),
            text: Some(common::page(&["title: Home"], "Changed.\n")),
        });
        let result = e.w.services.proposals.create_and_submit(
            &principal_of(&e.maintainer),
            &e.w.wiki,
            None,
            beside_a_page,
        );
        assert!(
            matches!(
                result,
                Err(ProposalError::Invalid(
                    inklkunn_core::proposal::Invalid::KnowledgeCurated
                ))
            ),
            "{}: {result:?}",
            kind.name
        );
    }
}

#[tokio::test]
async fn a_maintainer_under_rules_that_hold_the_moderate_rung_at_admin_is_refused_for_every_kind() {
    for kind in kinds() {
        let e = env_under("rungs:\n  moderate: admin\n");
        with_unreadable(&e, &kind);
        let a = as_(&e, &e.maintainer, &form_uri()).await;
        assert_eq!(
            a.status,
            StatusCode::FORBIDDEN,
            "{}: {}",
            kind.name,
            a.body()
        );
        let result = e.w.services.proposals.create_and_submit(
            &principal_of(&e.maintainer),
            &e.w.wiki,
            None,
            replace_edit(Some(READABLE)),
        );
        assert!(
            matches!(result, Err(ProposalError::NotAllowed)),
            "{}: {result:?}",
            kind.name
        );
        let a = send(&e, &e.admin, &fields("retire", "", "A reason", "Retire it")).await;
        assert_eq!(
            a.status,
            StatusCode::SEE_OTHER,
            "{}: {}",
            kind.name,
            a.body()
        );
    }
}
