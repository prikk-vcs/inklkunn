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
