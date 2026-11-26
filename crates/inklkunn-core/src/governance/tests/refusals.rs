//! RFC 006 acceptance criterion 3, the refusals of D-4 each by name: a wrong shape, an unknown key, the ordering rule
//! broken (each pair), a platform limit exceeded, a governance file in an alternative view; and the file-level faults,
//! which carry the file and, where it can be said, the line. A refusal never repeats what the file said.

use super::*;

const SECRET: &str = "ZZ-secret-ZZ";

#[test]
fn a_wrong_shape_is_refused_by_name() {
    for (text, field) in [
        // A list where a mapping goes, a mapping where a word goes, a word where a mapping goes.
        ("rungs: [comment]\n", "rungs"),
        ("entry:\n  a: b\n", "entry"),
        ("acceptance: never\n", "acceptance"),
        ("entry: [none]\n", "entry"),
        ("protect: everything\n", "protect"),
        ("protect:\n  - just a string\n", "protect"),
        ("knowledge: 30\n", "knowledge"),
        ("promotion: yes\n", "promotion"),
    ] {
        let f = policy_fault(text);
        assert_eq!(f.refusal, Refusal::WrongShape, "{text:?}");
        assert_eq!(f.field, field, "{text:?}");
        assert_eq!(f.file, Some(GovernanceFile::Policy));
        assert_eq!(f.name(), "wrong-shape");
    }
    // The top of a file is a mapping.
    for text in ["- a\n- b\n", "just words\n", "42\n", "\"x\"\n"] {
        let f = policy_fault(text);
        assert_eq!(f.refusal, Refusal::WrongShape, "{text:?}");
        assert_eq!(f.field, "");
    }
    for text in ["- a\n", "policy_change: 3\n", "constitution_change: [1]\n"] {
        assert_eq!(constitution_fault(text).refusal, Refusal::WrongShape);
    }
}

#[test]
fn text_outside_the_subset_is_a_wrong_shape_with_its_line() {
    for (text, line) in [
        ("entry: &a none\n", 1),
        ("# c\n\nentry: *a\n", 3),
        ("entry: !!str none\n", 1),
        (
            "entry: none\nrungs:\n   comment: observer\n  accept: admin\n",
            4,
        ),
        ("entry: \"none\n", 1),
    ] {
        let f = policy_fault(text);
        assert_eq!(f.refusal, Refusal::WrongShape, "{text:?}");
        assert_eq!(f.line, Some(line), "{text:?}");
    }
    // Nesting beyond the parser's depth.
    let deep = format!("entry: {}none{}\n", "[".repeat(40), "]".repeat(40));
    assert_eq!(policy_fault(&deep).refusal, Refusal::WrongShape);
}

#[test]
fn an_unknown_key_is_refused_by_name_at_every_level() {
    for (text, field, line) in [
        (format!("{SECRET}: 1\n"), "", 1),
        (format!("entry: none\n{SECRET}: 1\n"), "", 2),
        (format!("rungs:\n  {SECRET}: admin\n"), "rungs", 2),
        ("rungs:\n  accept: admin\n  grant: none\n".to_owned(), "rungs", 3),
        (format!("acceptance:\n  {SECRET}: 1\n"), "acceptance", 2),
        (format!("knowledge:\n  {SECRET}: 1\n"), "knowledge", 2),
        (format!("promotion:\n  {SECRET}: x\n"), "promotion", 2),
        (
            format!("protect:\n  - path: Home\n    require: review\n    reason: x\n    {SECRET}: 1\n"),
            "protect",
            5,
        ),
        (
            "protect:\n  - path: Home\n    reason: x\n    require:\n      rung: admin\n      how: 1\n"
                .to_owned(),
            "protect.require",
            6,
        ),
        // The words of 0.3.0's roles table that RFC 006 does not have.
        ("rungs:\n  admin: admin\n".to_owned(), "rungs", 2),
        ("rungs:\n  export: admin\n".to_owned(), "rungs", 2),
        ("rungs:\n  read: observer\n".to_owned(), "rungs", 2),
        ("version: 1\n".to_owned(), "", 1),
    ] {
        let f = policy_fault(&text);
        assert_eq!(f.refusal, Refusal::UnknownKey, "{text:?}");
        assert_eq!(f.field, field, "{text:?}");
        assert_eq!(f.line, Some(line), "{text:?}");
        assert_eq!(f.name(), "unknown-key");
        assert!(!f.to_string().contains(SECRET), "never repeats the file: {f}");
    }
    for (text, field) in [
        (format!("{SECRET}: 1\n"), ""),
        (format!("policy_change:\n  {SECRET}: 1\n"), "policy_change"),
        (
            format!("constitution_change:\n  {SECRET}: 1\n"),
            "constitution_change",
        ),
    ] {
        let f = constitution_fault(&text);
        assert_eq!(
            (f.refusal, f.field),
            (Refusal::UnknownKey, field),
            "{text:?}"
        );
        assert_eq!(f.file, Some(GovernanceFile::Constitution));
        assert!(!f.to_string().contains(SECRET));
    }
}

