#![allow(clippy::expect_used, clippy::unwrap_used)]

use axum::http::{HeaderMap, HeaderValue, header};

use super::*;

fn with(values: &[&str]) -> HeaderMap {
    let mut h = HeaderMap::new();
    for v in values {
        h.append(header::AUTHORIZATION, HeaderValue::from_str(v).unwrap());
    }
    h
}

#[test]
fn a_bearer_credential_is_a_token_a_malformed_one_or_nothing_of_ours() {
    assert_eq!(presented(&HeaderMap::new()), Presented::Nothing);
    assert_eq!(
        presented(&with(&["Bearer ink_x"])),
        Presented::Token("ink_x")
    );
    assert_eq!(
        presented(&with(&["bearer ink_x"])),
        Presented::Token("ink_x")
    );
    assert_eq!(
        presented(&with(&["BEARER ink_x"])),
        Presented::Token("ink_x")
    );
    // It names the scheme, so the request presented a credential, and a bad one: not a visitor.
    for bad in ["Bearer", "Bearer ", "Bearer a b", "Bearer  a"] {
        assert_eq!(presented(&with(&[bad])), Presented::Malformed, "{bad:?}");
    }
    assert_eq!(
        presented(&with(&["Bearer ink_x", "Bearer ink_y"])),
        Presented::Malformed
    );
    assert_eq!(
        presented(&with(&["Basic dXNlcjpwdw==", "Bearer ink_x"])),
        Presented::Malformed
    );
    // Another scheme (a proxy's Basic) is not ours and does not stop a cookie.
    assert_eq!(
        presented(&with(&["Basic dXNlcjpwdw=="])),
        Presented::Nothing
    );
    assert_eq!(
        presented(&with(&["Digest x", "Basic y"])),
        Presented::Nothing
    );
    assert_eq!(presented(&with(&["Bearerish x"])), Presented::Nothing);
}

#[test]
fn a_session_cookie_is_noticed_by_name_without_a_lookup() {
    let cookie = |v: &str| {
        let mut h = HeaderMap::new();
        h.insert(header::COOKIE, HeaderValue::from_str(v).unwrap());
        h
    };
    assert!(carries_session_cookie(&cookie(
        "a=b; __Host-inklkunn-session=whatever"
    )));
    assert!(!carries_session_cookie(&cookie("inklkunn_lang=ja")));
    assert!(!carries_session_cookie(&HeaderMap::new()));
}
