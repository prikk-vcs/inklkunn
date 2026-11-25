#![allow(clippy::expect_used, clippy::unwrap_used)]

use axum::http::StatusCode;
use inklkunn_core::governance::{Fault, GovernanceFile};
use inklkunn_core::proposal::ProposalError;

use super::refusal;
use crate::catalog::{EN, JA_DEF, Key, Lang, T};

#[test]
fn a_governance_file_in_another_view_is_refused_as_a_page_in_both_languages_without_the_acceptances_note()
 {
    // C-3e at proposing: the sentence is the rule's, the one the acceptance says, and the acceptance's note (the proposal is
    // held for review, accept it again) is not said to someone whose proposal was never made.
    let error = ProposalError::GovernanceInvalid(vec![
        Fault::outside_default_view(Some(GovernanceFile::Policy)),
        Fault::outside_default_view(Some(GovernanceFile::Policy)),
        Fault::outside_default_view(None),
    ]);
    for t in [T::new(EN), T::new(Lang::from_def(&JA_DEF))] {
        let page = refusal(t, &error).expect("a page of its own");
        assert_eq!(page.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(page.heading, Key::RefusedInvalidHeading);
        assert!(!page.retry);
        let rule = t.get(Key::GovernanceRuleOutsideDefaultView);
        // The same file and rule said twice is said once; a path that is neither file is told by the rule alone.
        assert_eq!(page.paragraphs.len(), 2, "{:?}", page.paragraphs);
        assert!(
            page.paragraphs
                .iter()
                .all(|p| p.contains(&rule) && *p != t.get(Key::OutcomeNoteGovernanceInvalid)),
            "{:?}",
            page.paragraphs
        );
        assert!(
            page.paragraphs
                .first()
                .unwrap()
                .contains("governance/policy.yaml")
        );
    }
}
