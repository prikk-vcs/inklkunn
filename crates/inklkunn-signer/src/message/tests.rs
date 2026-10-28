#![allow(clippy::expect_used, clippy::unwrap_used)]

use proptest::prelude::*;

use super::*;

const ACCOUNT: &str = "019a0000-0000-7000-8000-0000000000a1";
const ACCEPTOR: &str = "019a0000-0000-7000-8000-0000000000b2";
const PROPOSAL_ID: &str = "019a0000-0000-7000-8000-0000000000c3";
const BLOCK: &str = "4bf5122f344554c53bde2ebb8cd2b7e3d0a5a8e9c0e5c2e0b1ad9b4f0b4e11aa";

fn account(id: &str, name: &str) -> Actor {
    Actor::Account {
        id: AccountId::parse(id).expect("id"),
        name: DisplayName::new(name).expect("name"),
    }
}

fn summary(s: &str) -> Summary {
    Summary::new(s).expect("summary")
}

fn proposal_message() -> Message {
    Message::proposal(
        summary("Fix a typo in the introduction"),
        ProposalId::parse(PROPOSAL_ID).expect("proposal"),
        account(ACCOUNT, "Aiko Tanaka"),
        account(ACCEPTOR, "Ben"),
        BlockId::new(BLOCK).expect("block"),
    )
    .expect("message")
}

#[test]
fn a_proposal_message_has_exactly_the_documented_text() {
    let text = proposal_message().to_text();
    assert_eq!(
        text,
        format!(
            "Fix a typo in the introduction\n\n\
             Proposal: {PROPOSAL_ID}\n\
             Proposed-By: {ACCOUNT} (Aiko Tanaka)\n\
             Accepted-By: {ACCEPTOR} (Ben)\n\
             Base: {BLOCK}\n\
             Kind: proposal\n"
        )
    );
}

#[test]
fn every_kind_round_trips_with_every_field() {
    let rollback = Message::rollback(
        summary("Undo “Fix a typo”"),
        ProposalId::parse(PROPOSAL_ID).expect("proposal"),
        Actor::Anonymous(Pseudonym::new("~k3x9q").expect("pseudonym")),
        account(ACCEPTOR, "Ben (maintainer)"),
        BlockId::new(BLOCK).expect("block"),
    )
    .expect("rollback");
    let import = Message::import(
        summary("Import 212 pages"),
        SystemLabel::new("import").expect("label"),
        ImportSource::new("https://example.org/wiki-bundle.tar").expect("source"),
    );
    let system = Message::system(
        summary("Create the wiki First Wiki"),
        SystemLabel::inklkunn(),
    );
    for m in [proposal_message(), rollback, import, system] {
        let text = m.to_text();
        assert!(text.ends_with('\n') && !text.ends_with("\n\n"), "{text:?}");
        assert!(!text.contains('\r'));
        let back = Message::parse(&text).unwrap_or_else(|e| panic!("{e}: {text:?}"));
        assert_eq!(back, m);
        assert_eq!(back.to_text(), text, "the canonical text is a fixed point");
    }
}

#[test]
fn accessors_return_what_was_composed() {
    let m = proposal_message();
    assert_eq!(m.kind(), Kind::Proposal);
    assert_eq!(m.summary().as_str(), "Fix a typo in the introduction");
    assert_eq!(m.proposal_id().map(ProposalId::as_str), Some(PROPOSAL_ID));
    assert_eq!(m.base().map(BlockId::as_str), Some(BLOCK));
    assert!(matches!(m.proposed_by(), Actor::Account { id, .. } if id.as_str() == ACCOUNT));
    assert!(matches!(m.accepted_by(), Actor::Account { id, .. } if id.as_str() == ACCEPTOR));
    assert!(m.imported_from().is_none());
}

