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
            "stale"
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
