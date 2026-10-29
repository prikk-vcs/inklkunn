#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_prikk::PatchMessage;

use super::*;

fn block(messages: &[&str]) -> HistoryBlock {
    HistoryBlock {
        block_id: "b".repeat(64),
        ref_state_id: "s".repeat(64),
        update_seq: 3,
        kind: "Normal".to_owned(),
        rollback_block: false,
        patch_count: messages.len() as u64,
        patch_messages: messages
            .iter()
            .enumerate()
            .map(|(i, m)| PatchMessage {
                patch_id: format!("{i:064x}"),
                message: (*m).to_owned(),
            })
            .collect(),
        previous_ref_state_id: None,
    }
}

#[test]
fn a_message_the_service_wrote_gives_its_kind_summary_and_people() {
    let text = "Add the glossary\n\nProposal: 019a0000-0000-7000-8000-00000000aaaa\n\
                Proposed-By: 019a0000-0000-7000-8000-0000000000a1 (Aiko Tanaka)\n\
                Accepted-By: 019a0000-0000-7000-8000-0000000000a2 (Ben Okoye)\n\
                Base: bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n\
                Kind: proposal\n";
    let d = describe(&block(&[text]));
    assert_eq!(d.kind, ChangeKind::Proposal);
    assert_eq!(d.summary, "Add the glossary");
    assert_eq!(d.proposed_by, Some(Person::Account("Aiko Tanaka".into())));
    assert_eq!(d.accepted_by, Some(Person::Account("Ben Okoye".into())));
    assert!(!d.kind.is_automated());
}

#[test]
fn a_message_someone_else_sealed_is_listed_by_its_first_line_with_no_people() {
    let d = describe(&block(&[
        "Edit Home; add the Glossary\n\nSigned-off-by: someone\n",
    ]));
    assert_eq!(d.kind, ChangeKind::Foreign);
    assert_eq!(d.summary, "Edit Home; add the Glossary");
    assert_eq!(d.proposed_by, None);
    assert_eq!(d.accepted_by, None);
    // A long line is cut, with a mark that it was.
    let long = "x".repeat(300);
    let d = describe(&block(&[&long]));
    assert_eq!(d.summary.chars().count(), FOREIGN_SUMMARY_CHARS + 1);
    assert!(d.summary.ends_with('…'));
    // Control characters do not reach a page.
    let d = describe(&block(&["Tab\there\u{7}bell"]));
    assert!(!d.summary.chars().any(char::is_control));
    // No message at all.
    assert_eq!(describe(&block(&[])).summary, "");
}

#[test]
fn an_import_is_automated() {
    let text = "Import the handbook\n\nProposed-By: system (inklkunn)\nAccepted-By: system (inklkunn)\n\
                Kind: import\nImported-From: bundle:sha256:abcd\n";
    let d = describe(&block(&[text]));
    assert_eq!(d.kind, ChangeKind::Import);
    assert!(d.kind.is_automated());
    assert_eq!(d.accepted_by, Some(Person::Service("inklkunn".into())));
}

#[test]
fn a_namespace_holds_its_hierarchy_and_not_a_title_that_only_starts_alike() {
    assert!(in_namespace("Guides/Setup", "Guides"));
    assert!(in_namespace("guides/Setup/Deep", "Guides/"));
    assert!(in_namespace("Anything", ""));
    assert!(!in_namespace("Guides", "Guides"));
    assert!(!in_namespace("GuidesExtra/Setup", "Guides"));
    assert!(!in_namespace("Other/Guides/Setup", "Guides"));
}

#[test]
fn only_pages_are_pages() {
    assert!(is_page_path("pages/Home.md"));
    assert!(is_page_path("pages/a/b.md"));
    assert!(!is_page_path("pages/Home.txt"));
    assert!(!is_page_path("attachments/pic.png"));
    assert!(!is_page_path("pagesHome.md"));
    assert!(!is_page_path("wiki-bundle.yaml"));
}

#[test]
fn the_filters_say_whether_they_narrow_the_list() {
    assert!(!ChangeFilter::default().is_active());
    for f in [
        ChangeFilter {
            view: Some("main".into()),
            ..ChangeFilter::default()
        },
        ChangeFilter {
            namespace: Some("A".into()),
            ..ChangeFilter::default()
        },
        ChangeFilter {
            page: Some("A".into()),
            ..ChangeFilter::default()
        },
        ChangeFilter {
            automated: Tri::Hide,
            ..ChangeFilter::default()
        },
        ChangeFilter {
            minor: Tri::Only,
            ..ChangeFilter::default()
        },
    ] {
        assert!(f.is_active(), "{f:?}");
    }
    assert!(Tri::Any.admits(true) && Tri::Any.admits(false));
    assert!(Tri::Only.admits(true) && !Tri::Only.admits(false));
    assert!(!Tri::Hide.admits(true) && Tri::Hide.admits(false));
}
