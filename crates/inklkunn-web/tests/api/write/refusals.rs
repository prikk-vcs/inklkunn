//! **Every refusal of an acceptance names its cause in the API** (threat model C-3g; readiness gap 4): each cause of the closed list
//! (`causes.rs`) is sent through the accept endpoint by an acceptor that answers it, and the problem it comes to is one a program can
//! act on: a status, a stable `type`, the `reason` (or the `word`) that says which, a title and a detail in each language, and each
//! code and each name is written in the description of the API. **A cause added to the list has no row here until it is given one**:
//! [`expected`] matches every cause and has no wildcard.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use inklkunn_core::proposal::{Invalid, ProposalError};
use inklkunn_core::{AcceptFailure, Accepted, Principal};
use inklkunn_store::{ProposalId, WikiRecord};
use inklkunn_web::{Acceptor, Services};

use super::{API, Cast, cast, id_of, open, post, tag_header, token};
use crate::causes::{ALL, Cause, cause_of, samples};

/// An acceptor that answers each ask with the failure it was given.
struct Scripted(Mutex<Option<AcceptFailure>>);

impl Acceptor for Scripted {
    fn accept(
        &self,
        _: &Principal,
        _: &WikiRecord,
        _: &ProposalId,
        _: u32,
    ) -> Result<Accepted, AcceptFailure> {
        Err(self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .unwrap_or(AcceptFailure::NotAllowed))
    }
}

fn serving(c: &mut Cast, failure: AcceptFailure) {
    c.w.services = std::sync::Arc::new(Services {
        acceptor: Some(std::sync::Arc::new(Scripted(Mutex::new(Some(failure))))),
        ..(*c.w.services).clone()
    });
}

/// What one problem said: the status, the code of its `type`, the name that says which (its `reason`, else its `word`), and the title
/// and the detail.
#[derive(Debug, PartialEq, Eq)]
struct Said {
    status: u16,
    code: String,
    name: Option<String>,
    title: String,
    detail: String,
}

/// The failure sent to the accept endpoint with the language asked for.
async fn sent(failure: &AcceptFailure, language: &str) -> Said {
    let mut c = cast();
    let p = open(&c, &c.contributor, true).await;
    serving(&mut c, failure.clone());
    let accept = format!("{API}/proposals/{}/acceptances", id_of(&p));
    let a = post(
        &c.w,
        &accept,
        &[
            token("accept", &c.maintainer),
            tag_header(&p),
            ("accept-language", language.to_owned()),
        ],
        "{}".to_owned(),
    )
    .await;
    let json = a.json();
    let text = |pointer: &str| {
        json.pointer(pointer)
            .and_then(|v| v.as_str())
            .map(str::to_owned)
    };
    Said {
        status: a.status.as_u16(),
        code: text("/type")
            .unwrap_or_default()
            .trim_start_matches("urn:inklkunn:problem:")
            .to_owned(),
        name: text("/reason").or_else(|| text("/word")),
        title: text("/title").unwrap_or_default(),
        detail: text("/detail").unwrap_or_default(),
    }
}