/// The text M0 wrote for a created wiki must keep parsing: existing wikis have it in their history.
#[test]
fn the_m0_system_message_still_parses() {
    let text = "Create the wiki First Wiki\n\nProposed-By: system (inklkunn)\nAccepted-By: system (inklkunn)\nKind: system\n";
    let m = Message::parse(text).expect("parses");
    assert_eq!(m.kind(), Kind::System);
    assert_eq!(m.summary().as_str(), "Create the wiki First Wiki");
    assert_eq!(m.to_text(), text);
}

#[test]
fn a_display_name_with_parentheses_in_any_position_round_trips() {
    for name in [
        "Bob (dev)",
        "(Bob)",
        "Bob)",
        "Bob (",
        "(((",
        ")))",
        "Bob ) (x) (",
        "日本語 (にほんご)",
        "قارئ (reader)",
        "A: B",
        "Proposal: x",
    ] {
        let m = Message::proposal(
            summary("s"),
            ProposalId::parse(PROPOSAL_ID).expect("proposal"),
            account(ACCOUNT, name),
            account(ACCEPTOR, "Ben"),
            BlockId::new(BLOCK).expect("block"),
        )
        .expect("message");
        let back = Message::parse(&m.to_text()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back, m, "{name}");
        assert!(
            matches!(back.proposed_by(), Actor::Account { name: n, .. } if n.as_str() == name),
            "{name}"
        );
    }
}

