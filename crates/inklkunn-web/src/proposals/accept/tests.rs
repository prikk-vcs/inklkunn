#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::Mutex;

use inklkunn_core::knowledge::check::Refusal;
use inklkunn_core::knowledge::{Fault, Rule};

use super::*;

#[test]
fn a_knowledge_file_refused_before_the_signer_is_a_page_of_its_own_and_the_log_names_the_file() {
    let logged = Mutex::new(Vec::new());
    let log = |line: &str| logged.lock().unwrap().push(line.to_owned());
    let refusal = Refusal::of("knowledge/stray.txt", Fault::whole(Rule::Path));
    let outcome = outcome_of(Err(AcceptFailure::KnowledgeInvalid(vec![refusal])), &log);
    assert!(matches!(
        outcome,
        AcceptOutcome::Unchanged(Unchanged::KnowledgeInvalid)
    ));
    let lines = logged.lock().unwrap();
    assert_eq!(lines.len(), 1);
    assert!(lines.first().unwrap().contains("knowledge/stray.txt"));
}