/// What a refusal of this cause is as a problem: the status, the code and the name. **No wildcard**: a cause added to the list is
/// given its row here before this compiles.
fn expected(failure: &AcceptFailure) -> (u16, &'static str, Option<&'static str>) {
    match cause_of(failure) {
        Cause::NotAllowed => (403, "forbidden", None),
        Cause::Restricted => (403, "forbidden", Some("restricted")),
        Cause::AuthorNotAllowed => (403, "forbidden", Some("author-not-allowed")),
        Cause::NotFound => (404, "not-found", None),
        Cause::Proposal => match failure {
            AcceptFailure::Proposal(ProposalError::Invalid(Invalid::GovernanceMixed)) => {
                (422, "unacceptable", Some("governance-mixed"))
            }
            _ => (409, "not-in-this-state", None),
        },
        Cause::Busy => (503, "busy", None),
        Cause::Refused => (429, "refused-limit", None),
        Cause::NeedsRecovery | Cause::Incomplete => (503, "needs-recovery", None),
        Cause::InProgress => (409, "acceptance-in-progress", None),
        Cause::Overlap => (409, "overlap", None),
        Cause::NothingToChange => (409, "nothing-to-change", None),
        Cause::KnowledgeInvalid => (502, "acceptance-failed", Some("knowledge-invalid")),
        Cause::GovernanceInvalid => (422, "unacceptable", Some("governance-invalid")),
        Cause::GovernanceRung => (403, "forbidden", Some("governance-rung")),
        Cause::ApprovalsMissing => (403, "forbidden", Some("approvals-missing")),
        Cause::SummaryHidden => (403, "forbidden", Some("summary-hidden")),
        Cause::Protected => (403, "forbidden", Some("protected")),
        Cause::RulesUnreadable => (403, "forbidden", Some("rules-unreadable")),
        Cause::Failed => (502, "acceptance-failed", Some("refused")),
        Cause::Unavailable | Cause::Store => (503, "unavailable", None),
    }
}

/// The causes that are one problem to a program, each for its reason: the wiki that needs recovery (an acceptance that did not finish
/// is the same state to whoever asks again), and the records or the repository that could not be used (the operator's, in the log).
fn shared(a: Cause, b: Cause) -> bool {
    let one = [Cause::NeedsRecovery, Cause::Incomplete];
    let other = [Cause::Unavailable, Cause::Store];
    (one.contains(&a) && one.contains(&b)) || (other.contains(&a) && other.contains(&b))
}

#[tokio::test]
async fn every_cause_of_a_refusal_comes_to_a_problem_that_names_it_in_both_languages() {
    let mut named: BTreeMap<(String, Option<String>), Vec<Cause>> = BTreeMap::new();
    for failure in samples() {
        let cause = cause_of(&failure);
        let (status, code, name) = expected(&failure);
        let en = sent(&failure, "en").await;
        let ja = sent(&failure, "ja").await;
        assert_eq!(
            (en.status, en.code.as_str(), en.name.as_deref()),
            (status, code, name),
            "{failure:?}"
        );
        // The language changes the words and nothing else.
        assert_eq!(
            (ja.status, &ja.code, &ja.name),
            (en.status, &en.code, &en.name),
            "{cause:?}"
        );
        for said in [&en, &ja] {
            assert!(
                !said.title.is_empty() && !said.detail.is_empty(),
                "{cause:?}: a problem with no title or no detail: {said:?}"
            );
        }
        assert!(
            !ja.title.is_ascii(),
            "{cause:?}: the Japanese is not Japanese: {ja:?}"
        );
        assert_ne!(en.title, ja.title, "{cause:?}");
        named
            .entry((en.code.clone(), en.name.clone()))
            .or_default()
            .push(cause);
    }
    // Each cause is told apart from the others by its code and its name, but where the list says two are one problem.
    for ((code, name), causes) in &named {
        for (i, a) in causes.iter().enumerate() {
            for b in causes.iter().skip(i + 1) {
                assert!(
                    a == b || shared(*a, *b),
                    "{a:?} and {b:?} come to the same problem ({code}, {name:?})"
                );
            }
        }
    }
    // Every cause was sent.
    let sent_causes: Vec<Cause> = named.values().flatten().copied().collect();
    for cause in ALL {
        assert!(sent_causes.contains(&cause), "{cause:?} was not sent");
    }
}

