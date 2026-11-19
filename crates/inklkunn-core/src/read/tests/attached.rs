//! The sources a change attached (RFC 005 part 2 D-8; maintenance M56): the source files a change *created* under
//! `knowledge/sources/`, read at that very block, bounded, with a file that cannot be read as a source counted and
//! never listed. Over the in-memory repository, so that both test runs run all of it.

use inklkunn_prikk::StorageError;

use super::super::testing::{World, block_id};
use super::history::{change, op};
use super::*;
use crate::knowledge::{Reference, ReferenceForm, Source, SourceId, SourceKind, source_path};

fn id(n: u8) -> SourceId {
    SourceId::parse(&format!("01931111-0000-7000-8000-0000000000{n:02x}")).expect("a source id")
}

fn cited(n: u8, value: &str) -> Source {
    let mut s = Source::new(id(n));
    s.kind = SourceKind::Primary;
    s.reference = Some(Reference {
        form: ReferenceForm::Citation,
        value: value.to_owned(),
    });
    s
}

fn files(sources: &[&Source]) -> Vec<(String, String)> {
    sources
        .iter()
        .map(|s| (source_path(&s.id), s.to_yaml()))
        .collect()
}

/// A wiki whose version 1 is `Home`, and whose version 2 creates the files given (as the operations say).
fn attaching(write: &[(String, String)], ops: Vec<inklkunn_prikk::Operation>) -> World {
    let w = World::pages_with(
        &[("Home", &[], "Words.\n")],
        ReadConfig {
            tip_ttl: std::time::Duration::ZERO,
            ..ReadConfig::default()
        },
    );
    w.fake().say(1, "Create the wiki");
    w.fake()
        .show(1, change(vec![op("create-file", &["pages/Home.md"])]));
    w.fake().set_files(write);
    w.fake().say(2, "Attach");
    w.fake().show(2, change(ops));
    w
}

fn attached(w: &World, version: u64, limit: usize) -> Attached {
    w.service
        .attached_sources(&w.wiki, &block_id(version), limit)
        .expect("what the change attached")
}

fn nth(written: &[(String, String)], n: usize) -> String {
    written
        .get(n)
        .map(|(path, _)| path.clone())
        .expect("a written file")
}

fn create(path: &str) -> inklkunn_prikk::Operation {
    op("create-file", &[path])
}

#[test]
fn the_source_files_a_change_created_are_listed_with_their_references_in_the_order_the_change_says()
{
    let (a, b) = (
        cited(1, "The Harbour Survey, page 12"),
        cited(2, "A letter"),
    );
    let written = files(&[&a, &b]);
    let w = attaching(
        &written,
        vec![create(&nth(&written, 0)), create(&nth(&written, 1))],
    );
    let got = attached(&w, 2, 10);
    assert_eq!(
        got.sources,
        [
            AttachedSource {
                id: a.id.to_string(),
                reference: Some("The Harbour Survey, page 12".to_owned())
            },
            AttachedSource {
                id: b.id.to_string(),
                reference: Some("A letter".to_owned())
            },
        ]
    );
    assert_eq!((got.unreadable, got.omitted, got.too_large), (0, 0, false));
    // The change that created the page attached none.
    assert_eq!(attached(&w, 1, 10), Attached::default());
}

#[test]
fn a_source_is_read_as_the_change_wrote_it_and_not_as_it_is_now() {
    let first = cited(1, "The first wording of the reference");
    let written = files(&[&first]);
    let w = attaching(&written, vec![create(&nth(&written, 0))]);
    // A later change rewrites the file; the first change still attached what it wrote.
    let later = cited(1, "A later wording of the reference");
    w.fake().set_files(&files(&[&later]));
    w.fake().say(3, "Edit the source");
    w.fake()
        .show(3, change(vec![op("edit-text", &[&nth(&written, 0)])]));
    assert_eq!(
        attached(&w, 2, 10)
            .sources
            .first()
            .and_then(|s| s.reference.as_deref()),
        Some("The first wording of the reference")
    );
    // An edit creates nothing: the later change attached none.
    assert_eq!(attached(&w, 3, 10), Attached::default());
}

