#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;
use crate::api::write::body;

fn sent(files: Vec<File>, renames: Vec<Rename>) -> Sent {
    Sent {
        summary: "x".to_owned(),
        sources: Vec::new(),
        files,
        renames,
        minor: false,
        automated: false,
    }
}

fn page(path: &str, bytes: usize) -> File {
    File {
        path: Some(path.to_owned()),
        title: String::new(),
        text: Some("a".repeat(bytes)),
    }
}

#[test]
fn a_page_over_a_mebibyte_and_a_proposal_over_two_are_refused_by_size_alone() {
    let mib = 1024 * 1024;
    assert_eq!(
        check_sizes(&sent(vec![page("pages/A.md", mib)], vec![])),
        Ok(())
    );
    assert_eq!(
        check_sizes(&sent(vec![page("pages/A.md", mib + 1)], vec![])),
        Err(Invalid::PageTooLarge)
    );
    // Two full pages leave no room for their paths.
    assert_eq!(
        check_sizes(&sent(
            vec![page("pages/A.md", mib), page("pages/B.md", mib)],
            vec![]
        )),
        Err(Invalid::TooLarge)
    );
    assert_eq!(
        check_sizes(&sent(
            vec![page("pages/A.md", mib), page("pages/B.md", mib - 20)],
            vec![]
        )),
        Ok(())
    );
    // Renames count, and a removal has no text to count.
    let rename = Rename {
        from: "f".repeat(mib),
        to: "t".repeat(mib + 1),
    };
    assert_eq!(
        check_sizes(&sent(vec![], vec![rename])),
        Err(Invalid::TooLarge)
    );
    let removal = File {
        path: Some("pages/A.md".to_owned()),
        title: String::new(),
        text: None,
    };
    assert_eq!(check_sizes(&sent(vec![removal], vec![])), Ok(()));
}

#[test]
fn the_files_of_a_body_are_a_path_or_a_title_and_a_text_and_nothing_else() {
    let read_files = |json: &str| {
        let mut fields = body::parse(json.as_bytes()).unwrap();
        read(&mut fields)
    };
    let ok = |json: &str| read_files(json).unwrap();
    let bad = |json: &str| read_files(json).unwrap_err();

    let s = ok(
        r#"{"summary":"s","files":[{"path":"pages/A.md","text":"t"},{"path":"pages/B.md","text":null},{"title":"New","text":"n"}],"renames":[{"from":"a","to":"b"}],"sources":["u"],"minor":true}"#,
    );
    assert_eq!(s.files.len(), 3);
    assert!(s.files.get(1).is_some_and(|f| f.text.is_none()));
    assert_eq!(s.files.get(2).map(|f| f.path.is_none()), Some(true));
    assert_eq!(s.renames.len(), 1);
    assert!(s.minor && !s.automated);

    for json in [
        r#"{"summary":"s","files":[{"path":"pages/A.md","title":"A","text":"t"}]}"#,
        r#"{"summary":"s","files":[{"text":"t"}]}"#,
        r#"{"summary":"s","files":[{"title":"A","text":null}]}"#,
        r#"{"summary":"s","files":[{"path":"pages/A.md"}]}"#,
        r#"{"summary":"s","files":[{"path":"pages/A.md","text":"t","x":1}]}"#,
        r#"{"summary":"s","files":"pages/A.md"}"#,
        r#"{"summary":"s","renames":[{"from":"a"}]}"#,
        r#"{"summary":"s","sources":[1]}"#,
        r#"{"summary":"s","minor":"yes"}"#,
    ] {
        assert_eq!(
            bad(json).code,
            crate::api::problem::Code::InvalidBody,
            "{json}"
        );
    }
    // At most as many pages as a form carries.
    let many = format!(
        r#"{{"summary":"s","files":[{}]}}"#,
        vec![r#"{"path":"pages/A.md","text":"t"}"#; MAX_FORM_PAGES + 1].join(",")
    );
    assert_eq!(bad(&many).code, crate::api::problem::Code::InvalidBody);
}
