//! The bubblewrap vector `curl` runs in. Its shape is the driver's (`inklkunn-prikk`'s `sandbox`, whose
//! system view and `--die-with-parent` it copies) with the opposite network policy and nothing of the
//! wiki mounted:
//!
//! - `--unshare-all --share-net`: every namespace is new except the network, which `curl` needs. (Nothing
//!   else of the host's is reachable through it: the filesystem below is the whole filesystem.)
//! - **Mounts**: `/usr` read-only (with the usr-merge symlinks a dynamically linked program needs), the
//!   `curl` binary read-only (wherever it lives), the system CA bundle read-only at a fixed path, `/proc`
//!   and `/dev` minimal, and a fresh `tmpfs` for `/tmp`. **No repository, no key directory, no seed, no
//!   data directory, no home.**
//! - The environment is cleared: no `HOME` (so no `.curlrc`, no `.netrc`), no `http_proxy`, no
//!   `SSL_CERT_*`, no `CURL_*`. Only `PATH` is set.
//! - `--die-with-parent`, `--new-session`, `--cap-drop ALL`.
//!
//! The time and byte bounds are curl's own flags *and* the server's: the runner kills the child at a deadline
//! and reads at most the cap from its standard output.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the CA bundle sits inside the vector.
pub(crate) const VECTOR_CA_PATH: &str = "/etc/ssl/certs/ca-certificates.crt";

/// Where a distribution keeps the CA bundle, in the order tried.
const CA_CANDIDATES: &[&str] = &[
    "/etc/ssl/certs/ca-certificates.crt",
    "/etc/pki/tls/certs/ca-bundle.crt",
    "/etc/ssl/cert.pem",
];

/// The host's CA bundle, resolved to the file itself (so a symlink into `/etc/ca-certificates` mounts the
/// real file).
pub(crate) fn find_ca_bundle() -> Option<PathBuf> {
    CA_CANDIDATES
        .iter()
        .filter_map(|p| Path::new(p).canonicalize().ok())
        .find(|p| p.is_file())
}

/// `bwrap … -- <curl> <args>`: the confined command. `curl` and `ca_bundle` are absolute host paths.
pub(crate) fn bwrap_command(
    bwrap: &OsStr,
    curl: &Path,
    ca_bundle: &Path,
    curl_args: &[String],
) -> Command {
    let mut c = Command::new(bwrap);
    c.env_clear().env("PATH", "/usr/bin:/bin");
    // A read-only view of the system, with the usr-merge symlinks.
    c.arg("--ro-bind").arg("/usr").arg("/usr");
    for (target, link) in [
        ("usr/lib", "/lib"),
        ("usr/lib", "/lib64"),
        ("usr/bin", "/bin"),
        ("usr/bin", "/sbin"),
    ] {
        c.arg("--symlink").arg(target).arg(link);
    }
    c.arg("--proc").arg("/proc");
    c.arg("--dev").arg("/dev");
    // The fresh /tmp comes before the binds that may live under it (a stand-in curl in a test).
    c.arg("--tmpfs").arg("/tmp");
    c.arg("--ro-bind").arg(curl).arg(curl);
    c.arg("--ro-bind").arg(ca_bundle).arg(VECTOR_CA_PATH);
    c.arg("--chdir").arg("/tmp");
    c.arg("--unshare-all").arg("--share-net");
    c.arg("--die-with-parent");
    c.arg("--new-session");
    c.arg("--cap-drop").arg("ALL");
    c.arg("--clearenv");
    c.arg("--setenv").arg("PATH").arg("/usr/bin");
    c.arg("--").arg(curl).args(curl_args);
    c
}

/// `curl <args>` directly, with a cleared environment. **Tests only** (`test-knobs`): no confinement.
#[cfg(any(test, feature = "test-knobs"))]
pub(crate) fn unconfined_command(curl: &Path, curl_args: &[String]) -> Command {
    let mut c = Command::new(curl);
    c.env_clear().env("PATH", "/usr/bin:/bin").args(curl_args);
    c
}

#[cfg(test)]
mod tests;
