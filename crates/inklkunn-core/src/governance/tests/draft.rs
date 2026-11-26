//! The drafts the policy form sends: what they write is read by the one validator, and a typed word or text never becomes
//! syntax.

use super::*;
use crate::governance::draft::{ConstitutionDraft, PolicyDraft, ProtectDraft, fields};

const RICH: &str = "entry: questioner\nrungs:\n  comment: observer\n  propose: questioner\n  review: contributor\n  create_view: reviewer\n  accept: reviewer\n  moderate: reviewer\n  decide_appeal: maintainer\n  grant_up_to: contributor\nacceptance:\n  approvals: 2\n  self: never\nknowledge:\n  review_after: 400\npromotion:\n  contributor: \"A few accepted edits, then a word from a reviewer.\"\nprotect:\n  - namespace: \"Help\"\n    require: review\n    reason: \"Read before it changes.\"\n  - path: \"Home\"\n    require:\n      rung: maintainer\n    reason: \"The front page.\"\n";

#[test]
fn a_policy_made_a_draft_and_read_again_is_the_policy() {
    for text in ["", RICH] {
        let policy = policy(text);
        let draft = PolicyDraft::of(&policy);
        let read = validate_policy(&draft.to_text(), &PlatformLimits::default())
            .expect("a draft of a policy is a policy");
        assert_eq!(read, policy);
        // The canonical text is what the proposal carries, and it reads back the same.
        assert_eq!(policy.to_yaml(), read.to_yaml());
    }
}

#[test]
fn a_constitution_made_a_draft_and_read_again_is_the_constitution() {
    for text in [
        "",
        "policy_change:\n  rung: admin\n  approvals: 3\nconstitution_change:\n  approvals: 5\n",
    ] {
        let c = validate_constitution(text).expect("a constitution");
        let draft = ConstitutionDraft::of(&c);
        assert_eq!(validate_constitution(&draft.to_text()).expect("read"), c);
    }
}

#[test]
fn a_word_that_is_not_one_of_the_words_is_refused_at_its_key_and_never_read_as_syntax() {
    let base = PolicyDraft::of(&Policy::default());
    // A value that would end its line and open a key of its own, if it were written as it was typed.
    let mut draft = base.clone();
    draft.accept = "maintainer\nentry: contributor".to_owned();
    let fault =
        validate_policy(&draft.to_text(), &PlatformLimits::default()).expect_err("not a rung");
    assert_eq!(fault.refusal, Refusal::UnknownValue);
    assert_eq!(fault.field, fields::ACCEPT);
    assert!(fault.line.is_some(), "the line of the key is told");

    // A word with a comment mark, a quote or a colon in it is a word that is not one of the words.
    for hostile in [
        "maintainer # x",
        "\"maintainer",
        "maintainer: admin",
        "{accept: admin}",
    ] {
        let mut draft = base.clone();
        draft.entry = hostile.to_owned();
        let fault =
            validate_policy(&draft.to_text(), &PlatformLimits::default()).expect_err("not entry");
        assert_eq!(
            (fault.refusal, fault.field),
            (Refusal::UnknownValue, fields::ENTRY)
        );
    }
}

#[test]
fn a_text_with_a_control_character_or_a_line_end_is_refused_at_its_key() {
    let mut draft = PolicyDraft::of(&Policy::default());
    draft.promotion = vec![(Role::Reviewer, "one line\nand another".to_owned())];
    let fault = validate_policy(&draft.to_text(), &PlatformLimits::default()).expect_err("lines");
    assert_eq!(fault.refusal, Refusal::ForbiddenCharacter);
    assert_eq!(fault.field, fields::promotion(Role::Reviewer));

    let mut draft = PolicyDraft::of(&Policy::default());
    draft.protect = vec![ProtectDraft {
        namespace: false,
        title: "Home".to_owned(),
        require: "review".to_owned(),
        reason: "a\u{202e}reversed".to_owned(),
    }];
    let fault = validate_policy(&draft.to_text(), &PlatformLimits::default()).expect_err("bidi");
    assert_eq!(
        (fault.refusal, fault.field, fault.item),
        (Refusal::ForbiddenCharacter, fields::PROTECT_REASON, Some(1))
    );
}

#[test]
fn an_empty_text_of_a_promotion_is_no_text_and_an_empty_reason_is_missing() {
    let mut draft = PolicyDraft::of(&Policy::default());
    draft.promotion = vec![(Role::Reviewer, "   ".to_owned())];
    let policy = validate_policy(&draft.to_text(), &PlatformLimits::default()).expect("no text");
    assert_eq!(policy.promotion(Role::Reviewer), None);

    draft.protect = vec![ProtectDraft {
        namespace: true,
        title: "Help".to_owned(),
        require: "review".to_owned(),
        reason: " ".to_owned(),
    }];
    let fault = validate_policy(&draft.to_text(), &PlatformLimits::default()).expect_err("reason");
    assert_eq!(
        (fault.refusal, fault.field),
        (Refusal::Missing, fields::PROTECT_REASON)
    );
}

