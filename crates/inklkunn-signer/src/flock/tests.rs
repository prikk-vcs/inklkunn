#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::process::{Command, Stdio};

use super::*;

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("inklkunn-flock-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_second_holder_is_refused_and_told_who_holds_the_lock() {
    let d = dir("busy");
    let path = d.join("w.lock");
    let first = match try_exclusive(&path).unwrap() {
        Acquired::Held(l) => l,
        Acquired::Busy { .. } => panic!("a free lock was busy"),
    };
    match try_exclusive(&path).unwrap() {
        Acquired::Busy { holder } => assert_eq!(holder, Some(std::process::id())),
        Acquired::Held(_) => panic!("two holders at once"),
    }
    drop(first);
    // Released when the last descriptor closes. A thread of this very process may be forking right now (other
    // tests spawn children), and a child holds a copy of the descriptor until it execs: so "free" is a few
    // milliseconds away at most, never a state that lasts.
    assert!(
        matches!(
            exclusive_within(&path, Duration::from_secs(2)).unwrap(),
            Acquired::Held(_)
        ),
        "the lock was not released"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn waiting_is_bounded() {
    let d = dir("bounded");
    let path = d.join("w.lock");
    let _held = try_exclusive(&path).unwrap();
    let started = Instant::now();
    assert!(matches!(
        exclusive_within(&path, Duration::from_millis(60)).unwrap(),
        Acquired::Busy { .. }
    ));
    assert!(started.elapsed() < Duration::from_secs(5));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn the_lock_file_is_private_and_a_link_is_not_followed() {
    use std::os::unix::fs::{MetadataExt, symlink};
    let d = dir("mode");
    let path = d.join("w.lock");
    drop(try_exclusive(&path).unwrap());
    assert_eq!(std::fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    let target = d.join("elsewhere");
    std::fs::write(&target, "x").unwrap();
    let link = d.join("l.lock");
    symlink(&target, &link).unwrap();
    assert!(try_exclusive(&link).is_err(), "a link was followed");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "x");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_lock_held_by_another_process_refuses_us_and_dies_with_its_holder() {
    if Command::new("flock").arg("--version").output().is_err() {
        eprintln!("skipping: flock(1) not available");
        return;
    }
    let d = dir("process");
    let path = d.join("w.lock");
    // The shell opens the file on descriptor 9, locks it with util-linux `flock 9`, then *becomes* `sleep`
    // (`exec`), so the process that holds the lock is the one we kill (a `flock FILE CMD` would leave `CMD`
    // as a child that inherits the descriptor and keeps the lock alive).
    let mut holder = Command::new("sh")
        .args(["-c", "exec 9<>\"$0\" && flock 9 && exec sleep 30"])
        .arg(&path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // Wait until the child has the lock (our own attempt is the probe).
    let started = Instant::now();
    loop {
        // Busy twice, apart: a sibling test forking for a moment holds the descriptor too.
        let busy = |p: &std::path::Path| matches!(try_exclusive(p).unwrap(), Acquired::Busy { .. });
        if busy(&path) && {
            std::thread::sleep(Duration::from_millis(100));
            busy(&path)
        } {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the child never took the lock"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    holder.kill().unwrap();
    holder.wait().unwrap();
    // Killed: the kernel released it; nothing stale remains.
    assert!(
        matches!(
            exclusive_within(&path, Duration::from_secs(5)).unwrap(),
            Acquired::Held(_)
        ),
        "the lock outlived its holder"
    );
    let _ = std::fs::remove_dir_all(&d);
}
