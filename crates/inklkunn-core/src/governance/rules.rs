//! The rules in force for one wiki at one tip (RFC 006 D-5): the policy, the constitution, the block that set them, what
//! a platform limit lowered, and the fault that makes the wiki refuse writes. **A value, built by the loader and held by
//! the [`Authorizer`](crate::Authorizer)'s source**: nothing a handler sends is ever part of it.

use super::fault::ReadFault;
use super::limits::Lowered;
use super::{Constitution, PlatformLimits, Policy};

/// How many characters of a block id are shown (the audit trail and the pages name a block by its beginning).
pub const SHORT_BLOCK_CHARS: usize = 12;

/// What an audit record and a page say for the rules of a wiki that has none: its defaults, which are 0.3.0's.
pub const DEFAULTS_WORD: &str = "defaults";

/// The first [`SHORT_BLOCK_CHARS`] characters of a block id (a block id is lower-case hexadecimal, so this never cuts a
/// character in two).
pub fn short_block(block: &str) -> &str {
    block.get(..SHORT_BLOCK_CHARS).unwrap_or(block)
}

/// The rules in force for one wiki at the tip of its default view.
///
/// - **No governance files**: [`Rules::defaults`], block `None`: 0.3.0's behaviour, key by key.
/// - **Files that read**: the policy as [`Policy::as_read_under`] the platform limits, the constitution, and the block
///   that last changed a governance file.
/// - **A file that cannot be read** ([`Rules::faulted`]): the wiki refuses every write ([`Rules::fault`]), and **the
///   policy in force is the default policy** whichever of the two files is the broken one, so that nothing half read ever
///   decides anything and the repair of the files (an admin's change to `governance/` only) is judged by the defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rules {
    block: Option<String>,
    policy: Policy,
    constitution: Constitution,
    lowered: Vec<Lowered>,
    fault: Option<ReadFault>,
}

impl Default for Rules {
    fn default() -> Self {
        Rules::defaults()
    }
}

impl Rules {
    /// The rules of a wiki with no governance files: 0.3.0's behaviour.
    pub fn defaults() -> Rules {
        Rules {
            block: None,
            policy: Policy::default(),
            constitution: Constitution::default(),
            lowered: Vec::new(),
            fault: None,
        }
    }

    /// The rules of governance files that were read, as they are in force under `limits`: a value above a limit is read as
    /// if it were at it ([`Policy::as_read_under`]), and [`Rules::lowered`] says which. `block` is the block that last
    /// changed a governance file.
    pub fn read(
        block: Option<String>,
        policy: Policy,
        constitution: Constitution,
        limits: &PlatformLimits,
    ) -> Rules {
        let (policy, lowered) = policy.as_read_under(limits);
        Rules {
            block,
            policy,
            constitution,
            lowered,
            fault: None,
        }
    }

    /// The rules of a wiki one of whose governance files cannot be read: **the defaults, and the fault**. `block` is the
    /// block that last changed a governance file (the one that brought the unreadable file, when the walk found it).
    pub fn faulted(block: Option<String>, fault: ReadFault) -> Rules {
        Rules {
            block,
            fault: Some(fault),
            ..Rules::defaults()
        }
    }

    /// The block that last changed a governance file, or `None` when the rules are the defaults.
    pub fn block(&self) -> Option<&str> {
        self.block.as_deref()
    }

    /// The block as an audit record and a page name it: its first [`SHORT_BLOCK_CHARS`] characters, or
    /// [`DEFAULTS_WORD`].
    pub fn short_block(&self) -> &str {
        self.block().map_or(DEFAULTS_WORD, short_block)
    }

    /// The policy in force (the default policy while [`Rules::fault`] is set).
    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    /// The constitution in force (the default one while [`Rules::fault`] is set).
    pub fn constitution(&self) -> &Constitution {
        &self.constitution
    }

    /// What a platform limit lowered when the policy was read: empty for every policy accepted under the limits.
    pub fn lowered(&self) -> &[Lowered] {
        &self.lowered
    }

    /// The governance file that cannot be read, when there is one: **the wiki refuses every write while this is set**.
    pub fn fault(&self) -> Option<&ReadFault> {
        self.fault.as_ref()
    }
}
