//! History, change, difference and permalink (handoff 003 T5): the scripted past of a test wiki. Each version
//! of the in-memory repository is a block; the messages, what `show` says and what `diff` answers are given
//! here, so that the page tests read one history.
#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use inklkunn_prikk::storage::{DiffSide, PathRef};
use inklkunn_prikk::{Change, Diff, DiffEntry, Operation, Patch, Result};
use inklkunn_store::DisplayName;

use super::{World, block_id, page};

/// One operation of a block, on the paths given.
pub fn op(kind: &str, paths: &[&str]) -> Operation {
    Operation {
        kind: kind.to_owned(),
        paths: paths
            .iter()
            .map(|p| PathRef {
                path: Some((*p).to_owned()),
                unresolved_node_id: None,
            })
            .collect(),
        content: Default::default(),
    }
}

/// What `show` says a block did.
pub fn shown(ops: Vec<Operation>) -> Result<Change> {
    Ok(Change {
        schema_version: "show-report-v1".to_owned(),
        patches: vec![Patch {
            patch_id: "p".repeat(64),
            queued: false,
            operations: ops,
        }],
    })
}

/// A message in the form the service writes for an accepted proposal.
pub fn proposal_text(summary: &str, proposer: &str, acceptor: &str) -> String {
    use inklkunn_core::message::{Actor, BlockId, Message, Summary};
    use inklkunn_store::{AccountId, ProposalId};
    let actor = |id: &str, name: &str| Actor::Account {
        id: AccountId::parse(id).expect("id"),
        name: DisplayName::new(name).expect("name"),
    };
    Message::proposal(
        Summary::new(summary).expect("summary"),
        ProposalId::parse("019a0000-0000-7000-8000-00000000aaaa").expect("proposal"),
        actor("019a0000-0000-7000-8000-0000000000a1", proposer),
        actor("019a0000-0000-7000-8000-0000000000a2", acceptor),
        BlockId::new(&block_id(1)).expect("block"),
    )
    .expect("a proposal message")
    .to_text()
}

/// A message in the form the service writes for a change it makes itself.
pub fn service_text(summary: &str) -> String {
    use inklkunn_core::message::{Message, Summary, SystemLabel};
    Message::system(
        Summary::new(summary).expect("summary"),
        SystemLabel::inklkunn(),
    )
    .to_text()
}

/// What `diff` says of one page between the blocks of two versions.
pub fn diff_of(path: &str, status: &str, hunks: &[&str], from: u64, to: u64) -> Diff {
    Diff {
        schema_version: "diff-report-v1".to_owned(),
        from: DiffSide {
            point: block_id(from),
            target_block_id: block_id(from),
        },
        to: DiffSide {
            point: block_id(to),
            target_block_id: block_id(to),
        },
        entries: vec![DiffEntry {
            path: path.to_owned(),
            status: status.to_owned(),
            from: None,
            to: None,
            hunks: hunks.iter().map(|h| (*h).to_owned()).collect(),
        }],
        unsupported_paths: Vec::new(),
    }
}

/// The hunk the page tests show: a changed line with HTML in it, a Japanese line, an added line with an
/// ampersand, and prikk's marker for a text with no final line break.
pub const HOME_HUNK: &str = "@@ -6,5 +6,6 @@ Welcome\n # Home\n \n-Welcome. See [[Guide]] and <b>the</b> [[Nowhere]].\n+Welcome to the whole wiki. See [[Guide]] and <script>alert(1)</script> [[Nowhere]].\n-日本語のページです。\n+日本語の新しいページです。\n+A new line & more.\n tail\n\\ No newline at end of file\n";

/// Give the handbook a history after its first version: version 2 is an accepted proposal that edits `Home`,
/// 3 a change the service made itself that edits `Guide` and adds an attachment, 4 a change someone else sealed
/// with a message of their own that edits `Home`. `Home` is a page of three versions.
pub fn script_history(w: &World) {
    let home = |text: &str| page(&["title: Home", "description: Start here"], text);
    w.fake.say(1, "Create the handbook");
    w.fake.show(
        1,
        shown(vec![
            op("create-file", &["pages/Home.md"]),
            op("create-file", &["pages/Guide.md"]),
        ]),
    );
    w.fake
        .set_files(&[("pages/Home.md", &home("# Welcome\n\nSecond.\n"))]);
    w.fake.say(
        2,
        &proposal_text("Improve the welcome", "Mika Tanaka", "Ren Sato"),
    );
    w.fake
        .show(2, shown(vec![op("edit-text", &["pages/Home.md"])]));
    w.fake.set_files(&[(
        "pages/Guide.md",
        &page(
            &["title: Guide", "aliases: [How-to]"],
            "The guide, edited.\n",
        ),
    )]);
    w.fake.say(3, &service_text("Import the guide"));
    w.fake.show(
        3,
        shown(vec![
            op("edit-text", &["pages/Guide.md"]),
            op("create-file", &["attachments/diagram.svg"]),
        ]),
    );
    w.fake
        .set_files(&[("pages/Home.md", &home("# Welcome\n\nThird.\n"))]);
    w.fake.say(4, "fixup\u{7}: a plain message");
    w.fake
        .show(4, shown(vec![op("edit-text", &["pages/Home.md"])]));
    w.fake
        .diff_says(Ok(diff_of("pages/Home.md", "modified", &[HOME_HUNK], 1, 2)));
}