/// Every refusal names a rule and never echoes the offending text.
#[test]
fn the_parser_refuses_everything_that_is_not_canonical() {
    let ok = proposal_message().to_text();
    let lines: Vec<&str> = ok.trim_end_matches('\n').split('\n').collect();
    let join = |ls: &[&str]| format!("{}\n", ls.join("\n"));
    let swap = |a: usize, b: usize| {
        let mut l = lines.clone();
        l.swap(a, b);
        join(&l)
    };
    let without = |i: usize| {
        let mut l = lines.clone();
        l.remove(i);
        join(&l)
    };
    let cases: Vec<(&str, String)> = vec![
        ("empty", String::new()),
        ("only a newline", "\n".to_owned()),
        ("no final line feed", ok.trim_end_matches('\n').to_owned()),
        ("two final line feeds", format!("{ok}\n")),
        ("CRLF", ok.replace('\n', "\r\n")),
        ("a lone CR", ok.replacen('\n', "\r", 1)),
        ("no blank line after the summary", without(1)),
        ("text on the blank line", ok.replacen("\n\n", "\n \n", 1)),
        ("trailers out of order", swap(2, 3)),
        ("kind before base", swap(5, 6)),
        ("a repeated trailer", {
            let mut repeated = lines.clone();
            repeated.push(lines.get(2).copied().expect("a trailer line"));
            join(&repeated)
        }),
        ("an unknown trailer", format!("{ok}Signed-Off-By: x\n")),
        (
            "a trailer without a colon",
            ok.replace("Kind: proposal", "Kind proposal"),
        ),
        (
            "a trailer with no space after the colon",
            ok.replace("Kind: proposal", "Kind:proposal"),
        ),
        (
            "trailing space in a value",
            ok.replace("Kind: proposal", "Kind: proposal "),
        ),
        ("an empty value", ok.replace("Kind: proposal", "Kind: ")),
        (
            "a blank line among the trailers",
            ok.replace("Accepted-By", "\nAccepted-By"),
        ),
        ("a missing proposal", without(2)),
        ("a missing proposer", without(3)),
        ("a missing acceptor", without(4)),
        ("a missing base", without(5)),
        ("a missing kind", without(6)),
        ("an unknown kind", ok.replace("proposal\n", "delete\n")),
        (
            "an upper-case kind",
            ok.replace("Kind: proposal", "Kind: Proposal"),
        ),
        (
            "an upper-case block id",
            ok.replace(BLOCK, &BLOCK.to_uppercase()),
        ),
        ("a short block id", ok.replace(BLOCK, &BLOCK[..63])),
        ("a long block id", ok.replace(BLOCK, &format!("{BLOCK}0"))),
        (
            "a non-hex block id",
            ok.replace(BLOCK, &BLOCK.replace('a', "g")),
        ),
        ("a bad proposal id", ok.replace(PROPOSAL_ID, "not-a-uuid")),
        (
            "an upper-case account id",
            ok.replace(ACCOUNT, &ACCOUNT.to_uppercase()),
        ),
        (
            "a name with no closing parenthesis",
            ok.replace("(Aiko Tanaka)", "(Aiko Tanaka"),
        ),
        ("no name", ok.replace(" (Aiko Tanaka)", "")),
        ("an empty name", ok.replace("(Aiko Tanaka)", "()")),
        (
            "a padded name",
            ok.replace("(Aiko Tanaka)", "( Aiko Tanaka)"),
        ),
        (
            "a system acceptor on a proposal",
            ok.replace(&format!("{ACCEPTOR} (Ben)"), "system (inklkunn)"),
        ),
        (
            "a system proposer on a proposal",
            ok.replace(&format!("{ACCOUNT} (Aiko Tanaka)"), "system (inklkunn)"),
        ),
        (
            "an anonymous acceptor",
            ok.replace(&format!("{ACCEPTOR} (Ben)"), "anonymous (~abc)"),
        ),
        (
            "a pseudonym without its tilde",
            ok.replace(&format!("{ACCOUNT} (Aiko Tanaka)"), "anonymous (abc)"),
        ),
        (
            "an imported-from on a proposal",
            format!("{ok}Imported-From: x\n"),
        ),
        (
            "a summary of 121 characters",
            format!("{}{}", "x".repeat(121), &ok[ok.find('\n').unwrap()..]),
        ),
        (
            "an empty summary",
            ok.replacen("Fix a typo in the introduction", "", 1),
        ),
        ("a summary with a tab", ok.replacen("Fix a", "Fix\ta", 1)),
        (
            "a summary with a leading space",
            ok.replacen("Fix", " Fix", 1),
        ),
        (
            "a summary with a trailing space",
            ok.replacen("introduction", "introduction ", 1),
        ),
    ];
    for (what, text) in &cases {
        let err = Message::parse(text);
        let e = err.as_ref().expect_err(what);
        let shown = e.to_string();
        assert!(!shown.is_empty(), "{what}");
        for secret in ["Aiko Tanaka", ACCOUNT, BLOCK, "Signed-Off-By"] {
            assert!(
                !shown.contains(secret),
                "{what}: the error echoes `{secret}`: {shown}"
            );
        }
    }
    // The system and import shapes have their own refusals.
    let system =
        "S\n\nProposed-By: system (inklkunn)\nAccepted-By: system (inklkunn)\nKind: system\n";
    assert!(Message::parse(system).is_ok());
    for (what, text) in [
        (
            "a system message with a proposal",
            system.replace(
                "Proposed-By",
                &format!("Proposal: {PROPOSAL_ID}\nProposed-By"),
            ),
        ),
        (
            "a system message with a base",
            system.replace("Kind:", &format!("Base: {BLOCK}\nKind:")),
        ),
        (
            "a system message with an account",
            system.replace("system (inklkunn)", &format!("{ACCOUNT} (A)")),
        ),
        (
            "a system message with an import source",
            format!("{system}Imported-From: x\n"),
        ),
        (
            "an import with no source",
            system.replace("Kind: system", "Kind: import"),
        ),
        (
            "a bad system label",
            system.replace("(inklkunn)", "(Inklkunn)"),
        ),
        (
            "a label with a double hyphen",
            system.replace("(inklkunn)", "(a--b)"),
        ),
    ] {
        assert!(Message::parse(&text).is_err(), "{what}");
    }
}

