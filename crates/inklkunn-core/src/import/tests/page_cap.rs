//! The import is judged under the operator's page cap throughout (handoff 005 corrections, sixth round, K57; the review's
//! X-d): the plan's checks, the read of the files again at the run, and the page ids `wiki_state` reads out of the wiki's
//! own pages. A page the plan accepts is a page the server can read, and one it refuses is one the server would refuse.
//! No test needs prikk.

use super::*;
use crate::read::testing::{World, page};

fn config(page_max_bytes: u64) -> ReadConfig {
    ReadConfig {
        page_max_bytes,
        ..ReadConfig::default()
    }
}

/// A page file of exactly `len` bytes.
fn page_of(len: usize) -> Vec<u8> {
    let head = "---\ntitle: P\n---\n\n";
    let mut text = head.to_owned();
    text.push_str(&"a".repeat(len - head.len()));
    assert_eq!(text.len(), len);
    text.into_bytes()
}

/// The plan of a folder of one page of `len` bytes under a cap of `cap`.
fn plan_of(tag: &str, len: usize, cap: u64) -> (Dir, ImportPlan) {
    let d = dir(tag);
    put(&d.0, "pages/P.md", &page_of(len));
    let plan = plan_folder(&d.0, &nothing(), false, &config(cap)).unwrap();
    (d, plan)
}

fn refused_as_too_large(plan: &ImportPlan) -> bool {
    plan.refused
        .iter()
        .any(|r| r.name == "pages/P.md" && r.reason.contains("is larger than the"))
}

#[test]
fn a_lowered_cap_refuses_a_page_one_byte_over_it_and_takes_one_exactly_at_it() {
    let (_d, at) = plan_of("cap-low-at", 2048, 2048);
    assert!(at.is_importable(), "{}", at.report());
    let (_d, over) = plan_of("cap-low-over", 2049, 2048);
    assert!(!over.is_importable() && refused_as_too_large(&over));
    assert!(
        over.report().contains("2.0 KiB"),
        "the cap the plan used is the one it names: {}",
        over.report()
    );
    // The default cap would have taken both: the lowered one is what judged them.
    let (_d, default) = plan_of(
        "cap-low-default",
        2049,
        ReadConfig::default().page_max_bytes,
    );
    assert!(default.is_importable());
}

#[test]
fn a_raised_cap_takes_a_page_over_the_default_and_refuses_one_byte_over_itself() {
    let default = ReadConfig::default().page_max_bytes;
    let raised = default * 2;
    let over_default = usize::try_from(default).unwrap() + 1;
    let (_d, refused) = plan_of("cap-raised-default", over_default, default);
    assert!(!refused.is_importable() && refused_as_too_large(&refused));
    let (_d, taken) = plan_of("cap-raised-over-default", over_default, raised);
    assert!(taken.is_importable(), "{}", taken.report());
    let (_d, at) = plan_of("cap-raised-at", usize::try_from(raised).unwrap(), raised);
    assert!(at.is_importable(), "{}", at.report());
    let (_d, over) = plan_of(
        "cap-raised-over",
        usize::try_from(raised).unwrap() + 1,
        raised,
    );
    assert!(!over.is_importable() && refused_as_too_large(&over));
}

#[test]
fn the_run_reads_a_page_again_under_the_cap_its_plan_was_made_under() {
    let default = ReadConfig::default().page_max_bytes;
    // A page over the default cap, planned under a raised one: read again at the run it is still a page.
    let (_d, raised) = plan_of(
        "reread-raised",
        usize::try_from(default).unwrap() + 1,
        default * 2,
    );
    assert!(raised.is_importable());
    let files = read_files(&raised).unwrap();
    assert_eq!(files.len(), 1);
    // And under a lowered one, the page at its cap is read whole, not cut at the default's.
    let (_d, lowered) = plan_of("reread-lowered", 2048, 2048);
    assert_eq!(read_files(&lowered).unwrap().len(), 1);
    // A page that grew past the plan's cap since it was planned is refused as changed, whatever the default is.
    let (d, grown) = plan_of("reread-grown", 2048, 2048);
    put(&d.0, "pages/P.md", &page_of(2049));
    assert!(matches!(read_files(&grown), Err(ImportError::Changed(_))));
}

/// The ids `wiki_state` read out of the wiki's pages, under `cap`.
fn ids_under(w: &World, cap: u64) -> Existing {
    wiki_state(&*w.counting, &w.layout, &w.wiki, &config(cap)).unwrap()
}

#[test]
fn the_page_ids_of_the_wikis_own_pages_are_read_under_the_operators_cap_both_ways() {
    // A page of the wiki with an id, exactly `len` bytes.
    let front = ["title: Home", "id: home-1"];
    let len = page(&front, "").len() + 3000;
    let body = "y".repeat(3000);
    assert_eq!(page(&front, &body).len(), len);
    let w = World::pages(&[("Home", &["id: home-1"], &body)]);
    w.fake().list_views();
    let cap = len as u64;
    // At the cap the page is read and its id is taken; one byte under, it is not a page the server reads, and so its id
    // is not an id the wiki has.
    let at = ids_under(&w, cap);
    assert_eq!(
        at.page_ids.get("home-1").map(String::as_str),
        Some("pages/Home.md")
    );
    let under = ids_under(&w, cap - 1);
    assert!(!under.page_ids.contains_key("home-1"), "{under:?}");
    // The file is in the wiki's state either way (its digest is taken whole): only the id is read under the cap.
    assert_eq!(at.files.len(), under.files.len());
    // Raised, as the operator raises it for a wiki of larger pages: the id is read.
    let raised = ids_under(&w, cap * 4);
    assert!(raised.page_ids.contains_key("home-1"));
    // The default cap is 1 MiB: the page is far under it, and both agree.
    assert_eq!(ids_under(&w, ReadConfig::default().page_max_bytes), raised);
}
