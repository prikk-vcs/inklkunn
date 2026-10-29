#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

#[test]
fn a_200_gives_the_body_without_the_trailer() {
    assert_eq!(
        read_answer(b"{\"a\":1}\n200".to_vec(), 100).unwrap(),
        b"{\"a\":1}"
    );
    assert_eq!(read_answer(b"\n200".to_vec(), 100).unwrap(), b"");
    // A body that ends in what looks like a status is still read from the end.
    assert_eq!(
        read_answer(b"x\n404\n200".to_vec(), 100).unwrap(),
        b"x\n404"
    );
}

#[test]
fn other_statuses_are_typed() {
    assert_eq!(
        read_answer(b"no\n404".to_vec(), 100),
        Err(CurlError::Status(404))
    );
    assert_eq!(
        read_answer(b"\n204".to_vec(), 100),
        Err(CurlError::Status(204))
    );
    assert_eq!(
        read_answer(b"\n500".to_vec(), 100),
        Err(CurlError::Status(500))
    );
    for code in [301u16, 302, 303, 307, 308] {
        assert_eq!(
            read_answer(format!("moved\n{code}").into_bytes(), 100),
            Err(CurlError::Redirect(code))
        );
    }
    assert!(matches!(
        read_answer(b"\n000".to_vec(), 100),
        Err(CurlError::Malformed(_))
    ));
}

#[test]
fn a_missing_or_misshapen_trailer_is_malformed() {
    for bad in [
        &b""[..],
        b"200",
        b"abc200",
        b"body 200",
        b"body\n20",
        b"body\n2000",
        b"body\nabc",
    ] {
        assert!(
            matches!(read_answer(bad.to_vec(), 100), Err(CurlError::Malformed(_))),
            "{bad:?}"
        );
    }
}

#[test]
fn a_body_over_the_cap_is_too_large_even_if_the_runner_let_it_through() {
    assert_eq!(read_answer(b"12345\n200".to_vec(), 5).unwrap(), b"12345");
    assert_eq!(
        read_answer(b"123456\n200".to_vec(), 5),
        Err(CurlError::TooLarge)
    );
}

#[test]
fn curl_exit_codes_are_typed() {
    assert!(matches!(classify(7, "d".into()), CurlError::Connect(d) if d == "d"));
    assert_eq!(classify(28, String::new()), CurlError::TimedOut);
    assert_eq!(classify(63, String::new()), CurlError::TooLarge);
    for code in [35, 51, 58, 60, 77, 83] {
        assert!(matches!(classify(code, "d".into()), CurlError::Tls { code: c, .. } if c == code));
    }
    for code in [1, 3, 22, 52, 56] {
        assert!(matches!(classify(code, "d".into()), CurlError::Curl { code: c, .. } if c == code));
    }
    // The vector's pid 1 reports a signal death as 128 + n.
    assert_eq!(
        classify(137, String::new()),
        CurlError::Killed("by signal 9".to_owned())
    );
}

#[test]
fn a_missing_curl_is_found_before_anything_is_built() {
    let f = CurlFetcher::new().with_programs("bwrap", "/nonexistent/dir/curl");
    assert!(matches!(f.probe(), Err(CurlError::CurlMissing(p)) if p == "/nonexistent/dir/curl"));
}

#[test]
fn errors_say_what_happened_in_words_an_operator_can_use() {
    let shown = [
        CurlError::TimedOut,
        CurlError::TooLarge,
        CurlError::Redirect(302),
        CurlError::Status(404),
        CurlError::NoCaBundle,
        CurlError::Killed("by signal 9".into()),
    ]
    .iter()
    .map(ToString::to_string)
    .collect::<Vec<_>>()
    .join("|");
    for word in ["in time", "byte cap", "302", "404", "CA bundle", "signal 9"] {
        assert!(shown.contains(word), "{word} in {shown}");
    }
}
