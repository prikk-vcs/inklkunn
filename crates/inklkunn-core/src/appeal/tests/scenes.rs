//! One scene for each kind of action an appeal is against: who it affects, who took it, and the handle a link names.

use inklkunn_store::{AppealedKind, Role};

use super::support::{Person, World};
use crate::appeal::ContestRef;

/// An action in force, the person it affects, and the person who took it.
pub(super) struct Scene {
    /// The person the action is about: who appeals.
    pub(super) appellant: Person,
    /// Who took it.
    pub(super) actor: Person,
    /// The handle of the action.
    pub(super) contest: ContestRef,
    /// A proposal the scene made, for the kinds that have one.
    pub(super) proposal: Option<inklkunn_store::ProposalId>,
    /// A comment the scene hid, for a tombstone.
    pub(super) comment: Option<inklkunn_store::CommentId>,
}

/// The scene of `kind` in `w`. The person who took the action holds the rung `maintainer` (an admin for a grant), and
/// the world's wiki has an admin, **Dee**, who took none of them.
pub(super) fn scene(w: &World, kind: AppealedKind) -> Scene {
    match kind {
        AppealedKind::Decline => {
            let author = w.person("Pat", Some(Role::Contributor));
            let actor = w.person("Max", Some(Role::Maintainer));
            let proposal = w.proposal(&author, "Home");
            let contest = w.decline(&proposal, &actor);
            Scene {
                appellant: author,
                actor,
                contest,
                proposal: Some(proposal),
                comment: None,
            }
        }
        AppealedKind::Tombstone => {
            let author = w.person("Pat", Some(Role::Contributor));
            let actor = w.person("Max", Some(Role::Maintainer));
            let (contest, comment) = w.hide(&actor, &author);
            Scene {
                appellant: author,
                actor,
                contest,
                proposal: None,
                comment: Some(comment),
            }
        }
        AppealedKind::Restriction => {
            let person = w.person("Pat", Some(Role::Contributor));
            let actor = w.person("Max", Some(Role::Maintainer));
            let contest = w.restrict(&actor, &person, 3);
            Scene {
                appellant: person,
                actor,
                contest,
                proposal: None,
                comment: None,
            }
        }
        AppealedKind::Protection => {
            let author = w.person("Pat", Some(Role::Contributor));
            let actor = w.person("Max", Some(Role::Maintainer));
            let proposal = w.proposal(&author, "Home");
            let contest = w.protect(&actor, "Home", 3);
            Scene {
                appellant: author,
                actor,
                contest,
                proposal: Some(proposal),
                comment: None,
            }
        }
        AppealedKind::Grant => {
            let target = w.person("Pat", Some(Role::Maintainer));
            let actor = w.person("Ada", Some(Role::Admin));
            let contest = w.regrade(
                Some(&actor),
                &target,
                Some(Role::Maintainer),
                Some(Role::Reviewer),
            );
            Scene {
                appellant: target,
                actor,
                contest,
                proposal: None,
                comment: None,
            }
        }
    }
}
