#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_core::knowledge::Rule;
use inklkunn_core::knowledge::check::{Refusal, UNREADABLE};
use inklkunn_core::proposal::Invalid;

use super::*;
use crate::catalog::{EN, JA_DEF, Lang};

fn refusal(removes: bool, field: &'static str) -> Refusal {
    Refusal {
        file: "knowledge/claims/x.yaml".to_owned(),
        rule: Rule::Status,
        field,
        line: None,
        removes,
        carries: false,
        bound: None,
    }
}

/// Per language: what a curation proposal makes, what is said of a removal (not offered; retire it, RFC 006 D-10), and what is said of
/// a file that cannot be read (who replaces it).
fn both_languages() -> [(T, &'static str, &'static str, &'static str); 2] {
    [
        (
            T::new(EN),
            "which only a curation proposal may do",
            "Removing a claim or a source file is not offered",
            "someone who may moderate this wiki",
        ),
        (
            T::new(Lang::from_def(&JA_DEF)),
            "これができるのはキュレーション提案だけです",
            "削除する操作は提供していません",
            "モデレーションを行える役割を持つ人",
        ),
    ]
}

#[test]
fn a_change_curation_makes_is_told_to_be_curations_and_a_removal_or_an_unreadable_file_to_be_not_offered_each_once()
 {
    for (t, curation, not_offered, by_moderate) in both_languages() {
        // What a curation proposal makes: its sentence, and nothing of what is not offered.
        let made = sentences(t, &[refusal(false, "community_status")]);
        assert_eq!(made.len(), 1);
        assert!(made.iter().all(|s| s.contains(curation)), "{made:?}");
        assert!(!made.iter().any(|s| s.contains(not_offered)), "{made:?}");
        // A removal is not offered, and an unreadable file written over is the moderating rung's: each said once, and never also
        // *only a curation proposal may*.
        for (refused, said_by) in [
            (vec![refusal(true, "community_status")], vec![not_offered]),
            (vec![refusal(false, UNREADABLE)], vec![by_moderate]),
            (
                vec![refusal(true, "sources"), refusal(false, UNREADABLE)],
                vec![not_offered, by_moderate],
            ),
        ] {
            let said = sentences(t, &refused);
            assert_eq!(said.len(), said_by.len(), "{said:?}");
            for (sentence, part) in said.iter().zip(&said_by) {
                assert!(sentence.contains(part), "{said:?}");
            }
            assert!(!said.iter().any(|s| s.contains(curation)), "{said:?}");
        }
        // Both kinds in one change: each sentence once, and the first has nothing of removals in it.
        let mixed = sentences(
            t,
            &[refusal(false, "community_status"), refusal(true, "sources")],
        );
        assert_eq!(mixed.len(), 2, "{mixed:?}");
        assert!(mixed.first().is_some_and(|s| s.contains(curation)));
        assert!(mixed.get(1).is_some_and(|s| s.contains(not_offered)));
        assert_eq!(mixed.iter().filter(|s| s.contains(curation)).count(), 1);
        assert_eq!(mixed.iter().filter(|s| s.contains(not_offered)).count(), 1);
        assert!(!mixed.iter().any(|s| s.contains(by_moderate)), "{mixed:?}");
        // No refusal known: the sentence of the rule, as the service words it.
        assert_eq!(sentences(t, &[]), [t.get(Key::KnowNotCuration)]);
    }
}

#[test]
fn the_explanation_the_api_gives_for_a_curated_change_says_nothing_of_removals() {
    // The write API's `knowledge-curated` explanation is the service's own words for the refusal: the rule's sentence,
    // without the removal and unreadable-file clauses (K67); the API makes no removal of a claim or source file.
    for (t, curation, not_offered, _) in both_languages() {
        let explanation = crate::proposals::words::invalid_text(t, Invalid::KnowledgeCurated);
        assert!(explanation.contains(curation), "{explanation}");
        assert!(!explanation.contains(not_offered), "{explanation}");
        for removal in [
            "removes a claim",
            "cannot be read",
            "主張や出典を削除",
            "読み取れないファイル",
        ] {
            assert!(!explanation.contains(removal), "{removal}: {explanation}");
        }
    }
}
