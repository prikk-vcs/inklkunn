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
use crate::signer::superseded::LateSeal;
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
    signer
        .create_repository(&repo, &crate::KeyAuthority::for_tests())
        .unwrap();
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

    let ids = signer
        .create_keys(&crate::KeyAuthority::for_tests(), &repo)
        .unwrap();
    assert_eq!(ids.author, format!("inklkunn-{wiki}-author"));
    assert_eq!(ids.maintainer, format!("inklkunn-{wiki}-maintainer"));
    let status = signer.key_status(&wiki).unwrap();
    assert_eq!((status.generation, status.rotated_at), (1, None));
    assert!(
        matches!(
            signer.create_keys(&crate::KeyAuthority::for_tests(), &repo),
            Err(SignerError::Refused(_))
        ),
        "not twice"
    );
}

#[test]
fn a_rotation_that_keeps_the_old_key_gives_new_ids_signs_with_them_and_the_repository_verifies() {
    let Some(w) = world("keep") else { return };
    let before = w.signer.key_status(&w.wiki).unwrap();
    assert_eq!(before.generation, 1);
    let rotation = w
        .signer
        .rotate_keys(&crate::KeyAuthority::for_tests(), &w.repo, OldKey::Keep)
        .unwrap();
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
    w.signer
        .rotate_keys(&crate::KeyAuthority::for_tests(), &w.repo, OldKey::Keep)
        .unwrap();
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
    let rotation = w
        .signer
        .rotate_keys(&crate::KeyAuthority::for_tests(), &w.repo, OldKey::Retire)
        .unwrap();
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
    assert!(
        w.signer
            .rotate_keys(&crate::KeyAuthority::for_tests(), &w.repo, OldKey::Keep)
            .is_err()
    );
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
        w.signer
            .rotate_keys(&crate::KeyAuthority::for_tests(), &elsewhere, OldKey::Keep),
        Err(SignerError::Refused(_))
    ));
    // A well-formed wiki id with no keys.
    let other = WikiId::new();
    let other_repo = w.signer.root.prepare(&other).unwrap();
    assert!(matches!(
        w.signer
            .rotate_keys(&crate::KeyAuthority::for_tests(), &other_repo, OldKey::Keep),
        Err(SignerError::Refused(_))
    ));
}

