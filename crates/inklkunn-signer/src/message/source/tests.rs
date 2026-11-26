#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::super::{ImportSource, Kind, Message, SystemLabel};
use super::*;
use crate::message::Summary;

const FROM: &str = "bundle sha256:00";

fn import(revision: SourceRevision) -> Message {
    Message::import_revision(
        Summary::new("Import a revision of Guides/Setup").expect("summary"),
        SystemLabel::new("import").expect("label"),
        ImportSource::new(FROM).expect("source"),
        revision,
    )
}

fn full() -> SourceRevision {
    SourceRevision::new("1234", 1_234_567_890)
        .expect("revision")
        .with_author(SourceAuthorKind::Person, "Ada (the first)")
        .with_summary("Fix: a typo")
        .minor(true)
        .automated(true)
}

#[test]
fn a_replayed_revision_has_exactly_the_documented_text_and_reads_back() {
    let m = import(full());
    let text = m.to_text();
    assert_eq!(
        text,
        "Import a revision of Guides/Setup\n\n\
         Proposed-By: system (import)\n\
         Accepted-By: system (import)\n\
         Kind: import\n\
         Imported-From: bundle sha256:00\n\
         Imported-Rev: 1234\n\
         Imported-At: 2009-02-13T23:31:30Z\n\
         Imported-Author: person (Ada (the first))\n\
         Imported-Summary: Fix: a typo\n\
         Imported-Minor: true\n\
         Imported-Automated: true\n"
    );
    let back = Message::parse(&text).expect("parses");
    assert_eq!(back, m);
    assert_eq!(back.to_text(), text, "the canonical text is a fixed point");
    let source = back.source().expect("the facts");
    assert_eq!(source.rev(), "1234");
    assert_eq!(source.at(), 1_234_567_890);
    assert_eq!(
        source.author().map(SourceAuthor::name),
        Some("Ada (the first)")
    );
    assert_eq!(
        source.author().map(SourceAuthor::kind),
        Some(SourceAuthorKind::Person)
    );
    assert_eq!(source.summary(), Some("Fix: a typo"));
    assert!(source.is_minor() && source.is_automated());
}

#[test]
fn what_the_source_hid_is_said_hidden_and_never_an_empty_value() {
    let both = SourceRevision::new("r", 0)
        .expect("revision")
        .with_author_hidden()
        .with_summary_hidden();
    let text = import(both.clone()).to_text();
    assert!(
        text.contains("Imported-Hidden: author, summary\n"),
        "{text}"
    );
    assert!(!text.contains("Imported-Author"), "{text}");
    assert!(!text.contains("Imported-Summary"), "{text}");
    assert!(
        !text.contains("Imported-Minor"),
        "false is not written: {text}"
    );
    assert_eq!(Message::parse(&text).expect("parses").source(), Some(&both));
    let one = SourceRevision::new("r", 0)
        .expect("revision")
        .with_author(SourceAuthorKind::Bot, "AWB")
        .with_summary_hidden();
    let text = import(one.clone()).to_text();
    assert!(text.contains("Imported-Hidden: summary\n"), "{text}");
    assert_eq!(Message::parse(&text).expect("parses").source(), Some(&one));
    // A part given after it was hidden is given, and one hidden after it was given is hidden: never both.
    let hidden_then_given = SourceRevision::new("r", 0)
        .expect("revision")
        .with_author_hidden()
        .with_author(SourceAuthorKind::Person, "A");
    assert!(!hidden_then_given.author_hidden());
    let given_then_hidden = SourceRevision::new("r", 0)
        .expect("revision")
        .with_author(SourceAuthorKind::Person, "A")
        .with_author_hidden();
    assert!(given_then_hidden.author().is_none() && given_then_hidden.author_hidden());
}

#[test]
fn a_revision_with_nothing_but_its_id_and_time_is_a_message() {
    let bare = SourceRevision::new("a", 86_400).expect("revision");
    let text = import(bare.clone()).to_text();
    assert!(
        text.ends_with("Imported-Rev: a\nImported-At: 1970-01-02T00:00:00Z\n"),
        "{text}"
    );
    assert_eq!(Message::parse(&text).expect("parses").source(), Some(&bare));
}

