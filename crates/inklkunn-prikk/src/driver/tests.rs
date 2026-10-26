use super::*;

#[test]
fn versions_parse_and_compare_to_the_floor() {
    assert_eq!(parse_version("prikk 0.54.1"), Some((0, 54, 1)));
    assert_eq!(parse_version("prikk 1.2.3-rc1\n"), Some((1, 2, 3)));
    assert_eq!(parse_version("something else"), None);
    assert!(parse_version("prikk 0.53.9").is_some_and(|v| v < floor()));
    assert!(parse_version("prikk 0.54.0").is_some_and(|v| v >= floor()));
}

#[test]
fn a_report_with_another_schema_is_refused_before_its_shape_is_read() {
    let doctored = br#"{"schema_version": "branch-list-v9", "branches": []}"#;
    let r: Result<Views> = parse_report("branch", doctored, schema::BRANCH_LIST);
    assert!(
        matches!(r, Err(StorageError::UnsupportedSchema { found, pinned, .. }) if found == "branch-list-v9" && pinned == "branch-list-v1")
    );
    let not_json: Result<Views> = parse_report("branch", b"error", schema::BRANCH_LIST);
    assert!(matches!(not_json, Err(StorageError::Damaged { .. })));
}
