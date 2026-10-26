# RFC 002 — Identity, keys and signing: who a principal is, what they may do, and how the wiki service signs on their behalf

**Status.** **Accepted by the owner 2026-10-27** (*"Accepted."*); proposed the same day by the architect. The
implementer may start per the handoff **once RFC 001's tasks T1–T5 have landed and been approved**:
[`../handoffs/002-identity-keys-and-signing/identity-keys-and-signing-handoff-v1.md`](../handoffs/002-identity-keys-and-signing/identity-keys-and-signing-handoff-v1.md).
**Tracks.** ROADMAP M0 → M2, theme T2; the owner's ruling **DR-4**. Requirements `ID-1 … ID-9`, `PR-4`,
`PR-12`, `GV-2` (the bootstrap role set), `SEC-1`, `SEC-3`, `SEC-4`, `SEC-6`, `PRV-2`, `INT-2`, `INT-4`;
external design `BD-05`, `AUTH-01 … AUTH-05`, `PK-03`, `PK-05`, `CT-03`, `OP-07`; internal design
`SGN-*`, `AZ-*`, `DM-3 … DM-5`, `ENF-1`, `ENF-2`; threat model `T-1`, `T-2`, `T-3`, `T-12`, `INV-1`,
`INV-2`, `INV-4`, `INV-6`, `INV-10`, `RR-1`, `RR-4`.
**Touches.** `inklkunn-auth` (principals), `inklkunn-core` (`authorize()`, the honesty view-model, the
message composer, the reconciliation), `inklkunn-signer` (the key store, the `Accept` implementation, key
lifecycle), `inklkunn-store` (accounts, identities, roles, tokens, audit). **Not here:** the proposal
records and the acceptance orchestration (RFC 004), governance policy as data (RFC 006), shared identity
with planeter (RFC 008).

## Summary

The owner ruled (DR-4) that the wiki service signs as itself: one author key and one maintainer key per
wiki, held by the service, with the human recorded in the signed message and honest labels. This RFC turns
that ruling into a contract that can be built and tested: **who a principal is** (D-1), **what they may do**
(D-2), **where the keys live and who can reach them** (D-3, D-4), **how a change records the people behind
it** (D-5), **how the service tells the truth about signatures** (D-6), and **how a seal that matches no
acceptance would be caught** (D-7). The load-bearing result is structural: the signer is the only holder of
keys and the only runner of prikk's write verbs, reachable from the rest of the system through one call that
takes a recorded, accepted proposal.

## The constraints that scope this design

- prikk requires an author signature on every patch and a maintainer signature on every seal; maintainer
  trust is repository-local and adopted explicitly; there is no author-key revocation (prikk trust model;
  UD-5).
- Contributors hold no keys and must not need to (PU-1, PR-4).
- planeter's keyless property does not hold for inklkunn; the threat model accepts that (RR-1) on the
  condition that custody is bounded, detectable, attributable and recoverable (C-1a … C-1f).
- The web tier must not reach key material; the RFC 001 gate guards the crate boundary; this RFC fixes what
  crosses it.

## Decisions

- **D-1 — Principals.** Exactly one of: **Anonymous**; an **Account** reached through a session or a token
  (the token's scopes intersect the account's roles); a **System** principal (import, recovery) that can
  never be reached from the network. Accounts have a local password (Argon2id), an OpenID Connect identity
  (issuer, subject), or both; linking an identity is an administrative act or, where the wiki's policy says,
  the first sign-in. Display names are one free-text field in any script. Sign-in is constant-work
  (unknown account and wrong password cost the same), throttled per account and per client address (the
  address from a declared trusted proxy only), with `Secure`, `HttpOnly`, `SameSite` cookies and a CSRF
  token on every cookie write. Tokens are hashed, scoped (`read` ⊂ `propose` ⊂ `review` ⊂ `accept` ⊂
  `admin`), expiring and revocable. A second factor is not in v1 (RR-4); the gap is stated in SECURITY.md.
