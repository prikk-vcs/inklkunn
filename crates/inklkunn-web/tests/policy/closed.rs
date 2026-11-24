//! **The policy page fails closed** (handoff G-T7, review correction G-2): the history of the rules names each change by the
//! proposal that made it, and says a summary only when it is known not to be hidden; and one change whose text moderation hides
//! is *hidden* in the history, not the whole history *could not be read now*. Every text that must not be shown carries `ZEBRA`.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use inklkunn_core::proposal::{ProposalService, QuotaConfig};
use inklkunn_core::tombstone::Tombstones;
use inklkunn_store::{
    AcceptanceBegin, AcceptanceEntry, AcceptanceId, AcceptanceRecord, AcceptanceResult,
    AcceptanceStore, AccountId, GovernanceStore, IntentCompletion, IntentEntry, IntentRecord,
    LeaseHolder, MemoryStore, ProposalId, Result, SealedAcceptance, StoreError, WikiId,
};

use super::page::{a_proposal, changed_rules};
use super::*;
use common::NOW;

// ------------------------------------------------------------------------------------ the store that says what is hidden fails

/// The acceptance records, over the in-memory store, **with a reader that fails**: `acceptances_of` is what the tombstone reader
/// asks to say what is hidden of a proposal's revision. It answers `allow` times and then fails, so that a test says *which* of the
/// page's lookups is the one that fails.
/// More reads than a page makes.
const READS: i64 = 1_000;

struct Flaky {
    inner: Arc<MemoryStore>,
    allow: AtomicI64,
}

impl Flaky {
    /// Answer `n` more reads of a proposal's revision, then fail.
    fn arm(&self, n: i64) {
        self.allow.store(n, Ordering::SeqCst);
    }
}

impl AcceptanceStore for Flaky {
    fn begin_acceptance(&self, a: &AcceptanceRecord) -> Result<AcceptanceBegin> {
        self.inner.begin_acceptance(a)
    }
    fn finish_acceptance(&self, r: &AcceptanceResult) -> Result<()> {
        self.inner.finish_acceptance(r)
    }
    fn acceptances_of(&self, p: &ProposalId, revision: u32) -> Result<Vec<AcceptanceEntry>> {
        if self.allow.fetch_sub(1, Ordering::SeqCst) <= 0 {
            return Err(StoreError::Backend("the records are down".to_owned()));
        }
        self.inner.acceptances_of(p, revision)
    }
    fn pending_acceptance(
        &self,
        id: &AcceptanceId,
    ) -> Result<Option<(AcceptanceRecord, IntentRecord)>> {
        self.inner.pending_acceptance(id)
    }
    fn acceptance_entry(&self, id: &AcceptanceId) -> Result<Option<AcceptanceEntry>> {
        self.inner.acceptance_entry(id)
    }
    fn pending_acceptances(&self, wiki: &WikiId) -> Result<Vec<AcceptanceRecord>> {
        self.inner.pending_acceptances(wiki)
    }
    fn sealed_acceptances(&self, wiki: &WikiId) -> Result<Vec<SealedAcceptance>> {
        self.inner.sealed_acceptances(wiki)
    }
    fn accepted_bytes_since(
        &self,
        wiki: &WikiId,
        proposer: &AccountId,
        since: i64,
    ) -> Result<Vec<(i64, u64)>> {
        self.inner.accepted_bytes_since(wiki, proposer, since)
    }
    fn record_intent(&self, i: &IntentRecord) -> Result<()> {
        self.inner.record_intent(i)
    }
    fn complete_intent(&self, c: &IntentCompletion) -> Result<()> {
        self.inner.complete_intent(c)
    }
    fn open_intents(&self, wiki: &WikiId) -> Result<Vec<IntentEntry>> {
        self.inner.open_intents(wiki)
    }
    fn intents(&self, wiki: &WikiId) -> Result<Vec<IntentEntry>> {
        self.inner.intents(wiki)
    }
    fn lease_holder(&self, wiki: &WikiId) -> Result<Option<LeaseHolder>> {
        self.inner.lease_holder(wiki)
    }
    fn record_lease_holder(&self, h: &LeaseHolder) -> Result<Option<LeaseHolder>> {
        self.inner.record_lease_holder(h)
    }
    fn release_lease_holder(&self, wiki: &WikiId, holder: &str) -> Result<bool> {
        self.inner.release_lease_holder(wiki, holder)
    }
}

