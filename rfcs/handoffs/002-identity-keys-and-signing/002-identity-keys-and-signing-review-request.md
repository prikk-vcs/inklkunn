# Review request — handoff 002 (identity, keys and signing)

| | |
|---|---|
| Handoff | `identity-keys-and-signing-handoff-v1.md` (RFC 002, accepted 2026-10-27), tasks T1 … T8, built together with handoff 003's T1 … T3 (their request is `../003-read-path-and-rendering/003-read-path-and-rendering-review-request.md`) |
| Implementer | the dev team (the mid-capability model), session of 2026-10-28; T2, T3 of handoff 003 and the independent readings below were done by sub-agents under the same rules |
| Reviewer | the architect, against RFC 002, the handoff and the owner's two principles: *finally clean, safe and secure, robust and sophisticated design*; *APIs and UI/UX that cannot confuse or mislead* |
| Submitted | 2026-10-28 |
| State | ten commits for this handoff on local `main` (the seven tasks, T5 in two, then two that carry the fixes from two independent readings; and `15ded08`, a one-number correction to the changelog), none pushed (the architect's own commits `d06b06e` and `8e0a08e` sit among them; neither side has pushed the other's work) |

## The one thing to read first

**RFC 002 D-8 and threat model C-1d cannot both hold on prikk 0.54.1** (ledger **PK-34**, measured). They say a
rotation adopts the new maintainer key, *removes the old one from the repository's trust store*, and "the
repository still verifies". After `prikk trust maintainer remove --key-id <old>`, `prikk verify` exits 1:
`publication-trust [PRIKK-TRUST-PUBLICATION-UNTRUSTED]: … has no trusted MAINTAINER signature` for the block, the
ref state and the ref update that only the removed key signed. Every wiki would show *Verification failed* the day
its keys are rotated for a compromise, and keeping the old key trusted instead leaves a leaked seed able to seal
changes that verify.

What I built, so nothing was left silently wrong (`docs/KEYS.md` is the runbook):

- `inklkunn wiki keys rotate` **keeps the old maintainer key trusted by default**; the history keeps verifying; the
  service no longer holds the old seed. A test says out loud what that costs (a copy of the old seed can still
  seal and verify).
- `--retire-old --accept-unverifiable-history` removes it from trust. Two flags, because the second names the cost.
  A canary test (`signer/lifecycle/tests.rs`, and the same through the command line in `keys/tests.rs`) **fails the
  day prikk can verify a retired key's history**, so the ledger row and the runbook cannot go stale unnoticed.
  After a retirement every later acceptance still seals, but cannot confirm it: it returns
  `SealedUnconfirmed { why: VerifyFailed }` (RFC 004's orchestrator must read that as *sealed, needs a look*, not
  as a failure to retry), and the runbook says so.
- If the retirement itself fails after the keys rotated, the command exits 1 saying the old key is **still
  trusted** and prints the `prikk trust maintainer remove` command to finish by hand; the wiki record follows the
  key store either way.
- I did not decide the design question; I bounded it. **Decision requested** (open question 1).

## What landed (hash → task)

| Commit | Task |
|---|---|
| `3913137`, `bc60dc6` | **T5** the patch message contract: composer and strict canonical parser (`Message`, `Kind`, `Summary`, `Actor`, trailers once each in order), the four kinds, round trips under proptest, a real prikk returns a sealed change's message byte for byte |
| `fb95846` | **T2** `authorize()` (a pure `decide` over a snapshot the `Authorizer` loads itself), the role ladder, token scopes intersected with roles, existence-neutral denial, audited consequential decisions, the route table, and two gates (route registration; privileged principals) |
| `fdcee3f` | **T3** the key store: Argon2id master key from the operator's secret file, XChaCha20-Poly1305 per record with the wiki, key ids, generation and times as associated data, the 0700 tmpfs seed directory for one call, zeroization, the start-up sweep, a gate that no public item of the signer mentions a seed, a secret or a key directory (+22 lock entries) |
| `348f2de` | **T4** `Accept` and `create_view` over the sandboxed write verbs, bound to a recorded act: guard before any write, restore on a failure before `commit`, a typed outcome after it (it said `Incomplete` here; `Queued`, `SealedUnconfirmed` and `WorktreeDirty` since the second reading), first-change-only bootstrap; stand-in-prikk fault injection and a SIGKILL test. **The costed table of write-verb memory the handoff's binding note asks for is not done** (see the limitations) |
| `3f2712b` | **T6** the append-only audit trail (SQLite triggers abort UPDATE and DELETE; JSON Lines export) and the read-only reconciliation of the history with the acceptance and system-act records |
| `d7a8ab3` | **T1** principals and credentials: `inklkunn-auth`, store schema 4 (accounts, linked identities, sessions, tokens); DR-9 adopted (`jsonwebtoken` 11.1.0, `aws_lc_rs`; 102 → 129 lock entries; `rsa`, `crypto-bigint` and `ring` absent; no advisory ignored) |
| `0a4027e` | **T7** key lifecycle: `inklkunn wiki keys status|create|rotate`, the binary's configuration, `docs/KEYS.md`, ledger PK-34 and PK-35 |
| `b6e9ada` | the **first independent reading** (T1, `inklkunn-auth`): throttles counted before they are judged, credentials revoked on recovery, an authenticator that owns its OpenID Connect flow, and the rest of the table below |
| `60b1591` | the **second independent reading** (T2 – T7): a reconciliation that reads the whole history (ledger PK-36), acceptance outcomes that say what state the repository is in, one acceptance at a time per wiki, `inklkunn audit export`, gates that are a lexer and not a spelling list; `EXPECTED_SKIPS` 43 → 55 |

