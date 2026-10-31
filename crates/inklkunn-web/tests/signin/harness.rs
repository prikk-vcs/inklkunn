//! The real world the sign-in tests run in: a wiki, an authenticator on its store, the pages over both.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, header};
use inklkunn_auth::{
    Argon2idHasher, AuthConfig, AuthError, Authenticator, JoinPolicy, PasswordHasher,
};
use inklkunn_store::{Account, AccountId, AccountStore, Role, RoleGrant, RoleStore, Visibility};
use inklkunn_web::{AuthIdentity, Services};
use tower::ServiceExt as _;

use crate::common::{APP, Answer, NOW, World, page};

pub(crate) const PASSWORD: &str = "a password that is long enough";

/// A cheap real Argon2id that counts the passwords it checks.
pub(crate) struct Counting {
    inner: Argon2idHasher,
    pub(crate) verifies: AtomicUsize,
}

impl PasswordHasher for Counting {
    fn hash(&self, password: &str) -> Result<String, AuthError> {
        self.inner.hash(password)
    }

    fn is_verifiable(&self, stored: &str) -> bool {
        self.inner.is_verifiable(stored)
    }

    fn verify(&self, password: &str, stored: &str) -> bool {
        self.verifies.fetch_add(1, Ordering::SeqCst);
        self.inner.verify(password, stored)
    }
}

pub(crate) struct Real {
    pub(crate) w: World,
    pub(crate) auth: Arc<Authenticator>,
    pub(crate) hasher: Arc<Counting>,
}

pub(crate) fn config() -> AuthConfig {
    AuthConfig {
        hash_permits: 4,
        gate_wait: std::time::Duration::from_millis(500),
        ..AuthConfig::default()
    }
}

pub(crate) fn joining(policy: JoinPolicy) -> AuthConfig {
    AuthConfig {
        join: policy,
        ..config()
    }
}

/// A public wiki `garden` and, beside it, the real sign-in.
pub(crate) fn real(config: AuthConfig) -> Real {
    real_with(config, Visibility::Public)
}

pub(crate) fn real_with(config: AuthConfig, visibility: Visibility) -> Real {
    real_built(config, visibility, |auth| auth)
}

/// As [`real_with`], with the authenticator passed through `finish` (to switch a provider on).
pub(crate) fn real_built(
    config: AuthConfig,
    visibility: Visibility,
    finish: impl FnOnce(Authenticator) -> Authenticator,
) -> Real {
    let mut w = World::with_visibility(
        "garden",
        "The Garden",
        visibility,
        &[(
            "pages/Home.md",
            &page(&["title: Home"], "Welcome to the garden.\n"),
        )],
    );
    let hasher = Arc::new(Counting {
        inner: Argon2idHasher::with_cost(64, 1, 1).unwrap(),
        verifies: AtomicUsize::new(0),
    });
    let auth = Arc::new(finish(
        Authenticator::with_hasher(w.store.clone(), config, hasher.clone())
            .unwrap()
            .with_audit(w.audit.clone()),
    ));
    let log = w.services.log.clone();
    let identity = AuthIdentity::new(auth.clone(), w.store.clone(), || NOW, log);
    w.services = Arc::new(Services {
        identity: Arc::new(identity),
        auth: Some(auth.clone()),
        ..(*w.services).clone()
    });
    Real { w, auth, hasher }
}

impl Real {
    /// An account that signs in with `login` and `PASSWORD`, holding `role` in the wiki.
    pub(crate) fn person(&self, login: &str, name: &str, role: Option<Role>) -> AccountId {
        let account = self
            .auth
            .register_local(login, name, PASSWORD, NOW - 90 * 86_400)
            .unwrap();
        if let Some(role) = role {
            self.w
                .store
                .grant(RoleGrant {
                    account: account.id.clone(),
                    wiki: self.w.wiki.id.clone(),
                    role,
                    granted_by: None,
                    granted_at: NOW,
                })
                .unwrap();
        }
        account.id
    }

    pub(crate) fn account(&self, id: &AccountId) -> Account {
        self.w.store.account(id).unwrap().unwrap()
    }