#[test]
fn a_repeated_key_is_refused_in_a_block_and_in_a_flow_mapping() {
    for text in [
        "entry: none\nentry: observer\n",
        "rungs:\n  accept: admin\n  accept: maintainer\n",
        "rungs: {accept: admin, accept: maintainer}\n",
        "acceptance: {approvals: 1, approvals: 2}\n",
        "entry: none\nrungs: {}\nentry: none\n",
    ] {
        let f = policy_fault(text);
        assert_eq!(f.refusal, Refusal::RepeatedKey, "{text:?}");
        assert_eq!(f.name(), "repeated-key");
        assert!(f.line.is_some(), "{text:?}");
    }
    assert_eq!(
        constitution_fault("policy_change:\n  rung: admin\n  rung: admin\n").refusal,
        Refusal::RepeatedKey
    );
    // A key repeated across promotion rungs.
    assert_eq!(
        policy_fault("promotion:\n  admin: a\n  admin: b\n").refusal,
        Refusal::RepeatedKey
    );
}

#[test]
fn a_word_outside_its_list_is_an_unknown_value() {
    for (text, field) in [
        ("entry: maintainer\n", "entry"),
        ("entry: everyone\n", "entry"),
        ("entry: None\n", "entry"),
        ("rungs:\n  accept: owner\n", "rungs.accept"),
        ("rungs:\n  comment: Questioner\n", "rungs.comment"),
        ("rungs:\n  moderate: \"\"\n", "rungs.moderate"),
    ] {
        let f = policy_fault(text);
        // An empty value (`moderate: ""`) is a word that is not on the list.
        assert!(
            matches!(f.refusal, Refusal::UnknownValue),
            "{text:?}: {:?}",
            f.refusal
        );
        assert_eq!(f.field, field);
        assert_eq!(f.name(), "unknown-value");
    }
}