- **D-2 — `authorize(principal, action, resource) -> Decision`.** Pure, default-deny, in `inklkunn-core`,
  consulted on every path. Actions: `read`, `comment`, `propose`, `review`, `accept`, `create-view`,
  `moderate`, `admin`, `export`, `import`. Resources: wiki, namespace, page, view, proposal, discussion.
  Roles per wiki, the ladder as a fixed set for M2 (`observer`, `questioner`, `contributor`, `reviewer`,
  `maintainer`, `admin`), granted directly; RFC 006 replaces the fixed set with policy as data without
  changing this contract. Visibility is a read gate: `public` allows Anonymous, `internal` any account,
  `private` a grant. A denied private resource is indistinguishable from an absent one. Consequential
  decisions (accept, moderate, admin, export, import, role grants) are audited (D-9). A route-enumeration
  test asserts every route and API verb consults the function or is registered public with a reason.
- **D-3 — The key store.** One record per wiki in `inklkunn-signer`'s private module: the author seed and
  the maintainer seed, each encrypted with an authenticated cipher under a key derived from the
  **operator's secret** (supplied at start through an environment variable naming a file; never stored with
  the records), a fresh nonce per record, the key ids, created and rotated-at times. The chosen primitives
  follow the dependency policy (RFC 010) and are named in the handoff after measurement; the requirement is
  an AEAD and a memory-hard derivation. Key ids are `inklkunn-<wiki id>-author` and
  `inklkunn-<wiki id>-maintainer`. A decrypted seed exists only as a mode-0600 file in a private
  tmpfs-backed directory, created for one write call, mounted into the sandbox, removed after; the path and
  the bytes never appear in logs, errors, content or exports.
