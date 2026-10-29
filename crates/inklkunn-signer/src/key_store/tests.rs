#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use super::crypto::{CryptoError, KdfParams, MasterKey};
use super::seeds::{DIR_PREFIX, fs_type_of};
use super::*;

const SECRET: &str = "correct horse battery staple, with a long tail 7731";
const AUTHOR_SEED: &str = "1d2dcc973fb1644d6970090b127e314a3935ce3b95b205671111111111111111\n";
const MAINTAINER_SEED: &str = "e998909328e4d1216c0e5ccf4b00141b32c6109498f04ba94a65cc61124ab687\n";

/// Cheap derivation costs: the tests exercise the logic, not the cost.
const CHEAP: KdfParams = KdfParams {
    m_cost_kib: 8,
    t_cost: 1,
    p_cost: 1,
};

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        // A test may leave a directory unwritable on purpose.
        fn open_up(p: &Path) {
            let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700));
            if let Ok(entries) = std::fs::read_dir(p) {
                for e in entries.flatten() {
                    if e.path().is_dir() {
                        open_up(&e.path());
                    }
                }
            }
        }
        open_up(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> TempDir {
    let p = std::env::temp_dir().join(format!("inklkunn-keys-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp");
    TempDir(p)
}

fn secret() -> Secret {
    Secret::from_bytes(SECRET.as_bytes()).expect("secret")
}

fn store_in(dir: &Path) -> KeyStore {
    KeyStore::open_with(dir, &secret(), CHEAP).expect("key store")
}

fn meta(wiki: &WikiId) -> Meta {
    Meta {
        wiki: wiki.clone(),
        generation: 1,
        author_key_id: format!("inklkunn-{wiki}-author"),
        maintainer_key_id: format!("inklkunn-{wiki}-maintainer"),
        created_at: 1_700_000_000,
        rotated_at: None,
    }
}

fn seeds() -> Seeds {
    Seeds::new(AUTHOR_SEED.as_bytes(), MAINTAINER_SEED.as_bytes()).expect("seeds")
}

/// A directory of its own on the tmpfs, removed afterwards: the tests run in parallel, and a shared base
/// would let one test's seed directories show up in another's before-and-after listing.
struct ShmDir(PathBuf);

impl Drop for ShmDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A seed base on a real tmpfs (its own directory under `/dev/shm`), or `None` with a visible note on a
/// host that has no tmpfs there.
fn base(tag: &str) -> Option<(ShmDir, SeedBase)> {
    let dir = PathBuf::from("/dev/shm").join(format!("inklkunn-test-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    if std::fs::create_dir(&dir).is_err() {
        println!("skipping: no writable tmpfs at /dev/shm here");
        return None;
    }
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    let guard = ShmDir(dir.clone());
    match SeedBase::tmpfs(&dir) {
        Ok(b) => Some((guard, b)),
        Err(e) => {
            println!("skipping: no tmpfs seed base at /dev/shm here ({e})");
            None
        }
    }
}

fn seed_dirs(base: &SeedBase) -> Vec<PathBuf> {
    std::fs::read_dir(base.path())
        .map(|it| {
            it.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with(DIR_PREFIX))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn mode(p: &Path) -> u32 {
    std::fs::metadata(p).expect("metadata").permissions().mode() & 0o777
}

// ---- the operator's secret ----

#[test]
fn a_secret_is_the_file_without_its_trailing_line_break() {
    let plain = Secret::from_bytes(SECRET.as_bytes()).unwrap();
    for ending in ["\n", "\r\n", "\n\n", "\r"] {
        let with = Secret::from_bytes(format!("{SECRET}{ending}").as_bytes()).unwrap();
        assert_eq!(with.0.as_slice(), plain.0.as_slice(), "{ending:?}");
    }
    // Only trailing line breaks go; a space is part of the secret.
    let spaced = Secret::from_bytes(format!("{SECRET} ").as_bytes()).unwrap();
    assert_ne!(spaced.0.as_slice(), plain.0.as_slice());
    // Bounds.
    assert!(matches!(
        Secret::from_bytes(b"short"),
        Err(KeyStoreError::Secret(_))
    ));
    assert!(matches!(
        Secret::from_bytes(&[b'x'; 15]),
        Err(KeyStoreError::Secret(_))
    ));
    assert!(Secret::from_bytes(&[b'x'; 16]).is_ok());
    assert!(Secret::from_bytes(&[b'x'; 1024]).is_ok());
    assert!(matches!(
        Secret::from_bytes(&[b'x'; 1025]),
        Err(KeyStoreError::Secret(_))
    ));
    assert!(matches!(
        Secret::from_bytes(b"\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n"),
        Err(KeyStoreError::Secret(_))
    ));
}

#[test]
fn a_secret_file_must_be_an_absolute_regular_file_and_a_link_is_followed() {
    let tmp = temp("secret-file");
    let file = tmp.0.join("secret");
    std::fs::write(&file, format!("{SECRET}\n")).unwrap();
    assert!(Secret::from_file(&file).is_ok());
    let link = tmp.0.join("link");
    std::os::unix::fs::symlink(&file, &link).unwrap();
    assert!(
        Secret::from_file(&link).is_ok(),
        "secret mounts are often links"
    );
    assert!(matches!(
        Secret::from_file(Path::new("relative/secret")),
        Err(KeyStoreError::Secret(_))
    ));
    assert!(matches!(
        Secret::from_file(&tmp.0.join("missing")),
        Err(KeyStoreError::Secret(_))
    ));
    assert!(matches!(
        Secret::from_file(&tmp.0),
        Err(KeyStoreError::Secret(_))
    ));
    std::fs::write(tmp.0.join("big"), vec![b'x'; 5000]).unwrap();
    assert!(matches!(
        Secret::from_file(&tmp.0.join("big")),
        Err(KeyStoreError::Secret(_))
    ));
}

// ---- the store and its header ----

#[test]
fn a_new_store_is_private_and_reopens_with_the_same_secret() {
    let tmp = temp("open");
    let dir = tmp.0.join("keys");
    let store = store_in(&dir);
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&dir.join("store.json")), 0o600);
    let wiki = WikiId::new();
    store.create(&meta(&wiki), &seeds()).unwrap();
    drop(store);
    // The costs a new store is made with are recorded; a reopen uses the header's, not the caller's.
    let reopened = KeyStore::open_with(&dir, &secret(), KdfParams::DEFAULT).unwrap();
    assert_eq!(reopened.status(&wiki).unwrap(), meta(&wiki));
}

#[test]
fn a_wrong_secret_does_not_open_the_store_and_changes_nothing() {
    let tmp = temp("wrong");
    let dir = tmp.0.join("keys");
    let wiki = WikiId::new();
    store_in(&dir).create(&meta(&wiki), &seeds()).unwrap();
    let header_before = std::fs::read(dir.join("store.json")).unwrap();
    let record_before = std::fs::read(dir.join(format!("{wiki}.key.json"))).unwrap();
    let other = Secret::from_bytes(b"another secret entirely, also long").unwrap();
    assert_eq!(
        KeyStore::open_with(&dir, &other, CHEAP).err(),
        Some(KeyStoreError::Locked)
    );
    assert_eq!(
        std::fs::read(dir.join("store.json")).unwrap(),
        header_before
    );
    assert_eq!(
        std::fs::read(dir.join(format!("{wiki}.key.json"))).unwrap(),
        record_before
    );
    // The right secret still works afterwards.
    assert!(KeyStore::open_with(&dir, &secret(), CHEAP).is_ok());
}

#[test]
fn an_altered_header_is_refused_and_never_opens_with_the_right_secret() {
    let tmp = temp("header");
    let dir = tmp.0.join("keys");
    drop(store_in(&dir));
    let original = std::fs::read_to_string(dir.join("store.json")).unwrap();
    let edit = |from: &str, to: &str| -> String {
        assert!(original.contains(from), "{from}");
        original.replacen(from, to, 1)
    };
    let cases: Vec<(&str, String, bool)> = vec![
        // (what, header text, expect Locked (true) or a Header refusal (false))
        (
            "a different salt",
            edit("\"salt\": \"", "\"salt\": \"00"),
            false,
        ),
        (
            "a different time cost",
            edit("\"t_cost\": 1", "\"t_cost\": 2"),
            true,
        ),
        (
            "a different memory cost",
            edit("\"m_cost_kib\": 8", "\"m_cost_kib\": 16"),
            true,
        ),
        (
            "another format",
            edit("inklkunn-key-store/1", "inklkunn-key-store/2"),
            false,
        ),
        ("another kdf", edit("argon2id", "argon2i"), false),
        (
            "another version",
            edit("\"version\": 19", "\"version\": 16"),
            false,
        ),
        (
            "a memory cost beyond the bound",
            edit("\"m_cost_kib\": 8", "\"m_cost_kib\": 4194304"),
            false,
        ),
        (
            "a time cost beyond the bound",
            edit("\"t_cost\": 1", "\"t_cost\": 99"),
            false,
        ),
        (
            "an extra field",
            edit("\"format\"", "\"extra\": 1, \"format\""),
            false,
        ),
        ("not JSON", "not json at all".to_owned(), false),
        ("empty", String::new(), false),
    ];
    for (what, text, locked) in cases {
        std::fs::write(dir.join("store.json"), &text).unwrap();
        let r = KeyStore::open_with(&dir, &secret(), CHEAP);
        match (&r, locked) {
            (Err(KeyStoreError::Locked), true) => {}
            (Err(KeyStoreError::Header(_)), false) => {}
            (Err(KeyStoreError::Locked), false) => {} // an altered salt may also derive another key
            _ => panic!("{what}: {r:?}"),
        }
    }
    // Restore it and the store opens again.
    std::fs::write(dir.join("store.json"), &original).unwrap();
    assert!(KeyStore::open_with(&dir, &secret(), CHEAP).is_ok());
}

#[test]
fn a_directory_of_other_files_is_not_made_into_a_key_store() {
    let tmp = temp("foreign");
    let dir = tmp.0.join("keys");
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(dir.join("notes.txt"), "mine").unwrap();
    assert!(matches!(
        KeyStore::open_with(&dir, &secret(), CHEAP),
        Err(KeyStoreError::Header(_))
    ));
    assert!(
        !dir.join("store.json").exists(),
        "nothing was written into it"
    );
}

#[test]
fn the_key_directory_must_be_private_real_and_absolute() {
    let tmp = temp("dir");
    let open = tmp.0.join("open");
    std::fs::create_dir(&open).unwrap();
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        KeyStore::open_with(&open, &secret(), CHEAP),
        Err(KeyStoreError::Io(_, std::io::ErrorKind::PermissionDenied))
    ));
    let real = tmp.0.join("real");
    std::fs::create_dir(&real).unwrap();
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o700)).unwrap();
    let link = tmp.0.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    assert!(matches!(
        KeyStore::open_with(&link, &secret(), CHEAP),
        Err(KeyStoreError::Io(_, _))
    ));
    assert!(matches!(
        KeyStore::open_with(Path::new("keys"), &secret(), CHEAP),
        Err(KeyStoreError::Io(_, _))
    ));
}

