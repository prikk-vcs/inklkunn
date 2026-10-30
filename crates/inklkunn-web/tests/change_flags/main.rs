//! The flags of changes through the real router (handoff 004 T6; RFC 004 D-10, C-13a): *minor* and
//! *automated* are the accepted proposal's, so the changes page, its feed and the read API filter on them,
//! label machine-assisted content with the person who accepted it, and say they are filtered. The records are a
//! memory store and the repository an in-memory double, so these run in both test runs.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;

use common::{NOW, World, block_id, get, import_message, page};
use inklkunn_store::{
    Account, AccountId, AccountStore, Base, DisplayName, FileChange, ProposalChange, ProposalId,
    ProposalOp, ProposalRecord, ProposalState, ProposalStore, ProposalTransition, RevisionContent,
    RevisionRecord, Visibility,
};
use serde_json::Value;

const PLAIN: &str = "019a0000-0000-7000-8000-0000000000f1";
const MINOR: &str = "019a0000-0000-7000-8000-0000000000f2";
const MACHINE: &str = "019a0000-0000-7000-8000-0000000000f3";

/// The message of a change accepted from the proposal `id`, as the service writes it.
fn message(summary: &str, id: &str) -> String {
    format!(
        "{summary}\n\nProposal: {id}\n\
         Proposed-By: 019a0000-0000-7000-8000-0000000000a1 (Aiko Tanaka)\n\
         Accepted-By: 019a0000-0000-7000-8000-0000000000a2 (Ben Okoye)\n\
         Base: {}\nKind: proposal\n",
        "b".repeat(64)
    )
}

/// A submitted proposal with this id whose accepted revision has these flags.
fn record(w: &World, who: &AccountId, id: &str, summary: &str, minor: bool, automated: bool) {
    let id = ProposalId::parse(id).unwrap();
    let proposal = ProposalRecord {
        id: id.clone(),
        wiki: w.wiki.id.clone(),
        view: "heads/main".to_owned(),
        state: ProposalState::Draft,
        author: who.clone(),
        summary: summary.to_owned(),
        sources: Vec::new(),
        superseded_by: None,
        created_at: NOW,
        updated_at: NOW,
        submitted_at: None,
        closed_at: None,
    };
    let content = RevisionContent {
        base: Base::new(&format!("{:064x}", 1_u64), &block_id(1)).unwrap(),
        files: vec![FileChange {
            path: "pages/Home.md".to_owned(),
            text: Some("text".to_owned()),
        }],
        renames: Vec::new(),
        minor,
        automated,
    };
    let step = |op, from, to| ProposalTransition {
        proposal: id.clone(),
        op,
        from,
        to,
        actor: who.clone(),
        reason: None,
        at: NOW,
    };
    w.store
        .create_proposal(
            &proposal,
            &content,
            &step(ProposalOp::Create, None, ProposalState::Draft),
        )
        .unwrap();
    w.store
        .change_proposal(&ProposalChange {
            transition: step(
                ProposalOp::Submit,
                Some(ProposalState::Draft),
                ProposalState::HeldForReview,
            ),
            text: None,
            revision: Some(RevisionRecord {
                proposal: id.clone(),
                seq: 1,
                content,
                created_at: NOW,
            }),
            superseded_by: None,
        })
        .unwrap();
}

/// A wiki with four changes on `main`: an import (the service's), a plain edit, a minor one and a
/// machine-assisted one.
fn world() -> World {
    let w = World::with_visibility(
        "handbook",
        "The Handbook",
        Visibility::Public,
        &[("pages/Home.md", &page(&["title: Home"], "Welcome.\n"))],
    );
    let who = AccountId::new();
    w.store
        .add_account(&Account {
            id: who.clone(),
            display_name: DisplayName::new("Aiko Tanaka").unwrap(),
            local: None,
            created_at: NOW,
            disabled: false,
        })
        .unwrap();
    record(&w, &who, PLAIN, "A plain edit", false, false);
    record(&w, &who, MINOR, "A minor edit", true, false);
    record(&w, &who, MACHINE, "A machine edit", false, true);
    w.fake.tip_message(&message("A machine edit", MACHINE));
    w.fake.touches(&block_id(1), &["pages/Home.md"]);
    for (n, text) in [
        (101, import_message("Import the handbook")),
        (102, message("A plain edit", PLAIN)),
        (103, message("A minor edit", MINOR)),
    ] {
        w.fake.push_change("heads/main", &block_id(n), &text);
        w.fake.touches(&block_id(n), &["pages/Home.md"]);
    }
    w
}

fn api(tail: &str) -> String {
    format!("/handbook/-/api/v1/{tail}")
}

/// The member at a JSON pointer.
fn at<'a>(v: &'a Value, pointer: &str) -> &'a Value {
    v.pointer(pointer)
        .unwrap_or_else(|| panic!("no {pointer} in {v}"))
}