#[test]
fn only_a_created_source_file_is_an_attached_source() {
    let a = cited(1, "A letter");
    let written = files(&[&a]);
    let mut all = written.clone();
    all.push((
        "pages/Other.md".to_owned(),
        "---\ntitle: Other\n---\n\nx\n".to_owned(),
    ));
    all.push((
        "knowledge/claims/01930000-0000-7000-8000-000000000001.yaml".to_owned(),
        "kind: observation\n".to_owned(),
    ));
    let w = attaching(
        &all,
        vec![
            create("pages/Other.md"),
            create("knowledge/claims/01930000-0000-7000-8000-000000000001.yaml"),
            op("edit-text", &[&nth(&written, 0)]),
            op("delete-node", &[&nth(&written, 0)]),
        ],
    );
    assert_eq!(attached(&w, 2, 10), Attached::default());
}

#[test]
fn a_file_that_cannot_be_read_as_a_source_is_counted_and_never_listed() {
    let fine = cited(1, "A letter");
    let controlled = cited(2, "fine\u{202e}gnp.exe");
    let mut written = files(&[&fine, &controlled]);
    // Not YAML of the format, and not text.
    let (junk, bytes) = (source_path(&id(3)), source_path(&id(4)));
    written.push((junk.clone(), "this: is: not: a source\n".to_owned()));
    written.push((bytes.clone(), String::new()));
    let w = attaching(
        &written,
        written.iter().map(|(p, _)| create(p)).collect::<Vec<_>>(),
    );
    w.fake().set_bytes(&bytes, &[0xff, 0xfe, 0xfd]);
    // The version the change made is version 2; rewriting a file moved the tip, so the change under test is the
    // snapshot of version 2 and still holds what it was.
    let got = attached(&w, 2, 10);
    assert_eq!(
        got.sources,
        [AttachedSource {
            id: fine.id.to_string(),
            reference: Some("A letter".to_owned())
        }]
    );
    // The control, the junk and the empty file; the bytes written later are not in version 2.
    assert_eq!(got.unreadable, 3, "{got:?}");
}

#[test]
fn no_more_files_are_read_than_the_limit_and_the_rest_are_counted() {
    let many: Vec<Source> = (1..=5)
        .map(|n| cited(n, &format!("Reference {n}")))
        .collect();
    let written = files(&many.iter().collect::<Vec<_>>());
    let w = attaching(
        &written,
        written.iter().map(|(p, _)| create(p)).collect::<Vec<_>>(),
    );
    let got = attached(&w, 2, 3);
    assert_eq!(got.sources.len(), 3);
    assert_eq!(got.omitted, 2);
    assert_eq!(
        got.sources
            .iter()
            .filter_map(|s| s.reference.as_deref())
            .collect::<Vec<_>>(),
        ["Reference 1", "Reference 2", "Reference 3"]
    );
    // A limit of nothing reads nothing, and counts all.
    let none = attached(&w, 2, 0);
    assert!(none.sources.is_empty());
    assert_eq!(none.omitted, 5);
}

#[test]
fn a_change_too_large_to_describe_is_said_to_be_and_never_read_as_having_attached_nothing() {
    let a = cited(1, "A letter");
    let written = files(&[&a]);
    let w = attaching(&written, vec![create(&nth(&written, 0))]);
    w.fake().show(
        2,
        Err(StorageError::Unavailable {
            verb: "show",
            reason: inklkunn_prikk::Unavailable::OutputExceeded,
        }),
    );
    let got = attached(&w, 2, 10);
    assert!(got.too_large);
    assert!(got.sources.is_empty());
}

#[test]
fn a_read_that_fails_fails_the_answer_and_never_a_partial_list() {
    let a = cited(1, "A letter");
    let written = files(&[&a]);
    let w = attaching(&written, vec![create(&nth(&written, 0))]);
    w.fake().fail_read(Some(StorageError::Driver {
        verb: "cat",
        detail: "gone".to_owned(),
    }));
    assert!(
        w.service
            .attached_sources(&w.wiki, &block_id(2), 10)
            .is_err()
    );
}

#[test]
fn something_that_is_not_a_block_id_attached_nothing() {
    let w = attaching(&[], vec![]);
    assert_eq!(
        w.service
            .attached_sources(&w.wiki, "not a block", 10)
            .expect("an answer"),
        Attached::default()
    );
}
