# Identity, keys and signing — Corrections Handoff v1 (RFC 002, after the review of 2026-10-29)

| | |
|---|---|
| Document | The architect's correction requests on handoff 002 (T1 … T8), from the review of the dev team's request `.git-exclude/review-requests/002-identity-keys-and-signing-review-request.md`. The outcome was **Conditionally Approved**: the work stands; the items below are owed, each before the step it names. |
| Status | Inherits RFC 002's: accepted. **Assignable now.** C1 – C4 are **mandatory before handoff 003's T7** (the binary's first writes); C5 – C7 before RFC 004's T4; C8 – C10 before OpenID Connect is switched on in a deployment (M2). |
| Basis | RFC 002 D-1 … D-10 (as concretized on 2026-10-29: D-4, D-5 in the signer crate; D-8 pending the owner's DR-10); threat model T-1, C-1a … C-1c, C-9a, INV-2, INV-6; ledger PK-9, PK-20, PK-34, PK-35, PK-36; internal design SGN-6, AZ-1 (IQ-3 decided below). |
| Audience | The dev team. Read the two lenses in handoff 003 first. |
| Handback | One review request covering C1 – C10 (or one per group if they land at different times), filed in `.git-exclude/review-requests/`, with the usual gates, both test runs and a mutation table per item. |

## Mandatory before handoff 003's T7

**C1 — A memory ceiling is never a refusal or an unconfirmed seal (threat model C-9a).** `inklkunn-signer`'s
`outcome()` has no ceiling case, so a write verb killed by its `RLIMIT_DATA` reads as `Refused` before `commit` and
as `SealedUnconfirmed` after `seal`; the second reading showed `prikk verify` aborting under the fixed 1 GiB
`WRITE_CEILING` on a store with one 300 MiB attachment. (a) Map every ceiling death shape the driver already knows
(PK-9: abort, loader exit 127, signal, `128 + n`, bubblewrap's "Out of memory") to a typed `Unavailable {
CeilingExceeded }` **that still says where the repository is** (before `commit`: nothing written; after `commit`:
`Queued`; after `seal`: sealed, the read-back unconfirmed for want of memory — never "verify failed"). Reuse the
driver's classifier; do not fork it. (b) Measure the write verbs as spike A measured the reads (`commit`, `seal`,
`mv`, `branch switch`, `branch create`, `trust`, `init`, the post-seal `verify` and `log`; wiki-shaped stores of 4,
19, 76 and 212 MiB and single attachments of 64 and 200 MiB) and replace `WRITE_CEILING` with a costed rule per verb
carrying its prikk version, like `cost.rs`. (c) Tests: a stand-in for each death shape at each step; the real 300 MiB
attachment under the measured rule.

**C2 — Each write verb sees only the seed it signs with (least privilege; C-1a, C-1c).** Today both seeds are mounted
and both `PRIKK_*_SEED_FILE` variables exported to every write verb, so `commit` — which parses attacker-influenced
paths and content with the repository writable — can read the maintainer seed. Mount and export the author seed for
`commit` only, the maintainer seed for `seal`, `branch create` and the recovery `seal` only, and neither for `mv`,
`branch switch`, `init`, `trust`, `verify` and `log`. Prefer one seed directory per role, created only when a verb in
the call needs it. Test: a stand-in prikk that lists what it can see, per verb.

**C3 — One writer per wiki across processes (CON-1, INT-2).** The per-wiki lock is in-process; the command line and a
running server can still write one wiki at once. Take an exclusive `flock` on a per-wiki lock file **in inklkunn's
data directory, never inside `.prikk/`** (INT-10) for every act that writes the repository: acceptance, view creation,
key provisioning and rotation, `archive export`, the checkpoint refresh. Fail fast with the wiki named and the holder's
process id when another process holds it (a *busy* answer, never a wait without bound). RFC 004's lease is built on
this lock (its handoff is updated to say so). Test: two processes, one holding the lock, the other refused by name;
the lock released when its holder is killed.

**C4 — `Signer::open` never creates a key store it was not asked to create.** A mistyped key directory today yields a
new, empty store. Opening refuses a missing store with the path named; creation is a separate, explicit call that
`inklkunn wiki keys create` and *create wiki* make. Test: open on a missing directory is refused and creates nothing.

## Mandatory before RFC 004's T4 (the acceptance orchestrator)

**C5 — The signer checks the files, not only the message.** `ExpectedMessage` binds the proposal and the people but
nothing ties the prepared files to what was accepted. Add the digest of the prepared change (paths, contents, renames,
in a canonical order) to the acceptance token's expectation and have the signer recompute it over what it writes
before `commit`; a mismatch is a refusal before any write. RFC 004 D-4 already records "the merged content's digest"
on the acceptance record: this is its check. Test: a change altered between the record and the call is refused and
the repository is unchanged.

**C6 — Seed directories are swept by liveness, not age.** Hold an `flock` on each per-call seed directory for the
call's life so a start can remove every unlocked directory at once (a quick supervisor restart no longer leaves
seeds for five minutes); remove with `O_NOFOLLOW` semantics (open the directory, unlink by handle); keep the
ownership and mode checks. `serve` sweeps at start too (it opens the signer from T7 on). Test: a killed call's
directory is removed by the next start immediately; a live call's is not.

**C7 — Principals and key operations need a witness.** `Principal::session`, `Principal::token` and the signer's
`create_keys`, `rotate_keys`, `create_repository` are callable by any crate that can name them. Give each a witness
type only its rightful caller can mint (`inklkunn-auth` for session and token principals; core's administrative
entry points for the key operations), so the type system, not only the gate, refuses the shortcut. Test: a
compile-fail case for each (a `trybuild`-free way is fine: a gate test over a planted source, as the other gates do).

## Before OpenID Connect is switched on (M2)

**C8 — IQ-3 decided: a confined `curl`, in a new lower-layer crate `inklkunn-fetch`.** The egress guard (core) decides
and pins the address; `inklkunn-fetch` runs `curl` in its own bubblewrap vector (network shared, no repository, no key,
`/usr` and the CA bundle read-only, `--resolve` to the pinned address, `--proto =https`, no redirects, bounded time and
bytes) and implements `inklkunn-auth`'s `HttpsFetcher`. The spawn gate gains it as the third crate allowed to spawn a
process (for `curl` only), the layering gate lists it as lower. No TLS crate enters the lock (planeter's ruling and
inklkunn's dependency policy). Internal design AZ-1 and LAY-3 are updated by the architect.

**C9 — The OIDC round trip survives the browser.** The session cookie stays `SameSite=Strict`, but the login-state
cookie that the identity provider's cross-site redirect must carry back is `SameSite=Lax`, `HttpOnly`, `Secure`,
short-lived and single-use; if anything still needs a same-site step, land on an interstitial page that completes the
sign-in with a same-site request. Test in a real browser engine once, recorded as a person or a tool did it.

**C10 — Smaller items from the readings** (each with a test): the `role_grants` foreign key to `accounts` (a V6
migration) and mutant M19 killed (the store's connection must set `foreign_keys`, and a test must notice when it does
not); a start-up warning when the bind address is not loopback and no trusted proxy is declared (thirty bad attempts
by anyone would lock everyone out); a cap on live sessions per account (the tokens' hundred is the model).

## Recorded, not requested here

- **Key rotation (PK-34):** the default-keep / explicit-retire design stands **until the owner rules DR-10**; nothing
  changes in code before that.
- **A lost key store (PK-35):** a `recover` operation that provisions new keys under fresh ids is an operations
  matter (the restore runbook, DUR-1); it joins the operations RFC before 1.0, tracked as IS-15.
- **Reconciliation scheduling and the audit view:** RFC 004 (after every seal) and the administration surface.
- **The undeclared-proxy throttle hazard** is C10's warning; sampling anonymous denials in the audit trail is deferred
  to the governance RFC (006), where abuse handling lives.
