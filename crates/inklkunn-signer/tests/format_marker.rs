//! A format marker prikk will not read, against real prikk binaries (maintenance M17, findings R-a and R-h).
//!
//! prikk 0.57.0 makes repositories in format 8 (`.prikk/FORMAT` holds `8` and a newline) and reads them; prikk 0.55.0
//! and 0.56.0 refuse them with `error: unsupported format version: 8`. The same words come from prikk 0.57.0 itself when
//! the marker is damaged (`8` without its newline, measured) or names a number nobody knows. The driver and the signer
//! classify them with one classifier: a mismatch only where the running prikk is known not to read the format, a damaged
//! or unknown marker anywhere else. Each test finds its prikk binaries on `PATH` itself (any directory, not only the
//! first) and says so when the one it needs is not there.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use inklkunn_prikk::{Admission, PrikkCli, Sandbox, Storage, StorageError, probe_version};
use inklkunn_signer::{SignerError, Witnesses};
use inklkunn_store::{RootLayout, WikiId};

type Version = (u64, u64, u64);

/// The first `prikk` on `PATH` (in any directory) whose version satisfies `wanted`.
fn prikk_where(wanted: impl Fn(Version) -> bool) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("prikk"))
        .filter(|c| c.is_file())
        .find(|c| probe_version(c.as_os_str()).is_some_and(&wanted))
}

struct Tmp(PathBuf);

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A repository made by `maker` (`prikk setup`, with its own home so no key of the machine is touched).
struct Repo {
    _tmp: Tmp,
    root: RootLayout,
    wiki: WikiId,
    repo: PathBuf,
}

fn made_by(maker: &Path, tag: &str) -> Repo {
    let tmp =
        Tmp(std::env::temp_dir().join(format!("inklkunn-marker-{}-{tag}", std::process::id())));
    let _ = std::fs::remove_dir_all(&tmp.0);
    std::fs::create_dir_all(tmp.0.join("home")).unwrap();
    let root = RootLayout::new(tmp.0.join("root"));
    let wiki = WikiId::new();
    let repo = root.prepare(&wiki).expect("directories");
    let out = std::process::Command::new(maker)
        .arg("setup")
        .arg(&repo)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", tmp.0.join("home"))
        .env("XDG_CONFIG_HOME", tmp.0.join("home/cfg"))
        .output()
        .expect("prikk setup runs");
    assert!(out.status.success(), "{out:?}");
    Repo {
        _tmp: tmp,
        root,
        wiki,
        repo,
    }
}

impl Repo {
    fn marker(&self, bytes: &[u8]) {
        std::fs::write(self.repo.join(".prikk/FORMAT"), bytes).unwrap();
    }

    /// What the driver and the signer each make of one read by `reader`.
    fn read_by(&self, reader: &Path) -> (Result<(), StorageError>, Result<(), SignerError>) {
        let driver = PrikkCli::open(
            reader,
            Sandbox::Unconfined,
            Admission::new(1 << 30, 64 << 20, Duration::from_millis(200)),
        )
        .expect("a prikk at the floor");
        let signer = Witnesses::new(reader, Sandbox::Unconfined, self.root.clone());
        (
            driver.status(&self.repo).map(|_| ()),
            signer.doctor(&self.wiki).map(|_| ()),
        )
    }
}

#[test]
fn a_marker_prikk_0_57_0_will_not_read_is_a_damaged_or_unknown_marker_for_the_driver_and_the_signer()
 {
    let Some(new) = prikk_where(|v| v >= (0, 57, 0)) else {
        println!("skipping: prikk 0.57.0 or newer not found");
        return;
    };
    let repo = made_by(&new, "new");
    // Sound: prikk reads what it made.
    let (driver, signer) = repo.read_by(&new);
    assert!(driver.is_ok(), "{driver:?}");
    assert!(signer.is_ok(), "{signer:?}");
    // `8` without its newline: prikk 0.57.0 itself says `unsupported format version: 8` (measured). The version in the
    // answer is the one prikk gave; nothing says the repository is intact.
    // `9`: a number no table knows.
    for (what, bytes, format) in [
        ("8 without its newline", b"8".as_slice(), 8),
        ("9", b"9\n".as_slice(), 9),
    ] {
        repo.marker(bytes);
        let (driver, signer) = repo.read_by(&new);
        match driver {
            Err(StorageError::FormatMarker { version, .. }) => {
                assert_eq!(version, format, "{what}")
            }
            other => panic!("{what}: the driver said {other:?}"),
        }
        match signer {
            Err(SignerError::FormatMarker { version }) => assert_eq!(version, format, "{what}"),
            other => panic!("{what}: the signer said {other:?}"),
        }
        let said = StorageError::FormatMarker {
            verb: "status",
            version: format,
        }
        .to_string();
        assert!(said.contains(&format!("format {format}")), "{said}");
        assert!(said.contains(".prikk/FORMAT"), "{said}");
        assert!(
            !said.contains("nothing in the repository is damaged"),
            "{said}"
        );
    }
}

#[test]
fn a_format_8_repository_read_by_a_prikk_older_than_0_57_0_is_the_operators_mismatch_for_the_driver_and_the_signer()
 {
    let Some(new) = prikk_where(|v| v >= (0, 57, 0)) else {
        println!("skipping: prikk 0.57.0 or newer not found");
        return;
    };
    let Some(old) = prikk_where(|v| ((0, 54, 0)..(0, 57, 0)).contains(&v)) else {
        println!("skipping: no prikk older than 0.57.0 (and at the floor) found");
        return;
    };
    let repo = made_by(&new, "old");
    let (driver, signer) = repo.read_by(&old);
    match driver {
        Err(StorageError::UnsupportedFormat { version, .. }) => assert_eq!(version, 8),
        other => panic!("the driver said {other:?}"),
    }
    match signer {
        Err(SignerError::UnsupportedFormat { version }) => assert_eq!(version, 8),
        other => panic!("the signer said {other:?}"),
    }
    // The same words about the same repository from the prikk that made it are not a mismatch once the marker is hurt,
    // and an older prikk that is told `9` does not know it either.
    repo.marker(b"9\n");
    let (driver, signer) = repo.read_by(&old);
    assert!(
        matches!(driver, Err(StorageError::FormatMarker { version: 9, .. })),
        "{driver:?}"
    );
    assert!(
        matches!(signer, Err(SignerError::FormatMarker { version: 9 })),
        "{signer:?}"
    );
}
