//! The sentences of the rules: each key has one, in both languages, at default and at a non-default value; each refusal of the
//! validator has one for the form; and no sentence is raw YAML or says *official* (RFC 006 acceptance criterion 15).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeSet;

use inklkunn_core::governance::draft::{ConstitutionDraft, PolicyDraft, ProtectDraft};
use inklkunn_core::governance::{
    Constitution, PlatformLimits, Policy, Refusal, validate_constitution, validate_policy,
};
use inklkunn_markup::Value;
use inklkunn_markup::frontmatter::parse_yaml;
use inklkunn_store::Role;

use super::rules_change::{Compared, policy_lines, policy_side};
use super::rules_form::refusal_text;
use super::rules_phrases::{COVERED, constitution_facts, policy_facts};
use crate::catalog::{EN, JA_DEF, Lang, T};

fn both() -> [T; 2] {
    [T::new(EN), T::new(Lang::from_def(&JA_DEF))]
}

/// Every key of a mapping, as a path through the mappings (a list is a leaf).
fn paths(prefix: &str, value: &Value, out: &mut BTreeSet<String>) {
    if let Value::Map(pairs) = value {
        for (key, v) in pairs {
            let path = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            if matches!(v, Value::Map(_)) {
                paths(&path, v, out);
            } else {
                out.insert(path);
            }
        }
    }
}

/// A policy with every key set to something other than its default.
fn full_policy() -> Policy {
    let mut draft = PolicyDraft::of(&Policy::default());
    draft.entry = "observer".to_owned();
    draft.accept = "reviewer".to_owned();
    draft.moderate = "reviewer".to_owned();
    draft.decide_appeal = "maintainer".to_owned();
    draft.grant_up_to = "contributor".to_owned();
    draft.approvals = 2;
    draft.self_acceptance = "never".to_owned();
    draft.review_after = Some(400);
    draft.promotion = Role::ALL
        .iter()
        .map(|r| (*r, format!("Text for {}", r.as_str())))
        .collect();
    draft.protect = vec![
        ProtectDraft {
            namespace: true,
            title: "Help".to_owned(),
            require: "review".to_owned(),
            reason: "Read first.".to_owned(),
        },
        ProtectDraft {
            namespace: false,
            title: "Home".to_owned(),
            require: "maintainer".to_owned(),
            reason: "The front page.".to_owned(),
        },
    ];
    validate_policy(&draft.to_text(), &PlatformLimits::default()).expect("a policy")
}

fn full_constitution() -> Constitution {
    validate_constitution(
        &ConstitutionDraft {
            policy_rung: "admin".to_owned(),
            policy_approvals: 2,
            constitution_approvals: 1,
        }
        .to_text(),
    )
    .expect("a constitution")
}

#[test]
fn every_key_of_the_files_has_a_sentence_and_adding_a_key_without_one_fails_here() {
    // The keys the files' own writer says, for a policy and a constitution with every key set.
    let mut keys = BTreeSet::new();
    paths(
        "",
        &parse_yaml(&full_policy().to_yaml()).unwrap(),
        &mut keys,
    );
    paths(
        "",
        &parse_yaml(&full_constitution().to_yaml()).unwrap(),
        &mut keys,
    );
    // `protect` is a list, said by the sentences of a protection.
    let covered: BTreeSet<String> = COVERED.iter().map(|k| (*k).to_owned()).collect();
    assert_eq!(
        keys, covered,
        "a key the files have and no sentence says (or the reverse)"
    );
    // And every key is said by a fact: the facts' paths and `protect` are the same set.
    let t = T::new(EN);
    let mut said: BTreeSet<String> = BTreeSet::new();
    for fact in policy_facts(t, &full_policy(), 1096)
        .into_iter()
        .chain(constitution_facts(t, &full_constitution()))
    {
        said.extend(fact.paths.iter().map(|p| (*p).to_owned()));
    }
    said.insert("protect".to_owned());
    assert_eq!(said, covered);
}

