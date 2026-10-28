# inklkunn — Threat Model

| | |
|---|---|
| Document | inklkunn Threat Model (security): what inklkunn must defend, against whom, and how, so that the design and the tests can be checked against it. A first-class release deliverable (project rules): updated by any release that touches identity, signing, the prikk boundary, rendering or inbound content; re-verified by the others. |
| Version | v0.1, amended 2026-10-29 (C-1d and RR-8: key rotation on prikk as it is, the owner's DR-10 ruling after prikk confirmed it) |
| Date | 2026-10-27 |
| Basis | Requirements v0.1 (SEC/PRV/ID/GV/INT), the external design v0.1 (BD/WEB/API/AUTH/PK/FL/CT/OP), the rulings DR-2 and DR-4, prikk's trust and threat model, planeter's threat model v0.4 (the house precedent and the neighbour whose guarantees inklkunn must not weaken), wiki-commons chapter 14, the requirements register items 7, 8, 14, 17. |
| ID scheme | `A-` asset · `TB-` trust boundary · `T-` threat · `C-` control · `INV-` security invariant · `RR-` residual risk · `ASSUME-` assumption |

**The essence, in one paragraph.** inklkunn differs from planeter on the axis that matters most: **it holds
history-signing keys.** By the owner's ruling (DR-4) the wiki service is the author and the maintainer of its
own repositories, because wiki contributors hold no keys and must not need to. So the central property of a
prikk forge, "a compromised service can deny history but never forge it", does **not** hold for inklkunn as
a whole; what inklkunn offers instead is that a forged change is **detectable and attributable**: every
signature is the wiki service's and says so, every change names the proposal and the people behind it in a
signed message, planeter can hold the same repository read-only as an independent witness, and the keys live
behind a boundary no web request can cross. The second signature danger is the familiar one: a service that
renders and stores text from strangers, faces the network, and serves mutually distrusting users inherits
every web threat a wiki has ever had, plus the social ones a wiki's thesis invites, since the same low
friction that makes participation inclusive makes abuse cheap (register item 7). The modern addition is AI in
the loop, answered by keeping content as data and keeping any AI out of the authority path. **The design
consequence that follows is the signer boundary (TB-5, INV-2): keys reachable from one component only, that
component reachable from the web tier through one narrow call, and the whole arrangement a property of the
build graph.**

---

## 1. Assets (A-…)

- **A-HISTORY** — the integrity and prikk-verifiability of each wiki's history. Its value: anyone holding the
  repository can `prikk verify` it offline and read who accepted what.
- **A-KEYS** — the per-wiki author and maintainer keys. The highest-value secret: whoever holds a wiki's
  maintainer key can seal history that prikk accepts.
- **A-ATTRIBUTION** — the truth of "who proposed, who accepted": in the signed patch message and in the
  service records, cross-checkable.
- **A-AUTHZ** — roles, policy, protections; corruption grants writes or reads to the wrong people.
- **A-CREDS** — passwords, sessions, tokens.
- **A-CONTENT** — private wikis' pages and proposals; held-for-review content that is not yet public.
- **A-RECORD** — the service store: review state, audit log; and the derived cache, whose corruption could
  mislead a page.
- **A-HOST**, **A-AVAILABILITY**, **A-SUPPLY** — as for any service.
- **A-COMMUNITY** — the social system's health: a wiki captured by a faction or buried in spam is as lost as
  a corrupted one (register item 7, 12 of the main transcript).

## 2. Trust boundaries (TB-…)

| ID | Boundary | Direction and trust |
|---|---|---|
| **TB-1** | anonymous internet → inklkunn | untrusted in: every byte attacker-controlled |
| **TB-2** | authenticated principal → inklkunn | semi-trusted, scoped by `authorize()`; an account may be hostile or stolen |
| **TB-3** | proposed and pasted content → rendering and, on acceptance, the repository | untrusted text that becomes a file: path, frontmatter, body, attachment |
| **TB-4** | inklkunn → prikk subprocess | inklkunn drives; prikk verifies; the process is a containment boundary for a parser fault |
| **TB-5** | web and API tier → the signer | **the key boundary**: one narrow call in, a sealed result out; no key material crosses it |
| **TB-6** | inklkunn → external services (OpenID Connect, interwiki checks, link previews, webhooks) | every user-influenced URL is an SSRF vector |
| **TB-7** | inklkunn → the viewer's browser | rendered content in a privileged origin: the XSS boundary |
| **TB-8** | operator ↔ inklkunn | trusted; the operator supplies the key-store secret |
| **TB-9** | planeter ↔ inklkunn | semi-trusted peer: identity assertions in, read-only hosting out |
| **TB-10** | imported bundles and archives → inklkunn | untrusted archives: traversal, bombs, hostile frontmatter |

## 3. Threats and controls (T-…, C-…)

### T-1 — Forged history: a change sealed without an authorized acceptance
The maintainer key is used by something other than FL-04's acceptance: a compromised web tier calling the
signer directly, a stolen key, an insider with the key store secret, a bug that seals without a proposal.
(A-HISTORY, A-KEYS, A-ATTRIBUTION.)

- **C-1a — The signer is the only holder and the only user of keys** (TB-5; INV-2). Key material is loaded
  by the signer component alone; the web and API tiers depend on an interface whose one operation is
  *accept this prepared change for this proposal* and whose result is the sealed change's ids. A build gate
  fails if any other crate depends on the key store.
- **C-1b — Every seal is bound to a recorded proposal and acceptance.** The signer refuses a change that
  names no proposal in an *accepted-pending-seal* state with an intent record; the patch message (CT-03)
  carries the proposal id and the acceptor; the acceptance record carries the block and patch ids. A seal
  that matches no acceptance record is detectable by a reconciliation that runs after every seal and on a
  schedule, and is reported, never hidden.
- **C-1c — Keys at rest are encrypted under the operator's secret**, decrypted for one write onto a private
  mount only the sandboxed prikk can read, removed after; never logged, never in content, never exported
  (SEC-4, OP-07). The secret is supplied at start, not stored beside the keys.
- **C-1d — Per-wiki keys and rotation.** A compromise is bounded to one wiki; rotation adopts a new key id
  and the service seals only with it from then on; the procedure is documented and rehearsed. **On prikk,
  removing a key from trust is revocation** (it makes everything that key ever sealed fail verification; prikk
  confirmed this is by design, 2026-10-29; ledger PK-34), so by the owner's ruling DR-10 a rotation **keeps the
  old maintainer key trusted** and destroys the service's copy of its seed, and **retiring** it is an explicit,
  named act for a compromise, which turns the wiki's verification label to *Verification failed* until prikk can
  rotate without revoking. A kept key's leaked seed could still seal changes that verify; the rotation records
  the point where the old key stopped, and inklkunn reports any block sealed by it after that point (RR-8).
- **C-1e — Independent witness.** The repository can be hosted read-only by planeter and exported as an
  archive at any time (MD-5, IX-2), so a forged change is visible to a path the wiki does not control.
- **C-1f — Honest labels.** Every change is labelled *signed by the wiki service on behalf of …* (ID-5): the
  reader is never told a person signed.

### T-2 — Authentication and session attacks
Credential stuffing, session theft, OpenID Connect misconfiguration, token leakage (A-CREDS).

- **C-2a** Argon2id passwords; OpenID Connect with PKCE, one-shot state, nonce-bound tokens verified against
  the provider's keys; constant-work sign-in so account existence does not leak by timing (planeter's IS-31
  lesson, adopted from the start).