#[test]
fn the_ordering_rule_and_the_limits_are_the_validators() {
    let mut draft = PolicyDraft::of(&Policy::default());
    draft.propose = "maintainer".to_owned();
    draft.review = "reviewer".to_owned();
    let fault = validate_policy(&draft.to_text(), &PlatformLimits::default()).expect_err("order");
    assert_eq!(
        fault.refusal,
        Refusal::Ordering(OrderingPair::ProposeAboveReview)
    );
    let mut draft = PolicyDraft::of(&Policy::default());
    draft.entry = "contributor".to_owned();
    let limits = PlatformLimits {
        entry_max: Entry::Observer,
        ..PlatformLimits::default()
    };
    let fault = validate_policy(&draft.to_text(), &limits).expect_err("limit");
    assert_eq!(fault.refusal, Refusal::PlatformLimit(Limit::EntryMax));
    let mut draft = PolicyDraft::of(&Policy::default());
    draft.approvals = 6;
    let fault = validate_policy(&draft.to_text(), &PlatformLimits::default()).expect_err("range");
    assert_eq!(
        (fault.refusal, fault.field),
        (Refusal::OutOfRange, fields::APPROVALS)
    );
}

#[test]
fn every_field_a_fault_can_name_is_in_the_list_of_fields() {
    // The faults the schema's own reader gives, for the keys a form has: each field it names is one `fields` lists.
    let known: Vec<&str> = fields::POLICY
        .iter()
        .chain(fields::CONSTITUTION.iter())
        .copied()
        .chain(Role::ALL.iter().map(|r| fields::promotion(*r)))
        .collect();
    for (text, field) in [
        ("entry: x\n", fields::ENTRY),
        ("rungs:\n  comment: x\n", fields::COMMENT),
        ("rungs:\n  decide_appeal: x\n", fields::DECIDE_APPEAL),
        ("rungs:\n  grant_up_to: x\n", fields::GRANT_UP_TO),
        ("acceptance:\n  self: x\n", fields::SELF_ACCEPTANCE),
        ("knowledge:\n  review_after: 0\n", fields::REVIEW_AFTER),
        (
            "protect:\n  - path: A\n    require: x\n    reason: r\n",
            fields::PROTECT_REQUIRE,
        ),
    ] {
        let fault = policy_fault(text);
        assert_eq!(fault.field, field, "{text}");
        assert!(known.contains(&fault.field), "{field}");
    }
}

#[test]
fn an_interwiki_map_made_a_draft_and_read_again_is_the_map() {
    use crate::governance::draft::InterwikiDraft;
    let none = std::collections::BTreeSet::new();
    for text in [
        "",
        "wikipedia: https://en.wikipedia.org/wiki/{title}\ndocs: \"https://example.org/docs?page={title}&x=1#top\"\n",
    ] {
        let map = validate_interwiki(text, &none).expect("a map");
        let draft = InterwikiDraft::of(&map);
        let read = validate_interwiki(&draft.to_text(), &none).expect("a draft of a map is a map");
        assert_eq!(read, map);
        // The canonical text is what the proposal carries, and it reads back the same.
        assert_eq!(map.to_yaml(), read.to_yaml());
    }
}

#[test]
fn a_typed_prefix_or_template_never_becomes_syntax() {
    use crate::governance::draft::InterwikiDraft;
    let none = std::collections::BTreeSet::new();
    let row = |prefix: &str, template: &str| InterwikiDraft {
        rows: vec![(prefix.to_owned(), template.to_owned())],
    };
    // A prefix that tries to end its key and write another, or a comment, is one key the schema does not have.
    for prefix in [
        "a\": \"https://x.example/{title}\"\nb",
        "a: b",
        "a # b",
        "a\nb",
        "",
    ] {
        let fault = validate_interwiki(&row(prefix, "https://x.example/{title}").to_text(), &none)
            .expect_err("a prefix that is not one");
        assert_eq!(
            (fault.refusal, fault.field),
            (Refusal::UnknownKey, fields::INTERWIKI_PREFIX),
            "{prefix:?}"
        );
    }
    // A template that tries to end its value and write a second key is a template that breaks a rule, at the template.
    let fault = validate_interwiki(
        &row("a", "https://x.example/{title}\"\nb: \"https://y.example/{title}").to_text(),
        &none,
    )
    .expect_err("a template that is not one");
    assert_eq!(fault.field, fields::INTERWIKI_TEMPLATE);
    // Written twice, a prefix is one the validator refuses by name.
    let twice = InterwikiDraft {
        rows: vec![
            ("a".to_owned(), "https://x.example/{title}".to_owned()),
            ("a".to_owned(), "https://y.example/{title}".to_owned()),
        ],
    };
    let fault = validate_interwiki(&twice.to_text(), &none).expect_err("a repeated key");
    assert_eq!(fault.refusal, Refusal::RepeatedKey);
}

#[test]
fn the_fields_of_the_interwiki_map_are_the_ones_its_faults_name() {
    let none = std::collections::BTreeSet::new();
    let prefix =
        validate_interwiki("\"a b\": \"https://x.example/{title}\"\n", &none).expect_err("a prefix");
    let template =
        validate_interwiki("a: \"http://x.example/{title}\"\n", &none).expect_err("a template");
    assert_eq!(prefix.field, fields::INTERWIKI_PREFIX);
    assert_eq!(template.field, fields::INTERWIKI_TEMPLATE);
    assert!(
        fields::INTERWIKI.contains(&prefix.field) && fields::INTERWIKI.contains(&template.field)
    );
}