#[test]
fn each_key_says_its_sentence_in_both_languages_at_the_default_and_at_another_value() {
    let default = Policy::default();
    let other = full_policy();
    for t in both() {
        let (d, o) = (
            policy_facts(t, &default, 1096),
            policy_facts(t, &other, 1096),
        );
        assert_eq!(d.len(), o.len());
        for (a, b) in d.iter().zip(&o) {
            assert_eq!(a.paths, b.paths);
            for text in [&a.label, &a.value, &b.label, &b.value] {
                assert!(!text.is_empty(), "{:?}", a.paths);
                assert!(!text.contains('[') && !text.contains('{'), "{text}");
            }
            assert_eq!(a.label, b.label, "{:?}", a.paths);
        }
        // Each key but the ones the full policy leaves at the default says something else at another value.
        let changed: Vec<&&[&str]> = d
            .iter()
            .zip(&o)
            .filter(|(a, b)| a.value != b.value)
            .map(|(a, _)| &a.paths)
            .collect();
        // accept, moderate, decide_appeal, grant, entry, approvals, self, review and six promotions.
        assert_eq!(changed.len(), 14, "{changed:?}");
        for c in [Constitution::default(), full_constitution()] {
            for fact in constitution_facts(t, &c) {
                assert!(!fact.label.is_empty() && !fact.value.is_empty());
            }
        }
    }
}

#[test]
fn a_change_is_said_as_what_it_said_and_what_it_says() {
    let t = T::new(EN);
    let before = Policy::default().to_yaml();
    let after = {
        let mut d = PolicyDraft::of(&Policy::default());
        d.accept = "reviewer".to_owned();
        validate_policy(&d.to_text(), &PlatformLimits::default())
            .unwrap()
            .to_yaml()
    };
    let Compared::Lines(lines) = policy_lines(
        t,
        &policy_side(Some(&before)),
        &policy_side(Some(&after)),
        1096,
    ) else {
        panic!("both read");
    };
    assert_eq!(
        lines,
        ["Who may accept a proposal: maintainer or above → reviewer or above"]
    );
    // A file that is not there says the defaults; one that cannot be read has no sentences.
    let Compared::Lines(none) =
        policy_lines(t, &policy_side(None), &policy_side(Some(&before)), 1096)
    else {
        panic!("reads");
    };
    assert!(none.is_empty());
    assert!(matches!(
        policy_lines(
            t,
            &policy_side(None),
            &policy_side(Some("entry: nobody\n")),
            1096
        ),
        Compared::Unreadable(_)
    ));
}

#[test]
fn every_refusal_of_the_validator_has_a_sentence_for_the_form_in_both_languages() {
    let limits = PlatformLimits::default();
    for t in both() {
        let mut seen = BTreeSet::new();
        for refusal in Refusal::ALL {
            let text = refusal_text(t, refusal, &limits);
            assert!(!text.trim().is_empty(), "{}", refusal.name());
            assert!(
                !text.contains('[') && !text.contains('{'),
                "{}: {text}",
                refusal.name()
            );
            assert!(
                seen.insert(text.clone()),
                "{}: {text} is another's",
                refusal.name()
            );
        }
    }
}

#[test]
fn every_field_a_refusal_can_be_about_has_a_control_in_the_form() {
    use super::rules_form::control_of;
    use inklkunn_core::governance::draft::fields;
    use inklkunn_core::governance::{Fault, GovernanceFile};
    let promotions: Vec<&'static str> = Role::ALL.iter().map(|r| fields::promotion(*r)).collect();
    for field in fields::POLICY
        .iter()
        .chain(fields::CONSTITUTION.iter())
        .chain(promotions.iter())
    {
        let fault = Fault {
            item: Some(1),
            ..Fault::at(GovernanceFile::Policy, Refusal::UnknownValue, field)
        };
        assert!(
            control_of(&fault, &[1]).is_some(),
            "`{field}` has no control to be said beside"
        );
    }
    // What is about the whole file has none: it is said at the top.
    let whole = Fault::whole(GovernanceFile::Policy, Refusal::TooLong);
    assert_eq!(control_of(&whole, &[]), None);
}

#[test]
fn no_sentence_is_raw_yaml_or_says_official() {
    let tokens = [
        "rungs.",
        "acceptance.",
        "create_view",
        "decide_appeal",
        "grant_up_to",
        "review_after",
        "small-wiki",
        "policy_change",
        "constitution_change",
        "entry:",
        "rungs:",
    ];
    for t in both() {
        let mut texts = Vec::new();
        for fact in policy_facts(t, &full_policy(), 1096)
            .into_iter()
            .chain(constitution_facts(t, &full_constitution()))
        {
            texts.push(fact.label);
            texts.push(fact.value);
        }
        for refusal in Refusal::ALL {
            texts.push(refusal_text(t, refusal, &PlatformLimits::default()));
        }
        for text in texts {
            for token in tokens {
                assert!(!text.contains(token), "`{token}` in `{text}`");
            }
            assert!(!text.to_lowercase().contains("official"), "{text}");
            assert!(!text.contains("公式"), "{text}");
        }
    }
}
