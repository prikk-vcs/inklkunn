#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn every_list_has_its_address_and_the_address_leads_back_to_it() {
    let slugs: Vec<&str> = Kind::ALL.iter().map(|k| k.slug()).collect();
    assert_eq!(
        slugs,
        [
            "wanted",
            "orphans",
            "dead-ends",
            "redirects",
            "wants-help",
            "stale",
            "knowledge-files",
            "sources-to-review"
        ]
    );
    for kind in Kind::ALL {
        assert_eq!(Kind::from_slug(kind.slug()), Some(kind));
    }
    assert_eq!(Kind::from_slug("Wanted"), None);
    assert_eq!(Kind::from_slug(""), None);
    assert_eq!(Kind::from_slug("../wanted"), None);
}

#[test]
fn the_status_rule_is_worded_with_what_a_curation_proposal_does_and_not_with_removals() {
    // K29, K67: the page's sentence says as much as the core's, in both languages; a removal and an unreadable file are
    // said by `know.not_by_form`, not here.
    let ja = crate::catalog::Lang::from_tag("ja").unwrap();
    let en = crate::catalog::T::new(crate::catalog::EN).get(rule_key(Rule::Status));
    for part in [
        "source link",
        "a source that was checked in its title, author, date, reference or the source that replaces it",
        "outside a curation proposal",
    ] {
        assert!(en.contains(part), "{part}: {en}");
    }
    for part in ["removes a claim or a source", "cannot be read"] {
        assert!(!en.contains(part), "{part}: {en}");
    }
    let ja = crate::catalog::T::new(ja).get(rule_key(Rule::Status));
    for part in [
        "出典との結びつき",
        "確認済みの出典のタイトル・著者・日付・参照先・置き換え先",
    ] {
        assert!(ja.contains(part), "{part}: {ja}");
    }
    for part in ["主張や出典の削除", "読み取れないファイル"] {
        assert!(!ja.contains(part), "{part}: {ja}");
    }
}

#[test]
fn the_removal_clause_is_said_once_in_each_language_by_the_sentence_that_says_it_is_not_offered() {
    // K67: of the sentences that say a rule (the page's reason, the proposal page's note, the core's sentence) and the
    // ones that say what is not offered and who replaces a file, only those speak of a removal or of a file that cannot be read.
    use crate::catalog::{EN, JA_DEF, Key, Lang, T};
    let removal = |text: &str, words: &[&str]| words.iter().any(|w| text.contains(w));
    let en_words = ["removes a claim", "Removing a claim", "cannot be read"];
    let ja_words = [
        "主張や出典の削除",
        "主張や出典を削除",
        "主張や出典のファイルを削除",
        "読み取れないファイル",
    ];
    for (t, words) in [
        (T::new(EN), &en_words[..]),
        (T::new(Lang::from_def(&JA_DEF)), &ja_words[..]),
    ] {
        for key in [Key::HealthRuleStatus, Key::KnowNotCuration] {
            assert!(!removal(&t.get(key), words), "{key:?}: {}", t.get(key));
        }
        assert!(removal(&t.get(Key::KnowNotByForm), words));
        assert!(removal(&t.get(Key::KnowUnreadableByModerate), words));
    }
    assert!(!removal(Rule::Status.sentence(), &en_words));
}

#[test]
fn what_is_not_offered_says_what_is_offered_instead_and_the_sentences_of_0_3_0_are_gone_in_both_languages()
 {
    // RFC 006 D-10 (handoff G-T6): K62's sentence, *not offered in this version*, is replaced. A removal of a claim or a source file
    // is not offered, and the claim or the source is retired instead, by a curation proposal that says why; a file that cannot be
    // read is replaced, or retired, by whoever may moderate the wiki. No sentence says *in this version* any more.
    use crate::catalog::{EN, JA_DEF, Key, Lang, T};
    let en = T::new(EN);
    let not_by_form = en.get(Key::KnowNotByForm);
    assert!(
        not_by_form.starts_with("Removing a claim or a source file is not offered."),
        "{not_by_form}"
    );
    assert!(
        not_by_form.contains("retire it with a curation proposal, and say why"),
        "{not_by_form}"
    );
    let by_moderate = en.get(Key::KnowUnreadableByModerate);
    assert!(
        by_moderate.contains("someone who may moderate this wiki"),
        "{by_moderate}"
    );
    assert!(by_moderate.contains("or retired"), "{by_moderate}");
    for word in ["reviewer", "form"] {
        assert!(
            !by_moderate.to_lowercase().contains(word),
            "{word}: {by_moderate}"
        );
    }
    let ja = T::new(Lang::from_def(&JA_DEF));
    let (ja_form, ja_moderate) = (
        ja.get(Key::KnowNotByForm),
        ja.get(Key::KnowUnreadableByModerate),
    );
    assert!(
        ja_form.contains("削除する操作は提供していません"),
        "{ja_form}"
    );
    assert!(
        ja_form.contains("役目を終えたことにしてください"),
        "{ja_form}"
    );
    assert!(
        ja_moderate.contains("モデレーションを行える役割を持つ人"),
        "{ja_moderate}"
    );
    for t in [&en, &ja] {
        for key in [
            Key::KnowNotByForm,
            Key::KnowUnreadableByModerate,
            Key::HealthKnowledgeAction,
        ] {
            let text = t.get(key);
            assert!(!text.contains("in this version"), "{key:?}: {text}");
            assert!(!text.contains("このバージョンでは"), "{key:?}: {text}");
        }
    }
}