/// The wiki served with a proposal service whose tombstone reader asks `flaky`, over the same records.
fn with_flaky_tombstones(e: Env) -> (Env, Arc<Flaky>) {
    let store = e.w.store.clone();
    let flaky = Arc::new(Flaky {
        inner: store.clone(),
        allow: AtomicI64::new(READS),
    });
    let mut services = (*e.w.services).clone();
    services.proposals = Arc::new(
        ProposalService::new(
            store.clone(),
            store.clone(),
            store.clone(),
            (*services.authorizer).clone(),
            services.read.clone(),
            QuotaConfig::default(),
        )
        .with_roles(store.clone())
        .with_tombstones(Arc::new(Tombstones::new(store, flaky.clone())))
        .with_clock(|| NOW),
    );
    let e = Env {
        w: e.w.with_services(Arc::new(services)),
        ..e
    };
    (e, flaky)
}

#[tokio::test]
async fn a_store_that_cannot_say_what_is_hidden_shows_the_tombstones_place_and_never_the_summary() {
    let e = env();
    let proposal = a_proposal(&e, "ZEBRASUMMARY of the proposal").await;
    changed_rules(
        &e,
        2,
        &proposal,
        "ZEBRASUMMARY of the proposal",
        "governance/policy.yaml",
        Some("rungs:\n  accept: reviewer\n"),
    );
    let (e, flaky) = with_flaky_tombstones(e);

    // Control: while the store answers, the summary is the proposal's label. How many reads of the store the page makes is counted.
    flaky.arm(READS);
    let main = main_of(&as_(&e, &e.contributor, PAGE).await);
    assert!(main.contains("ZEBRASUMMARY of the proposal"), "{main}");
    let reads = READS - flaky.allow.load(Ordering::SeqCst);
    assert!(
        reads >= 2,
        "the page asks what is hidden of the proposal twice: {reads}"
    );

    // Whichever read of the store is the one that fails (the first, the second, or all), the label is the place of the summary
    // and the words are nowhere on the page, and the page is whole.
    let mut shown_place = 0;
    for allowed in 0..reads {
        flaky.arm(allowed);
        let a = as_(&e, &e.contributor, PAGE).await;
        assert_eq!(a.status, StatusCode::OK, "{allowed}: {}", a.body());
        let body = a.body();
        assert!(!body.contains("ZEBRA"), "{allowed}: {body}");
        let main = main_of(&a);
        assert!(
            main.contains("Who may accept a proposal: maintainer or above → reviewer or above"),
            "{allowed}: the rest of the page is whole: {main}"
        );
        if main.contains("(summary hidden)") {
            shown_place += 1;
        }
    }
    assert!(shown_place > 0, "the place of the summary was shown");
}

// ------------------------------------------------------------------------------------ one hidden change

/// Three changes of the rules (blocks 2, 3 and 4, the last the tip), the middle one with a protection whose reason must not be
/// shown once the change is hidden.
async fn three_changes() -> Env {
    let e = env();
    for (version, summary, text) in [
        (2, "First of three", "rungs:\n  accept: reviewer\n"),
        (
            3,
            "Second of three",
            "rungs:\n  accept: reviewer\nprotect:\n  - path: Home\n    require: review\n    reason: \"ZEBRAREASON of the second\"\n",
        ),
        (
            4,
            "Third of three",
            "rungs:\n  accept: reviewer\nentry: observer\n",
        ),
    ] {
        let proposal = a_proposal(&e, summary).await;
        changed_rules(
            &e,
            version,
            &proposal,
            summary,
            "governance/policy.yaml",
            Some(text),
        );
    }
    e
}

