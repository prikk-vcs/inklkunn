# Upstream and supply-chain maintenance, November 2026 — Developer Handoff v1 (RFC 010)

| | |
|---|---|
| Document | Three things on `main` that changed outside inklkunn, found at the review of 2026-11-13: an advisory against the bundled SQLite (M1), a test that assumed one curl build's memory footprint (M2), and prikk 0.56.0 and 0.57.0, both released (M3). |
| Status | Written by the architect 2026-11-13. **Assignable now**, beside handoff 005's K15 – K23. M1 first: CI's supply-chain job is red on `main` until it lands. |
| Decision | M1 by the owner's standing rule (`docs/dependency-policy.md`: **no standing ignore**): fix by upgrading. Reachability is low and is **not** the reason to wait (below). planeter is making the same change for the same advisory (its `advisory-rusqlite-handoff-v1`); the two projects' minimum Rust versions move in step. |
| Basis | RUSTSEC-2026-0459 (`libsqlite3-sys` < 0.39.0, bundled SQLite < 3.54.0: a crafted database and `-wal` pair with different page sizes can return uninitialized page-cache memory; local; CVSS 4.0 medium); `rusqlite` 0.41 / 0.42 release notes; prikk's `CHANGELOG.md` for 0.56.0 (2026-11-08) and 0.57.0 (2026-11-12); `docs/dependency-policy.md` rules 1 and 3; ledger PK-21, PK-26, PK-36, PK-39, PK-43. |
| Handback | One review request, `010-maintenance-2026-11-review-request.md`, with every gate, the prikk version of each with-prikk run, and the advisory database's HEAD. |

## M1 — RUSTSEC-2026-0459: upgrade `rusqlite`, no ignore

1. **Measure first, in a scratch worktree** (dependency policy rule 1), for `rusqlite` 0.41 and the newest release (0.42 today):
   the lock's crate count before and after, deny, audit (a fresh advisory database), new licenses, **the bundled SQLite version
   (≥ 3.54.0) with how you know it**, the declared Rust version of `rusqlite` and `libsqlite3-sys`, and the source lines each needs.
2. **Adopt the newest that clears the advisory with the fewest source changes** (say why if not the newest). If it declares a
   higher Rust version than the workspace's 1.88.0, raise `rust-version` to **exactly** what it declares, nothing more, and
   update everything that names it (the `msrv` CI job, `pin-check`'s rule, `docs/`, the CHANGELOG). The compiler pin (1.99.1) does
   not change.
3. Code changes in `inklkunn-store` (and wherever `rusqlite` is used) only for API changes; **no test edited to pass** except for a
   signature that no longer exists (list each).
4. **Reachability, as evidence:** list every `Connection::open*` and the file it opens and who can write it. (Today one: the store's
   own database in the operator's data directory.) Report anything that opens a path an outsider can influence.
5. A row in `docs/dependency-policy.md` (the advisory, the versions, the bundled SQLite, the lock counts, no ignore) and one line
   under `[Unreleased]` in `CHANGELOG.md`.

**0.2.0's exposure (for the record):** a crafted pair needs write access to the data directory as the service's user, which
already gives the attacker the store itself. No patch release; 0.3.0 ships the fixed SQLite.

## M2 — the curl ceiling test

`inklkunn-fetch`'s `the_real_vector_runs_under_the_ceiling_and_the_real_curl_is_stopped_by_a_small_one` assumes curl cannot start
under 1 MiB; curl 8.23.0 can. Keep what the test is for (handoff 002's F3: the ceiling reaches the real curl) without assuming any
curl build's footprint: the real vector runs curl through the one shared `prlimit` wrapper with exactly the configured ceiling
(assert the command the wrapper runs and the limit the child reports, as the first half does), and a deterministic stand-in that
allocates past a small ceiling is stopped. No test may depend on the host's curl version.

## M3 — prikk 0.56.0 and 0.57.0: the re-baseline

**Measured by the architect on 2026-11-13** (tree `1ac27d8`): the whole suite with prikk **0.56.0** on `PATH` passes except M2's test
(2,618 passed, 1 failed; the same failure as on 0.55.0). 0.56.0's security fixes (`bundle import`, `archive import` and
`extract --claim` admitted forged signatures) are in commands inklkunn never runs (it runs `archive export` only), so the floor does
not have to move for them; the operator notes say an operator who imports by hand should run ≥ 0.56.0.

1. **0.56.0 — confirm and record.** Read the 0.56.0 output changes against every verb the driver and the signer run (`log --ref` now
   exits 1 for a block id — PK-36's fix; `branch --format json` and its signer field — PK-26; `verify` and `doctor` may report
   `unreached objects` as a warning) and say for each whether inklkunn's parsing is affected; a test where one is. The ledger rows
   are the architect's, from your table.
2. **0.57.0 — measure, then adopt.** Install it privately (never over the host's `prikk`), run the suite with it, and report
   per failing test why. Then: C11's late-seal check reads `sealed_by` (PK-39) where prikk provides it, and recovery offers
   discarding a sound queued patch as an operator-confirmed step (PK-43) — or say why either cannot be taken yet. **The floor moves
   only with a feature that needs it,** and then to exactly the release that has it; say which.
3. The with-prikk gate runs name the prikk version they used; until the floor moves, CI's floor stays 0.54.0.

## Non-change scope

No `deny.toml` or `.cargo/audit.toml` ignore. No other dependency update unless the new `rusqlite` forces it (list it). No schema,
file layout or WAL change. No tag, no release, no version change.
