//! **A web-only entry is never served to a bearer token** (RFC 006 D-12; threat model C-14f). Moderation, restrictions,
//! protections, grants and the decision of an appeal are deliberate acts of a person at the site, "never scripted through the
//! API", and the API document says so. They are forms of the application origin, whose handlers take the principal the
//! CSRF layer decided on, and a token is judged by the token alone there (it needs no CSRF token: a browser does not send one by
//! itself). Left so, whoever holds an admin's token could do them by posting the form. So the route table marks them
//! ([`inklkunn_core::RouteEntry::web_only`]) and **this layer, put in front of each marked entry's handler once, where the router is
//! built, refuses a request that presents a bearer token**, whatever the token's scope, whatever the method, and whichever wiki
//! the address names. A handler does not repeat it, and a new web-only form cannot forget it: the route enumeration harness walks
//! the table and holds each marked entry to this refusal.
//!
//! * **What counts as presenting a token** is what the identity layer counts: any `Authorization` header that names the `Bearer`
//!   scheme, a good token or not, and a credential that cannot be a token too ([`crate::identity::presented`]). Another scheme
//!   (an operator's proxy may add Basic) is not ours and does not refuse a browser's session.
//! * **A request with both a token and a session cookie is a token request** (the one rule of the identity layer: a request that
//!   presents a token is judged by the token alone), and is refused here. The session alone, with the CSRF token the CSRF layer
//!   checks first, does the act as before.
//! * **The refusal looks at nothing but the header.** It does not ask whether the token is good, whose it is or what scope it has,
//!   and it runs before the wiki is looked up: the same answer for a token that is valid and one that is not, for a wiki that
//!   exists and one that does not. A token holder learns nothing from it that the route table does not already say.
//! * The answer is `403` and a page in the visitor's language (a script reads the status; a person who pastes a token into a
//!   form reads the sentence): this is done at the site, by a person who is signed in, and a token cannot do it; nothing was
//!   changed.

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;

use crate::catalog::{Key, T};
use crate::identity::{Presented, presented};
use crate::pages::{Page, answer, render_notice};

/// The layer: refuse a request that presents a bearer token, let any other through to the handler.
pub(crate) async fn refuse_a_token(req: Request, next: Next) -> Response {
    if presented(req.headers()) == Presented::Nothing {
        return next.run(req).await;
    }
    let page = Page::new(
        T::new(crate::lang::negotiate_from_headers(req.headers())),
        "/",
    )
    .without_switcher();
    answer(
        StatusCode::FORBIDDEN,
        &page,
        render_notice(
            &page,
            Key::TitleWebOnly,
            Key::WebOnlyHeading,
            Key::WebOnlyBody,
            None,
        ),
    )
}
