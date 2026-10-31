use std::collections::HashMap;

use axum::body::Body;
use axum::extract::{Form, FromRequest as _};
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode, header};

use super::{fields, parse};

/// What axum's own `Form` extractor makes of `body`.
async fn extractor(body: &str) -> Option<HashMap<String, String>> {
    let request = Request::post("/")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body.to_owned()))
        .unwrap();
    Form::<HashMap<String, String>>::from_request(request, &())
        .await
        .ok()
        .map(|Form(fields)| fields)
}

#[tokio::test]
async fn the_parser_agrees_with_the_extractor_it_replaces() {
    for body in [
        "",
        "a=1",
        "a=1&b=2",
        "a=1&&b=2&",
        "a",
        "a=",
        "=x",
        "a=1&a=2",
        "a=b+c%20d",
        "a=%E6%97%A5%E6%9C%AC%E8%AA%9E",
        "%E3%83%86=%E3%82%B9%E3%83%88",
        "a=100%25&b=%2B&c=%26%3D",
        "a=%zz&b=%4&c=%",
        "a=x%0D%0Ay",
        "pages=1&p0_path=pages%2FHome.md&p0_text=Line+one%0D%0ALine+two&summary=Fix+a+%26+b",
        "a=1=2",
        "a%3Db=c",
    ] {
        assert_eq!(parse(body.as_bytes()), extractor(body).await, "{body:?}");
    }
}

#[test]
fn a_body_that_is_not_text_is_not_a_form() {
    assert_eq!(parse(b"a=%FF"), None);
    assert_eq!(parse(b"a=\xFF"), None);
    assert_eq!(parse(b"\xFF=1"), None);
}

fn form_headers(content_type: Option<&str>) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if let Some(value) = content_type {
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(value).unwrap());
    }
    headers
}

#[test]
fn the_body_is_taken_as_a_form_only_when_it_is_sent_as_one() {
    let body = || Ok(axum::body::Bytes::from_static(b"a=1"));
    for good in [
        "application/x-www-form-urlencoded",
        "application/x-www-form-urlencoded; charset=UTF-8",
        "Application/X-WWW-Form-Urlencoded",
        " application/x-www-form-urlencoded ;charset=utf-8",
    ] {
        assert_eq!(
            fields(&form_headers(Some(good)), body()).unwrap().get("a"),
            Some(&"1".to_owned()),
            "{good}"
        );
    }
    for bad in [None, Some("text/plain"), Some("application/json"), Some("")] {
        assert_eq!(
            fields(&form_headers(bad), body()),
            Err(StatusCode::UNSUPPORTED_MEDIA_TYPE),
            "{bad:?}"
        );
    }
    assert_eq!(
        fields(
            &form_headers(Some("application/x-www-form-urlencoded")),
            Ok(axum::body::Bytes::from_static(b"a=%FF"))
        ),
        Err(StatusCode::BAD_REQUEST)
    );
}