- **D-4 — The `Accept` trait has one operation and one guard.** `accept(wiki, view, acceptance_token,
  prepared_change) -> SealedChange`. The acceptance token names an acceptance record in state *pending
  seal* with an open intent (RFC 004's records; this RFC fixes the guard). The signer: verifies the record's
  state and that the prepared message names the same proposal and acceptor (D-5); brings the worktree to the
  view (`branch switch` when needed); writes the files and declares renames with `prikk mv`; `prikk commit
  -m <message>` under the author seed; `prikk seal --allow-no-audit` under the maintainer seed; `prikk
  verify`; returns the block id and patch ids. Any refusal before `commit` returns a typed error and changes
  nothing; a failure after `commit` returns a typed *incomplete* that RFC 004's recovery completes. The
  second operation, `create_view(wiki, name, from)`, is a maintainer-signed `branch create` bound to a
  recorded view-creation act. No other operation exists; no key, seed path or signing primitive is exposed.
- **D-5 — The patch message is the attribution contract** (CT-03). Line 1: the proposal's summary, at most
  120 characters, no newline. Line 2: empty. Then trailers, one per line, in this order and no others:
  `Proposal: <uuid>`, `Proposed-By: <account id> (<display name>)` or `Proposed-By: anonymous (<pseudonym>)`,
  `Accepted-By: <account id> (<display name>)`, `Base: <block id>`, `Kind: proposal | rollback | import |
  system`, optionally `Imported-From: <url or bundle digest>` for imports. Display names are the ones at
  the time; account ids are stable. The composer in `inklkunn-core` writes it; the signer re-parses it and
  refuses a mismatch with the acceptance record; `prikk log` shows it to anyone; the reconciliation (D-7)
  parses it back. *Fact:* prikk stores the message signed since 0.32.0.
- **D-6 — Honesty labels.** The `Assurance` view-model has the values *Verified*, *Verification failed*,
  *Could not be checked now* (the repository, re-derived, never cached), *Signed into the history* (a change
  read from `log`), *Held for review* (a proposal, open), *Accepted* and *Declined* (proposal states). Every
  change page says *signed by the wiki service on behalf of <proposer>, accepted by <acceptor>*; no text
  anywhere says or implies a person signed cryptographically. The catalog and the glossary carry the words;
  the retired-words test forbids `sealed`, `commit`, `patch` and `block` outside the on-demand explanations.
- **D-7 — Reconciliation.** After every seal and on a schedule, for each wiki: every acceptance record in
  state *accepted* names a block that `log` lists and whose message parses to the same proposal and
  acceptor; every block that `log` lists carries a message that parses to an acceptance record in state
  *accepted*, or has `Kind: import | system` with a matching system act. A mismatch is reported in the audit
  log, on the administration surface and as a metric; it never changes anything by itself (INV-1, INV-10).
- **D-8 — Key lifecycle.** `create` (two `prikk key generate`, `trust maintainer add` in the wiki's
  repository, record), `rotate` (new ids, adopt the new maintainer key, remove the old from trust, record;
  the old author key simply stops being used, since prikk has no author revocation), `status` (ids and
  dates, never a secret). Each is an administrator's audited act through the administration surface or a
  CLI subcommand of the binary, requiring re-authentication. The compromise runbook: rotate, re-verify,
  export the audit log, review the reconciliation, state what was signed during the window; it lives in
  `docs/` and SECURITY.md names it.
- **D-9 — The audit log.** Append-only records {at, principal, action, resource, outcome, detail without
  content} in `inklkunn-store`, exportable as JSON Lines; the sink is a trait so a later release can ship
  it off-box.
- **D-10 — Anonymous proposing** (OQ-1, recommended off): designed as a policy option: when enabled, an
  anonymous proposer is given a temporary pseudonym (`~<short token>`) bound to their session, recorded as
  `Proposed-By: anonymous (<pseudonym>)`, with no address stored beyond the throttle's window. Off by
  default; the option's switch is an admin act.

## What "done" means (acceptance criteria)

- D-1: local and OpenID Connect sign-in work; constant-work verification is measured (unknown account and
  wrong password within noise); throttles lock as specified; tokens cannot exceed their scope.
- D-2: default deny on every (action, resource) for a principal with no grant (property test); a private
  wiki denies identically whether or not it exists; the route-enumeration test passes and fails a planted
  unguarded route.
- D-3: key records are unreadable without the operator secret; a seed file exists only during a write call
  (a test inspects before, during and after); no secret appears in logs or errors (a test greps).
- D-4: `accept` refuses a token whose record is not pending; refuses a message naming a different
  proposal or acceptor; on a repository at the floor, commits and seals a prepared change and returns ids
  that `prikk log` and `prikk verify` confirm; a failure injected after `commit` returns *incomplete* and
  leaves exactly one queued patch for recovery.
- D-5: the composer and the parser round-trip every field; `prikk log --format json` shows the message.
- D-6: an open proposal never renders *Verified* or *Signed into the history*; the retired-words test
  passes.
- D-7: the reconciliation reports a planted unrecorded seal and a planted unsealed acceptance.
- D-8: create, rotate and status work on a real repository; after rotation the old maintainer key no
  longer verifies new seals and the repository still verifies.
- The RFC 001 gate (b) still passes: no crate but `inklkunn-core` depends on `inklkunn-signer`, and only
  through the trait.

## Alternatives considered

- **Per-account author keys held by the service.** Deferred (DR-4 b): finer attribution inside prikk at the
  cost of a store of user keys and no revocation path in prikk. The message contract gives per-person
  attribution without it; the option stays open.
- **Client-side keys.** Deferred (DR-4 c): right for maintainers who want it, wrong as a requirement for
  newcomers.
- **Human maintainers seal on their own machines** (planeter's model). Rejected as the default for a wiki:
  the page would update only when a maintainer's machine ran; kept as a later opt-in.
- **A separate signer process now.** Deferred (internal design SGN-5, IQ-4): the trait is designed for it;
  the in-process module with the build gate is sufficient for M2 and is revisited at threat model v0.2.

## Security considerations

This RFC is the threat model's T-1 in full: C-1a (one holder, one call), C-1b (bound seals), C-1c (keys at
rest), C-1d (per-wiki keys, rotation), C-1f (labels); T-2 and T-3 through D-1 and D-2; T-12 through D-5,
D-7 and D-9. The residual RR-1 is accepted by DR-4 and stated in SECURITY.md in the same release.

## Open questions and dependencies

- **IQ-3** The outbound HTTPS client for OpenID Connect: a confined `curl` through the egress guard
  (planeter's shape, no TLS crates) or an in-process client; measured under RFC 010's policy in the handoff.
- **IQ-4** Signer process separation: revisited at threat model v0.2.
- **OQ-1** Anonymous proposing default (recommended off); D-10 is designed either way.
- Depends on RFC 001 (the crates, the gates, the storage driver) and provides what RFC 004 needs.

## Sequencing and handoff

Second, on RFC 001's base. The handoff is drafted (v1) and becomes assignable on acceptance and when RFC 001's
M0 work has landed. RFC 004 then builds the proposal records and the acceptance orchestration on `Accept`.
