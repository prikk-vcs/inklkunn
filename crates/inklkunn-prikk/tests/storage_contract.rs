//! `Storage::all_views` is a **required** method (handoff 005 K23). Its old default answered with the open views, so an
//! implementation that forgot it let a closed view's name be taken again without anyone noticing (prikk keeps a closed
//! branch's name). Every implementation, doubles included, now says what it has. A default is a body, and this test
//! keeps one out of the trait's declaration, so that adding it is a decision somebody has to make visibly.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[test]
fn all_views_is_declared_without_a_default_body() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/storage.rs"),
    )
    .expect("the trait's source");
    let trait_start = source
        .find("pub trait Storage: Send + Sync {")
        .expect("the trait");
    let declaration = source
        .get(trait_start..)
        .and_then(|rest| {
            rest.lines()
                .find(|l| l.trim_start().starts_with("fn all_views("))
        })
        .expect("all_views is declared in the trait");
    assert!(
        declaration.trim_end().ends_with(';'),
        "all_views has a default body: {declaration:?}"
    );
}
