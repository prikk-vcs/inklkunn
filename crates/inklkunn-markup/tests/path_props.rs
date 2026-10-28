//! RFC 001 D-6: no title escapes `pages/`; the path is ASCII; the function is deterministic; the inverse
//! recovers the normalized, per-component trimmed title **exactly**, for every title that has a path.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_markup::path::{collision_key, is_inside_pages, path_to_title, title_to_path};
use proptest::prelude::*;
use unicode_normalization::UnicodeNormalization;

proptest! {
    #[test]
    fn never_escapes_pages_and_is_ascii(title in "\\PC{0,64}") {
        if let Ok(p) = title_to_path(&title) {
            prop_assert!(is_inside_pages(&p), "{p}");
            prop_assert!(p.ends_with(".md"));
            prop_assert!(p.is_ascii());
            prop_assert!(!p.contains("/../") && !p.contains("//"));
            prop_assert!(p.len() <= 1024);
        }
    }

    #[test]
    fn deterministic(title in "\\PC{0,64}") {
        prop_assert_eq!(title_to_path(&title), title_to_path(&title));
    }

    #[test]
    fn inverse_recovers_every_title_that_has_a_path(title in "\\PC{0,64}") {
        if let Ok(p) = title_to_path(&title) {
            let back = path_to_title(&p).expect("a path we made decodes");
            let nfc: String = title.nfc().collect();
            let expected: String = nfc
                .trim()
                .trim_matches('/')
                .split('/')
                .map(str::trim)
                .collect::<Vec<_>>()
                .join("/");
            prop_assert_eq!(back, expected);
        }
    }

    #[test]
    fn distinct_titles_never_share_a_path(a in "[A-Za-z0-9 ._%-]{1,12}", b in "[A-Za-z0-9 ._%-]{1,12}") {
        let na: String = a.nfc().collect();
        let nb: String = b.nfc().collect();
        if na.trim() != nb.trim()
            && let (Ok(pa), Ok(pb)) = (title_to_path(&a), title_to_path(&b)) {
            prop_assert_ne!(pa, pb);
        }
    }

    #[test]
    fn collision_keys_fold_case_only(title in "[A-Za-z]{1,8}") {
        // A Windows device name (CON, PRN, AUX, NUL) has its first letter percent-encoded *as written*
        // (`%70rn` for `pRN`, `%50RN` for `PRN`). Those two paths are different bytes even on a
        // case-insensitive file system, so prikk sees no collision and their keys rightly differ. (This was a
        // flaw in the property, not in the key: it failed once the generator happened on `pRN`.)
        prop_assume!(!["CON", "PRN", "AUX", "NUL"].contains(&title.to_ascii_uppercase().as_str()));
        let p = title_to_path(&title).unwrap();
        let q = title_to_path(&title.to_uppercase()).unwrap();
        prop_assert_eq!(collision_key(&p), collision_key(&q));
    }
}
