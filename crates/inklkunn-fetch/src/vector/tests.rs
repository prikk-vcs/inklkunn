#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

fn args_of(c: &Command) -> Vec<String> {
    c.get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect()
}

fn command() -> Command {
    bwrap_command(
        OsStr::new("bwrap"),
        Path::new("/usr/bin/curl"),
        Path::new("/etc/ca-certificates/extracted/tls-ca-bundle.pem"),
        &[
            "-q".to_owned(),
            "--url".to_owned(),
            "https://x.example/".to_owned(),
        ],
    )
}

fn has_sequence(args: &[String], seq: &[&str]) -> bool {
    args.windows(seq.len())
        .any(|w| w.iter().zip(seq).all(|(a, b)| a == b))
}

#[test]
fn the_network_is_shared_and_everything_else_is_not() {
    let a = args_of(&command());
    assert!(a.iter().any(|x| x == "--unshare-all"));
    assert!(a.iter().any(|x| x == "--share-net"));
    let unshare = a.iter().position(|x| x == "--unshare-all").unwrap();
    let share = a.iter().position(|x| x == "--share-net").unwrap();
    assert!(
        unshare < share,
        "--share-net must come after --unshare-all to re-share the network"
    );
    assert!(!a.iter().any(|x| x == "--unshare-net"));
    for flag in ["--die-with-parent", "--new-session", "--clearenv"] {
        assert!(a.iter().any(|x| x == flag), "{flag}");
    }
    assert!(has_sequence(&a, &["--cap-drop", "ALL"]));
}

#[test]
fn only_the_system_curl_the_ca_bundle_and_scratch_are_mounted() {
    let a = args_of(&command());
    assert!(has_sequence(&a, &["--ro-bind", "/usr", "/usr"]));
    assert!(has_sequence(
        &a,
        &["--ro-bind", "/usr/bin/curl", "/usr/bin/curl"]
    ));
    assert!(has_sequence(
        &a,
        &[
            "--ro-bind",
            "/etc/ca-certificates/extracted/tls-ca-bundle.pem",
            VECTOR_CA_PATH
        ]
    ));
    assert!(has_sequence(&a, &["--tmpfs", "/tmp"]));
    // Nothing writable is bound from the host, and nothing but those two read-only binds besides /usr.
    assert!(
        !a.iter()
            .any(|x| x == "--bind" || x == "--bind-try" || x == "--dev-bind")
    );
    let binds: Vec<_> = a
        .windows(3)
        .filter(|w| w.first().is_some_and(|f| f == "--ro-bind"))
        .map(|w| w.get(1).cloned().unwrap_or_default())
        .collect();
    assert_eq!(
        binds,
        vec![
            "/usr".to_owned(),
            "/usr/bin/curl".to_owned(),
            "/etc/ca-certificates/extracted/tls-ca-bundle.pem".to_owned()
        ]
    );
}

#[test]
fn the_tmpfs_is_set_up_before_a_curl_that_lives_under_tmp_is_bound() {
    let c = bwrap_command(
        OsStr::new("bwrap"),
        Path::new("/tmp/standin/curl"),
        Path::new("/etc/ssl/certs/ca-certificates.crt"),
        &[],
    );
    let a = args_of(&c);
    let tmpfs = a.iter().position(|x| x == "--tmpfs").unwrap();
    let bind = a.iter().position(|x| x == "/tmp/standin/curl").unwrap();
    assert!(tmpfs < bind);
}

#[test]
fn the_environment_is_cleared_and_curl_runs_last_with_its_arguments() {
    let c = command();
    let envs: Vec<_> = c
        .get_envs()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.map(|v| v.to_string_lossy().into_owned()),
            )
        })
        .collect();
    assert_eq!(
        envs,
        vec![("PATH".to_owned(), Some("/usr/bin:/bin".to_owned()))]
    );
    let a = args_of(&c);
    assert!(has_sequence(
        &a,
        &["--clearenv", "--setenv", "PATH", "/usr/bin"]
    ));
    let tail: Vec<_> = a.iter().rev().take(5).rev().cloned().collect();
    assert_eq!(
        tail,
        vec!["--", "/usr/bin/curl", "-q", "--url", "https://x.example/"]
    );
    assert_eq!(c.get_program(), "bwrap");
}

#[test]
fn no_repository_key_or_data_directory_is_nameable_in_the_vector() {
    let joined = args_of(&command()).join(" ");
    for word in ["repo", "key", "seed", ".prikk", "data", "home", "HOME"] {
        assert!(!joined.contains(word), "{word} in {joined}");
    }
}

#[test]
fn the_unconfined_command_is_curl_itself_with_a_cleared_environment() {
    let c = unconfined_command(Path::new("/usr/bin/curl"), &["-q".to_owned()]);
    assert_eq!(c.get_program(), "/usr/bin/curl");
    assert_eq!(args_of(&c), vec!["-q"]);
    assert_eq!(c.get_envs().count(), 1);
}