#[test]
fn the_ordering_rule_is_enforced_for_each_pair_by_name() {
    // propose <= review
    for (propose, review) in [("reviewer", "contributor"), ("maintainer", "reviewer")] {
        let f = policy_fault(&format!(
            "rungs:\n  propose: {propose}\n  review: {review}\n  accept: admin\n"
        ));
        assert_eq!(
            f.refusal,
            Refusal::Ordering(OrderingPair::ProposeAboveReview)
        );
        assert_eq!(f.name(), "ordering-propose-above-review");
        assert_eq!(f.field, "rungs.propose");
        assert_eq!(f.line, Some(2));
    }
    // review <= accept
    for (review, accept) in [("maintainer", "reviewer"), ("admin", "maintainer")] {
        let f = policy_fault(&format!("rungs:\n  review: {review}\n  accept: {accept}\n"));
        assert_eq!(
            f.refusal,
            Refusal::Ordering(OrderingPair::ReviewAboveAccept)
        );
        assert_eq!(f.name(), "ordering-review-above-accept");
        assert_eq!(f.field, "rungs.review");
        assert_eq!(f.line, Some(2));
    }
    // moderate >= reviewer
    for moderate in ["observer", "questioner", "contributor"] {
        let f = policy_fault(&format!("rungs:\n  moderate: {moderate}\n"));
        assert_eq!(
            f.refusal,
            Refusal::Ordering(OrderingPair::ModerateBelowReviewer)
        );
        assert_eq!(f.name(), "ordering-moderate-below-reviewer");
        assert_eq!(f.field, "rungs.moderate");
        assert_eq!(f.line, Some(2));
    }
    // decide_appeal >= moderate (A-4): the one who decides appeals can do what an overturn does, which is undo a moderator's act.
    // `moderate: admin` beside `decide_appeal: maintainer` would let a maintainer undo an admin's hiding or restriction.
    let f = policy_fault("rungs:\n  moderate: admin\n  decide_appeal: maintainer\n");
    assert_eq!(
        f.refusal,
        Refusal::Ordering(OrderingPair::DecideAppealBelowModerate)
    );
    assert_eq!(f.name(), "ordering-decide-appeal-below-moderate");
    assert_eq!(f.field, "rungs.decide_appeal");
    assert_eq!(f.line, Some(3));
    // Judged over the keys the file leaves out too: the default `decide_appeal` is admin, and a lower one beside the default
    // `moderate` (maintainer) is the edge that is allowed.
    let f =
        policy_fault("rungs:\n  moderate: admin\n  decide_appeal: maintainer\n  accept: admin\n");
    assert_eq!(
        f.refusal,
        Refusal::Ordering(OrderingPair::DecideAppealBelowModerate)
    );
    // The edges are allowed: equal rungs, and the defaults.
    for text in [
        "rungs:\n  moderate: maintainer\n  decide_appeal: maintainer\n",
        "rungs:\n  moderate: admin\n  decide_appeal: admin\n",
        "rungs:\n  moderate: reviewer\n  decide_appeal: maintainer\n",
        "rungs:\n  moderate: admin\n",
        "",
    ] {
        assert!(
            validate_policy(text, &PlatformLimits::default()).is_ok(),
            "{text:?}"
        );
    }
    // The first pair broken is the one named: `moderate` below reviewer comes before it.
    let f = policy_fault("rungs:\n  moderate: observer\n  decide_appeal: maintainer\n");
    assert_eq!(
        f.refusal,
        Refusal::Ordering(OrderingPair::ModerateBelowReviewer)
    );
    // The rule is judged over the keys the file leaves out too: `propose: maintainer` alone is above the default
    // `review`.
    let f = policy_fault("rungs:\n  propose: maintainer\n");
    assert_eq!(
        f.refusal,
        Refusal::Ordering(OrderingPair::ProposeAboveReview)
    );
    // The edges are allowed: equal rungs, and a moderator at exactly reviewer.
    assert!(
        validate_policy(
            "rungs:\n  propose: admin\n  review: admin\n  accept: admin\n  moderate: reviewer\n",
            &PlatformLimits::default()
        )
        .is_ok()
    );
    assert!(
        validate_policy(
            "rungs:\n  propose: observer\n  review: observer\n  accept: observer\n  moderate: reviewer\n",
            &PlatformLimits::default()
        )
        .is_ok()
    );
    // The pairs that are not ordered are free: comment and create_view move anywhere.
    for rung in Role::ALL {
        let text = format!(
            "rungs:\n  comment: {r}\n  create_view: {r}\n",
            r = rung.as_str()
        );
        assert!(validate_policy(&text, &PlatformLimits::default()).is_ok());
    }
}

