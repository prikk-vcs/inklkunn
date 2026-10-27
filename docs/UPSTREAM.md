# inklkunn — the upstreams: correspondence and re-baseline

inklkunn stands on three moving things: **prikk** (its storage; pre-1.0 JSON; releases days apart),
**planeter** (the forge whose patterns it reuses and whose module it will become) and **wiki-commons** (the
guideline suite and conformance corpus it conforms to; a working draft). This document is the procedure for
staying true to all three (RFC 010 D-1, D-2, D-8). The record of what each upstream does is the
[upstream dependency ledger](src/inklkunn-upstream-dependency-ledger.md); the record of every crate is the
[dependency policy](dependency-policy.md).

## The relationship

- The owner of inklkunn is also the owner of prikk, planeter and wiki-commons. That makes a letter cheap
  to send and a ruling cheap to get, and it makes it tempting to skip the record. The record is not skipped:
  **every fact inklkunn relies on is a ledger row with evidence**, whoever stated it.
- inklkunn asks prikk for nothing it can do itself above prikk, and never works around a prikk refusal
  (a refusal is a precondition, PK-2). What it needs and cannot do is **shown as pending and asked by
  letter** (requirements UD-10; external design PK-06).

## Correspondence protocol

- Letters are **private** files under `.git-exclude/upstream/<prikk|planeter>/`: `send/draft/` (the
  architect drafts), `send/` (the owner adjusted and sent it; the file moves up), `receive/` (replies and
  notices as received). Names are `YYYY-MM-DD-<subject>.md`. wiki-commons is the owner's own suite; a
  finding goes to it as an issue or a corpus case rather than a letter, and is recorded the same way.
- **The architect drafts; the owner conveys.** No letter leaves without the owner. A draft states what
  inklkunn measured, what it asks, and the planning assumption behind the ask, with numbers.
- **Every received reply is folded into the committed records before any code changes:** rows in the
  upstream ledger (status, evidence, the letter named in the version line), ROADMAP's dependency notes and
  milestone table when a schedule or ruling moved, and a hold note in the affected handoff. The letters are
  the source; the ledger is what a reader relies on.
- Open asks as of 2026-10-27 (drafts to write): **PK-8** (the exit code of a failing `verify` verdict under
  `--format json`, so the driver's reading of the report is confirmed against prikk's intent) and **PK-11**
  (the 1024-byte path limit: whether it is a stable rule inklkunn should surface as a lint, and whether
  deeper hierarchies are wanted anywhere).

## Re-baseline procedure, per prikk release (RFC 010 D-1)

1. **Read the release**: notes, the changelog's "Output changes", any letter. List what touches inklkunn:
   verbs, JSON shapes, exit codes, environment variables, formats, security fixes.
2. **Install the binary** on the development host (`cargo install prikk --version X.Y.Z --locked`; the
   owner's act) and record `prikk --version`.
3. **Run the version-pinned suites with it on `PATH`** (`cargo test --workspace --locked`): the driver
   refuses an unexpected `schema_version`, so drift fails loudly. Then run once more with prikk **shimmed off
   `PATH`** (what CI's second run sees): every prikk-dependent test must print its `skipping:` line.
4. **Decide the floor.** Bump `MIN_PRIKK_VERSION` only for a verb inklkunn needs or a security or correctness
   fix a wiki depends on. The three floors move together — `crates/inklkunn-prikk/src/lib.rs`,
   `.github/workflows/ci.yml` (`PRIKK_VERSION`), `Dockerfile` (`ARG PRIKK_VERSION`) — and `floor-check`
   fails until they do; every "≥ x.y.z" mention follows (README, RELEASING, ROADMAP, STATUS). A floor bump
   is release-relevant: the review request says so; the owner decides the version.
5. **Re-measure the cost table** (`crates/inklkunn-prikk/src/cost.rs`): `BOUND_TO` must equal the floor
   (a test fails otherwise). Method: `cargo run --release -p inklkunn-corpus -- wiki …` for the 1 / 16 / 64 /
   200 MiB shapes, then `tools/spikes/spike-a.py --bisect`; update the multiples **only from the
   measurement**, never to make a test pass; record the run in the spikes report and the ledger (PK-20).
6. **Rebuild the driver fixture** (`crates/inklkunn-prikk/tests/fixtures/README.md`) if the on-disk format
   changed; it is otherwise left alone so that history stays comparable.
7. **Record**: the ledger's version line and the affected rows; `docs/STATUS.md`'s upstream table; a hold
   note on any handoff whose assumption moved.
8. **Formats are a deployment rule, not code**: the serving binary stays at or above the on-disk format its
   hosted repositories carry; inklkunn never upgrades a wiki's repository format on its own (DUR-3).

## Re-baseline per planeter release (RFC 010 D-2)

inklkunn links no planeter code in v1. The check is narrower:

1. Read planeter's release notes and `docs/STATUS.md`; note changes to the patterns inklkunn reuses: the
   sandbox vector and its findings (PL-2), the admission shape (PL-3), the dependency policy and its rejected
   list (PL-4), RFC 012's catalog rules and glossary words shared under MD-7, RFC 004's recovery witnesses
   (PL-6).
2. Record divergences in the ledger; where planeter learned something inklkunn's copy lacks (a sandbox
   finding, a cost correction), open an issue-register entry the same day.
3. Re-check the joint RFC 008's assumptions (the module mechanism, shared identity, OQ-4) and move PL-1
   when planeter rules.

## Re-baseline per wiki-commons change

1. Compare the corpus: copy `corpus/markup` and `corpus/resolution` again, update `CORPUS-SOURCE.md`'s
   commit, run `floor-check` and paste the new digest; run the markup tests. A new or changed expectation
   that inklkunn fails is a finding: either a scanner fix here or a corpus issue upstream, decided case by case
   and recorded (WC-1, WC-2).
2. Compare the identifiers inklkunn cites (`MKUP-`, `META-`, `XFER-`, …) and the schema files; the
   conformance claim's version follows the suite's.

## Supply-chain gates at project events (RFC 010 D-4)

`cargo deny check` and `cargo audit` run on every push and pull request (`ci.yml`), on a weekly schedule and
on demand (`supply-chain.yml`), in the release gate (`release.yml`), at every re-baseline, and at every RFC
disposition checkpoint by the architect against a freshly fetched advisory database. Red becomes an
issue-register entry the same day and is triaged on the ladder in `dependency-policy.md`.
