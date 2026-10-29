//! Key lifecycle against a real prikk (RFC 002 D-8; handoff 002 T7). Self-skips with a visible line when
//! prikk is not on `PATH`. Unconfined: these tests are about keys and trust, and the sandbox has its own.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::Command;

use inklkunn_prikk::sandbox::Sandbox;
use inklkunn_store::RootLayout;

use super::*;
use crate::RepositoryBootstrap;
use crate::accept::{AcceptanceLedger, AcceptanceToken, ExpectedMessage, LedgerError};
use crate::keys::Role;
use crate::message::{Message, Summary, SystemLabel};
use crate::{SignerConfig, accept::ViewCreationAct, accept::ViewCreationFacts};

const SECRET: &str = "an operator secret for the lifecycle tests, long enough";

struct NoLedger;

impl AcceptanceLedger for NoLedger {
    fn pending(&self, _: &AcceptanceToken) -> Result<Option<ExpectedMessage>, LedgerError> {
        Ok(None)
    }
    fn view_creation(&self, _: &ViewCreationAct) -> Result<Option<ViewCreationFacts>, LedgerError> {
        Ok(None)
    }
}

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn real_prikk() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("prikk"))
            .find(|c| c.is_file())
    })
}

fn dir(base: &Path, tag: &str) -> Dir {
    let p = base.join(format!("inklkunn-lc-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
    Dir(p)
}

struct World {
    _tmp: Dir,
    shm: Dir,
    signer: Signer,
    wiki: WikiId,
    repo: PathBuf,
    prikk: PathBuf,
    root: RootLayout,
    key_dir: PathBuf,
    secret_file: PathBuf,
}

/// A wiki made through the signer's own entry points, with its first change sealed.
fn world(tag: &str) -> Option<World> {
    let Some(prikk) = real_prikk() else {
        println!("skipping: prikk not on PATH");
        return None;
    };
    let tmp = dir(&std::env::temp_dir(), tag);
    let shm = dir(Path::new("/dev/shm"), tag);
    let root = RootLayout::new(tmp.0.join("wikis"));
    let key_dir = tmp.0.join("keys");
    let secret_file = tmp.0.join("operator.secret");
    std::fs::write(&secret_file, SECRET).unwrap();
    let mut cfg = SignerConfig::new(
        &prikk,
        Sandbox::Unconfined,
        root.clone(),
        &key_dir,
        &secret_file,
        &shm.0,
    );
    cfg.cheap_derivation = true;
    let signer = Signer::open_creating_store(cfg, std::sync::Arc::new(NoLedger)).unwrap();
    let wiki = WikiId::new();
    let repo = root.prepare(&wiki).unwrap();
    signer.create_repository(&repo).unwrap();
    std::fs::write(repo.join("Main.md"), "# Main\n").unwrap();
    signer
        .commit_and_seal(
            &repo,
            &Message::system(
                Summary::from_text("Create the wiki").unwrap(),
                SystemLabel::inklkunn(),
            )
            .to_text(),
        )
        .unwrap();
    Some(World {
        _tmp: tmp,
        shm,
        signer,
        wiki,
        repo,
        prikk,
        root,
        key_dir,
        secret_file,
    })
}

impl World {
    /// A second signer over the same wiki, key store and seed base, that runs this `prikk` instead.
    fn signer_with_prikk(&self, prikk: &Path) -> Signer {
        let mut cfg = SignerConfig::new(
            prikk,
            Sandbox::Unconfined,
            self.root.clone(),
            &self.key_dir,
            &self.secret_file,
            &self.shm.0,
        );
        cfg.cheap_derivation = true;
        Signer::open_creating_store(cfg, std::sync::Arc::new(NoLedger)).unwrap()
    }

    fn prikk(&self, args: &[&str]) -> std::process::Output {
        Command::new(&self.prikk)
            .args(args)
            .current_dir(&self.repo)
            .output()
            .unwrap()
    }

    fn verify(&self) -> (bool, String) {
        let o = self.prikk(&["verify"]);
        (
            o.status.success(),
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            ),
        )
    }

    /// A change sealed by the service with whatever keys are in force now (the acceptance guard is
    /// `Accept`'s, and has its own tests; this is about which key signs).
    fn seal_with_current_keys(&self, text: &str) {
        std::fs::write(self.repo.join("Main.md"), text).unwrap();
        let message = Message::system(
            Summary::from_text("A change").unwrap(),
            SystemLabel::inklkunn(),
        )
        .to_text();
        let seeds = self.signer.seeds(&self.wiki);
        self.signer
            .writes
            .commit_and_seal(&self.repo, &seeds, &message)
            .unwrap();
        assert!(!seeds.left_over());
    }

    /// The maintainer key ids that signed the sealed blocks, as `prikk verify` lists them.
    fn sealing_keys(&self) -> Vec<String> {
        let (_, out) = self.verify();
        out.lines()
            .filter_map(|l| l.strip_prefix("sealed-block "))
            .filter_map(|l| l.split(": ").nth(1))
            .map(str::to_owned)
            .collect()
    }

    /// Copy the seeds in force now to a directory the test keeps, as a thief would have them.
    fn steal_seeds(&self, into: &Path) -> (String, String) {
        std::fs::create_dir_all(into).unwrap();
        let mut ids = (String::new(), String::new());
        for role in [Role::Author, Role::Maintainer] {
            let (meta, seeds) = self
                .signer
                .store
                .checkout(&self.wiki, &self.signer.seed_base, role)
                .unwrap();
            std::fs::copy(
                seeds.key_dir().seed_file(role.name()),
                into.join(format!("{}.seed", role.name())),
            )
            .unwrap();
            seeds.finish().unwrap();
            ids = (meta.author_key_id, meta.maintainer_key_id);
        }
        ids
    }

    /// Try to commit and seal under stolen keys; the output of the failing step, or `None` when it worked.
    fn seal_with_stolen(&self, dir: &Path, ids: &(String, String)) -> Option<String> {
        std::fs::write(self.repo.join("Main.md"), "forged\n").unwrap();
        let run = |args: &[&str]| {
            Command::new(&self.prikk)
                .args(args)
                .current_dir(&self.repo)
                .env("PRIKK_AUTHOR_KEY_ID", &ids.0)
                .env("PRIKK_AUTHOR_SEED_FILE", dir.join("author.seed"))
                .env("PRIKK_MAINTAINER_KEY_ID", &ids.1)
                .env("PRIKK_MAINTAINER_SEED_FILE", dir.join("maintainer.seed"))
                .output()
                .unwrap()
        };
        let commit = run(&["commit", "-m", "forged"]);
        assert!(
            commit.status.success(),
            "{}",
            String::from_utf8_lossy(&commit.stderr)
        );
        let seal = run(&["seal", "--allow-no-audit"]);
        (!seal.status.success()).then(|| String::from_utf8_lossy(&seal.stderr).into_owned())
    }
}

#[test]
fn keys_are_created_for_an_initialised_repository_that_has_none_and_only_once() {
    let Some(prikk) = real_prikk() else {
        println!("skipping: prikk not on PATH");
        return;
    };
    let tmp = dir(&std::env::temp_dir(), "create");
    let shm = dir(Path::new("/dev/shm"), "create");
    let root = RootLayout::new(tmp.0.join("wikis"));
    let secret_file = tmp.0.join("operator.secret");
    std::fs::write(&secret_file, SECRET).unwrap();
    let mut cfg = SignerConfig::new(
        &prikk,
        Sandbox::Unconfined,
        root.clone(),
        tmp.0.join("keys"),
        &secret_file,
        &shm.0,
    );
    cfg.cheap_derivation = true;
    let signer = Signer::open_creating_store(cfg, std::sync::Arc::new(NoLedger)).unwrap();
    let wiki = WikiId::new();
    let repo = root.prepare(&wiki).unwrap();
    // A repository that exists and has no keys (restored from a backup of the repository alone).
    let init = Command::new(&prikk)
        .args(["init", "."])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(init.status.success());
    assert!(
        matches!(signer.key_status(&wiki), Err(SignerError::Refused(_))),
        "no keys yet"
    );

    let ids = signer.create_keys(&repo).unwrap();
    assert_eq!(ids.author, format!("inklkunn-{wiki}-author"));
    assert_eq!(ids.maintainer, format!("inklkunn-{wiki}-maintainer"));
    let status = signer.key_status(&wiki).unwrap();
    assert_eq!((status.generation, status.rotated_at), (1, None));
    assert!(
        matches!(signer.create_keys(&repo), Err(SignerError::Refused(_))),
        "not twice"
    );
}

#[test]
fn a_rotation_that_keeps_the_old_key_gives_new_ids_signs_with_them_and_the_repository_verifies() {
    let Some(w) = world("keep") else { return };
    let before = w.signer.key_status(&w.wiki).unwrap();
    assert_eq!(before.generation, 1);
    let rotation = w.signer.rotate_keys(&w.repo, OldKey::Keep).unwrap();
    let after = w.signer.key_status(&w.wiki).unwrap();
    assert_eq!(after, rotation.status);
    assert_eq!(after.generation, 2);
    assert_eq!(
        rotation.previous_maintainer_key_id,
        before.maintainer_key_id
    );
    assert_ne!(after.author_key_id, before.author_key_id);
    assert_ne!(after.maintainer_key_id, before.maintainer_key_id);
    assert!(
        after
            .maintainer_key_id
            .starts_with(&format!("inklkunn-{}-maintainer-2-", w.wiki))
    );
    assert_eq!(
        after.created_at, before.created_at,
        "the first keys' date is kept"
    );
    assert!(after.rotated_at.is_some());

    // New changes are signed by the new keys; the old history still verifies.
    w.seal_with_current_keys("second\n");
    let (ok, out) = w.verify();
    assert!(ok, "{out}");
    let signers = w.sealing_keys();
    assert!(signers.contains(&before.maintainer_key_id), "{signers:?}");
    assert!(signers.contains(&after.maintainer_key_id), "{signers:?}");
}

#[test]
fn keeping_the_old_key_trusted_means_a_copy_of_its_seed_can_still_seal() {
    let Some(w) = world("keep-thief") else { return };
    let loot = dir(Path::new("/dev/shm"), "keep-loot");
    let stolen_ids = w.steal_seeds(&loot.0);
    w.signer.rotate_keys(&w.repo, OldKey::Keep).unwrap();
    // This is what `Keep` costs, said in a test so nobody has to take it on trust.
    assert_eq!(
        w.seal_with_stolen(&loot.0, &stolen_ids),
        None,
        "the old key still seals"
    );
    let (ok, out) = w.verify();
    assert!(ok, "and the repository verifies: {out}");
}

#[test]
fn retiring_the_old_key_stops_it_sealing_and_prikk_then_cannot_verify_what_it_sealed() {
    let Some(w) = world("retire") else { return };
    let loot = dir(Path::new("/dev/shm"), "retire-loot");
    let stolen_ids = w.steal_seeds(&loot.0);
    let rotation = w.signer.rotate_keys(&w.repo, OldKey::Retire).unwrap();
    assert_eq!(rotation.old_key, OldKeyOutcome::Retired);

    // The new keys seal.
    w.seal_with_current_keys("second\n");
    // The old maintainer key no longer verifies new seals: prikk refuses to seal under it at all.
    let refused = w
        .seal_with_stolen(&loot.0, &stolen_ids)
        .expect("the old key must not seal");
    assert!(refused.contains("not trusted by policy"), "{refused}");

    // The canary for ledger PK-34. prikk 0.54.1 cannot keep a retired key's history verifiable: `verify`
    // fails for the blocks it signed. When a prikk release fixes that, this assertion flips and RFC 002 D-8's
    // "the repository still verifies" becomes true of `Retire`; update PK-34 and the runbook then.
    let (ok, out) = w.verify();
    assert!(
        !ok,
        "prikk now verifies history sealed by a retired key; see PK-34\n{out}"
    );
    assert!(out.contains("PRIKK-TRUST-PUBLICATION-UNTRUSTED"), "{out}");
    assert!(
        w.sealing_keys().iter().all(|k| *k != stolen_ids.1),
        "the retired key's seals are not counted as sealed blocks"
    );
}

#[test]
fn a_rotation_that_fails_before_the_record_changes_leaves_the_wiki_as_it_was() {
    let Some(w) = world("atomic") else { return };
    let before = w.signer.key_status(&w.wiki).unwrap();
    // Damage the repository so that adopting the new key fails.
    std::fs::remove_dir_all(w.repo.join(".prikk")).unwrap();
    assert!(w.signer.rotate_keys(&w.repo, OldKey::Keep).is_err());
    assert_eq!(
        w.signer.key_status(&w.wiki).unwrap(),
        before,
        "the record is untouched"
    );
    // No decrypted seed or scratch directory was left behind.
    assert_eq!(w.signer.leftover_call_directories_removed(), 0);
}

#[test]
fn only_a_wiki_with_keys_can_be_rotated_and_only_its_own_repository_directory() {
    let Some(w) = world("refuse") else { return };
    let elsewhere = w.repo.parent().unwrap().join("not-a-wiki-id");
    std::fs::create_dir_all(&elsewhere).unwrap();
    assert!(matches!(
        w.signer.rotate_keys(&elsewhere, OldKey::Keep),
        Err(SignerError::Refused(_))
    ));
    // A well-formed wiki id with no keys.
    let other = WikiId::new();
    let other_repo = w.signer.root.prepare(&other).unwrap();
    assert!(matches!(
        w.signer.rotate_keys(&other_repo, OldKey::Keep),
        Err(SignerError::Refused(_))
    ));
}

#[test]
fn two_rotations_in_a_row_get_distinct_ids_each_time() {
    let Some(w) = world("twice") else { return };
    let first = w.signer.rotate_keys(&w.repo, OldKey::Keep).unwrap().status;
    let second = w.signer.rotate_keys(&w.repo, OldKey::Keep).unwrap().status;
    assert_eq!((first.generation, second.generation), (2, 3));
    assert_ne!(first.maintainer_key_id, second.maintainer_key_id);
    assert_ne!(first.author_key_id, second.author_key_id);
    w.seal_with_current_keys("third\n");
    assert!(w.verify().0);
}

#[test]
fn a_retirement_that_fails_is_a_rotation_that_happened_and_says_the_old_key_is_still_trusted() {
    let Some(w) = world("retire-fails") else {
        return;
    };
    let before = w.signer.key_status(&w.wiki).unwrap();
    // `trust maintainer remove` of an id prikk refuses to remove: make the adopted set hold only the key we
    // are about to leave, so removing it after the new one is adopted is allowed — then break the repository
    // so that the removal itself fails (the new key's adoption must succeed first, so only the removal fails).
    // A stand-in is the clean way to do that: wrap prikk so that `trust maintainer remove` fails.
    let standin = w._tmp.0.join("prikk-standin");
    std::fs::write(
        &standin,
        format!(
            "#!/bin/sh\nif [ \"$1 $2 $3\" = \"trust maintainer remove\" ]; then echo 'error: precondition not met: injected' >&2; exit 1; fi\nexec {} \"$@\"\n",
            w.prikk.display()
        ),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&standin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let signer = w.signer_with_prikk(&standin);
    let rotation = signer
        .rotate_keys(&w.repo, OldKey::Retire)
        .expect("the rotation happened");
    assert!(
        matches!(rotation.old_key, OldKeyOutcome::RetireFailed(ref why) if why.contains("injected")),
        "{rotation:?}"
    );
    // The key store did change, and the old key is still trusted, so the history still verifies.
    let after = signer.key_status(&w.wiki).unwrap();
    assert_eq!(after.generation, before.generation + 1);
    assert_eq!(after, rotation.status);
    assert!(
        w.verify().0,
        "the old key is still trusted: {}",
        w.verify().1
    );
}
