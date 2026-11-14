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
fn the_status_rule_is_worded_with_the_removals_and_the_unreadable_files_it_refuses() {
    // K29: the page's sentence says as much as the core's, in both languages.
    let ja = crate::catalog::Lang::from_tag("ja").unwrap();
    let en = crate::catalog::T::new(crate::catalog::EN).get(rule_key(Rule::Status));
    for part in [
        "removes a claim or a source",
        "cannot be read",
        "source link",
        "a source that was checked in its title, author, date, reference or the source that replaces it",
        "outside a curation proposal",
    ] {
        assert!(en.contains(part), "{part}: {en}");
    }
    let ja = crate::catalog::T::new(ja).get(rule_key(Rule::Status));
    for part in [
        "主張や出典の削除",
        "読み取れないファイル",
        "出典リンク",
        "確認済みの出典の題名・著者・日付・参照先・置き換え先",
    ] {
        assert!(ja.contains(part), "{part}: {ja}");
    }
}

#[test]
fn what_is_not_offered_says_so_and_names_no_reviewer_or_form_in_both_languages() {
    // K62: a removal of a claim or a source file, or the replacement of a file that cannot be read, is not offered in
    // this version (RFC 005 part 2). No reviewer and no form is named: none can do it.
    use crate::catalog::{EN, JA_DEF, Key, Lang, T};
    let en = T::new(EN).get(Key::KnowNotByForm);
    assert!(en.ends_with("is not offered in this version."), "{en}");
    for word in ["reviewer", "form", "curation", "proposal"] {
        assert!(!en.to_lowercase().contains(word), "{word}: {en}");
    }
    let ja = T::new(Lang::from_def(&JA_DEF)).get(Key::KnowNotByForm);
    assert!(ja.contains("このバージョンでは提供されていません"), "{ja}");
    for word in ["レビュアー", "フォーム", "キュレーション", "提案"] {
        assert!(!ja.contains(word), "{word}: {ja}");
    }
}

#[test]
fn the_sentences_that_say_what_only_curation_does_say_it_alike_in_both_languages() {
    // K36: the page's reason, the proposal page's note and the core's sentence name the same fields (title, author, date,
    // reference, and the source that replaces it; not the kind) and the same actions on an unreadable file (replaces or
    // removes), in English and in Japanese.
    use crate::catalog::{EN, JA_DEF, Key, Lang, T};
    for (t, fields, unreadable) in [
        (
            T::new(EN),
            &["title, author, date, reference or the source that replaces it"][..],
            "replaces or removes a file that cannot be read",
        ),
        (
            T::new(Lang::from_def(&JA_DEF)),
            &["題名・著者・日付・参照先・置き換え先"][..],
            "",
        ),
    ] {
        for key in [Key::HealthRuleStatus, Key::KnowNotCuration] {
            let text = t.get(key);
            for field in fields {
                assert!(text.contains(field), "{field}: {text}");
            }
            assert!(!text.contains("who or what"), "{text}");
            assert!(!text.contains("kind"), "{text}");
            assert!(text.contains(unreadable), "{unreadable}: {text}");
        }
    }
    let ja = T::new(Lang::from_def(&JA_DEF));
    for key in [Key::HealthRuleStatus, Key::KnowNotCuration] {
        let text = ja.get(key);
        assert!(text.contains("削除"), "{text}");
    }
    let ja = ja.get(Key::KnowNotCuration);
    assert!(
        ja.contains("読み取れないファイルを置き換えるか削除"),
        "{ja}"
    );
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
