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
            "knowledge-files"
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
        "outside a curation proposal",
    ] {
        assert!(en.contains(part), "{part}: {en}");
    }
    let ja = crate::catalog::T::new(ja).get(rule_key(Rule::Status));
    for part in ["主張や出典の削除", "読み取れないファイル", "出典リンク"] {
        assert!(ja.contains(part), "{part}: {ja}");
    }
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
