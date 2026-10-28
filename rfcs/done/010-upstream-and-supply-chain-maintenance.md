# RFC 010 — Upstream tracking and supply-chain maintenance: two fast upstreams, one checked floor, gates at every project event

**Status.** **Done** — implemented on `main` (M0) and **approved by the owner on 2026-10-28** (review request v2 under `../handoffs/`, after the architect's independent review; *"Your recommendations are approved."*). Accepted by the owner 2026-10-27 (*"Accepted."*; the open question on the container image is taken
as the recommendation: binaries and an image from the first release). Proposed the same day by the architect.
**Deferred (recorded 2026-10-28):** the last acceptance criterion, *the first re-baseline executed by the
procedure*, waits for the first prikk release after M0 began: prikk 0.55.0 (in progress, unreleased on 2026-10-28;
`docs/STATUS.md`). The procedure is in place; its first run is recorded there when it happens.
Numbered 010 by the roadmap's plan so that 003–009 keep
their reserved subjects; it is written with RFC 001 because M0's workspace must carry its gates from the
first commit. Handoff, assignable now together with RFC 001's:
[`../handoffs/010-upstream-and-supply-chain-maintenance/upstream-and-supply-chain-handoff-v1.md`](../handoffs/010-upstream-and-supply-chain-maintenance/upstream-and-supply-chain-handoff-v1.md).
**Tracks.** ROADMAP theme T10 and §5 (release cycles); requirements `SEC-8`, `SEC-10`,
`DUR-3`, `OPS-4`, `OPS-6`, `INT-1`, `INT-9`, `UD-1 … UD-10`, `BN-4`, `BN-5`; threat model `T-11`,
`C-11`, `INV-9`; the project rules on releases and on documentation.
**Touches.** `tools/floor-check`, `tools/pin-check`, the CI and release workflows, `docs/dependency-policy.md`,
`docs/src/inklkunn-upstream-dependency-ledger.md`, `docs/UPSTREAM.md`, `.git-exclude/upstream/` (private
correspondence). **Not here:** any product behaviour.

## Summary

inklkunn depends on two fast-moving upstreams: **prikk**, whose JSON is pre-1.0 and whose releases arrive days
apart, and **planeter**, whose patterns inklkunn reuses and whose module mechanism does not exist yet. It also
adopts **wiki-commons**, a working draft whose identifiers may change. And it ships a Rust service whose
dependency tree is an attack and audit surface. This RFC makes the maintenance of all of that a procedure
with gates, adopted from planeter's RFC 010 where the problem is the same and adapted where inklkunn's is
different: **one checked floor per upstream, a re-baseline at every upstream release, supply-chain gates at
every project event, a pinned build chain, a dependency ledger that records every accepted and rejected
crate with its measured cost, and an upstream ledger that records what each upstream really does, by letter
and by measurement.**

## Decisions

- **D-1 — The re-baseline procedure, per prikk release.** (1) Read the release notes, the changelog's
  "Output changes" and any letter. (2) Install the binary on the development host (the owner's act) and
  record `prikk --version`. (3) Run the version-pinned suites with it on PATH, then once with prikk shimmed
  off PATH (what CI sees). (4) Decide the floor: bump only for a verb inklkunn needs or a security or
  correctness fix a wiki depends on; the three floors move together (the driver constant, CI, the container
  recipe) and every "≥ x.y.z" mention follows. (5) Re-measure the per-verb cost table (RFC 001 D-4) on the
  new binary; update multiples only from measurement. (6) Record the outcome in the upstream ledger
  (D-7); advance STATUS; add a hold note to any handoff whose assumption moved. (7) A format change is a
  deployment rule: the serving binary stays at or above the hosted format; no wiki repository is upgraded
  without an operator's intended act (DUR-3).
- **D-2 — The re-baseline per planeter release.** inklkunn links no planeter code in v1, so the check is
  narrower: the patterns reused (the catalog's rules, the glossary words shared under MD-7, the admission
  shape, the recovery witnesses) are compared with planeter's release, divergences are recorded in the
  ledger, and the joint RFC 008's assumptions are re-checked. The wiki-commons check is the same shape:
  corpus version, identifier changes, the conformance claims' version.
- **D-3 — The floor is one checked value per upstream.** `tools/floor-check` fails unless the prikk floor in
  `inklkunn-prikk`, in the CI workflow and in the container recipe is one `X.Y.Z`; the wiki-commons corpus
  copy records its commit, and a test fails if the fixtures and the recorded commit disagree.
- **D-4 — Supply-chain gates at project events.** `cargo deny check` and `cargo audit` exit 0 at (a) every
  push and pull request, (b) every release's gate job, (c) every re-baseline, (d) every RFC disposition
  checkpoint, where the architect runs both against a freshly fetched advisory database, (e) whenever the
  owner asks. A manually dispatched workflow leaves a durable record for (d). Red becomes an issue-register
  entry the same day.
- **D-5 — The advisory triage ladder** (adopted from planeter): (1) upgrade to a fixed version; (2) remove
  the path (trim a feature, swap a crate); (3) a scoped ignore with the reachability shown and a `reason` in
  both `deny.toml` and `.cargo/audit.toml`, re-examined at every change touching its path; (4) never a
  silent or unscoped ignore. The ledger records which rung was taken and why.
- **D-6 — The dependency policy and ledger** (`docs/dependency-policy.md`): measure before commit (build with
  the candidate; count the lock; run deny and audit; report "N → M crates (+K); deny: …; audit: …; new
  licenses: …" before the commit that adopts it); licenses are an allow-list (Apache-2.0, MIT, Apache-2.0
  WITH LLVM-exception, Unicode-3.0, BSD-3-Clause, ISC, MPL-2.0 file-level; adding one is a decision request
  with the crate, the path and the compatibility argument); prefer a confined subprocess to an in-process
  protocol stack; `forbid(unsafe_code)` in every crate; `--locked` everywhere and the lock committed. Two
  tables: accepted (date, crates, measured cost, notes) and rejected (date, candidate, why), so settled
  battles are not refought. planeter's rejected list (an SSH-key crate that locks an unfixable advisory;
  in-process TLS and SSH stacks; a heavier JWT backend; an in-process HTTPS client for OpenID Connect) is
  carried in as evidence, re-measured before any inklkunn decision relies on it.
