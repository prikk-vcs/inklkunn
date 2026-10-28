#![forbid(unsafe_code)]
//! The `inklkunn` binary. What exists in this release: `--version`, which names the build's commit and the
//! prikk floor it was built for and the prikk it finds, so an operator can tell what they run; `wiki keys`, a
//! wiki's keys over time (RFC 002 D-8); and `serve`, which checks everything and serves (RFC 003; its pages
//! arrive task by task).

mod cli;
mod config;
mod keys;
mod serve;

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
