# Identity, keys and signing — Corrections Handoff v1 (RFC 002, after the review of 2026-10-29)

| | |
|---|---|
| Document | The architect's correction requests on handoff 002 (T1 … T8), from the review of the dev team's request `.git-exclude/review-requests/002-identity-keys-and-signing-review-request.md`. The outcome was **Conditionally Approved**: the work stands; the items below are owed, each before the step it names. |
| Status | Inherits RFC 002's: accepted. **Assignable now.** C1 – C4 are **mandatory before handoff 003's T7** (the binary's first writes); C5 – C7 before RFC 004's T4; C8 – C10 before OpenID Connect is switched on in a deployment (M2); **C11 before 0.1.0** (added 2026-10-29 with the owner's DR-10 ruling). |
| Basis | RFC 002 D-1 … D-10 (as concretized on 2026-10-29: D-4, D-5 in the signer crate; D-8 as ruled by the owner in DR-10, 2026-10-29); threat model T-1, C-1a … C-1c, C-9a, INV-2, INV-6; ledger PK-9, PK-20, PK-34, PK-35, PK-36; internal design SGN-6, AZ-1 (IQ-3 decided below). |
| Audience | The dev team. Read the two lenses in handoff 003 first. |
| Handback | One review request covering C1 – C11 (or one per group if they land at different times), filed in `.git-exclude/review-requests/`, with the usual gates, both test runs and a mutation table per item. |

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

## Before 0.1.0 (the first release that creates wikis with real keys)

**C11 — A rotation records where the old key stopped, and a seal after that point is reported (DR-10, PK-34).** The
owner ruled DR-10 on 2026-10-29 after prikk confirmed the design: **a rotation keeps the old maintainer key trusted by
default, retirement stays the explicit, named act** (`OldKey::Keep` / `Retire` as built). prikk also confirmed that a
leaked copy of a still-trusted key can seal anything that verifies, and that a copy of the repository cannot prove a
block was sealed by the old key *after* the new one was adopted (adoption is local and recorded nowhere a copy can
check). inklkunn can: it knows the moment. So:
- **Record the point.** At the rotation's commit point (step 3, under the per-wiki lock of C3), the key record keeps,
  for each superseded generation, its maintainer key id and the tip block id of every view at that moment (a record
  `format` bump; the old ids are today only in the audit trail and in prikk's trust store). A retired key is recorded
  the same way, marked retired.
- **Check it.** A core function (the reconciliation's first half that needs no acceptance record, so it works before
  RFC 004) reads each block's maintainer signer (from whichever prikk output the driver pins for it; measure, and
  record a ledger entry) and reports every block sealed by a superseded key that is **not** in the history up to that
  key's recorded point: *sealed by a superseded key after the rotation*. Reported in the audit log and, once they
  exist, on the administration surface and as a metric; it never changes anything by itself (D-7, INV-1). It runs
  after each rotation and with the reconciliation (RFC 004 puts it after every seal and on a schedule); until then
  `inklkunn wiki keys status` runs it and prints the result.
- **Say what it covers.** It detects a leaked seed used against the wiki's own repository while the service's
  records are intact (a leaked backup, a leaked seed file); it does not help when the host itself is taken over (the
  records are the attacker's then) — that is `--retire-old`'s case. `docs/KEYS.md` says so in the table's
  neighbourhood and in the runbook's step 1, and replaces "a request to prikk is open" with prikk's answer of
  2026-10-29: confirmed, a rotation that keeps history verifiable is not scheduled.
- **Tests:** a planted seal under the old key after a default rotation is reported, with its block id; the old
  key's seals before the point are not; a retirement records the point too; an old-format record still opens (or is
  refused naming the format, if the format policy says so).

## Recorded, not requested here

- **Key rotation (PK-34):** DR-10 ruled 2026-10-29 (above, C11). prikk will write when a rotation that keeps history
  verifiable is scheduled; until then the threat model carries it as a waiting dependency with no date.
- **A lost key store (PK-35):** a `recover` operation that provisions new keys under fresh ids is an operations
  matter (the restore runbook, DUR-1); it joins the operations RFC before 1.0, tracked as IS-15.
- **Reconciliation scheduling and the audit view:** RFC 004 (after every seal) and the administration surface.
- **The undeclared-proxy throttle hazard** is C10's warning; sampling anonymous denials in the audit trail is deferred
  to the governance RFC (006), where abuse handling lives.
