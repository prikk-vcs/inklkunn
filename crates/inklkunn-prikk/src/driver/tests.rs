use super::*;

#[test]
fn versions_parse_and_compare_to_the_floor() {
    assert_eq!(parse_version("prikk 0.54.1"), Some((0, 54, 1)));
    assert_eq!(parse_version("prikk 1.2.3-rc1\n"), Some((1, 2, 3)));
    assert_eq!(parse_version("something else"), None);
    assert!(parse_version("prikk 0.53.9").is_some_and(|v| v < FLOOR));
    assert!(parse_version("prikk 0.54.0").is_some_and(|v| v >= FLOOR));
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

#[test]
fn argument_shapes_are_checked() {
    assert!(is_ref_name("heads/main"));
    assert!(is_ref_name("tags/v1.0"));
    assert!(!is_ref_name("main"));
    assert!(!is_ref_name("heads/"));
    assert!(!is_ref_name("heads/../x"));
    assert!(!is_ref_name("heads/-x"));
    assert!(is_hex_id(&"a".repeat(64)));
    assert!(!is_hex_id("deadbeef"));
    assert!(!is_hex_id(&"A".repeat(64)));
    assert!(is_repo_path("pages/Home.md"));
    assert!(is_repo_path("pages/Guides/Getting Started.md"));
    for bad in [
        "",
        "/etc/passwd",
        "../x",
        "a/./b",
        "a//b",
        "-x",
        "a\nb",
        &"x".repeat(1025),
    ] {
        assert!(!is_repo_path(bad), "{bad:?}");
    }
}
