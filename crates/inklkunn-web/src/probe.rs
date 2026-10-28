//! The liveness probe (`/-/health`): a fixed word for the operator's proxy. It serves no wiki data and no
//! page, so it has no language and no layout; the token is machine-facing text, not a sentence a person reads.

/// `GET /-/health`: `ok`.
pub(crate) async fn health() -> &'static str {
    "ok\n"
}