#[test]
fn a_platform_limit_exceeded_is_refused_naming_the_limit() {
    let limits = PlatformLimits {
        entry_max: Entry::Questioner,
        limit_days: 90,
    };
    assert!(validate_policy("entry: questioner\n", &limits).is_ok());
    assert!(validate_policy("entry: observer\n", &limits).is_ok());
    assert!(validate_policy("", &limits).is_ok());
    let f = validate_policy("entry: contributor\n", &limits).unwrap_err();
    assert_eq!(f.refusal, Refusal::PlatformLimit(Limit::EntryMax));
    assert_eq!(f.name(), "platform-limit-entry-max");
    assert_eq!(f.field, "entry");
    assert_eq!(f.line, Some(1));
    assert!(
        f.to_string().contains("INKLKUNN_GOVERNANCE_ENTRY_MAX"),
        "{f}"
    );
    assert_eq!(Limit::EntryMax.variable(), "INKLKUNN_GOVERNANCE_ENTRY_MAX");
    assert_eq!(
        Limit::LimitDays.variable(),
        "INKLKUNN_GOVERNANCE_LIMIT_DAYS"
    );
    // The operator's lowest limit allows only `none`.
    let none = PlatformLimits {
        entry_max: Entry::None,
        limit_days: 1,
    };
    assert!(validate_policy("entry: none\n", &none).is_ok());
    let f = validate_policy("# the rules\nrungs: {}\nentry: observer\n", &none).unwrap_err();
    assert_eq!(
        (f.refusal, f.line),
        (Refusal::PlatformLimit(Limit::EntryMax), Some(3))
    );
    // The schema is judged before the limits: a wrong shape is a wrong shape.
    let f = validate_policy("entry: [contributor]\n", &limits).unwrap_err();
    assert_eq!(f.refusal, Refusal::WrongShape);
}

#[test]
fn the_days_limit_is_a_hook_for_the_periods_that_come() {
    let limits = PlatformLimits::default();
    assert!(limits.allows_days(0));
    assert!(limits.allows_days(90));
    assert!(!limits.allows_days(91));
    assert_eq!(
        Refusal::PlatformLimit(Limit::LimitDays).name(),
        "platform-limit-limit-days"
    );
}

#[test]
fn a_governance_file_in_an_alternative_view_is_refused() {
    for touched in [
        vec![POLICY_PATH],
        vec![CONSTITUTION_PATH],
        vec!["pages/A.md", POLICY_PATH],
        vec!["governance/other.yaml"],
        vec!["governance"],
        vec!["./governance/policy.yaml"],
        vec!["governance//policy.yaml"],
        vec!["Governance/policy.yaml"],
    ] {
        let f = refuse_outside_default_view(touched.iter().copied(), ViewPlace::Alternative)
            .expect_err(&format!("{touched:?}"));
        assert_eq!(f.refusal, Refusal::OutsideDefaultView);
        assert_eq!(f.name(), "outside-default-view");
        // The same proposal in the default view is not refused here.
        assert!(refuse_outside_default_view(touched.iter().copied(), ViewPlace::Default).is_ok());
    }
    // It names the file when it is one of the two, and says `governance/` otherwise.
    let policy = refuse_outside_default_view([POLICY_PATH], ViewPlace::Alternative).unwrap_err();
    assert_eq!(policy.file, Some(GovernanceFile::Policy));
    assert!(
        policy.to_string().starts_with("governance/policy.yaml: "),
        "{policy}"
    );
    let other =
        refuse_outside_default_view(["governance/x.md"], ViewPlace::Alternative).unwrap_err();
    assert_eq!(other.file, None);
    assert!(other.to_string().starts_with("governance/: "), "{other}");
    assert!(other.to_string().contains("default view"));
    // Nothing else is refused: pages, knowledge, a page that merely starts with the word.
    for touched in [
        vec!["pages/A.md"],
        vec!["knowledge/claims/x.yaml"],
        vec!["pages/governance/policy.yaml"],
        vec!["governance-notes.md"],
        vec!["governances/x"],
        vec![],
    ] {
        assert!(
            refuse_outside_default_view(touched.iter().copied(), ViewPlace::Alternative).is_ok(),
            "{touched:?}"
        );
    }
    assert!(is_governance_path("governance/policy.yaml"));
    assert!(!is_governance_path("pages/governance/x.md"));
}