#[test]
fn two_rotations_in_a_row_get_distinct_ids_each_time() {
    let Some(w) = world("twice") else { return };
    let first = w
        .signer
        .rotate_keys(&crate::KeyAuthority::for_tests(), &w.repo, OldKey::Keep)
        .unwrap()
        .status;
    let second = w
        .signer
        .rotate_keys(&crate::KeyAuthority::for_tests(), &w.repo, OldKey::Keep)
        .unwrap()
        .status;
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
        .rotate_keys(&crate::KeyAuthority::for_tests(), &w.repo, OldKey::Retire)
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

// ---- seals by a superseded key (corrections handoff C11, DR-10) ----

impl World {
    fn tip_block(&self) -> String {
        self.signer
            .writes
            .tip(&self.repo, "heads/main")
            .unwrap()
            .expect("a block")
            .block_id
    }
}

#[test]
fn a_seal_by_the_old_key_after_a_default_rotation_is_reported_with_its_block_and_earlier_seals_are_not()
 {
    let Some(w) = world("late-seal") else { return };
    // A change sealed by the first keys before anything is stolen or rotated: history the old key really made.
    w.seal_with_current_keys("one\n");
    let loot = dir(Path::new("/dev/shm"), "late-loot");
    let stolen = w.steal_seeds(&loot.0);
    let first = w.signer.key_status(&w.wiki).unwrap();
    // Never rotated: nothing superseded, nothing to report.
    assert!(w.signer.superseded_keys(&w.wiki).unwrap().is_empty());
    assert_eq!(w.signer.seals_by_superseded_keys(&w.repo).unwrap(), vec![]);

    w.signer
        .rotate_keys(&crate::KeyAuthority::for_tests(), &w.repo, OldKey::Keep)
        .unwrap();
    // The rotation recorded where the old key stopped: one view, and the key is still trusted.
    let superseded = w.signer.superseded_keys(&w.wiki).unwrap();
    assert_eq!(superseded.len(), 1);
    assert_eq!(superseded.first().unwrap().generation, 1);
    assert_eq!(
        superseded.first().unwrap().maintainer_key_id,
        first.maintainer_key_id
    );
    assert!(!superseded.first().unwrap().retired);
    assert_eq!(superseded.first().unwrap().views_recorded, 1);
    assert_eq!(w.signer.seals_by_superseded_keys(&w.repo).unwrap(), vec![]);

    // The service seals with the new keys: nothing is reported for those.
    w.seal_with_current_keys("two\n");
    assert_eq!(w.signer.seals_by_superseded_keys(&w.repo).unwrap(), vec![]);

    // A thief with the old seed seals: reported, with the block and the key.
    assert_eq!(w.seal_with_stolen(&loot.0, &stolen), None);
    let forged = w.tip_block();
    let late = w.signer.seals_by_superseded_keys(&w.repo).unwrap();
    assert_eq!(
        late,
        vec![LateSeal {
            generation: 1,
            maintainer_key_id: first.maintainer_key_id.clone(),
            block_id: forged.clone(),
        }]
    );
    assert!(late.first().unwrap().to_string().contains(&forged));
    // The repository still verifies (that is the cost of keeping the key) and the report does not change it.
    assert!(w.verify().0);
    // The service goes on sealing with its new keys; the report stays exactly the one block.
    w.seal_with_current_keys("three\n");
    assert_eq!(w.signer.seals_by_superseded_keys(&w.repo).unwrap(), late);
}

#[test]
fn a_second_rotation_does_not_make_what_the_first_keys_sealed_in_between_late() {
    let Some(w) = world("late-twice") else { return };
    let loot = dir(Path::new("/dev/shm"), "late-twice-loot");
    w.signer
        .rotate_keys(&crate::KeyAuthority::for_tests(), &w.repo, OldKey::Keep)
        .unwrap();
    // The second pair seals one change, is stolen, and is then rotated away too.
    w.seal_with_current_keys("by the second pair\n");
    let stolen_second = w.steal_seeds(&loot.0);
    w.signer
        .rotate_keys(&crate::KeyAuthority::for_tests(), &w.repo, OldKey::Keep)
        .unwrap();
    assert_eq!(w.signer.superseded_keys(&w.wiki).unwrap().len(), 2);
    // What the second pair sealed before its replacement is history up to its recorded point: not late.
    assert_eq!(w.signer.seals_by_superseded_keys(&w.repo).unwrap(), vec![]);
    // After it, the second pair's key seals: late, and attributed to generation 2 only.
    assert_eq!(w.seal_with_stolen(&loot.0, &stolen_second), None);
    let late = w.signer.seals_by_superseded_keys(&w.repo).unwrap();
    assert_eq!(late.len(), 1);
    assert_eq!(late.first().unwrap().generation, 2);
    assert_eq!(late.first().unwrap().block_id, w.tip_block());
}

#[test]
fn a_retirement_records_the_point_too_and_marks_the_key_retired() {
    let Some(w) = world("late-retire") else {
        return;
    };
    w.signer
        .rotate_keys(&crate::KeyAuthority::for_tests(), &w.repo, OldKey::Retire)
        .unwrap();
    let superseded = w.signer.superseded_keys(&w.wiki).unwrap();
    assert_eq!(superseded.len(), 1);
    assert!(
        superseded.first().unwrap().retired,
        "the record says the key was retired"
    );
    assert_eq!(superseded.first().unwrap().views_recorded, 1);
    // The sealed block, its ref state and its ref update: what `verify` named just after the removal (P2′).
    assert_eq!(superseded.first().unwrap().findings_recorded, 3);
    // After a retirement prikk cannot verify the old history (ledger PK-34) and prints no list of who sealed
    // what (ledger PK-39), so the check cannot run: it says so and never answers "nothing found" from nothing.
    // It does not need to: the retired key can seal nothing more.
    let r = w.signer.seals_by_superseded_keys(&w.repo);
    assert!(
        matches!(&r, Err(SignerError::Refused(why)) if why.contains("publication-trust")),
        "{r:?}"
    );
}

#[test]
fn a_retirement_recorded_without_findings_excuses_nothing() {
    // P2′: a record from before the set was kept (format 2 opens as a retired key with no findings) excuses nothing,
    // even when the failure is, in fact, that key's.
    let Some(w) = world("retire-old-format") else {
        return;
    };
    w.signer
        .rotate_keys(&crate::KeyAuthority::for_tests(), &w.repo, OldKey::Retire)
        .unwrap();
    assert!(
        matches!(w.signer.retirement_explains(&w.repo, &w.wiki), Ok(Some(_))),
        "the same failure is excused by the record that holds it"
    );
    let mut meta = w.signer.store.status(&w.wiki).unwrap();
    meta.superseded.last_mut().unwrap().excused = Excused::default();
    let seeds = crate::key_store::Seeds::new(
        format!("{}\n", "1".repeat(64)).as_bytes(),
        format!("{}\n", "2".repeat(64)).as_bytes(),
    )
    .unwrap();
    w.signer.store.replace(&meta, &seeds).unwrap();
    assert_eq!(
        w.signer.retirement_explains(&w.repo, &w.wiki),
        Ok(None),
        "a retirement with no recorded findings excuses nothing"
    );
}

#[test]
fn a_verdict_and_a_text_that_disagree_conclude_nothing() {
    // P14: the verdict (JSON) and the findings (text) are one look at `verify`. A prikk whose text says "verifies"
    // where its verdict says "fails" is not read from: nothing is concluded and nothing is excused.
    let Some(w) = world("retire-disagree") else {
        return;
    };
    w.signer
        .rotate_keys(&crate::KeyAuthority::for_tests(), &w.repo, OldKey::Retire)
        .unwrap();
    let standin = w._tmp.0.join("prikk-disagrees");
    std::fs::write(
        &standin,
        format!(
            "#!/bin/sh\nif [ \"$1\" = verify ] && [ $# -eq 1 ]; then {} verify; exit 0; fi\nexec {} \"$@\"\n",
            w.prikk.display(),
            w.prikk.display()
        ),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&standin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let signer = w.signer_with_prikk(&standin);
    let r = signer.retirement_explains(&w.repo, &w.wiki);
    assert!(
        matches!(&r, Err(SignerError::Unavailable(why)) if why.contains("same answer")),
        "{r:?}"
    );
    // The same prikk without the disguise: excused.
    assert!(matches!(
        w.signer.retirement_explains(&w.repo, &w.wiki),
        Ok(Some(_))
    ));
}

#[test]
fn the_text_of_prikks_verify_is_read_strictly() {
    let block = "ab".repeat(32);
    let text = format!(
        "verified repository: /x\nsealed blocks: 2\nsealed-block {block}: inklkunn-w-maintainer-2-0a1b2c3d\n\
         sealed-block {}: \nsealed-block {block}x: key\nsealed-block {block}: bad key\nsealed-block short: k\n\
         sealed-blocks: {block}: k\n",
        "cd".repeat(32)
    );
    assert_eq!(
        crate::writes::parse_sealed_blocks(&text),
        vec![
            (block.clone(), "inklkunn-w-maintainer-2-0a1b2c3d".to_owned()),
            ("cd".repeat(32), String::new()),
        ]
    );
}

use crate::signer::retirement::{RetirementAbandoned, RetirementFinished};

// ---- a retirement never strands a wiki (handoff 004, Q1; Q3, Q4, Q7)

impl World {
    /// A `prikk` that does the real thing and misbehaves on cue, by the files named in the returned directory:
    /// `remove-fails` (the removal of a key from trust fails), `lock-the-key-store` (the key directory stops being
    /// writable once a key was removed), and, once a key was removed, `verify-dies` (it cannot run), `disagree` (its
    /// text says the repository verifies where its verdict says it fails), `count-differs` (the text counts more
    /// records than before); at any time `count-missing` (the text prints no count), `duplicate-block` (the text lists
    /// the first block twice, with another signer), `hide-signers` (the text lists no `sealed-block` line), `extra-finding` (the text names a finding of another condition) and `halted` (the verdict is that of a run that stopped early).
    fn faulty(&self) -> (PathBuf, Signer) {
        let ctl = self._tmp.0.join("ctl");
        std::fs::create_dir_all(&ctl).unwrap();
        let script = format!(
            r#"#!/bin/sh
CTL="{ctl}"
REAL="{real}"
if [ "$1 $2 $3" = "trust maintainer remove" ]; then
  if [ -e "$CTL/remove-fails" ]; then echo 'error: precondition not met: injected' >&2; exit 1; fi
  "$REAL" "$@"; rc=$?
  touch "$CTL/removed"
  if [ -e "$CTL/lock-the-key-store" ]; then chmod 500 "{keys}"; fi
  exit $rc
fi
if [ "$1" = verify ]; then
  if [ -e "$CTL/removed" ] && [ -e "$CTL/verify-dies" ]; then echo 'error: injected' >&2; exit 2; fi
  if [ $# -eq 1 ]; then
    out=$("$REAL" verify 2>/dev/null); rc=$?
    if [ -e "$CTL/removed" ] && [ -e "$CTL/disagree" ]; then echo 'verified repository: x'; exit 0; fi
    if [ -e "$CTL/removed" ] && [ -e "$CTL/count-missing-after" ]; then printf '%s\n' "$out" | grep -v '^checked publication trust records:'; exit $rc; fi
    if [ -e "$CTL/removed" ] && [ -e "$CTL/count-differs" ]; then
      printf '%s\n' "$out" | sed 's/^checked publication trust records: .*/checked publication trust records: 99/'; exit $rc
    fi
    if [ -e "$CTL/count-missing" ]; then printf '%s\n' "$out" | grep -v '^checked publication trust records:'; exit $rc; fi
    if [ -e "$CTL/duplicate-block" ]; then
      printf '%s\n' "$out"
      first=$(printf '%s\n' "$out" | grep '^sealed-block ' | head -1); b=${{first#sealed-block }}; b=${{b%%:*}}
      echo "sealed-block $b: someone-else"; exit $rc
    fi
    if [ -e "$CTL/hide-signers" ]; then printf '%s\n' "$out" | grep -v '^sealed-block '; exit $rc; fi
    if [ -e "$CTL/extra-finding" ]; then
      printf '%s\n' "$out"; echo 'ref-publication [PRIKK-VERIFY-INJECTED]: appeared between the two runs'; exit $rc
    fi
    printf '%s\n' "$out"; exit $rc
  fi
  if [ -e "$CTL/halted" ]; then "$REAL" "$@" | sed '0,/"evaluated"/s//"halted"/'; exit 0; fi
fi
exec "$REAL" "$@"
"#,
            ctl = ctl.display(),
            real = self.prikk.display(),
            keys = self.key_dir.display()
        );
        let path = self._tmp.0.join("prikk-faulty");
        std::fs::write(&path, script).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        (ctl, self.signer_with_prikk(&path))
    }

    fn retire(&self, signer: &Signer) -> Rotation {
        signer
            .rotate_keys(
                &crate::KeyAuthority::for_tests(),
                &self.repo,
                OldKey::Retire,
            )
            .expect("the rotation happened")
    }

    fn finish(&self, signer: &Signer) -> Result<RetirementFinished, SignerError> {
        signer.finish_retirement(&crate::KeyAuthority::for_tests(), &self.repo)
    }

    /// The key record's plain facts.
    fn facts(&self) -> crate::key_store::Meta {
        self.signer.store.status(&self.wiki).unwrap()
    }

    /// The wiki is as a retirement that ended without its record leaves it: the key is out of trust, nothing is
    /// excused, the record holds the state to finish from, and `rotation` says so and names the command.
    fn assert_unfinished(&self, rotation: &Rotation) {
        assert!(
            matches!(&rotation.old_key, OldKeyOutcome::RetiredUnrecorded(why)
                if why.contains(&finish_retirement_command(&self.wiki))),
            "{rotation:?}"
        );
        assert!(!self.verify().0, "the key is out of the trust store");
        let facts = self.facts();
        let retiring = facts.retiring.as_ref().expect("the state to finish from");
        assert_eq!(
            retiring.maintainer_key_id,
            rotation.previous_maintainer_key_id
        );
        let newest = facts.superseded.last().unwrap();
        assert!(!newest.retired && newest.excused.is_empty());
        assert_eq!(
            self.signer.retirement_explains(&self.repo, &self.wiki),
            Ok(None),
            "nothing is excused"
        );
        assert_eq!(
            self.signer.unfinished_retirement(&self.wiki).unwrap(),
            Some(rotation.previous_maintainer_key_id.clone())
        );
    }

    /// The finished retirement: three findings (the block, its ref state and its ref update), recorded, the state
    /// cleared, and the failure is excused.
    fn assert_finished(&self, done: &RetirementFinished, rotation: &Rotation) {
        assert_eq!(done.maintainer_key_id, rotation.previous_maintainer_key_id);
        assert_eq!(done.findings_recorded, 3);
        let facts = self.facts();
        assert!(facts.retiring.is_none());
        let newest = facts.superseded.last().unwrap();
        assert!(newest.retired);
        assert_eq!(newest.excused.len(), 3);
        assert_eq!(self.signer.unfinished_retirement(&self.wiki).unwrap(), None);
        assert!(
            matches!(
                self.signer.retirement_explains(&self.repo, &self.wiki),
                Ok(Some(_))
            ),
            "the failure is the retirement's, and is excused"
        );
        // ... and a rotation is possible again.
        assert!(
            self.signer
                .rotate_keys(&crate::KeyAuthority::for_tests(), &self.repo, OldKey::Keep)
                .is_ok()
        );
    }
}

/// A test's lower limit for the findings a retirement may record, put back when the test is over.
struct RecordLimit;

impl RecordLimit {
    fn set(n: usize) -> Self {
        super::super::retirement::RECORD_LIMIT.with(|c| c.set(Some(n)));
        RecordLimit
    }
}

impl Drop for RecordLimit {
    fn drop(&mut self) {
        super::super::retirement::RECORD_LIMIT.with(|c| c.set(None));
    }
}

#[test]
fn each_way_a_retirement_can_end_without_its_record_ends_in_a_state_finish_retirement_finishes() {
    // (the control file, what it does to the retirement). In each the key IS removed from trust.
    let ways = [
        "verify-dies",
        "disagree",
        "count-differs",
        "count-missing-after",
        "lock-the-key-store",
        "record-limit",
    ];
    for way in ways {
        let Some(w) = world(&format!("q1-{way}")) else {
            return;
        };
        let (ctl, signer) = w.faulty();
        let limit = (way == "record-limit").then(|| RecordLimit::set(2));
        if way != "record-limit" {
            std::fs::write(ctl.join(way), "").unwrap();
        }
        let rotation = w.retire(&signer);
        drop(limit);
        w.assert_unfinished(&rotation);
        // The cause goes away (the key directory is writable again; the limit and the faults are not in force), and
        // the operation finishes the record from the state the retirement kept.
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&w.key_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let done = w.finish(&w.signer).unwrap_or_else(|e| panic!("{way}: {e}"));
        w.assert_finished(&done, &rotation);
    }
}

#[test]
fn finishing_is_refused_while_the_cause_is_still_there_and_changes_nothing() {
    let Some(w) = world("q1-still-faulty") else {
        return;
    };
    let (ctl, signer) = w.faulty();
    std::fs::write(ctl.join("verify-dies"), "").unwrap();
    let rotation = w.retire(&signer);
    w.assert_unfinished(&rotation);
    let before = w.facts();
    // The same prikk, still failing: refused, with the reason, and the state is untouched.
    let r = w.finish(&signer);
    assert!(
        matches!(&r, Err(SignerError::Refused(why)) if why.contains("cannot be finished") && why.contains("could not be run")),
        "{r:?}"
    );
    assert_eq!(w.facts(), before);
    // A text and a verdict that disagree, and a count that changed, are refused the same way.
    std::fs::remove_file(ctl.join("verify-dies")).unwrap();
    for (flag, said) in [("disagree", "same answer"), ("count-differs", "published")] {
        std::fs::write(ctl.join(flag), "").unwrap();
        let r = w.finish(&signer);
        assert!(
            matches!(&r, Err(SignerError::Refused(why)) if why.contains(said)),
            "{flag}: {r:?}"
        );
        assert_eq!(w.facts(), before, "{flag}");
        std::fs::remove_file(ctl.join(flag)).unwrap();
    }
    // With nothing in the way it is finished, by the very same signer.
    let done = w.finish(&signer).unwrap();
    w.assert_finished(&done, &rotation);
}

#[test]
fn a_write_after_the_failure_makes_finish_retirement_refuse_and_says_why() {
    let Some(w) = world("q1-written-since") else {
        return;
    };
    let (ctl, signer) = w.faulty();
    std::fs::write(ctl.join("verify-dies"), "").unwrap();
    let rotation = w.retire(&signer);
    w.assert_unfinished(&rotation);
    // A change is sealed meanwhile (with the new keys): nothing is excused, so it is sealed and cannot be confirmed.
    w.seal_with_current_keys("written while the retirement is unfinished\n");
    let before = w.facts();
    let r = w.finish(&w.signer);
    assert!(
        matches!(&r, Err(SignerError::Refused(why))
            if why.contains("written") && why.contains("has changed") && why.contains("nothing was changed")),
        "{r:?}"
    );
    assert_eq!(w.facts(), before, "nothing was changed");
    assert!(w.facts().retiring.is_some());
    // The wiki's own answers name the way out, whatever the next step turns out to be.
    assert!(
        w.signer
            .unfinished_retirement_note(&w.wiki)
            .unwrap()
            .contains(&finish_retirement_command(&w.wiki))
    );
}

#[test]
fn nothing_else_is_done_to_the_keys_while_a_retirement_is_unfinished() {
    let Some(w) = world("q1-no-rotation") else {
        return;
    };
    let (ctl, signer) = w.faulty();
    std::fs::write(ctl.join("verify-dies"), "").unwrap();
    let rotation = w.retire(&signer);
    w.assert_unfinished(&rotation);
    let before = w.facts();
    for old in [OldKey::Keep, OldKey::Retire] {
        let r = w
            .signer
            .rotate_keys(&crate::KeyAuthority::for_tests(), &w.repo, old);
        assert!(
            matches!(&r, Err(SignerError::Refused(why)) if why.contains(&finish_retirement_command(&w.wiki))),
            "{old:?}: {r:?}"
        );
        assert_eq!(w.facts(), before);
    }
    // And with nothing unfinished there is nothing to finish.
    let Some(other) = world("q1-nothing-to-finish") else {
        return;
    };
    let r = other.finish(&other.signer);
    assert!(
        matches!(&r, Err(SignerError::Refused(why)) if why.contains("no key retirement")),
        "{r:?}"
    );
}

#[test]
fn a_removal_that_failed_keeps_the_state_and_finishing_does_the_removal_itself() {
    // W2: the key is still trusted (the removal failed, as a kill before the untrust leaves it). `finish-retirement`
    // takes a fresh look, removes the key itself and records what the removal made fail; nothing tells the operator
    // to run prikk by hand.
    let Some(w) = world("w2-remove-fails") else {
        return;
    };
    let (ctl, signer) = w.faulty();
    std::fs::write(ctl.join("remove-fails"), "").unwrap();
    let rotation = w.retire(&signer);
    let OldKeyOutcome::RetireFailed(said) = &rotation.old_key else {
        panic!("{rotation:?}");
    };
    assert!(w.verify().0, "the key is still trusted");
    assert!(w.facts().retiring.is_some());
    assert!(!said.contains("prikk trust maintainer"), "{said}");
    // While the removal keeps failing, finishing says so, keeps the state, and names both ways out.
    let before = w.facts();
    let r = w.finish(&signer);
    assert!(
        matches!(&r, Err(SignerError::Refused(why))
            if why.contains("removing the key") && why.contains(&finish_retirement_command(&w.wiki))
                && why.contains(&abandon_retirement_command(&w.wiki))
                && !why.contains("prikk trust maintainer")),
        "{r:?}"
    );
    assert!(w.verify().0, "the key is still trusted");
    // The state was taken again (it is the same repository, so it is the same state).
    assert_eq!(w.facts().retiring, before.retiring);
    // Once the removal can be done, the command does it, and records the rest.
    std::fs::remove_file(ctl.join("remove-fails")).unwrap();
    let done = w.finish(&signer).unwrap();
    assert!(done.removed_now);
    w.assert_finished(&done, &rotation);
    assert!(
        !w.verify().0,
        "the key was removed from the trust store by the command"
    );
}

#[test]
fn finishing_with_the_key_still_trusted_retakes_the_look_and_does_not_need_the_views_unchanged() {
    // W2: with the key still trusted the state is written again from a fresh `verify`, so a block written since (here
    // with the new keys, outside the signer's acceptance) does not stop the finish, and is not the old key's.
    let Some(w) = world("w2-fresh") else {
        return;
    };
    let (ctl, signer) = w.faulty();
    std::fs::write(ctl.join("remove-fails"), "").unwrap();
    let rotation = w.retire(&signer);
    let stale = w.facts().retiring.unwrap();
    w.seal_with_current_keys("written while the retirement was unfinished\n");
    // The removal still fails: the command refuses, and what it leaves in the key record is the state taken again from
    // the look it took now (a kill here must leave something `finish-retirement` can go on from), not the stale one.
    let r = w.finish(&signer);
    assert!(matches!(&r, Err(SignerError::Refused(_))), "{r:?}");
    let renewed = w.facts().retiring.unwrap();
    assert_ne!(renewed.views, stale.views, "the views are those of now");
    assert_ne!(
        renewed.checked_records, stale.checked_records,
        "the count is the one of now"
    );
    assert_eq!(renewed.views, w.signer.current_tips(&w.repo).unwrap());
    std::fs::remove_file(ctl.join("remove-fails")).unwrap();
    let done = w.finish(&signer).unwrap();
    assert!(done.removed_now);
    assert_eq!(
        done.findings_recorded, 3,
        "the old key's block, ref state and ref update"
    );
    w.assert_finished(&done, &rotation);
    // The retirement was begun again from the look taken now: its counts are not the stale state's.
    assert!(
        stale.views != w.signer.current_tips(&w.repo).unwrap(),
        "the views moved since the stale state"
    );
}

#[test]
fn finishing_with_the_key_still_trusted_needs_a_repository_that_verifies_and_changes_nothing_if_it_does_not()
 {
    // W2: the fresh look must pass, as `rotate` requires. A `verify` that does not run to the end is not a repository
    // known to verify: the key stays trusted, the state stays, and the refusal names both ways out.
    let Some(w) = world("w2-must-pass") else {
        return;
    };
    let (ctl, signer) = w.faulty();
    std::fs::write(ctl.join("remove-fails"), "").unwrap();
    let rotation = w.retire(&signer);
    std::fs::remove_file(ctl.join("remove-fails")).unwrap();
    std::fs::write(ctl.join("halted"), "").unwrap();
    let before = w.facts();
    let r = w.finish(&signer);
    assert!(
        matches!(&r, Err(SignerError::NeedsRecovery(why))
            if why.contains("did not run to the end") && why.contains("still trusted")
                && why.contains(&finish_retirement_command(&w.wiki))
                && why.contains(&abandon_retirement_command(&w.wiki))),
        "{r:?}"
    );
    assert_eq!(w.facts(), before);
    assert!(w.verify().0, "the key was not removed");
    std::fs::remove_file(ctl.join("halted")).unwrap();
    let done = w.finish(&signer).unwrap();
    w.assert_finished(&done, &rotation);
}

#[test]
fn the_trusted_keys_must_be_the_ones_the_retirement_began_with_less_the_retired_key() {
    // W5: a maintainer key added or removed outside inklkunn while the retirement is unfinished would have its blocks,
    // ref states and ref updates recorded under this retirement. Finishing requires the trust list to be exactly the
    // kept list without the retired key; otherwise it refuses, changes nothing, and names `abandon-retirement`.
    for removed in [false, true] {
        let Some(w) = world(&format!("w5-trust-list-{removed}")) else {
            return;
        };
        // A second key, adopted by hand before the retirement is begun (so it is in the kept list).
        w.adopt_other_key("second-maintainer");
        let (ctl, signer) = w.faulty();
        std::fs::write(ctl.join("verify-dies"), "").unwrap();
        let rotation = w.retire(&signer);
        w.assert_unfinished(&rotation);
        let kept = w.facts().retiring.unwrap().trusted_maintainers;
        assert!(kept.contains(&"second-maintainer".to_owned()), "{kept:?}");
        std::fs::remove_file(ctl.join("verify-dies")).unwrap();
        if removed {
            // ... and then removed by hand in the window.
            let o = w.prikk(&[
                "trust",
                "maintainer",
                "remove",
                "--key-id",
                "second-maintainer",
            ]);
            assert!(o.status.success());
        } else {
            w.adopt_other_key("stranger-maintainer");
        }
        let before = w.facts();
        let r = w.finish(&signer);
        assert!(
            matches!(&r, Err(SignerError::Refused(why))
                if why.contains("trusted maintainer keys")
                    && why.contains("outside inklkunn")
                    && why.contains(&abandon_retirement_command(&w.wiki))
                    && why.contains("nothing was changed")),
            "removed={removed}: {r:?}"
        );
        assert_eq!(w.facts(), before, "removed={removed}");
        // The way out the refusal names works: the old key is trusted again, the state is gone.
        let done = w.abandon(&signer).unwrap();
        assert!(done.restored);
        assert!(w.facts().retiring.is_none());
        assert!(w.verify().0, "the history verifies again");
    }
}

#[test]
fn a_state_of_the_format_before_has_no_list_and_can_still_be_finished_but_not_abandoned_after_the_removal()
 {
    // The format before W3 and W5 kept neither the key's public key nor the list. Its state opens; finishing does not
    // compare a list that was not kept (and says nothing else is different); abandoning a key already removed would
    // need the public key, so it is refused naming `finish-retirement`.
    for finish_first in [true, false] {
        let Some(w) = world(&format!("w3-legacy-{finish_first}")) else {
            return;
        };
        let (ctl, signer) = w.faulty();
        std::fs::write(ctl.join("verify-dies"), "").unwrap();
        let rotation = w.retire(&signer);
        w.assert_unfinished(&rotation);
        std::fs::remove_file(ctl.join("verify-dies")).unwrap();
        w.signer
            .store
            .rewrite_as_format_four(&w.wiki, |m| {
                let r = m.retiring.as_mut().unwrap();
                r.maintainer_public_key = String::new();
                r.trusted_maintainers = Vec::new();
            })
            .unwrap();
        assert!(w.facts().retiring.unwrap().maintainer_public_key.is_empty());
        let before = w.facts();
        if finish_first {
            let done = w.finish(&signer).unwrap();
            w.assert_finished(&done, &rotation);
        } else {
            let r = w.abandon(&signer);
            assert!(
                matches!(&r, Err(SignerError::Refused(why))
                    if why.contains("older build") && why.contains(&finish_retirement_command(&w.wiki))),
                "{r:?}"
            );
            assert_eq!(w.facts(), before);
        }
    }
}

// ---- giving a retirement up (handoff 004, W3)

impl World {
    fn abandon(&self, signer: &Signer) -> Result<RetirementAbandoned, SignerError> {
        signer.abandon_retirement(&crate::KeyAuthority::for_tests(), &self.repo)
    }

    /// A key adopted in the repository by hand, as a stranger's (or another operator's) would be: its id and
    /// nothing else of it is kept. Returns its public key.
    fn adopt_other_key(&self, id: &str) -> String {
        let seed = self._tmp.0.join(format!("{id}.seed"));
        let made = Command::new(&self.prikk)
            .args(["key", "generate", "--out"])
            .arg(&seed)
            .current_dir(&self.repo)
            .output()
            .unwrap();
        assert!(made.status.success());
        let public = Command::new(&self.prikk)
            .args(["key", "public", "--seed-file"])
            .arg(&seed)
            .current_dir(&self.repo)
            .output()
            .unwrap();
        let public = String::from_utf8(public.stdout)
            .unwrap()
            .lines()
            .find_map(|l| l.strip_prefix("public key: "))
            .unwrap()
            .trim()
            .to_owned();
        let added = self.prikk(&[
            "trust",
            "maintainer",
            "add",
            "--key-id",
            id,
            "--public-key",
            &public,
        ]);
        assert!(added.status.success());
        public
    }

    /// The maintainer key ids the repository trusts, sorted.
    fn trusted_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .signer
            .writes
            .trusted_maintainers(&self.repo)
            .unwrap()
            .into_iter()
            .map(|k| k.key_id)
            .collect();
        ids.sort();
        ids
    }
}

#[test]
fn giving_up_a_retirement_whose_key_is_still_trusted_clears_the_state_and_the_rotation_stands() {
    let Some(w) = world("w3-still-trusted") else {
        return;
    };
    let (ctl, signer) = w.faulty();
    std::fs::write(ctl.join("remove-fails"), "").unwrap();
    let rotation = w.retire(&signer);
    assert!(matches!(&rotation.old_key, OldKeyOutcome::RetireFailed(_)));
    let trusted = w.trusted_ids();
    let generation = w.facts().generation;
    let done = w.abandon(&signer).unwrap();
    assert_eq!(done.maintainer_key_id, rotation.previous_maintainer_key_id);
    assert!(!done.restored, "it had never left the trust store");
    let facts = w.facts();
    assert!(facts.retiring.is_none());
    assert_eq!(facts.generation, generation, "the rotation stands");
    let newest = facts.superseded.last().unwrap();
    assert!(
        !newest.retired && newest.excused.is_empty(),
        "as a rotation that kept the old key"
    );
    assert_eq!(w.trusted_ids(), trusted, "the trust list is as it was");
    assert!(w.verify().0);
    assert_eq!(w.signer.unfinished_retirement(&w.wiki).unwrap(), None);
    assert_eq!(w.signer.seal_pause(&w.wiki), None);
    // Nothing is waiting any more.
    let r = w.abandon(&signer);
    assert!(
        matches!(&r, Err(SignerError::Refused(why)) if why.contains("no key retirement")),
        "{r:?}"
    );
}

#[test]
fn giving_up_a_retirement_whose_key_was_removed_adopts_it_again_under_its_own_id_and_key() {
    for way in ["verify-dies", "count-differs"] {
        let Some(w) = world(&format!("w3-removed-{way}")) else {
            return;
        };
        let old_public = w
            .signer
            .writes
            .trusted_maintainers(&w.repo)
            .unwrap()
            .into_iter()
            .find(|k| k.key_id == w.facts().maintainer_key_id)
            .unwrap()
            .public_key;
        let (ctl, signer) = w.faulty();
        std::fs::write(ctl.join(way), "").unwrap();
        let rotation = w.retire(&signer);
        w.assert_unfinished(&rotation);
        assert!(!w.verify().0, "{way}: the key is out of the trust store");
        let old = rotation.previous_maintainer_key_id.clone();
        assert!(!w.trusted_ids().contains(&old));
        let state = w.facts().retiring.unwrap();
        assert_eq!(
            state.maintainer_public_key, old_public,
            "the public key of the old key was kept"
        );
        let done = w.abandon(&w.signer).unwrap();
        assert!(done.restored, "{way}");
        assert_eq!(done.maintainer_key_id, old);
        assert!(w.trusted_ids().contains(&old));
        let again = w
            .signer
            .writes
            .trusted_maintainers(&w.repo)
            .unwrap()
            .into_iter()
            .find(|k| k.key_id == old)
            .unwrap();
        assert_eq!(
            again.public_key, old_public,
            "{way}: adopted with the public key it had"
        );
        assert!(w.facts().retiring.is_none());
        assert!(
            w.verify().0,
            "{way}: what the old key sealed verifies again"
        );
        let newest = w.facts().superseded.last().cloned().unwrap();
        assert!(!newest.retired && newest.excused.is_empty(), "{way}");
        assert_eq!(w.signer.seal_pause(&w.wiki), None);
        assert_eq!(
            w.signer.retirement_explains(&w.repo, &w.wiki),
            Ok(None),
            "nothing is excused, and the repository verifies"
        );
        // A rotation is possible again (and so is a new retirement).
        assert!(
            w.signer
                .rotate_keys(&crate::KeyAuthority::for_tests(), &w.repo, OldKey::Keep)
                .is_ok()
        );
    }
}

#[test]
fn a_refusal_no_later_run_can_cure_names_abandon_retirement_and_one_a_later_run_can_do_not_need_it()
{
    // W3: a count that differs (something was published in between) and a wiki written outside inklkunn are refusals
    // no later run can cure; they name `abandon-retirement`. `verify` that cannot run is one a later run can cure: it
    // says to run `finish-retirement` again.
    let Some(w) = world("w3-incurable-count") else {
        return;
    };
    let (ctl, signer) = w.faulty();
    std::fs::write(ctl.join("verify-dies"), "").unwrap();
    let rotation = w.retire(&signer);
    w.assert_unfinished(&rotation);
    let OldKeyOutcome::RetiredUnrecorded(first) = &rotation.old_key else {
        panic!("{rotation:?}");
    };
    assert!(
        first.contains(&finish_retirement_command(&w.wiki))
            && first.contains("once the cause is gone"),
        "{first}"
    );
    assert!(!first.contains("no later run"), "{first}");
    std::fs::remove_file(ctl.join("verify-dies")).unwrap();
    // The cause is now one that cannot be cured.
    std::fs::write(ctl.join("count-differs"), "").unwrap();
    let r = w.finish(&signer);
    assert!(
        matches!(&r, Err(SignerError::Refused(why))
            if why.contains("published in between") && why.contains("no later run")
                && why.contains(&abandon_retirement_command(&w.wiki))),
        "{r:?}"
    );
    // The repository changed outside inklkunn.
    let Some(w) = world("w3-incurable-written") else {
        return;
    };
    let (ctl, signer) = w.faulty();
    std::fs::write(ctl.join("verify-dies"), "").unwrap();
    w.retire(&signer);
    w.seal_with_current_keys(
        "written outside
",
    );
    let r = w.finish(&w.signer);
    assert!(
        matches!(&r, Err(SignerError::Refused(why))
            if why.contains("has changed") && why.contains(&abandon_retirement_command(&w.wiki)) && why.contains("no later run")),
        "{r:?}"
    );
}

#[test]
fn a_finish_that_would_leave_blocks_out_records_nothing_keeps_the_state_and_names_abandon() {
    // W4: the verify before listed nobody as the retired key's signer (the shim hides the lines), so the block the
    // removal made fail would not be the key's as far as the record knows. A record that left it out would look
    // complete and would not be (the wiki stops on it): nothing is recorded, the state stays, and the way out is
    // `abandon-retirement`. Inline (`rotate`) and through `finish-retirement` alike.
    for via_finish in [false, true] {
        let Some(w) = world(&format!("w4-left-out-{via_finish}")) else {
            return;
        };
        let (ctl, signer) = w.faulty();
        std::fs::write(ctl.join("hide-signers"), "").unwrap();
        if via_finish {
            std::fs::write(ctl.join("verify-dies"), "").unwrap();
        }
        let rotation = w.retire(&signer);
        let abandon = abandon_retirement_command(&w.wiki);
        let finish_text = if via_finish {
            w.assert_unfinished(&rotation);
            std::fs::remove_file(ctl.join("verify-dies")).unwrap();
            let before = w.facts();
            let r = w.finish(&signer);
            assert_eq!(w.facts(), before, "nothing was recorded");
            match r {
                Err(SignerError::Refused(why)) => why,
                other => panic!("{other:?}"),
            }
        } else {
            let OldKeyOutcome::RetiredUnrecorded(why) = &rotation.old_key else {
                panic!("{rotation:?}");
            };
            why.clone()
        };
        assert!(
            finish_text.contains("were not sealed by the retired key")
                && finish_text.contains(&abandon),
            "via_finish={via_finish}: {finish_text}"
        );
        let facts = w.facts();
        assert!(facts.retiring.is_some(), "the state is kept");
        let newest = facts.superseded.last().unwrap();
        assert!(
            !newest.retired && newest.excused.is_empty(),
            "nothing was recorded"
        );
        assert_eq!(w.signer.retirement_explains(&w.repo, &w.wiki), Ok(None));
        // The way out works.
        let done = w.abandon(&signer).unwrap();
        assert!(done.restored);
        assert!(w.verify().0);
    }
}

#[test]
fn a_retirement_is_paused_not_hidden_the_signer_names_the_command_while_the_state_is_set() {
    // W1 at the signer: `seal_pause` is the one sentence, naming the command and the key, from the key record.
    let Some(w) = world("w1-sentence") else {
        return;
    };
    assert_eq!(w.signer.seal_pause(&w.wiki), None);
    let (ctl, signer) = w.faulty();
    std::fs::write(ctl.join("remove-fails"), "").unwrap();
    let rotation = w.retire(&signer);
    let sentence = w.signer.seal_pause(&w.wiki).expect("a pause");
    assert!(
        sentence.contains(&finish_retirement_command(&w.wiki)),
        "{sentence}"
    );
    assert!(sentence.contains(&rotation.previous_maintainer_key_id));
    assert!(sentence.contains("nothing was written"));
    // One sentence: no line break, one full stop at the end.
    assert!(!sentence.contains('\n'));
    std::fs::remove_file(ctl.join("remove-fails")).unwrap();
    let done = w.finish(&signer);
    assert!(done.is_ok(), "{done:?}");
    assert_eq!(w.signer.seal_pause(&w.wiki), None);
}

#[test]
fn a_retirement_is_made_only_from_a_repository_whose_look_is_complete() {
    // Q4 and Q7: the count must be printed, a block must be listed once, and the run must be a whole one. Each
    // refuses the retirement before anything is changed.
    for (flag, said) in [
        ("count-missing", "how many publication trust records"),
        ("duplicate-block", "more than once"),
        ("halted", "did not run to the end"),
    ] {
        let Some(w) = world(&format!("q7-{flag}")) else {
            return;
        };
        let (ctl, signer) = w.faulty();
        std::fs::write(ctl.join(flag), "").unwrap();
        let before = w.facts();
        let r = signer.rotate_keys(&crate::KeyAuthority::for_tests(), &w.repo, OldKey::Retire);
        assert!(
            matches!(&r, Err(SignerError::NeedsRecovery(why)) if why.contains(said) && why.contains("still trusted")),
            "{flag}: {r:?}"
        );
        assert_eq!(w.facts(), before, "{flag}: nothing was changed");
        assert!(w.verify().0, "{flag}: the key is still trusted");
        // Keeping the old key does not need the look, and is not refused by it.
        std::fs::remove_file(ctl.join(flag)).unwrap();
    }
}

#[test]
fn the_text_of_a_verify_that_names_a_finding_of_another_condition_is_refused() {
    use crate::writes::findings_of_other_conditions as other;
    let id = "ab".repeat(32);
    let trust = format!(
        "publication-trust [PRIKK-TRUST-PUBLICATION-UNTRUSTED]: block {id} has no trusted MAINTAINER signature\n"
    );
    assert!(other(&trust).is_empty());
    let text = format!(
        "{trust}ref-publication [PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING]: pointer and log agree\nobject-integrity [PRIKK-VERIFY-OBJECT-BAD]: an object\nsealed-block {id}: key\nchecked refs: 1\n"
    );
    assert_eq!(other(&text), ["ref-publication", "object-integrity"]);
    // Lines that only look a little like a finding are not one.
    assert!(
        other(
            "Publication [PRIKK-X]: no\nref-publication [other]: no\nref-publication [PRIKK-X] no\n"
        )
        .is_empty()
    );
}

#[test]
fn a_finding_of_another_condition_in_the_text_is_refused_even_when_everything_else_is_the_retirements()
 {
    // Q3: the verdict (the first run) says publication trust only; the text (the second) holds a finding of another
    // condition, as damage that appeared between the two runs would leave it. Nothing is concluded, nothing excused.
    let Some(w) = world("q3-extra-finding") else {
        return;
    };
    w.retire(&w.signer);
    assert!(matches!(
        w.signer.retirement_explains(&w.repo, &w.wiki),
        Ok(Some(_))
    ));
    let (ctl, signer) = w.faulty();
    std::fs::write(ctl.join("extra-finding"), "").unwrap();
    let r = signer.retirement_explains(&w.repo, &w.wiki);
    assert!(
        matches!(&r, Err(SignerError::Unavailable(why)) if why.contains("ref-publication") && why.contains("not publication trust")),
        "{r:?}"
    );
}
