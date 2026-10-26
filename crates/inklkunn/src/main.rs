#![forbid(unsafe_code)]
//! The `inklkunn` server binary. M0 wires nothing yet: the crates arrive task by task (RFC 001).

fn main() {
    // The binary gains its configuration, prerequisites probe and server in RFC 003's handoff.
    println!("inklkunn {} (M0: no server yet)", env!("CARGO_PKG_VERSION"));
}
