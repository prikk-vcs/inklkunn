#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

fn file(path: &str, content: Option<&str>) -> PreparedFile {
    PreparedFile {
        path: path.into(),
        content: content.map(|c| c.as_bytes().to_vec()),
    }
}

fn change(files: Vec<PreparedFile>, renames: Vec<(&str, &str)>) -> PreparedChange {
    PreparedChange {
        files,
        renames: renames
            .into_iter()
            .map(|(a, b)| (a.into(), b.into()))
            .collect(),
        message: "unused".to_owned(),
    }
}

#[test]
fn the_digest_format_is_pinned() {
    // A record written today must still match tomorrow: changing the encoding changes this value on purpose.
    // (The value was also computed by an independent implementation of the encoding in Python.)
    let c = change(
        vec![file("pages/A.md", Some("a\n")), file("pages/B.md", None)],
        vec![("pages/C.md", "pages/D.md")],
    );
    assert_eq!(
        c.digest().to_hex(),
        "9e7a1b81cda855c7a7c15e6905af71d3e50213f5590ffba4f75b79d27256d5f8",
        "the canonical encoding of a prepared change changed"
    );
}

#[test]
fn the_digest_follows_the_files_the_renames_and_their_order_and_nothing_else() {
    let base = change(
        vec![
            file("pages/A.md", Some("a\n")),
            file("pages/B.md", Some("b\n")),
        ],
        vec![("pages/X.md", "pages/Y.md"), ("pages/Y.md", "pages/Z.md")],
    );
    let d = base.digest();
    // The order the files are listed in is not meaning; the message is not part of it.
    let mut shuffled = base.clone();
    shuffled.files.reverse();
    shuffled.message = "another message".to_owned();
    assert_eq!(shuffled.digest(), d);
    // Everything else is.
    let differs: Vec<(&str, PreparedChange)> = vec![
        (
            "a changed byte",
            change(
                vec![
                    file("pages/A.md", Some("a!\n")),
                    file("pages/B.md", Some("b\n")),
                ],
                vec![("pages/X.md", "pages/Y.md"), ("pages/Y.md", "pages/Z.md")],
            ),
        ),
        (
            "a changed path",
            change(
                vec![
                    file("pages/A2.md", Some("a\n")),
                    file("pages/B.md", Some("b\n")),
                ],
                vec![("pages/X.md", "pages/Y.md"), ("pages/Y.md", "pages/Z.md")],
            ),
        ),
        (
            "a removal instead of an empty file",
            change(
                vec![file("pages/A.md", None), file("pages/B.md", Some("b\n"))],
                vec![("pages/X.md", "pages/Y.md"), ("pages/Y.md", "pages/Z.md")],
            ),
        ),
        (
            "an extra file",
            change(
                vec![
                    file("pages/A.md", Some("a\n")),
                    file("pages/B.md", Some("b\n")),
                    file("pages/C.md", Some("")),
                ],
                vec![("pages/X.md", "pages/Y.md"), ("pages/Y.md", "pages/Z.md")],
            ),
        ),
        (
            "renames in the other order (a chain is not its reverse)",
            change(
                vec![
                    file("pages/A.md", Some("a\n")),
                    file("pages/B.md", Some("b\n")),
                ],
                vec![("pages/Y.md", "pages/Z.md"), ("pages/X.md", "pages/Y.md")],
            ),
        ),
        (
            "a rename dropped",
            change(
                vec![
                    file("pages/A.md", Some("a\n")),
                    file("pages/B.md", Some("b\n")),
                ],
                vec![("pages/X.md", "pages/Y.md")],
            ),
        ),
        (
            "the same bytes split another way between path and content",
            change(
                vec![
                    file("pages/A.mda\n", Some("")),
                    file("pages/B.md", Some("b\n")),
                ],
                vec![("pages/X.md", "pages/Y.md"), ("pages/Y.md", "pages/Z.md")],
            ),
        ),
    ];
    for (what, c) in differs {
        assert_ne!(c.digest(), d, "{what}");
    }
}

#[test]
fn a_digest_prints_as_64_hex_characters_and_keeps_its_bytes() {
    let d = change(vec![file("pages/A.md", Some(""))], vec![]).digest();
    assert_eq!(d.to_hex().len(), 64);
    assert!(d.to_hex().bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(ChangeDigest::from_bytes(*d.as_bytes()), d);
    assert_eq!(format!("{d}"), d.to_hex());
}
