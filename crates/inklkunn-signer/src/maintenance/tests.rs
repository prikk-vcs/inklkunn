#![allow(clippy::expect_used, clippy::unwrap_used)]
//! The keyless writable mode against a stand-in `prikk` that records what it was asked and what it could see
//! (no real prikk needed: `tests/maintenance.rs` runs the real one).

use std::os::unix::fs::PermissionsExt;

use inklkunn_prikk::sandbox::BwrapConfig;

use super::*;
use crate::flock::{self, Acquired};

struct Fixture {
    dir: PathBuf,
    root: RootLayout,
    wiki: WikiId,
    log: PathBuf,
    mode: PathBuf,
    standin: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn script(path: &Path, text: &str) {
    std::fs::write(path, text).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A wiki directory, a stand-in `prikk` and a fake `bwrap` that logs its arguments and runs what follows `--`.
fn fixture(tag: &str) -> Fixture {
    let dir = std::env::temp_dir().join(format!("inklkunn-maint-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let root = RootLayout::new(dir.join("wikis"));
    let wiki = WikiId::new();
    let repo = root.prepare(&wiki).unwrap();
    // The cost rule measures the store: a repository has a `.prikk` directory.
    std::fs::create_dir_all(repo.join(".prikk")).unwrap();
    let log = dir.join("log");
    let mode = dir.join("mode");
    let standin = dir.join("prikk");
    script(
        &standin,
        &format!(
            r#"#!/bin/sh
echo "ARGS:$*" >> {log}
env | grep -c '^PRIKK' >> {log}
M=$(cat {mode} 2>/dev/null)
case "$1" in
  archive)
    case "$M" in
      short) printf 'ARCHIVE' > "$3"; SIZE=99 ;;
      link) ln -s {secret} "$3"; SIZE=6 ;;
      racer) printf 'ARCHIVE-BYTES' > "$3"; printf 'theirs' > {dest}; SIZE=13 ;;
      *) printf 'ARCHIVE-BYTES' > "$3"; SIZE=13 ;;
    esac
    SCHEMA=prepo-export-report-v1
    [ "$M" = schema2 ] && SCHEMA=prepo-export-report-v2
    echo "{{\"schema_version\":\"$SCHEMA\",\"ok\":true,\"output\":\"x\",\"repository_format\":7,\"tool_version\":\"0.55.0\",\"total_bytes\":$SIZE}}"
    ;;
  verify)
    case "$M" in
      partial) echo '{{"schema_version":"verify-report-v1","verdict":{{"ok":false}},"stages":[{{"stage":"objects","status":"failed","message":"i/o error: out of memory"}}]}}' ;;
      *) echo '{{"schema_version":"verify-report-v1","verdict":{{"ok":true}},"stages":[{{"stage":"objects","status":"evaluated"}}]}}' ;;
    esac
    ;;
esac
"#,
            log = log.display(),
            mode = mode.display(),
            secret = dir.join("secret").display(),
            dest = dir.join("out.prikk").display()
        ),
    );
    std::fs::write(dir.join("secret"), "SECRET").unwrap();
    Fixture {
        dir,
        root,
        wiki,
        log,
        mode,
        standin,
    }
}

impl Fixture {
    fn maintenance(&self) -> Maintenance {
        Maintenance::new(self.standin.clone(), Sandbox::Unconfined, self.root.clone())
    }

    /// Under a fake bwrap that records every flag it was given, then runs the stand-in.
    fn confined(&self) -> Maintenance {
        let bwrap = self.dir.join("fake-bwrap");
        script(
            &bwrap,
            &format!(
                r#"#!/bin/sh
echo "BWRAP:$*" >> {log}
while [ "$1" != "--" ]; do shift; done
shift
exec "$@"
"#,
                log = self.log.display()
            ),
        );
        Maintenance::new(
            self.standin.clone(),
            Sandbox::Bwrap(BwrapConfig {
                bwrap: bwrap.into_os_string(),
                ..BwrapConfig::default()
            }),
            self.root.clone(),
        )
    }