    /// One request to the application origin. `headers` are as given; a form's body is `body`.
    pub(crate) async fn call(
        &self,
        method: &str,
        uri: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Answer {
        self.call_from(None, method, uri, headers, body).await
    }

    /// The same from the connection of `peer`.
    pub(crate) async fn call_from(
        &self,
        peer: Option<SocketAddr>,
        method: &str,
        uri: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> Answer {
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::HOST, APP);
        if method == "POST" && !headers.iter().any(|(k, _)| *k == "content-type") {
            req = req.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
        }
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let mut req = req.body(Body::from(body.to_owned())).unwrap();
        if let Some(peer) = peer {
            req.extensions_mut().insert(ConnectInfo(peer));
        }
        let res = crate::common::site(&self.w).oneshot(req).await.unwrap();
        let (parts, body) = res.into_parts();
        Answer {
            status: parts.status,
            headers: parts.headers,
            bytes: axum::body::to_bytes(body, 1 << 24).await.unwrap().to_vec(),
        }
    }

    pub(crate) async fn get(&self, uri: &str, headers: &[(&str, &str)]) -> Answer {
        self.call("GET", uri, headers, "").await
    }
}

pub(crate) const SAME_SITE: (&str, &str) = ("sec-fetch-site", "same-origin");

/// A form body.
pub(crate) fn form(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                inklkunn_markup::percent_encode(k),
                inklkunn_markup::percent_encode(v)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Every `Set-Cookie` of an answer.
pub(crate) fn set_cookies(a: &Answer) -> Vec<String> {
    a.headers
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect()
}

/// The `Set-Cookie` line that sets cookie `name`.
pub(crate) fn cookie_line(a: &Answer, name: &str) -> Option<String> {
    set_cookies(a)
        .into_iter()
        .find(|c| c.starts_with(&format!("{name}=")))
}

/// The value cookie `name` is set to.
pub(crate) fn cookie_value(a: &Answer, name: &str) -> Option<String> {
    cookie_line(a, name).and_then(|c| {
        c.split(';')
            .next()
            .and_then(|nv| nv.split_once('='))
            .map(|(_, v)| v.to_owned())
    })
}

/// The value of the hidden field `csrf` of the first form that carries one.
pub(crate) fn csrf_field(html: &str) -> Option<String> {
    let at = html.find("name=\"csrf\" value=\"")? + "name=\"csrf\" value=\"".len();
    let rest = html.get(at..)?;
    rest.split('"').next().map(str::to_owned)
}

/// A person signed in through the pages: the `Cookie` header to send and the session's CSRF token.
pub(crate) struct Session {
    pub(crate) cookie: String,
    pub(crate) csrf: String,
}

impl Session {
    /// The headers of a same-site form post that presents this session and its token in the header.
    pub(crate) fn headers(&self) -> Vec<(&str, &str)> {
        vec![
            ("cookie", self.cookie.as_str()),
            ("sec-fetch-site", "same-origin"),
            (inklkunn_web::CSRF_HEADER, self.csrf.as_str()),
        ]
    }
}

impl Real {
    /// Sign in through the pages, as a browser does.
    pub(crate) async fn sign_in(&self, login: &str) -> Session {
        let page = self.get("/-/sign-in", &[]).await;
        assert_eq!(page.status, 200, "{}", page.body());
        let token = cookie_value(&page, "__Host-inklkunn-csrf").expect("the double-submit cookie");
        let body = form(&[
            ("csrf", &csrf_field(&page.body()).expect("the field")),
            ("login", login),
            ("password", PASSWORD),
            ("next", "/"),
        ]);
        let done = self
            .call(
                "POST",
                "/-/sign-in",
                &[
                    SAME_SITE,
                    ("cookie", &format!("__Host-inklkunn-csrf={token}")),
                ],
                &body,
            )
            .await;
        assert_eq!(done.status, 303, "{}", done.body());
        let secret = cookie_value(&done, "__Host-inklkunn-session").expect("a session cookie");
        let cookie = format!("__Host-inklkunn-session={secret}");
        let account = self.get("/-/account", &[("cookie", &cookie)]).await;
        assert_eq!(account.status, 200, "{}", account.body());
        let csrf = csrf_field(&account.body()).expect("the session's token is in the page");
        Session { cookie, csrf }
    }
}