#[tokio::test]
async fn each_code_and_each_name_of_a_refusal_is_written_in_the_description_of_the_api() {
    let w = cast().w;
    let doc = super::super::get(&w, "/-/api/v1/openapi.json").await.text();
    let document: serde_json::Value = serde_json::from_str(&doc).expect("a document");
    let listed: Vec<String> = document
        .get("x-inklkunn-problems")
        .and_then(|p| p.as_array())
        .map(|p| {
            p.iter()
                .filter_map(|t| t.get("code").and_then(|c| c.as_str()).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        !listed.is_empty(),
        "the document lists no problem types: {:?}",
        document.get("x-inklkunn-problems")
    );
    for failure in samples() {
        let (_, code, name) = expected(&failure);
        assert!(
            listed.iter().any(|l| l == code),
            "{failure:?}: the code {code} is not among the problem types of the description: {listed:?}"
        );
        // The name a `reason` gives is written in the description, in the words of the code that sets it. (The `word` of an
        // `acceptance-failed` is the next test's.)
        if let Some(name) = name.filter(|_| code != "acceptance-failed") {
            assert!(
                doc.contains(&format!("`{name}`")),
                "{failure:?}: `{name}` is written nowhere in the description"
            );
        }
    }
}

#[tokio::test]
async fn the_word_of_an_acceptance_that_failed_is_written_in_the_description_of_the_api() {
    let w = cast().w;
    let doc = super::super::get(&w, "/-/api/v1/openapi.json").await.text();
    for failure in samples() {
        if let (_, "acceptance-failed", Some(word)) = expected(&failure) {
            assert!(
                doc.contains(&format!("`{word}`")),
                "{failure:?}: the word `{word}` is written nowhere in the description"
            );
        }
    }
}

#[tokio::test]
async fn every_word_the_acceptance_can_fail_with_is_in_the_description_of_the_problem_and_is_the_one_sent()
 {
    // The description's words are the core's list (not hand-written), but the problem of a retirement that is not finished, which
    // is its own code; and each is the word the problem carries for the failure that has it.
    let w = cast().w;
    let document: serde_json::Value =
        serde_json::from_str(&super::super::get(&w, "/-/api/v1/openapi.json").await.text())
            .expect("a document");
    let schema = document
        .pointer("/components/schemas/Problem/properties/word")
        .expect("the `word` member of the problem is in the schema");
    let listed: Vec<&str> = schema
        .get("enum")
        .and_then(|e| e.as_array())
        .expect("the words")
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    let from_core: Vec<&str> = inklkunn_core::acceptance::words::FAILED
        .into_iter()
        .filter(|word| *word != inklkunn_core::acceptance::words::RETIREMENT_UNFINISHED)
        .collect();
    assert_eq!(listed, from_core);
    let said = schema
        .get("description")
        .and_then(|d| d.as_str())
        .expect("a description");
    for word in &listed {
        assert!(said.contains(&format!("`{word}`")), "{word}: {said}");
    }
    // The problems the API sends carry words of the list.
    for failure in samples() {
        if let (_, "acceptance-failed", Some(word)) = expected(&failure) {
            assert!(
                listed.contains(&word),
                "{failure:?}: `{word}` is not listed"
            );
        }
    }
}

#[tokio::test]
async fn the_detail_of_no_problem_is_a_template_that_was_not_filled() {
    for failure in samples() {
        for language in ["en", "ja"] {
            let said = sent(&failure, language).await;
            assert!(
                !said.detail.contains('{') && !said.title.contains('{'),
                "{failure:?} ({language}): {said:?}"
            );
            assert!(
                language == "en" || !said.detail.is_ascii(),
                "{failure:?}: the detail is not Japanese: {said:?}"
            );
        }
    }
}

#[tokio::test]
async fn the_detail_of_a_refused_rules_file_is_the_sentence_the_page_shows_in_the_language_asked() {
    // The detail names the file, the rule and the line as the page of the same refusal does (never what the file said), then says
    // that nothing was changed.
    let governance = samples()
        .into_iter()
        .find(|f| matches!(f, AcceptFailure::GovernanceInvalid(_)))
        .expect("a sample of the cause");
    let en = sent(&governance, "en").await;
    let ja = sent(&governance, "ja").await;
    assert!(en.detail.contains("governance/policy.yaml"), "{en:?}");
    assert!(en.detail.contains("Nothing was changed"), "{en:?}");
    assert!(ja.detail.contains("governance/policy.yaml"), "{ja:?}");
    assert!(ja.detail.contains("何も変更しておらず"), "{ja:?}");
}
