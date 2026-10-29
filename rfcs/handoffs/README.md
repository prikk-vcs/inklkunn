# inklkunn handoffs — structure, storage and status

Companion execution documents to the RFCs, per the 5-folder lifecycle
([`../done/000-rfc-lifecycle-policy.md`](../done/000-rfc-lifecycle-policy.md)). An RFC defines and justifies a
change; a **handoff** directs its implementation and must not silently redefine its RFC. If handoff work finds a
design conflict, the RFC changes first, then the handoff.

## Layout and naming

- One directory per RFC: `NNN-slug/`. **Every handoff implements an RFC.** A follow-on handoff of a done RFC lives
  in that RFC's directory and is listed in the RFC's Status as deferred or follow-on work.
- Files: `<slug>-handoff-vN.md` (the handoff), `<slug>-review-request*.md` (the implementer's package back),
  `t<N>-*.md` (task-level records such as measurement reports). **A v2 supersedes a v1 and both stay**: the v1
  keeps the reasoning that the measurement overturned.
- Every handoff opens with the header table (Document · Status · Basis · Audience · Scope) and has three fixed
  parts: **the task breakdown in build order** (`T1…`, each with purpose, applicable RFC decisions, change scope,
  non-change scope, required tests), **the QA checklist** drawn from the RFC's acceptance criteria, and **the
  definition of done and handback** (which gates, which evidence, what the review request must contain).
- Evidence is by reference: commit hashes on `main`, CI run ids, gate outputs quoted with their exit codes. A claim
  without a reference is not evidence.

## State of handoffs

A handoff has no state of its own: it inherits its RFC's. **The RFC index ([`../README.md`](../README.md)) is the
single source of truth.** Under an RFC in `proposed/` a handoff is **not startable**; under `accepted/` it is
assignable; under `done/` it is history. A handoff may additionally name a prerequisite handoff that must have
landed and been approved first (RFC 002's names RFC 001's).

## What a review request carries back

**Where it lives (since 2026-10-28; results since 2026-10-29):** review requests are private files in
`.git-exclude/review-requests/` (named `<NNN>-<handoff slug>-review-request.md`), and the architect's result for each is a
private file in `.git-exclude/reviewed/` (named `<NNN>-<handoff slug>-review-result.md`), like the upstream
correspondence; the outcome and every correction that binds future work are folded into the committed record
(the RFC's Status, a corrections handoff or a dated note in the handoff, `docs/STATUS.md`). A handoff that still says
"beside this file" means that folder.

The implementation summary; the commits (hash → what landed); every gate with its command and exit code,
including the test run **with** prikk on PATH and the run **with prikk shimmed off PATH** (CI's own run has
prikk installed at the floor, but the shimmed run proves the self-skips are visible); the dependency-tree
delta with `cargo deny` and `cargo audit` results when a manifest changed; deviations from the handoff and why;
known limitations; open questions for the architect. The architect answers *Approved*, *Conditionally
Approved*, *Corrections Required*, *Design Reconsideration Required*, *Requirements Clarification Required* or
*Owner Decision Required*.
