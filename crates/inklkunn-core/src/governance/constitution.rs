//! The wiki's constitution: who may change `governance/` (RFC 006 D-1).
//!
//! ```yaml
//! policy_change:            # a change to governance/policy.yaml needs
//!   rung: maintainer        #   maintainer | admin           (default maintainer)
//!   approvals: 0            #   0 to 5 approvals              (default 0)
//! constitution_change:      # a change to governance/constitution.yaml needs
//!   approvals: 0            #   0 to 5 approvals              (default 0); the rung is admin
//! ```
//!
//! **The constitution's rung is fixed at admin and is not a key**: writing `constitution_change.rung` is an unknown key
//! (the same refusal as any other key the schema does not have), so that no file can say that someone below an admin
//! changes the constitution, and the admin's hatch (RFC 006 D-4, INV-14) is never in doubt. **A policy change's rung
//! is `maintainer` or `admin`**: a rung below a maintainer is refused as a word the key does not allow, because the
//! rules of a wiki would then be changeable by someone who cannot accept an ordinary proposal in a default wiki. (A
//! later schema version can widen the list.)

use inklkunn_store::Role;

use super::defaults;

/// What a change to a governance file needs (RFC 006 D-1): the rung and the number of approvals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Requirement {
    /// The lowest rung that may accept the change.
    pub rung: Role,
    /// The approvals, from people other than the proposer, the change needs (0 to
    /// [`MAX_APPROVALS`](super::MAX_APPROVALS)).
    pub approvals: u8,
}

/// Which governance file a change is to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GovernanceChange {
    /// A change to `governance/policy.yaml`, **or to `governance/interwiki.yaml`**: the map says where every link with a
    /// prefix leads, which is more than one page's edit, and it needs what a change to the policy needs (RFC 007 D-8).
    Policy,
    /// A change to `governance/constitution.yaml`.
    Constitution,
}

impl GovernanceChange {
    /// What a change of this kind needs under `constitution`. **The hatch is not here**: an admin of the wiki may
    /// accept either kind with no approvals whatever this says, and that lives in the acceptance code (RFC 006 D-4).
    pub fn required(self, constitution: &Constitution) -> Requirement {
        match self {
            GovernanceChange::Policy => constitution.policy_change,
            GovernanceChange::Constitution => constitution.constitution_change,
        }
    }

    /// The kind of change a repository path is, if it is one of the governance files.
    pub fn of_path(path: &str) -> Option<GovernanceChange> {
        match super::GovernanceFile::of_path(path)? {
            super::GovernanceFile::Policy | super::GovernanceFile::Interwiki => {
                Some(GovernanceChange::Policy)
            }
            super::GovernanceFile::Constitution => Some(GovernanceChange::Constitution),
        }
    }

    /// The kind of change a repository path is when it is one of the two files **or lies below one** (a directory that
    /// stands where the file belongs: removing what is in it is a change to that file's place, and needs what a change to the
    /// file needs; handoff G-T1 correction D-1).
    pub fn at_or_below(path: &str) -> Option<GovernanceChange> {
        match super::GovernanceFile::at_or_below(path)? {
            super::GovernanceFile::Policy | super::GovernanceFile::Interwiki => {
                Some(GovernanceChange::Policy)
            }
            super::GovernanceFile::Constitution => Some(GovernanceChange::Constitution),
        }
    }
}

/// The wiki's constitution. Built by [`Constitution::default`] (0.3.0's behaviour) or by reading a file
/// ([`read_constitution`](super::read_constitution), [`validate_constitution`](super::validate_constitution)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Constitution {
    pub(super) policy_change: Requirement,
    pub(super) constitution_change: Requirement,
}

impl Default for Constitution {
    /// A policy change needs a maintainer and no approvals; a constitution change needs an admin and no approvals.
    /// 0.3.0 has no governance files, and the accepter's rung is the whole of the rule; this is that rule.
    fn default() -> Self {
        Constitution {
            policy_change: Requirement {
                rung: defaults::POLICY_CHANGE_RUNG,
                approvals: defaults::POLICY_CHANGE_APPROVALS,
            },
            constitution_change: Requirement {
                rung: defaults::CONSTITUTION_CHANGE_RUNG,
                approvals: defaults::CONSTITUTION_CHANGE_APPROVALS,
            },
        }
    }
}

impl Constitution {
    /// What a change to the policy needs.
    pub fn policy_change(&self) -> Requirement {
        self.policy_change
    }

    /// What a change to the constitution needs. Its rung is always admin.
    pub fn constitution_change(&self) -> Requirement {
        self.constitution_change
    }

    /// The canonical text of the constitution: every key the schema has. Reading it back gives the constitution
    /// again.
    pub fn to_yaml(&self) -> String {
        format!(
            "policy_change:\n  rung: {}\n  approvals: {}\nconstitution_change:\n  approvals: {}\n",
            self.policy_change.rung.as_str(),
            self.policy_change.approvals,
            self.constitution_change.approvals,
        )
    }
}
