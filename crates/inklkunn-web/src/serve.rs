//! Bind, serve, stop. The server speaks plain HTTP/1.1 behind the operator's TLS-terminating proxy, which also
//! owns connection limits and timeouts; it stops accepting on `SIGTERM` or `SIGINT` and lets requests in
//! flight finish. The async runtime lives here, so the binary needs neither tokio nor axum.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;

use axum::Router;

use crate::config::WebConfig;

/// Why serving ended badly.
#[derive(Debug)]
pub enum ServeError {
    /// The async runtime could not be started.
    Runtime(std::io::Error),
    /// The address could not be bound.
    Bind(std::io::Error),
    /// The server stopped on an I/O error.
    Io(std::io::Error),
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServeError::Runtime(e) => write!(f, "the async runtime could not be started: {e}"),
            ServeError::Bind(e) => write!(f, "the address could not be bound: {e}"),
            ServeError::Io(e) => write!(f, "the server stopped: {e}"),
        }
    }
}

impl std::error::Error for ServeError {}

/// When to stop.
pub struct Shutdown(Pin<Box<dyn Future<Output = ()> + Send>>);

/// Stops a server started with [`Shutdown::manual`].
pub struct StopHandle(Option<tokio::sync::oneshot::Sender<()>>);

impl StopHandle {
    /// Ask the server to stop accepting and finish what is in flight.
    pub fn stop(mut self) {
        if let Some(tx) = self.0.take() {
            let _ = tx.send(());
        }
    }
}

impl Shutdown {
    /// Stop on `SIGTERM` or `SIGINT` (what a service manager and a terminal send).
    pub fn on_signal() -> Self {
        Shutdown(Box::pin(termination()))
    }

    /// Stop when the handle says so (a test).
    pub fn manual() -> (Self, StopHandle) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        (
            Shutdown(Box::pin(async move {
                let _ = rx.await;
            })),
            StopHandle(Some(tx)),
        )
    }
}

async fn termination() {
    use tokio::signal::unix::{SignalKind, signal};
    let interrupt = tokio::signal::ctrl_c();
    match signal(SignalKind::terminate()) {
        Ok(mut term) => {
            tokio::select! {
                _ = interrupt => {}
                _ = term.recv() => {}
            }
        }
        // Without a SIGTERM handler the interrupt alone ends the server.
        Err(_) => {
            let _ = interrupt.await;
        }
    }
}

/// Bind the configured address and serve `site` until `shutdown`. `on_listening` is told the address that
/// was bound (it differs from the configured one only when the configured port was 0). Blocks.
pub fn run_blocking(
    config: &WebConfig,
    site: Router,
    shutdown: Shutdown,
    on_listening: impl FnOnce(SocketAddr),
) -> Result<(), ServeError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(ServeError::Runtime)?;
    runtime.block_on(async {
        let listener = tokio::net::TcpListener::bind(config.bind)
            .await
            .map_err(ServeError::Bind)?;
        if let Ok(addr) = listener.local_addr() {
            on_listening(addr);
        }
        axum::serve(listener, site)
            .with_graceful_shutdown(shutdown.0)
            .await
            .map_err(ServeError::Io)
    })
}
