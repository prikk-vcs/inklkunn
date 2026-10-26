# Identity, keys and signing — Developer Handoff v1 (RFC 002)

| | |
|---|---|
| Document | The implementation package for RFC 002: tasks in build order, the QA checklist from the RFC's acceptance criteria, and the definition of done and handback. A handoff never redefines its RFC. |
| Status | Inherits RFC 002's: **proposed — not startable** until the RFC is accepted **and** RFC 001's T1–T5 have landed and been approved. |
| Basis | RFC 002 (D-1 … D-10); RFC 001 (the crates, the gates, `Storage`, the seams); internal design `SGN-*`, `AZ-*`, `DM-3 … DM-5`, `ENF-1`, `ENF-2`; external design `CT-03`, `OP-07`; threat model `T-1`, `T-2`, `T-3`, `T-12`, `INV-1`, `INV-2`, `INV-4`, `INV-10`, §7; RFC 010 D-6 (every new crate is measured). |
| Audience | The dev team. |
| Scope | `inklkunn-auth`, `authorize()` in `inklkunn-core`, the key store and `Accept` in `inklkunn-signer`, the message composer and parser, the reconciliation, the audit log, the key-lifecycle commands. **Not** the proposal records or the acceptance orchestration (RFC 004: this handoff builds `Accept` against a test double of the acceptance record), **not** any web page beyond what tests need. |

## Before you start

- RFC 001's workspace is on `main` with its gates green; `Storage` works against prikk 0.54.x.
- The threat model's §7 names what must be tested from day one; T3, T4 and T6 below are those tests.
- Every new dependency follows RFC 010 D-6: measure, report the delta with deny and audit before the commit
  that adopts it. Candidates for this handoff: Argon2id (`argon2`), an AEAD and a KDF for the key store (the
  RustCrypto families planeter already carries are the first candidates), an OpenID Connect token verifier
  (`jsonwebtoken` with the RustCrypto backend, as planeter), and the outbound HTTPS question of IQ-3 (a
  confined `curl` through the egress guard first; an in-process client only if measured and justified).

## Task breakdown, in build order

### T1 — Principals and credentials (`inklkunn-auth`; RFC 002 D-1)
- **Change scope.** Accounts, identities (local, OpenID Connect), sessions (opaque ids, server-side,
  `Secure`/`HttpOnly`/`SameSite=Strict`), CSRF tokens, scoped hashed tokens, Argon2id with constant-work
  verification (a dummy hash made at start; every verification takes one permit from a bounded gate off the
  async workers), per-account and per-client-address throttles (the address from a declared trusted proxy
  only), OpenID Connect code + PKCE with one-shot state and nonce-bound tokens verified by `kid` against the
  provider's keys fetched through the egress guard.
- **Required tests.** Constant-work: unknown account and wrong password within noise of each other; throttle
  locks and refuses a correct password while locked; token scope cannot be exceeded; CSRF mismatch refused;
  OIDC flow with a stub provider, including replayed state and wrong nonce refused.

### T2 — `authorize()` (`inklkunn-core`; RFC 002 D-2)
- **Change scope.** The pure function over a snapshot of roles and visibility; the fixed M2 role set;
  scopes intersected with roles; existence-neutral denial; the audit hook for consequential decisions; the
  route-enumeration test harness (a registry of routes with an `Action` or a public reason, the scan that
  flags a registration bypassing it) ready for RFC 003's first routes.
- **Required tests.** A property test over every (action, resource) asserting default deny for a principal
  with no grant; visibility gates; a private wiki denies identically whether or not it exists; the harness
  fails a planted unguarded route.

### T3 — The key store (`inklkunn-signer`; RFC 002 D-3)
- **Change scope.** The private module: encrypted seed records under a key derived from the operator secret
  (an environment variable naming a file), AEAD with a fresh nonce per record, key ids per RFC 002 D-3; the
  private tmpfs-backed seed directory created for one call and removed after; a `Zeroize` on in-memory
  seeds.
