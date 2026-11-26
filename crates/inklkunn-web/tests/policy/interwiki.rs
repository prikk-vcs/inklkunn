//! The interwiki map on the surfaces (RFC 007 D-8; handoff I-T4): the policy page shows it, read-only, and says so when it cannot be
//! read; a proposal that changes it says the change as sentences and what accepting it takes (the policy change's); the repair of a map that cannot be read removes it; the
//! site description publishes it; and a link with one of its prefixes is an external link on the page.

use inklkunn_core::governance::{GovernanceFile, ReadFault, Refusal, Rules};
use inklkunn_core::{IdentityAuthority, Principal, ProposalEdit};
use inklkunn_store::{FileChange, ProposalFilter, ProposalStore, Visibility, WikiRecord};

use super::*;

const MAP_FILE: &str = "governance/interwiki.yaml";
const MAP: &str = "wikipedia: https://en.wikipedia.org/wiki/{title}\ndocs: \"https://example.org/docs?page={title}\"\n";

/// A public wiki that holds the map, under the default rules.
fn env_with_map(map: &str) -> Env {
    let w = common::World::with_visibility(
        "handbook",
        "The Handbook",
        Visibility::Public,
        &[
            (
                "pages/Home.md",
                &common::home_with("See [[wikipedia:Rust (language)]] and [[nope:Thing]]."),
            ),
            (MAP_FILE, map),
        ],
    );
    cast(common::with_accounts(w))
}

fn principal(who: &inklkunn_store::AccountId) -> Principal {
    Principal::session(&IdentityAuthority::for_tests(), who.clone())
}

/// A held proposal that writes `text` as the map (`None`: removes it), made by `who`.
fn propose(
    e: &Env,
    who: &inklkunn_store::AccountId,
    text: Option<&str>,
) -> inklkunn_store::ProposalId {
    let p = principal(who);
    let wiki: &WikiRecord = &e.w.wiki;
    let edit = ProposalEdit {
        summary: "The map".to_owned(),
        files: vec![FileChange {
            path: MAP_FILE.to_owned(),
            text: text.map(str::to_owned),
        }],
        ..ProposalEdit::default()
    };
    let draft =
        e.w.services
            .proposals
            .create(&p, wiki, None, edit)
            .expect("a draft");
    e.w.services
        .proposals
        .submit(&p, wiki, &draft.id)
        .expect("held for review")
        .id
}