T8 (the observations for threat model v0.2) is the last section.

## Deviations from the handoff or the RFC text, and why

1. **`Accept` and the message grammar live in `inklkunn-signer`, not `inklkunn-core`** (RFC 001 D-9, RFC 002 D-5
   say core; `inklkunn_core::accept` and `::message` re-export them). The signer must name the types to enforce
   the guard, and the layering gate lets only core depend on the signer; the other direction would have made the
   signer depend on core. Consequence: a new edge signer → store (ledger row; `+0` lock entries).
2. **`DisplayName` now refuses U+2028 and U+2029** (the store's own error text always said "no line break of any
   kind"; a property test over arbitrary names found it accepted them). A reader that splits lines on those would
   see a forged trailer inside a signed message. Listed as an output change in the CHANGELOG.
3. **Rotated key ids are `inklkunn-<wiki id>-<role>-<generation>-<8 hex of the public key>`**, not `…-maintainer-2`.
   prikk binds an id to one key for good (PK-35, confirmed), so a retry after a half-failed rotation must not
   collide with the first attempt's adopted key; the key-derived suffix makes a retry's ids new.
4. **The old maintainer key stays trusted by default** (the section above).
5. **Store schema numbering.** Version 2 holds the role grants, 3 the audit trail, 4 the identity tables; handoff
   003 T3's "schema version 2" is therefore version 5 (it says so in its own request).
6. **`Visibility::Restricted` is RFC 002's `private`** (the store's enum predates the RFC's wording; one name, one
   meaning in code, the word `private` in the user-facing catalog).
7. **`SignerConfig` is `#[non_exhaustive]` with `SignerConfig::new`**: its two test-only fields exist under a cargo
   feature that a workspace build may or may not unify in, and must never change what a caller writes.
8. **"Re-authentication" for the command line** is the operator's possession of the secret file (the key store does
   not open without it) plus the wiki's slug typed (`--confirm`); the web administration surface will ask for the
   password again. If D-8 meant something stronger for the CLI, say so.
9. **Accounts have a sign-in name** (`LoginName`: 3–64 characters of `a-z 0-9 . _ -`, case-folded), a concept the RFC
   does not name: the display name is free text in any script and cannot be a login, the id is a UUID. It is not
   shown anywhere a reader sees.
