# Verification verdicts under a memory ceiling — Developer Handoff v1 (RFC 001 follow-on, issue IS-7)

| | |
|---|---|
| Document | A follow-on of RFC 001's driver (`inklkunn-prikk`): `verify` must never turn a memory-ceiling hit into *Verification failed*. One task, one review request. |
| Status | **Done and approved 2026-10-29** (`34def34`; review result `.git-exclude/reviewed/001-verify-verdict-honesty-review-result.md`); follow-ups V1, V2 below are owed before RFC 003's T4 shows the badge. Was: RFC 001 is **done**; this follow-on is **assignable now** (architect, 2026-10-28). It must land **before RFC 003's T4** (the badge and the verification page), and it may run beside RFC 002's handoff and RFC 003's T1–T3. |
| Basis | RFC 001 D-3, D-4; threat model C-9a ("a ceiling hit is *unavailable*, never a verification verdict"), INV-6; requirements RD-4, INT-6 (a wiki whose last verification failed refuses acceptances); ledger PK-8, PK-9, **PK-22** (the shape, measured on prikk 0.54.1), **PL-7** (planeter's IS-43 rule, the precedent); `crates/inklkunn-prikk/src/driver.rs` (`verify`, `failure`), `src/storage.rs` (`Stage`, `Verdict`, `Verification`), `tests/driver.rs` (`a_too_small_ceiling_is_unavailable_never_a_verdict`). |
| Audience | The dev team. Read the two lenses in RFC 003's handoff first; they apply here unchanged. |
| Scope | The driver's reading of `verify-report-v1` and its tests. **Not** the badge, the verification page or any wording (RFC 003 T4); **not** the cost table (RFC 003 T9). |

## Why

Under `RLIMIT_DATA`, prikk 0.54.1's `verify` sometimes does not abort: its `objects` stage fails with the message
`i/o error: out of memory`, the other stages finish, and prikk prints a **complete** `verify-report-v1` with
`verdict.ok: false` and exits 1. The architect reproduced it on 2026-10-28 with the driver's own vector (`prlimit
--data=N:N bwrap … --ro-bind <repo> … prikk verify --format json`) on a 70-block, 40-page repository at ceilings of
2–3 MiB (larger ceilings verify, smaller ones abort with exit 134). The driver today accepts **any** parseable failing
report as a verdict (`run.status.success() || !report.verdict.ok`), so this shape would be shown as *Verification
failed* and, by INT-6, the wiki would refuse acceptances: a resource limit presented as damage to the repository.

Production ceilings sit well above what wiki-shaped repositories need (spike A), so the shape is reached only when
the cost table underestimates a repository; RFC 003 T9 re-measures the table, but the driver must be honest
**whatever** the table says. The existing confined ceiling test already rejects a failed verdict, but its two-block
fixture never produces the shape (measured: small histories only abort), so it cannot see the defect.

What genuine damage looks like on 0.54.1 (measured the same day, a ref pointer index with two flipped bytes): exit 1;
`objects`, `refs` and `local-tag-trust` `failed` with messages starting `integrity error:`; `ref-update-schema-trust`
and `publication-reclassification` `not_evaluated` with `"blocked_by": "refs"`. The report itself tells the two
apart; stderr does not (both print "repository verification did not complete every stage").

## T1 — A failing report is a verdict only when it is complete

- **Model the stage** as prikk reports it: `evaluated`; `failed` with its `message`; `not_evaluated` with `blocked_by`;
  `halted`; and an unknown status kept as such (never mapped to `evaluated`). The public `Stage` type may change; it
  has no caller outside the crate yet (check with `grep`), and the change is listed in the review request.
- **The rule** (planeter's IS-43 rule, re-derived for this driver, not copied): a report with `verdict.ok == false`,
  exit 1 and no timeout or output bound is returned as `Ok(Verification)` **only if every stage ran to an outcome**:
  `evaluated`; `failed` with a message that is **not** a resource failure; or `not_evaluated` whose `blocked_by`
  names a stage **of the same report** that is `failed` with a finding. A `halted` stage, an unknown status, a
  `not_evaluated` stage whose blocker is missing, unknown, itself `not_evaluated`, or failed for lack of memory makes
  the report **partial**.
- **A resource failure** is a `failed` stage whose message, after the exact prefix `i/o error:`, is exactly `out of
  memory` (trimmed) — the measured shape — or exactly `Cannot allocate memory (os error 12)` (Rust's text for a raw
  `ENOMEM`, accepted only whole). **Never a substring match**: a message may embed a name the repository's
  contributors chose (a page path, a ref), and a page titled "out of memory" must not turn damage into a ceiling hit.
- **A partial report** is `Unavailable { CeilingExceeded { bytes } }` when a ceiling was set (the stage that did not
  finish ran out of memory or was blocked behind it), and `Damaged` with the stage outcomes as detail when none was
  set (a call without a ceiling never happens in production; say so in the doc comment). Never a verdict.
- `failure()`'s existing mapping for aborts, loader exits, bubblewrap's "Out of memory" and signals is unchanged.

## T2 — Tests that see the shape

- **Real prikk, the measured shape:** a history long enough to produce it (the architect's was 70 blocks of 40 small
  pages). **Generate it in a test through the `bootstrap` signer** (a dev-dependency feature, as `create_wiki`'s tests
  do; the read driver's crate may name no write verb, so the generating test lives where the gate allows it), or commit
  a fixture under 1 MiB if a smaller history shows the shape (measure; say which). Then the property: **on a repository
  that verifies unconfined, no ceiling in a sweep from 1 MiB to 16 MiB in 256 KiB steps ever yields `verdict.ok ==
  false`**; every outcome is `Ok` with `ok == true` or `CeilingExceeded`. Record which ceilings produced the shape on
  your machine. This test must **fail on the current driver** (show it in the review request).
- **Stand-in prikk** (the counting-shim pattern of M0): the measured out-of-memory report with exit 1 → ceiling;
  a genuine-damage report (the shape above) → verdict; a planted name — a stage message `integrity error: page "out
  of memory" …` — → verdict; a `not_evaluated` stage blocked by a stage missing from the report, by an unknown id, by
  a `not_evaluated` stage, by an out-of-memory stage → partial; a `halted` stage → partial; an unknown status →
  partial; each with and without a ceiling.
- **Mutation evidence:** remove the prefix requirement (the planted-name test fails); accept any `not_evaluated`
  (a stand-in test fails); accept any failing report (the real-prikk sweep fails).
- `EXPECTED_SKIPS` in `ci.yml` and `release.yml` moves with every new prikk-dependent test.

## Non-change scope

No change to the other verbs, the cost table, the admission gate, the sandbox vector, any wording or page; no new
dependency; no `unsafe`; no version change.

## Documentation

The doc comments of `verify` and `Stage`; the ledger row PK-22 is the architect's (send the measured ceilings and
messages in the review request and the architect folds them); CHANGELOG `[Unreleased]`: "A memory limit reached
while checking a repository is reported as *could not be checked now*, never as a failed verification."

## Acceptance criteria and handback

T1's rule as written; T2's tests pass and the real-prikk sweep fails on the old code; the mutation table; every gate
green (fmt, clippy on both compilers, rustdoc, tests with prikk on `PATH` and shimmed off with the new
`EXPECTED_SKIPS`, deny, audit, the four gates). The review request, `verify-verdict-honesty-review-request.md` in
`.git-exclude/review-requests/`, carries: the commits; the measured ceilings and report excerpts; the gate table with exit codes; the
mutation table; deviations and open questions. The architect answers with one of the six outcomes.

## Follow-ups from the review of 2026-10-29 (binding; owed before RFC 003's T4 shows the badge)

- **V1.** Apply the completeness rule to every parsed report whose `verdict.ok` is false, **whatever the exit code**:
  prikk's contract sends findings with exit 1, so `ok: false` with exit 0 should never come, but if it does it must not
  pass unchecked. A unit case pins it.
- **V2.** A failing report with **no stages** is partial, not a verdict (prikk's report walks every stage by
  construction, so an empty list proves nothing finished). Give the stand-in in `crates/inklkunn-signer/tests/accept.rs`
  (*a verification that fails after the seal is sealed but unconfirmed*) a real-shaped report; a unit case pins the rule.
