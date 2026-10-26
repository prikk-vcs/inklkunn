//! RFC 001 D-6: no title escapes `pages/`; the function is deterministic; the inverse recovers a title
//! that has no characters to substitute.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_markup::path::{is_inside_pages, path_to_title, title_to_path};
use proptest::prelude::*;

proptest! {
    #[test]
    fn never_escapes_pages(title in "\\PC{0,64}") {
        if let Ok(p) = title_to_path(&title) {
            prop_assert!(is_inside_pages(&p), "{p}");
            prop_assert!(p.ends_with(".md"));
            prop_assert!(!p.contains("/../") && !p.contains("//"));
        }
    }

    #[test]
    fn deterministic(title in "\\PC{0,64}") {
        prop_assert_eq!(title_to_path(&title), title_to_path(&title));
    }

    #[test]
    fn inverse_recovers_clean_titles(title in "[A-Za-z0-9]([A-Za-z0-9 ]{0,18}[A-Za-z0-9])?(/[A-Za-z0-9]([A-Za-z0-9 ]{0,18}[A-Za-z0-9])?){0,2}") {
        let p = title_to_path(&title).unwrap();
        let back = path_to_title(&p).unwrap();
        let expected: String = title.split('/').map(str::trim).collect::<Vec<_>>().join("/");
        prop_assert_eq!(back, expected);
    }
}
