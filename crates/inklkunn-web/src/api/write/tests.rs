#![allow(clippy::expect_used, clippy::unwrap_used)]

use axum::http::HeaderValue;

use super::base::{self, ProposalTag};
use super::*;

fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.append(*name, HeaderValue::from_str(value).unwrap());
    }
    map
}

const ID: &str = "019a0000-0000-7000-8000-00000000cccc";

#[test]
fn only_a_bearer_token_is_a_token() {
    assert!(!presented_token(&headers(&[])).unwrap());
    assert!(presented_token(&headers(&[("authorization", "Bearer ink_abc")])).unwrap());
    assert!(presented_token(&headers(&[("authorization", "bearer ink_abc")])).unwrap());
    for bad in [
        "Basic dXNlcjpwYXNz",
        "Bearer",
        "Bearer ",
        "Bearer a b",
        "Token ink_abc",
        "ink_abc",
        "",
    ] {
        let problem = presented_token(&headers(&[("authorization", bad)])).unwrap_err();
        assert_eq!(problem.code, Code::InvalidToken, "{bad:?}");
    }
    let two = headers(&[("authorization", "Bearer a"), ("authorization", "Bearer b")]);
    assert_eq!(presented_token(&two).unwrap_err().code, Code::InvalidToken);
}

#[test]
fn a_body_is_json_when_it_says_so_and_nothing_else_is() {
    for good in [
        "application/json",
        "Application/JSON",
        "application/json; charset=utf-8",
        "application/json;charset=\"UTF-8\"",
    ] {
        assert!(is_json(&headers(&[("content-type", good)])), "{good}");
    }
    for bad in [
        "application/x-www-form-urlencoded",
        "text/plain",
        "application/json; charset=latin1",
        "application/json; boundary=x",
        "application/jsonp",
        "application/vnd.api+json",
        "",
    ] {
        assert!(!is_json(&headers(&[("content-type", bad)])), "{bad:?}");
    }
    assert!(!is_json(&headers(&[])));
}

#[test]
fn a_proposals_tag_is_its_id_and_revision_in_one_spelling() {
    let tag = ProposalTag {
        id: ProposalId::parse(ID).unwrap(),
        revision: 3,
    };
    assert_eq!(tag.render(), format!("\"{ID}-r3\""));
    assert_eq!(ProposalTag::parse(&format!("{ID}-r3")), Some(tag));
    for bad in [
        format!("{ID}-r03"),
        format!("{ID}-r+3"),
        format!("{ID}-r"),
        format!("{ID}-r-1"),
        format!("{ID}-3"),
        ID.to_string(),
        "-r3".to_owned(),
        format!("x{ID}-r3"),
        format!("{ID}-r99999999999"),
        String::new(),
    ] {
        assert_eq!(ProposalTag::parse(&bad), None, "{bad}");
    }
}

#[test]
fn if_match_is_one_quoted_strong_tag() {
    let one = |v: &str| base::if_match(&headers(&[("if-match", v)]));
    assert_eq!(one("\"abc\"").unwrap(), "abc");
    assert_eq!(one("  \"abc\"  ").unwrap(), "abc");
    for bad in [
        "*",
        "abc",
        "W/\"abc\"",
        "\"a\", \"b\"",
        "\"\"",
        "\"a b\"",
        "\"a",
    ] {
        assert_eq!(one(bad).unwrap_err().code, Code::InvalidParameter, "{bad}");
    }
    assert_eq!(
        base::if_match(&headers(&[])).unwrap_err().code,
        Code::BaseRequired
    );
    let two = headers(&[("if-match", "\"a\""), ("if-match", "\"b\"")]);
    assert_eq!(
        base::if_match(&two).unwrap_err().code,
        Code::InvalidParameter
    );
}