#[test]
fn a_name_and_a_summary_are_made_one_line_and_bounded_and_the_text_of_a_person_cannot_add_a_trailer()
 {
    let hostile = "Eve\nImported-At: 1999-01-01T00:00:00Z\r\nKind: system\u{85}\u{2028}x";
    let r = SourceRevision::new("r", 5)
        .expect("revision")
        .with_author(SourceAuthorKind::Person, hostile)
        .with_summary(hostile);
    let text = import(r).to_text();
    assert_eq!(text.matches("Imported-At: 1970").count(), 1, "{text}");
    assert_eq!(text.matches("\nKind: ").count(), 1, "{text}");
    assert!(Message::parse(&text).is_ok(), "{text}");
    // Bounded, with a closing mark.
    let long = "x".repeat(900);
    let r = SourceRevision::new("r", 5)
        .expect("revision")
        .with_author(SourceAuthorKind::Person, &long)
        .with_summary(&long);
    assert_eq!(
        r.author().expect("author").name().chars().count(),
        SOURCE_NAME_MAX_CHARS
    );
    assert!(r.author().expect("author").name().ends_with('…'));
    assert_eq!(
        r.summary().expect("summary").chars().count(),
        SOURCE_SUMMARY_MAX_CHARS
    );
    assert!(Message::parse(&import(r).to_text()).is_ok());
    // Nothing left is nothing given.
    let r = SourceRevision::new("r", 5)
        .expect("revision")
        .with_author(SourceAuthorKind::Person, " \t\n ")
        .with_summary("\u{2028}");
    assert!(r.author().is_none() && r.summary().is_none());
    // A name with the parentheses and the colons of a person's name survives whole.
    for name in [
        "Bob (dev)",
        "(((",
        ")))",
        "A: B",
        "Imported-Rev: x",
        "日本語 (にほんご)",
    ] {
        let r = SourceRevision::new("r", 5)
            .expect("revision")
            .with_author(SourceAuthorKind::Group, name);
        let back = Message::parse(&import(r.clone()).to_text()).expect(name);
        assert_eq!(back.source(), Some(&r), "{name}");
        assert_eq!(
            back.source()
                .and_then(SourceRevision::author)
                .map(SourceAuthor::name),
            Some(name)
        );
    }
}

#[test]
fn a_rev_is_an_identifier_and_is_refused_when_it_is_not_one_line() {
    for bad in [
        "", " a", "a ", "a\nb", "a\rb", "a\tb", "a\u{0}b", "\u{2028}",
    ] {
        assert!(SourceRevision::new(bad, 0).is_err(), "{bad:?}");
    }
    assert!(SourceRevision::new(&"r".repeat(256), 0).is_ok());
    assert!(SourceRevision::new(&"r".repeat(257), 0).is_err());
    assert!(
        SourceRevision::new("r", 253_402_300_800).is_err(),
        "the year 10000"
    );
    assert!(
        SourceRevision::new("r", -62_135_596_801).is_err(),
        "before the year 1"
    );
    assert!(SourceRevision::new("r", 253_402_300_799).is_ok());
    assert!(SourceRevision::new("r", -62_135_596_800).is_ok());
}

#[test]
fn times_round_trip_across_the_range_and_only_the_canonical_text_reads() {
    for secs in [
        -62_135_596_800,
        -1,
        0,
        951_782_400,
        1_078_099_199,
        1_234_567_890,
        4_102_444_800,
        253_402_300_799,
    ] {
        let text = import(SourceRevision::new("r", secs).expect("revision")).to_text();
        assert_eq!(
            Message::parse(&text)
                .expect("parses")
                .source()
                .map(SourceRevision::at),
            Some(secs),
            "{secs}"
        );
    }
    let ok = import(SourceRevision::new("r", 1_234_567_890).expect("revision")).to_text();
    for bad in [
        "2009-02-13 23:31:30Z",
        "2009-02-13T23:31:30",
        "2009-02-13T23:31:30+00:00",
        "2009-02-13T23:31:30.5Z",
        "2009-02-13t23:31:30z",
        "2009-2-13T23:31:30Z",
        "2009-13-13T23:31:30Z",
        "2009-02-30T23:31:30Z",
        "2009-02-13T24:31:30Z",
        "2009-02-13T23:60:30Z",
        "2009-02-13T23:31:60Z",
        "0000-01-01T00:00:00Z",
        "+009-02-13T23:31:30Z",
        "２００９-02-13T23:31:30Z",
        "1234567890",
    ] {
        let text = ok.replace("2009-02-13T23:31:30Z", bad);
        let e = Message::parse(&text).expect_err(bad);
        assert!(
            !e.to_string().contains(bad),
            "{bad}: the error echoes the value: {e}"
        );
    }
}