#[test]
fn a_fault_names_the_file_the_field_and_the_line_and_never_the_content() {
    let f = policy_fault(&format!("entry: none\nrungs:\n  accept: {SECRET}\n"));
    assert_eq!(f.file, Some(GovernanceFile::Policy));
    assert_eq!(f.path(), "governance/policy.yaml");
    assert_eq!(f.line, Some(3));
    let shown = f.to_string();
    assert_eq!(
        shown,
        "governance/policy.yaml, line 3: a value is not one of the words its key allows (`rungs.accept`)"
    );
    assert!(!shown.contains(SECRET));
    let f = constitution_fault("policy_change:\n  rung: nobody\n");
    assert_eq!(f.path(), "governance/constitution.yaml");
    assert_eq!(f.line, Some(2));
    // A protection names its entry.
    let f = policy_fault(
        "protect:\n  - path: A\n    require: review\n    reason: x\n  - path: B\n    require: review\n",
    );
    assert_eq!(
        (f.refusal, f.field, f.item, f.line),
        (Refusal::Missing, "protect.reason", Some(2), Some(5))
    );
    assert!(
        f.to_string().ends_with("(`protect.reason`, entry 2)"),
        "{f}"
    );
}

#[test]
fn a_file_in_the_repository_that_cannot_be_read_is_a_read_fault_that_says_where() {
    let bytes = format!("entry: none\n\nrungs:\n  moderate: {SECRET}\n");
    let e = read_policy(bytes.as_bytes()).unwrap_err();
    assert_eq!(e.file(), GovernanceFile::Policy);
    assert_eq!(e.line(), Some(4));
    assert_eq!(e.refusal(), Refusal::UnknownValue);
    let shown = e.to_string();
    assert!(
        shown.starts_with("governance/policy.yaml, line 4: "),
        "{shown}"
    );
    assert!(!shown.contains(SECRET));
    let e = read_constitution(b"policy_change: [\n").unwrap_err();
    assert_eq!(e.file(), GovernanceFile::Constitution);
    assert!(
        e.to_string()
            .starts_with("governance/constitution.yaml, line 1: "),
        "{e}"
    );
    // Not text is a fault of the whole file.
    let e = read_policy(&[0xff, 0xfe, 0x00]).unwrap_err();
    assert_eq!((e.refusal(), e.line()), (Refusal::NotText, None));
    assert!(e.to_string().starts_with("governance/policy.yaml: "), "{e}");
    assert_eq!(
        read_constitution(&[0xc3, 0x28]).unwrap_err().refusal(),
        Refusal::NotText
    );
    // A file in the repository is read without the platform limits: above one it is read, then lowered.
    let above = b"entry: contributor\n";
    let p = read_policy(above).unwrap();
    let (read, lowered) = p.as_read_under(&PlatformLimits {
        entry_max: Entry::None,
        limit_days: 90,
    });
    assert_eq!(read.entry(), Entry::None);
    assert_eq!(lowered.len(), 1);
    // The ordering rule is a read fault: a hand-written file that breaks it is not read.
    assert_eq!(
        read_policy(b"rungs:\n  moderate: observer\n")
            .unwrap_err()
            .refusal(),
        Refusal::Ordering(OrderingPair::ModerateBelowReviewer)
    );
    assert_eq!(
        read_policy(b"rungs:\n  moderate: admin\n  decide_appeal: maintainer\n")
            .unwrap_err()
            .refusal(),
        Refusal::Ordering(OrderingPair::DecideAppealBelowModerate)
    );
    // And a good file is read.
    assert_eq!(read_policy(b"").unwrap(), Policy::default());
    assert_eq!(read_constitution(b"").unwrap(), Constitution::default());
}

