#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Two free local ports for a test that starts a server (an app origin and a content origin).

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// Two free ports on `127.0.0.1` that differ, and that no earlier call of this process returned.
///
/// Both listeners are bound before either is dropped, so the operating system cannot hand the same port out twice
/// (a server rightly refuses a content origin equal to the app origin). The ports are let go before the server binds
/// them, and the system may give a port it has just been given back to the next test's call: that test's server would
/// then answer a connection meant for the first, or the first would find its port in use. So a port is never handed out
/// twice by one process, however long ago, and a test run starts a few hundred servers at most.
pub(super) fn two_free_ports() -> (u16, u16) {
    static GIVEN: OnceLock<Mutex<HashSet<u16>>> = OnceLock::new();
    let given = GIVEN.get_or_init(Mutex::default);
    loop {
        let first = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let second = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let (a, b) = (
            first.local_addr().unwrap().port(),
            second.local_addr().unwrap().port(),
        );
        // Both or neither: a pair with one port already given is let go whole.
        let mut given = given.lock().unwrap();
        if !given.contains(&a) && !given.contains(&b) {
            given.insert(a);
            given.insert(b);
            return (a, b);
        }
    }
}

#[cfg(test)]
mod tests;