#[test]
fn every_rule_of_the_trailers_refuses_and_none_echoes_a_value() {
    let ok = import(full()).to_text();
    let lines: Vec<&str> = ok.trim_end_matches('\n').split('\n').collect();
    let join = |l: &[&str]| format!("{}\n", l.join("\n"));
    let without = |name: &str| {
        join(
            &lines
                .iter()
                .copied()
                .filter(|l| !l.starts_with(name))
                .collect::<Vec<_>>(),
        )
    };
    let swap = |a: usize, b: usize| {
        let mut l = lines.clone();
        l.swap(a, b);
        join(&l)
    };
    let secret = "SECRET-VALUE";
    let cases: Vec<(&str, String)> = vec![
        ("a rev with no time", without("Imported-At")),
        ("a time with no rev", without("Imported-Rev")),
        ("the time before the rev", swap(5, 6)),
        ("an author before the time", swap(6, 7)),
        ("a repeated rev", format!("{ok}Imported-Rev: x\n")),
        (
            "an unknown kind of author",
            ok.replace("person (Ada", &format!("{secret} (Ada")),
        ),
        (
            "an author with no kind",
            ok.replace("person (Ada (the first))", "Ada"),
        ),
        (
            "an author with no closing parenthesis",
            ok.replace("person (Ada (the first))", "person (Ada"),
        ),
        (
            "an empty name",
            ok.replace("person (Ada (the first))", "person ()"),
        ),
        ("a padded name", ok.replace("person (Ada", "person ( Ada")),
        (
            "a minor flag that is false",
            ok.replace("Imported-Minor: true", "Imported-Minor: false"),
        ),
        (
            "an automated flag that is something else",
            ok.replace(
                "Imported-Automated: true",
                &format!("Imported-Automated: {secret}"),
            ),
        ),
        (
            "a hidden list of something else",
            ok.replace(
                "Imported-Minor",
                &format!("Imported-Hidden: {secret}\nImported-Minor"),
            ),
        ),
        (
            "a part hidden and given",
            ok.replace("Imported-Minor", "Imported-Hidden: summary\nImported-Minor"),
        ),
        (
            "the hidden list in the wrong order",
            ok.replace(
                "Imported-Minor",
                "Imported-Hidden: summary, author\nImported-Minor",
            ),
        ),
        (
            "the facts on a system message",
            "S\n\nProposed-By: system (inklkunn)\nAccepted-By: system (inklkunn)\nKind: system\nImported-Rev: r\nImported-At: 1970-01-01T00:00:00Z\n"
                .to_owned(),
        ),
        (
            "an empty value",
            ok.replace("Imported-Rev: 1234", "Imported-Rev: "),
        ),
        (
            "a summary with a padded value",
            ok.replace("Imported-Summary: Fix", "Imported-Summary:  Fix"),
        ),
        (
            "the facts without the source",
            ok.replace("Imported-From: bundle sha256:00\n", ""),
        ),
    ];
    for (what, text) in &cases {
        let e = Message::parse(text)
            .err()
            .unwrap_or_else(|| panic!("{what}: parsed"));
        let shown = e.to_string();
        for echoed in [secret, "Ada", "1234", "Fix"] {
            assert!(
                !shown.contains(echoed),
                "{what}: echoes `{echoed}`: {shown}"
            );
        }
    }
}

#[test]
fn only_an_import_may_carry_the_facts() {
    let system = Message::parse(
        "S\n\nProposed-By: system (inklkunn)\nAccepted-By: system (inklkunn)\nKind: system\n",
    )
    .expect("system");
    assert_eq!(system.kind(), Kind::System);
    assert!(system.source().is_none());
    let plain = Message::import(
        Summary::new("Import 3 pages").expect("summary"),
        SystemLabel::new("import").expect("label"),
        ImportSource::new(FROM).expect("source"),
    );
    assert!(plain.source().is_none());
    assert_eq!(Message::parse(&plain.to_text()).expect("parses"), plain);
}