- **D-7 — The upstream ledger** (`docs/src/inklkunn-upstream-dependency-ledger.md`): one row per behaviour
  inklkunn relies on, `PK-nn` for prikk, `PL-nn` for planeter, `WC-nn` for wiki-commons, each with a status
  **CONFIRMED** (measured here), **RULED** (the upstream owner's word, cited), **ASSUMED** (documents only,
  the live risk list), **GATED** (accepted upstream, not shipped), and the evidence. Every letter received
  is folded in before any code changes. The first rows come from the startup review and the design set
  (UD-1 … UD-10).
- **D-8 — Correspondence protocol.** Letters to the prikk or planeter teams are private files under
  `.git-exclude/upstream/<project>/` in `send/draft/` (the architect), `send/` (the owner adjusted and sent),
  `receive/` (as received), named `YYYY-MM-DD-<subject>.md`. **The architect drafts; the owner conveys.** No
  letter leaves without the owner. A draft states what inklkunn measured, what it asks, and the planning
  assumptions behind the ask, with numbers.
- **D-9 — The pinned build chain.** One compiler version across CI, the checkpoint workflow and the release
  build, named in one place; the declared `rust-version` proven by a job on exactly that compiler; CI
  actions by 40-hex commit SHA and only from GitHub-owned namespaces; images by digest; named runners,
  never `*-latest`; `tools/pin-check` fails on any of these and has a planted failure per rule. A pin
  changes only by a reviewed change naming the upstream diff.
- **D-10 — Release gates and rehearsal.** The release workflow runs the full gate set on the tagged commit
  (fmt, clippy, tests, deny, audit, the RFC 001 gates, floor-check, pin-check, rustdoc), builds the Linux
  binaries and the container image, attests them, publishes the GitHub release with the CHANGELOG section,
  and publishes the crates in dependency order; a `rehearsal` dispatch mode pushes only to a throwaway
  package and can never reach the real one (pin-check enforces the shape). Tagging, publishing and the
  release itself are the owner's acts or the architect's when delegated per release.

## What "done" means (acceptance criteria)

- `floor-check` and `pin-check` exist, run in CI and in the release gates, and each fails its planted
  failure.
- The CI workflow runs the gate set on every push with the pinned toolchain and named runners; the
  checkpoint workflow exists; the release workflow has the rehearsal mode and the rehearsal has run green
  once to the throwaway package.
- `docs/dependency-policy.md`, the upstream ledger with the UD-1 … UD-10 rows, and `docs/UPSTREAM.md` (the
  procedure of D-1, D-2, D-8) exist; `.git-exclude/upstream/` has the three folders per upstream.
- The first re-baseline (the next prikk release after M0 starts) is executed by the procedure and recorded.

## Alternatives considered

- **A clock-driven audit schedule** instead of event gates. Rejected: an advisory is caught at the next
  event; a schedule adds noise without changing when the fix lands; the manual checkpoint covers quiet
  periods (planeter's D-3 reasoning).
- **Unpinned actions and `*-latest` runners.** Rejected: the build chain is part of the supply chain
  (planeter IS-35).
- **No planeter re-baseline because no code is linked.** Rejected: the shared vocabulary and reused
  patterns drift silently otherwise, and RFC 008 will depend on planeter's surfaces.

## Open questions

- Whether inklkunn publishes a container image from the first release or only binaries: the owner's,
  recommended both (the image pairs the binary with its prerequisites, as planeter ruled for itself).
- The wiki-commons corpus is copied, not depended on; whether to propose inklkunn's own cases back is
  BN-4's ordinary course.

## Sequencing and handoff

With RFC 001: the gates and workflows land in M0's first commits. The handoff is drafted (v1) and becomes
assignable with RFC 001's.