#[test]
fn the_sentences_that_say_what_only_curation_does_say_it_alike_in_both_languages() {
    // K36, K67: the page's reason, the proposal page's note and the core's sentence name the same fields (title, author,
    // date, reference, and the source that replaces it; not the kind), in English and in Japanese. A source link taken
    // away is still said (a link's removal is something a curation proposal makes); a file's removal is not (K67).
    use crate::catalog::{EN, JA_DEF, Key, Lang, T};
    for (t, fields) in [
        (
            T::new(EN),
            &["title, author, date, reference or the source that replaces it"][..],
        ),
        (
            T::new(Lang::from_def(&JA_DEF)),
            &["タイトル・著者・日付・参照先・置き換え先"][..],
        ),
    ] {
        for key in [Key::HealthRuleStatus, Key::KnowNotCuration] {
            let text = t.get(key);
            for field in fields {
                assert!(text.contains(field), "{field}: {text}");
            }
            assert!(!text.contains("who or what"), "{text}");
            assert!(!text.contains("kind"), "{text}");
        }
    }
    let ja = T::new(Lang::from_def(&JA_DEF));
    for key in [Key::HealthRuleStatus, Key::KnowNotCuration] {
        let text = ja.get(key);
        assert!(text.contains("出典との結びつき"), "{text}");
        assert!(text.contains("変更・削除"), "{text}");
    }
    // The core's sentence is the English one.
    assert!(
        Rule::Status
            .sentence()
            .contains("title, author, date, reference or the source that replaces it")
    );
}

#[test]
fn the_pages_and_the_api_name_the_same_lists() {
    use inklkunn_core::read::HealthList;
    let api: Vec<&str> = HealthList::ALL.iter().map(|l| l.slug()).collect();
    let pages: Vec<&str> = Kind::ALL.iter().map(|k| k.slug()).collect();
    assert_eq!(api, pages);
}

#[test]
fn what_resolves_a_row_is_one_word_in_the_pages_and_one_code_in_the_api() {
    let all = [
        Resolve::WantedAdd,
        Resolve::Orphan,
        Resolve::DeadEnd,
        Resolve::Broken,
        Resolve::Loop,
        Resolve::Long,
        Resolve::Double,
        Resolve::Help,
        Resolve::Review,
        Resolve::KnowledgeFile,
        Resolve::SourceCheck,
    ];
    let ja = crate::catalog::Lang::from_tag("ja").unwrap();
    for resolve in all {
        let code = resolve.code();
        assert!(
            code.split('-')
                .all(|w| !w.is_empty() && w.bytes().all(|b| b.is_ascii_lowercase())),
            "{code}"
        );
        // The words are the catalog's, in both languages.
        for lang in [crate::catalog::EN, ja] {
            let words = crate::catalog::T::new(lang).get(resolve.key());
            assert!(
                !words.trim().is_empty() && !words.starts_with('['),
                "{resolve:?}"
            );
        }
    }
    // Two rows that the pages tell to do the same thing share a code, and no other two do.
    let mut codes: Vec<&str> = all.iter().map(|r| r.code()).collect();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), all.len() - 1);
    assert_eq!(Resolve::Long.code(), Resolve::Double.code());
    assert_eq!(Resolve::of_redirect(RedirectIssueKind::Loop), Resolve::Loop);
    assert_eq!(
        Resolve::of_redirect(RedirectIssueKind::TooLong),
        Resolve::Long
    );
}
