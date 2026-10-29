#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::sync::Mutex;

use super::InvalidReason as R;
use inklkunn_store::ExternalIdentity;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};

use super::*;

const ISSUER: &str = "https://idp.example";
const CLIENT: &str = "inklkunn-client";
const REDIRECT: &str = "https://wiki.example/-/oidc/callback";
const NOW: i64 = 1_800_000_000;

// Two throwaway P-256 keys made for these tests (PKCS#8, base64 of the DER) and their public JWK coordinates.
const KEY_1: &str = "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgopZeFiRGZXWvz9h4G2WRp6WFSymfcUW7Kavsub8xSMehRANCAATQ+QQEj43jJBGpA4RYiR3l6PnckdxwBXaTdqRaPA7mOrNmL/VGO0eeJS9V/25SRXPdRAdaGWs75OxYcjsHqSZV";
const X_1: &str = "0PkEBI-N4yQRqQOEWIkd5ej53JHccAV2k3akWjwO5jo";
const Y_1: &str = "s2Yv9UY7R54lL1X_blJFc91EB1oZazvk7FhyOwepJlU";
const KEY_2: &str = "MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgqq4VHMYEXnNMdetY9/WwBoQ0sI21rgMxz62AlhU5qm+hRANCAAS9R97wCSI7wFbNAawvsinQhde3WIu8Dno3EUGUD2FMOCjH15vN84XsBuY5gQExcrcof2IiAziLgYvsEJAUc8l2";
const X_2: &str = "vUfe8AkiO8BWzQGsL7Ip0IXXt1iLvA56NxFBlA9hTDg";
const Y_2: &str = "KMfXm83zhewG5jmBATFytyh_YiIDOIuBi-wQkBRzyXY";

/// A fetcher answering from a map and logging every request.
#[derive(Default)]
struct MockFetcher {
    routes: Mutex<HashMap<String, Vec<u8>>>,
    log: Mutex<Vec<FetchRequest>>,
}

impl MockFetcher {
    fn set(&self, url: &str, body: impl Into<Vec<u8>>) {
        self.routes
            .lock()
            .unwrap()
            .insert(url.to_owned(), body.into());
    }
    fn requests_to(&self, url: &str) -> Vec<FetchRequest> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.url == url)
            .cloned()
            .collect()
    }
}

impl HttpsFetcher for MockFetcher {
    fn fetch(&self, req: &FetchRequest) -> Result<Vec<u8>, FetchError> {
        self.log.lock().unwrap().push(req.clone());
        self.routes
            .lock()
            .unwrap()
            .get(&req.url)
            .cloned()
            .ok_or_else(|| FetchError::Transport(format!("no route for {}", req.url)))
    }
}

fn state_header(state: &str) -> String {
    format!("{STATE_COOKIE}={state}")
}

