//! The server the real-browser run drives (handoff 002's F2; `rfcs/handoffs/002-identity-keys-and-signing/
//! evidence/c9-browser-run.py`): the real router and the real authenticator on a real port, with the stand-in
//! provider's server side in this process (discovery and the token endpoint; the ID token's signature is the
//! auth crate's own tests) and its browser side in the script (the page the browser is sent to). Ignored: it
//! serves until told to stop, so it runs only when the script starts it, with these variables set:
//!
//! * `INKLKUNN_BROWSER_APP_PORT`: the port the application origin (`http://localhost:<port>`) is served on;
//! * `INKLKUNN_BROWSER_IDP`: the provider's address as the browser reaches it (`https://idp.example:<port>`);
//! * `INKLKUNN_BROWSER_STOP_FILE`: a path; the server stops when a file appears there.
//!
//! It prints `LISTENING <port>` once it serves. Joining through the provider is open, so the first sign-in of
//! the person at the provider makes an account (the run checks both that and the later sign-ins).

use std::time::{Duration, Instant};

use inklkunn_auth::JoinPolicy;
use inklkunn_web::WebConfig;
use inklkunn_web::serve::{Shutdown, run_blocking};

use crate::provider::provider_at;

#[test]
#[ignore = "serves until told to stop; started by the real-browser run (evidence/c9-browser-run.py)"]
fn the_server_the_real_browser_run_drives() {
    let (Ok(port), Ok(idp), Ok(stop_file)) = (
        std::env::var("INKLKUNN_BROWSER_APP_PORT"),
        std::env::var("INKLKUNN_BROWSER_IDP"),
        std::env::var("INKLKUNN_BROWSER_STOP_FILE"),
    ) else {
        println!(
            "skipping: INKLKUNN_BROWSER_APP_PORT, INKLKUNN_BROWSER_IDP and INKLKUNN_BROWSER_STOP_FILE are not set"
        );
        return;
    };
    let (r, p) = provider_at(
        JoinPolicy::Provider,
        &idp,
        &format!("http://localhost:{port}/-/sign-in/oidc/callback"),
    );
    p.person("browser-person");
    let config = WebConfig::new(
        &format!("127.0.0.1:{port}"),
        &format!("http://localhost:{port}"),
        &format!("http://127.0.0.1:{port}"),
    )
    .unwrap();
    let site = inklkunn_web::routes::site_with(&config, Some(r.w.services.clone())).unwrap();
    let (shutdown, stop) = Shutdown::manual();
    let server = std::thread::spawn(move || {
        run_blocking(&config, site, shutdown, |addr| {
            println!("LISTENING {}", addr.port());
        })
    });
    let started = Instant::now();
    while !std::path::Path::new(&stop_file).exists() && started.elapsed() < Duration::from_secs(300)
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    stop.stop();
    server.join().unwrap().unwrap();
}
