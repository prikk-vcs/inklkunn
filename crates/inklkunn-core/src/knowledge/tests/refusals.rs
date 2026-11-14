//! What a knowledge file may not be: outside the subset, numeric, unknown, half-specified. A file that is not
//! wholly well formed gives no claim at all, and a refusal never repeats what the file said.

use super::*;

fn good_claim_text() -> String {
    format!(
        "kind: observation\nlifecycle: proposed\ncommunity_status:\n  status: unknown\n\
         expressions:\n  - page_id: p1\n    block: b1\nsources:\n  - source: {}\n    relation: supports\n    \
         bearing: direct\n    judged_on: {}\n",
        source_id(1),
        digest_of("words")
    )
}

fn claim_rule(text: &str) -> Rule {
    Claim::parse(claim_id(1), text).expect_err("refused").rule
}

fn source_rule(text: &str) -> Rule {
    Source::parse(source_id(1), text).expect_err("refused").rule
}

#[test]
fn the_good_text_is_read() {
    Claim::parse(claim_id(1), &good_claim_text()).expect("read");
}

#[test]
fn outside_the_subset_is_refused() {
    let with = |extra: &str| format!("{}{extra}", good_claim_text());
    // An anchor, an alias and a tag are outside the plain subset (threat model C-4b).
    assert_eq!(claim_rule(&with("note: &a x\n")), Rule::Syntax);
    assert_eq!(claim_rule(&with("note: *a\n")), Rule::Syntax);
    assert_eq!(claim_rule(&with("note: !!str x\n")), Rule::Syntax);
    // A key repeated in a block mapping, and in a flow mapping.
    assert_eq!(claim_rule(&with("kind: opinion\n")), Rule::Syntax);
    let flow = good_claim_text().replace(
        "  - page_id: p1\n    block: b1\n",
        "  - {page_id: p1, block: b1, block: b2}\n",
    );
    assert_eq!(claim_rule(&flow), Rule::Syntax);
    // Bad indentation, an open quote, a line that is no key.
    assert_eq!(
        claim_rule("kind: observation\n   lifecycle: x\n"),
        Rule::Syntax
    );
    assert_eq!(claim_rule("kind: \"observation\n"), Rule::Syntax);
    assert_eq!(claim_rule("just words\n"), Rule::Type);
    // The same for a source.
    assert_eq!(
        source_rule("kind: primary\nkind: other\navailability: unknown\n"),
        Rule::Syntax
    );
    assert_eq!(
        source_rule("kind: &a primary\navailability: unknown\n"),
        Rule::Syntax
    );
}

#[test]
fn a_repeated_key_is_refused_and_the_message_does_not_name_it() {
    let err = Claim::parse(
        claim_id(1),
        "kind: observation\nkind: opinion\nlifecycle: proposed\n",
    )
    .expect_err("refused");
    assert_eq!(err.rule, Rule::Syntax);
    assert_eq!(err.line, Some(2));
    let shown = err.to_string();
    assert!(!shown.contains("opinion"), "{shown}");
}

#[test]
fn nothing_numeric_is_accepted_anywhere() {
    // A number in every position that takes a text or a word.
    for (from, to) in [
        ("bearing: direct", "bearing: 3"),
        ("bearing: direct", "bearing: 0.9"),
        ("status: unknown", "status: 1"),
        ("block: b1", "block: 7"),
        ("page_id: p1", "page_id: 42"),
        ("kind: observation", "kind: 5"),
    ] {
        let text = good_claim_text().replace(from, to);
        assert_eq!(claim_rule(&text), Rule::Number, "{to}");
    }
    let note = good_claim_text().replace("bearing: direct", "bearing: direct\n    note: 12");
    assert_eq!(claim_rule(&note), Rule::Number);
    // A year is text, quoted.
    let year = good_claim_text().replace("bearing: direct", "bearing: direct\n    note: \"1950\"");
    Claim::parse(claim_id(1), &year).expect("a quoted year is a text");
    // The same in a source.
    for text in [
        "kind: primary\navailability: unknown\nidentity:\n  date: 1951\n",
        "kind: primary\navailability: unknown\nidentity:\n  title: 3.5\n",
        "kind: primary\navailability: unknown\nreference:\n  type: url\n  value: 9\n",
        "kind: 1\navailability: unknown\n",
    ] {
        assert_eq!(source_rule(text), Rule::Number, "{text}");
    }
    // No enumeration has a word that is a number, so none can be written one way and read another.
    let words = StatementKind::ALL
        .iter()
        .map(|w| w.word())
        .chain(Bearing::ALL.iter().map(|w| w.word()));
    for word in words {
        assert!(word.parse::<f64>().is_err(), "{word}");
    }
}

#[test]
fn the_format_has_no_key_for_a_score_a_rank_or_a_truth() {
    for key in [
        "score",
        "rank",
        "truth",
        "weight",
        "confidence",
        "verified",
        "truthful",
    ] {
        let text = format!("{}{key}: high\n", good_claim_text());
        assert_eq!(claim_rule(&text), Rule::Unknown, "{key}");
        let text = format!("kind: primary\navailability: unknown\n{key}: high\n");
        assert_eq!(source_rule(&text), Rule::Unknown, "{key}");
    }
    let nested =
        good_claim_text().replace("bearing: direct", "bearing: direct\n    strength: high");
    assert_eq!(claim_rule(&nested), Rule::Unknown);
}