#[test]
fn the_bytes_forms_judge_like_the_text_forms() {
    let limits = PlatformLimits::default();
    assert_eq!(
        validate_policy_bytes(b"entry: observer\n", &limits)
            .unwrap()
            .entry(),
        Entry::Observer
    );
    assert_eq!(
        validate_policy_bytes(&[0xff], &limits).unwrap_err().refusal,
        Refusal::NotText
    );
    assert!(validate_constitution_bytes(b"policy_change:\n  approvals: 1\n").is_ok());
    assert_eq!(
        validate_constitution_bytes(&[0x80]).unwrap_err().refusal,
        Refusal::NotText
    );
}

#[test]
fn every_refusal_has_one_stable_name_and_a_sentence() {
    let all = [
        Refusal::NotText,
        Refusal::TooLong,
        Refusal::WrongShape,
        Refusal::UnknownKey,
        Refusal::RepeatedKey,
        Refusal::UnknownValue,
        Refusal::OutOfRange,
        Refusal::Bounds,
        Refusal::ForbiddenCharacter,
        Refusal::Missing,
        Refusal::Duplicate,
        Refusal::Ordering(OrderingPair::ProposeAboveReview),
        Refusal::Ordering(OrderingPair::ReviewAboveAccept),
        Refusal::Ordering(OrderingPair::ModerateBelowReviewer),
        Refusal::Ordering(OrderingPair::DecideAppealBelowModerate),
        Refusal::PlatformLimit(Limit::EntryMax),
        Refusal::PlatformLimit(Limit::LimitDays),
        Refusal::OutsideDefaultView,
    ];
    let names: std::collections::BTreeSet<_> = all.iter().map(|r| r.name()).collect();
    assert_eq!(names.len(), all.len(), "names are distinct");
    let sentences: std::collections::BTreeSet<_> = all.iter().map(|r| r.sentence()).collect();
    assert_eq!(sentences.len(), all.len(), "sentences are distinct");
    for r in all {
        assert!(r.name().chars().all(|c| c.is_ascii_lowercase() || c == '-'));
        assert!(r.sentence().is_ascii());
    }
}

// ------------------------------------------------------------------- the closed set of paths, and the names read back

#[test]
fn every_refusal_is_in_the_list_and_a_stored_name_reads_back_as_the_refusal_it_was() {
    // Written as an exhaustive match, so that a refusal added without a place in the list does not compile.
    fn listed(r: Refusal) -> bool {
        match r {
            Refusal::NotText
            | Refusal::TooLong
            | Refusal::WrongShape
            | Refusal::UnknownKey
            | Refusal::RepeatedKey
            | Refusal::UnknownValue
            | Refusal::OutOfRange
            | Refusal::Bounds
            | Refusal::ForbiddenCharacter
            | Refusal::Missing
            | Refusal::Duplicate
            | Refusal::Ordering(OrderingPair::ProposeAboveReview)
            | Refusal::Ordering(OrderingPair::ReviewAboveAccept)
            | Refusal::Ordering(OrderingPair::ModerateBelowReviewer)
            | Refusal::Ordering(OrderingPair::DecideAppealBelowModerate)
            | Refusal::PlatformLimit(Limit::EntryMax)
            | Refusal::PlatformLimit(Limit::LimitDays)
            | Refusal::OutsideDefaultView
            | Refusal::UnknownFile
            | Refusal::NotAFile
            | Refusal::PrefixIsNamespace
            | Refusal::TemplateScheme
            | Refusal::TemplateHost
            | Refusal::TemplateUserInfo
            | Refusal::TemplatePlaceholder
            | Refusal::TemplateCharacter => Refusal::ALL.contains(&r),
        }
    }
    for r in Refusal::ALL {
        assert!(listed(r), "{}", r.name());
        assert_eq!(Refusal::from_name(r.name()), Some(r));
    }
    let names: std::collections::BTreeSet<&str> = Refusal::ALL.iter().map(|r| r.name()).collect();
    assert_eq!(names.len(), Refusal::ALL.len(), "one name for one refusal");
    assert_eq!(Refusal::from_name("official"), None);
    assert_eq!(Refusal::from_name(""), None);
}

