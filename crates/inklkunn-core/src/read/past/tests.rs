//! What a base that is not a tip gives besides a page's text (RFC 006 D-8; INV-13; handoff G-T4, corrections J-3, L-5, L-10): the title
//! and the knowledge indexes at a base, with what a hidden change wrote taken out. The repository is the read tests' double; the
//! moderation records are the memory store's, written directly.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::atomic::Ordering;

use super::super::testing::{block_id, state_id};
use super::super::writes::tests::{Rig, rig};
use super::*;
use crate::knowledge::{Claim, ClaimId, Expression, claim_path};

const ONE: &str = "---\ntitle: One\nid: p1\naliases: [Uno]\n---\n\nSaid. ^b1\n\nOther.\n";
const ONE_SECRET: &str =
    "---\ntitle: One\nid: p1\naliases: [Uno]\n---\n\nSaid in ZEBRA secret. ^b1\n\nOther.\n";
const TWO: &str = "---\ntitle: Two\nid: p2\n---\n\nPlain. ^b2\n";

fn claim_id(n: u8) -> ClaimId {
    ClaimId::parse(&format!("01930000-0000-7000-8000-0000000000{n:02x}")).expect("a claim id")
}

/// The path and text of a claim said at `page_id#block`.
fn claim_file(n: u8, page_id: &str, block: &str) -> (String, String) {
    let mut claim = Claim::new(claim_id(n));
    claim.expressions = vec![Expression::new(page_id, block).expect("an expression")];
    (claim_path(&claim.id), claim.to_yaml())
}

/// 1 creates `One` (the page the hidden change will write, with the id `p1`), `Two` and a claim said on each; 2 writes the secret to
/// `One`; 3 and 4 edit `Two`.
fn world() -> Rig {
    let (a_path, a_text) = claim_file(1, "p1", "b1");
    let (b_path, b_text) = claim_file(2, "p2", "b2");
    let r = rig(&[
        ("pages/One.md", ONE),
        ("pages/Two.md", TWO),
        (a_path.as_str(), a_text.as_str()),
        (b_path.as_str(), b_text.as_str()),
    ]);
    r.w.fake().set("pages/One.md", ONE_SECRET); // 2
    r.w.fake()
        .set("pages/Two.md", &TWO.replace("Plain.", "Plain again.")); // 3
    r.w.fake().set(
        "pages/Two.md",
        &TWO.replace("Plain.", "Plain, a third time."),
    ); // 4
    r
}

fn titles_at_3(r: &Rig) -> std::sync::Arc<TitleIndex> {
    r.service
        .title_index_at_base(&r.w.wiki, None, &state_id(3), &block_id(3))
        .expect("an index")
}

fn knowledge_at_3(r: &Rig) -> std::sync::Arc<KnowledgeIndex> {
    r.service
        .knowledge_index_at_base(&r.w.wiki, None, &state_id(3), &block_id(3))
        .expect("an index")
}

/// **The places of a claim on a page whose text is hidden are not in the knowledge index at a base** (correction L-5): an anchor says
/// whether the paragraph is there and what its digest is, which is read from the page's text, and the page is found by the `id` its
/// frontmatter carries. The claim's own file is not hidden, and the places on a page that is not are as they were.
#[test]
fn the_places_of_a_claim_on_a_hidden_page_are_not_in_the_knowledge_index_at_a_base() {
    let r = world();
    // Nothing hidden: both claims are said where they are.
    let k = knowledge_at_3(&r);
    let (a, b) = (claim_id(1).to_string(), claim_id(2).to_string());
    assert_eq!(k.anchors_of(&a).count(), 1);
    assert_eq!(k.anchors_of(&b).count(), 1);
    let digest_of_secret = k
        .anchors_of(&a)
        .next()
        .and_then(|row| row.digest.clone())
        .expect("a digest of the paragraph");
    r.hide(2);
    r.warm();
    let k = knowledge_at_3(&r);
    assert!(k.claim(&a).is_some(), "a claim's own file is not hidden");
    assert_eq!(
        k.anchors_of(&a).count(),
        0,
        "the place on the page `One`, which change 2 wrote and has hidden"
    );
    assert_eq!(k.anchors_of(&b).count(), 1, "a page that is not hidden");
    for row in &k.rows().anchors {
        assert_ne!(row.page_path.as_deref(), Some("pages/One.md"), "{row:?}");
        assert_ne!(row.digest.as_deref(), Some(digest_of_secret.as_str()));
    }
    assert!(k.claims_at("p1", "b1").next().is_none());
}

/// **The title index at a base is filtered once for a base and a set of hides** (correction L-10), and **never served after a hide or
/// an unhide**: what a hidden change wrote is out of the index served, and back in when it is restored, at once.
#[test]
fn the_title_index_at_a_base_is_held_and_is_never_served_after_a_hide_or_an_unhide() {
    let r = world();
    let runs = || r.service.titles_filter_runs.load(Ordering::Relaxed);
    let one = |index: &TitleIndex| index.entry("pages/One.md").cloned().expect("a page");
    // Nothing hidden: nothing is filtered, and nothing is held.
    assert_eq!(one(&titles_at_3(&r)).aliases, ["Uno"]);
    assert_eq!(runs(), 0);
    let hide = r.hide(2);
    r.warm();
    let hidden = one(&titles_at_3(&r));
    assert_eq!(hidden.problem.as_deref(), Some(HIDDEN_PROBLEM));
    assert!(hidden.aliases.is_empty(), "nothing is read from the text");
    assert_eq!(runs(), 1);
    // The second request is served from what was held: no walk of the index.
    assert_eq!(
        one(&titles_at_3(&r)).problem.as_deref(),
        Some(HIDDEN_PROBLEM)
    );
    assert_eq!(runs(), 1, "held for the base and the moderation log");
    // An unhide gives the text back at once: with nothing hidden nothing is filtered, and nothing held is served.
    r.unhide(&hide, 2);
    let back = one(&titles_at_3(&r));
    assert_eq!(back.aliases, ["Uno"], "restored at once");
    assert!(back.problem.is_none());
    // And a hide hides it at once.
    r.hide(2);
    let again = one(&titles_at_3(&r));
    assert_eq!(again.problem.as_deref(), Some(HIDDEN_PROBLEM));
    assert!(again.aliases.is_empty());
    assert_eq!(runs(), 2);
    titles_at_3(&r);
    assert_eq!(runs(), 2, "held again");
    // **A hide that is added, and an unhide of it while another hide is still in force**, each change what is hidden: the held index
    // is of neither (the `Two` that change 3 wrote is hidden with it, and is back without it).
    let two = |index: &TitleIndex| index.entry("pages/Two.md").cloned().expect("a page");
    assert!(two(&titles_at_3(&r)).problem.is_none());
    let second = r.hide(3);
    assert_eq!(
        two(&titles_at_3(&r)).problem.as_deref(),
        Some(HIDDEN_PROBLEM)
    );
    assert_eq!(runs(), 3);
    r.unhide(&second, 3);
    let restored = titles_at_3(&r);
    assert!(
        two(&restored).problem.is_none(),
        "the unhide of one hide is not answered with the index made before it"
    );
    assert_eq!(
        one(&restored).problem.as_deref(),
        Some(HIDDEN_PROBLEM),
        "and the other hide is still in force"
    );
    assert_eq!(runs(), 4);
}