10. **The OpenID Connect first-sign-in linking option** ("linked … administratively or, where the wiki's policy
    says, the first sign-in") is **not built**: the policy it depends on is RFC 006's. Linking is administrative
    only, and an identity nobody linked signs nobody in.

## Open questions and decisions requested

1. **PK-34 (above).** Options: (a) accept the default-keep and the explicit retire as built, with the runbook
   honest about the cost; (b) amend D-8 and C-1d to say what prikk allows today; (c) put it in the next bundled
   letter to prikk (the owner bears the cost of each): *a retired maintainer key keeps verifying what it sealed
   while it was trusted*, or a countersign command. I recommend (a) now, (c) at the next letter, and (b) in the
   threat model's next revision.
2. **The confined `curl` fetcher has no home.** OpenID Connect needs outbound HTTPS through the egress guard
   (C-8). The spawn gate lets only the prikk driver and the signer spawn a process, and `Mounts` is the
   signer's. I built the flow against an `HttpsFetcher` seam in `inklkunn-auth` (tested with a map, and the
   verifier with real ES256 signatures), so **OIDC works in tests and cannot be switched on in a deployment
   yet.** Recommendation: a small lower-layer crate for confined outbound fetches (its own allow-list entry in
   the spawn gate, `curl` in bubblewrap with no repository, the guard's pinned address), implementing the seam.
   That is an architecture change; yours to decide.
3. **A lost key store cannot be recreated under the old ids** (PK-35): `create` says so rather than hiding it.
   RFC 002 has no recovery path for "the repository survived, the key records did not". Recommendation: a
   `recover` operation that provisions generation-N keys under fresh ids, requiring the operator's confirmation.
4. **Throttle numbers** (5 failures per sign-in name, 30 per client address, both locking 15 minutes; sessions seven
   days; tokens at most a year and a day; passwords ten characters, no composition rule) are my choices, not the
   RFC's. They are constants in `throttle.rs`, `session.rs`, `token.rs` and `hashing.rs`.
5. **Two writers on one wiki.** Inside one process the signer now takes a lock per wiki around an acceptance, a
   view creation, a key provision and a rotation (a test that fails without it: before it, two acceptances sealed
   one change's message over the other's file). **Between processes nothing excludes them** (the command line
   against a running server): that is RFC 004's lease, and an `flock` in the signer would be the stopgap if you
   want one before it. Until then the runbook says to rotate when the wiki is quiet.
6. **Reconciliation is a function, not a service.** Nothing runs it "after every seal and on a schedule" yet, and
   its findings reach the audit trail only when a caller records them. Scheduling and the administration view are
   RFC 004 / RFC 003 work; say if you want it earlier.

## Known limitations (all stated where they bite)

- No route uses `inklkunn-auth` yet (handoff 003's non-change scope); `Authenticator` is blocking by design (Argon2id
  and a gate wait) and must be called from `spawn_blocking`.
- Throttle state and pending OIDC logins are per process (threat model RR-7); a restart forgets them.
- A `SIGKILL` in the middle of a signing call strands its decrypted seeds on the tmpfs until the next start sweeps
  directories older than five minutes (SECURITY.md says so; see T8).
- The operator secret file's mode is not enforced (Docker secrets are mode 0444); passwords and request bodies are
  not zeroized in memory.
- OIDC signatures are tested end to end with ES256 only (the other three algorithms go through the same library
  call); `at_hash` and `c_hash` are not checked (the ID token arrives directly from the token endpoint).
- The constant-work timing test is statistical (best of seven runs, within 2× plus 5 ms); the structural test (one
  verification per attempt, whatever the credential) is the guarantee, and is exact.
- The verify-verdict follow-on (`../001-foundations/verify-verdict-honesty-handoff-v1.md`, IS-7) was **not started**:
  it is outside this assignment, and it must land before handoff 003's T4.
- The M0 driver's `is_ref_name` checks `//` only in the tail (`heads//x` passes it; prikk refuses it anyway). The
  signer's own check is whole-string.
- **The write ceiling is still the fixed 1 GiB** (`WRITE_CEILING`). The handoff's binding note asks for the write
  verbs to be measured and the ceiling sized from the repository; I did not do it. The second reading showed why it
  matters: `prlimit --data=1073741824 prikk verify` aborts (exit 134, an allocation of 629 MB) on a store holding one
  300 MiB attachment, and the signer's `outcome()` has no ceiling case, so such an abort would read as `Refused`, or,
  after a seal, as `SealedUnconfirmed`. **Open: measure as spike A did, derive the ceiling, map a ceiling death to
  `Unavailable`.** Until then a wiki with a very large attachment cannot be accepted into.
- **Both seeds are mounted and exported to every write verb** (`commit` needs only the author seed, `seal` only the
  maintainer seed, `mv`, `switch`, `init` and `trust` neither). `commit` parses attacker-influenced paths and content
  with the repository writable, so a parser fault could read the maintainer seed. The exposure is certain, the fault
  chain is not. Recommended: one seed directory per role, mounted per verb. Not built (it changes the mounts, which
  are yours to approve).
- **Seed directories are swept by age, not by liveness.** A call touches its directory so a long acceptance is not
  swept (tested), the base must be owned by the service's user and not writable by others, and a failed removal is
  `SeedsNotRemoved`. Not built: an `flock` held for the call (so any start can sweep every unlocked directory) and
  `O_NOFOLLOW` on the removal. A `SIGKILL`ed call's seeds survive a restart for up to five minutes, which is the
  ordinary supervisor case; `serve` does not open the signer, so only the command line sweeps today.
- **`Principal::session` and `Principal::token` are constructible by any crate** that can name `inklkunn-core`
  (the gate watches web sources for them; the type does not forbid it). Recommended: a witness only
  `inklkunn-auth` can mint. The signer is likewise nameable from web through core's re-exports; a gate now fails a
  surface that names it, but the type system does not. `rotate_keys`, `create_keys` and `create_repository` take no
  authority witness.
- **`Authenticator`'s API still takes `now: i64`** (and `lifetime_secs: i64` beside it) rather than a newtype; an
  account has no cap on live sessions (tokens are capped at a hundred); `role_grants` has no foreign key to
  `accounts` (a V2 table, V4 added the accounts); the audit trail grows with every denied consequential attempt,
  anonymous ones included (sample or aggregate them; a failed audit write for a denial is dropped).
- **`ExpectedMessage` binds the message but not the files**: nothing ties a digest of the prepared files to the
  committed patch, and the summary line the signer signs is whatever core composed. Recommended in the second
  reading; not built.
- **Web-layer notes the first reading left open:** the session cookie is `SameSite=Strict`, which a browser may
  withhold on the redirect that follows an identity provider's cross-site callback (needs an interstitial; browser
  behaviour not checked); a refused `accept` that first switched views leaves the worktree on the other view
  (documented on `Refused`); `Signer::open` creates an empty key store for a mistyped key directory (the command
  line guards against it; the signer does not).
