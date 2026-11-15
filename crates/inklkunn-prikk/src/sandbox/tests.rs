#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

fn args_of(c: &Command) -> Vec<String> {
    c.get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

fn envs_of(c: &Command) -> Vec<(String, Option<String>)> {
    c.get_envs()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.map(|v| v.to_string_lossy().into_owned()),
            )
        })
        .collect()
}

struct Tmp(PathBuf);

impl Tmp {
    fn new(tag: &str) -> Self {
        let p = std::env::temp_dir().join(format!("inklkunn-sandbox-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("tmp");
        Tmp(p)
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn unconfined_runs_the_binary_directly_with_a_cleared_environment() {
    let c = Sandbox::Unconfined.command_mounts(
        OsStr::new("prikk"),
        &Mounts::read(Path::new("/repo"), None),
        &["branch"],
        CallLimits {
            data_ceiling: None,
            ..CallLimits::default()
        },
    );
    // The name is resolved to a path before the environment is cleared (a bare name would not be found).
    assert_eq!(
        Path::new(c.get_program()).file_name(),
        Some(OsStr::new("prikk"))
    );
    assert_eq!(args_of(&c), ["branch"]);
    assert!(c.get_envs().len() == 1, "only PATH: {:?}", envs_of(&c));
}

#[test]
fn bwrap_binds_the_repository_read_only_and_the_scratch_read_write() {
    let c = Sandbox::default().command_mounts(
        OsStr::new("prikk"),
        &Mounts::read(Path::new("/repo"), Some(Path::new("/scratch"))),
        &["tree", "--format", "json"],
        CallLimits {
            data_ceiling: None,
            ..CallLimits::default()
        },
    );
    assert_eq!(c.get_program(), OsStr::new("bwrap"));
    let a = args_of(&c);
    assert!(
        a.windows(3).any(|w| w == ["--ro-bind", "/repo", "/repo"]),
        "{a:?}"
    );
    assert!(
        a.windows(3)
            .any(|w| w == ["--bind", "/scratch", "/scratch"]),
        "{a:?}"
    );
    assert!(
        !a.windows(3).any(|w| w == ["--bind", "/repo", "/repo"]),
        "repo must never be rw"
    );
    for flag in [
        "--unshare-all",
        "--die-with-parent",
        "--new-session",
        "--clearenv",
    ] {
        assert!(a.contains(&flag.to_owned()), "{flag}");
    }
    assert!(a.windows(2).any(|w| w == ["--chdir", "/repo"]));
    assert_eq!(a.last().map(String::as_str), Some("json"));
    assert_eq!(c.get_envs().len(), 1, "bwrap itself gets only PATH");
}

#[test]
fn a_ceiling_puts_prlimit_in_front_with_the_same_environment() {
    let c = Sandbox::default().command_mounts(
        OsStr::new("prikk"),
        &Mounts::read(Path::new("/repo"), None),
        &["verify"],
        CallLimits {
            data_ceiling: Some(1 << 20),
            ..CallLimits::default()
        },
    );
    assert_eq!(c.get_program(), OsStr::new("prlimit"));
    let a = args_of(&c);
    assert_eq!(a.first().map(String::as_str), Some("--data=1048576"));
    assert_eq!(a.get(1).map(String::as_str), Some("--"));
    assert_eq!(a.get(2).map(String::as_str), Some("bwrap"));
    assert_eq!(c.get_envs().len(), 1);
}

#[test]
fn mounts_are_resolved_and_checked() {
    let tmp = Tmp::new("mounts");
    let repo = tmp.0.join("repo");
    let scratch = tmp.0.join("scratch");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::create_dir_all(&scratch).unwrap();
    let ok = Mounts::read(&repo, Some(&scratch))
        .validate()
        .expect("valid");
    assert_eq!(ok.repo(), repo.canonicalize().unwrap());
    assert!(!ok.is_writable());
    // Inside the repository: overlap.
    std::fs::create_dir_all(repo.join("x")).unwrap();
    assert!(matches!(
        Mounts::read(&repo, Some(&repo.join("x"))).validate(),
        Err(MountError::Overlap(..))
    ));
    // Not absolute, or with dot components: refused before resolution.
    assert!(matches!(
        Mounts::read(Path::new("relative"), None).validate(),
        Err(MountError::InvalidPath(_))
    ));
    assert!(matches!(
        Mounts::read(&repo, Some(&tmp.0.join("scratch/../scratch"))).validate(),
        Err(MountError::InvalidPath(_))
    ));
    // Missing: unresolvable.
    assert!(matches!(
        Mounts::read(&tmp.0.join("nope"), None).validate(),
        Err(MountError::Unresolvable(..))
    ));
    // A symlink that points into the repository is caught once resolved.
    let link = tmp.0.join("link");
    std::os::unix::fs::symlink(repo.join("x"), &link).unwrap();
    assert!(matches!(
        Mounts::read(&repo, Some(&link)).validate(),
        Err(MountError::Overlap(..))
    ));
}

#[test]
fn resolve_binary_makes_paths_absolute() {
    assert_eq!(
        resolve_binary(OsStr::new("/opt/prikk")),
        PathBuf::from("/opt/prikk")
    );
    assert!(resolve_binary(OsStr::new("./prikk")).is_absolute());
}

#[test]
fn write_mounts_bind_the_repo_rw_the_keys_ro_and_set_the_variables() {
    let m = Mounts::write(
        Path::new("/repo"),
        Some(Path::new("/keys")),
        vec![(
            "PRIKK_AUTHOR_SEED_FILE".to_owned(),
            "/keys/author.seed".to_owned(),
        )],
    );
    assert!(m.is_writable());
    let c = Sandbox::default().command_mounts(
        OsStr::new("prikk"),
        &m,
        &["log", "--format", "json"],
        CallLimits::default(),
    );
    let a = args_of(&c);
    assert!(
        a.windows(3).any(|w| w == ["--bind", "/repo", "/repo"]),
        "{a:?}"
    );
    assert!(
        a.windows(3).any(|w| w == ["--ro-bind", "/keys", "/keys"]),
        "{a:?}"
    );
    assert!(
        a.windows(3)
            .any(|w| w == ["--setenv", "PRIKK_AUTHOR_SEED_FILE", "/keys/author.seed"]),
        "{a:?}"
    );
    let tmp = Tmp::new("write");
    let repo = tmp.0.join("repo");
    std::fs::create_dir_all(repo.join("keys")).unwrap();
    let overlapping = Mounts::write(&repo, Some(&repo.join("keys")), Vec::new());
    assert!(matches!(
        overlapping.validate(),
        Err(MountError::Overlap(..))
    ));
    let one = Mounts::dir_writable(&repo);
    assert!(one.is_writable());
    assert!(one.validate().is_ok());
}

#[test]
fn a_write_without_a_key_directory_mounts_no_keys_and_sets_no_variables() {
    let m = Mounts::write(Path::new("/repo"), None, Vec::new());
    let c = Sandbox::default().command_mounts(
        OsStr::new("prikk"),
        &m,
        &["log", "--format", "json"],
        CallLimits::default(),
    );
    let a = args_of(&c);
    assert!(
        a.windows(3).any(|w| w == ["--bind", "/repo", "/repo"]),
        "{a:?}"
    );
    assert!(!a.iter().any(|x| x.contains("keys")), "{a:?}");
    assert!(!a.iter().any(|x| x.starts_with("PRIKK_")), "{a:?}");
}

#[test]
fn the_keyless_writable_mode_binds_the_repo_and_one_output_directory_and_nothing_of_keys() {
    let m = Mounts::write_keyless_with_output(Path::new("/repo"), Path::new("/out"));
    assert!(m.is_writable());
    assert!(m.env().is_empty());
    let c = Sandbox::default().command_mounts(
        OsStr::new("prikk"),
        &m,
        &["verify", "--format", "json"],
        CallLimits::default(),
    );
    let a = args_of(&c);
    assert!(
        a.windows(3).any(|w| w == ["--bind", "/repo", "/repo"]),
        "{a:?}"
    );
    assert!(
        a.windows(3).any(|w| w == ["--bind", "/out", "/out"]),
        "{a:?}"
    );
    // Only the system, the binary, the repository and the output: no key directory, no variable but PATH.
    assert!(!a.iter().any(|x| x.contains("keys")), "{a:?}");
    assert!(!a.iter().any(|x| x.starts_with("PRIKK_")), "{a:?}");
    assert_eq!(a.iter().filter(|x| x.as_str() == "--setenv").count(), 1);
    // The output may not sit inside the repository.
    let tmp = Tmp::new("keyless");
    let repo = tmp.0.join("repo");
    std::fs::create_dir_all(repo.join("out")).unwrap();
    assert!(matches!(
        Mounts::write_keyless_with_output(&repo, &repo.join("out")).validate(),
        Err(MountError::Overlap(..))
    ));
}

fn run_of(status: i32, stderr: &str) -> Run {
    use std::os::unix::process::ExitStatusExt;
    Run {
        status: std::process::ExitStatus::from_raw(status),
        stdout: Vec::new(),
        stderr: stderr.as_bytes().to_vec(),
        timed_out: false,
        output_exceeded: false,
    }
}

#[test]
fn every_way_a_call_dies_of_its_memory_ceiling_is_read_as_the_ceiling_and_a_refusal_is_not() {
    // from_raw takes a wait status: exit code n is n << 8; a death by signal n is n.
    let exited = |code: i32| code << 8;
    for (what, run) in [
        (
            "the allocator aborts",
            run_of(6, "memory allocation of 629145648 bytes failed"),
        ),
        (
            "the allocator aborts under bwrap",
            run_of(exited(134), "memory allocation of 8 bytes failed"),
        ),
        ("a segfault before any message", run_of(11, "")),
        ("bwrap reports a signal as 128 + n", run_of(exited(139), "")),
        (
            "the loader cannot allocate",
            run_of(
                exited(127),
                "error while loading shared libraries: cannot allocate TLS",
            ),
        ),
        (
            "the loader cannot map",
            run_of(exited(127), "cannot map zero-fill pages"),
        ),
        ("bwrap is out of memory", run_of(exited(1), "Out of memory")),
    ] {
        assert!(run.died_of_ceiling(), "{what}");
    }
    for (what, run) in [
        (
            "a refusal",
            run_of(exited(1), "error: precondition not met: something"),
        ),
        ("a success", run_of(0, "")),
        (
            "an unrelated 127",
            run_of(exited(127), "sh: prikk: not found"),
        ),
        ("a plain failure", run_of(exited(2), "")),
    ] {
        assert!(!run.died_of_ceiling(), "{what}");
    }
}

#[test]
fn a_run_is_bounded_in_time_and_output() {
    // `sh` is always present; the runner is policy-independent here.
    let mut slow = Command::new("/bin/sh");
    slow.args(["-c", "sleep 5"]);
    let r = run_bounded(slow, Some(Duration::from_millis(200)), 1 << 20).expect("runs");
    assert!(r.timed_out);
    assert!(r.stdout.is_empty());

    let mut chatty = Command::new("/bin/sh");
    chatty.args(["-c", "yes | head -c 200000"]);
    let r = run_bounded(chatty, Some(Duration::from_secs(10)), 1000).expect("runs");
    assert!(r.output_exceeded);
    assert!(!r.timed_out);

    let mut fine = Command::new("/bin/sh");
    fine.args(["-c", "echo ok; echo err >&2; exit 3"]);
    let r = run_bounded(fine, Some(Duration::from_secs(10)), 1 << 20).expect("runs");
    assert_eq!(r.status.code(), Some(3));
    assert_eq!(r.stdout, b"ok\n");
    assert_eq!(r.stderr, b"err\n");
    assert!(!r.timed_out && !r.output_exceeded);
}

#[test]
fn a_run_can_be_given_input_and_a_child_that_never_reads_it_is_not_an_error() {
    let mut cat = Command::new("/bin/sh");
    cat.args(["-c", "cat"]);
    let r = run_command(cat, Some(b"hello"), Some(Duration::from_secs(10)), 1000).expect("runs");
    assert_eq!(r.stdout, b"hello");
    assert!(r.status.success());

    let mut deaf = Command::new("/bin/sh");
    deaf.args(["-c", "exit 4"]);
    let r = run_command(deaf, Some(b"ignored"), Some(Duration::from_secs(10)), 1000).expect("runs");
    assert_eq!(r.status.code(), Some(4));

    // Without input the child's stdin is closed.
    let mut cat = Command::new("/bin/sh");
    cat.args(["-c", "cat; echo end"]);
    let r = run_command(cat, None, Some(Duration::from_secs(10)), 1000).expect("runs");
    assert_eq!(r.stdout, b"end\n");
}

/// `probe_bwrap` over a stand-in `bwrap` that prints `stderr` and exits with `code`, retrying while the kernel says
/// the script just written is still open in a concurrently forked child (`ETXTBSY`).
fn probe_stand_in(tag: &str, stderr: &[u8], code: i32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let tmp = Tmp::new(tag);
    let said = tmp.0.join("said");
    std::fs::write(&said, stderr).expect("stderr file");
    let script = tmp.0.join("bwrap");
    std::fs::write(
        &script,
        format!("#!/bin/sh\ncat '{}' >&2\nexit {code}\n", said.display()),
    )
    .expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    for _ in 0..50 {
        match probe_bwrap(script.as_os_str()) {
            Err(e) if e.contains("busy") => std::thread::sleep(Duration::from_millis(40)),
            other => return other,
        }
    }
    probe_bwrap(script.as_os_str())
}

#[test]
fn a_bwrap_that_cannot_confine_says_why_in_its_own_first_line() {
    let e = probe_stand_in(
        "why",
        b"bwrap: setting up uid map: Permission denied\nsecond line\n",
        1,
    )
    .unwrap_err();
    assert!(e.contains("cannot confine a process here"), "{e}");
    assert!(
        e.ends_with(": bwrap: setting up uid map: Permission denied"),
        "{e}"
    );
    assert!(!e.contains("second line"), "{e}");
    // Blank lines before it are not the line.
    let e = probe_stand_in(
        "blank",
        b"\n  \nbwrap: No permissions to create new namespace\n",
        1,
    )
    .unwrap_err();
    assert!(
        e.ends_with(": bwrap: No permissions to create new namespace"),
        "{e}"
    );
}

#[test]
fn the_reason_is_cleaned_and_bounded() {
    let e = probe_stand_in(
        "clean",
        "bwrap:\u{1b}[31m\u{200b} no\u{2028}\u{202e} way\r\n".as_bytes(),
        1,
    )
    .unwrap_err();
    assert!(e.ends_with(": bwrap:[31m no way"), "{e:?}");
    let long = format!("bwrap: {}\n", "x".repeat(1000));
    let e = probe_stand_in("long", long.as_bytes(), 1).unwrap_err();
    let said = e.split_once("): ").expect("a reason").1;
    assert!(said.starts_with("bwrap: xxx"), "{e}");
    assert_eq!(said.chars().count(), 200, "{e}");
    // Bytes that are not UTF-8 are read lossily, never a failure.
    let e = probe_stand_in("bytes", &[0xff, b'n', b'o', b'\n'], 1).unwrap_err();
    assert!(e.ends_with("no"), "{e:?}");
}

#[test]
fn a_bwrap_that_says_nothing_is_reported_by_its_exit_status() {
    let e = probe_stand_in("silent", b"", 3).unwrap_err();
    assert!(e.contains("cannot confine a process here"), "{e}");
    assert!(
        e.contains("exit status: 3") && e.contains("said nothing"),
        "{e}"
    );
    // A reason made only of characters that are dropped is no reason.
    let e = probe_stand_in("invisible", "\u{200b}\u{7}\n".as_bytes(), 2).unwrap_err();
    assert!(e.contains("said nothing"), "{e}");
}

#[test]
fn a_bwrap_that_works_and_one_that_is_missing_are_told_apart() {
    assert!(probe_stand_in("fine", b"", 0).is_ok());
    let e = probe_bwrap(std::ffi::OsStr::new("/nonexistent/bwrap")).unwrap_err();
    assert!(e.contains("could not run"), "{e}");
}

#[test]
fn the_format_character_set_is_the_category_cf_of_unicode_16() {
    // `char` has no general-category table in the standard library; the category (Unicode 16.0) has 170 code
    // points, and the set holds exactly those. The boundaries of each range are pinned by name.
    let count = (0..=0x0010_FFFF_u32)
        .filter_map(char::from_u32)
        .filter(|c| is_format_character(*c))
        .count();
    assert_eq!(count, 170);
    for yes in [
        '\u{ad}',
        '\u{600}',
        '\u{605}',
        '\u{61c}',
        '\u{6dd}',
        '\u{70f}',
        '\u{890}',
        '\u{891}',
        '\u{8e2}',
        '\u{180e}',
        '\u{200b}',
        '\u{200c}',
        '\u{200d}',
        '\u{200e}',
        '\u{200f}',
        '\u{202a}',
        '\u{202e}',
        '\u{2060}',
        '\u{2064}',
        '\u{2066}',
        '\u{2069}',
        '\u{206f}',
        '\u{feff}',
        '\u{fff9}',
        '\u{fffb}',
        '\u{110bd}',
        '\u{110cd}',
        '\u{13430}',
        '\u{1343f}',
        '\u{1bca0}',
        '\u{1bca3}',
        '\u{1d173}',
        '\u{1d17a}',
        '\u{e0001}',
        '\u{e0020}',
        '\u{e007f}',
    ] {
        assert!(is_format_character(yes), "U+{:04X}", yes as u32);
    }
    for no in [
        ' ',
        'a',
        '\u{3042}',
        '\u{ac}',
        '\u{ae}',
        '\u{606}',
        '\u{2028}',
        '\u{2029}',
        '\u{205f}',
        '\u{2065}',
        '\u{fefe}',
        '\u{ff00}',
        '\u{e0000}',
        '\u{e0002}',
        '\u{e0080}',
    ] {
        assert!(!is_format_character(no), "U+{:04X}", no as u32);
    }
}