/// A person's name cannot reach a second line, so it cannot add a trailer to what is signed.
#[test]
fn a_display_name_cannot_inject_a_trailer() {
    for hostile in [
        "Eve)\nAccepted-By: 019a0000-0000-7000-8000-0000000000b2 (Mallory",
        "Eve\r\nKind: system",
        "Eve\u{85}x",
        "Eve\u{2028}x",
        "Eve\u{2029}x",
        "Eve\tx",
        "Eve\0x",
    ] {
        assert!(
            DisplayName::new(hostile).is_err(),
            "{hostile:?} must not be a display name"
        );
    }
    // And if a hostile text reached the parser anyway, it is not a message.
    let text = format!(
        "s\n\nProposal: {PROPOSAL_ID}\nProposed-By: {ACCOUNT} (Eve)\nAccepted-By: {ACCEPTOR} (Mallory)\nAccepted-By: {ACCEPTOR} (Ben)\nBase: {BLOCK}\nKind: proposal\n"
    );
    assert!(Message::parse(&text).is_err());
}

#[test]
fn the_constructors_enforce_who_may_act() {
    let proposal = ProposalId::parse(PROPOSAL_ID).expect("proposal");
    let base = BlockId::new(BLOCK).expect("block");
    let system = Actor::System(SystemLabel::inklkunn());
    // A proposal needs an account to accept it, and a proposer who is a person or a pseudonym.
    assert!(matches!(
        Message::proposal(
            summary("s"),
            proposal.clone(),
            account(ACCOUNT, "A"),
            system.clone(),
            base.clone()
        ),
        Err(MessageError::NotAllowed { .. })
    ));
    assert!(matches!(
        Message::proposal(
            summary("s"),
            proposal.clone(),
            system.clone(),
            account(ACCEPTOR, "B"),
            base.clone()
        ),
        Err(MessageError::NotAllowed { .. })
    ));
    assert!(matches!(
        Message::proposal(
            summary("s"),
            proposal.clone(),
            account(ACCOUNT, "A"),
            Actor::Anonymous(Pseudonym::new("~abc").expect("pseudonym")),
            base.clone(),
        ),
        Err(MessageError::NotAllowed { .. })
    ));
    // An anonymous proposer is fine when an account accepts.
    assert!(
        Message::proposal(
            summary("s"),
            proposal,
            Actor::Anonymous(Pseudonym::new("~abc").expect("pseudonym")),
            account(ACCEPTOR, "B"),
            base,
        )
        .is_ok()
    );
}

#[test]
fn summaries_are_normalized_from_free_text_and_strict_when_read_back() {
    // From free text (a proposal's "what and why"): line breaks become spaces, the ends are trimmed.
    let s = Summary::from_text("  Fix a typo\nAccepted-By: attacker\t!  ").expect("summary");
    assert_eq!(s.as_str(), "Fix a typo Accepted-By: attacker !");
    // Over the limit: cut to exactly 120 characters, closing with the ellipsis.
    let long = Summary::from_text(&"x".repeat(300)).expect("summary");
    assert_eq!(long.as_str().chars().count(), SUMMARY_MAX_CHARS);
    assert!(long.as_str().starts_with(&"x".repeat(119)));
    assert!(long.as_str().ends_with('…'));
    // Exactly at the limit is left alone.
    let exact = "y".repeat(SUMMARY_MAX_CHARS);
    assert_eq!(Summary::from_text(&exact).expect("summary").as_str(), exact);
    // Multi-byte characters count as characters, not bytes.
    let jp = Summary::from_text(&"あ".repeat(200)).expect("summary");
    assert_eq!(jp.as_str().chars().count(), SUMMARY_MAX_CHARS);
    // Nothing to say is refused rather than invented.
    for blank in ["", "   ", "\n\t\n", "\u{2028}"] {
        assert_eq!(
            Summary::from_text(blank),
            Err(MessageError::EmptySummary),
            "{blank:?}"
        );
    }
    // Strict mode (the parser's) does not normalize.
    assert_eq!(
        Summary::new("a\nb"),
        Err(MessageError::NotOneLine("summary"))
    );
    assert_eq!(
        Summary::new(&"x".repeat(121)),
        Err(MessageError::SummaryTooLong)
    );
    assert!(Summary::new(&"x".repeat(120)).is_ok());
}