- **There is no request-level route-guard harness yet.** `inklkunn-core::routes` says what is and is not
  tested; D-2's "a planted unguarded route fails the build" holds for registration outside `routes.rs` only.
  The harness arrives with the first guarded route (handoff 003 T4).

## Dependency deltas (RFC 010 D-6; each measured before its commit, rows in `docs/dependency-policy.md`)

| Change | Lock entries | `cargo deny check` | `cargo audit` | Other |
|---|---|---|---|---|
| `argon2`, `chacha20poly1305`, `zeroize` (T3) | 80 → 102 (+22) | 0 | 0 | Argon2id 64 MiB, 3 passes: 56 ms release, about 1 s debug |
| `jsonwebtoken` 11.1.0 `aws_lc_rs`, `sha2` (T1; DR-9) | 102 → 129 (+27) | 0 | 0 | `rsa`, `crypto-bigint`, `ring`, `p256`, `p384` absent (counted 0 in the lock); build +5.1 s wall; probe binary +3.4 MB (+3.1 MB stripped); `cargo +1.88.0 check` passes; **aarch64 not built here** — the first CI run confirms |
| `axum`, `tokio` 1.54.1, `tower-service` (handoff 003 T1) | 129 → 155 (+26) | 0 | 0 | past RUSTSEC-2026-0386 |
| `askama`, `ammonia`, `html5ever` dev (handoff 003 T2/T3) | 155 → 209 (+54) | 0 | 0 | new licences Unicode-3.0 and MPL-2.0, both already allowed |
| signer → store, binary → core/store/web (workspace-internal edges) | +0 | 0 | 0 | |

## Gates: commands and exit codes

One run for both handoffs, on local `main` at **`b0090ec`** (it contains handoff 003's T1 – T3 as well); the commands,
the exit codes and the notes on what was not run are in `../003-read-path-and-rendering/003-read-path-and-rendering-review-request.md`
("Gates"). In short, every one exited 0: `cargo fmt --check`; `cargo clippy --workspace --all-targets --locked -D warnings`
on the current toolchain and on 1.88.0; `cargo test --workspace --locked` with prikk on `PATH` (**676 passed, 0
failed**) and shimmed off (**676 passed, 0 failed, 57 self-skips = `EXPECTED_SKIPS`**); rustdoc with `-D warnings`;
`cargo deny check`; `cargo audit` (209 crates).

For this handoff's own tests: the real-prikk ones (the key store with a real signer, `accept` against a real prikk
including the `SIGKILL` test, the reconciliation over a history longer than its first read, key rotation and
retirement with the PK-34 canary) run for real in the first run, and in the second each prints
`skipping: prikk not on PATH` and returns (it counts as passed, so the skip count is what proves it: 57 lines, which
is every prikk-dependent test in both handoffs and what `EXPECTED_SKIPS` is set to). **Not run:** CI itself, aarch64,
the release rehearsal.