    fn log(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn set_mode(&self, mode: &str) {
        std::fs::write(&self.mode, mode).unwrap();
    }

    fn destination(&self) -> PathBuf {
        self.dir.join("out.prikk")
    }
}

#[test]
fn an_archive_export_runs_the_pinned_verb_with_no_key_and_hands_over_a_private_file() {
    let f = fixture("export");
    let done = f
        .maintenance()
        .archive_export(&f.wiki, &f.destination())
        .unwrap();
    assert_eq!(
        done,
        ArchiveExported {
            bytes: 13,
            repository_format: 7,
            prikk_version: "0.55.0".to_owned()
        }
    );
    let log = f.log();
    assert!(
        log.contains("ARGS:archive export ") && log.contains(" --format json"),
        "{log}"
    );
    // The prikk process saw no PRIKK_* variable at all (the count of such variables printed by the stand-in).
    assert!(log.lines().any(|l| l == "0"), "{log}");
    assert_eq!(
        std::fs::read(f.destination()).unwrap(),
        b"ARCHIVE-BYTES".to_vec()
    );
    let mode = std::fs::metadata(f.destination())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600, "an archive holds private pages");
    // The scratch copy is gone.
    let left: Vec<_> = std::fs::read_dir(f.root.scratch_dir(&f.wiki))
        .unwrap()
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

#[test]
fn the_keyless_mode_mounts_the_repository_and_one_output_directory_and_no_key_or_variable() {
    let f = fixture("mounts");
    f.confined()
        .archive_export(&f.wiki, &f.destination())
        .unwrap();
    f.confined().refresh_checkpoint(&f.wiki).unwrap();
    let log = f.log();
    let calls: Vec<&str> = log.lines().filter(|l| l.starts_with("BWRAP:")).collect();
    assert_eq!(calls.len(), 2, "{log}");
    let repo = f.root.repo_dir(&f.wiki).canonicalize().unwrap();
    let scratch = f.root.scratch_dir(&f.wiki).canonicalize().unwrap();
    for call in &calls {
        // The repository is bound writable…
        assert!(
            call.contains(&format!("--bind {} {}", repo.display(), repo.display())),
            "{call}"
        );
        // …and nothing is bound read-only but the system and the binary: no key directory.
        let ro: Vec<&str> = call
            .split("--ro-bind ")
            .skip(1)
            .filter_map(|s| s.split_whitespace().next())
            .collect();
        assert!(
            ro.iter().all(|p| *p == "/usr" || p.ends_with("/prikk")),
            "{call}: {ro:?}"
        );
        // …and no variable but PATH is set.
        let set: Vec<&str> = call
            .split("--setenv ")
            .skip(1)
            .filter_map(|s| s.split_whitespace().next())
            .collect();
        assert_eq!(set, vec!["PATH"], "{call}");
    }
    // Only the export has the output directory; the refresh has the repository alone.
    assert!(calls.first().is_some_and(|c| c.contains(&format!(
        "--bind {} {}",
        scratch.display(),
        scratch.display()
    ))));
    assert!(
        calls
            .get(1)
            .is_some_and(|c| !c.contains(&scratch.display().to_string()))
    );
}

#[test]
fn a_destination_that_exists_or_is_relative_or_has_no_directory_is_refused_before_prikk_runs() {
    let f = fixture("refused");
    std::fs::write(f.destination(), "mine").unwrap();
    for (dest, why) in [
        (f.destination(), "already exists"),
        (PathBuf::from("relative.prikk"), "absolute"),
        (f.dir.join("nowhere/x.prikk"), "directory does not exist"),
    ] {
        let err = f.maintenance().archive_export(&f.wiki, &dest).unwrap_err();
        assert!(
            matches!(&err, SignerError::Refused(m) if m.contains(why)),
            "{dest:?}: {err:?}"
        );
    }
    assert_eq!(f.log(), "", "prikk ran for a refused destination");
    assert_eq!(std::fs::read(f.destination()).unwrap(), b"mine".to_vec());
}

#[test]
fn an_archive_that_disagrees_with_its_report_or_its_schema_leaves_no_destination() {
    let f = fixture("damaged");
    for mode in ["short", "schema2"] {
        f.set_mode(mode);
        let err = f
            .maintenance()
            .archive_export(&f.wiki, &f.destination())
            .unwrap_err();
        assert!(matches!(err, SignerError::Damaged(_)), "{mode}: {err:?}");
        assert!(!f.destination().exists(), "{mode}");
        assert!(
            std::fs::read_dir(f.root.scratch_dir(&f.wiki))
                .unwrap()
                .next()
                .is_none(),
            "{mode}: the scratch copy was left"
        );
    }
}

#[test]
fn an_export_and_a_refresh_wait_for_nobody_and_say_busy_while_another_act_writes_the_wiki() {
    let f = fixture("busy");
    let Acquired::Held(_writer) = flock::try_exclusive(&f.root.lock_file(&f.wiki)).unwrap() else {
        panic!("the lock was busy");
    };
    let err = f
        .maintenance()
        .archive_export(&f.wiki, &f.destination())
        .unwrap_err();
    assert!(matches!(err, SignerError::Busy { .. }), "{err:?}");
    let err = f.maintenance().refresh_checkpoint(&f.wiki).unwrap_err();
    assert!(matches!(err, SignerError::Busy { .. }), "{err:?}");
    assert_eq!(f.log(), "", "prikk ran beside a writer");
    assert!(!f.destination().exists());
}

#[test]
fn a_killed_exports_leftover_in_the_scratch_directory_is_removed_by_the_next() {
    let f = fixture("leftover");
    let stale = f.root.scratch_dir(&f.wiki).join("archive-export-1-2.tmp");
    std::fs::write(&stale, "half an archive").unwrap();
    let other = f.root.scratch_dir(&f.wiki).join("tip-index");
    std::fs::create_dir(&other).unwrap();
    f.maintenance()
        .archive_export(&f.wiki, &f.destination())
        .unwrap();
    assert!(!stale.exists());
    assert!(other.is_dir(), "the read path's scratch was touched");
}

#[test]
fn a_refresh_is_a_writable_verify_and_a_partial_report_is_not_a_refresh() {
    let f = fixture("refresh");
    let v = f.maintenance().refresh_checkpoint(&f.wiki).unwrap();
    assert!(v.verdict.ok);
    assert!(f.log().contains("ARGS:verify --format json"), "{}", f.log());
    f.set_mode("partial");
    let err = f.maintenance().refresh_checkpoint(&f.wiki).unwrap_err();
    assert!(
        matches!(err, SignerError::CeilingExceeded { .. }),
        "a memory ceiling is not a verdict: {err:?}"
    );
    // An unknown wiki has no repository to refresh.
    let err = f
        .maintenance()
        .refresh_checkpoint(&WikiId::new())
        .unwrap_err();
    assert!(matches!(err, SignerError::Refused(_)), "{err:?}");
}

#[test]
fn a_link_where_the_archive_should_be_is_refused_and_what_it_points_to_is_not_copied() {
    let f = fixture("link");
    f.set_mode("link");
    let err = f
        .maintenance()
        .archive_export(&f.wiki, &f.destination())
        .unwrap_err();
    assert!(
        matches!(&err, SignerError::Damaged(m) if m.contains("a link")),
        "{err:?}"
    );
    assert!(!f.destination().exists(), "the link's target was copied");
    assert_eq!(
        std::fs::read(f.dir.join("secret")).unwrap(),
        b"SECRET".to_vec()
    );
    assert!(
        std::fs::read_dir(f.root.scratch_dir(&f.wiki))
            .unwrap()
            .next()
            .is_none(),
        "the planted link was left"
    );
}

#[test]
fn the_archive_handle_must_be_a_regular_file_on_the_scratch_directorys_device() {
    let f = fixture("device");
    let scratch = f.root.scratch_dir(&f.wiki);
    std::fs::create_dir_all(&scratch).unwrap();
    let file = scratch.join("archive-export-1-1.tmp");
    std::fs::write(&file, "twelve bytes").unwrap();
    let (_, size) = open_archive(&file, &scratch).unwrap();
    assert_eq!(size, 12);
    // A directory is not an archive.
    let sub = scratch.join("archive-export-2-2.tmp");
    std::fs::create_dir(&sub).unwrap();
    let err = open_archive(&sub, &scratch).unwrap_err();
    assert!(
        matches!(&err, SignerError::Damaged(m) if m.contains("regular file")),
        "{err:?}"
    );
    // A file on another device than the scratch directory is not what prikk wrote there. `/proc` is never the
    // device of a temporary directory.
    let proc = Path::new("/proc");
    if proc.is_dir()
        && std::fs::metadata(proc).unwrap().dev() != std::fs::metadata(&scratch).unwrap().dev()
    {
        let err = open_archive(&file, proc).unwrap_err();
        assert!(
            matches!(&err, SignerError::Damaged(m) if m.contains("device")),
            "{err:?}"
        );
    } else {
        println!("skipping the device check: /proc is not on another device here");
    }
}

#[test]
fn a_destination_that_appeared_meanwhile_is_refused_and_not_removed() {
    let f = fixture("racer");
    f.set_mode("racer");
    let err = f
        .maintenance()
        .archive_export(&f.wiki, &f.destination())
        .unwrap_err();
    assert!(
        matches!(&err, SignerError::Unavailable(m) if m.contains("could not be created")),
        "{err:?}"
    );
    assert_eq!(
        std::fs::read(f.destination()).unwrap(),
        b"theirs".to_vec(),
        "a file this call did not make was removed"
    );
}

#[test]
fn an_export_and_a_refresh_inside_a_lease_borrow_it_and_a_handle_outside_it_is_told_busy() {
    use crate::{LeaseRefusal, WriteLeases};
    let f = fixture("leased");
    let leases = WriteLeases::new(f.root.clone());
    let leased = f.maintenance().with_leases(&leases);
    let lease = leases.try_acquire(&f.wiki).unwrap();
    {
        let _scope = lease.scope();
        leased
            .archive_export(&f.wiki, &f.destination())
            .expect("the export borrows the lease it runs inside");
        leased
            .refresh_checkpoint(&f.wiki)
            .expect("so does the refresh");
    }
    // The acts gave nothing up: the lease is still held.
    assert_eq!(
        leases.try_acquire(&f.wiki).unwrap_err(),
        LeaseRefusal::ThisProcess
    );
    // The command line beside the server (a handle with turns of its own) finds the file lock held: busy, and
    // nothing was done.
    let cli = f.maintenance();
    assert!(matches!(
        cli.refresh_checkpoint(&f.wiki),
        Err(SignerError::Busy { .. })
    ));
    let before = f.log();
    assert!(matches!(
        cli.archive_export(&f.wiki, &f.dir.join("second.prikk")),
        Err(SignerError::Busy { .. })
    ));
    assert_eq!(f.log(), before, "prikk was not run");
}