- **C-2b** Per-account and per-client-IP throttles; the client IP only from a declared trusted proxy.
- **C-2c** `Secure`, `HttpOnly`, `SameSite` cookies; CSRF double-submit on every cookie write; scoped, hashed,
  expiring, revocable tokens; re-authentication for key operations and role grants.
- **C-2d** A second factor is a stated gap for v1 (RR-4); an OpenID Connect provider that enforces it is the
  interim answer.

### T-3 — Authorization bypass and confused deputy
A reader reaches a private wiki, a proposer accepts their own proposal, a token with `read` proposes, a role
is escalated through a policy edit (A-AUTHZ, A-CONTENT).

- **C-3a** One pure default-deny `authorize()` on every path (ID-7); a test enumerates every route and verb
  and asserts the call (planeter's ENF-4, written before the first write handler).
- **C-3b** Scopes intersected with roles per endpoint; existence-neutral denials.
- **C-3c** Policy changes are proposals under the constitutional layers (GV-1); the bootstrap roles cannot
  be removed by a policy edit; protections are authorization outcomes, not UI.

### T-4 — Hostile proposed content
Attacker-controlled text that becomes a file in a repository: a title that is a path (`../`, reserved names,
case collisions), frontmatter that is YAML with surprises, a body that is a bomb, an attachment that lies
about its type, a rename to a path outside `pages/` (TB-3; A-HOST, A-HISTORY).

- **C-4a** Title-to-path mapping is a pure, documented, fuzz-tested function that refuses anything not a safe
  relative component under the wiki's directories (CT-01); the repository root and `.prikk/` are never a
  destination; prikk's own path safety is the second line, never the first.
- **C-4b** Frontmatter is parsed with a plain YAML subset (scalars, lists, maps; no anchors, no tags) and
  validated against the metadata schema; unknown keys preserved as data; secrets and control fields
  (`visibility`, `kind: system`) are set by the service, not by the proposer, unless the role allows.
- **C-4c** Sizes bounded before parsing (PERF-4); attachments type-checked by content, served as attachments
  from the content origin; SVG sanitized or served inert.
- **C-4d** Pasted and proposed Markdown is data; no directive executes (NG-8); raw HTML reduced to the
  documented subset.

### T-5 — Hostile imports (TB-10)
Archive traversal, symbolic links, deep nesting, size bombs, a bundle whose manifest lies, a prikk archive
carrying keys to adopt.

- **C-5a** Imports are admin-only, dry-run first, bounded, extracted into a private scratch directory with
  every entry's path validated; symbolic links refused; checksums verified when present (wiki-commons
  SEC-7).
