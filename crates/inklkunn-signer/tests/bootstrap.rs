//! The bootstrap signer against a real prikk: the three error classes, the key directory's modes, and the
//! rule that no seed ever leaves its file. Self-skips with a visible line when prikk is not on `PATH`;
//! confined when bubblewrap works.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use inklkunn_prikk::sandbox::{Sandbox, probe_bwrap};
use inklkunn_signer::{BootstrapSigner, RepositoryBootstrap, SignerError};

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> TempDir {
    let p = std::env::temp_dir().join(format!("inklkunn-signer-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp");
    TempDir(p)
}

fn prikk_on_path() -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("prikk").is_file()))
        .unwrap_or(false)
}

fn sandbox() -> Sandbox {
    if probe_bwrap(std::ffi::OsStr::new("bwrap")).is_ok() {
        println!("policy: confined (bubblewrap)");
        Sandbox::default()
    } else {
        println!("skipping: bubblewrap cannot confine a process here; running unconfined");
        Sandbox::Unconfined
    }
}

fn seeds(keys: &std::path::Path) -> Vec<String> {
    ["author", "maintainer"]
        .iter()
        .map(|r| {
            std::fs::read_to_string(keys.join(format!("{r}.seed")))
                .expect("seed")
                .trim()
                .to_owned()
        })
        .collect()
}

fn bytes_under(dir: &std::path::Path, out: &mut Vec<u8>) {
    for e in std::fs::read_dir(dir).expect("dir").flatten() {
        let p = e.path();
        if p.is_dir() {
            bytes_under(&p, out);
        } else if let Ok(b) = std::fs::read(&p) {
            out.extend_from_slice(&b);
        }
    }
}

#[test]
fn the_key_directory_is_private_and_the_constructor_checks_the_path() {
    let r = BootstrapSigner::new("prikk", Sandbox::Unconfined, "relative/keys");
    assert!(matches!(r, Err(SignerError::Unavailable(_))), "{r:?}");
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let tmp = temp("keys");
    let keys = tmp.0.join("keys");
    let signer = BootstrapSigner::new("prikk", sandbox(), &keys).expect("signer");
    let ids = signer.ensure_keys().expect("keys");
    assert!(ids.author.starts_with("ed25519-"));
    assert!(ids.maintainer.starts_with("ed25519-"));
    assert_eq!(ids.maintainer_public_key.len(), 64);
    let mode = std::fs::metadata(&keys).expect("dir").permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "the key directory is private");
    for s in seeds(&keys) {
        assert_eq!(s.len(), 64);
        assert_ne!(
            s, ids.maintainer_public_key,
            "the public key is not the seed"
        );
        assert!(
            !format!("{signer:?}{ids:?}").contains(&s),
            "a seed leaked into a debug output"
        );
    }
    let seed_mode = std::fs::metadata(keys.join("author.seed"))
        .expect("seed")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(seed_mode, 0o600);
    // Idempotent: the same ids the second time.
    assert_eq!(signer.ensure_keys().expect("again"), ids);
}

#[test]
fn a_seed_without_its_key_id_is_damaged_not_guessed() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let tmp = temp("noid");
    let keys = tmp.0.join("keys");
    let signer = BootstrapSigner::new("prikk", sandbox(), &keys).expect("signer");
    signer.ensure_keys().expect("keys");
    std::fs::remove_file(keys.join("author.seed.key-id")).expect("remove");
    let r = signer.ensure_keys();
    assert!(matches!(r, Err(SignerError::Damaged(_))), "{r:?}");
}

#[test]
fn refused_incomplete_and_sealed_are_told_apart_and_no_seed_enters_the_repository() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let tmp = temp("seq");
    let keys = tmp.0.join("keys");
    let repo = tmp.0.join("repo");
    std::fs::create_dir_all(&repo).expect("repo");
    let signer = BootstrapSigner::new("prikk", sandbox(), &keys).expect("signer");
    signer.create_repository(&repo).expect("init and trust");

    // Nothing to commit: prikk refuses, and the text is prikk's.
    let empty = signer.commit_and_seal(&repo, "Nothing");
    assert!(matches!(empty, Err(SignerError::Refused(_))), "{empty:?}");

    // A change the maintainer cannot seal (its seed unreadable): committed, not sealed, typed as such.
    std::fs::write(repo.join("a.md"), "# A\n").expect("write");
    let seed = keys.join("maintainer.seed");
    let saved = std::fs::read(&seed).expect("seed");
    std::fs::set_permissions(&seed, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    let incomplete = signer.commit_and_seal(&repo, "Add A");
    std::fs::set_permissions(&seed, std::fs::Permissions::from_mode(0o600)).expect("chmod back");
    assert_eq!(
        std::fs::read(&seed).expect("seed"),
        saved,
        "the seed is untouched"
    );
    match &incomplete {
        Err(SignerError::Incomplete {
            step: "seal",
            detail,
        }) => {
            for s in seeds(&keys) {
                assert!(!detail.contains(&s), "a seed in an error text");
            }
        }
        other => panic!("expected Incomplete at seal, got {other:?}"),
    }

    // The queued patch can be sealed by a later change; the ids come back.
    std::fs::write(repo.join("b.md"), "# B\n").expect("write");
    let sealed = signer.commit_and_seal(&repo, "Add B").expect("sealed");
    assert_eq!(sealed.block_id.len(), 64);
    assert!(!sealed.patch_ids.is_empty());

    // No seed anywhere in the repository, in any error text, or in the ids.
    let mut all = Vec::new();
    bytes_under(&repo, &mut all);
    let all = String::from_utf8_lossy(&all);
    for s in seeds(&keys) {
        assert!(!all.contains(&s), "a seed inside the repository");
        assert!(!format!("{empty:?}{incomplete:?}{sealed:?}").contains(&s));
    }
}
