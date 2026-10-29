#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;
use crate::request::FetchTarget;

const SECRET_AUTH: &str = "Basic c2VjcmV0LWNsaWVudC1pZDpzZWNyZXQtY2xpZW50LXNlY3JldA==";
const SECRET_FORM: &str =
    "grant_type=authorization_code&code=SECRETCODE&code_verifier=SECRETVERIFIER";

fn request(form: Option<&str>, auth: Option<&str>) -> CurlRequest {
    CurlRequest {
        target: FetchTarget {
            url: "https://idp.example.org/token".to_owned(),
            host: "idp.example.org".to_owned(),
            port: 443,
            addrs: vec!["203.0.113.7".parse().unwrap()],
        },
        form: form.map(str::to_owned),
        authorization: auth.map(str::to_owned),
    }
}

fn value_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).map(String::as_str)
}

fn limits() -> FetchLimits {
    FetchLimits::default()
}

#[test]
fn every_flag_that_confines_curl_is_present() {
    let inv = invocation(
        &request(None, None),
        &limits(),
        "/etc/ssl/certs/ca-certificates.crt",
    )
    .unwrap();
    let a = &inv.args;
    assert_eq!(
        a.first().map(String::as_str),
        Some("-q"),
        "no config file, first"
    );
    assert_eq!(value_after(a, "--proto"), Some("=https"));
    assert_eq!(value_after(a, "--proto-redir"), Some("=https"));
    assert_eq!(value_after(a, "--max-redirs"), Some("0"));
    assert_eq!(value_after(a, "--noproxy"), Some("*"));
    assert_eq!(value_after(a, "--max-time"), Some("15"));
    assert_eq!(value_after(a, "--connect-timeout"), Some("5"));
    assert_eq!(value_after(a, "--max-filesize"), Some("1048576"));
    assert_eq!(
        value_after(a, "--cacert"),
        Some("/etc/ssl/certs/ca-certificates.crt")
    );
    assert_eq!(
        value_after(a, "--url"),
        Some("https://idp.example.org/token")
    );
    assert_eq!(value_after(a, "--config"), Some("-"));
    for flag in ["--silent", "--show-error", "--globoff", "--tlsv1.2"] {
        assert!(a.iter().any(|x| x == flag), "{flag} missing from {a:?}");
    }
    // Neither `-L` (follow redirects) nor a proxy nor a netrc is asked for.
    for flag in [
        "-L",
        "--location",
        "--proxy",
        "-x",
        "-n",
        "--netrc",
        "--insecure",
        "-k",
    ] {
        assert!(!a.iter().any(|x| x == flag), "{flag} present in {a:?}");
    }
}

#[test]
fn the_pinned_addresses_become_one_resolve_entry() {
    let inv = invocation(&request(None, None), &limits(), "/ca").unwrap();
    assert_eq!(
        value_after(&inv.args, "--resolve"),
        Some("idp.example.org:443:203.0.113.7")
    );
}

#[test]
fn limits_are_whole_seconds_of_at_least_one() {
    let l = FetchLimits {
        max_time: Duration::from_millis(1500),
        connect_timeout: Duration::from_millis(1),
        max_bytes: 77,
        kill_slack: Duration::ZERO,
    };
    let inv = invocation(&request(None, None), &l, "/ca").unwrap();
    assert_eq!(value_after(&inv.args, "--max-time"), Some("2"));
    assert_eq!(value_after(&inv.args, "--connect-timeout"), Some("1"));
    assert_eq!(value_after(&inv.args, "--max-filesize"), Some("77"));
    let zero = FetchLimits {
        max_time: Duration::ZERO,
        ..l
    };
    let inv = invocation(&request(None, None), &zero, "/ca").unwrap();
    assert_eq!(
        value_after(&inv.args, "--max-time"),
        Some("1"),
        "zero would mean no bound"
    );
}

#[test]
fn no_secret_is_in_the_argument_vector_and_both_are_in_the_configuration() {
    let inv = invocation(
        &request(Some(SECRET_FORM), Some(SECRET_AUTH)),
        &limits(),
        "/ca",
    )
    .unwrap();
    let argv = inv.args.join("\u{1}");
    for piece in [
        SECRET_AUTH,
        "SECRETCODE",
        "SECRETVERIFIER",
        "c2VjcmV0",
        "Authorization",
        "grant_type",
    ] {
        assert!(
            !argv.contains(piece),
            "{piece} found in the argument vector: {argv}"
        );
    }
    let config = String::from_utf8(inv.stdin.clone()).unwrap();
    assert_eq!(
        config,
        format!("header = \"Authorization: {SECRET_AUTH}\"\ndata-raw = \"{SECRET_FORM}\"\n")
    );
    // The invocation's Debug shows the arguments and the size of the configuration, not its content.
    let shown = format!("{inv:?}");
    assert!(
        !shown.contains("SECRETCODE") && !shown.contains(SECRET_AUTH),
        "{shown}"
    );
}

#[test]
fn a_get_without_secrets_sends_an_empty_configuration() {
    let inv = invocation(&request(None, None), &limits(), "/ca").unwrap();
    assert!(inv.stdin.is_empty());
}

#[test]
fn quotes_and_backslashes_are_escaped_and_controls_refused_by_field_name() {
    let inv = invocation(&request(None, Some("Bearer a\"b\\c")), &limits(), "/ca").unwrap();
    assert_eq!(
        String::from_utf8(inv.stdin).unwrap(),
        "header = \"Authorization: Bearer a\\\"b\\\\c\"\n"
    );
    // A newline would end the line and let a value add a parameter of its own.
    let smuggled = "Bearer x\"\nurl = \"https://evil.example.net/";
    let err = invocation(&request(None, Some(smuggled)), &limits(), "/ca").unwrap_err();
    assert_eq!(
        err,
        CurlError::InvalidRequest(
            "the authorization header contains a control character".to_owned()
        )
    );
    assert!(!err.to_string().contains("evil"));
    let err = invocation(&request(Some("a=1\r\nb=2"), None), &limits(), "/ca").unwrap_err();
    assert_eq!(
        err,
        CurlError::InvalidRequest("the form body contains a control character".to_owned())
    );
    let long = "x".repeat(MAX_SECRET_FIELD + 1);
    assert!(invocation(&request(Some(&long), None), &limits(), "/ca").is_err());
}

#[test]
fn a_disagreeing_target_is_refused_before_any_argument_is_built() {
    let mut r = request(None, None);
    r.target.url = "http://idp.example.org/token".to_owned();
    assert!(matches!(
        invocation(&r, &limits(), "/ca"),
        Err(CurlError::InvalidRequest(_))
    ));
}
