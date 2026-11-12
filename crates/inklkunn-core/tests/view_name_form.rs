//! The form of a view's name is checked twice, and the two must agree (handoff 005 K3): the core's `views::new_name`
//! before anything is recorded, and the store's `is_view_name` (and the SQLite `CHECK` behind it) when it is. A name
//! the core allows and the store refuses would be a view nobody could create; one the store allows and the core
//! refuses is a record the service would never write. Only the **form** is compared: the words a view may not be
//! called are the service's alone.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_core::views::{MAX_NAME_CHARS, new_name};
use inklkunn_store::views::{MAX_NAME_CHARS as STORE_MAX, is_view_name};

#[test]
fn the_store_and_the_core_agree_on_the_form_of_a_views_name() {
    assert_eq!(MAX_NAME_CHARS, STORE_MAX);
    let longest = "a".repeat(MAX_NAME_CHARS);
    let too_long = "a".repeat(MAX_NAME_CHARS + 1);
    let cases = [
        "",
        "a",
        "0",
        "draft",
        "draft-2",
        "a.b_c-d",
        "1.2",
        "A",
        "Draft",
        "dRaft",
        "a b",
        " a",
        "a ",
        "-a",
        "_a",
        ".a",
        "a-",
        "a_",
        "a.",
        "a..b",
        "a...",
        "a/b",
        "a:b",
        "a\\b",
        "a\u{0}b",
        "a\nb",
        "\u{e9}",
        "caf\u{e9}",
        "\u{3042}",
        "a\u{3042}",
        "heads/x",
        longest.as_str(),
        too_long.as_str(),
    ];
    for name in cases {
        assert_eq!(
            new_name(name).is_ok(),
            is_view_name(name),
            "the core and the store disagree about {name:?}"
        );
    }
}