- **Required tests.** Records unreadable without the secret; a seed file exists only during a call (inspect
  before, during, after); no secret in logs or error strings (a test greps the captured output); RFC 001's
  key-reachability gate still passes and still fails its planted violation.

### T4 — `Accept` and `create_view` (`inklkunn-signer`; RFC 002 D-4)
- **Change scope.** The implementation over the sandboxed write verbs: `branch switch`, file writes, `prikk
  mv`, `prikk commit -m`, `prikk seal --allow-no-audit`, `prikk verify`, with the seeds on the private mount
  and the environment variables prikk reads for key material set only for the child; the guard against the
  acceptance record's state and the message (D-5); typed `incomplete` after `commit`; `create_view` as a
  maintainer-signed `branch create`.
- **Non-change scope.** No acceptance orchestration, lease or intent journal (RFC 004); this task takes a
  test double of the acceptance record.
- **Required tests.** Against a real repository: a prepared change is committed and sealed and `prikk log`
  shows the message and `prikk verify` passes; a token whose record is not pending is refused before any
  write; a message naming another proposal is refused; a failure injected after `commit` returns
  `incomplete` and leaves exactly one queued patch (`prikk status`) for recovery; a server SIGKILL during
  `seal` leaves a state RFC 004's recovery can complete (record the observed state for RFC 004).

### T5 — The message contract (`inklkunn-core`; RFC 002 D-5)
- **Change scope.** Composer and parser for CT-03; validation (summary ≤ 120 characters, no newline; the
  trailer order; stable ids).
- **Required tests.** Round-trip every field; a property test over arbitrary display names (any script,
  parentheses, newlines refused); `prikk log --format json` of a sealed change shows the message verbatim.

### T6 — Reconciliation and the audit log (`inklkunn-core`, `inklkunn-store`; RFC 002 D-7, D-9)
- **Change scope.** The acceptance-record ↔ block reconciliation over `Storage::history` and the message
  parser; the append-only audit records with a sink trait and the JSON Lines export.
- **Required tests.** A planted unrecorded seal and a planted unsealed acceptance are both reported; the
  reconciliation changes nothing; the audit log refuses updates and deletes.

### T7 — Key lifecycle commands (`inklkunn` binary; RFC 002 D-8)
- **Change scope.** `inklkunn wiki keys create|rotate|status` (and the same through the administration
  seam for RFC 003's UI later): re-authentication, audit, the runbook in `docs/`.
- **Required tests.** Create then rotate on a real repository; the old maintainer key no longer verifies new
  seals; the repository verifies; `status` prints ids and dates and never a secret.

### T8 — Threat model inputs
- Record in the review request what T3/T4 observed about seed-file lifetime, the sandbox's view of the
  private mount, and the post-`commit` failure states, for the architect's threat model v0.2.

## QA checklist (from RFC 002's acceptance criteria)

- [ ] D-1 constant-work, throttles, token scopes, CSRF, OIDC refusals.
- [ ] D-2 default-deny property test; existence-neutral denial; the route harness fails a planted route.
- [ ] D-3 unreadable without the secret; seed lifetime; no secret in output; the reachability gate.
- [ ] D-4 refusals before any write; commit and seal confirmed by prikk; injected failure → `incomplete`.
- [ ] D-5 round trip; visible in `prikk log`.
- [ ] D-6 (with RFC 003's first page) an open proposal never renders *Verified*; retired-words test.
- [ ] D-7 both planted mismatches reported.
- [ ] D-8 create, rotate, status.
- [ ] Both test runs (prikk on PATH and shimmed off) recorded; dependency deltas measured per crate.

## Definition of done and handback

As RFC 001's handoff: tests exist and pass in both runs, gates green in CI, the review request
(`002-identity-keys-and-signing-review-request.md`) carries the summary, commits, every gate's command and
exit code, the dependency deltas with deny and audit, the T8 observations, deviations, limitations and open
questions. The architect reviews and answers with one of the six outcomes.