fn summaries(body: &Value) -> Vec<String> {
    at(body, "/items")
        .as_array()
        .unwrap()
        .iter()
        .map(|i| at(i, "/summary").as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn the_changes_page_labels_flags_and_filters_on_them_and_says_so() {
    let w = world();
    let all = get(&w, "/handbook/-/changes").await.body();
    for text in [
        "A plain edit",
        "A minor edit",
        "A machine edit",
        "Import the handbook",
    ] {
        assert!(all.contains(text), "{text}: {all}");
    }
    // Machine-assisted content is labelled and attributed to the person who accepted it; minor is marked.
    assert!(
        all.contains("Machine-assisted content, accepted by Ben Okoye"),
        "{all}"
    );
    assert_eq!(
        all.matches("<small>Machine-assisted content").count(),
        1,
        "{all}"
    );
    assert_eq!(
        all.matches("<small>Minor change</small>").count(),
        1,
        "{all}"
    );
    // The form offers the choice, and each choice is a filter the page says is in use.
    assert!(
        all.contains("name=\"minor\"") && all.contains("Minor changes"),
        "{all}"
    );
    for (query, shown, hidden, says) in [
        (
            "minor=hide",
            "A plain edit",
            "A minor edit",
            "Minor changes are hidden",
        ),
        (
            "minor=only",
            "A minor edit",
            "A plain edit",
            "Only minor changes",
        ),
        (
            "automated=hide",
            "A minor edit",
            "A machine edit",
            "or with the help of a machine are hidden",
        ),
        (
            "automated=only",
            "A machine edit",
            "A plain edit",
            "Only changes made by the wiki service itself or with the help of a machine",
        ),
    ] {
        let a = get(&w, &format!("/handbook/-/changes?{query}")).await;
        assert_eq!(a.status, 200, "{query}");
        let body = a.body();
        assert!(body.contains(shown), "{query}: {body}");
        assert!(!body.contains(hidden), "{query}: {body}");
        assert!(body.contains("The list is filtered"), "{query}: {body}");
        assert!(body.contains(says), "{query}: {body}");
        // The feed link carries the filter, so the feed is the list.
        assert!(
            body.contains(&format!("href=\"/handbook/-/changes.atom?{query}")),
            "{query}: {body}"
        );
    }
    // The service's own import goes with the machine-assisted ones.
    let hand = get(&w, "/handbook/-/changes?automated=hide").await.body();
    assert!(!hand.contains("Import the handbook"), "{hand}");
}

#[tokio::test]
async fn the_feed_filters_on_the_flags_and_carries_them_as_categories_and_words() {
    let w = world();
    let minor = get(&w, "/handbook/-/changes.atom?minor=only").await;
    assert_eq!(minor.status, 200);
    let xml = minor.body();
    assert!(
        xml.contains("A minor edit") && !xml.contains("A plain edit"),
        "{xml}"
    );
    assert!(xml.contains("<category term=\"minor\"/>"), "{xml}");
    assert!(xml.contains("<summary type=\"text\">Minor change"), "{xml}");

    let machine = get(&w, "/handbook/-/changes.atom?automated=only")
        .await
        .body();
    assert!(machine.contains("A machine edit") && machine.contains("Import the handbook"));
    assert!(
        !machine.contains("A plain edit") && !machine.contains("A minor edit"),
        "{machine}"
    );
    assert!(
        machine.contains("<category term=\"automated\"/>"),
        "{machine}"
    );
    assert!(
        machine.contains("Machine-assisted content, accepted by Ben Okoye"),
        "{machine}"
    );

    let plain = get(&w, "/handbook/-/changes.atom?minor=hide&automated=hide")
        .await
        .body();
    assert!(plain.contains("A plain edit"), "{plain}");
    assert!(
        !plain.contains("A minor edit") && !plain.contains("A machine edit"),
        "{plain}"
    );
    assert!(!plain.contains("<category"), "{plain}");
    // The unfiltered feed has every change, flagged ones with their category.
    let every = get(&w, "/handbook/-/changes.atom").await.body();
    assert_eq!(every.matches("<entry>").count(), 4, "{every}");
    assert_eq!(every.matches("<category term=\"minor\"/>").count(), 1);
    assert_eq!(every.matches("<category term=\"automated\"/>").count(), 2);
}

#[tokio::test]
async fn the_read_api_filters_on_minor_for_real_and_reports_the_flags() {
    let w = world();
    let body: Value = json(&get(&w, &api("changes?minor=only")).await);
    assert_eq!(summaries(&body), ["A minor edit"]);
    let hidden: Value = json(&get(&w, &api("changes?minor=hide")).await);
    assert_eq!(summaries(&hidden).len(), 3);
    assert!(!summaries(&hidden).contains(&"A minor edit".to_owned()));
    let machine: Value = json(&get(&w, &api("changes?automated=only")).await);
    assert_eq!(summaries(&machine).len(), 2);

    let every: Value = json(&get(&w, &api("changes")).await);
    let item = |name: &str| {
        at(&every, "/items")
            .as_array()
            .unwrap()
            .iter()
            .find(|i| at(i, "/summary") == name)
            .unwrap()
            .clone()
    };
    assert_eq!(at(&item("A minor edit"), "/minor"), true);
    assert_eq!(at(&item("A minor edit"), "/machine_assisted"), false);
    assert_eq!(at(&item("A machine edit"), "/machine_assisted"), true);
    assert_eq!(at(&item("A machine edit"), "/automated"), true);
    assert_eq!(at(&item("A plain edit"), "/minor"), false);
    assert_eq!(at(&item("A plain edit"), "/automated"), false);
    assert_eq!(at(&item("Import the handbook"), "/machine_assisted"), false);
    assert_eq!(at(&item("Import the handbook"), "/automated"), true);

    // Anything but show, hide or only is refused by name.
    let bad = get(&w, &api("changes?minor=maybe")).await;
    assert_eq!(bad.status, 400);
    assert_eq!(at(&json(&bad), "/parameter"), "minor");
    // The description names the parameter and the members.
    let doc = get(&w, "/-/api/v1/openapi.json").await.body();
    assert!(
        doc.contains("\"minor\"") && doc.contains("machine_assisted"),
        "{doc}"
    );
}

fn json(a: &common::Answer) -> Value {
    serde_json::from_slice(&a.bytes).unwrap()
}