## Independent reading

*An author's review is not a review* (the STATUS lesson), so before submitting I had two readers who had not seen
my work read it adversarially. Each was a fresh sub-agent given the code and the specifications and **not** this
request or my conclusions; each ran experiments in a throwaway copy of the workspace and applied planted mutations
to see which tests turn red. Reader A read T1 (`inklkunn-auth` and the identity half of the store, commit `d7a8ab3`):
12 findings, 28 mutations of which 24 were caught. Reader B read T2 – T7: 14 findings, and five mutants that
survived. The tables give each finding as the reader ranked it, and what I did. **"Fixed" means a test fails
without the fix; "declared" means it is in the limitations above and not built.** Both readers' reports are in
this session's transcript; the commits are `b6e9ada` and `60b1591`.

### Reader A — T1 (principals and credentials)

| # | As ranked | Finding | What I did |
|---|---|---|---|
| 1 | HIGH | Both throttles were check-then-act: 60 simultaneous wrong guesses at one name were all judged (policy 5); 120 names from one address were all judged (policy 30); a correct password queued before the lock still signed in | **Fixed.** `Throttle::begin` checks the lock and counts the attempt in one step; a right or unjudged attempt gives its count back. Burst tests: 60 threads → 5 judged, 55 locked; 120 names from one address → 30 judged |
| 2 | MEDIUM | The constant-work timing test could not fail: its last samples were instant `Locked` answers | **Fixed.** Distinct names and addresses per sample, medians, and a structural check that the dummy hash costs what a real one does |
| 3 | MEDIUM | A stored hash that did not parse answered in 13 µs against 42 ms for an unknown name, so timing showed that exact account exists | **Fixed.** An unusable stored hash costs a dummy verification, and the store refuses a hash that is not an Argon2id string |
| 4 | MEDIUM | "Cannot forge" was false at the type level: `Sessions` and `Tokens` were public (a session for any account without a password); `finish_login` took two identical strings; `ClientAddress::from_ip` bypassed the proxy rule; CSRF was a separate skippable call; no change-password | **Fixed in part.** `Sessions`, `Tokens`, the throttles and the gate are private to the crate; a `ClientAddress` comes only from the trusted-proxy rule; a write goes through `principal_for_write`, which cannot skip the CSRF token; the authenticator reads the state cookie from the `Cookie` header itself; `change_password(old, new)`. **Declared:** a time newtype instead of `now: i64`; `Principal::session` / `::token` still constructible by any crate |
| 5 | MED-LOW | A password change, a reset or a disabling left API tokens alive | **Fixed.** All three revoke every token as well as every session; enabling does not bring them back |
| 6 | MED-LOW | Lock events were invisible; OpenID Connect refusals carried no reason; an unreadable `X-Forwarded-For` from a trusted proxy was silently attributed to the proxy; a store failure after a right password told a guesser | **Fixed in part.** Locks reach the audit trail; every ID-token refusal carries an `InvalidReason` for the log; `ClientAddress::fell_back` says a trusted proxy's header was missing or unreadable. The last point is a decision, not a change: a person who typed the right password is told about an outage, and the doc on `sign_in` says what that costs |
| 7 | LOW-MED | (a) `SameSite=Strict` may hide the cookie after the provider's cross-site callback. (b) Nothing limited `begin_login`: 10,000 calls evicted every in-flight login, and a down provider was asked again on every request | (b) **Fixed:** starting a login spends the client address's budget, and a provider that is down is not asked again for five seconds. (a) **Declared:** not checked in a browser |
| 8 | LOW | `Debug` of `FetchRequest`, `BeginLogin` and `VerifiedIdentity` printed the client secret (the Basic header), the state-cookie secret and the e-mail address | **Fixed.** Redacted, and the no-secret-in-`Debug` test covers all three |
| 9 | LOW | The two stores disagreed (error precedence, `tokens_of` order); V4 left `role_grants` without a foreign key to the new `accounts`; four tests overclaimed (surviving mutants M19, M24, M27, M28) | **Fixed in part.** The stores agree on ordering and on the order of their checks; the busy-gate and oversized-password tests now assert what their names say. **Declared:** the `role_grants` foreign key; **M19 still survives** (removing `PRAGMA foreign_keys=ON` from the store's connection: the pre-checks hide it, and the schema test sets the pragma itself), as does M27 (a redundant disabled-account check) |
| 10 | LOW | A JWK marked `use: enc` verified signatures; `iat` was optional and unbounded (a 95-year-old token was accepted); RS384 and RS512 were accepted but undocumented | **Fixed.** Such keys are refused; `iat` is required and at most fifteen minutes old; the documentation lists the six algorithms |
| 11 | LOW | `session_ttl_secs: 0` was accepted (every session dead on arrival); `with_cost_unchecked` claimed an error it never gave | **Fixed.** Configuration that cannot work is refused at start |
| 12 | LOW | No cap on sessions or tokens per account | **Fixed in part.** A hundred live tokens per account. **Declared:** sessions uncapped |

Reader A also noted what is inherent (lock-out abuse is documented; throttle state is per process, RR-7), a
deployment hazard (behind an undeclared reverse proxy with the default `ProxyPolicy::none()`, thirty bad attempts by
anyone lock everyone: nothing warns; **I did not add a warning**), and that no route uses the crate yet and there is
no production `HttpsFetcher` (open question 2). It checked closely and found sound: Argon2id parameters and the
ceiling on stored ones, one verification per sign-in whatever the credential, the gate, session ids and cookies,
CSRF comparison, token hashing and scope intersection, the proxy walk, PKCE, state binding, the algorithm
confusions (`none`, `HS*`), parameterised SQL, and a lock free of `rsa`, `crypto-bigint` and `ring`.

### Reader B — T2 to T7 (authorize, key store, accept, audit and reconciliation, key lifecycle)

| # | As ranked | Finding | What I did |
|---|---|---|---|
| 1 | HIGH | The reconciliation examined only the newest 500 blocks: it paged by block id, which real prikk 0.54.1 answers with an empty list and exit 0 (**ledger PK-36**), so past 500 changes every older accepted record became a `MissingBlock` finding, and the fake in the tests paged "like the real one" | **Fixed.** Each view is read whole with one `log --limit N` (four times larger while it comes back full, a typed `TooLarge` at a stated bound); the driver refuses a block point for history; a standing finding is reported once; a **real-prikk** test with a history longer than the first read; the fake no longer pretends block paging works |
| 2 | HIGH | No per-wiki mutual exclusion: two concurrent acceptances sealed one's message over the other's unaccepted file (7 of 12 runs) | **Fixed in process:** a lock per wiki around accept, view creation, provisioning, sealing and rotation, with a test that fails without it. **Declared:** between processes (RFC 004's lease), and a digest of the prepared files bound into `ExpectedMessage` |
| 3 | HIGH-MED | `AcceptError` could not say what state the repository was in (`Incomplete` meant both "queued, not sealed" and "sealed, could not confirm"; `Unavailable` was documented as "as it was found" and was not) | **Fixed.** `Queued { step }` (one patch queued, nothing sealed), `SealedUnconfirmed { sealed, why }` (sealed; do not retry), `WorktreeDirty` (a failure that could not be undone), beside `Refused`, `Unavailable`, `SeedsNotRemoved`. Docs say what each means |
| 4 | MEDIUM | After `rotate --retire-old` every later `accept` returned `Incomplete` though it had sealed; the write ceiling is still the fixed 1 GiB | **Retire-old fixed as far as it can be:** every acceptance is `SealedUnconfirmed { VerifyFailed }`, pinned by a test, with `docs/KEYS.md` saying so (the cause is PK-34). **The ceiling is declared, not done** (limitations) |
| 5 | MEDIUM | The key commands audited a partial success as a failure, left the wiki record stale, could skip the audit on success, and wrote it after the act | **Fixed.** A retirement that fails is a `Rotation` whose `old_key` says `RetireFailed`; the command exits 1 with "STILL TRUSTED" and the `prikk trust maintainer remove` command to finish by hand; the wiki record follows the key store; an act is audited *before* (the command refuses to begin one it cannot record) and *after* |
| 6 | MEDIUM | A failed wiki creation left its sealed key record while the error said "nothing was left on disk" | **Fixed.** `RepositoryBootstrap::discard_unfinished` removes the record; `Cleanup::KeysLeftBehind` tells the truth when it cannot; a test with the real signer |
| 7 | MEDIUM | Both seeds are mounted and exported to every write verb | **Declared** (limitations): per-verb mounts are yours to approve |
| 8 | MEDIUM | (a) Decrypted seeds outlive a call longer than documented (five minutes, a sweep only in `Signer::open`), and a start can sweep a live call; (b) the seed base is not vetted | **Fixed in part.** A live call touches its directory (a long acceptance is not swept); the base must be owned by the service's user and not writable by others; SECURITY.md says "up to five minutes, until a start". **Declared:** an `flock` for liveness, `O_NOFOLLOW` on removal |
| 9 | MEDIUM | prikk's stderr carried the seed directory's path into error strings | **Fixed.** `scrub()` removes the directory's path (and the hint) before words become errors; a test with a stand-in that prints it |
| 10 | MEDIUM | The boundary gates were spelling lints (`Principal as P`, `Principal::*`, `<Principal>::`, `Router::route(r, …)`, any file named `routes.rs`), the web tier could name the signer, and `Decision` was not `#[must_use]` | **Fixed in part.** The matcher is a small lexer (so `impl X` or `dyn X` cannot hide `X`); the spellings above are caught, each with a planted case; the registry is matched by its exact path; **a surface may not name the signer** (new gate); `#[must_use]` on `Decision`. **Declared:** the type system still lets web name the signer through core's re-exports, and `Principal::Account` is constructible |
| 11 | MEDIUM | The route harness could not fail a planted unguarded route; the doc claimed more | **Doc corrected;** the request-level harness arrives with the first guarded route (limitations) |
| 12 | MEDIUM | `INSERT OR REPLACE INTO audit_log` rewrote row 2 on a raw connection (triggers do not fire with `recursive_triggers` off), against the stated "even through SQL"; anonymous denials grow the table without bound | **Replace fixed:** a `BEFORE INSERT` trigger aborts when the position exists, with a test for `INSERT OR REPLACE` and `REPLACE`, and the doc now says the triggers guard code paths and not someone with the file (there is no hash chain). **Declared:** sampling anonymous denials, and a dropped audit write for a denial |
| 13 | LOW | The docs promised tools that did not exist ("run the reconciliation", "export the audit trail") | **Fixed in part.** `inklkunn audit export` writes the trail as JSON Lines (it needs only `INKLKUNN_DATA_DIR`). The reconciliation has no command: SECURITY.md and `docs/KEYS.md` now say it is a function today |
| 14 | LOW | Smaller: seeds read into plain `Vec<u8>` temporaries; a refused accept that first switched views leaves the other view; the "old key kept" output omitted what KEYS.md says; `ExpectedMessage` excludes the summary; `KeyStore::open` makes an empty store for a mistyped directory; stale "arrive later" comments and a module-wide `allow(dead_code)` | **Fixed:** `Zeroizing`; the stale comments and the blanket `allow(dead_code)` (the dead cleanup it hid was finding 6). **Declared:** the others |

Reader B's *tests that cannot fail*: five surviving mutants. Four have a test now: seeds decrypted before the guard
runs (a test that checks the directory *while* the call is refused, not after `Drop`), `restore()` reporting
"clean" (`WorktreeDirty`), `SeedsNotRemoved` swallowed after a successful accept, and the `ours` read-back check
dropped (`NotOurs`). **Still surviving:** loosening the 1024-byte path bound by 100 (the constant is right: real
prikk accepts 1024 and refuses 1025; the test uses far-out lengths). The tests that passed for the wrong reason
were `a_long_history_is_read_in_full…` (finding 1), the audit test (12), the error-text test (9) and the route
table (11): the first three are fixed, the last is declared.

Reader B checked closely and found sound: the key store's sealing (nonce per seal, associated data over wiki, ids,
generation and times, a 15-case tamper matrix, exclusive create by hard link, 0600 in 0700, no logging at all in
product code), the message grammar (strict, total, canonical; display-name injection tested), `repo_path`/`Plan`/
`Applied` (no traversal, symlink, dotfile or reserved-name bypass found), the order of the guard before any prikk
call, the post-`commit` kill states, `decide` (default-deny over every role, scope, visibility and action; a
restricted wiki and an absent one give the same decision, lookup count and audit content), the command line's
refusals before the key store is opened, and that product builds enable neither `bootstrap` nor `test-knobs`.

**What neither reader could check:** RFC 004 (orchestration, the lease, recovery) does not exist yet, so
cross-process races were reasoned about only; failures inside bubblewrap were not exercised (the stand-in tests run
unconfined by design); nothing was run on aarch64; the browser behaviour of `SameSite=Strict` after a
cross-site callback; an actual identity provider, and any signature algorithm but ES256 with the test keys.

**What I did not do about the readings, in one list:** the write-verb memory table and ceiling; per-verb seed
mounts; cross-process exclusion; a witness for `Principal::session` / `::token` and for the key operations; a time
newtype; a session cap and sampled anonymous denials; a foreign key on `role_grants`; a digest of the prepared
files in `ExpectedMessage`; a warning for the undeclared-proxy deployment; a request-level route-guard harness.

## T8 — inputs for the architect's threat model v0.2

**Seed-file lifetime** (T3, T4, T7). A seed exists in the clear only as `<seed dir>/inklkunn-seeds-<32 random hex>/
{author,maintainer}.seed`, mode 0600 in a 0700 directory, on a tmpfs the signer verified through `/proc/self/mountinfo`
at start (it refuses a disk). It is made for one call (`with_seeds`), overwritten with zeros and unlinked when the
call returns, and removed by a `Drop` backstop on a panic; a removal that fails is `SeedsNotRemoved`, never
swallowed, and never replaces the failure of the call that was running. Observed: **`SIGKILL` strands the directory**
(the killed-server test shows the queued patch and the stranded seeds); the next start removes directories older
than five minutes and reports how many (`leftover_call_directories_removed`). Window: from the kill until a start
that finds the directory older than five minutes, so a quick supervisor restart does not clear it (a live call
touches its directory, so a long acceptance is not swept as dead; `serve` does not open the signer, so only the
command line sweeps today). The seed base must be owned by the service's user and not writable by others. Ideas for
v0.2: a bubblewrap `--ro-bind-data` / fd-passed seed so the bytes never touch a filesystem (cost:
prikk wants a path; `/proc/self/fd/N` needs checking), and an operator alert on a non-zero sweep.

**What the sandbox sees.** A confined write call gets: `/usr` and prikk read-only, the repository **writable**, the
per-call key directory **read-only**, a cleared environment containing only `PATH=/usr/bin` and `PRIKK_{AUTHOR,MAINTAINER}_{SEED_FILE,KEY_ID}`
(paths and ids, never a seed), every namespace unshared (so no network), `--die-with-parent`, a `prlimit` data ceiling (`WRITE_CEILING`, a fixed 1 GiB, **not measured**: the costed table
the handoff asks for is not done, and `prlimit --data=1073741824 prikk verify` aborts on a store with one 300 MiB
attachment) and a two-minute deadline per prikk call (one acceptance makes at least seven calls). **Both seeds are
mounted for every write verb**, whichever one it needs. The seed **paths** are visible to
the confined prikk and to same-uid processes through `/proc/<pid>/environ` for the call's duration; the bytes are
readable by prikk itself, which is the point. The key store's directory is never mounted.

**Post-`commit` failure states** (T4; ledger PK-31, PK-32). A failure after `commit` leaves exactly one queued patch
(`status` shows `queue.count` 1 and its `message`; `log` shows no new block; the worktree holds the written files; no
lock file remains when the killer was the stand-in) and `seal --allow-no-audit --ref <view>` under the maintainer key
alone then completes it and `verify` passes. A kill *inside* prikk's own `seal` can additionally leave
`active.lock`/a ref lock, which `prikk unlock` lists: RFC 004's kill matrix. A second `commit` while one is queued is
accepted by prikk and `seal` then seals both as one block, so the signer **refuses to start** while `queue.count` is
nonzero. The typed outcomes (`AcceptError`): `Queued { step, detail }` (committed, not sealed: sealing it completes
the acceptance, retrying would commit twice), `SealedUnconfirmed { sealed, why, detail }` (sealed; the block was not
read back, is not the change written, or the repository did not verify: do not retry), `WorktreeDirty` (a failure
before `commit` that could not be undone), `SeedsNotRemoved { sealed }` (finished; the directory is the operator's to
remove). After `rotate --retire-old` every acceptance is `SealedUnconfirmed { why: VerifyFailed }` (PK-34).

**Two further observations.** Removing a maintainer key from trust makes its history unverifiable (PK-34, above). And
a `SIGKILL` during `accept` *before* `commit` would leave written-but-uncommitted files; the next acceptance's
"nothing unsealed and a clean worktree" precondition refuses to proceed over them, which is the intended stop and
RFC 004's recovery to clear. That case is by design and by the precondition's own tests; **no test kills the
server at that instant**, so it is not observed.
