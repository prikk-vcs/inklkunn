//! What the process does about itself before it does anything else (corrections A1 of the sign-in handoff's
//! review of U5).
//!
//! **No core dumps.** The serving process holds the key store's derived master key for its whole life and a
//! wiki's seed for the length of one signing call. A core file (or a `ptrace` attach by another process of the
//! same user, or a read of `/proc/<pid>/mem`) would hold them in the clear, so the process makes itself
//! non-dumpable at start: `PR_SET_DUMPABLE` 0 through `rustix`'s safe wrapper (the crate forbids `unsafe`).
//! That also turns off the kernel's core dump for the process whatever `ulimit -c` says. **The operator's unit
//! says `LimitCORE=0` as well** (`SECURITY.md`): the second wall, for the day the first is removed.

use std::io::Write;

use rustix::process::{DumpableBehavior, set_dumpable_behavior};

/// Make this process non-dumpable. A failure (a kernel without `prctl`, a sandbox that forbids it) is said, once,
/// and does not stop the command: the process is no worse off than it was.
pub(crate) fn no_core_dumps(err: &mut dyn Write) {
    if let Err(e) = set_dumpable_behavior(DumpableBehavior::NotDumpable) {
        let _ = writeln!(
            err,
            "inklkunn: warning: this process could not be made non-dumpable ({e}); \
             run it with LimitCORE=0 and without ptrace permission for other processes"
        );
    }
}

#[cfg(test)]
mod tests;
