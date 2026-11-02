#![allow(clippy::expect_used, clippy::unwrap_used)]
//! The warning about the secret file's mode and owner (corrections A2): no prikk needed.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use super::*;

fn path() -> PathBuf {
    PathBuf::from("/run/secrets/inklkunn")
}

#[test]
fn a_file_only_its_owner_can_use_is_not_warned_about() {
    for mode in [0o400, 0o600, 0o100_600, 0o100_400] {
        assert!(
            secret_file_warnings(&path(), mode, 1000, 1000).is_empty(),
            "{mode:o}"
        );
    }
    // Owned by root is fine too (a secret mount).
    assert!(secret_file_warnings(&path(), 0o400, 0, 1000).is_empty());
}

#[test]
fn a_file_group_or_others_can_read_or_change_is_warned_about_with_its_mode_and_nothing_secret() {
    for mode in [0o644, 0o640, 0o604, 0o660, 0o602, 0o100_644, 0o777] {
        let warnings = secret_file_warnings(&path(), mode, 1000, 1000);
        assert_eq!(warnings.len(), 1, "{mode:o}: {warnings:?}");
        let line = warnings.first().unwrap();
        assert!(
            line.starts_with("inklkunn: warning: ")
                && !line.contains('\n')
                && line.contains("/run/secrets/inklkunn")
                && line.contains("INKLKUNN_SECRET_FILE")
                && line.contains(&format!("mode {:04o}", mode & 0o777)),
            "{mode:o}: {line}"
        );
    }
}

#[test]
fn a_file_owned_by_another_user_than_the_servers_or_roots_is_warned_about() {
    let warnings = secret_file_warnings(&path(), 0o400, 1234, 1000);
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let line = warnings.first().unwrap();
    assert!(
        line.contains("owned by user 1234") && line.contains("(1000)"),
        "{line}"
    );
    // Both at once are two lines.
    assert_eq!(secret_file_warnings(&path(), 0o644, 1234, 1000).len(), 2);
}

#[test]
fn the_file_on_disk_is_looked_at_through_a_link_and_a_missing_one_is_not_warned_about_here() {
    let dir = std::env::temp_dir().join(format!("inklkunn-a2-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("secret");
    std::fs::write(&file, "x").unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let link = dir.join("link");
    std::os::unix::fs::symlink(&file, &link).unwrap();
    for p in [&file, &link] {
        let warnings = secret_file_check(p);
        assert_eq!(warnings.len(), 1, "{}: {warnings:?}", p.display());
    }
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(secret_file_check(&link).is_empty());
    assert!(secret_file_check(&dir.join("absent")).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
