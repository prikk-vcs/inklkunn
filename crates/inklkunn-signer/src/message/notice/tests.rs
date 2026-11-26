#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;
use crate::message::{ImportSource, Kind, Message, SourceRevision, Summary, SystemLabel};

const FROM: &str = "bundle sha256:00";

fn change(notice: ImportNotice, revision: Option<SourceRevision>) -> Message {
    Message::import_of_bundle(
        Summary::new("Import 3 pages").expect("summary"),
        SystemLabel::new("import").expect("label"),
        ImportSource::new(FROM).expect("source"),
        notice,
        revision,
    )
}

#[test]
fn the_notice_of_a_bundle_has_exactly_the_documented_text_and_reads_back() {
    let notice = ImportNotice::new(
        1_764_000_000,
        ChecksState::None,
        LicenseWord::DeclaredByAdmin,
    )
    .expect("notice")
    .with_source_name("Example Wiki (2012 dump)")
    .with_source_address("https://wiki.example.org/");
    let m = change(notice.clone(), None);
    let text = m.to_text();
    assert_eq!(
        text,
        "Import 3 pages\n\n\
         Proposed-By: system (import)\n\
         Accepted-By: system (import)\n\
         Kind: import\n\
         Imported-From: bundle sha256:00\n\
         Imported-On: 2025-11-24T16:00:00Z\n\
         Imported-Source-Name: Example Wiki (2012 dump)\n\
         Imported-Source-Address: https://wiki.example.org/\n\
         Imported-Checks: none\n\
         Imported-License: declared-by-admin\n"
    );
    let back = Message::parse(&text).expect("parses");
    assert_eq!(back, m);
    assert_eq!(back.to_text(), text);
    assert_eq!(back.notice(), Some(&notice));
    assert!(
        back.source().is_none(),
        "the first change names no revision"
    );
    let n = back.notice().expect("notice");
    assert_eq!(n.on(), 1_764_000_000);
    assert_eq!(n.source_name(), Some("Example Wiki (2012 dump)"));
    assert_eq!(n.source_address(), Some("https://wiki.example.org/"));
    assert_eq!(n.checks(), ChecksState::None);
    assert_eq!(n.license(), LicenseWord::DeclaredByAdmin);
}

#[test]
fn a_notice_with_no_source_named_is_the_day_the_checksums_and_the_licence() {
    let m = change(
        ImportNotice::new(0, ChecksState::Verified, LicenseWord::Manifest).expect("notice"),
        None,
    );
    let text = m.to_text();
    assert!(
        text.ends_with(
            "Imported-On: 1970-01-01T00:00:00Z\nImported-Checks: verified\nImported-License: manifest\n"
        ),
        "{text}"
    );
    assert_eq!(Message::parse(&text).expect("parses"), m);
}

#[test]
fn a_name_and_an_address_are_text_of_one_line_and_cannot_add_a_trailer() {
    let hostile = "Wiki\nImported-Checks: verified\r\nKind: system\u{2028}x";
    let notice = ImportNotice::new(5, ChecksState::None, LicenseWord::Manifest)
        .expect("notice")
        .with_source_name(hostile)
        .with_source_address(hostile);
    let text = change(notice, None).to_text();
    assert_eq!(text.matches("\nImported-Checks: ").count(), 1, "{text}");
    assert_eq!(text.matches("\nKind: ").count(), 1, "{text}");
    assert!(Message::parse(&text).is_ok(), "{text}");
    let long = "x".repeat(900);
    let notice = ImportNotice::new(5, ChecksState::None, LicenseWord::Manifest)
        .expect("notice")
        .with_source_name(&long)
        .with_source_address(&long);
    assert_eq!(
        notice.source_name().expect("name").chars().count(),
        SOURCE_NAME_MAX_CHARS
    );
    assert_eq!(
        notice.source_address().expect("address").chars().count(),
        SOURCE_ADDRESS_MAX_CHARS
    );
    assert!(Message::parse(&change(notice, None).to_text()).is_ok());
    let nothing = ImportNotice::new(5, ChecksState::None, LicenseWord::Manifest)
        .expect("notice")
        .with_source_name(" \n ")
        .with_source_address("");
    assert!(nothing.source_name().is_none() && nothing.source_address().is_none());
}

#[test]
fn every_rule_of_the_notice_refuses_and_none_echoes_a_value() {
    let ok = change(
        ImportNotice::new(1_764_000_000, ChecksState::Verified, LicenseWord::Manifest)
            .expect("notice")
            .with_source_name("Name")
            .with_source_address("Address"),
        None,
    )
    .to_text();
    let secret = "SECRET-VALUE";
    let cases: Vec<(&str, String)> = vec![
        ("no day", ok.replace("Imported-On: 2025-11-24T16:00:00Z\n", "")),
        ("no checks", ok.replace("Imported-Checks: verified\n", "")),
        ("no licence", ok.replace("Imported-License: manifest\n", "")),
        (
            "a day that is not canonical",
            ok.replace("2025-11-24T16:00:00Z", &format!("{secret}-2025-11-24T16:00:00Z")),
        ),
        (
            "a day with an offset",
            ok.replace("2025-11-24T16:00:00Z", "2025-11-24T16:00:00+00:00"),
        ),
        (
            "checks that are something else",
            ok.replace("Imported-Checks: verified", &format!("Imported-Checks: {secret}")),
        ),
        (
            "a licence that is something else",
            ok.replace("Imported-License: manifest", &format!("Imported-License: {secret}")),
        ),
        (
            "a name with a padded value",
            ok.replace("Imported-Source-Name: Name", "Imported-Source-Name:  Name"),
        ),
        (
            "the address before the name",
            ok.replace(
                "Imported-Source-Name: Name\nImported-Source-Address: Address\n",
                "Imported-Source-Address: Address\nImported-Source-Name: Name\n",
            ),
        ),
        (
            "the checks before the day",
            ok.replace(
                "Imported-On: 2025-11-24T16:00:00Z\n",
                "",
            )
            .replace(
                "Imported-Checks: verified\n",
                "Imported-Checks: verified\nImported-On: 2025-11-24T16:00:00Z\n",
            ),
        ),
        (
            "a notice on a system message",
            "S\n\nProposed-By: system (inklkunn)\nAccepted-By: system (inklkunn)\nKind: system\nImported-On: 2025-11-24T16:00:00Z\nImported-Checks: none\nImported-License: manifest\n"
                .to_owned(),
        ),
    ];
    for (what, text) in &cases {
        let e = Message::parse(text)
            .err()
            .unwrap_or_else(|| panic!("{what}: parsed"));
        let shown = e.to_string();
        for echoed in [secret, "Address", "Name"] {
            assert!(
                !shown.contains(echoed),
                "{what}: echoes `{echoed}`: {shown}"
            );
        }
    }
    assert!(ImportNotice::new(253_402_300_800, ChecksState::None, LicenseWord::Manifest).is_err());
    assert!(ImportNotice::new(-62_135_596_801, ChecksState::None, LicenseWord::Manifest).is_err());
}

#[test]
fn a_notice_belongs_to_an_import_and_a_revision_to_a_notice() {
    let m = Message::parse(
        &change(
            ImportNotice::new(1, ChecksState::None, LicenseWord::Manifest).expect("notice"),
            Some(SourceRevision::new("r", 2).expect("revision")),
        )
        .to_text(),
    )
    .expect("parses");
    assert_eq!(m.kind(), Kind::Import);
    assert!(m.notice().is_some() && m.source().is_some());
}