fn jwks(entries: &[(&str, &str, &str, Option<&str>)]) -> String {
    let keys: Vec<String> = entries
        .iter()
        .map(|(kid, x, y, alg)| {
            let alg = alg.map(|a| format!(r#","alg":"{a}""#)).unwrap_or_default();
            format!(
                r#"{{"kty":"EC","crv":"P-256","x":"{x}","y":"{y}","kid":"{kid}","use":"sig"{alg}}}"#
            )
        })
        .collect();
    format!(r#"{{"keys":[{}]}}"#, keys.join(","))
}

fn discovery_json(issuer: &str) -> String {
    format!(
        r#"{{"issuer":"{issuer}","authorization_endpoint":"https://idp.example/auth","token_endpoint":"https://idp.example/token","jwks_uri":"https://idp.example/jwks"}}"#
    )
}

fn config(secret: Option<&str>) -> OidcConfig {
    OidcConfig {
        issuer: ISSUER.into(),
        client_id: CLIENT.into(),
        client_secret: secret.map(str::to_owned),
        redirect_uri: REDIRECT.into(),
    }
}

fn idp() -> Arc<MockFetcher> {
    let f = Arc::new(MockFetcher::default());
    f.set(
        "https://idp.example/.well-known/openid-configuration",
        discovery_json(ISSUER),
    );
    f.set(
        "https://idp.example/jwks",
        jwks(&[("k1", X_1, Y_1, Some("ES256"))]),
    );
    f
}

fn provider(secret: Option<&str>) -> (OidcProvider, Arc<MockFetcher>) {
    let f = idp();
    (OidcProvider::new(config(secret), f.clone()), f)
}

fn sign_with(key: &str, kid: Option<&str>, alg: Algorithm, claims: &serde_json::Value) -> String {
    use base64ct::{Base64, Encoding as _};
    let der = Base64::decode_vec(key).unwrap();
    let mut header = Header::new(alg);
    header.kid = kid.map(str::to_owned);
    encode(&header, claims, &EncodingKey::from_ec_der(&der)).unwrap()
}

fn sign(claims: &serde_json::Value) -> String {
    sign_with(KEY_1, Some("k1"), Algorithm::ES256, claims)
}

fn claims(nonce: &str) -> serde_json::Value {
    serde_json::json!({
        "iss": ISSUER, "sub": "user-42", "aud": CLIENT,
        "exp": NOW + 300, "iat": NOW, "nonce": nonce, "email": "u@example.org"
    })
}

fn with(mut c: serde_json::Value, key: &str, value: serde_json::Value) -> serde_json::Value {
    if let Some(object) = c.as_object_mut() {
        object.insert(key.to_owned(), value);
    }
    c
}

fn without(mut c: serde_json::Value, key: &str) -> serde_json::Value {
    c.as_object_mut().unwrap().remove(key);
    c
}

/// Begin a login and answer the token endpoint with a token carrying `claims_for(nonce)`.
fn login_with(
    p: &OidcProvider,
    f: &MockFetcher,
    id_token: impl FnOnce(&str) -> String,
) -> (BeginSignIn, Result<VerifiedIdentity, OidcError>) {
    let b = p.begin_sign_in(NOW).unwrap();
    let (nonce, _) = p.peek_pending(&b.state).unwrap();
    f.set(
        "https://idp.example/token",
        format!(
            r#"{{"id_token":"{}","access_token":"x"}}"#,
            id_token(&nonce)
        ),
    );
    let result = p.finish_sign_in(&b.state, Some(&state_header(&b.state)), "the-code", NOW + 1);
    (b, result)
}

#[test]
fn begin_login_builds_a_pkce_url_keeps_the_secrets_and_asks_for_nothing_but_openid() {
    let (p, _) = provider(None);
    let b = p.begin_sign_in(NOW).unwrap();
    let url = &b.authorization_url;
    assert!(
        url.starts_with("https://idp.example/auth?response_type=code&client_id=inklkunn-client"),
        "{url}"
    );
    assert!(url.contains("&scope=openid&"), "{url}");
    assert!(url.contains("code_challenge_method=S256"), "{url}");
    assert!(url.contains(&format!("&state={}&", b.state)), "{url}");
    assert!(
        url.contains("redirect_uri=https%3A%2F%2Fwiki.example%2F-%2Foidc%2Fcallback"),
        "{url}"
    );
    let (nonce, verifier) = p.peek_pending(&b.state).unwrap();
    assert!(url.contains(&format!("nonce={nonce}")));
    assert!(
        !url.contains(&verifier),
        "the verifier never leaves the server"
    );
    // The challenge is the S256 of the verifier.
    let challenge = Base64UrlUnpadded::encode_string(&Sha256::digest(verifier.as_bytes()));
    assert!(url.contains(&format!("code_challenge={challenge}")));
    // The state cookie is __Host-, Secure, HttpOnly, and Lax so the provider's redirect carries it.
    let cookie = b.state_cookie();
    assert!(
        cookie.starts_with(&format!("__Host-inklkunn-oidc={}; ", b.state)),
        "{cookie}"
    );
    for part in [
        "Path=/",
        "Secure",
        "HttpOnly",
        "SameSite=Lax",
        "Max-Age=600",
    ] {
        assert!(cookie.split("; ").any(|p| p == part), "{part} in {cookie}");
    }
}

#[test]
fn the_whole_flow_verifies_a_real_signature_and_the_state_is_one_shot() {
    let (p, f) = provider(Some("s3cret/&"));
    let (b, result) = login_with(&p, &f, |nonce| sign(&claims(nonce)));
    let id = result.unwrap();
    assert_eq!(id.identity().issuer(), ISSUER);
    assert_eq!(id.identity().subject(), "user-42");
    assert_eq!(id.email_hint(), Some("u@example.org"));
    // The exchange carried the code, the PKCE verifier and the client's Basic credentials, percent-encoded.
    let sent = f.requests_to("https://idp.example/token");
    let sent = sent.first().unwrap();
    let form = sent.form.as_deref().unwrap();
    assert!(
        form.starts_with("grant_type=authorization_code&code=the-code&"),
        "{form}"
    );
    assert!(
        form.contains("&redirect_uri=https%3A%2F%2Fwiki.example%2F-%2Foidc%2Fcallback&"),
        "{form}"
    );
    assert!(form.contains("&code_verifier="), "{form}");
    assert_eq!(
        sent.authorization.as_deref(),
        Some(
            format!(
                "Basic {}",
                Base64::encode_string(b"inklkunn-client:s3cret%2F%26")
            )
            .as_str()
        )
    );
    // Redeeming the same state again finds nothing.
    assert_eq!(
        p.finish_sign_in(&b.state, Some(&state_header(&b.state)), "the-code", NOW + 2),
        Err(OidcError::State)
    );
}

#[test]
fn a_public_client_sends_no_secret() {
    let (p, f) = provider(None);
    login_with(&p, &f, |n| sign(&claims(n))).1.unwrap();
    let sent = f.requests_to("https://idp.example/token");
    assert_eq!(sent.first().unwrap().authorization, None);
}

#[test]
fn a_callback_the_browser_did_not_start_is_refused_and_does_not_spend_the_login() {
    let (p, f) = provider(None);
    let b = p.begin_sign_in(NOW).unwrap();
    let (nonce, _) = p.peek_pending(&b.state).unwrap();
    f.set(
        "https://idp.example/token",
        format!(r#"{{"id_token":"{}"}}"#, sign(&claims(&nonce))),
    );
    // An attacker replays a victim's state without the victim's cookie, or with their own, or with the cookie
    // twice (ambiguous), or under another name.
    let short = format!("{STATE_COOKIE}={}", &b.state[..10]);
    let twice = format!("{0}; {0}", state_header(&b.state));
    let other_name = format!("__Host-other={}", b.state);
    let empty = format!("{STATE_COOKIE}=");
    let attacker = state_header("attacker-state");
    let headers: [Option<&str>; 7] = [
        None,
        Some(&empty),
        Some(&attacker),
        Some(&short),
        Some(&twice),
        Some(&other_name),
        Some(""),
    ];
    for header in headers {
        assert_eq!(
            p.finish_sign_in(&b.state, header, "c", NOW + 1),
            Err(OidcError::State),
            "{header:?}"
        );
    }
    assert_eq!(
        p.finish_sign_in("", Some(&empty), "c", NOW + 1),
        Err(OidcError::State),
        "empty is not a match"
    );
    assert!(
        f.requests_to("https://idp.example/token").is_empty(),
        "no code was exchanged"
    );
    // The real browser can still finish.
    assert!(
        p.finish_sign_in(&b.state, Some(&state_header(&b.state)), "c", NOW + 2)
            .is_ok()
    );
}

#[test]
fn a_login_expires_and_an_unknown_state_is_refused() {
    let (p, _) = provider(None);
    let b = p.begin_sign_in(NOW).unwrap();
    assert_eq!(
        p.finish_sign_in(
            &b.state,
            Some(&state_header(&b.state)),
            "c",
            NOW + PENDING_TTL_SECS
        ),
        Err(OidcError::State)
    );
    assert_eq!(
        p.finish_sign_in(
            "never-issued",
            Some(&state_header("never-issued")),
            "c",
            NOW
        ),
        Err(OidcError::State)
    );
}

#[test]
fn a_wrong_nonce_audience_issuer_or_time_is_refused_and_each_says_which() {
    type Make = Box<dyn Fn(&str) -> serde_json::Value>;
    let cases: Vec<(&str, Make, R)> = vec![
        (
            "another nonce",
            Box::new(|_| claims("someone-else")),
            R::Nonce,
        ),
        (
            "no nonce",
            Box::new(|n| without(claims(n), "nonce")),
            R::Nonce,
        ),
        (
            "another audience",
            Box::new(|n| with(claims(n), "aud", "someone-else".into())),
            R::Audience,
        ),
        (
            "another issuer",
            Box::new(|n| with(claims(n), "iss", "https://evil.example".into())),
            R::Issuer,
        ),
        (
            "expired",
            Box::new(|n| with(claims(n), "exp", (NOW - 3600).into())),
            R::Expired,
        ),
        (
            "expires right now",
            Box::new(|n| with(claims(n), "exp", (NOW + 1 - 60).into())),
            R::Expired,
        ),
        (
            "not yet valid",
            Box::new(|n| with(claims(n), "nbf", (NOW + 3600).into())),
            R::NotYetValid,
        ),
        (
            "issued in the future",
            Box::new(|n| with(claims(n), "iat", (NOW + 3600).into())),
            R::IssuedInFuture,
        ),
        (
            "issued twenty minutes ago",
            Box::new(|n| with(claims(n), "iat", (NOW - 1200).into())),
            R::IssuedTooLongAgo,
        ),
        (
            "issued ninety-five years ago",
            Box::new(|n| with(claims(n), "iat", 0.into())),
            R::IssuedTooLongAgo,
        ),
        (
            "no issued-at",
            Box::new(|n| without(claims(n), "iat")),
            R::MissingClaim,
        ),
        (
            "no expiry",
            Box::new(|n| without(claims(n), "exp")),
            R::MissingClaim,
        ),
        (
            "no subject",
            Box::new(|n| without(claims(n), "sub")),
            R::MissingClaim,
        ),
        (
            "no issuer",
            Box::new(|n| without(claims(n), "iss")),
            R::MissingClaim,
        ),
        (
            "no audience",
            Box::new(|n| without(claims(n), "aud")),
            R::MissingClaim,
        ),
        (
            "an empty subject",
            Box::new(|n| with(claims(n), "sub", "".into())),
            R::Subject,
        ),
        (
            "a subject over 255 bytes",
            Box::new(|n| with(claims(n), "sub", "s".repeat(256).into())),
            R::Subject,
        ),
        (
            "two audiences and no azp",
            Box::new(|n| with(claims(n), "aud", serde_json::json!([CLIENT, "other"]))),
            R::AuthorizedParty,
        ),
        (
            "an azp that is not us",
            Box::new(|n| with(claims(n), "azp", "other".into())),
            R::AuthorizedParty,
        ),
    ];
    for (what, make, reason) in cases {
        let (p, f) = provider(None);
        let (_, result) = login_with(&p, &f, |n| sign(&make(n)));
        assert_eq!(result, Err(OidcError::Invalid(reason)), "{what}");
    }
}

#[test]
fn the_reason_is_for_the_log_and_says_nothing_the_token_said() {
    let shown = OidcError::Invalid(R::Nonce).to_string();
    assert!(shown.contains("nonce mismatch"), "{shown}");
    for reason in [
        R::Malformed,
        R::Algorithm,
        R::Signature,
        R::Issuer,
        R::Audience,
        R::Expired,
        R::Nonce,
    ] {
        assert!(!reason.as_str().is_empty() && reason.as_str().len() < 48);
    }
}

#[test]
fn a_key_published_for_encryption_does_not_verify_a_signature() {
    let (p, f) = provider(None);
    f.set(
        "https://idp.example/jwks",
        format!(
            r#"{{"keys":[{{"kty":"EC","crv":"P-256","x":"{X_1}","y":"{Y_1}","kid":"k1","use":"enc","alg":"ES256"}}]}}"#
        ),
    );
    let (_, result) = login_with(&p, &f, |n| sign(&claims(n)));
    assert_eq!(result, Err(OidcError::Invalid(R::KeyNotForSignatures)));
    // A key with no `use` at all is a signing key (RFC 7517 leaves it to the algorithm).
    let (p, f) = provider(None);
    f.set(
        "https://idp.example/jwks",
        format!(r#"{{"keys":[{{"kty":"EC","crv":"P-256","x":"{X_1}","y":"{Y_1}","kid":"k1"}}]}}"#),
    );
    assert!(login_with(&p, &f, |n| sign(&claims(n))).1.is_ok());
}

#[test]
fn several_audiences_are_fine_when_the_authorized_party_is_us_and_a_skew_is_tolerated() {
    let (p, f) = provider(None);
    let (_, result) = login_with(&p, &f, |n| {
        sign(&with(
            with(claims(n), "aud", serde_json::json!([CLIENT, "other"])),
            "azp",
            CLIENT.into(),
        ))
    });
    assert!(result.is_ok());
    // Expired 30 seconds ago, inside the 60-second skew.
    let (p, f) = provider(None);
    let (_, result) = login_with(&p, &f, |n| sign(&with(claims(n), "exp", (NOW - 30).into())));
    assert!(result.is_ok());
}

#[test]
fn a_token_signed_by_another_key_or_under_another_algorithm_is_refused() {
    // Signed with key 2 but naming key 1's kid.
    let (p, f) = provider(None);
    let (_, r) = login_with(&p, &f, |n| {
        sign_with(KEY_2, Some("k1"), Algorithm::ES256, &claims(n))
    });
    assert_eq!(
        r,
        Err(OidcError::Invalid(R::Signature)),
        "a signature by another key"
    );
    // A token with no kid.
    let (p, f) = provider(None);
    let (_, r) = login_with(&p, &f, |n| {
        sign_with(KEY_1, None, Algorithm::ES256, &claims(n))
    });
    assert_eq!(r, Err(OidcError::Invalid(R::MissingKeyId)), "no kid");
    // The key declares ES384; the token says ES256.
    let (p, f) = provider(None);
    f.set(
        "https://idp.example/jwks",
        jwks(&[("k1", X_1, Y_1, Some("ES384"))]),
    );
    let (_, r) = login_with(&p, &f, |n| sign(&claims(n)));
    assert_eq!(
        r,
        Err(OidcError::Invalid(R::KeyAlgorithmMismatch)),
        "an algorithm the key did not declare"
    );
    // HMAC with a made-up secret: the algorithm-confusion attack. It must never reach a signature check.
    let (p, f) = provider(None);
    let (_, r) = login_with(&p, &f, |n| {
        let mut h = Header::new(Algorithm::HS256);
        h.kid = Some("k1".into());
        encode(
            &h,
            &claims(n),
            &EncodingKey::from_secret(b"the public key as a secret"),
        )
        .unwrap()
    });
    assert_eq!(
        r,
        Err(OidcError::Invalid(R::Algorithm)),
        "HS256 is not an allowed algorithm"
    );
    // `alg: none`, written by hand.
    let (p, f) = provider(None);
    let (_, r) = login_with(&p, &f, |n| {
        let b = |s: &str| Base64UrlUnpadded::encode_string(s.as_bytes());
        format!(
            "{}.{}.",
            b(r#"{"alg":"none","kid":"k1"}"#),
            b(&claims(n).to_string())
        )
    });
    assert_eq!(
        r,
        Err(OidcError::Invalid(R::Malformed)),
        "no signature at all"
    );
    // Garbage.
    let (p, f) = provider(None);
    let (_, r) = login_with(&p, &f, |_| "not.a.jwt".to_owned());
    assert_eq!(r, Err(OidcError::Invalid(R::Malformed)));
}

#[test]
fn an_unknown_kid_refetches_the_keys_but_not_more_than_once_a_minute() {
    let (p, f) = provider(None);
    let jwks_url = "https://idp.example/jwks";
    let token =
        |kid: &str, nonce: &str| sign_with(KEY_2, Some(kid), Algorithm::ES256, &claims(nonce));
    let v = |kid: &str, nonce: &str, now: i64| {
        let b = p.begin_sign_in(now).unwrap();
        let (n, _) = p.peek_pending(&b.state).unwrap();
        let _ = nonce;
        f.set(
            "https://idp.example/token",
            format!(r#"{{"id_token":"{}"}}"#, token(kid, &n)),
        );
        p.finish_sign_in(&b.state, Some(&state_header(&b.state)), "c", now)
    };
    // The first verification fetches the key set once (k1 only); k2 is unknown.
    assert_eq!(v("k2", "", NOW), Err(OidcError::Invalid(R::UnknownKey)));
    assert_eq!(f.requests_to(jwks_url).len(), 1);
    // An attacker sends tokens with invented kids: no further fetch inside the minute.
    for i in 0..20 {
        assert_eq!(
            v(&format!("invented-{i}"), "", NOW + 30),
            Err(OidcError::Invalid(R::UnknownKey))
        );
    }
    assert_eq!(
        f.requests_to(jwks_url).len(),
        1,
        "invented kids do not make outbound requests"
    );
    // The provider rotates to k2; a minute later the unknown kid is worth one refetch, and it succeeds.
    f.set(jwks_url, jwks(&[("k2", X_2, Y_2, None)]));
    assert!(v("k2", "", NOW + 61).is_ok());
    assert_eq!(f.requests_to(jwks_url).len(), 2);
    // A known kid is served from the cache.
    assert!(v("k2", "", NOW + 62).is_ok());
    assert_eq!(f.requests_to(jwks_url).len(), 2);
}

#[test]
fn a_discovery_document_for_another_issuer_or_with_a_plain_http_endpoint_is_refused() {
    let f = Arc::new(MockFetcher::default());
    f.set(
        "https://idp.example/.well-known/openid-configuration",
        discovery_json("https://other.example"),
    );
    let p = OidcProvider::new(config(None), f);
    assert!(
        matches!(p.begin_sign_in(NOW), Err(OidcError::Provider(_))),
        "another issuer"
    );

    for (what, doc) in [
        (
            "http authorization endpoint",
            r#"{"issuer":"https://idp.example","authorization_endpoint":"http://idp.example/auth","token_endpoint":"https://idp.example/token","jwks_uri":"https://idp.example/jwks"}"#,
        ),
        (
            "http token endpoint",
            r#"{"issuer":"https://idp.example","authorization_endpoint":"https://idp.example/auth","token_endpoint":"http://idp.example/token","jwks_uri":"https://idp.example/jwks"}"#,
        ),
        (
            "http keys",
            r#"{"issuer":"https://idp.example","authorization_endpoint":"https://idp.example/auth","token_endpoint":"https://idp.example/token","jwks_uri":"http://idp.example/jwks"}"#,
        ),
        (
            "not a URL",
            r#"{"issuer":"https://idp.example","authorization_endpoint":"a","token_endpoint":"t","jwks_uri":"j"}"#,
        ),
        (
            "no host",
            r#"{"issuer":"https://idp.example","authorization_endpoint":"https:///auth","token_endpoint":"https://idp.example/token","jwks_uri":"https://idp.example/jwks"}"#,
        ),
    ] {
        let f = Arc::new(MockFetcher::default());
        f.set("https://idp.example/.well-known/openid-configuration", doc);
        let p = OidcProvider::new(config(None), f);
        assert!(
            matches!(p.begin_sign_in(NOW), Err(OidcError::Provider(_))),
            "{what}"
        );
    }
}

#[test]
fn a_provider_that_cannot_be_reached_or_answers_rubbish_is_a_provider_error() {
    // Nothing configured at the fetcher: the transport fails.
    let p = OidcProvider::new(config(None), Arc::new(MockFetcher::default()));
    assert!(matches!(p.begin_sign_in(NOW), Err(OidcError::Provider(_))));
    // A token endpoint that answers something that is not JSON, or JSON without an id_token.
    for body in ["<html>", r#"{"access_token":"x"}"#] {
        let (p, f) = provider(None);
        let b = p.begin_sign_in(NOW).unwrap();
        f.set("https://idp.example/token", body);
        assert!(
            matches!(
                p.finish_sign_in(&b.state, Some(&state_header(&b.state)), "c", NOW + 1),
                Err(OidcError::Provider(_))
            ),
            "{body}"
        );
    }
    // A response over the cap is refused before it is parsed.
    let (p, f) = provider(None);
    f.set(
        "https://idp.example/jwks",
        vec![b' '; MAX_RESPONSE_BYTES + 1],
    );
    let b = p.begin_sign_in(NOW).unwrap();
    let (n, _) = p.peek_pending(&b.state).unwrap();
    f.set(
        "https://idp.example/token",
        format!(r#"{{"id_token":"{}"}}"#, sign(&claims(&n))),
    );
    assert!(matches!(
        p.finish_sign_in(&b.state, Some(&state_header(&b.state)), "c", NOW + 1),
        Err(OidcError::Provider(_))
    ));
}

/// A verifier that records what it was asked and says what it is told.
struct StubVerifier {
    seen: Mutex<Vec<(String, String, i64)>>,
    answer: Result<VerifiedIdentity, OidcError>,
}

impl TokenVerifier for StubVerifier {
    fn verify(
        &self,
        id_token: &str,
        expected_nonce: &str,
        now: i64,
    ) -> Result<VerifiedIdentity, OidcError> {
        self.seen
            .lock()
            .unwrap()
            .push((id_token.to_owned(), expected_nonce.to_owned(), now));
        self.answer.clone()
    }
}

#[test]
fn the_flow_hands_the_verifier_the_token_the_nonce_it_made_and_the_time() {
    let identity = VerifiedIdentity::for_test(ExternalIdentity::new(ISSUER, "u1").unwrap(), None);
    let stub = Arc::new(StubVerifier {
        seen: Mutex::default(),
        answer: Ok(identity.clone()),
    });
    let p = OidcProvider::with_verifier(config(None), idp(), stub.clone());
    let b = p.begin_sign_in(NOW).unwrap();
    let (nonce, _) = p.peek_pending(&b.state).unwrap();
    // The stub fetcher's token endpoint.
    let f = idp();
    f.set(
        "https://idp.example/token",
        r#"{"id_token":"the.id.token"}"#,
    );
    let p = OidcProvider::with_verifier(config(None), f, stub.clone());
    let b2 = p.begin_sign_in(NOW).unwrap();
    let (nonce2, _) = p.peek_pending(&b2.state).unwrap();
    assert_ne!(nonce, nonce2);
    assert_eq!(
        p.finish_sign_in(&b2.state, Some(&state_header(&b2.state)), "c", NOW + 5),
        Ok(identity)
    );
    assert_eq!(
        stub.seen.lock().unwrap().as_slice(),
        [("the.id.token".to_owned(), nonce2, NOW + 5)]
    );
}

#[test]
fn a_verifier_that_refuses_refuses_the_login() {
    let f = idp();
    f.set("https://idp.example/token", r#"{"id_token":"x"}"#);
    let stub = Arc::new(StubVerifier {
        seen: Mutex::default(),
        answer: Err(OidcError::Invalid(R::Signature)),
    });
    let p = OidcProvider::with_verifier(config(None), f, stub);
    let b = p.begin_sign_in(NOW).unwrap();
    assert_eq!(
        p.finish_sign_in(&b.state, Some(&state_header(&b.state)), "c", NOW),
        Err(OidcError::Invalid(R::Signature))
    );
}

#[test]
fn pending_logins_are_bounded_and_the_oldest_go_first() {
    let pending = PendingLogins::default();
    // Twenty logins to a second, all inside the ten minutes a login may take.
    for i in 0..10_050_i64 {
        pending.insert(format!("s{i}"), "n".into(), "v".into(), NOW + i / 20);
    }
    assert_eq!(pending.len(), 10_000);
    assert!(
        (0..40).all(|i| pending.peek(&format!("s{i}")).is_none()),
        "the oldest were dropped"
    );
    assert!(pending.peek("s10049").is_some() && pending.peek("s9000").is_some());
    // Old ones are swept when anything new arrives.
    pending.insert("late".into(), "n".into(), "v".into(), NOW + 100_000);
    assert_eq!(pending.len(), 1);
}

#[test]
fn the_configuration_never_shows_its_secret() {
    let shown = format!("{:?}", config(Some("hunter2-hunter2")));
    assert!(!shown.contains("hunter2"), "{shown}");
}

#[test]
fn percent_encoding_leaves_only_unreserved_characters() {
    assert_eq!(pct("aZ09-_.~"), "aZ09-_.~");
    assert_eq!(pct("a b&c=d/é"), "a%20b%26c%3Dd%2F%C3%A9");
}

/// The cookie attributes of the login-state cookie and of its removal (handoff 002 C9): `__Host-` rules
/// (`Secure`, `Path=/`, no `Domain`), `HttpOnly`, `SameSite=Lax` (the provider's cross-site redirect must
/// carry it back; `Strict` would not be sent), short-lived, and removed by a cookie of the same shape.
#[test]
fn the_state_cookie_is_lax_short_lived_and_host_prefixed_and_its_removal_matches_it() {
    let (p, _f) = provider(None);
    let b = p.begin_sign_in(1_000).unwrap();
    let set = b.state_cookie();
    let attrs = |c: &str| -> Vec<String> { c.split("; ").skip(1).map(str::to_owned).collect() };
    assert_eq!(
        attrs(&set),
        [
            "Path=/",
            "Secure",
            "HttpOnly",
            "SameSite=Lax",
            "Max-Age=600"
        ]
    );
    assert!(!set.contains("Strict"), "{set}");
    assert!(
        !set.contains("Domain"),
        "a __Host- cookie has no Domain: {set}"
    );
    assert_eq!(PENDING_TTL_SECS, 600, "short-lived: ten minutes");
    let clear = clear_state_cookie();
    assert!(clear.starts_with("__Host-inklkunn-oidc=; "), "{clear}");
    assert_eq!(
        attrs(&clear),
        ["Path=/", "Secure", "HttpOnly", "SameSite=Lax", "Max-Age=0"]
    );
}
