//! Helpers the integration tests share: a scratch directory, stand-in scripts, a request.
#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use inklkunn_fetch::{CurlError, CurlFetcher, CurlRequest, FetchTarget};

pub const SECRET_AUTH: &str = "Basic c2VjcmV0LWNsaWVudC1pZDpzZWNyZXQtY2xpZW50LXNlY3JldA==";
pub const SECRET_FORM: &str =
    "grant_type=authorization_code&code=SECRETCODE&code_verifier=SECRETVERIFIER";
pub const SECRET_PIECES: &[&str] = &[
    "c2VjcmV0LWNsaWVudC1pZDpzZWNyZXQtY2xpZW50LXNlY3JldA==",
    "SECRETCODE",
    "SECRETVERIFIER",
    "authorization_code",
];

/// A scratch directory under the system temporary directory, removed on drop.
pub struct Dir(pub PathBuf);

impl Dir {
    pub fn new(tag: &str) -> Self {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let p =
            std::env::temp_dir().join(format!("inklkunn-fetch-{}-{n}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("scratch dir");
        Dir(p)
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    pub fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.path(name)).unwrap_or_default()
    }

    /// An executable script `name` in this directory. Its first lines record the arguments
    /// (`<name>.argv`) and the standard input (`<name>.stdin`) into this directory; `body` follows.
    pub fn script(&self, name: &str, body: &str) -> PathBuf {
        let rec = self.0.display();
        let text = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{rec}/{name}.argv'\ncat > '{rec}/{name}.stdin'\n{body}\n"
        );
        self.raw_script(name, &text, 0o755)
    }

    /// A file with exactly `text` and mode `mode`.
    pub fn raw_script(&self, name: &str, text: &str, mode: u32) -> PathBuf {
        let p = self.path(name);
        std::fs::write(&p, text).expect("write script");
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).expect("chmod");
        p
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A request to `host` (which the guard would have pinned to `ip`), with the secrets set or not.
pub fn request(host: &str, port: u16, ip: &str, with_secrets: bool) -> CurlRequest {
    let url = if port == 443 {
        format!("https://{host}/token")
    } else {
        format!("https://{host}:{port}/token")
    };
    CurlRequest {
        target: FetchTarget {
            url,
            host: host.to_owned(),
            port,
            addrs: vec![ip.parse().unwrap()],
        },
        form: with_secrets.then(|| SECRET_FORM.to_owned()),
        authorization: with_secrets.then(|| SECRET_AUTH.to_owned()),
    }
}

/// `fetch`, retrying while the kernel says the script we just wrote is still open for writing in a
/// concurrently forked child (`ETXTBSY`), which parallel tests that write scripts can provoke.
pub fn fetch(f: &CurlFetcher, r: &CurlRequest) -> Result<Vec<u8>, CurlError> {
    for _ in 0..50 {
        match f.fetch(r) {
            Err(CurlError::Spawn(e)) if e.contains("busy") => {
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
            other => return other,
        }
    }
    f.fetch(r)
}

/// Nothing secret appears in the error's text or `Debug`.
pub fn assert_no_secret(e: &CurlError) {
    let shown = format!("{e} || {e:?} || {e:#?}");
    for piece in SECRET_PIECES {
        assert!(!shown.contains(piece), "secret fragment {piece} in {shown}");
    }
}
