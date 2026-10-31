//! The server attaches the peer of each connection to its requests (`ConnectInfo`), which the sign-in throttle
//! counts. The route is registered here, in a test of the listener; no page is.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::io::{Read, Write};
use std::net::TcpStream;

use axum::Router;
use axum::extract::ConnectInfo;
use axum::routing::get;

use std::net::SocketAddr;

use inklkunn_web::WebConfig;
use inklkunn_web::serve::{Shutdown, run_blocking};

/// The server hands every request the address of the connection it came on, so a throttle can count it.
#[test]
fn a_request_carries_the_peer_that_opened_its_connection() {
    let site = Router::new().route(
        "/who",
        get(|ConnectInfo(peer): ConnectInfo<SocketAddr>| async move { peer.ip().to_string() }),
    );
    let config = WebConfig::new(
        "127.0.0.1:0",
        "http://localhost:8080",
        "http://localhost:8081",
    )
    .unwrap();
    let (shutdown, stop) = Shutdown::manual();
    let (tx, rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        run_blocking(&config, site, shutdown, move |addr| {
            tx.send(addr).unwrap();
        })
    });
    let addr = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("the server bound");
    let mut stream = TcpStream::connect(addr).unwrap();
    write!(
        stream,
        "GET /who HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(answer.ends_with("127.0.0.1"), "{answer}");
    stop.stop();
    server.join().unwrap().unwrap();
}
