//! A system-principal import through `Accept` (handoff 003 T7; RFC 003 D-12), checked **independently of
//! inklkunn**: after the change, the real `prikk verify` run by the test itself passes on the repository, and
//! `prikk log` shows the CT-03 trailers of an import. `inklkunn-core`'s `import-folder` produces exactly this
//! kind of change; the core crate may not spawn a process (the boundary gate), so the independent check lives
//! here. Self-skips with a visible line when prikk is not on `PATH`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use inklkunn_prikk::Sandbox;
use inklkunn_signer::message::{Actor, BlockId, ImportSource, Kind, Message, Summary, SystemLabel};
use inklkunn_signer::{
    Accept, AcceptError, AcceptanceLedger, AcceptanceToken, ExpectedMessage, KeyAuthority,
    LedgerError, PreparedChange, PreparedFile, RepositoryBootstrap, Signer, SignerConfig,
    ViewCreationAct, ViewCreationFacts,
};
use inklkunn_store::{RootLayout, WikiId};

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir_under(base: &Path, tag: &str) -> Dir {
    let p = base.join(format!("inklkunn-ik-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
    Dir(p)
}

fn real_prikk() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("prikk"))
            .find(|c| c.is_file())
    })
}

#[derive(Default)]
struct Ledger(Mutex<HashMap<String, ExpectedMessage>>);

impl AcceptanceLedger for Ledger {
    fn pending(&self, t: &AcceptanceToken) -> Result<Option<ExpectedMessage>, LedgerError> {
        Ok(self.0.lock().unwrap().get(t.as_str()).cloned())
    }
    fn view_creation(&self, _: &ViewCreationAct) -> Result<Option<ViewCreationFacts>, LedgerError> {
        Ok(None)
    }
}

fn prikk_in(prikk: &Path, repo: &Path, args: &[&str]) -> std::process::Output {
    Command::new(prikk)
        .args(args)
        .current_dir(repo)
        .output()
        .expect("prikk runs")
}