#[test]
fn two_processes_starting_on_a_new_directory_end_with_one_store() {
    let tmp = temp("race");
    let dir = tmp.0.join("keys");
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let dir = dir.clone();
            std::thread::spawn(move || KeyStore::open_with(&dir, &secret(), CHEAP).map(|_| ()))
        })
        .collect();
    for h in handles {
        assert_eq!(h.join().unwrap(), Ok(()));
    }
    // One header, and it opens for everyone.
    let wiki = WikiId::new();
    store_in(&dir).create(&meta(&wiki), &seeds()).unwrap();
    assert_eq!(store_in(&dir).status(&wiki).unwrap().wiki, wiki);
}

#[test]
fn opening_never_makes_a_store_and_names_the_path_it_did_not_find() {
    let tmp = temp("no-store");
    let missing = tmp.0.join("mistyped");
    let err = KeyStore::open(&missing, &secret()).unwrap_err();
    assert_eq!(err, KeyStoreError::NoStore(missing.clone()));
    assert!(
        err.to_string().contains(&*missing.to_string_lossy()),
        "{err}"
    );
    assert!(!missing.exists(), "opening made a directory");
    // An existing, private, empty directory is no store either, and stays empty.
    let empty = tmp.0.join("empty");
    std::fs::create_dir(&empty).unwrap();
    std::fs::set_permissions(&empty, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(
        KeyStore::open(&empty, &secret()).unwrap_err(),
        KeyStoreError::NoStore(empty.clone())
    );
    assert_eq!(std::fs::read_dir(&empty).unwrap().count(), 0);
    // Asked to create, the same call makes it, and then plain open finds it.
    KeyStore::create_or_open(&missing, &secret()).unwrap();
    assert!(KeyStore::open(&missing, &secret()).is_ok());
}

#[test]
fn a_second_creator_arriving_while_the_first_has_only_a_temporary_file_ends_with_the_one_store() {
    use std::sync::mpsc;
    use std::time::Duration;
    let tmp = temp("race-det");
    let dir = tmp.0.join("keys");
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let (go, arrive) = mpsc::channel::<()>();
    let second = {
        let dir = dir.clone();
        std::thread::spawn(move || {
            arrive.recv().unwrap();
            KeyStore::open_with(&dir, &secret(), CHEAP).map(|_| ())
        })
    };
    // The first creator, stopped with its temporary header written and not yet linked, lets the second one
    // start and gives it time to look at the directory.
    let first = KeyStore::open_inner(&dir, &secret(), CHEAP, Access::MayCreate, &|| {
        go.send(()).unwrap();
        std::thread::sleep(Duration::from_millis(300));
    });
    assert!(first.is_ok(), "{first:?}");
    assert_eq!(second.join().unwrap(), Ok(()));
    assert!(KeyStore::open_with(&dir, &secret(), CHEAP).is_ok());
    let headers = std::fs::read_dir(&dir)
        .unwrap()
        .filter(|e| e.as_ref().unwrap().file_name() == "store.json")
        .count();
    assert_eq!(headers, 1);
}

#[test]
fn a_temporary_file_left_by_a_crash_does_not_block_creation_but_foreign_files_still_do() {
    let tmp = temp("leftover");
    let dir = tmp.0.join("keys");
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(dir.join(".store.json.tmp-0123456789abcdef"), "half").unwrap();
    KeyStore::open_with(&dir, &secret(), CHEAP).expect("a crash's leftover blocked creation");
    assert!(
        !dir.join(".store.json.tmp-0123456789abcdef").exists(),
        "the leftover was kept"
    );
    // Something that is not this store's own is still a refusal, and is left alone.
    let other = tmp.0.join("other");
    std::fs::create_dir(&other).unwrap();
    std::fs::set_permissions(&other, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::write(other.join("notes.txt"), "mine").unwrap();
    assert!(matches!(
        KeyStore::open_with(&other, &secret(), CHEAP),
        Err(KeyStoreError::Header(_))
    ));
    assert_eq!(
        std::fs::read_to_string(other.join("notes.txt")).unwrap(),
        "mine"
    );
}

// ---- records ----

#[test]
fn a_record_holds_the_plain_facts_and_the_seeds_come_back_exactly_in_files() {
    let Some((_shm, base)) = base("a_record_holds_the_plain") else {
        return;
    };
    let tmp = temp("record");
    let store = store_in(&tmp.0.join("keys"));
    let wiki = WikiId::new();
    store.create(&meta(&wiki), &seeds()).unwrap();
    assert_eq!(store.status(&wiki).unwrap(), meta(&wiki));
    assert_eq!(store.wikis().unwrap(), vec![wiki.clone()]);
    let (m, dir) = store.checkout(&wiki, &base).unwrap();
    assert_eq!(m, meta(&wiki));
    let env = dir.key_dir().env();
    let get = |k: &str| {
        env.iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap()
    };
    // prikk is told the ids and the paths, and nothing else.
    assert_eq!(
        get("PRIKK_AUTHOR_KEY_ID"),
        format!("inklkunn-{wiki}-author")
    );
    assert_eq!(
        get("PRIKK_MAINTAINER_KEY_ID"),
        format!("inklkunn-{wiki}-maintainer")
    );
    assert_eq!(env.len(), 4);
    let author = PathBuf::from(get("PRIKK_AUTHOR_SEED_FILE"));
    let maintainer = PathBuf::from(get("PRIKK_MAINTAINER_SEED_FILE"));
    assert_eq!(std::fs::read_to_string(&author).unwrap(), AUTHOR_SEED);
    assert_eq!(
        std::fs::read_to_string(&maintainer).unwrap(),
        MAINTAINER_SEED
    );
    assert_eq!(mode(&author), 0o600);
    assert_eq!(mode(&maintainer), 0o600);
    assert_eq!(mode(author.parent().unwrap()), 0o700);
    assert!(author.starts_with(base.path()));
    dir.finish().unwrap();
}

#[test]
fn a_seed_file_exists_only_during_the_call() {
    let Some((_shm, base)) = base("a_seed_file_exists_only_") else {
        return;
    };
    let tmp = temp("lifetime");
    let store = store_in(&tmp.0.join("keys"));
    let wiki = WikiId::new();
    store.create(&meta(&wiki), &seeds()).unwrap();
    // Before: no seed directory, and no seed text anywhere under the key store's own directory either.
    let before = seed_dirs(&base);
    let (_, dir) = store.checkout(&wiki, &base).unwrap();
    // During: one more directory, with exactly the two files.
    let during = seed_dirs(&base);
    assert_eq!(during.len(), before.len() + 1);
    let mine: Vec<&PathBuf> = during.iter().filter(|p| !before.contains(p)).collect();
    let mine_dir = mine.first().copied().unwrap();
    let files: Vec<String> = std::fs::read_dir(mine_dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(files.len(), 2, "{files:?}");
    let path = mine_dir.clone();
    dir.finish().unwrap();
    // After: gone, with its files.
    assert!(!path.exists());
    assert_eq!(seed_dirs(&base), before);
    // An early return or a panic removes it as well.
    {
        let (_, dropped) = store.checkout(&wiki, &base).unwrap();
        let p = dropped.key_dir().path().to_path_buf();
        assert!(p.exists());
        drop(dropped);
        assert!(!p.exists(), "Drop is the backstop");
    }
    let result = std::panic::catch_unwind(|| {
        let (_, _held) = store.checkout(&wiki, &base).unwrap();
        panic!("a call that dies");
    });
    assert!(result.is_err());
    assert_eq!(seed_dirs(&base), before, "a panic left nothing behind");
}

#[test]
fn the_seed_files_are_overwritten_before_they_are_unlinked() {
    let Some((_shm, base)) = base("the_seed_files_are_overw") else {
        return;
    };
    let tmp = temp("overwrite");
    let store = store_in(&tmp.0.join("keys"));
    let wiki = WikiId::new();
    store.create(&meta(&wiki), &seeds()).unwrap();
    let (_, dir) = store.checkout(&wiki, &base).unwrap();
    let author = dir
        .key_dir()
        .env()
        .into_iter()
        .find(|(k, _)| k == "PRIKK_AUTHOR_SEED_FILE")
        .map(|(_, v)| PathBuf::from(v))
        .unwrap();
    // Keep a second name for the same file: after removal it is the only way to look at what was left.
    let alias = author.parent().unwrap().join("alias");
    std::fs::hard_link(&author, &alias).unwrap();
    // `finish` removes the alias too (it removes every file in the directory); read through an open handle.
    let handle = std::fs::File::open(&author).unwrap();
    dir.finish().unwrap();
    let mut left = String::new();
    std::io::Read::read_to_string(&mut &handle, &mut left).unwrap();
    assert!(
        !left.contains(AUTHOR_SEED.trim()),
        "the bytes were overwritten before the unlink"
    );
    assert!(left.bytes().all(|b| b == 0), "{left:?}");
}

#[test]
fn removal_that_fails_is_reported_and_names_no_path() {
    let Some((_shm, base)) = base("removal_that_fails_is_re") else {
        return;
    };
    let tmp = temp("stuck");
    let store = store_in(&tmp.0.join("keys"));
    let wiki = WikiId::new();
    store.create(&meta(&wiki), &seeds()).unwrap();
    let (_, dir) = store.checkout(&wiki, &base).unwrap();
    let held = dir.key_dir().path().to_path_buf();
    // A directory that cannot be written to cannot have files unlinked: the failure to test (as a user who
    // is not root; root ignores the mode).
    std::fs::set_permissions(&held, std::fs::Permissions::from_mode(0o500)).unwrap();
    let root_probe = held.join("probe");
    if std::fs::write(&root_probe, b"x").is_ok() {
        let _ = std::fs::remove_file(&root_probe);
        std::fs::set_permissions(&held, std::fs::Permissions::from_mode(0o700)).unwrap();
        dir.finish().unwrap();
        println!("skipping: running as a user for whom the mode does not bind");
        return;
    }
    let err = dir.finish().unwrap_err();
    assert_eq!(err, KeyStoreError::SeedsNotRemoved);
    let shown = format!("{err} {err:?}");
    assert!(!shown.contains(held.to_string_lossy().as_ref()), "{shown}");
    assert!(
        !shown.contains(held.file_name().unwrap().to_string_lossy().as_ref()),
        "the random name stays out of the text: {shown}"
    );
    assert!(
        shown.contains(DIR_PREFIX),
        "the operator can find it by its prefix"
    );
    // The operator's cleanup works.
    std::fs::set_permissions(&held, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(std::fs::remove_dir_all(&held).is_ok());
}

#[test]
fn two_checkouts_are_two_directories() {
    let Some((_shm, base)) = base("two_checkouts_are_two_di") else {
        return;
    };
    let tmp = temp("two");
    let store = store_in(&tmp.0.join("keys"));
    let wiki = WikiId::new();
    store.create(&meta(&wiki), &seeds()).unwrap();
    let (_, a) = store.checkout(&wiki, &base).unwrap();
    let (_, b) = store.checkout(&wiki, &base).unwrap();
    assert_ne!(a.key_dir().path(), b.key_dir().path());
    a.finish().unwrap();
    assert!(
        b.key_dir().path().exists(),
        "finishing one leaves the other"
    );
    b.finish().unwrap();
}

#[test]
fn a_record_is_not_readable_without_the_secret_and_holds_no_seed_text() {
    let tmp = temp("sealed");
    let dir = tmp.0.join("keys");
    let store = store_in(&dir);
    let wiki = WikiId::new();
    store.create(&meta(&wiki), &seeds()).unwrap();
    let bytes = std::fs::read(dir.join(format!("{wiki}.key.json"))).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    for seed in [AUTHOR_SEED, MAINTAINER_SEED] {
        let seed = seed.trim();
        assert!(!text.contains(seed), "the seed is in the record");
        // Nor any 16-character piece of it, nor its bytes raw.
        for i in (0..=seed.len() - 16).step_by(4) {
            assert!(
                !text.contains(&seed[i..i + 16]),
                "a piece of the seed is in the record"
            );
        }
        assert!(
            !bytes
                .windows(32)
                .any(|w| w == seed.as_bytes().get(..32).unwrap())
        );
    }
    assert!(!text.contains(SECRET));
    // The plain fields are readable, which is what `status` shows without any key.
    assert!(text.contains(&format!("inklkunn-{wiki}-author")));
    // And opening needs the secret: the wrong one never gets as far as a record.
    let other = Secret::from_bytes(b"not the secret, but long enough").unwrap();
    assert!(KeyStore::open_with(&dir, &other, CHEAP).is_err());
}

#[test]
fn every_record_has_its_own_nonce_and_ciphertext() {
    let tmp = temp("nonce");
    let dir = tmp.0.join("keys");
    let store = store_in(&dir);
    let (a, b) = (WikiId::new(), WikiId::new());
    // The same seeds under two wikis, and the same wiki replaced twice.
    store.create(&meta(&a), &seeds()).unwrap();
    store.create(&meta(&b), &seeds()).unwrap();
    let field = |wiki: &WikiId, key: &str| -> String {
        let v: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join(format!("{wiki}.key.json"))).unwrap())
                .unwrap();
        v.get(key)
            .and_then(serde_json::Value::as_str)
            .unwrap()
            .to_owned()
    };
    assert_ne!(field(&a, "nonce"), field(&b, "nonce"));
    assert_ne!(field(&a, "sealed"), field(&b, "sealed"));
    let first = (field(&a, "nonce"), field(&a, "sealed"));
    store.replace(&meta(&a), &seeds()).unwrap();
    assert_ne!(first.0, field(&a, "nonce"), "a rewrite draws a fresh nonce");
    assert_ne!(first.1, field(&a, "sealed"));
    assert_eq!(field(&a, "nonce").len(), 48);
}

#[test]
fn a_record_cannot_be_changed_moved_or_corrupted_without_notice() {
    let tmp = temp("tamper");
    let dir = tmp.0.join("keys");
    let store = store_in(&dir);
    let (a, b) = (WikiId::new(), WikiId::new());
    store.create(&meta(&a), &seeds()).unwrap();
    let mut other = Seeds::new("ab".repeat(32).as_bytes(), "cd".repeat(32).as_bytes()).unwrap();
    other.author = Zeroizing::new(format!("{}\n", "ab".repeat(32)).into_bytes());
    store.create(&meta(&b), &other).unwrap();
    let path_a = dir.join(format!("{a}.key.json"));
    let path_b = dir.join(format!("{b}.key.json"));
    let original = std::fs::read_to_string(&path_a).unwrap();
    let json: serde_json::Value = serde_json::from_str(&original).unwrap();
    let mutate = |f: &dyn Fn(&mut serde_json::Value)| -> String {
        let mut v = json.clone();
        f(&mut v);
        serde_json::to_string(&v).unwrap()
    };
    let flip_last = |s: &str| -> String {
        let (head, last) = s.split_at(s.len() - 1);
        format!("{head}{}", if last == "0" { "1" } else { "0" })
    };
    let cases: Vec<(&str, String)> = vec![
        (
            "a flipped ciphertext byte",
            mutate(&|v| {
                let c = flip_last(v["sealed"].as_str().unwrap());
                v["sealed"] = c.into();
            }),
        ),
        (
            "a flipped nonce byte",
            mutate(&|v| {
                let c = flip_last(v["nonce"].as_str().unwrap());
                v["nonce"] = c.into();
            }),
        ),
        (
            "a changed author key id",
            mutate(&|v| v["author_key_id"] = "inklkunn-someone-else".into()),
        ),
        (
            "a changed maintainer key id",
            mutate(&|v| v["maintainer_key_id"] = "inklkunn-x".into()),
        ),
        (
            "a changed generation",
            mutate(&|v| v["generation"] = 2.into()),
        ),
        (
            "a changed creation time",
            mutate(&|v| v["created_at"] = 1.into()),
        ),
        (
            "an invented rotation",
            mutate(&|v| v["rotated_at"] = 1_800_000_000i64.into()),
        ),
        (
            "the wiki field of another wiki",
            mutate(&|v| v["wiki"] = b.to_string().into()),
        ),
        (
            "another format",
            mutate(&|v| v["format"] = "inklkunn-key-record/2".into()),
        ),
        ("an extra field", mutate(&|v| v["note"] = "x".into())),
        ("a short nonce", mutate(&|v| v["nonce"] = "00".into())),
        (
            "an upper-case nonce",
            mutate(&|v| {
                let c = v["nonce"]
                    .as_str()
                    .unwrap()
                    .to_uppercase()
                    .replace("0", "A");
                v["nonce"] = c.into();
            }),
        ),
        (
            "a truncated ciphertext",
            mutate(&|v| {
                let c = v["sealed"].as_str().unwrap();
                v["sealed"] = c[..c.len() - 2].to_owned().into();
            }),
        ),
        ("not JSON", "garbage".to_owned()),
        ("empty", String::new()),
    ];
    let Some((_guard, seed_base)) = base("tamper") else {
        return;
    };
    for (what, text) in cases {
        std::fs::write(&path_a, &text).unwrap();
        let r = store.status(&a);
        assert!(matches!(r, Err(KeyStoreError::Record(_))), "{what}: {r:?}");
        let r = store.checkout(&a, &seed_base);
        assert!(matches!(r, Err(KeyStoreError::Record(_))), "{what}");
    }
    // Wiki B's record under wiki A's name is not A's keys.
    std::fs::copy(&path_b, &path_a).unwrap();
    assert!(matches!(store.status(&a), Err(KeyStoreError::Record(_))));
    // Restore: it reads again.
    std::fs::write(&path_a, &original).unwrap();
    assert_eq!(store.status(&a).unwrap(), meta(&a));
}

#[test]
fn a_record_that_is_a_link_or_too_large_is_refused() {
    let tmp = temp("shape");
    let dir = tmp.0.join("keys");
    let store = store_in(&dir);
    let wiki = WikiId::new();
    store.create(&meta(&wiki), &seeds()).unwrap();
    let path = dir.join(format!("{wiki}.key.json"));
    let real = tmp.0.join("elsewhere");
    std::fs::rename(&path, &real).unwrap();
    std::os::unix::fs::symlink(&real, &path).unwrap();
    assert!(matches!(store.status(&wiki), Err(KeyStoreError::Io(_, _))));
    std::fs::remove_file(&path).unwrap();
    std::fs::write(&path, vec![b'x'; 20_000]).unwrap();
    assert!(matches!(store.status(&wiki), Err(KeyStoreError::Io(_, _))));
}

#[test]
fn creating_twice_is_refused_replacing_needs_keys_and_removing_says_what_it_did() {
    let tmp = temp("rules");
    let store = store_in(&tmp.0.join("keys"));
    let wiki = WikiId::new();
    assert_eq!(store.status(&wiki).err(), Some(KeyStoreError::NoKeys));
    assert_eq!(
        store.replace(&meta(&wiki), &seeds()).err(),
        Some(KeyStoreError::NoKeys)
    );
    assert!(!store.remove(&wiki).unwrap());
    store.create(&meta(&wiki), &seeds()).unwrap();
    assert_eq!(
        store.create(&meta(&wiki), &seeds()).err(),
        Some(KeyStoreError::Exists),
        "an existing pair is never overwritten by a create"
    );
    let mut rotated = meta(&wiki);
    rotated.generation = 2;
    rotated.rotated_at = Some(1_700_000_500);
    rotated.maintainer_key_id = format!("inklkunn-{wiki}-maintainer-2");
    store.replace(&rotated, &seeds()).unwrap();
    assert_eq!(store.status(&wiki).unwrap(), rotated);
    assert!(store.remove(&wiki).unwrap());
    assert_eq!(store.status(&wiki).err(), Some(KeyStoreError::NoKeys));
    // No temporary file is left behind by any of it.
    let leftovers: Vec<_> = std::fs::read_dir(tmp.0.join("keys"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn a_record_that_would_be_nonsense_is_never_written() {
    let tmp = temp("nonsense");
    let store = store_in(&tmp.0.join("keys"));
    let wiki = WikiId::new();
    let mut bad = meta(&wiki);
    bad.generation = 0;
    assert!(store.create(&bad, &seeds()).is_err());
    let mut bad = meta(&wiki);
    bad.author_key_id = "has a space".into();
    assert!(store.create(&bad, &seeds()).is_err());
    let mut bad = meta(&wiki);
    bad.maintainer_key_id = bad.author_key_id.clone();
    assert!(store.create(&bad, &seeds()).is_err());
    assert!(Seeds::new(b"short", MAINTAINER_SEED.as_bytes()).is_err());
    assert!(Seeds::new(AUTHOR_SEED.as_bytes(), AUTHOR_SEED.as_bytes()).is_err());
    assert!(
        Seeds::new(
            AUTHOR_SEED.to_uppercase().as_bytes(),
            MAINTAINER_SEED.as_bytes()
        )
        .is_err()
    );
    assert!(store.wikis().unwrap().is_empty(), "nothing was written");
}

#[test]
fn nothing_secret_appears_in_any_error_or_debug_output() {
    let tmp = temp("grep");
    let dir = tmp.0.join("keys");
    let store = store_in(&dir);
    let wiki = WikiId::new();
    store.create(&meta(&wiki), &seeds()).unwrap();
    let mut shown: Vec<String> = vec![
        format!("{store:?}"),
        format!("{:?}", secret()),
        format!("{:?}", seeds()),
        format!(
            "{:?}",
            MasterKey::derive(SECRET.as_bytes(), &[7; 16], CHEAP).unwrap()
        ),
    ];
    // Errors from every way in.
    let other = Secret::from_bytes(b"not the secret, but long enough").unwrap();
    shown.push(format!(
        "{:?}",
        KeyStore::open_with(&dir, &other, CHEAP).err()
    ));
    shown.push(format!(
        "{}",
        KeyStore::open_with(&dir, &other, CHEAP).unwrap_err()
    ));
    shown.push(format!(
        "{:?}",
        Secret::from_bytes(SECRET.as_bytes().get(..10).unwrap()).err()
    ));
    shown.push(format!("{:?}", store.status(&WikiId::new()).err()));
    std::fs::write(dir.join(format!("{wiki}.key.json")), "{}").unwrap();
    shown.push(format!("{:?}", store.status(&wiki).err()));
    shown.push(format!("{}", store.status(&wiki).unwrap_err()));
    for e in [
        KeyStoreError::Locked,
        KeyStoreError::NoKeys,
        KeyStoreError::Exists,
        KeyStoreError::SeedsNotRemoved,
        KeyStoreError::Random,
        KeyStoreError::SeedBase("x"),
        KeyStoreError::Header("x"),
        KeyStoreError::Record("x"),
        KeyStoreError::Secret("x"),
    ] {
        shown.push(format!("{e} {e:?}"));
    }
    if let Some((_shm, base)) = base("nothing_secret_appears_i") {
        let w2 = WikiId::new();
        store.create(&meta(&w2), &seeds()).unwrap();
        let (_, d) = store.checkout(&w2, &base).unwrap();
        shown.push(format!("{d:?} {:?} {base:?}", d.key_dir()));
        let path = d.key_dir().path().to_string_lossy().into_owned();
        d.finish().unwrap();
        for text in &shown {
            assert!(
                !text.contains(&path),
                "the seed directory's path is in: {text}"
            );
        }
    }
    let secrets = [
        SECRET,
        AUTHOR_SEED.trim(),
        MAINTAINER_SEED.trim(),
        &AUTHOR_SEED.trim()[..24],
        &MAINTAINER_SEED.trim()[..24],
    ];
    for text in &shown {
        for secret in secrets {
            assert!(!text.contains(secret), "`{secret}` is in: {text}");
        }
    }
}

// ---- the tmpfs check ----

const MOUNTINFO: &str = "\
22 28 0:21 / /sys rw,nosuid,nodev,noexec,relatime shared:7 - sysfs sysfs rw
28 1 259:3 / / rw,relatime shared:1 - btrfs /dev/nvme0n1p3 rw,ssd,subvol=/@
31 28 0:25 / /dev/shm rw,nosuid,nodev shared:4 - tmpfs tmpfs rw
40 28 0:30 / /tmp rw,nosuid,nodev - tmpfs tmpfs rw,size=16384000k
41 28 259:3 /home /home rw,relatime shared:9 - btrfs /dev/nvme0n1p3 rw,subvol=/@home
50 41 0:50 / /home/user/odd\\040name rw - tmpfs tmpfs rw
60 28 0:60 / /srv rw - ext4 /dev/sda1 rw
61 60 0:61 / /srv rw - tmpfs tmpfs rw
";

#[test]
fn the_filesystem_of_a_path_is_read_from_the_mount_table() {
    let ty = |p: &str| fs_type_of(MOUNTINFO, Path::new(p));
    assert_eq!(ty("/dev/shm").as_deref(), Some("tmpfs"));
    assert_eq!(ty("/dev/shm/inklkunn-seeds-x").as_deref(), Some("tmpfs"));
    assert_eq!(ty("/tmp/x").as_deref(), Some("tmpfs"));
    assert_eq!(ty("/home/user").as_deref(), Some("btrfs"));
    assert_eq!(
        ty("/home/user/odd name/x").as_deref(),
        Some("tmpfs"),
        "octal escapes"
    );
    assert_eq!(
        ty("/var/lib").as_deref(),
        Some("btrfs"),
        "the root mount holds the rest"
    );
    assert_eq!(
        ty("/srv/data").as_deref(),
        Some("tmpfs"),
        "a later mount over the same point shadows"
    );
    // A prefix of the text is not a prefix of the path.
    assert_eq!(ty("/dev/shmem").as_deref(), Some("btrfs"));
    assert_eq!(ty("/tmpfoo").as_deref(), Some("btrfs"));
    assert_eq!(fs_type_of("", Path::new("/x")), None);
    assert_eq!(fs_type_of("garbage\nmore garbage\n", Path::new("/x")), None);
}

#[test]
fn a_seed_base_must_be_an_existing_absolute_tmpfs_directory() {
    assert!(matches!(
        SeedBase::tmpfs(Path::new("dev/shm")),
        Err(KeyStoreError::SeedBase(_))
    ));
    assert!(matches!(
        SeedBase::tmpfs(Path::new("/definitely/not/there")),
        Err(KeyStoreError::SeedBase(_))
    ));
    // The crate's own directory is on a disk, unless the checkout itself lives on a tmpfs.
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").unwrap();
    match fs_type_of(&mountinfo, &here.canonicalize().unwrap()).as_deref() {
        Some("tmpfs" | "ramfs") => println!("skipping: the checkout is on a tmpfs here"),
        _ => {
            let r = SeedBase::tmpfs(here);
            assert!(matches!(r, Err(KeyStoreError::SeedBase(_))), "{r:?}");
            let text = r.unwrap_err().to_string();
            assert!(text.contains("never written to a disk"), "{text}");
        }
    }
}

// ---- the primitives ----

#[test]
fn the_cipher_authenticates_its_key_its_nonce_and_its_associated_data() {
    let key = MasterKey::derive(SECRET.as_bytes(), &[1; 16], CHEAP).unwrap();
    let same = MasterKey::derive(SECRET.as_bytes(), &[1; 16], CHEAP).unwrap();
    let other_salt = MasterKey::derive(SECRET.as_bytes(), &[2; 16], CHEAP).unwrap();
    let other_secret =
        MasterKey::derive(b"some other secret, long enough", &[1; 16], CHEAP).unwrap();
    let (nonce, sealed) = key.seal(b"plaintext", b"context").unwrap();
    assert_ne!(sealed.as_slice(), b"plaintext");
    assert_eq!(
        sealed.len(),
        b"plaintext".len() + 16,
        "the tag is the only growth"
    );
    assert_eq!(
        same.open(&nonce, &sealed, b"context").unwrap().as_slice(),
        b"plaintext"
    );
    for (what, r) in [
        ("another salt", other_salt.open(&nonce, &sealed, b"context")),
        (
            "another secret",
            other_secret.open(&nonce, &sealed, b"context"),
        ),
        ("other context", key.open(&nonce, &sealed, b"Context")),
        ("no context", key.open(&nonce, &sealed, b"")),
        ("another nonce", key.open(&[9; 24], &sealed, b"context")),
        ("a flipped byte", {
            let mut s = sealed.clone();
            if let Some(b) = s.first_mut() {
                *b ^= 1;
            }
            key.open(&nonce, &s, b"context")
        }),
        (
            "a short ciphertext",
            key.open(&nonce, sealed.get(..5).unwrap(), b"context"),
        ),
        ("an empty ciphertext", key.open(&nonce, &[], b"context")),
    ] {
        assert_eq!(r.err(), Some(CryptoError::Authentication), "{what}");
    }
}

#[test]
fn the_derivation_costs_have_bounds() {
    for (m, t, p) in [
        (7, 1, 1),
        (8, 0, 1),
        (8, 1, 0),
        (8, 11, 1),
        (8, 1, 9),
        (1 << 21, 1, 1),
    ] {
        let params = KdfParams {
            m_cost_kib: m,
            t_cost: t,
            p_cost: p,
        };
        assert_eq!(
            params.check().err(),
            Some(CryptoError::Params),
            "{m} {t} {p}"
        );
        assert_eq!(
            MasterKey::derive(SECRET.as_bytes(), &[0; 16], params).err(),
            Some(CryptoError::Params)
        );
    }
    assert!(KdfParams::DEFAULT.check().is_ok());
    assert!(CHEAP.check().is_ok());
}

#[test]
fn randomness_is_real_and_hex_is_strict() {
    use super::random::{bytes, hex, unhex};
    let (a, b) = (bytes::<24>().unwrap(), bytes::<24>().unwrap());
    assert_ne!(a, b);
    assert_ne!(a, [0u8; 24]);
    assert_eq!(unhex(&hex(&a)).unwrap(), a);
    assert_eq!(hex(&[0x00, 0xff, 0x1a]), "00ff1a");
    for bad in ["0", "zz", "AA", "0g", "ff ", " ff", "f"] {
        assert_eq!(unhex(bad), None, "{bad:?}");
    }
    assert_eq!(unhex(""), Some(Vec::new()));
}

#[test]
fn the_production_open_uses_the_default_costs_and_takes_a_fraction_of_a_second() {
    let tmp = temp("default");
    let started = std::time::Instant::now();
    let store = KeyStore::create_or_open(&tmp.0.join("keys"), &secret()).unwrap();
    let took = started.elapsed();
    println!("default Argon2id (64 MiB, 3 passes, 1 lane), store opened: {took:?}");
    // A loose bound: a slow CI runner must not fail this, but a derivation that stopped being memory-hard
    // (a few milliseconds) or became unbearable (many seconds) is a mistake in the parameters.
    assert!(took < std::time::Duration::from_secs(10), "{took:?}");
    assert!(
        took > std::time::Duration::from_millis(5),
        "{took:?}: no longer memory-hard?"
    );
    // The header recorded the costs it was made with.
    let header = std::fs::read_to_string(tmp.0.join("keys/store.json")).unwrap();
    assert!(header.contains("\"m_cost_kib\": 65536") && header.contains("\"t_cost\": 3"));
    let wiki = WikiId::new();
    store.create(&meta(&wiki), &seeds()).unwrap();
    drop(store);
    assert_eq!(
        KeyStore::create_or_open(&tmp.0.join("keys"), &secret())
            .unwrap()
            .status(&wiki)
            .unwrap(),
        meta(&wiki)
    );
}

#[test]
fn a_seed_base_others_can_write_to_or_that_the_user_does_not_own_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let Some((guard, _)) = base("vetting") else {
        return;
    };
    let dir = guard.0.clone();
    // Group- or world-writable: another local user could swap a call's directory.
    for mode in [0o770, 0o707, 0o777, 0o722] {
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(mode)).unwrap();
        let r = SeedBase::tmpfs(&dir);
        assert!(
            matches!(&r, Err(KeyStoreError::SeedBase(why)) if why.contains("writable")),
            "{mode:o}: {r:?}"
        );
    }
    // Readable by others is a different matter and not this check's concern; mode 0700 and 0750 pass.
    for mode in [0o700, 0o750] {
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(mode)).unwrap();
        assert!(SeedBase::tmpfs(&dir).is_ok(), "{mode:o}");
    }
    // /dev/shm itself is root's and world-writable: never a seed base.
    let r = SeedBase::tmpfs(Path::new("/dev/shm"));
    assert!(matches!(r, Err(KeyStoreError::SeedBase(_))), "{r:?}");
}

#[test]
fn a_call_that_is_still_running_is_not_taken_for_a_dead_one() {
    let Some((guard, seed_base)) = base("heartbeat") else {
        return;
    };
    let (a, b) = (seed_base.scratch().unwrap(), seed_base.scratch().unwrap());
    // Both directories look a quarter of an hour old to the sweep…
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(900);
    for d in [a.path(), b.path()] {
        std::fs::File::open(d).unwrap().set_modified(old).unwrap();
    }
    // …but the one whose call said it was alive (every prikk call does) is touched and so is kept.
    touch(a.path());
    let removed = seed_base
        .sweep_stale(std::time::Duration::from_secs(300))
        .unwrap();
    assert_eq!(removed, 1, "only the silent one");
    assert!(a.path().exists() && !b.path().exists());
    drop((a, b, guard));
}