/// Hide the content of change `n` as the moderator.
async fn hide(e: &Env, n: u64) {
    let a = post_form(
        &e.w,
        "/handbook/-/moderate",
        Some(&e.maintainer),
        &[
            ("kind", "page-revision"),
            ("block", common::block_id(n).as_str()),
            ("part_content", "1"),
            ("category", "personal-data"),
            ("note", ""),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
}

const THIS_HIDDEN: &str =
    "The text of the rules in this change is hidden, so what it did is not shown.";
const EARLIER_HIDDEN: &str = "The text of the earlier change is hidden, so no difference is shown.";

#[tokio::test]
async fn one_hidden_change_of_the_rules_is_said_hidden_and_the_others_are_listed() {
    let e = three_changes().await;
    // Nothing is hidden: all three are listed, and the second says what it did.
    let main = main_of(&as_(&e, &e.contributor, PAGE).await);
    for want in [
        "First of three",
        "Second of three",
        "Third of three",
        "ZEBRAREASON of the second",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    assert!(!main.contains(THIS_HIDDEN), "{main}");

    hide(&e, 3).await;
    for who in [&e.contributor, &e.maintainer, &e.observer] {
        let a = as_(&e, who, PAGE).await;
        assert_eq!(a.status, StatusCode::OK, "{}", a.body());
        let body = a.body();
        assert!(!body.contains("ZEBRA"), "{body}");
        let main = main_of(&a);
        // The history is whole: not *could not be read now*, and every change is listed with its proposal.
        assert!(
            !main.contains("The history of the rules could not be read now."),
            "{main}"
        );
        for want in ["First of three", "Second of three", "Third of three"] {
            assert!(main.contains(want), "{want}: {main}");
        }
        // The first is as it was; the hidden one says hidden; the one after it cannot say what it changed from.
        assert!(
            main.contains("Who may accept a proposal: maintainer or above → reviewer or above"),
            "{main}"
        );
        assert_eq!(main.matches(THIS_HIDDEN).count(), 1, "{main}");
        assert_eq!(main.matches(EARLIER_HIDDEN).count(), 1, "{main}");
        assert!(!main.contains("Protection added"), "{main}");
    }
    // In Japanese too.
    let ja = main_of(&as_ja(&e, Some(&e.contributor), PAGE).await);
    assert!(
        ja.contains(
            "この変更に含まれるルールの本文は非表示のため、何を変えたかは表示していません。"
        ),
        "{ja}"
    );
    assert!(!ja.contains("ZEBRA"), "{ja}");
}

#[tokio::test]
async fn the_oldest_change_hidden_leaves_the_newer_ones_whole_and_a_restoring_gives_the_sentences_back()
 {
    let e = three_changes().await;
    hide(&e, 2).await;
    let main = main_of(&as_(&e, &e.contributor, PAGE).await);
    // The first change of three is hidden; the second cannot say what it changed from; the third is as it was.
    assert_eq!(main.matches(THIS_HIDDEN).count(), 1, "{main}");
    assert_eq!(main.matches(EARLIER_HIDDEN).count(), 1, "{main}");
    assert!(
        main.contains("people hold only the role they are granted → observer"),
        "{main}"
    );
    // The moderator restores it: nothing is hidden any more.
    let log =
        e.w.store
            .tombstone_log(&e.w.wiki.id, None, 10)
            .expect("the log");
    let hide_id = log
        .iter()
        .find(|event| event.act == inklkunn_store::TombstoneAct::Hide)
        .expect("a hide")
        .id
        .to_string();
    let a = post_form(
        &e.w,
        "/handbook/-/moderate/unhide",
        Some(&e.maintainer),
        &[("hide", hide_id.as_str())],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let main = main_of(&as_(&e, &e.contributor, PAGE).await);
    assert!(!main.contains(THIS_HIDDEN), "{main}");
    assert!(!main.contains(EARLIER_HIDDEN), "{main}");
    assert!(
        main.contains("Who may accept a proposal: maintainer or above → reviewer or above"),
        "{main}"
    );
}
