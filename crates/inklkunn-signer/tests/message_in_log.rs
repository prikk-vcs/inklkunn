//! The patch message in real history (RFC 002 D-5; handoff 002 T5): every kind is sealed with a real prikk,
//! read back through the driver's `log`, and found **byte for byte** as composed and parseable to the same
//! message. prikk's own JSON is the witness, not a mock. Self-skips with a visible line when prikk is not on
//! `PATH`; runs confined when bubblewrap works.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::time::Duration;

use inklkunn_prikk::sandbox::probe_bwrap;
use inklkunn_prikk::{Admission, Point, PrikkCli, Sandbox, Storage};
use inklkunn_signer::message::{
    Actor, BlockId, ImportSource, Message, Pseudonym, Summary, SystemLabel,
};
use inklkunn_signer::{BootstrapSigner, RepositoryBootstrap};
use inklkunn_store::{AccountId, DisplayName, ProposalId};

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> TempDir {
    let p = std::env::temp_dir().join(format!("inklkunn-msg-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp");
    TempDir(p)
}

fn prikk_on_path() -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("prikk").is_file()))
        .unwrap_or(false)
}

fn policies() -> Vec<Sandbox> {
    if probe_bwrap(std::ffi::OsStr::new("bwrap")).is_ok() {
        vec![Sandbox::default(), Sandbox::Unconfined]
    } else {
        println!("skipping: bubblewrap cannot confine a process here; running unconfined");
        vec![Sandbox::Unconfined]
    }
}

fn account(name: &str) -> Actor {
    Actor::Account {
        id: AccountId::new(),
        name: DisplayName::new(name).expect("name"),
    }
}

fn summary(s: &str) -> Summary {
    Summary::from_text(s).expect("summary")
}

#[test]
fn every_kind_of_message_is_found_in_prikks_log_exactly_as_composed() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let base = BlockId::new(&"ab".repeat(32)).expect("block");
    let messages = [
        Message::proposal(
            summary("Fix a typo — “colour” (UK) / 色"),
            ProposalId::new(),
            account("Aiko Tanaka (she/her)"),
            account("Ben (maintainer)"),
            base.clone(),
        )
        .expect("proposal"),
        Message::rollback(
            summary("Undo the typo fix"),
            ProposalId::new(),
            Actor::Anonymous(Pseudonym::new("~k3x9q").expect("pseudonym")),
            account("日本語 (にほんご)"),
            base,
        )
        .expect("rollback"),
        Message::import(
            summary("Import 212 pages"),
            SystemLabel::new("import").expect("label"),
            ImportSource::new(
                "sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
            )
            .expect("source"),
        ),
        Message::system(summary(&"x".repeat(300)), SystemLabel::inklkunn()),
    ];
    for (n, sandbox) in policies().into_iter().enumerate() {
        let tmp = temp(&format!("log-{n}"));
        let keys = tmp.0.join("keys");
        let repo = tmp.0.join("repo");
        std::fs::create_dir_all(&repo).expect("repo");
        let signer = BootstrapSigner::new("prikk", sandbox.clone(), &keys).expect("signer");
        signer.create_repository(&repo).expect("repository");
        for (i, m) in messages.iter().enumerate() {
            std::fs::write(repo.join(format!("page-{i}.md")), format!("# {i}\n")).expect("write");
            signer
                .commit_and_seal(&repo, &m.to_text())
                .unwrap_or_else(|e| panic!("{e}"));
        }
        let storage = PrikkCli::open(
            "prikk",
            sandbox,
            Admission::new(1 << 30, 64 << 20, Duration::from_millis(200)),
        )
        .expect("prikk at the floor");
        let history = storage
            .history(&repo, &Point::View("heads/main".to_owned()), None)
            .expect("history");
        assert_eq!(history.blocks.len(), messages.len());
        // Newest first.
        for (block, composed) in history.blocks.iter().zip(messages.iter().rev()) {
            let stored = &block.patch_messages.first().expect("patch").message;
            assert_eq!(
                stored,
                &composed.to_text(),
                "prikk stored the message byte for byte"
            );
            assert_eq!(
                &Message::parse(stored).expect("the stored message parses"),
                composed
            );
        }
    }
}
