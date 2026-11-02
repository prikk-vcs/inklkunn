#![allow(clippy::expect_used, clippy::unwrap_used)]

use rustix::process::{DumpableBehavior, dumpable_behavior, set_dumpable_behavior};

use super::*;

/// One test, not two, and no assertion that the flag is *set*: it belongs to the whole process, tests run on
/// threads of one, and every command run by another test makes it non-dumpable too (only that direction is safe).
#[test]
fn the_process_is_non_dumpable_after_it_says_so_and_every_command_starts_by_saying_it() {
    set_dumpable_behavior(DumpableBehavior::Dumpable).unwrap();
    let mut err = Vec::new();
    no_core_dumps(&mut err);
    assert_eq!(dumpable_behavior().unwrap(), DumpableBehavior::NotDumpable);
    assert!(err.is_empty(), "nothing to say when it worked");

    // The command line is the one entry point (`main` only hands it the process), so `serve` and the commands
    // that open the key store are covered by the same call. An unknown command is the cheapest to run.
    set_dumpable_behavior(DumpableBehavior::Dumpable).unwrap();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = crate::cli::run(
        &["no-such-command".to_owned()],
        &|_| None,
        0,
        &mut out,
        &mut err,
    );
    assert_eq!(status, 2);
    assert_eq!(dumpable_behavior().unwrap(), DumpableBehavior::NotDumpable);
}