#[tokio::test]
async fn the_policy_page_lists_the_prefixes_and_where_they_lead_and_says_when_there_are_none() {
    let e = env_with_map(MAP);
    let a = as_(&e, &e.contributor, PAGE).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let main = main_of(&a);
    for want in [
        "Interwiki map",
        "A link written with one of these prefixes goes to the address shown",
        "<dt>docs</dt>",
        "https://example.org/docs?page={title}",
        "<dt>wikipedia</dt>",
        "https://en.wikipedia.org/wiki/{title}",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    assert!(!main.contains("This wiki has no interwiki map."), "{main}");
    let ja = as_ja(&e, Some(&e.contributor), PAGE).await;
    for want in [
        "Wiki 間リンクの対応表",
        "https://en.wikipedia.org/wiki/{title}",
    ] {
        assert!(main_of(&ja).contains(want), "{want}: {}", ja.body());
    }
    // A wiki with no map says so.
    let none = env();
    let main = main_of(&as_(&none, &none.contributor, PAGE).await);
    assert!(main.contains("This wiki has no interwiki map."), "{main}");
}

#[tokio::test]
async fn a_map_that_cannot_be_read_is_named_by_its_file_and_line_and_is_not_listed_as_none() {
    let fault = ReadFault::from_stored(GovernanceFile::Interwiki, Some(2), Refusal::TemplateScheme);
    let e = env_rules(Rules::faulted(Some(common::POLICY_BLOCK.to_owned()), fault));
    let main = main_of(&as_(&e, &e.contributor, PAGE).await);
    for want in [
        "governance/interwiki.yaml, line 2",
        "An address template of the interwiki map is not an https address.",
        "The interwiki map could not be read now.",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    assert!(!main.contains("This wiki has no interwiki map."), "{main}");
}

#[tokio::test]
async fn a_proposal_that_changes_the_map_says_each_prefix_added_removed_or_sent_elsewhere_and_what_accepting_it_takes()
 {
    let e = env_with_map(MAP);
    let id = propose(
        &e,
        &e.contributor,
        Some(
            "wikipedia: https://de.wikipedia.org/wiki/{title}\nnew: https://new.example/{title}\n",
        ),
    );
    let a = as_(&e, &e.maintainer, &format!("/handbook/-/proposals/{id}")).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let main = main_of(&a);
    for want in [
        "What this proposal changes in the rules",
        "Interwiki prefix removed: docs (it led to https://example.org/docs?page={title})",
        "Interwiki prefix added: new → https://new.example/{title}",
        "Interwiki prefix changed: wikipedia led to https://en.wikipedia.org/wiki/{title} and now leads to https://de.wikipedia.org/wiki/{title}",
        // What it takes is what a change to the policy takes.
        "Accepting this change takes: maintainer or above",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    let ja = as_ja(
        &e,
        Some(&e.maintainer),
        &format!("/handbook/-/proposals/{id}"),
    )
    .await;
    for want in [
        "Wiki 間リンクの接頭辞を追加: new → https://new.example/{title}",
        "Wiki 間リンクの接頭辞を削除: docs（移動先は https://example.org/docs?page={title} でした）",
    ] {
        assert!(main_of(&ja).contains(want), "{want}: {}", ja.body());
    }
    // The removal of the file is said as a change too: every prefix goes.
    let gone = propose(&e, &e.contributor, None);
    let main = main_of(&as_(&e, &e.maintainer, &format!("/handbook/-/proposals/{gone}")).await);
    assert!(
        main.contains(
            "Interwiki prefix removed: wikipedia (it led to https://en.wikipedia.org/wiki/{title})"
        ),
        "{main}"
    );
}

#[tokio::test]
async fn the_repair_of_a_map_that_cannot_be_read_removes_it_and_back_to_the_defaults_leaves_a_sound_one()
 {
    // The form is for an admin, starts from the default rules, and sent unchanged takes the broken map away.
    let fault = ReadFault::from_stored(GovernanceFile::Interwiki, Some(1), Refusal::TemplateHost);
    let e = env_rules(Rules::faulted(Some(common::POLICY_BLOCK.to_owned()), fault));
    let fields = opened(&e, &e.admin, "send").await;
    let id = proposal_of(&post_fields(&e.w, Some(&e.admin), &fields).await);
    assert_eq!(files_of(&e.w, &id), vec![(MAP_FILE.to_owned(), None)]);
    // Back to the default rules does not take a map that reads away: it is not a rule of who may do what.
    let e = env_with_map(MAP);
    e.w.fake
        .set_files(&[("governance/policy.yaml", "entry: observer\n")]);
    let fields = opened(&e, &e.maintainer, "defaults").await;
    let id = proposal_of(&post_fields(&e.w, Some(&e.maintainer), &fields).await);
    assert_eq!(
        files_of(&e.w, &id),
        vec![("governance/policy.yaml".to_owned(), None)]
    );
    // With only the map there, there is nothing to go back to.
    let only = env_with_map(MAP);
    let fields = opened(&only, &only.maintainer, "defaults").await;
    let a = post_fields(&only.w, Some(&only.maintainer), &fields).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    assert_eq!(
        only.w
            .store
            .proposals(&only.w.wiki.id, &ProposalFilter::default(), None)
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn the_site_description_publishes_the_map_and_a_page_links_through_it() {
    let e = env_with_map(MAP);
    let a = as_(&e, &e.contributor, "/handbook/-/site.json").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let body = a.body();
    let at = body.find("\"interwiki\":{").expect("the key");
    assert!(
        body.get(at..).is_some_and(|t| t.contains(
            "\"docs\":\"https://example.org/docs?page={title}\",\"wikipedia\":\"https://en.wikipedia.org/wiki/{title}\"}"
        )),
        "{body}"
    );
    // A wiki with no map publishes an empty one, as it did.
    let none = env();
    let body = as_(&none, &none.contributor, "/handbook/-/site.json")
        .await
        .body();
    assert!(body.contains("\"interwiki\":{}"), "{body}");
    // The page: a mapped prefix is an external link and an unmapped one is a link to a page that is not there.
    let page = as_(&e, &e.contributor, "/handbook/Home").await.body();
    assert!(
        page.contains("href=\"https://en.wikipedia.org/wiki/Rust%20%28language%29\""),
        "{page}"
    );
    assert!(page.contains("rel=\"nofollow ugc\""), "{page}");
    assert!(page.contains("wikilink-missing"), "{page}");
}
