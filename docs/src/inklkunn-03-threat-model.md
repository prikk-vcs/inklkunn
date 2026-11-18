# inklkunn — Threat Model

| | |
|---|---|
| Document | inklkunn Threat Model (security): what inklkunn must defend, against whom, and how, so that the design and the tests can be checked against it. A first-class release deliverable (project rules): updated by any release that touches identity, signing, the prikk boundary, rendering or inbound content; re-verified by the others. |
| Version | **v0.3, 2026-11-18**, four sentences of C-17d and C-17g made exact at the readiness review (0.3.0: RFC 005 part 2, the knowledge layer: A-KNOWLEDGE, T-17 and C-17a – C-17j, INV-11; the November 2026 maintenance: T-18 and C-18a – C-18e, INV-12, the sandbox's host layout (IS-19) and the container settings; RR-12 – RR-14; §7 items for both). Earlier: v0.2, 2026-10-31 (RFC 004: the write path, the lease, the intent journal and recovery; C-1g – C-1k, additions to C-3, C-7, C-9 and C-12, RR-9 – RR-11). Earlier: v0.1, amended 2026-10-29 (C-1d and RR-8: key rotation on prikk as it is, the owner's DR-10 ruling after prikk confirmed it). **Section 7's tests re-verified** at the end of handoff 003 (M1; the record is §7.1) and **at the end of handoff 004 (M2; §7.2)** |
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
- **A-KNOWLEDGE** — the claims and sources beside the pages, their community status, the record of when a source was
  last checked and by which proposal, the alternative views and the claims' threads (RFC 005 part 2). Their value is a
  reader's trust in the answers to *what supports this?* and *do people disagree?*. A forged status, a check carried
  onto a source it was never made for, or a thread shown to someone who may not read it misleads exactly where the
  product promises not to.
- **A-COMMUNITY** — the social system's health: a wiki captured by a faction or buried in spam is as lost as
  a corrupted one (register item 7, 12 of the main transcript).

## 2. Trust boundaries (TB-…)

| ID | Boundary | Direction and trust |
|---|---|---|
| **TB-1** | anonymous internet → inklkunn | untrusted in: every byte attacker-controlled |
| **TB-2** | authenticated principal → inklkunn | semi-trusted, scoped by `authorize()`; an account may be hostile or stolen |
| **TB-3** | proposed and pasted content → rendering and, on acceptance, the repository | untrusted text that becomes a file: path, frontmatter, body, attachment, and (0.3.0) a claim or source file under `knowledge/` |
| **TB-4** | inklkunn → prikk subprocess | inklkunn drives; prikk verifies; the process is a containment boundary for a parser fault |
| **TB-5** | web and API tier → the signer | **the key boundary**: one narrow call in, a sealed result out; no key material crosses it |
| **TB-6** | inklkunn → external services (OpenID Connect, interwiki checks, link previews, webhooks) | every user-influenced URL is an SSRF vector |
| **TB-7** | inklkunn → the viewer's browser | rendered content in a privileged origin: the XSS boundary |
| **TB-8** | operator ↔ inklkunn | trusted; the operator supplies the key-store secret |
| **TB-9** | planeter ↔ inklkunn | semi-trusted peer: identity assertions in, read-only hosting out |
| **TB-10** | imported bundles and archives → inklkunn | untrusted archives: traversal, bombs, hostile frontmatter, and (0.3.0) knowledge files that claim a status or a check |
| **TB-11** | the host and the container runtime ↔ inklkunn's sandboxes | the host's root layout is read to build every sandbox; a container's own settings decide whether bubblewrap can confine at all, and what the server keeps around it |

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
  (SEC-4, OP-07). The secret is supplied at start, not stored beside the keys: the process that opens the key store, the command
  line or the server, reads the file `INKLKUNN_SECRET_FILE` names **at start** (the serving process opens the signer up to three
  times, for the sweep of leftover seed directories, for recovery and for the acceptor, and each copy of the secret
  is zeroized when its signer is dropped); it is never asked for on a
  terminal, never taken from a variable, and a test searches the server's log lines and every page for its value.
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
- **C-1g — Acceptance is one act under the wiki's write lease** (RFC 004 D-4; CON-1). The lease is the signer's
  per-wiki `flock` (so the command line and the server exclude each other) with a durable holder record written at
  the take and cleared at the release, and a fair queue bounded at 64 per wiki; a waiter is told *the wiki is
  busy* with the depth and an estimate, never a surprising prikk `lock conflict`. Archive export and the checkpoint
  refresh take the same lease. Authorization is asked **before** the lease is taken.
- **C-1h — The intent journal: nothing is sealed that was not recorded first.** The acceptance record (*pending
  seal*, with the prepared change's digest) and the intent (acceptance, wiki, view, base, the composed message)
  are written **before** `Accept::accept`; the endings are second, write-once records (the store refuses an
  update, a delete, or an insert over a taken key, by trigger, tested on raw SQL); a retry of the same revision
  finds the record and seals nothing twice (CON-6). Wiki creation and every system change record their act the
  same way, so the reconciliation (C-1b) has nothing to report after any kind of act.
- **C-1i — An overlap is never resolved by the service** (INT-7; HIST-9). The merge is a pure function over
  blocks; an overlap stops the acceptance **before `commit`**, records both texts on the proposal, and leaves the
  repository unchanged.
- **C-1j — Recovery completes or discards, and never guesses** (RFC 004 D-7; INV-7). A wiki whose acceptance was
  incomplete is flagged *needs recovery*: it refuses writes, serves reads, and is recovered at start and on demand
  under the lease. Recovery reads the witnesses (intents, the lease record, prikk `status` and `doctor`, the lock
  listing, foreign processes), builds a **plan that is recorded before any step is taken**, audits each step before
  taking it, and takes only these: *seal alone* a queued patch that matches the intent's message, base and files
  (never commit again), `doctor --discard-damaged-commits` for a torn commit, put the worktree back, clear the
  five lock kinds an acceptance leaves. **A queued patch that matches no intent is a stop, not a seal**: sealing
  it would put an unaccepted change in the history. A doctor or lock report of a shape nobody knows is a stop. **A
  lock's holder is judged by inklkunn's lease record, never by prikk's process check**, which means nothing inside
  the sandbox's PID namespace (ledger PL-8). `verify` and the reconciliation run before the flag is cleared.
- **C-1k — Every recovery act is bound and recorded like an acceptance.** The signer's recovery operations
  need the wiki's lease lent to the calling thread and a recorded recovery act naming the operation; `seal` is
  bound to the pending acceptance or system act it completes.
- **C-1f — Honest labels.** Every change is labelled *signed by the wiki service on behalf of …* (ID-5): the
  reader is never told a person signed.

### T-2 — Authentication and session attacks
Credential stuffing, session theft, OpenID Connect misconfiguration, token leakage (A-CREDS).

- **C-2a** Argon2id passwords; OpenID Connect with PKCE, one-shot state, nonce-bound tokens verified against
  the provider's keys; constant-work sign-in so account existence does not leak by timing (planeter's IS-31
  lesson, adopted from the start).
- **C-2b** Per-account and per-client-IP throttles; the client IP only from a declared trusted proxy.
- **C-2c** `Secure`, `HttpOnly`, `SameSite` cookies (the session cookie is `__Host-`); the session's CSRF synchronizer
  token on every cookie-authenticated write, checked in one layer in front of every route and before any wiki is looked
  up, with the same-site check as the second wall (a double-submit token guards only the pages that make a session:
  sign-in, joining, an invitation); a request that presents a bearer token is judged by the token alone and never also by
  a cookie; scoped, hashed, expiring, revocable tokens; one-time, expiring, hashed invitations (accounts are never
  matched or linked by e-mail address); re-authentication for key operations and role grants.
- **C-2d** A second factor is a stated gap for v1 (RR-4); an OpenID Connect provider that enforces it is the
  interim answer.

### T-3 — Authorization bypass and confused deputy
A reader reaches a private wiki, a proposer accepts their own proposal, a token with `read` proposes, a role
is escalated through a policy edit (A-AUTHZ, A-CONTENT).

- **C-3a** One pure default-deny `authorize()` on every path (ID-7); a test enumerates every route and verb
  and asserts the call (planeter's ENF-4, written before the first write handler).
- **C-3b** Scopes intersected with roles per endpoint; existence-neutral denials. The write API checks the
  endpoint's action against the authorizer (token scope ∩ role) **before the body is read**; a token with
  `propose` cannot accept even when its account is a maintainer; an unauthenticated token is `invalid-token`
  before any wiki lookup; a private wiki's write endpoints answer 404 byte for byte as an absent wiki does.
  A proposer accepts their own proposal only where the policy allows (a wiki with fewer than three maintainers).
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
- **C-9d** (RFC 004 D-11) The proposal quotas (open proposals, revisions, proposals per hour per principal and per
  wiki, accepted bytes per day) refuse **by name** with the limit, the current value and the reset time; new
  accounts get half; a verification check is claimed per wiki (one at a time) and capped overall, and a check that
  could not run never replaces a verdict (R1).
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
- **C-11b** Release provenance: the release tag is signed by the owner's key (readers check it with
  `git verify-tag`), and every binary and image carries a keyless build attestation naming the commit and the
  workflow run. CI does not re-check the tag's signature (owner, 2026-10-29: little gain for its cost), so a
  release is as trustworthy as push access to the repository.

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

