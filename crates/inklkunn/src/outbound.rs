//! Outbound HTTPS for OpenID Connect, wired (internal design IQ-3, AZ-1; handoff 002 corrections C8). The
//! three parts live in three layers and meet only here, in the binary:
//!
//! - the **egress guard** (`inklkunn-core`) decides whether a URL may be fetched and pins the addresses;
//! - the **fetcher** (`inklkunn-fetch`, a lower layer) runs a confined `curl` at exactly those addresses;
//! - the **seam** (`inklkunn-auth`'s [`HttpsFetcher`]) is what discovery, the key set and the token exchange
//!   call.
//!
//! The fetch crate cannot implement the seam itself: it is a lower layer and may not depend on a core crate
//! (the layering gate), so this small adapter does. [`provider`] builds the configured provider over it
//! (the OpenID Connect handoff's U3): the settings, the client secret read once from its file, a vector that
//! was probed, and the callback address the operator must register.

use std::path::Path;
use std::sync::Arc;

use inklkunn_auth::oidc::{FetchError, FetchRequest, HttpsFetcher, OidcConfig, OidcProvider};
use inklkunn_core::{EgressGuard, StdEgressGuard};
use inklkunn_fetch::{CurlError, CurlFetcher, CurlRequest, FetchTarget};

use crate::config::{OidcSettings, WebSettings};

/// The longest a client secret file may be: a secret is a line of text, and a file this big is not one.
const SECRET_FILE_MAX: u64 = 4096;

/// `HttpsFetcher` over the egress guard and the confined `curl`.
pub(crate) struct GuardedCurlFetcher {
    guard: Arc<dyn EgressGuard>,
    transport: CurlFetcher,
}

impl GuardedCurlFetcher {
    /// Over `guard` (which decides and pins) and the confined fetcher (which connects only to what was pinned).
    pub(crate) fn new(guard: Arc<dyn EgressGuard>, transport: CurlFetcher) -> Self {
        GuardedCurlFetcher { guard, transport }
    }
}

/// The client secret in `path`: an absolute path (a symbolic link is followed, since secret mounts often are
/// links) to a regular file of at most 4 KiB holding one line of text; trailing line breaks are dropped (an
/// editor or `echo` adds one), nothing else is. **Never printed**: every refusal names the setting and the
/// reason, not the content.
pub(crate) fn read_client_secret(path: &Path) -> Result<String, String> {
    const NAME: &str = "INKLKUNN_OIDC_CLIENT_SECRET_FILE";
    let refuse = |why: &str| Err(format!("{NAME} ({}): {why}", path.display()));
    let Ok(meta) = std::fs::metadata(path) else {
        return refuse("the file could not be read");
    };
    if !meta.is_file() {
        return refuse("it is not a regular file");
    }
    if meta.len() > SECRET_FILE_MAX {
        return refuse(
            "it is longer than 4096 bytes; it should hold the client secret and nothing else",
        );
    }
    let Ok(raw) = std::fs::read(path) else {
        return refuse("the file could not be read");
    };
    let Ok(text) = String::from_utf8(raw) else {
        return refuse("it is not UTF-8 text");
    };
    let secret = text.trim_end_matches(['\n', '\r']);
    if secret.is_empty() {
        return refuse("it is empty");
    }
    if secret.chars().any(char::is_control) {
        return refuse("it holds more than one line, or a control character");
    }
    Ok(secret.to_owned())
}

/// The address the provider sends the browser back to, which the operator registers at the provider **exactly**.
pub(crate) fn callback_address(app_origin: &str) -> String {
    format!(
        "{}{}",
        app_origin.trim_end_matches('/'),
        inklkunn_web::OIDC_CALLBACK_PATH
    )
}

/// The provider the operator configured, or `None` when none is (`INKLKUNN_OIDC_*`): checked before anything is
/// opened, so a deployment that cannot sign people in says why at start.
pub(crate) fn provider_if_configured(web: &WebSettings) -> Result<Option<OidcProvider>, String> {
    web.oidc
        .as_ref()
        .map(|oidc| provider(oidc, &web.app_origin))
        .transpose()
}

/// The authenticator, with the provider switched on when there is one.
pub(crate) fn attach(
    auth: inklkunn_auth::Authenticator,
    provider: Option<OidcProvider>,
) -> inklkunn_auth::Authenticator {
    match provider {
        Some(provider) => auth.with_oidc(provider),
        None => auth,
    }
}

/// The configured provider, over the real vector: `curl` in its own bubblewrap under `prlimit`, at the addresses
/// the egress guard pinned. Refuses, naming the setting or the tool, when the secret cannot be read or the
/// vector cannot run on this host, so a deployment that cannot sign people in says so at start and not at the
/// first sign-in.
pub(crate) fn provider(oidc: &OidcSettings, app_origin: &str) -> Result<OidcProvider, String> {
    let transport = CurlFetcher::new();
    let client_secret = oidc
        .client_secret_file
        .as_deref()
        .map(read_client_secret)
        .transpose()?;
    transport.probe().map_err(|e| {
        format!(
            "INKLKUNN_OIDC_ISSUER is set, and provider sign-in needs curl, the system CA bundle, prlimit and \
             bubblewrap on this host ({e})"
        )
    })?;
    Ok(provider_over(
        oidc,
        app_origin,
        client_secret,
        Arc::new(GuardedCurlFetcher::new(
            Arc::new(StdEgressGuard::new()),
            transport,
        )),
    ))
}

/// The provider over `fetcher` (a test gives one that stands in for the network).
pub(crate) fn provider_over(
    oidc: &OidcSettings,
    app_origin: &str,
    client_secret: Option<String>,
    fetcher: Arc<dyn HttpsFetcher>,
) -> OidcProvider {
    OidcProvider::new(
        OidcConfig {
            issuer: oidc.issuer.clone(),
            client_id: oidc.client_id.clone(),
            client_secret,
            redirect_uri: callback_address(app_origin),
            scopes: oidc.scopes.clone(),
        },
        fetcher,
    )
}

/// What the start says about provider sign-in: that it is on, with the callback address to register, and a
/// setting that cannot work (joining through a provider with none configured).
pub(crate) fn start_notes(web: &WebSettings) -> Vec<String> {
    let mut notes = Vec::new();
    match &web.oidc {
        Some(oidc) => notes.push(format!(
            "provider sign-in is on: {} (register {} as the redirect address at the provider)",
            oidc.issuer,
            callback_address(&web.app_origin)
        )),
        None if web.join == inklkunn_auth::JoinPolicy::Provider => notes.push(
            "warning: INKLKUNN_JOIN=provider, but no provider is configured (INKLKUNN_OIDC_ISSUER), so nobody can join that way"
                .to_owned(),
        ),
        None => {}
    }
    notes
}

impl HttpsFetcher for GuardedCurlFetcher {
    fn fetch(&self, request: &FetchRequest) -> Result<Vec<u8>, FetchError> {
        let pinned = self
            .guard
            .check(&request.url)
            .map_err(|e| FetchError::Refused(e.to_string()))?;
        let target = FetchTarget {
            url: pinned.url,
            host: pinned.host,
            port: pinned.port,
            addrs: pinned.addrs.iter().map(|a| a.ip()).collect(),
        };
        self.transport
            .fetch(&CurlRequest {
                target,
                form: request.form.clone(),
                authorization: request.authorization.clone(),
            })
            .map_err(|e| match e {
                CurlError::TooLarge => FetchError::TooLarge,
                other => FetchError::Transport(other.to_string()),
            })
    }
}

#[cfg(test)]
mod tests;
