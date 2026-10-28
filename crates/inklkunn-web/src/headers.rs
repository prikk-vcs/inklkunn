//! The headers every response carries (RFC 003 D-5; threat model T-6, T-10; WEB-09), set in **one** place so
//! no handler can forget them. A handler that needs a different value for one of them sets it first; this
//! layer fills in only what is absent. `Content-Security-Policy` and the other security headers are **never**
//! overridden by a handler: they are set here, always, to the origin's own policy.

use axum::extract::Request;
use axum::http::{HeaderName, HeaderValue, header};
use axum::middleware::Next;
use axum::response::Response;

/// Which kind of origin a response leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// Pages, forms and the API.
    Application {
        /// Whether the origin is `https` (then `Strict-Transport-Security` is sent).
        https: bool,
    },
    /// Wiki-authored bytes as inert attachments.
    Content {
        /// Whether the origin is `https`.
        https: bool,
    },
}

/// The page policy: nothing is allowed that is not named. **No `script-src`**: no script runs on any page
/// (the application works without one); the one stylesheet comes from the application origin itself; images
/// only from it; forms only to it; nothing may frame a page.
pub const APPLICATION_CSP: &str = "default-src 'none'; style-src 'self'; img-src 'self'; form-action 'self'; \
base-uri 'none'; frame-ancestors 'none'";

/// The content origin's policy: no resource of any kind, and the document is sandboxed, so even a hostile
/// file opened directly has no origin, no script, no form and no frame.
pub const CONTENT_CSP: &str =
    "default-src 'none'; sandbox; base-uri 'none'; frame-ancestors 'none'";

const PERMISSIONS: &str = "accelerometer=(), camera=(), geolocation=(), gyroscope=(), magnetometer=(), \
microphone=(), payment=(), usb=(), interest-cohort=()";

fn set(res: &mut Response, name: HeaderName, value: &'static str) {
    res.headers_mut()
        .insert(name, HeaderValue::from_static(value));
}

fn fill(res: &mut Response, name: HeaderName, value: &'static str) {
    if !res.headers().contains_key(&name) {
        set(res, name, value);
    }
}

/// Put the headers on a finished response.
pub fn apply(res: &mut Response, surface: Surface) {
    let (csp, https) = match surface {
        Surface::Application { https } => (APPLICATION_CSP, https),
        Surface::Content { https } => (CONTENT_CSP, https),
    };
    // Always ours, whatever the handler said.
    set(res, header::CONTENT_SECURITY_POLICY, csp);
    set(res, header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    set(res, header::REFERRER_POLICY, "same-origin");
    set(
        res,
        HeaderName::from_static("permissions-policy"),
        PERMISSIONS,
    );
    set(res, header::X_FRAME_OPTIONS, "DENY");
    set(
        res,
        HeaderName::from_static("cross-origin-opener-policy"),
        "same-origin",
    );
    set(
        res,
        HeaderName::from_static("cross-origin-resource-policy"),
        match surface {
            Surface::Application { .. } => "same-origin",
            // Attachments are embedded by the application origin's own pages (an image in a page), never
            // by a third party: `same-site` covers a content origin under the application's registrable name.
            Surface::Content { .. } => "cross-origin",
        },
    );
    if https {
        set(res, header::STRICT_TRANSPORT_SECURITY, "max-age=31536000");
    }
    // Dynamic by default: never stored by a shared cache, revalidated by a private one, and different for
    // different cookies and languages. A handler (the stylesheet, an attachment) says otherwise itself.
    fill(res, header::CACHE_CONTROL, "private, no-cache");
    fill(res, header::VARY, "Accept-Language, Cookie");
}

/// The layer: run the handler, then [`apply`].
pub async fn layer(surface: Surface, req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    apply(&mut res, surface);
    res
}

#[cfg(test)]
mod tests;