### T-17 — Hostile or misleading knowledge (RFC 005 part 2; TB-3, TB-10)
Claims and sources are text from strangers that a reader is invited to trust. The ways they mislead:
- **Status without a decision:** a status, a review day or a link set outside a curation proposal, or by an import.
- **Check laundering:** a source's *last checked* day, or its *can be had* answer, carried onto a new reference that
  nobody checked, by moving the reference and keeping the old day.
- **A relation lost or invented** when two proposals merge.
- **A malformed file** that half-reads.
- **Spoofed text:** direction controls that make a reference or an identifier read as something else.
- **A reference or archive address** used as a link to a hostile site, a tracker, or an internal address.
- **A copy of a source** kept against its licence.
- **Views and threads:** a view made by anyone, a view called *canonical*, a thread that leaks a restricted wiki or
  changes a claim.
- **The provenance walk** (*Why is this here?*) or the knowledge index used to exhaust the server.

(A-KNOWLEDGE, A-COMMUNITY, A-CONTENT, A-AVAILABILITY.)

- **C-17a — Knowledge changes only through accepted proposals** (D-3).
  - Every claim and source file is written by an acceptance, like a page.
  - Community status, a source's review day and its *can be had* answer change only through a **curation proposal**.
  - A page-edit proposal, an import and every system act that touches them are refused by name.
- **C-17b — A check is stated, never inferred or carried** (D-3; the clarification *correcting a source*; K72 – K77).
  - When a source's reference moves (white space alone is not a move), its old check is cleared.
  - A check of the new place counts only when **this** proposal states it: `last_reviewed.proposal` names the proposal
    itself. One rule decides it at submit, at every revision and at acceptance.
  - Acceptance also holds the bounds: not before the previous review day, not after today in UTC+14, and an answer
    only with such a day.
  - An old `reviewed` or `availability` sent beside a move is refused by its field, never dropped.
  - Imports are judged as the service (no curation), so they cannot move a checked source or state a check.
- **C-17c — Validation before the signer is asked; never half-read** (D-5).
  - The YAML subset of C-4b, the schema, whole identifiers, and references to claims, sources and blocks that exist.
  - A refusal names the file and the rule.
  - A repository that holds invalid knowledge files (planted, or from before a rule) still serves its pages, lists the
    files in the health list, and reads none of them.