#[test]
fn closed_lists_refuse_a_word_they_do_not_have() {
    let bad = |from: &str, to: &str| claim_rule(&good_claim_text().replace(from, to));
    assert_eq!(bad("kind: observation", "kind: fact"), Rule::Value);
    assert_eq!(
        bad("lifecycle: proposed", "lifecycle: verified"),
        Rule::Value
    );
    assert_eq!(bad("status: unknown", "status: true"), Rule::Type);
    assert_eq!(bad("status: unknown", "status: canonical"), Rule::Value);
    assert_eq!(bad("bearing: direct", "bearing: strong"), Rule::Value);
    assert_eq!(bad("bearing: direct", "bearing: unknown"), Rule::Value);
    assert_eq!(bad("relation: supports", "relation: proves"), Rule::Value);
    assert_eq!(
        source_rule("kind: reliable\navailability: unknown\n"),
        Rule::Value
    );
    assert_eq!(
        source_rule("kind: primary\navailability: maybe\n"),
        Rule::Value
    );
}

#[test]
fn a_missing_key_gives_no_half_read_claim() {
    for text in [
        "",
        "kind: observation\n",
        "kind: observation\nlifecycle: proposed\n",
        "lifecycle: proposed\ncommunity_status:\n  status: unknown\n",
    ] {
        assert_eq!(claim_rule(text), Rule::Missing, "{text:?}");
    }
    let no_digest =
        good_claim_text().replace(&format!("    judged_on: {}\n", digest_of("words")), "");
    assert_eq!(claim_rule(&no_digest), Rule::Missing);
    assert_eq!(source_rule("kind: primary\n"), Rule::Missing);
    assert_eq!(source_rule(""), Rule::Missing);
}

#[test]
fn malformed_ids_digests_and_dates_are_refused() {
    let bad = |from: &str, to: &str| claim_rule(&good_claim_text().replace(from, to));
    assert_eq!(bad("block: b1", "block: \"b 1\""), Rule::Id);
    assert_eq!(bad("block: b1", "block: \"^b1\""), Rule::Id);
    assert_eq!(bad("page_id: p1", "page_id: \"\""), Rule::Id);
    assert_eq!(
        bad(
            &source_id(1).to_string(),
            "01931111-0000-4000-8000-000000000001"
        ),
        Rule::Id
    );
    assert_eq!(bad(digest_of("words").as_str(), "abc"), Rule::Value);
    assert_eq!(
        source_rule(
            "kind: primary\navailability: unknown\nlast_reviewed:\n  date: \"31 October\"\n"
        ),
        Rule::Value
    );
    assert_eq!(
        source_rule(
            "kind: primary\navailability: unknown\nlast_reviewed:\n  date: \"2026-13-01\"\n"
        ),
        Rule::Value
    );
    assert_eq!(
        source_rule(
            "kind: primary\navailability: unknown\nsnapshot:\n  kind: attachment\n  attachment: \"../../etc\"\n"
        ),
        Rule::Value
    );
}

#[test]
fn a_repeated_entry_is_refused() {
    let twice = good_claim_text().replace(
        "  - page_id: p1\n    block: b1\n",
        "  - page_id: p1\n    block: b1\n  - page_id: p1\n    block: b1\n",
    );
    assert_eq!(claim_rule(&twice), Rule::Duplicate);
    let rel = format!(
        "{}relations:\n  - type: refines\n    claim: {c}\n  - type: refines\n    claim: {c}\n",
        good_claim_text(),
        c = claim_id(5)
    );
    assert_eq!(claim_rule(&rel), Rule::Duplicate);
    // The same claim under two relation types is two relations.
    let two = format!(
        "{}relations:\n  - type: refines\n    claim: {c}\n  - type: contradicts\n    claim: {c}\n",
        good_claim_text(),
        c = claim_id(5)
    );
    Claim::parse(claim_id(1), &two).expect("read");
    let src = good_claim_text().replace(
        &format!("    judged_on: {}\n", digest_of("words")),
        &format!(
            "    judged_on: {d}\n  - source: {}\n    relation: challenges\n    bearing: partial\n    judged_on: {d}\n",
            source_id(1),
            d = digest_of("words")
        ),
    );
    assert_eq!(claim_rule(&src), Rule::Duplicate);
}

#[test]
fn a_claim_does_not_relate_to_itself_nor_a_source_replace_itself() {
    let me = format!(
        "{}relations:\n  - type: refines\n    claim: {}\n",
        good_claim_text(),
        claim_id(1)
    );
    assert_eq!(claim_rule(&me), Rule::Value);
    assert_eq!(
        source_rule(&format!(
            "kind: primary\navailability: unknown\nsuperseded_by: {}\n",
            source_id(1)
        )),
        Rule::Value
    );
}

