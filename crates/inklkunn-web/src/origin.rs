//! One listener, two origins: a request goes to the router of the origin its `Host` names, and a request that
//! names neither is **misdirected** (421) rather than served by whichever router happens to be first. This
//! is what keeps a hostile `Host` header from reaching a handler that builds links from it, and what keeps
//! the content origin from ever answering an application route (and the reverse).

use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::response::Response;
use tower_service::Service;

use crate::config::Origin;
use crate::headers::{Surface, apply};

/// Hands each request to the router of the origin it was addressed to.
#[derive(Clone)]
pub struct Dispatch {
    app: Router,
    content: Router,
    app_origin: Origin,
    content_origin: Origin,
}

impl Dispatch {
    /// Route between the two.
    pub fn new(app: Router, content: Router, app_origin: Origin, content_origin: Origin) -> Self {
        Dispatch {
            app,
            content,
            app_origin,
            content_origin,
        }
    }

    fn misdirected(&self) -> Response {
        let mut res = Response::builder()
            .status(StatusCode::MISDIRECTED_REQUEST)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(Body::from("This server does not answer for that host.\n"))
            .unwrap_or_default();
        apply(
            &mut res,
            Surface::Application {
                https: self.app_origin.scheme() == crate::config::Scheme::Https,
            },
        );
        res
    }
}

type Pending = Pin<Box<dyn Future<Output = Result<Response, Infallible>> + Send>>;

impl Service<Request> for Dispatch {
    type Response = Response;
    type Error = Infallible;
    type Future = Pending;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Pending {
        let host = req
            .headers()
            .get(header::HOST)
            .and_then(|h| h.to_str().ok())
            .map(str::to_owned);
        let mut target = match host.as_deref() {
            Some(h) if self.app_origin.matches_host_header(h) => self.app.clone(),
            Some(h) if self.content_origin.matches_host_header(h) => self.content.clone(),
            _ => {
                let res = self.misdirected();
                return Box::pin(async move { Ok(res) });
            }
        };
        Box::pin(async move { target.call(req).await })
    }
}
