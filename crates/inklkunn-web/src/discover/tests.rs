#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn a_json_string_cannot_be_broken_out_of() {
    assert_eq!(json_string("plain"), "\"plain\"");
    assert_eq!(json_string("a\"b\\c"), "\"a\\\"b\\\\c\"");
    assert_eq!(json_string("line\nbreak\ttab"), "\"line\\nbreak\\ttab\"");
    assert_eq!(json_string("\u{1}\u{2028}"), "\"\\u0001\\u2028\"");
    assert_eq!(json_string("日本語 é"), "\"日本語 é\"");
    // `</script>` has no special meaning in a document of its own, and is escaped text in any case.
    assert_eq!(json_string("</script>"), "\"</script>\"");
}

#[test]
fn a_license_is_linked_only_when_it_is_one_identifier() {
    assert_eq!(
        license_href("CC-BY-SA-4.0").as_deref(),
        Some("https://spdx.org/licenses/CC-BY-SA-4.0.html")
    );
    assert_eq!(license_href("MIT OR Apache-2.0"), None);
    assert_eq!(license_href("javascript:alert(1)"), None);
    assert_eq!(license_href(""), None);
}

#[test]
fn a_query_value_is_encoded_but_for_the_unreserved() {
    assert_eq!(query_escape("Home-page_1.~"), "Home-page_1.~");
    assert_eq!(query_escape("A B&c=d/日"), "A%20B%26c%3Dd%2F%E6%97%A5");
}
