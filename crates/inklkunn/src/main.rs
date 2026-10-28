#![forbid(unsafe_code)]
//! The `inklkunn` binary. What exists in this release: `--version`, which names the build's commit and the
//! prikk floor it was built for, so an operator can tell what they run, and `wiki keys`, a wiki's keys over
//! time (RFC 002 D-8). The `serve` command arrives with RFC 003's handoff.

mod cli;
mod config;
mod keys;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let status = cli::run(
        &args,
        &|name| std::env::var_os(name),
        inklkunn_store::record::now_secs(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    );
    std::process::exit(status);
}