- **C-17d — Text is data, escaped, and cannot impersonate.**
  - Every field is printed escaped, with its white space collapsed.
  - The twelve bidirectional controls are refused in knowledge text (the signer's `is_direction_or_format`). The other
    format characters (ZWNJ, ZWJ and those real text needs) stay as written. The wider Cf scrub is for subprocess
    output only: bubblewrap's reason line and `curl`'s stderr.
  - Identifiers are shown whole, in `<code>`, wherever two sources could share a title.
  - **A reference is shown as text, never as a link. A snapshot's archive address and attachment path are not shown at
    all. Nothing is fetched.** No crawler, no link check, no preview: the knowledge layer adds no fetch site, so T-8 is
    unchanged.
  - A snapshot kept as an attachment is served like any attachment, from the content origin (C-6c).
- **C-17e — The knowledge files merge and never resolve** (D-4).
  - Key by key; an overlap stops with both texts, as C-1i does for pages.
  - A relation is never lost, and a check is never carried across a reference that moved on either side.
- **C-17f — Views** (D-9; the owner's ruling of 2026-10-31).
  - Creating an alternative view needs a reviewer or above. The creation is recorded as an act before prikk is asked,
    and a killed creation flags the wiki (C-1h, C-1j).
  - *Compare views* shows differences in words; *canonical* appears nowhere a reader sees.
- **C-17g — Threads** (D-11).
  - Append-only records (the store refuses an update, a delete and a replacing insert, by trigger), read under the
    claim page's own `authorize()` (C-7a).
  - A comment raises no notification in 0.3.0, so nothing of a thread reaches an inbox (C-7b holds trivially).
  - A thread never changes a claim; only a curation proposal does.
  - Exported as one JSON line per comment, the body a fenced text block, so no Markdown reader of the bundle sees a
    construct in it.
- **C-17h — Derived, bounded, rebuildable** (D-6, D-8).
  - The knowledge index is built from the same `extract` as the title index, bounded the same way
    (`INKLKUNN_INDEX_MAX_MIB`, the file count), never authoritative (INV-5), and rebuilt with the derived store cleared.
  - *Why is this here?* walks the history within the history page's bounds, says so beyond them, and labels a chain
    traced by matching text.
- **C-17i — No word or number says *true*** (D-7; INV-6). The retired-words test holds both languages. *Verified* stays
  prikk's word for the history alone. *Bearing* describes how a source speaks to a claim, not how good either is.
- **C-17j — Scratch and export paths take nothing that is not theirs** (K74; TB-4).
  - Scratch extracts are held by `flock` on a lock file beside each, with an inode check.
  - An export's leftover partial beside its destination is swept only when its own lock file exists and its name has
    the three-part tail. A user's directory that looks like one is left alone.
  - Network filesystems stay unsupported (RR-10).

### T-18 — The sandbox and the container (TB-4, TB-11; IS-19)
prikk and `curl` run in bubblewrap so that a parser fault, or a hostile response, stays contained (C-9a, RR-3). Three
ways that containment can fail or weaken:
- **The sandbox does not fit the host,** so nothing runs confined and the operator reaches for the development flag.
  This was IS-19: 0.1.0 and 0.2.0 laid out `/lib64` as Arch does, and could not confine on Debian or Ubuntu.
- **The layout itself exposes the host:** a link followed into a bind, or a host path outside the system directories.
- **A container weakens around the server:** the settings bubblewrap needs inside a container take away the
  container's own layers.

(A-HOST, A-KEYS, A-AVAILABILITY.)

- **C-18a — One layout, taken from the host, failing closed.**
  - `sandbox::root_layout` builds the root of every sandbox: the probe, prikk's, and curl's vector. It binds `/usr`
    read-only. For `/bin`, `/sbin`, `/lib`, `/lib64` and `/lib32` it reads the host's entry **without following it**: a
    link is recreated with its own target, so one that leads outside the bound tree dangles; a real directory is bound
    read-only; an absent one is left out.
  - Nothing else of the host is added: no home, no `/etc`, no repository but the call's own.
  - A host where the loader is elsewhere (NixOS, for example) fails closed, with bubblewrap's reason.
- **C-18b — The program bound is the program run.** prikk's binary and `curl` are bound at, and run as, the file their
  path resolves to (`bind_target`). bubblewrap already followed a bind's source, so the content is the same file; a
  link inside a mounted tree is no longer refused as a destination.
- **C-18c — Confined or refused.**
  - The server and every command probe bubblewrap at start, and refuse to run prikk when it cannot confine. The
    refusal carries bubblewrap's own first line.
  - There is no fallback but `--dev-unconfined` (an argument, never an environment variable), which prints a banner.
  - A memory ceiling hit, in prikk or in bubblewrap's own loader, is *unavailable*, never a verdict and never a missing
    prerequisite (C-9a; M48).
- **C-18d — The container's settings are stated with their cost, and the hardening is run** (SECURITY.md, *What a host
  needs for confinement*).
  - In a container, bubblewrap needs `seccomp=unconfined`, `apparmor=unconfined` and `systempaths=unconfined` (docker's
    names). These take away the runtime's layers for the **whole** container: the server meets the kernel's whole
    system-call surface. Bubblewrap around prikk and `curl` is then the confinement inside the container (RR-12).
  - What can be added back is measured and run, not assumed: `--cap-drop ALL`, `--security-opt no-new-privileges` (the
    image's setuid programs gain nothing), and `--read-only` with `--tmpfs /tmp`.
  - The alternative, running on the host, needs only user namespaces.
- **C-18e — The release proves it before it publishes.**
  - CI runs the confined tests on Ubuntu with no bubblewrap skip line allowed. A guarded skip fails the run, and so does
    a missing bubblewrap.
  - The release workflow runs the built image once, under docker with the settings and the hardening of C-18d, and
    publishes only if `wiki create` succeeds. That command cannot succeed unless prikk is confined. The rehearsal of
    2026-11-17 (run 44103325108) did it on Debian 12, amd64 and arm64.

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
- **INV-11 — No knowledge without a decision.**
  - A claim or source changes only through an accepted proposal.
  - Its status and its checks change only through a curation proposal that states them itself.
  - Nothing the knowledge layer shows is fetched, and no reference is a link.
- **INV-12 — Confined or refused.** prikk and `curl` run inside bubblewrap on every supported host layout. Otherwise
  the server refuses to run them, unless an operator passed the development flag and the banner says so.

## 5. Residual risks and assumptions (RR-…, ASSUME-…)

- **RR-1 — The service holds the keys.** A total compromise of the host or of the key-store secret lets an
  attacker seal history that prikk accepts, in every wiki whose keys they obtained. The ruling DR-4 accepts
  this for the sake of keyless participation; the controls make it bounded (per-wiki keys), detectable
  (reconciliation, independent witness), attributable (every seal says it is the service's) and recoverable
  (rotation). This is the central difference from planeter and must be said plainly in SECURITY.md.
  **The serving process now opens the key store** (it did not in 0.1.0, whose server held no write signer): it reads
  the operator's secret file at start and decrypts a wiki's seed onto the tmpfs for the one signing call that needs
  it, so the secret and the sealed records are within reach of the process that parses visitors' requests, and a
  compromise of that process is a compromise of every wiki's keys, as a compromise of the host always was. The
  controls that bound it are unchanged and remain the ones that matter here: the signer is reached only through
  `Accept` (C-1a), refuses any seal not bound to a recorded acceptance (C-1b), is opened `with_leases` over the one
  lock the command line uses, and every call runs in the sandbox under a memory ceiling with only the seed it signs
  with. There is no second signer process (IQ-4 stays). A server started without the secret file never holds a key.
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
- **RR-9 — Recovery stops where it cannot be sure.** A sound queued patch that matches no recorded intent, a
  lock of a kind an acceptance does not leave, or a doctor report of an unknown shape leaves the wiki flagged
  and read-only until a person decides: automating the choice would either seal an unaccepted change or hide
  damage. The plan is recorded either way.
- **RR-10 — The lease is per host.** The `flock` and the in-process queue exclude every process on one host; a
  multi-host deployment on shared storage needs a shared lease (the same operations RFC as RR-7). Network filesystems
  (NFS, SMB, any `local_lock` setup) are **unsupported** for the data directory: there `flock` may be per process or
  local to one host, so two hosts can each hold the same lock, and nothing detects it. Recovery's foreign-process check
  reads `/proc` and sees only its own PID namespace, so the operator stops every process that writes the repository, in
  any container, before a recovery.
- **RR-11 — Accepting is served once sign-in and the acceptor are configured.** The server builds its acceptor at
  start (`inklkunn/src/serve/acceptor.rs`) when the operator's secret file is there and the key store opens; it then
  accepts proposals for the people its sign-in identifies, under the same write lease as the command line (C-1g), and
  asks for the title index and a verification after each seal off the request path, on one worker with a bounded
  queue. A server started **without** the secret file, or whose key store does not open, still serves every read,
  **still stores proposals and comments**, and refuses **every change to the history** (an acceptance) with one
  sentence (*this server is not set up to sign changes into the history …*), prints one
  line at start naming the setting, never asks for the secret on a terminal, and does not retry behind a visitor's
  back. Until the sign-in pages and the real `Identify` are served (handoff `serve-sign-in-and-accept`, U1 – U4, U6)
  every visitor is anonymous and `Accept` is refused for want of an account; this model is re-verified when they are
  (U6).
- **RR-12 — A container that runs bubblewrap gives up its own layers.**
  - With `seccomp=unconfined`, `apparmor=unconfined` and `systempaths=unconfined`, a compromise of the server process
    meets the kernel's whole system-call surface, including the creation of user namespaces, which is a common entry
    for kernel exploits.
  - The hardening of C-18d takes back capabilities, new privileges and a writable root, but not the system-call
    filter.
  - An operator who cannot accept this runs the server on the host, where bubblewrap needs only user namespaces.
    SECURITY.md says so.
- **RR-13 — Knowledge is a social record, not a proof.**
  - inklkunn records who stated a claim, a source and a check, and what the community decided. It cannot know whether
    a source says what a claim says, or whether a check was honest.
  - Citation laundering and source flooding are bounded by attribution, review and the curation rules (T-14, C-17a,
    C-17b), not prevented.
  - A snapshot kept as an attachment *only where its licence allows* is the reviewer's judgement; no code reads a
    licence (T-15 covers its removal).
- **RR-14 — A restricted wiki's raw source is not served when the content origin has its own host** (IS-18). The
  session cookie does not cross hosts, so the raw source answers *not found* to its own members. This fails closed: a
  loss of availability, not of confidentiality. The page itself reads, and a way across is designed with RFC 006.
- **ASSUME-1** The operator and the host are trusted; inklkunn defends against network actors and malicious
  users, not a hostile operator.
- **ASSUME-2** prikk's offline `verify` is correct and is the arbiter of history integrity.
- **ASSUME-3** planeter, when integrated, enforces its own invariants; inklkunn does not rely on planeter for
  any of its own.

## 6. Controls × threats

| Control ↓ / Threat → | T-1 | T-2 | T-3 | T-4 | T-5 | T-6 | T-7 | T-8 | T-9 | T-10 | T-11 | T-12 | T-13 | T-14 | T-15 | T-16 | T-17 | T-18 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| C-1 signer boundary, bound seals, keys at rest, witness, labels | ● | | | | | | | | | ○ | | ○ | | | | ○ | ○ | |
| C-2 authentication | | ● | ○ | | | | | | | | | | | | | | | |
| C-3 default-deny authz, scopes, constitution | | ○ | ● | | | | ○ | | | | | | | ○ | | | ○ | |
| C-4 title→path, frontmatter subset, bounds, no execution | | | | ● | ○ | | | | ○ | | | | | | | | ○ | |
| C-5 import handling | | | | ○ | ● | | | | ○ | | | | | | | | ○ | |
| C-6 CSP, sanitizer, content origin | | ○ | | | | ● | ○ | | | | | | | | | | ○ | |
| C-7 confidentiality | | | ○ | | | | ● | | | | | | | | ○ | | ○ | |
| C-8 egress guard | | | | | | | | ● | | | | | | | | | | |
| C-9 budgets, quotas, metering | | | | ○ | ○ | | | | ● | | | | ○ | ○ | | | ○ | ○ |
| C-10 source of truth, reconciliation, rebuild | ○ | | | | | | | | | ● | | ○ | | | | | ○ | |
| C-11 supply chain | | | | | | | | | | | ● | | ○ | | | | | ○ |
| C-12 attribution, audit | ○ | | | | | | | | | | | ● | | ○ | | | ○ | |
| C-13 AI outside authority, content as data | | | | | | | | | | | | | ● | ○ | | | | |
| C-14 visibility, friction, tombstones, constitution, metrics | | | ○ | | | | | | | | | | ○ | ● | ○ | | ○ | |
| C-15 suppression, procedure, pseudonymity | | | | | | | ○ | | | | | | | | ● | | | |
| C-16 verified assertions, no key sharing | ○ | | | | | | | | | | | | | | | ● | | |
| C-17 knowledge by proposal, stated checks, validation, text as data, merge, views, threads, bounds | | | | | | | | | | | | | | | | | ● | |
| C-18 host layout, bound program, confined or refused, container cost, release proof | | | | ○ | ○ | | | | ○ | | ○ | | | | | | | ● |

(● primary, ○ contributing.)

## 7. What must be tested from day one

INV-2 (a build gate on key reachability and a test that the web tier cannot obtain a signer with keys);
INV-1 and INV-10 (the reconciliation, with a planted unrecorded seal and a planted unsealed acceptance);
INV-4 (the route enumeration test); INV-3 (the title-to-path fuzz, a hostile bundle fixture, an SSRF fixture
per fetch site); INV-5 and INV-6 (the cache-cleared rebuild; a stale badge never served; an open proposal
never rendered as the record).

**Added by v0.3 (0.3.0):**
- **INV-11:** each path that could change a claim or a source without an accepted proposal, or a status or a check
  without a curation proposal that states it, is refused (a test per path: a page edit, an import, a system act, the
  generic write API; a check carried across a moved reference, at submit and at acceptance).
- **C-17c:** a repository with planted invalid knowledge files serves its pages and lists the files.
- **C-17d:** a reference is text, never `href`, and an archive address is not rendered; a bidirectional control in
  knowledge text is refused.
- **C-17g:** a restricted wiki's thread through every route answers as an absent wiki does (the route enumeration).
- **C-17i:** the retired-words test in both languages.
- **INV-12, C-18a:** `root_layout` for planted Arch-style, Debian-style and mixed roots; the probe and prikk's sandbox
  tied to it; the real probe; CI's confined tests on Ubuntu with no guarded skip.
- **C-18e:** the release workflow's image step.

The record for these is §7.4, written at 0.3.0's readiness and re-verified by the architect.

### 7.1 Verification record (2026-10-29, handoff 003 T9)

Every item of §7 below was re-run with the workspace test suite (`cargo test --workspace --locked`, prikk 0.55.0 on
`PATH`, and again with prikk shimmed off, where the prikk-dependent tests print their `skipping: prikk not on PATH`
line and the count is held to `EXPECTED_SKIPS`). A row says which tests carry the item and what they do **not**
cover; nothing is claimed beyond that. The dev team wrote this record; the architect's review re-verifies it.

| Item | Tests that carry it | Not covered |
|---|---|---|
| **INV-2** keys reachable from one component only; the web tier cannot obtain a signer with keys | `tools/boundary-check/tests/gates.rs`: `only_core_depends_on_the_signer_and_no_product_crate_enables_a_test_feature`, `key_reachability_gate_catches_planted_dependencies_and_features`, `the_key_store_module_is_private_unmentioned_and_the_signer_names_stay_in_the_signer`, `a_web_handler_receives_a_principal_it_never_builds` (each gate is shown to fail on a planted violation); the compile-fail doctests of `IdentityAuthority`, `AccountPrincipal` and `KeyAuthority` (`cargo test --doc`) | the running service's memory; the tests are the build graph and the types |
| **INV-1, INV-10** the reconciliation, with a planted unrecorded seal and a planted unsealed acceptance | `inklkunn-core/tests/reconcile.rs`: `an_unrecorded_seal_and_an_unsealed_acceptance_are_both_reported_and_nothing_is_changed`; `src/reconcile/tests.rs` | the schedule (it is run by the write path of RFC 004, which does not exist yet) |
| **INV-4** the route enumeration | `inklkunn-web/tests/route_enumeration`: `every_guarded_route_answers_a_restricted_wiki_exactly_as_one_that_is_not_there` walks the route registry (application and content origins), asks every guarded route of a restricted and of an absent wiki and requires the same refusal, byte for byte but for the echoed address (mutation: `open()` ignoring its decision is caught); `wiki_pages::a_private_wiki_and_a_wiki_that_is_not_there_are_the_same_answer`; the API's `a_wiki_you_may_not_read_answers_every_endpoint_exactly_as_a_wiki_that_is_not_there`; `boundary-check`: `routes_are_registered_in_the_registry_file_and_nowhere_else` (a planted bare route fails) | an authenticated principal with a role: sign-in is not served in this release, so only anonymous requests reach the router |
| **INV-3** the title-to-path fuzz | `inklkunn-markup/tests/path_props.rs` (property tests, with the regression file) | |
| **INV-3** a hostile bundle fixture | `inklkunn-core/src/import` tests and `crates/inklkunn/src/admin` tests: `a_hostile_folder_is_refused…` (links, device names, non-UTF-8 name and content, bad frontmatter, over-cap path, case collision, duplicate id, socket, non-ASCII name; every reason named, nothing written) | the Portable Wiki Bundle import (RFC 007); this is the folder import (RFC 003 D-12) |
| **INV-3** an SSRF fixture per fetch site | `inklkunn-core/src/egress/tests.rs` (24 refused literal ranges, schemes, the pin), `inklkunn-fetch/tests/standin.rs` and `real_vector.rs` (the confined `curl`: no redirect followed, caps, deadline) | the only fetch site in this release is the OIDC fetcher, which is not switched on |
| **INV-5** the cache-cleared rebuild | `inklkunn-core/src/read/tests/cache.rs`: `the_cache_cleared_and_every_view_rebuilt_yields_identical_pages`; `tests/read_path.rs` (real prikk) | |
| **INV-5, INV-6** a stale badge is never served | `wiki_pages::the_badge_is_the_records_and_never_the_caches`; `…when_prikk_is_busy_a_page_with_no_copy_is_unavailable_and_one_with_a_copy_is_served_stale` (a page served stale says *Could not be checked now* whatever the record says); the derived cache has no field for a verification (a type-level rule, `inklkunn-store/tests/derived.rs`) | |
| **INV-6** a memory limit is never a failed verification | `inklkunn-prikk` `verdict_of` unit tables; the real-prikk sweep `inklkunn-signer/tests/verify_verdicts.rs` (fails on the old driver); `inklkunn-core/src/check/tests.rs` | |
| **INV-6** an open proposal is never rendered as the record | **not testable yet**: proposals arrive with RFC 004. What exists: the `Assurance` values and the retired-words and glossary tests, and the change page that says only *Signed into the history* and who the message names (`wiki_history`: `a_change_the_service_made_itself_and_one_nobody_is_named…`) | the proposal states themselves |

Also re-run: the structural, catalog, glossary, retired-words and stylesheet tests on every page state in both languages
(`inklkunn-web/tests/structure`, `catalog_checks`, `glossary`, `retired_words`, `stylesheet`), the sanitizer tests
(`inklkunn-core/tests/sanitizer.rs`, `read/sanitize/tests.rs`, `inklkunn-markup/tests/author_html.rs` for IS-14), and
the supply-chain gates (`cargo deny check`, `cargo audit`).

### 7.2 Verification record (2026-10-31, handoff 004 T8)

The items RFC 004 adds to §7, re-run with the workspace test suite (prikk 0.55.0 on `PATH`, and again with prikk
shimmed off, where the count of `skipping: prikk not on PATH` lines is held to `EXPECTED_SKIPS`). As in §7.1, a row
says which tests carry the item and what they do **not** cover. The dev team wrote this record; the architect's
review re-verifies it.

| Item | Tests that carry it | Not covered |
|---|---|---|
| **INV-1, C-1g** an acceptance seals once, under the lease; busy with a depth; unauthorized refused before the lease | `inklkunn-core/tests/acceptance.rs` (real prikk): `an_acceptance_whose_base_is_the_tip_seals_and_the_log_shows_the_message`, `two_acceptances_racing_on_one_wiki_serialize_and_the_second_is_told_busy_with_a_depth`, `a_retried_accept_of_the_same_revision_seals_once`, `an_unauthorized_acceptor_is_refused_before_the_lease_is_taken`, `an_archive_export_and_a_checkpoint_refresh_hold_the_wiki_lease_and_an_acceptance_waits_for_them`; `inklkunn-core` `lease` and `acceptance` unit tests | Two **processes** racing (the unit tests race threads; the `flock` itself is tested in the signer) |
| **INV-1, INV-10, C-1b, C-1h** the reconciliation after every kind of act | `acceptance.rs`: `the_reconciliation_finds_nothing_after_every_kind_of_act`, `import_folder_produces_one_block_with_kind_import`; `create_wiki.rs`: `a_created_wikis_first_change_is_a_recorded_act_and_the_reconciliation_reports_nothing`; `inklkunn-core/tests/reconcile.rs` | A production history written before RFC 004 (no act rows): the reconciliation reports it, by design |
| **C-1h** the journal's records are write-once | `inklkunn-store/tests/acceptance_records.rs`, `acts.rs`, `recovery_acts.rs`, `proposals.rs`: raw-SQL refusals of an update, a delete and an insert over any taken key, per table, on both stores | A hand-edited database file (the triggers guard code paths, not someone with the file; the audit log note applies) |
| **C-1i** an overlap stops before `commit` and records both texts | `acceptance.rs`: `an_overlap_stops_before_commit_records_both_texts_and_the_repository_is_unchanged`; `inklkunn-core/tests/merge_corpus.rs` (the 120 spike pairs: exactly 99 clean, 21 overlaps, both texts); `merge/tests.rs` properties | Within-block merging (deferred by D-5 with a measurement hook) |
| **C-1g** failures: before `commit` nothing changes; after `commit` the wiki is flagged and reads are served | `acceptance.rs`: `a_failure_injected_before_commit_leaves_the_repository_unchanged_and_the_record_failed_with_the_word`, `a_failure_injected_after_commit_flags_the_wiki_and_reads_still_serve` | |
| **RR-1, RR-11, C-1c, C-1g** the serving process opens the key store; a server without the secret file reads, stores proposals and comments, and refuses every change to the history with one sentence; the command line and the server exclude each other; no secret reaches a log or a page | `inklkunn/src/serve/acceptor/tests.rs` (real prikk, real key store, a real socket): `a_start_without_the_secret_file_serves_a_page_and_refuses_a_write_with_one_sentence`, `a_start_with_the_secret_accepts_a_proposal_end_to_end_and_no_secret_reaches_a_log_or_a_page`, `a_secret_that_does_not_open_the_key_store_is_the_same_server_with_one_line_that_names_no_secret`, `a_wiki_the_command_line_holds_makes_the_servers_acceptance_busy_and_it_accepts_when_it_lets_go`, `a_wiki_the_server_is_signing_is_refused_to_the_command_line_by_name`, `a_wiki_flagged_needs_recovery_refuses_the_accept_though_the_signer_is_open`, `the_acceptor_is_built_after_the_start_up_work_has_recovered_a_flagged_wiki`; `serve/follow_up/tests.rs` (one worker, a bounded queue); `inklkunn-web/tests/proposals/accept.rs`, `tests/api/write.rs` (the sentence and the `signing-not-configured` problem) | The sign-in pages and the real `Identify` (U1 – U4): the tests stand a header-named identity behind the real router. A key store that appears after the start (a first `wiki create` while the server runs) is used from the next start |
| **C-1j** the kill test: SIGKILL at each step of an acceptance, then recovery | `inklkunn-signer/tests/acceptance_kill.rs`: `a_sigkill_at_each_step_of_an_acceptance_is_recovered_with_nothing_lost_or_made_twice` (fourteen kill points; the matrix is in the review request); `inklkunn-core/tests/recovery.rs` (15 tests: a queued patch completed, a torn commit discarded, a foreign queued patch a stop, a stale lock cleared even when prikk's pid check says alive (PL-8), an unknown lock kind a stop, no recovery without the lease, an interrupted recovery resumed, unreached-object warnings recorded, an unknown doctor shape a stop) | A kill of **recovery itself** at every step (one interrupted recovery is tested, not each step); a real SIGKILL inside each prikk call of the *creation* is the signer's `creation_kill.rs` |
| **C-1k** recovery operations need the lease and a recorded act | `inklkunn-signer/tests/recover.rs` (8, real prikk); `recovery_never_runs_without_the_lease_and_a_busy_wiki_is_told_so_with_nothing_done` | |
| **INV-4** the route enumeration covers every proposal route, the inbox, the watch form and every write endpoint | `inklkunn-web/tests/route_enumeration`, `routes::tests::every_route_that_can_show_wiki_data_is_guarded_and_the_rest_are_public`, the OpenAPI drift tests in both directions (`tests/api`) | |
| **C-3b** scope ∩ role; existence-neutral 404 on writes | `inklkunn-web/tests/api/write.rs` (a token with `propose` cannot accept, a private wiki answers as an absent one for four kinds of caller, an unauthenticated token is `invalid-token` first, `Retry-After`) | A real bearer token resolved from the store: sign-in is not served yet (RR-11); the test identity stands in |
| **INV-6** an open proposal is never rendered as the record | `inklkunn-web/tests/proposals/flows.rs` (`submitting_makes_a_proposal_held_for_review_with_the_text_that_was_sent`) and `review.rs` (`an_accepted_proposal_says_it_is_signed_into_the_history_and_links_to_its_change`: only an accepted proposal says so); the structural harness's proposal states and accept outcomes in both languages | |
| **T-7** a draft is its author's alone; the inbox leaks nothing to a non-reader | `inklkunn-store/tests/proposals.rs` and `inklkunn-core` `proposal/tests/drafts.rs` (a draft is invisible to another account and survives a restart); `inklkunn-web/tests/proposals/flows.rs` (`the_list_shows_what_was_sent_and_a_draft_only_to_its_author`, `a_draft_survives_a_session_and_is_resumed_with_its_text_and_summary`); `inklkunn-web/tests/inbox_watch` (a wiki restricted after the event leaks nothing) | |
| **T-4, INV-3** hostile proposed content is data | `inklkunn-web/tests/proposals/safety.rs` (hostile content in preview, diff output and the page), `tests/api/write.rs` (hostile JSON stored as text, escaped on output) | |
| **C-9b, C-9d** every quota refuses by name | `inklkunn-core/src/proposal/tests/quotas.rs` (both sides of each boundary); `inklkunn-core/src/check/tests.rs` (one check per wiki, the cap, a time in the future) | |

### 7.3 Verification record (2026-11-03, sign-in handoff U6: writes switched on)

The two tests of `crates/inklkunn/src/serve/writes_on/` (real prikk, a real key store, a real SQLite store, a real
socket; the binary's own `prepare` with its own `Identify`, nothing stood in) close the cells that §7.1 and §7.2 left
open because sign-in was not served: *a real bearer token resolved from the store* (C-3b), *an authenticated principal
with a role* (INV-4), and the stand-in identity of the RR-1 / RR-11 row. Accounts are made by the operator's
invitation and its page, sessions by `/-/sign-in` with a cookie jar that obeys the `__Host-` rules and the forms read
off the pages (a text area is sent with CR LF, as a browser sends it).

| Item | Tests that carry it | Not covered |
|---|---|---|
| **T-2, C-2a** sign-in through the page; the session cookie is `__Host-`, `Secure`, `HttpOnly`, `SameSite=Strict`, `Path=/`, no `Domain`; the double-submit cookie is spent; a wrong password and a forged double-submit token are refused; an invitation link works once; an ended session is no session | `writes_on::tests::an_invited_person_signs_in_proposes_is_reviewed_and_accepted_and_the_pages_say_so` | The throttles behind a proxy (G1 – G3), OpenID Connect against a real provider (F2) |
| **C-2b** a write by a signed-in person needs the session's CSRF token: a forged one, another session's, none, an empty one, and a post from another site each change nothing | the same test (mutation: the CSRF layer's `needs_token` made always false fails it at the forged token) | A cross-site form post in a real browser |
| **C-3a, C-3b** scope ∩ role: a `propose` token cannot review or accept, a `review` token cannot accept, a contributor's `accept` token cannot review or accept, an invalid token is `invalid-token`, a cookie with no CSRF token cannot write through the API | `writes_on::tests::api::a_script_with_scoped_tokens_proposes_comments_reviews_and_is_accepted_and_a_narrow_token_is_refused` | Token creation through the account page (G6 changes it) |
| **D9** nobody approves or asks for changes on their own proposal (a contributor is refused by role, a reviewer by D9, through the page and the API) | both tests | The page still offers the form to a proposer who is a reviewer; it is refused in words (observation, not a hole) |
| **INV-1, C-1g** propose, comment, approve, accept: one new block in `prikk log`, *Signed into the history* on the proposal, the change and the history; a retry writes nothing; a proposal whose base moved is re-authored (`reauthored: true`); an overlap stops with both texts and the repository unchanged | both tests | A kill during a web accept (the kill matrix of §7.2 stands) |
| **INV-5, INV-6** an open proposal is not the record (the page every reader sees and its badge are unchanged until acceptance); after an acceptance the badge comes from a check that ran afterwards (the verification page says *Verified*, with a time at or after the acceptance) | the first test (mutation: the follow-up's verify job made to return fails both) | |
| **RR-1, RR-11** the serving process opens the key store and no secret, password or cookie value reaches a log, a note or a page | the first test (the search covers what the operator and every visitor saw) | A core dump (A1: non-dumpable, `LimitCORE=0`) |

Also re-run: the structural, catalog, glossary, retired-words and stylesheet tests on every page state in both
languages; the sanitizer tests; the supply-chain gates (`cargo deny check`, `cargo audit`).

### 7.4 Verification record (2026-11-18, 0.3.0 readiness R-5; re-verified at R-6)

The items that "Added by v0.3 (0.3.0)" names, each test found by its name in the workspace and run with
`cargo test -p <crate> <name>` on the pinned compiler (1.99.1) with prikk 0.55.0 first on `PATH`. None of them printed
a `skipping:` line, so the tests that need a real prikk or a real bubblewrap ran. As in §7.1 – §7.3 a row says which
tests carry the item and what they do **not** cover; nothing is claimed beyond that, and where a path the threat model
names has no test, the cell says so. A file is named from `crates/` (`tools/` and `.github/` in full); a crate's unit
tests sit beside its code, its integration tests under `tests/`. The dev host's own root is Arch's (`/lib64` is a link
to `usr/lib`), so the tests that use the host's real bubblewrap ran on that layout only; the Debian and Ubuntu layouts
are the planted roots below, CI's Ubuntu run and the release image (the last two rows). The workspace-wide runs (with
prikk, and with prikk shimmed off) belong to the readiness report and are not repeated here. The dev team wrote this
record; the architect's review re-verifies it.

| Item | Tests that carry it | Not covered |
|---|---|---|
| **INV-11, C-17a** a **page edit** cannot set a status, a check, a link or a removal that only a curation proposal may; refused when sent, at every revision and at acceptance, before the signer | `inklkunn-core/src/knowledge/tests/curation.rs`: `a_page_edit_that_also_sets_a_status_is_refused_and_the_signer_is_not_asked`, `a_new_claim_that_arrives_decided_in_a_page_edit_is_refused_too`, `a_decided_claim_is_not_removed_by_a_page_edit`, `a_status_beside_a_page_edit_is_refused_when_it_is_sent_and_at_every_revision`, `every_way_the_acceptance_refuses_is_refused_when_it_is_sent` (a source's availability, each field of an existing link, a link taken away, a decided claim removed: each refused as `KnowledgeCurated` at submit and at revision), `a_review_date_is_named_by_the_proposal_that_set_it_and_only_a_curation_may_set_it`; `inklkunn-core/src/knowledge/tests/status.rs`: `a_new_claim_that_arrives_decided_is_refused_unless_the_proposal_is_a_curation`, `a_change_of_status_in_a_claim_that_exists_is_refused_unless_the_proposal_is_a_curation`, `a_sources_review_and_availability_are_set_by_a_curation_proposal_only`, `the_rule_the_page_and_the_submission_use_is_the_one_the_acceptance_uses`, `the_acceptance_refuses_the_same_outside_a_curation_proposal`; on a real history (real prikk), `inklkunn-core/tests/acceptance.rs`: `a_page_proposal_does_not_take_a_claim_and_its_links_away_and_a_curation_proposal_may` (sent: refused; accepted through the service: refused with `Rule::Status` naming the claim file, the history unchanged; the same files as a curation proposal are signed) | Someone with the repository's files rather than the service: the reconciliation (§7.1) finds a change nobody accepted, and does not judge knowledge text. The unit tests use a signer stand-in and a fake repository; only the last test is end to end. A rollback proposal that restores knowledge files: no test rolls back a change that wrote them |
| **INV-11, C-17a** an **import** sets no claim, source, status or check that only a curation proposal sets | `inklkunn-core/src/import/tests/knowledge.rs`: `an_import_sets_nothing_that_only_a_curation_proposal_sets` (a claim arriving with a decided community status and a source arriving with an availability answer: each refused naming its file, the signer never asked, no act recorded), `an_import_is_held_to_what_an_acceptance_holds_a_change_to`, `the_dry_run_says_what_the_run_would_refuse`, `knowledge_is_read_only_when_the_manifest_declares_the_extension_this_build_reads` (nine manifest shapes: a bundle that does not declare the extension imports no `knowledge/`), `a_bundle_that_declares_the_extension_has_its_claim_and_source_files_planned_and_the_extensions_directory_named` (the extension's own directory, with its claim threads, is named as not imported and "none of it is read"), `a_knowledge_file_that_does_not_read_or_is_not_a_claim_or_source_is_refused_by_rule_and_never_by_its_words`; `inklkunn-core/src/import/tests.rs`: `a_folder_that_carries_claims_with_a_community_status_imports_none_of_them`; and, for a source that already has a check, `inklkunn-core/src/import/tests/knowledge.rs`: `an_import_that_moves_a_checked_sources_reference_or_states_a_check_is_refused_by_name_and_nothing_is_written` (six cases: the reference moved with the old check left, with a new day, with the check cleared; the same reference with a new day or another answer; a new source that arrives already checked: each refused with the file and the field named, by the dry run and by the run, the signer never asked, no act recorded, the tip unchanged; the unchanged source is the control and the rest of the bundle is written); the 0.3.0 readiness test `serve::writes_on::tests::knowledge::the_knowledge_layer_works_end_to_end_on_a_real_prikk` offers the real exported bundle to a second wiki on a real prikk, and it is refused whole by file and rule for the status and review days it carries, nothing written | The import tests use a fake repository and a signer stand-in; the readiness test is the one that runs on a real prikk, and it exercises the refusal, not an accepted import of knowledge files onto a real history (this record claims none). `admin/tests.rs` judges a knowledge import under the operator's read limits through the command; it needs prikk and is not one of this row's tests |
| **INV-11, C-17a** a **system act** does not write the knowledge layer | `inklkunn-core/src/knowledge/tests/curation.rs`: `a_change_the_service_makes_for_itself_does_not_write_the_knowledge_layer` (a system act whose file is a claim: `Invalid`, the signer not asked; a page is still the system's to write); `inklkunn-core/src/import/tests/knowledge.rs`: `a_system_change_still_may_not_write_the_knowledge_layer_and_an_import_cannot_be_judged_by_a_service_that_cannot_read` (the refusal's sentence; an import through a service that cannot read the wiki is *unavailable* and the signer sees nothing) | A system act that renames a knowledge file (the guard looks at renames too; no test sends one). The only kinds are *system* and *import*; the recovery and maintenance operations are not system acts in this sense, and no test shows them leaving the knowledge layer alone |
| **INV-11, C-17a** the **generic write API** cannot write a knowledge file | `inklkunn-web/tests/api/write.rs`: `the_write_api_writes_no_claim_or_source_file_so_it_cannot_set_a_community_status` (a proposal carrying a claim that arrives *accepted* beside its page, and one carrying a source file: each answered 422 `path-outside-pages`, and no proposal is made) | The refusal is the API's own path check (`is_inside_pages`), not the acceptance's rule: the proposal service accepts knowledge paths, for the forms. Only the create-and-submit call is tried; the revision and draft calls share `propose/files.rs::prepare` and are not tried |
| **INV-11, C-17b** a **check carried across a moved reference** is refused at submit and at acceptance, and a check counts only when the proposal states itself | `inklkunn-core/src/knowledge/tests/reference_check.rs`: `a_source_moved_with_a_check_that_this_proposal_does_not_state_is_refused_before_the_signer_is_asked` (the old check carried, the tip's date with another proposal's id, a date with no proposal: each refused at acceptance on `last_reviewed`, the signer not asked, the proposal still held; the same move with its own check is accepted), `the_same_rule_refuses_it_when_it_is_sent_and_at_every_revision` (submit and revision: `KnowledgeCheckUnstated`; the check naming the draft's own id is sent; the same text on another proposal's revision is refused), `a_stated_check_holds_the_bounds_of_a_check_at_the_acceptance`, `the_validator_refuses_a_source_found_elsewhere_that_carries_the_old_check_by_field`, `a_check_stated_on_the_date_the_old_one_had_is_named_by_the_accepting_proposal_and_a_kept_one_is_refused`, `a_same_date_check_over_a_review_that_names_no_proposal_is_accepted_when_it_is_stated_and_refused_when_it_is_carried`; the merge (C-17e): `a_reference_moved_on_one_side_and_a_check_made_on_the_other_is_an_overlap_in_either_order`, `the_reviewers_two_orders_stop_the_acceptance_with_both_texts_and_never_write_a_check_on_the_new_reference` (an overlap, both texts recorded, the signer not asked); `inklkunn-core/src/knowledge/tests/status.rs`: `what_a_checked_source_is_and_where_it_is_found_is_curation_and_before_the_check_it_is_not` (a page edit may not move a checked source's reference, on both surfaces); on a real history, `inklkunn-core/tests/acceptance.rs`: `a_check_stated_by_the_proposal_that_moves_a_reference_is_accepted_and_a_carried_one_is_refused_on_a_real_history`; the form, `inklkunn-web/tests/proposals/curate_check.rs`: `a_changed_reference_ends_the_check_and_the_new_one_has_only_the_check_the_form_states`, `the_old_fields_beside_a_new_reference_are_refused_on_the_form_by_their_field_and_nothing_is_made` | The API has no curation call yet, so the rule is not shown through it; an import that moves a source is the gap of the *import* row above. White space alone as no move (K81) is in `status.rs` and is not repeated here |
| **C-17c** validation before the signer is asked, a refusal that names the file and the rule, never a half-read file; **a repository with planted invalid knowledge files serves its pages and lists them** | `inklkunn-core/src/knowledge/tests/index.rs`: `a_file_that_cannot_be_read_is_listed_and_nothing_is_read_from_it` (five planted files, an unknown key, a bad value, a stray file, a name that is no identifier and an over-long file: each listed with its rule, no claim or source read from any, the page still served), `the_index_is_bounded_and_a_tip_over_the_bound_still_serves_its_pages`; `inklkunn-core/src/knowledge/tests/health.rs`: `each_file_that_cannot_be_read_is_listed_with_its_rule_and_the_pages_are_still_served`, `the_list_is_walked_in_pages_by_a_cursor_stamped_with_the_tip`; `inklkunn-web/tests/search_health/main.rs`: `every_health_list_shows_its_planted_case_with_what_would_resolve_it` (the *knowledge files* page: path, rule and what would resolve it; a file named with markup shown as letters); `inklkunn-core/src/knowledge/tests/acceptance.rs`: `an_unsound_claim_stops_the_acceptance_before_the_signer_is_asked`, `a_claim_whose_source_is_missing_is_refused_by_name_and_rule`, `a_repository_with_invalid_files_still_takes_unrelated_changes`; `inklkunn-core/src/knowledge/tests/check.rs`: `a_file_that_is_not_sound_is_named_with_its_rule`, `a_refusal_names_the_file_and_the_rule_and_never_what_the_file_said`; `inklkunn-core/src/knowledge/tests/refusals.rs`: `a_missing_key_gives_no_half_read_claim`, `a_refusal_never_repeats_what_the_file_said`; real prikk, `inklkunn-core/tests/acceptance.rs`: `a_claim_with_its_source_and_its_block_is_signed_and_an_unsound_change_is_refused_before_the_signer` | The planted files are in a fake repository (the unit tests) or in the web test's world; no test plants them in a real prikk repository and then serves a page. The health list is read in one web test; its Japanese page is not read here |
| **C-17d** a reference is text, never `href`, and an archive address is not rendered; nothing is fetched; a bidirectional control in knowledge text is refused; text is printed escaped | `inklkunn-core/src/knowledge/tests/roundtrip.rs`: `a_text_of_a_claim_or_source_file_with_a_direction_character_does_not_read` (each of the twelve controls in a note and in a source's title, author, date and reference: refused as `Rule::Value` on its field; eight neighbouring code points, U+200B – U+200D among them, still read in a title), `a_form_refuses_a_direction_character_in_every_text_it_takes`; `inklkunn-web/tests/proposals/claims.rs`: `a_source_text_with_a_direction_character_is_refused_on_the_form_with_a_message_of_its_own`; `inklkunn-web/tests/knowledge_pages/main.rs`: `a_source_is_named_by_what_it_says_of_itself_and_its_whole_identifier_follows_in_code` (a reference that is a URL is printed in a sentence and there is no `href` to it; the whole identifier in `<code>`, once), `what_a_source_says_of_itself_is_escaped_and_on_one_line`; `inklkunn-web/tests/proposals/rendered.rs`: `a_changed_sources_new_identity_and_reference_are_on_its_lines` (the new reference is printed in `<bdi>`); `inklkunn-web/tests/knowledge_pages/thread.rs`: `every_text_is_printed_as_letters`; never fetched, by structure, `tools/boundary-check/tests/gates.rs`: `only_the_binary_depends_on_the_fetch_crate_and_the_fetch_crate_depends_on_the_driver_alone`, `only_the_fetch_crate_names_curl` (only the binary crate can reach the fetcher) | **An archive address is shown nowhere**: the only trace of a snapshot is *the snapshot changes* on a proposal's summary, so *never a link* holds because it is never shown, and no test shows an archive address as text. The no-`href` assertion is for the reference on the claim page; the proposal pages' `<bdi>` line asserts the markup, not the absence of a link. The scrub of the whole of Cf is `is_format_character`, which the code uses for bubblewrap's reason and `curl`'s standard error; no test shows the knowledge pages scrubbing it (they escape and collapse white space), and the zero-width joiners stay writable and shown, as C-17d intends. The structural gates show that the core cannot fetch; they do not read the knowledge code |
| **C-17g** a restricted wiki's thread answers through every route as an absent wiki does (the route enumeration); a thread is append-only and changes no claim | `inklkunn-web/tests/route_enumeration/main.rs`: `every_guarded_route_answers_a_restricted_wiki_exactly_as_one_that_is_not_there` (walks the route registry itself, so the claim page `/{slug}/-/claim/{id}` as a read and the post `/{slug}/-/claim/{id}/discuss` as a comment are each asked of a restricted and of an absent wiki, and the answers must be the same but for the echoed address); `inklkunn-web/tests/knowledge_pages/thread.rs`: `a_claim_the_wiki_does_not_hold_and_a_wiki_that_may_not_be_read_are_the_pages_they_always_were` (a restricted wiki that holds a claim: the post answers as the post to an absent wiki, in status, body and language header, and nothing of the wiki is in it), `a_comment_is_recorded_shown_in_order_with_its_number_and_never_changes_the_claim`; `inklkunn-core/src/proposal/tests/claim_thread.rs`: `the_thread_of_a_wiki_that_may_not_be_read_is_not_there_to_read_or_to_write_in` (on both stores: an account with no grant, an outsider and an anonymous visitor cannot read the thread; an observer reads and may not comment, and neither may an account with no grant), `a_discussion_changes_no_claim_no_file_and_makes_no_proposal`; `inklkunn-store/tests/claim_threads.rs`: `the_database_refuses_what_the_store_would_not_write` (raw SQL: an update, a delete, a replacing insert and a taken place in a thread are each refused as append-only), `the_memory_store_keeps_claim_threads`, `the_sqlite_store_keeps_the_same_claim_threads`; `inklkunn-core/tests/export_discussions.rs`: `a_claims_thread_is_exported_in_the_knowledge_extension_beside_the_claim_and_changes_nothing_of_it` (one JSON line per comment; the body a fenced text block, `<b>` kept as letters) | The enumeration's restricted wiki holds no claim and no thread, so the route walk shows that the refusal is made before any data is read, not that a thread's text never appears; the restricted wiki that holds a claim is tried for the post (`…_the_pages_they_always_were`), for the thread at the service (`claim_thread.rs`) and, since the 0.3.0 correction round, for every knowledge address: `inklkunn-web/tests/route_enumeration/main.rs`: `a_restricted_wiki_that_holds_a_claim_answers_every_knowledge_address_as_an_absent_wiki_does` (a restricted wiki whose page says a claim with a source, and a second view: the claim page, plain and `?view=`, *Why is this here?*, the views list and the comparison, and the API's `claims/{id}`, `sources/{id}`, `provenance/{title}` and `views/compare`, by GET and HEAD, to an anonymous visitor and to a signed-in non-member, each 404 and equal to an absent wiki's answer, with no sentence, wiki name or claim id in it; controls: the same addresses on a public wiki hold the claim, and a reader of the restricted wiki is let in; mutations: admitting any signed-in person fails it at the claim page, and admitting one only in the API's `run` fails it at `claims/{id}` while the whole API suite stays green). `views/compare/{page}` is not in the list (its control answers 503 on the fake repository). There is no thread route in the API. A claim comment raises no notification in this release (nothing in `notify` names a claim), so C-7b's rule has nothing to test here. The append-only triggers guard code paths, not someone with the database file |
| **C-17i** no word or number says *true*: the retired-words test holds both languages | `inklkunn-web/tests/retired_words.rs`: `no_message_of_any_language_uses_a_retired_word_and_the_allowlist_has_no_dead_lines` (every catalog message, English and Japanese, against the glossary's §3.1 list, which retires *true*, *false*, *proven*, *canonical*, *official*, *adopted* and *verified* and the Japanese terms), `the_check_covers_english_and_japanese`, `every_retired_english_word_is_found_wherever_it_hides`, `every_retired_japanese_term_is_found_and_ordinary_words_are_not_mistaken_for_one` (the check is shown able to fail), `verified_stays_prikks_word_for_the_history_alone` (the allowlist holds exactly the badge's label, once per language, and `Key::AssuranceVerified` is used by `wiki.rs` alone), `the_claim_pages_say_nothing_that_ranks` (every `claim.`, `why.` and `title.claim` message, at least forty keys); `inklkunn-core/src/knowledge/tests/refusals.rs`: `the_format_has_no_key_for_a_score_a_rank_or_a_truth`, `nothing_numeric_is_accepted_anywhere`; `inklkunn-core/src/knowledge/tests/index.rs`: `nothing_in_the_index_is_a_number_or_a_rank` | Text that is not a catalog message: a claim's own words, a source's title or a note (people's text), and the API's field names, which this test does not read. A word the glossary does not list. What *bearing* means (how a source speaks to a claim) is the glossary's and a reviewer's reading, not a test's |
| **INV-12, C-18a** `root_layout` for planted Arch-style, Debian-style and mixed roots | `inklkunn-prikk/src/sandbox/tests.rs`: `an_arch_style_root_gets_its_links_to_usr_lib_and_usr_bin` (the exact argument list), `a_debian_style_root_gets_its_loader_directory_not_arch_s` (`/lib64` to `usr/lib64`, `/sbin` to `usr/sbin`; Arch's `usr/lib` for `/lib64` is not assumed), `an_absent_entry_is_left_out_a_real_directory_is_bound_and_a_link_keeps_its_own_target` (a real `/bin` bound read-only, an absolute and a relative link target kept as read, an absent `/lib64` left out, a `/lib32` link), `a_file_where_an_entry_is_expected_is_left_out` | Only the argument lists are asserted: that a link leading outside the bound tree dangles is bubblewrap's behaviour, not the test's. No planted root is run through bubblewrap, and no NixOS-shaped root is planted at all (the fail-closed answer for one is the code's, and the real-probe row's). A link that cannot be read is left out in the code, and no test plants one |
| **INV-12, C-18a** the probe, prikk's sandbox and `curl`'s vector are tied to `root_layout` | `inklkunn-prikk/src/sandbox/tests.rs`: `prikks_sandbox_lays_out_its_root_as_root_layout_does_for_the_root_it_is_given` (two planted roots that differ in every entry that matters: the layout is in the command, first, whatever the root; the production command asks `/`), `the_probe_lays_out_its_root_as_root_layout_does_for_the_root_it_is_given` (a stand-in `bwrap` records what the probe gave it; the probe's flags are the sandbox's); `inklkunn-fetch/src/vector/tests.rs`: `the_system_view_is_the_hosts_own_layout_the_driver_builds`, `only_the_system_curl_the_ca_bundle_and_scratch_are_mounted` (nothing writable bound from the host; nothing but two read-only binds beside `/usr`); `inklkunn-fetch/tests/real_vector.rs` (real bubblewrap): `inside_the_vector_there_is_no_repository_no_key_no_home_and_a_read_only_system` | `curl`'s vector is tied to the host's root (`/`) only; its test plants no root. For prikk's sandbox no test lists the whole argument set, so *nothing else of the host is added* (no home, no `/etc`) is shown for `curl`'s vector in a real bubblewrap, and for prikk's by the mounts' test (`bwrap_binds_the_repository_read_only_and_the_scratch_read_write`), not by a closed list |
| **INV-12, C-18a, C-18b** the real probe, and the program bound is the program run | `inklkunn-prikk/src/sandbox/tests.rs`: `the_real_root_confines_a_dynamically_linked_program_or_says_why_not` (the probe runs `/usr/bin/true`, dynamically linked, on the host's own root; a host that cannot confine prints the guarded line, and the only failure is the layout's own, bubblewrap's `execvp … No such file or directory`), `a_prikk_that_is_a_link_is_bound_and_run_as_the_file_it_leads_to`, `a_prikk_that_is_a_link_inside_a_mounted_tree_runs_confined` (real bubblewrap), `the_bind_target_of_a_link_is_the_file_it_leads_to_and_of_anything_else_itself`; `inklkunn-fetch/src/fetcher/tests.rs`: `a_curl_that_is_a_link_is_the_file_it_leads_to`; `inklkunn/src/serve/tests.rs`: `the_real_probe_decides_the_real_start_and_an_injected_answer_refuses_on_any_host` | On the dev host the layout is Arch's; the same tests pass or skip, as the host allows, on other hosts, and the first prints its skip line. The Debian and Ubuntu layouts run in CI's Ubuntu job and in the image of the last row, not here. A host whose loader is elsewhere (NixOS) fails closed *in the code*; no host of that kind runs a test. `curl`'s bound link is tested at the path (`curl_path`), not in a real bubblewrap |
| **INV-12, C-18c** confined or refused: the server and the commands refuse to run prikk when bubblewrap cannot confine, with bubblewrap's first line; the only way round is `--dev-unconfined`, which prints a banner | `inklkunn/src/serve/tests.rs`: `without_the_dev_flag_a_host_that_cannot_confine_prikk_refuses_to_start` (an injected answer, so it holds on every host: the refusal carries bubblewrap's reason and names `--dev-unconfined`; nothing is created), `the_sandbox_decision_follows_the_answer_it_is_given_not_the_host` (a yes gives the confined policy, a no is refused, the dev flag never asks), `a_refusal_for_a_program_bubblewrap_cannot_find_names_the_loader_not_user_namespaces`, `a_started_server_answers_over_a_socket_with_its_headers_and_stops_when_told` (with the flag, the start's banner has the UNCONFINED line; it self-skips without prikk); `inklkunn/src/config/tests.rs`: `unconfined_is_a_flag_and_the_sandbox_follows_it` (no environment variable turns it on); `inklkunn-prikk/src/sandbox/tests.rs`: `a_bwrap_that_cannot_confine_says_why_in_its_own_first_line`, `the_reason_is_cleaned_and_bounded`, `a_bwrap_that_says_nothing_is_reported_by_its_exit_status`, `a_bwrap_that_works_and_one_that_is_missing_are_told_apart`, `every_way_a_call_dies_of_its_memory_ceiling_is_read_as_the_ceiling_and_a_refusal_is_not`; `inklkunn-prikk/tests/driver.rs`: `sandbox_tool_failures_and_deadlines_and_output_bounds_are_typed` (a failing bubblewrap at a call is *unavailable* and carries its own words); `inklkunn-fetch/tests/standin.rs`: `bubblewraps_own_refusal_is_a_sandbox_error_whatever_its_exit_code`, `the_probe_reports_a_sandbox_that_cannot_confine`, `a_program_that_cannot_be_started_is_a_typed_error` (`curl` is never run unconfined) | The refusals are tested through `Settings::sandbox_probed`, which the server and every command share, but no test runs a *command* (`wiki create`, `import-folder`, `export`, `recover`, `metrics`) on a host that cannot confine, and none shows a command's banner. `PrikkCli::open`'s own probe of bubblewrap (the driver, which the signer uses) is not tried with a bubblewrap that says no. The server's banner is asserted in a test that needs prikk |
| **INV-12, C-18e (CI half)** CI's confined tests on Ubuntu with no bubblewrap skip line allowed | `tools/floor-check/tests/floor.rs`: `the_test_only_prikk_is_one_value_in_both_workflows_and_is_never_a_floor` (applies the shape rules to the real `ci.yml` and `release.yml`: the with-prikk run keeps its output with `--nocapture` and `--no-fail-fast`, and has a `grep` of `with-prikk.log` that names all eight guarded fragments, `bubblewrap cannot confine`, `the confined policy`, `bwrap or curl not available` and `inklkunn 0.2.0 not found` among them, followed by `exit 1`), `the_test_only_prikk_shape_rules_catch_planted_failures` (the rules fail on a workflow with a fragment taken out), `both_workflows_lift_the_user_namespace_restriction_before_their_tests`, `the_user_namespace_rule_catches_planted_failures`; `tools/floor-check/tests/guarded_skips.rs`: `a_prikk_capability_is_reached_only_through_the_probe_module_of_each_test_crate`, `the_reviewers_shapes_are_planted_failures_of_the_rule`, `a_public_function_that_can_return_none_without_printing_is_a_planted_failure` (a test that needs prikk cannot skip without printing a guarded line); `inklkunn-prikk/src/sandbox/tests.rs`: `every_way_the_probe_fails_gives_a_line_the_with_prikk_guard_catches` (a missing, a refusing and a silent bubblewrap all give a line that has `skipping: bubblewrap cannot confine a process here`) | The Ubuntu run is CI's: no test here runs it. The guard is a `grep`; floor-check holds its shape. That a bubblewrap-dependent test which skips prints one of the guarded lines holds by the probe modules for prikk and by each test's own `println!` for the others (`real_vector.rs`, the sandbox tests): no test enforces the second part for every confined test. The user-namespace lift is read only for the jobs that run `cargo test`, not for the image job. R-6 re-reads the candidate's CI run (the last green run on record when this was written is 44108905975, on 67edaea) |
| **C-18e** the release workflow's image step: the built image is run once under docker with C-18d's settings and hardening, and nothing is published unless `wiki create` succeeds | **No test in the workspace reads this step**: nothing in `tools/` or `crates/` names it. What a test does hold is the line the step greps for, `inklkunn/src/admin/tests.rs`: `a_wiki_is_created_filled_exported_and_archived_through_the_commands_and_no_seed_shows_anywhere` (`wiki create` prints `created wiki <slug>`, with real prikk). The step is in `.github/workflows/release.yml`, job `image`, *The built image confines prikk*: `set -euo pipefail`, `--security-opt seccomp=unconfined`, `apparmor=unconfined` and `systempaths=unconfined`, `--cap-drop ALL`, `--security-opt no-new-privileges`, `--read-only --tmpfs /tmp`, a failure that ends the job before the two push steps that follow it in the same job, and a `grep` for `created wiki confine-check`. The record of its run: the rehearsal of 2026-11-17, run 44103325108, whose image jobs for amd64 and arm64 and the job after them succeeded (read from the run's record, not a test) | The step's text, its order before the push steps and the settings it passes are held by reading them and by the rehearsal, not by a test: an edit that moved the push before the check, or dropped a hardening option, would fail no test and would show only in the next rehearsal. `wiki create` is not tested to fail on a host that cannot confine (the refusal is tested at `Settings::sandbox_probed`, the row above). The rehearsal publishes nothing (`crates` and `release` are skipped in it). Podman's and Debian 13's measurements are SECURITY.md's, not this record's |

Also re-run: the structural, catalog, glossary, retired-words and stylesheet tests on every page state in both
languages; the sanitizer tests; the supply-chain gates (`cargo deny check`, `cargo audit`).

*End of Threat Model v0.3. Its headline is RR-1, stated so no one discovers it after depending on it: inklkunn
holds its wikis' signing keys by the owner's ruling, and everything in this model is built to make that
custody bounded, detectable, attributable and recoverable rather than invisible.*
