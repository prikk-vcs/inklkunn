//! What the provider publishes, fetched through the seam and kept: the discovery document and the key set.
//! Both are refetched on a schedule, and the key set also when a token names a key we do not have — **at
//! most once a minute**, so a flood of tokens with invented `kid`s cannot turn this service into a source
//! of outbound requests.

use std::sync::{Arc, Mutex, PoisonError};

use jsonwebtoken::jwk::JwkSet;
use serde::de::DeserializeOwned;

use super::OidcError;
use super::fetch::{FetchRequest, HttpsFetcher, MAX_RESPONSE_BYTES};

/// The discovery document is refetched after a day.
const DISCOVERY_TTL_SECS: i64 = 86_400;
/// The key set is refetched after an hour, or at once for an unknown `kid` if the last fetch is older than
/// this many seconds.
const JWKS_TTL_SECS: i64 = 3_600;
pub(super) const JWKS_MIN_REFRESH_SECS: i64 = 60;

/// The endpoints a provider publishes at `<issuer>/.well-known/openid-configuration`.
#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct Discovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
}

/// After a failed discovery the provider is not asked again for this many seconds: a provider that is down
/// must not be hit, and a blocking thread pinned, once for every person who tries to sign in.
const DISCOVERY_RETRY_SECS: i64 = 5;

#[derive(Default)]
struct Cache {
    discovery: Option<(Discovery, i64)>,
    discovery_failed_at: Option<i64>,
    jwks: Option<(Arc<JwkSet>, i64)>,
}

/// One provider's published state.
pub(super) struct Remote {
    issuer: String,
    fetcher: Arc<dyn HttpsFetcher>,
    cache: Mutex<Cache>,
}

fn fetch_json<T: DeserializeOwned>(
    fetcher: &dyn HttpsFetcher,
    request: &FetchRequest,
) -> Result<T, OidcError> {
    let body = fetcher
        .fetch(request)
        .map_err(|e| OidcError::Provider(e.to_string()))?;
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(OidcError::Provider("the response was too large".to_owned()));
    }
    serde_json::from_slice(&body)
        .map_err(|_| OidcError::Provider("the response was not the expected JSON".to_owned()))
}

fn is_https(url: &str) -> bool {
    url.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty() && !rest.starts_with('/') && !url.contains(char::is_whitespace)
    })
}

impl Remote {
    pub(super) fn new(issuer: String, fetcher: Arc<dyn HttpsFetcher>) -> Self {
        Remote {
            issuer,
            fetcher,
            cache: Mutex::new(Cache::default()),
        }
    }

    pub(super) fn issuer(&self) -> &str {
        &self.issuer
    }

    /// A request that is not a JSON read (the token exchange), bounded like the others.
    pub(super) fn fetch_raw(&self, request: &FetchRequest) -> Result<Vec<u8>, OidcError> {
        let body = self
            .fetcher
            .fetch(request)
            .map_err(|e| OidcError::Provider(e.to_string()))?;
        if body.len() > MAX_RESPONSE_BYTES {
            return Err(OidcError::Provider("the response was too large".to_owned()));
        }
        Ok(body)
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, Cache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn discovery(&self, now: i64) -> Result<Discovery, OidcError> {
        {
            let cache = self.cache();
            if let Some((d, at)) = &cache.discovery
                && now.saturating_sub(*at) < DISCOVERY_TTL_SECS
            {
                return Ok(d.clone());
            }
            if let Some(failed) = cache.discovery_failed_at
                && now.saturating_sub(failed) < DISCOVERY_RETRY_SECS
            {
                return Err(OidcError::Provider(
                    "the provider could not be reached a moment ago; not asking again yet"
                        .to_owned(),
                ));
            }
        }
        match self.fetch_discovery() {
            Ok(d) => {
                let mut cache = self.cache();
                cache.discovery = Some((d.clone(), now));
                cache.discovery_failed_at = None;
                Ok(d)
            }
            Err(e) => {
                self.cache().discovery_failed_at = Some(now);
                Err(e)
            }
        }
    }

    fn fetch_discovery(&self) -> Result<Discovery, OidcError> {
        let url = format!(
            "{}/.well-known/openid-configuration",
            self.issuer.trim_end_matches('/')
        );
        let d: Discovery = fetch_json(&*self.fetcher, &FetchRequest::get(url))?;
        // The document must describe the issuer we were configured with (RFC 8414 §3.3), and every endpoint
        // in it must be https: a document that sends the code or the keys over plain HTTP is refused.
        if d.issuer != self.issuer {
            return Err(OidcError::Provider(
                "the discovery document names another issuer".to_owned(),
            ));
        }
        if ![&d.authorization_endpoint, &d.token_endpoint, &d.jwks_uri]
            .into_iter()
            .all(|u| is_https(u))
        {
            return Err(OidcError::Provider(
                "a discovery endpoint is not an https URL".to_owned(),
            ));
        }
        Ok(d)
    }

    /// The provider's keys. `want` is the `kid` a token names: when the cached set lacks it, the set is
    /// refetched, but not more than once in [`JWKS_MIN_REFRESH_SECS`].
    pub(super) fn keys(&self, want: &str, now: i64) -> Result<Arc<JwkSet>, OidcError> {
        let cached = self.cache().jwks.clone();
        if let Some((set, at)) = &cached {
            let age = now.saturating_sub(*at);
            let has_it = set.find(want).is_some();
            if (has_it && age < JWKS_TTL_SECS) || (!has_it && age < JWKS_MIN_REFRESH_SECS) {
                return Ok(Arc::clone(set));
            }
        }
        let d = self.discovery(now)?;
        let set: Arc<JwkSet> =
            Arc::new(fetch_json(&*self.fetcher, &FetchRequest::get(d.jwks_uri))?);
        self.cache().jwks = Some((Arc::clone(&set), now));
        Ok(set)
    }
}
