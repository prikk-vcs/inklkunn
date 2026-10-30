#![allow(clippy::expect_used, clippy::unwrap_used)]
use std::cell::Cell;

use super::*;

const STATE: &str = "0000000000000000000000000000000000000000000000000000000000000001";
const BLOCK: &str = "00000000000000000000000000000000000000000000000000000000b10c0001";

fn fields(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn form_with(text: &str) -> HashMap<String, String> {
    fields(&[
        ("op", "submit"),
        ("pages", "1"),
        ("p0_path", "pages/Home.md"),
        ("p0_title", "Home"),
        ("p0_text", text),
        ("summary", "  Fix it  "),
        ("sources", "https://example.org/a\r\n\r\n  b  \r\n"),
        ("base_state", STATE),
        ("base_block", BLOCK),
    ])
}

/// A parser that counts how often it is asked.
struct Counting(Cell<usize>);

impl Frontmatter for Counting {
    fn keys(&self, _: &str) -> Option<BTreeMap<String, String>> {
        self.0.set(self.0.get() + 1);
        Some(BTreeMap::new())
    }
}

#[test]
fn a_form_is_read_into_its_pages_summary_sources_and_base() {
    let f = read(&form_with("text")).unwrap();
    assert_eq!(f.op, Op::Submit);
    assert_eq!(f.summary, "Fix it");
    assert_eq!(f.sources, ["https://example.org/a", "b"]);
    assert_eq!(f.pages.len(), 1);
    assert_eq!(
        f.pages.first().unwrap().path.as_deref(),
        Some("pages/Home.md")
    );
    assert_eq!(f.base.unwrap().block_id, BLOCK);
    assert_eq!(f.selection, Selection::Whole);
}

#[test]
fn a_form_that_is_not_one_a_page_offers_is_incomplete() {
    let mut f = form_with("x");
    f.remove("op");
    assert_eq!(read(&f), Err(FormError::Incomplete));
    let mut f = form_with("x");
    f.insert("pages".into(), "0".into());
    assert_eq!(read(&f), Err(FormError::Incomplete));
    let mut f = form_with("x");
    f.remove("p0_path");
    assert_eq!(read(&f), Err(FormError::Incomplete));
    let mut f = form_with("x");
    f.insert("base_state".into(), "nope".into());
    assert_eq!(read(&f), Err(FormError::Incomplete));
    let mut f = form_with("x");
    f.insert("part".into(), "section".into());
    assert_eq!(read(&f), Err(FormError::Incomplete));
}

#[test]
fn a_new_page_has_no_path_and_a_part_is_named_by_its_kind_and_position() {
    let mut f = form_with("x");
    f.remove("p0_path");
    f.insert("p0_new".into(), "1".into());
    f.insert("p0_title".into(), "Guides/Setup".into());
    f.insert("part".into(), "block".into());
    f.insert("ordinal".into(), "3".into());
    let f = read(&f).unwrap();
    assert!(f.pages.first().unwrap().path.is_none());
    assert_eq!(f.selection, Selection::Block(3));
}

#[test]
fn a_page_over_one_mebibyte_and_a_proposal_over_two_are_refused_before_any_parsing() {
    let counter = Counting(Cell::new(0));
    let over_page = read(&form_with(&"x".repeat(MAX_PAGE_BYTES + 1))).unwrap();
    assert_eq!(check_bounds(&over_page), Err(FormError::PageTooLarge));
    // Two pages, each within its bound, together over the proposal's.
    let mut f = form_with(&"x".repeat(MAX_PAGE_BYTES));
    f.insert("pages".into(), "3".into());
    for i in 1..3 {
        f.insert(format!("p{i}_path"), format!("pages/P{i}.md"));
        f.insert(format!("p{i}_text"), "y".repeat(MAX_PAGE_BYTES));
    }
    let over_proposal = read(&f).unwrap();
    assert_eq!(check_bounds(&over_proposal), Err(FormError::TooLarge));
    // The bound is judged on what was read; the parser is a separate step, run only after it passes, and the
    // sequence the handler follows leaves the parser unasked for a text that is too large.
    let run = |form: &Submitted| -> Result<(), FormError> {
        check_bounds(form)?;
        for p in &form.pages {
            let _ = check_owned_keys(&counter, None, &p.text, || false);
        }
        Ok(())
    };
    assert!(run(&over_page).is_err() && run(&over_proposal).is_err());
    assert_eq!(
        counter.0.get(),
        0,
        "the parser was asked for a text over the bound"
    );
    // A text within the bound does reach it, so the count above means something.
    assert!(run(&read(&form_with("small")).unwrap()).is_ok());
    assert_eq!(counter.0.get(), 1);
}

#[test]
fn a_page_at_exactly_the_bound_is_taken() {
    let f = read(&form_with(&"x".repeat(MAX_PAGE_BYTES))).unwrap();
    assert_eq!(check_bounds(&f), Ok(()));
}

#[test]
fn a_page_keeps_its_own_line_endings_and_a_browsers_are_not_a_change() {
    assert_eq!(Eol::of("a\r\nb\r\n"), Eol::CrLf);
    assert_eq!(Eol::of("a\nb\r\n"), Eol::Lf);
    assert_eq!(Eol::of("no breaks"), Eol::Lf);
    // A browser sends CR LF whatever the page had.
    assert_eq!(Eol::Lf.apply("a\r\nb\r\nc"), "a\nb\nc");
    assert_eq!(Eol::CrLf.apply("a\r\nb\nc"), "a\r\nb\r\nc");
    assert_eq!(Eol::Lf.apply("lone\rcarriage"), "lone\ncarriage");
    assert_eq!(for_display("a\r\nb\r"), "a\nb\n");
    // Text that is not ASCII goes through untouched.
    assert_eq!(Eol::CrLf.apply("日本語\n🙂"), "日本語\r\n🙂");
}

#[test]
fn the_keys_the_service_owns_may_not_change_unless_the_role_allows() {
    let old = "---\ntitle: Home\n---\n\nBody\n";
    let with_kind = "---\ntitle: Home\nkind: system\n---\n\nBody\n";
    assert_eq!(check_owned_keys(&Markup, Some(old), old, || false), Ok(()));
    assert_eq!(
        check_owned_keys(&Markup, Some(old), with_kind, || false),
        Err(Front::Owned(vec!["kind"]))
    );
    // Left as it was is not a change, even for a key the service owns.
    assert_eq!(
        check_owned_keys(&Markup, Some(with_kind), with_kind, || false),
        Ok(())
    );
    // Removing it is a change.
    assert_eq!(
        check_owned_keys(&Markup, Some(with_kind), old, || false),
        Err(Front::Owned(vec!["kind"]))
    );
    // Allowed by role: asked only when a key changed.
    let asked = Cell::new(0);
    assert_eq!(
        check_owned_keys(&Markup, Some(old), with_kind, || {
            asked.set(asked.get() + 1);
            true
        }),
        Ok(())
    );
    assert_eq!(
        check_owned_keys(&Markup, Some(old), old, || {
            asked.set(9);
            true
        }),
        Ok(())
    );
    assert_eq!(asked.get(), 1);
    let all = "---\nkind: a\nvisibility: public\nsystem: true\n---\n";
    assert_eq!(
        check_owned_keys(&Markup, None, all, || false),
        Err(Front::Owned(vec!["kind", "visibility", "system"]))
    );
}

#[test]
fn information_at_the_top_that_cannot_be_read_is_refused() {
    let bad = "---\ntitle: &anchor Home\n---\n";
    assert_eq!(
        check_owned_keys(&Markup, None, bad, || true),
        Err(Front::Unreadable)
    );
    // A page that is unreadable already is not made worse by a good text.
    assert_eq!(
        check_owned_keys(&Markup, Some(bad), "---\ntitle: Home\n---\n", || false),
        Ok(())
    );
}