- **C-5b** A prikk archive import never adopts carried keys; the wiki's own keys remain the only trusted
  signers (prikk's own rule: carried, not adopted).
- **C-5c** Imported content goes through the same sanitizer and the same path mapping as a proposal, and
  arrives as attributed changes with an import report (IX-3).

### T-6 — Cross-site scripting through rendered content (TB-7)
- **C-6a** Strict CSP, no inline script, no third-party scripts; `nosniff`, `frame-ancestors 'none'`,
  `Referrer-Policy`.
- **C-6b** Allowlist sanitizer on every render, including previews and diffs (a diff renders attacker text
  twice).
- **C-6c** Raw source, attachments and downloads only from the isolated content origin, as inert
  attachments; the host gate enforced, not only configured (planeter's IS-16 lesson).

### T-7 — Confidentiality of private wikis and held content
- **C-7a** `authorize()` on reads, feeds, search, backlinks, sitemaps, link previews and the API; `noindex`;
  existence-neutral errors (PRV-4).
- **C-7b** Held-for-review content is visible only to those the policy names; notifications never carry
  content the recipient could not read (PRV-5).
- **C-7c** Secrets committed into pages are carried by prikk and cannot be un-leaked; the risk is stated to
  operators (SEC-9; RR-2).

### T-8 — Server-side request forgery (TB-6)
- **C-8** One egress guard for every outbound fetch (SEC-6): public unicast only, pinned addresses, HTTPS
  only, no redirects followed blindly; link previews and interwiki checks are click-to-load or server-side
  through the guard; webhooks the same when they come.

### T-9 — Denial of service
Expensive prikk calls on large repositories, proposal floods, huge pages, acceptance queues, import bombs
(A-AVAILABILITY, A-HOST).

- **C-9a** Every prikk call under a time and memory budget with weighted admission and a per-call ceiling
  (SEC-7; planeter RFC 011 pattern); a ceiling hit is *unavailable*, never a verification verdict.
- **C-9b** Quotas per principal and per wiki (PR-14); rate limits with `Retry-After`; bounded page and
  attachment sizes; the acceptance queue bounded and visible.
- **C-9c** Metering and alerts on disk (repositories only grow; prikk deletes nothing) and on queue depth.

### T-10 — The service store or the cache as a false source of truth
A corrupted record or cache shows a page that is not the history, or marks a proposal accepted that was not
sealed (A-RECORD, A-HISTORY).

- **C-10a** The repository is the source of truth for content and history; the cache is keyed by prikk's
  content ids and rebuilt from prikk; a verification badge is never served from a cache (RD-4, CM-7).
- **C-10b** The reconciliation of C-1b runs both ways: every acceptance record names a block that exists and
  verifies; every sealed block names an acceptance record; disagreement is reported.
- **C-10c** A test rebuilds every view with the derived store cleared and compares (CM-7).

### T-11 — Supply chain
- **C-11** Pinned and locked dependencies, `cargo deny` and `cargo audit` at every project event, a license
  allow-list, `forbid(unsafe_code)`, measured growth, a preference for confined subprocesses over
  in-process protocol stacks (planeter's dependency policy, adopted); CI actions pinned by digest.

### T-12 — Repudiation
"I never proposed that", "I never accepted that" (A-ATTRIBUTION).

- **C-12a** The signed patch message names the proposal, the proposer and the acceptor (CT-03); the service
  records hold the same with timestamps; the audit log is append-only and exportable (ID-8).
- **C-12b** The limit is stated: prikk's signature proves the wiki service signed, not the person; the
  person's act is proven by the service's records and the audit log, which the operator controls (RR-1).

### T-13 — The AI-era surface
Prompt injection through page text against any future assistant; AI-scaled proposal spam; dependency
hallucination in the build (A-CONTENT, A-COMMUNITY, A-SUPPLY).

- **C-13a** No AI holds authority: nothing can propose, accept, moderate or change policy autonomously; any
  assistance is advisory, labelled, attributed to the human who accepted it (NG-5; C-13; AUTH-14).
- **C-13b** Content is data, never instructions, for inklkunn and for any integration it hosts.
- **C-13c** No private content leaves to an external AI without explicit per-use opt-in.
- **C-13d** AI-scaled abuse is bounded like any abuse: quotas, the ladder, review before acceptance, and the
  inclusiveness metrics that show concentration and flood.

### T-14 — Abuse of the social system
Astroturfing, brigading, sock puppets, source flooding, citation laundering, harassment dressed as epistemic
discussion, strategic forks, capture of the maintainer role (register item 7; A-COMMUNITY).

- **C-14a** Every consequential act is attributed, logged and visible: who proposed, reviewed, accepted,
  moderated, changed policy (PR-12, GV-3).
- **C-14b** Soft security first, then progressive friction set by policy: quotas, new-account limits,
  protections per page, review required (GV-4, GV-7).
- **C-14c** Moderation hides with a tombstone and an appeal; nothing is erased silently (GV-5, GV-8).
- **C-14d** The constitutional layers keep the root roles out of reach of a captured policy (GV-1); no
  opaque reputation becomes a lever for capture (GV-2).
- **C-14e** The inclusiveness metrics expose concentration and silence (GV-9).

### T-15 — Removal, privacy and the immutable record
Personal data, defamation, infringing or illegal content in a sealed change; a person's right to be
forgotten (register item 14; A-CONTENT).

- **C-15a** Suppression on the view is immediate: the content, summary or author of a revision hidden behind
  a tombstone with a reason, logged, reversible (GV-5, HIST-12).
- **C-15b** Permanent removal is a documented operator procedure that states its cost: prikk cannot delete an
  object, so the repository is re-created without the content and the ids of affected history change; the
  procedure exists and is rehearsed before any public deployment (GV-6).
- **C-15c** Pseudonymous participation, no published addresses, account deletion replaces the name with a
  stable pseudonym (PRV-2, PRV-3).

### T-16 — The planeter boundary (TB-9)
A forged identity assertion from a fake planeter; planeter's guarantees weakened by inklkunn.

- **C-16a** Identity assertions are verified cryptographically (OpenID Connect tokens or signed tokens per
  RFC 008), never trusted from a header alone except behind a declared trusted proxy.
- **C-16b** inklkunn never gives planeter a key, never writes to a planeter-hosted software repository, and
  planeter never writes to a wiki repository (MD-1, MD-5).

## 4. Security invariants (INV-…) — a change that breaks one is a security bug

- **INV-1 — No history without an authorized acceptance.** Every sealed change names a recorded proposal and
  acceptance; the reconciliation detects any other.
- **INV-2 — Keys are reachable from the signer alone.** No web, API or storage-read code path can read key
  material; the build graph enforces it; keys are encrypted at rest and never leave the host unencrypted.
- **INV-3 — All network input is untrusted; no content-provided code executes; the prikk parser runs confined;
  every user-influenced URL is SSRF-filtered.**
- **INV-4 — Authorization is default-deny on every access path**, enforced by one function and one test.
- **INV-5 — prikk is the source of truth; the service store and the cache are never authoritative for history;
  a verification badge is never served stale.**
- **INV-6 — Honesty of labels:** nothing says *verified* unless prikk verified it just now; nothing implies a
  person signed cryptographically; held-for-review content is never shown as the record.
- **INV-7 — Content is data, never instructions; no AI holds authority.**
- **INV-8 — Removal is suppression plus a documented procedure, never a silent rewrite.**
- **INV-9 — The supply chain is pinned, minimized and audited; every crate forbids unsafe code.**
- **INV-10 — Attribution survives either store:** the signed message and the service records agree, and
  disagreement is reported.

## 5. Residual risks and assumptions (RR-…, ASSUME-…)

- **RR-1 — The service holds the keys.** A total compromise of the host or of the key-store secret lets an
  attacker seal history that prikk accepts, in every wiki whose keys they obtained. The ruling DR-4 accepts
  this for the sake of keyless participation; the controls make it bounded (per-wiki keys), detectable
  (reconciliation, independent witness), attributable (every seal says it is the service's) and recoverable
  (rotation). This is the central difference from planeter and must be said plainly in SECURITY.md.
- **RR-2 — Secrets committed into pages** are carried faithfully; rotation in the real world is the only
  remedy.
- **RR-3 — The prikk subprocess parses attacker-influenced bytes** (paths, content, imported archives); a
  prikk parser fault is confined, not eliminated.
- **RR-4 — No second factor in v1** on local accounts; an OpenID Connect provider that enforces one is the
  interim answer.
- **RR-5 — Social capture is bounded, not prevented.** A faction holding the admin role can govern badly in
  the open; the model makes it visible and forkable, not impossible.
- **RR-6 — Removal is lossy.** A legal removal changes the ids of affected history and breaks permalinks;
  the procedure says so.
- **RR-7 — Throttles and leases are per process** in v1; a multi-replica deployment needs the proxy to
  throttle and a shared lease (operations RFC before 1.0).
- **RR-8 — A rotation cannot both stop the old key and keep its history verifying** (prikk 0.54 – 0.55; a
  rotation that does both is "not built yet, not refused" by prikk and **not scheduled**: a waiting dependency
  with no date, prikk's letter of 2026-10-29). Routine rotations keep the old key trusted; its seed, if it leaked,
  can seal anything that verifies. inklkunn detects such a seal in the wiki's own repository from its record of
  the rotation point while the host's records are intact; **a copy of the repository** (planeter as a read-only
  witness, an archive) sees which key sealed each block but **cannot prove** that a block came after the
  rotation, because prikk records the adoption of a key nowhere a copy can check. A host compromise is
  `--retire-old`'s case, at the cost of the *Verified* label.
- **ASSUME-1** The operator and the host are trusted; inklkunn defends against network actors and malicious
  users, not a hostile operator.
- **ASSUME-2** prikk's offline `verify` is correct and is the arbiter of history integrity.
- **ASSUME-3** planeter, when integrated, enforces its own invariants; inklkunn does not rely on planeter for
  any of its own.

## 6. Controls × threats

| Control ↓ / Threat → | T-1 | T-2 | T-3 | T-4 | T-5 | T-6 | T-7 | T-8 | T-9 | T-10 | T-11 | T-12 | T-13 | T-14 | T-15 | T-16 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| C-1 signer boundary, bound seals, keys at rest, witness, labels | ● | | | | | | | | | ○ | | ○ | | | | ○ |
| C-2 authentication | | ● | ○ | | | | | | | | | | | | | |
| C-3 default-deny authz, scopes, constitution | | ○ | ● | | | | ○ | | | | | | | ○ | | |
| C-4 title→path, frontmatter subset, bounds, no execution | | | | ● | ○ | | | | ○ | | | | | | | |
| C-5 import handling | | | | ○ | ● | | | | ○ | | | | | | | |
| C-6 CSP, sanitizer, content origin | | ○ | | | | ● | ○ | | | | | | | | | |
| C-7 confidentiality | | | ○ | | | | ● | | | | | | | | ○ | |
| C-8 egress guard | | | | | | | | ● | | | | | | | | |
| C-9 budgets, quotas, metering | | | | ○ | ○ | | | | ● | | | | ○ | ○ | | |
| C-10 source of truth, reconciliation, rebuild | ○ | | | | | | | | | ● | | ○ | | | | |
| C-11 supply chain | | | | | | | | | | | ● | | ○ | | | |
| C-12 attribution, audit | ○ | | | | | | | | | | | ● | | ○ | | |
| C-13 AI outside authority, content as data | | | | | | | | | | | | | ● | ○ | | |
| C-14 visibility, friction, tombstones, constitution, metrics | | | ○ | | | | | | | | | | ○ | ● | ○ | |
| C-15 suppression, procedure, pseudonymity | | | | | | | ○ | | | | | | | | ● | |
| C-16 verified assertions, no key sharing | ○ | | | | | | | | | | | | | | | ● |

(● primary, ○ contributing.)

## 7. What must be tested from day one

INV-2 (a build gate on key reachability and a test that the web tier cannot obtain a signer with keys);
INV-1 and INV-10 (the reconciliation, with a planted unrecorded seal and a planted unsealed acceptance);
INV-4 (the route enumeration test); INV-3 (the title-to-path fuzz, a hostile bundle fixture, an SSRF fixture
per fetch site); INV-5 and INV-6 (the cache-cleared rebuild; a stale badge never served; an open proposal
never rendered as the record).

*End of Threat Model v0.1. Its headline is RR-1, stated so no one discovers it after depending on it: inklkunn
holds its wikis' signing keys by the owner's ruling, and everything in this model is built to make that
custody bounded, detectable, attributable and recoverable rather than invisible.*
