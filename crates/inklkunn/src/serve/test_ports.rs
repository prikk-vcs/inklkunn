#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Two free local ports for a test that starts a server (an app origin and a content origin).

/// Two free ports on `127.0.0.1` that differ.
///
/// Both listeners are bound before either is dropped, so the operating system cannot hand the same port out twice
/// (a server rightly refuses a content origin equal to the app origin).
pub(super) fn two_free_ports() -> (u16, u16) {
    let first = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let second = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    (
        first.local_addr().unwrap().port(),
        second.local_addr().unwrap().port(),
    )
}

#[cfg(test)]
mod tests;