#[test]
fn a_read_fault_kept_by_a_snapshot_says_the_same_sentence_without_the_field() {
    let kept = ReadFault::from_stored(GovernanceFile::Policy, Some(4), Refusal::UnknownKey);
    assert_eq!(
        kept.to_string(),
        "governance/policy.yaml, line 4: the file has a key the rules of a wiki do not have"
    );
    let whole = ReadFault::from_stored(GovernanceFile::Constitution, None, Refusal::TooLong);
    assert_eq!(
        whole.to_string(),
        "governance/constitution.yaml: the file is longer than a governance file may be"
    );
    assert_eq!(
        (kept.file(), kept.line(), kept.refusal()),
        (GovernanceFile::Policy, Some(4), Refusal::UnknownKey)
    );
}

#[test]
fn only_the_two_files_are_a_path_under_governance_and_anything_else_is_refused_by_name() {
    for path in ["governance/policy.yaml", "governance/constitution.yaml"] {
        assert_eq!(refuse_unknown_files([path]), Ok(()), "{path}");
    }
    for path in [
        "governance/other.yaml",
        "governance/policy.yml",
        "governance/Policy.yaml",
        "governance/sub/policy.yaml",
        "governance/",
        "Governance/policy.yaml",
        "./governance/policy.yaml",
        "governance",
    ] {
        let fault = refuse_unknown_files(["pages/A.md", path]).unwrap_err();
        assert_eq!(fault.refusal, Refusal::UnknownFile, "{path}");
        assert_eq!(fault.name(), "unknown-file");
        assert_eq!(fault.file, None);
        assert_eq!(fault.path(), "governance/");
    }
    // Not a path under it at all.
    for path in [
        "pages/governance/policy.yaml",
        "governancey/x",
        "pages/A.md",
        "",
    ] {
        assert_eq!(refuse_unknown_files([path]), Ok(()), "{path:?}");
    }
}

#[test]
fn a_path_below_governance_is_one_a_removal_may_name_and_only_what_is_at_or_below_a_file_is_the_repair()
 {
    // Handoff G-T1 correction D-1: below `governance/`, spelled as the repository spells it.
    for path in [
        "governance/policy.yaml",
        "governance/policy.yaml/inner.txt",
        "governance/x/y.yaml",
        "governance/other.yaml",
    ] {
        assert!(is_below_governance(path), "{path}");
    }
    for path in [
        "governance",
        "governance/",
        "Governance/policy.yaml",
        "./governance/policy.yaml",
        "governancey/x",
        "pages/governance/policy.yaml",
        "pages/A.md",
        "",
    ] {
        assert!(!is_below_governance(path), "{path:?}");
    }
    // The repair is a change whose every path is one of the two files or lies below one.
    assert!(only_governance_files(["governance/policy.yaml"]));
    assert!(only_governance_files([
        "governance/policy.yaml/inner.txt",
        "governance/constitution.yaml",
        "governance/constitution.yaml/a/b.txt",
    ]));
    assert!(!only_governance_files([]));
    for path in [
        "governance/x/y.yaml",
        "governance/other.yaml",
        "governance/policy.yml/inner.txt",
        "Governance/policy.yaml",
        "pages/A.md",
    ] {
        assert!(
            !only_governance_files(["governance/policy.yaml", path]),
            "{path}"
        );
    }
    // The kind of change a path below a file is: that file's.
    assert_eq!(
        GovernanceChange::at_or_below("governance/policy.yaml/inner.txt"),
        Some(GovernanceChange::Policy)
    );
    assert_eq!(
        GovernanceChange::at_or_below("governance/constitution.yaml"),
        Some(GovernanceChange::Constitution)
    );
    assert_eq!(GovernanceChange::at_or_below("governance/x/y.yaml"), None);
    assert_eq!(
        GovernanceChange::of_path("governance/policy.yaml/inner.txt"),
        None
    );
}
