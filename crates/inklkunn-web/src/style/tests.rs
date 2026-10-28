#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn the_sheet_is_served_under_a_name_made_of_its_own_bytes() {
    let name = file_name();
    let hex = name
        .strip_prefix("site-")
        .and_then(|n| n.strip_suffix(".css"))
        .expect("site-<fingerprint>.css");
    assert_eq!(hex.len(), 16);
    assert!(
        hex.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
    assert_eq!(path(), format!("/-/assets/{name}"));
    assert_eq!(
        ROUTE.replace("{file}", name),
        path(),
        "the route pattern and the path agree"
    );
}

#[test]
fn the_fingerprint_changes_with_a_byte_and_does_not_with_the_clock() {
    let a = fingerprint(CSS.as_bytes());
    assert_eq!(a, fingerprint(CSS.as_bytes()));
    assert_ne!(a, fingerprint(format!("{CSS} ").as_bytes()));
    assert_ne!(a, fingerprint(CSS.replacen('a', "b", 1).as_bytes()));
    // FNV-1a's published vectors.
    assert_eq!(fingerprint(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fingerprint(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fingerprint(b"foobar"), 0x8594_4171_f739_67e8);
}

#[test]
fn the_embedded_sheet_is_not_empty_and_is_plain_css() {
    assert!(CSS.len() > 500);
    assert!(!CSS.contains("@import"), "one stylesheet, no other");
    assert!(!CSS.contains("url("), "no resource of any kind");
}