#[test]
fn an_import_under_the_system_principal_verifies_with_prikk_itself_and_says_what_it_is() {
    let Some(prikk) = real_prikk() else {
        println!("skipping: prikk not on PATH");
        return;
    };
    let tmp = dir_under(&std::env::temp_dir(), "import");
    let shm = dir_under(Path::new("/dev/shm"), "import");
    let secret = tmp.0.join("secret");
    std::fs::write(
        &secret,
        "an operator secret for the import test, long enough\n",
    )
    .unwrap();
    let root = RootLayout::new(tmp.0.join("root"));
    let ledger = Arc::new(Ledger::default());
    let mut cfg = SignerConfig::new(
        &prikk,
        Sandbox::Unconfined,
        root.clone(),
        tmp.0.join("keys"),
        &secret,
        &shm.0,
    );
    cfg.cheap_derivation = true;
    let signer = Signer::open_creating_store(cfg, ledger.clone()).unwrap();
    let wiki = WikiId::new();
    let repo = root.prepare(&wiki).unwrap();
    signer
        .create_repository(&repo, &KeyAuthority::for_tests())
        .unwrap();
    std::fs::create_dir_all(repo.join("pages")).unwrap();
    std::fs::write(repo.join("pages/Help.md"), "---\ntitle: Help\n---\n\nHi.\n").unwrap();
    let first = Message::system(
        Summary::from_text("Create the wiki").unwrap(),
        SystemLabel::inklkunn(),
    );
    signer.commit_and_seal(&repo, &first.to_text()).unwrap();

    // The change an `import-folder` makes: pages and an attachment, one message, the service on both sides.
    let files = vec![
        PreparedFile {
            path: "pages/Home.md".into(),
            content: Some(b"---\ntitle: Home\nid: h-1\n---\n\nSee [[Help]].\n".to_vec()),
        },
        PreparedFile {
            path: "pages/Deep/Page.md".into(),
            content: Some(b"---\ntitle: Deep/Page\n---\n\nx\n".to_vec()),
        },
        PreparedFile {
            path: "attachments/logo.bin".into(),
            content: Some((0..=255).collect()),
        },
    ];
    let label = SystemLabel::inklkunn();
    let mut change = PreparedChange {
        files,
        renames: vec![],
        message: String::new(),
    };
    let digest = change.digest();
    let source = ImportSource::new(&format!("folder sha256:{digest}")).unwrap();
    let message = Message::import(
        Summary::from_text("Import 2 page(s) and 1 attachment(s) from a folder").unwrap(),
        label.clone(),
        source.clone(),
    );
    change.message = message.to_text();
    let token = AcceptanceToken::new("import-1");
    // The tip the plan was made against: what `prikk log` says the view's newest block is.
    let tip_now = |repo: &Path| -> BlockId {
        let log = prikk_in(&prikk, repo, &["log", "--limit", "1", "--format", "json"]);
        let log: serde_json::Value = serde_json::from_slice(&log.stdout).unwrap();
        BlockId::new(
            log.pointer("/blocks/0/block_id")
                .and_then(serde_json::Value::as_str)
                .unwrap(),
        )
        .unwrap()
    };
    let planned_at = tip_now(&repo);
    let expected = |pinned_tip: BlockId| ExpectedMessage {
        wiki: wiki.clone(),
        view: "heads/main".to_owned(),
        kind: Kind::Import,
        proposal: None,
        proposed_by: Actor::System(label.clone()),
        accepted_by: Actor::System(label.clone()),
        base: None,
        imported_from: Some(source.clone()),
        pinned_tip: Some(pinned_tip),
        change_digest: digest,
    };
    // The wiki moved after the plan was made (here: the plan names a block that is not the view's tip): the
    // signer refuses before it writes a byte, and says why.
    let moved = BlockId::new(&"0".repeat(64)).unwrap();
    ledger
        .0
        .lock()
        .unwrap()
        .insert(token.as_str().to_owned(), expected(moved));
    let refused = signer
        .accept(&wiki, "heads/main", &token, &change)
        .unwrap_err();
    assert!(
        matches!(&refused, AcceptError::Refused(m) if m.contains("tip moved") && m.contains("nothing was written")),
        "{refused:?}"
    );
    assert!(!repo.join("pages/Home.md").exists());
    assert_eq!(
        tip_now(&repo),
        planned_at,
        "a refused import moved the view"
    );
    ledger
        .0
        .lock()
        .unwrap()
        .insert(token.as_str().to_owned(), expected(planned_at.clone()));
    // A different set of files under the same token is refused before a byte is written…
    let mut other = change.clone();
    other.files.pop();
    assert_eq!(
        signer.accept(&wiki, "heads/main", &token, &other),
        Err(AcceptError::ChangeMismatch)
    );
    assert!(!repo.join("pages/Home.md").exists());
    // …and the planned change is sealed.
    let sealed = signer.accept(&wiki, "heads/main", &token, &change).unwrap();

    // prikk, independently of inklkunn: the repository verifies, nothing is queued, and the log says it.
    let verify = prikk_in(&prikk, &repo, &["verify"]);
    assert!(
        verify.status.success(),
        "{}{}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );
    let status = prikk_in(&prikk, &repo, &["status", "--format", "json"]);
    let status: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert_eq!(
        status
            .pointer("/queue/count")
            .and_then(serde_json::Value::as_u64),
        Some(0)
    );
    let log = prikk_in(&prikk, &repo, &["log", "--limit", "1", "--format", "json"]);
    let log: serde_json::Value = serde_json::from_slice(&log.stdout).unwrap();
    assert_eq!(
        log.pointer("/blocks/0/block_id")
            .and_then(serde_json::Value::as_str),
        Some(sealed.block_id.as_str())
    );
    let text = log
        .pointer("/blocks/0/patch_messages/0/message")
        .and_then(serde_json::Value::as_str)
        .unwrap();
    assert!(text.contains("Kind: import"), "{text}");
    assert!(
        text.contains("Proposed-By: system (inklkunn)")
            && text.contains("Accepted-By: system (inklkunn)"),
        "{text}"
    );
    assert!(text.contains("Imported-From: folder sha256:"), "{text}");
    assert_eq!(
        std::fs::read(repo.join("attachments/logo.bin"))
            .unwrap()
            .len(),
        256
    );
    // No seed is anywhere in the repository's files or its log.
    assert!(
        shm.0.read_dir().unwrap().next().is_none(),
        "seed directories were left"
    );
}
