#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn paths_use_the_shard_and_the_id_never_a_name() {
    let id = WikiId::parse("019a0000-0000-7000-8000-000000000001").unwrap_or_else(|| panic!("id"));
    let l = RootLayout::new("/srv/inklkunn");
    assert_eq!(
        l.repo_dir(&id),
        PathBuf::from("/srv/inklkunn/01/019a0000-0000-7000-8000-000000000001")
    );
    assert_eq!(
        l.scratch_dir(&id),
        PathBuf::from("/srv/inklkunn/01/019a0000-0000-7000-8000-000000000001.scratch")
    );
    assert!(!l.scratch_dir(&id).starts_with(l.repo_dir(&id)));
}

#[test]
fn a_creation_mark_is_a_sibling_of_the_repository_and_is_found_only_in_its_own_shard() {
    let tmp = std::env::temp_dir().join(format!("inklkunn-root-marks-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let l = RootLayout::new(&tmp);
    // No root yet: no marks, and no error.
    assert_eq!(l.creation_marks().unwrap(), Vec::new());
    let id = WikiId::parse("019a0000-0000-7000-8000-000000000001").unwrap();
    let mark = l.creation_mark(&id);
    assert_eq!(
        mark,
        tmp.join("01/019a0000-0000-7000-8000-000000000001.creating")
    );
    assert!(!mark.starts_with(l.repo_dir(&id)));
    std::fs::create_dir_all(mark.parent().unwrap()).unwrap();
    std::fs::write(&mark, "").unwrap();
    // A look-alike in the wrong shard, a directory with the suffix and a stray name are not marks.
    std::fs::create_dir_all(tmp.join("02")).unwrap();
    std::fs::write(
        tmp.join("02/019a0000-0000-7000-8000-000000000001.creating"),
        "",
    )
    .unwrap();
    std::fs::create_dir_all(tmp.join("01/019a0000-0000-7000-8000-000000000002.creating")).unwrap();
    std::fs::write(tmp.join("01/notes.creating"), "").unwrap();
    assert_eq!(l.creation_marks().unwrap(), vec![id]);
    let _ = std::fs::remove_dir_all(&tmp);
}