#[test]
fn the_small_field_types_hold_their_shapes() {
    for ok in ["~a", "~abc123", "~0123456789abcdef"] {
        assert!(Pseudonym::new(ok).is_ok(), "{ok}");
    }
    for bad in ["", "~", "a", "~A", "~a b", "~0123456789abcdefg", "~日本"] {
        assert!(Pseudonym::new(bad).is_err(), "{bad}");
    }
    for ok in ["inklkunn", "import", "recovery", "a", "a-b-c", "a1"] {
        assert!(SystemLabel::new(ok).is_ok(), "{ok}");
    }
    for bad in [
        "",
        "1a",
        "-a",
        "a-",
        "a--b",
        "A",
        "a_b",
        "a b",
        &"a".repeat(33),
    ] {
        assert!(SystemLabel::new(bad).is_err(), "{bad}");
    }
    assert!(ImportSource::new("sha256:abcdef").is_ok());
    for bad in ["", " x", "x ", "a\nb", "a\u{2028}b"] {
        assert!(ImportSource::new(bad).is_err(), "{bad:?}");
    }
    assert!(ImportSource::new(&"u".repeat(IMPORT_SOURCE_MAX_CHARS + 1)).is_err());
}

fn display_name() -> impl Strategy<Value = String> {
    // Real-world names, parenthesis-heavy names, and arbitrary text: the filter keeps what the type accepts.
    prop_oneof![
        "[a-zA-Z0-9 ()あ-んア-ン一-龥:.,'-]{1,60}",
        "[()a ]{1,30}",
        any::<String>(),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Any display name the type accepts survives compose and parse unchanged; any it refuses is never
    /// composed (newlines and other controls are refused at the type, so no name can reach a second line).
    #[test]
    fn display_names_round_trip_or_are_refused_at_the_type(raw in display_name()) {
        match DisplayName::new(&raw) {
            Err(_) => prop_assert!(raw.is_empty() || raw.chars().count() > 120 || raw.chars().any(char::is_control) || raw.trim() != raw),
            Ok(name) => {
                let m = Message::proposal(
                    summary("s"),
                    ProposalId::parse(PROPOSAL_ID).expect("proposal"),
                    Actor::Account { id: AccountId::parse(ACCOUNT).expect("id"), name: name.clone() },
                    account(ACCEPTOR, "Ben"),
                    BlockId::new(BLOCK).expect("block"),
                ).expect("message");
                let text = m.to_text();
                // The summary, the blank line and five trailers: seven lines, each ended by one line feed.
                prop_assert_eq!(text.matches('\n').count(), 7, "a name added a line: {:?}", text);
                let back = Message::parse(&text).expect("parses");
                prop_assert_eq!(&back, &m);
                let Actor::Account { name: read_back, .. } = back.proposed_by() else {
                    return Err(TestCaseError::fail("the proposer is not an account"));
                };
                prop_assert_eq!(read_back, &name);
            }
        }
    }

    /// The parser is total: it returns a value or an error for any text, never panics, and what it accepts
    /// is exactly the canonical text (so no two texts name one message).
    #[test]
    fn the_parser_is_total_and_accepts_only_canonical_text(text in any::<String>()) {
        if let Ok(m) = Message::parse(&text) {
            prop_assert_eq!(m.to_text(), text);
        }
    }

    /// Mutating one character of a valid message either keeps it canonical (and then it round-trips) or
    /// makes it unparsable; it never yields a different message that serializes to the mutated text
    /// without being equal to what it parsed from.
    #[test]
    fn single_character_mutations_are_refused_or_canonical(pos in 0usize..400, c in any::<char>()) {
        let ok = proposal_message().to_text();
        let mut chars: Vec<char> = ok.chars().collect();
        if let Some(slot) = chars.get_mut(pos) {
            *slot = c;
        }
        let mutated: String = chars.into_iter().collect();
        if let Ok(m) = Message::parse(&mutated) {
            prop_assert_eq!(m.to_text(), mutated);
        }
    }
}
