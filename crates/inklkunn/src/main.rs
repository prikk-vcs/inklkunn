#![forbid(unsafe_code)]
//! The `inklkunn` server binary. M0 wires no server yet: the crates arrive task by task (RFC 001) and the
//! routes with RFC 003's handoff. What exists is `--version`, which names the build's commit and the prikk
//! floor it was built for, so an operator can tell what they run.

use inklkunn_prikk::MIN_PRIKK_VERSION;

/// The commit the release workflow built from (`INKLKUNN_COMMIT` at build time), or `unknown`.
const COMMIT: &str = match option_env!("INKLKUNN_COMMIT") {
    Some(c) => c,
    None => "unknown",
};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version" | "-V") | None => {
            println!(
                "inklkunn {} (commit {COMMIT}; requires prikk >= {MIN_PRIKK_VERSION}, bubblewrap, prlimit)",
                env!("CARGO_PKG_VERSION")
            );
            if args.is_empty() {
                println!("no server yet (M0); the `serve` command arrives with RFC 003's handoff");
            }
        }
        Some(other) => {
            eprintln!("inklkunn: unknown command `{other}`; only --version exists in M0");
            std::process::exit(2);
        }
    }
}