#[test]
fn lists_and_texts_are_bounded() {
    let many = (0..=MAX_EXPRESSIONS)
        .map(|i| format!("  - page_id: p\n    block: b{i}\n"))
        .collect::<String>();
    let text = format!(
        "kind: observation\nlifecycle: proposed\ncommunity_status:\n  status: unknown\nexpressions:\n{many}"
    );
    assert_eq!(claim_rule(&text), Rule::Bounds);
    let long = "x".repeat(MAX_TEXT_CHARS + 1);
    assert_eq!(
        source_rule(&format!(
            "kind: primary\navailability: unknown\nidentity:\n  title: \"{long}\"\n"
        )),
        Rule::Bounds
    );
}

#[test]
fn a_file_is_judged_by_its_path_its_size_and_its_bytes() {
    let good = good_claim_text();
    let ok = claim_path(&claim_id(1));
    assert!(read_file(&ok, good.as_bytes()).is_ok());
    let fault = |path: &str, bytes: &[u8]| read_file(path, bytes).expect_err("refused").rule;
    assert_eq!(
        fault("knowledge/claims/not-an-id.yaml", good.as_bytes()),
        Rule::Path
    );
    assert_eq!(fault("knowledge/other/x.yaml", good.as_bytes()), Rule::Path);
    assert_eq!(fault("pages/Home.md", good.as_bytes()), Rule::Path);
    assert_eq!(
        fault(
            &format!("knowledge/claims/sub/{}.yaml", claim_id(1)),
            good.as_bytes()
        ),
        Rule::Path
    );
    assert_eq!(
        fault(
            &format!("knowledge/claims/{}.yml", claim_id(1)),
            good.as_bytes()
        ),
        Rule::Path
    );
    assert_eq!(fault(&ok, &[0xff, 0xfe, 0x00]), Rule::NotText);
    assert_eq!(fault(&ok, &vec![b'#'; MAX_FILE_BYTES + 1]), Rule::TooLong);
}

#[test]
fn a_refusal_never_repeats_what_the_file_said() {
    // Every position that holds a word of the file's own: none of it may appear in the message.
    let secret = "SECRET-WORDS-7f3a";
    let texts = [
        format!("{secret}: x\n"),
        format!("kind: {secret}\n"),
        format!("kind: observation\nlifecycle: {secret}\n"),
        format!("{}{secret}: 1\n", good_claim_text()),
        good_claim_text().replace("block: b1", &format!("block: \"{secret} x\"")),
        good_claim_text().replace("bearing: direct", &format!("bearing: {secret}")),
        format!("kind: \"{secret}\n"),
    ];
    for text in texts {
        let fault = Claim::parse(claim_id(1), &text).expect_err("refused");
        let shown = format!("{fault} / {fault:?} / {}", fault.rule);
        assert!(!shown.contains(secret), "{shown}");
    }
}

#[test]
fn every_rule_has_a_code_that_reads_back_and_a_sentence() {
    for rule in Rule::ALL {
        assert_eq!(Rule::from_code(rule.code()), Some(rule));
        assert!(!rule.sentence().is_empty());
    }
    let mut codes: Vec<_> = Rule::ALL.iter().map(|r| r.code()).collect();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), Rule::ALL.len(), "the codes are distinct");
    assert_eq!(Rule::from_code("nonsense"), None);
}

#[test]
fn the_status_rule_names_what_a_curation_proposal_does_and_leaves_removals_to_their_own_sentence() {
    // K29, K67: the sentence of this rule names what a curation proposal can do; a removal, and a file that cannot be
    // read, are not offered in this version and are said so by the refusal itself, once.
    let sentence = Rule::Status.sentence();
    for part in [
        "community status",
        "review date",
        "availability",
        "source link",
        "a source that was checked in its title, author, date, reference or the source that replaces it",
        "outside a curation proposal",
    ] {
        assert!(sentence.contains(part), "{part}: {sentence}");
    }
    for part in [
        "removes a claim or a source",
        "cannot be read",
        "replaces or removes",
    ] {
        assert!(!sentence.contains(part), "{part}: {sentence}");
    }
}

#[test]
fn a_refusal_of_a_removal_or_an_unreadable_file_says_it_is_not_offered_and_not_what_curation_may_do()
 {
    use crate::knowledge::check::{NOT_OFFERED, Refusal, UNREADABLE};
    let said = |removes: bool, field: &'static str| Refusal {
        file: "knowledge/claims/x.yaml".to_owned(),
        rule: Rule::Status,
        field,
        line: None,
        removes,
    };
    // What a curation proposal can make keeps the rule's sentence.
    let made = said(false, "community_status");
    assert_eq!(made.sentence(), Rule::Status.sentence());
    assert!(made.to_string().contains(Rule::Status.sentence()));
    // A removal, and a file that cannot be read, say they are not offered: never the rule's sentence beside it.
    for refusal in [said(true, "community_status"), said(false, UNREADABLE)] {
        assert_eq!(refusal.sentence(), NOT_OFFERED);
        let text = refusal.to_string();
        assert!(text.contains("is not offered in this version"), "{text}");
        assert!(!text.contains(Rule::Status.sentence()), "{text}");
        assert!(!text.contains("curation proposal"), "{text}");
    }
}
