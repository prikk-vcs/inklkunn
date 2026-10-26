#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

fn args_of(c: &Command) -> Vec<String> {
    c.get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn unconfined_runs_the_binary_directly() {
    let c = Sandbox::Unconfined.command(
        OsStr::new("prikk"),
        Path::new("/repo"),
        None,
        &["branch"],
        CallLimits::default(),
    );
    assert_eq!(c.get_program(), OsStr::new("prikk"));
    assert_eq!(args_of(&c), ["branch"]);
}

#[test]
fn bwrap_binds_the_repository_read_only_and_the_scratch_read_write() {
    let c = Sandbox::default().command(
        OsStr::new("prikk"),
        Path::new("/repo"),
        Some(Path::new("/scratch")),
        &["tree", "--format", "json"],
        CallLimits::default(),
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
}

#[test]
fn a_ceiling_puts_prlimit_in_front() {
    let c = Sandbox::default().command(
        OsStr::new("prikk"),
        Path::new("/repo"),
        None,
        &["verify"],
        CallLimits {
            data_ceiling: Some(1 << 20),
        },
    );
    assert_eq!(c.get_program(), OsStr::new("prlimit"));
    let a = args_of(&c);
    assert_eq!(a.first().map(String::as_str), Some("--data=1048576"));
    assert_eq!(a.get(1).map(String::as_str), Some("--"));
    assert_eq!(a.get(2).map(String::as_str), Some("bwrap"));
}

#[test]
fn mounts_are_validated() {
    assert!(validate_mounts(Path::new("/a/repo"), Some(Path::new("/a/scratch"))).is_ok());
    assert!(matches!(
        validate_mounts(Path::new("/a/repo"), Some(Path::new("/a/repo/x"))),
        Err(MountError::Overlap(..))
    ));
    assert!(matches!(
        validate_mounts(Path::new("relative"), None),
        Err(MountError::NotAbsolute(_))
    ));
    assert!(matches!(
        validate_mounts(Path::new("/a"), Some(Path::new("/b/../c"))),
        Err(MountError::NotAbsolute(_))
    ));
}

#[test]
fn resolve_binary_passes_paths_through() {
    assert_eq!(
        resolve_binary(OsStr::new("/opt/prikk")),
        PathBuf::from("/opt/prikk")
    );
}
