# inklkunn — External Design Specification

| | |
|---|---|
| Document | inklkunn External Design (the black-box view): what inklkunn exposes at its boundaries — its web surfaces, API, identity surface, prikk-integration surface, the flows a person goes through, the external data contracts and the operational behaviours — for everything the approved requirements and rulings make designable. It designs up to each gate and stops there. |
| Version | v0.1 |
| Date | 2026-10-27 |
| Inputs | Requirements v0.1 (`inklkunn-01`, approved 2026-10-27), cited as PU/NG/US/CM/RD/PR/KN/GV/ID/IX/MD/SEC/PRV/INC/PERF/DUR/OPS/INT/BN/C/UD/OQ; the roadmap and the rulings DR-1 … DR-8; wiki-commons 0.4 Part III; prikk 0.54.1 reference pages; planeter 0.4.0's external design as the house precedent |
| Not | internal architecture, database schema, the OpenAPI document itself, or code. |
| ID scheme | `BD-` boundary · `WEB-` web surface · `API-` REST surface · `AUTH-` identity surface · `PK-` prikk-integration surface · `FL-` flow · `CT-` external data contract · `OP-` operational behaviour · `GATED-` a surface held behind an owner question, an upstream dependency or a later investigation |

Design stance carried from the requirements: **the VCS is the substrate, never the interface; every change is a
proposal; prikk is the source of truth for history; the wiki service signs as itself and says so; what prikk
verified, what the service accepted and what a person approved are three distinct, visible facts.**

---

## 1. System boundary (BD-…)

```
   [browser]           [API client / bot]        [planeter]               [operator]
       │ HTTPS               │ HTTPS                  │ shared identity,        │ deploys, backs up,
       ▼                     ▼                        │ links, read-only        │ holds the key secret
  ┌────────────────────── inklkunn process ──────────┼─────────────────────────┼─────────────────────┐
  │  WEB (pages · history · propose · review · admin)   API (OpenAPI)   feeds   site description     │
  │  AUTH (accounts, OpenID Connect, sessions, tokens, one default-deny authorize())                 │
  │  SERVICE STORE: wikis, accounts, roles, proposals, revisions, comments, decisions, intents,      │
  │     audit log, metrics, derived cache  — service-authoritative or derived, never history (CM-7)  │
  │  SIGNER: the per-wiki author and maintainer keys, encrypted at rest; the only component that     │
  │     runs prikk's write verbs (commit, seal, branch create)                                        │
  │  STORAGE: drives the prikk CLI as a sandboxed subprocess (reads: tree, cat, log, show, diff,      │
  │     extract, verify, branch; writes through the SIGNER only)                                      │
  └──────────────────────────────────────┬───────────────────────────────────────────────────────────┘
                                          ▼ subprocess, on disk
                      prikk CLI ──▶ one `.prikk` repository per wiki, whose worktree is a Portable Wiki Bundle
                      (source of truth for pages, attachments, knowledge and governance objects, and history)
```

- **BD-01 — Inside inklkunn:** the web and API surfaces, identity and authorization, the service store
  (review state, attribution records, audit, derived cache), the signer, and the storage driver. inklkunn
  holds authority over proposals and the keys of its wikis (DR-4).
- **BD-02 — Outside inklkunn:** people and their browsers; API clients; planeter (US-A9), which shares
  identity and may host a wiki's repository read-only; the operator, who deploys, backs up and supplies the
  secret that unlocks the key store; and prikk, which owns the repository (BN-2).
- **BD-03 — The prikk boundary is a process boundary** (INT-1): inklkunn invokes the prikk binary and
  consumes its JSON; it links no prikk crate.
- **BD-04 — Two sources of truth, never confused** (CM-7): the repository for content, knowledge and
  governance objects and for history; the service store for review state, attribution records and audit.
  Rendered pages, indexes and caches are derived from the two and rebuildable.
- **BD-05 — The signer is a boundary inside the process** (SEC-4; threat model TB-5): no web or API code
  path reaches key material; it asks the signer to accept a prepared change and receives a sealed result.
- **BD-06 — Honesty is externally observable** (RD-4, ID-5): any holder of the repository can run
  `prikk verify` and read the signed messages; what inklkunn shows about a change is re-derivable from
  that.

## 2. Surfaces

### 2.1 Web surface (WEB-…)

All pages are server-rendered, work without scripts (INC-4), carry the catalog's words in the reader's
language (INC-2), pass the structural accessibility checks (INC-1), and consult `authorize()` (ID-7).

- **WEB-01 — Wiki home and page view** (RD-1, RD-2, RD-14). A page shows its title and place, the view when
  not the default, the body rendered from the profile, created and changed and by whom, its license, the
  verification badge, and one-step links: history, discussion, *Propose a change*, raw source, backlinks.
  Dangling links are marked and lead to *Propose a new page*. Depth (provenance, verification detail,
  governance) is behind *Details*.
- **WEB-02 — History, diff and permalinks** (RD-3). The change list for a page; a diff between any two
  revisions with word-level marks and a text summary; a permalink to a revision renders the page at that
  point and says so, linking to the current one.
- **WEB-03 — Propose** (PR-3). From a page or a section: a form with the current text, a summary field
  ("what and why"), optional sources, *Preview* and *Show changes* (server round trips), *Save as draft*
  and *Submit*. Lint findings appear as suggestions (PR-9). Submission yields the proposal page. A
  signed-out reader sees the form's existence and is asked to sign in (or, when the wiki's policy allows
  anonymous proposing, to continue under a temporary pseudonym; OQ-1 default off).
- **WEB-04 — Proposal page and review** (PR-2, PR-5, PR-6, PR-7). The proposal's state, author, target,
  summary, what it changes (page by page, block by block), the discussion thread, inline comments anchored
  to blocks, decisions per reviewer bound to the revision, and the actions the viewer's role allows:
  comment, approve, request changes, accept, decline, withdraw, revise. A stale base shows *re-authored
  against the current page*; an overlap shows both texts and asks for a revision. An accepted proposal
  links to its change in the history.
- **WEB-05 — Discussion** (PR-17). A page's talk, threaded, resolvable, attributable; kinds labelled (KN-7).
- **WEB-06 — Recent changes, search and health** (RD-6, RD-7, RD-9). Filterable recent changes; search with
  *create "…"* when nothing matches; wanted, orphan, dead-end, broken-redirect and wants-help lists, each
  with its resolving action.
- **WEB-07 — Sign-in** (ID-1): local password or single sign-on; accessible authentication (no cognitive
  test, paste allowed).
- **WEB-08 — Administration** (US-A6): create a wiki (name, default view, license, language, policy
  bootstrap); roles; key status (which key ids, when rotated; never a secret); export (bundle, archive);
  import with a dry run and a report; the recovery door (DUR-2).
- **WEB-09 — Honesty labels** (ID-5, ID-9): *Verified*, *Verification failed*, *Could not be checked now* for
  the repository; *held for review*, *accepted*, *declined* for proposals; *signed by the wiki service on
  behalf of …* on every change. No page says more than is known.
- **WEB-10 — Safe rendering** (SEC-2): sanitizer, strict CSP, raw source and attachments on the content
  origin only.
- **WEB-11 — Knowledge surfaces** (M3; GATED-1): claims and evidence on a block, *Why is this here?*,
  *Show disagreement*, view comparison and creation.
- **WEB-12 — Governance surfaces** (M4; GATED-2): the policy page, roles and the ladder, protections,
  moderation with tombstones, appeals, the inclusiveness dashboard for maintainers (GV-9).

### 2.2 REST API (API-…)

- **API-01 — Described in OpenAPI** (IX-4), served at a documented path and advertised from every page by
  `rel="service-desc"` (IX-6). Versioned in the path; stable error codes.
- **API-02 — Reads:** list pages (cursor pagination, filters: namespace, tag, kind, changed since); a page
  as JSON (metadata + rendered fragment) and as raw source (RD-10; content negotiation `text/markdown`);
  history, a revision, a diff; backlinks and link health; search; recent changes; proposals and their
  revisions; discussions. Every response carries an `ETag`; conditional requests are honoured.
- **API-03 — Writes** (authorized, scoped): create a proposal, revise it, comment, review (approve /
  request changes), accept, decline, withdraw; each write names the base revision or `If-Match` and gets a
  conflict response when stale (API-11 of wiki-commons).
- **API-04 — Errors** are problem details (RFC 9457) with a stable `type`, a message and a hint in the
  caller's language; existence-neutral for private resources.
- **API-05 — Rate limits and scopes:** `RateLimit` and `Retry-After` headers; token scopes `read`,
  `propose`, `review`, `accept`, `admin`, enforced per endpoint, intersected with the account's roles.
- **API-06 — Feeds:** Atom for the wiki, a namespace, a page, and proposals (IX-5).
- **API-07 — Discovery:** the site description document (IX-6), OpenSearch, sitemap and `robots.txt` on
  public wikis, the interwiki map.
- **API-08 — Not a second source of truth** (NG-6): reads re-derive from prikk and the service records.

### 2.3 Identity and authorization surface (AUTH-…)

- **AUTH-01 — Sign-in:** local passwords (Argon2id) and OpenID Connect (code + PKCE); accounts linked to an
  issuer and subject administratively or at first sign-in per the wiki's policy; sessions `Secure`,
  `HttpOnly`, `SameSite`, with CSRF tokens on every cookie-authenticated write.
- **AUTH-02 — Machine credentials:** scoped, hashed, expiring, revocable tokens (API-05).
- **AUTH-03 — Authorization:** one pure default-deny `authorize(principal, action, resource)` (ID-7)
  consulted by web, API, feeds, search and export; roles per wiki from the ladder (GV-2); visibility
  public / internal / private is a read gate; a denied private resource is indistinguishable from an absent
  one.
- **AUTH-04 — Attribution:** every proposal, comment, decision and acceptance is recorded against the
  principal; every acceptance is written into the signed patch message (CT-03).
- **AUTH-05 — Audit log** (ID-8), append-only, exportable.
- **AUTH-06 — Shared identity with planeter** (MD-2; GATED-3): the mechanism is RFC 008's; until then a
  common OpenID Connect provider makes one sign-in serve both.

### 2.4 prikk-integration surface (PK-…)

- **PK-01 — One repository per wiki,** laid out as a Portable Wiki Bundle (CM-1), under an inklkunn-managed
  root keyed by an opaque wiki id (never the wiki's name on disk, so renames move no bytes).
- **PK-02 — Reads** through the storage driver: `branch` (views and tips), `tree` and `cat` at a point,
  `log` and `show` (history and a change's content), `diff` between points, `extract` of a point into a
  scratch directory, `verify`. All `--format json`; an unexpected `schema_version` is a refusal (INT-1).
- **PK-03 — Writes** only through the signer: bring the worktree to the target view (`branch switch`),
  write files, `mv` for renames, `commit -m` under the wiki's author key, `seal --allow-no-audit` under
  its maintainer key, `verify`; `branch create` for a new view (INT-4, INT-8).
- **PK-04 — Sandbox and pin:** every invocation confined (a restricted filesystem view: the repository,
  the scratch directory and, for writes, the decrypted seed files on a private mount), with a wall-time
  and memory budget (SEC-7); the prikk version checked at start against the floor.
- **PK-05 — Keys:** generated by `prikk key generate`; adopted with `trust maintainer add`; key ids
  `inklkunn-<wiki-id>-author` and `-maintainer`; rotation adopts a new id and removes the old from trust
  (ID-3).
- **PK-06 — Gaps shown, not faked** (INT-9).

## 3. Flows (FL-…)

- **FL-01 — Read a page (US-1).** Resolve wiki and title (aliases, redirects, normalization; CM-3) →
  authorize read → the view's tip (cached) → the page file at that tip → parse, resolve links, render,
  cache by content id → the page with its badges. A missing title shows *This page does not exist yet*
  with *Propose it* (NAV-3).
- **FL-02 — Propose (US-3).** Open the form on a page or section → draft saved on each preview (PR-10) →
  submit → the proposal is recorded (service store) with its base revision, content and summary → the
  proposal page; reviewers are notified per policy (PR-11).
- **FL-03 — Review (US-5).** Read what changes → comment inline or in the thread → approve or request
  changes, bound to the proposal revision → the proposer is told.
- **FL-04 — Accept (US-6).** An authorized role accepts → the service takes the wiki's write lease → checks
  the base against the view's tip; if moved, three-way merge per page; on overlap, stop and show both
  texts (the proposal becomes *needs a revision*) → writes an intent record (DUR-2) → the signer brings
  the worktree to the view, applies files and renames, commits with the CT-03 message, seals, verifies →
  the cache for the view is invalidated → the intent is completed and the proposal marked accepted with
  the block and patch ids → the proposer is told. Any failure before the seal changes nothing; a failure
  after it is completed by recovery (FL-10).
- **FL-05 — Decline or withdraw (US-4).** A reason is required; the message template assumes good faith
  and names the next step; the proposer is told.
- **FL-06 — Undo (US-7).** *Propose a rollback* on a change opens a proposal whose content is the inverse at
  content level; it goes through FL-02 … FL-04 and names the change it undoes.
- **FL-07 — Rename a page.** A proposal that changes `title` and the file path; on acceptance a declared
  move (`mv`), the old title kept in `aliases` (NAV-8, HIST-6).
- **FL-08 — Export (US-13).** An admin requests a bundle (fidelity 3, discussions when present,
  pseudonymization optional) or a prikk archive; the bundle is built from the repository and the service
  records and offered as a download from the content origin.
- **FL-09 — Import (US-13).** An admin uploads a bundle → dry run with a report → confirm → the pages become
  one or more accepted changes under the wiki's keys with the import's attribution in the messages →
  report kept.
- **FL-10 — Recover after a crash.** At start and after an unfinished acceptance: for each wiki with an open
  intent, check prikk's locks and `doctor`, complete or abandon per the witnesses, `verify`, clear the
  flag; the administration surface shows the plan and the record (DUR-2).
- **FL-11 — Create a wiki (US-A6).** Name, default view, license, languages, policy bootstrap → repository
  initialized, keys generated and adopted, manifest and help pages written as the first accepted change.
- **FL-12 — Reach the wiki from planeter (US-15; GATED-3).** Signed in to planeter, a person follows the
  wiki link; shared identity recognizes them; roles map by the documented table.

## 4. External data contracts (CT-…)

- **CT-01 — The page file** (CM-2): Portable Wiki Markdown Layer 2 with YAML frontmatter; UTF-8, LF, NFC;
  the path under `pages/` derived from the title by the bundle's file-name rules (XFER-3), with `title`
  and `id` in frontmatter authoritative. Title-to-path mapping is deterministic, documented, and refuses
  characters that cannot be a safe path component.
- **CT-02 — The repository layout** (CM-1, CM-7): `wiki-bundle.yaml`, `pages/`, `attachments/` with
  `<name>.meta.yaml` sidecars, `knowledge/` and `governance/` (schemas fixed by RFCs 005 and 006). Nothing
  of inklkunn's inside `.prikk/` (INT-10).
- **CT-03 — The patch message** (ID-4): a first line that is the proposal's summary, then trailer lines in a
  stable form, for example `Proposal: <proposal id>`, `Proposed-By: <account id> (<display name>)`,
  `Accepted-By: <account id>`, `Base: <block id>`, `Kind: proposal | rollback | import | system`; parsed
  back by inklkunn and readable by anyone with `prikk log`.
- **CT-04 — Proposal and review records** (service store): proposal {id, wiki, view, state, author,
  summary, sources, created}, revision {proposal, seq, base block, content per page, created}, comment
  {anchor (page, block id, quote selector), body, author}, decision {revision, reviewer, verdict}, intent
  {proposal, revision, wiki, state, attempts}, acceptance {proposal, block id, patch ids, acceptor}.
- **CT-05 — The derived cache**: keyed by prikk content ids (block id, blob id) and the catalog language;
  invalidated when a view's tip moves; never a security display's source (RD-4).
- **CT-06 — The bundle export** (IX-1): the repository's files as stored, `history/<page id>.jsonl` from
  prikk's history with the CT-03 attribution, `discussions/` from the service records, `users.yaml`,
  manifest with fidelity and checksums.
- **CT-07 — The site description document** (IX-6) and the link relations of every page head.
- **CT-08 — Exported service records** (NG-7): proposals, reviews, audit as JSON Lines.

## 5. Operational behaviours (OP-…)

- **OP-01 — Writes are serialized per wiki** by a lease; a second acceptance waits a bounded time, then
  *busy* with a wait estimate (PERF-2).
- **OP-02 — The honesty line is everywhere** (RD-4): a stale cache never serves a verification badge; an
  unverified repository shows it and refuses acceptances (INT-6).
- **OP-03 — Degrade safely:** if prikk is unavailable for a wiki, reads serve the cache labelled stale,
  writes refuse with a reason; nothing is invented.
- **OP-04 — Bounded:** page, proposal and attachment sizes (PERF-4); prikk calls under a budget (SEC-7);
  imports bounded and dry-runnable.
- **OP-05 — Formats:** the serving binary stays at or above each repository's prikk format; upgrades are
  the operator's act (DUR-3).
- **OP-06 — Verification cadence:** after every seal and on a schedule; results recorded per wiki.
- **OP-07 — Keys at rest:** encrypted under the operator's secret; decrypted only for a write, onto a private
  mount visible to the sandboxed prikk and removed after (SEC-4).

## 6. Gated surfaces (GATED-…)

| ID | Surface | Held on | Interim |
|---|---|---|---|
| GATED-1 | WEB-11 knowledge surfaces: claim identity, evidence, views, provenance | RFC 005 part 1 investigation (KN-1; register items 1, 2, 11, 12) | the content model carries `^id` anchors and `knowledge/` from M0 so M3 adds objects, not a rewrite |
| GATED-2 | WEB-12 governance surfaces | RFC 006 | M2 ships with a fixed role set from the policy bootstrap and protections off |
| GATED-3 | AUTH-06 / FL-12 shared identity and the planeter module | planeter OQ-4; joint RFC 008 (UD-7, UD-8) | a common OpenID Connect provider; links by URL |
| GATED-4 | Anonymous proposing | OQ-1 (recommended off by default) | sign-in required to propose; the policy option is designed but off |
| GATED-5 | A richer editor | roadmap R6 | the plain-text form with preview and diff |
| GATED-6 | Multi-repository wikis | OQ-5 | one repository per wiki; namespaces as directories |

## 7. Traceability (design → requirements)

| Design area | Realizes |
|---|---|
| BD-01 … 06 | PU-3, PU-4, CM-7, SEC-4, INT-1, BN-1 … 5 |
| WEB-01 … 12 | RD-*, PR-*, INC-1 … 4, ID-5, ID-9, US-1 … 12 |
| API-01 … 08 | IX-4 … IX-6, NG-6 |
| AUTH-01 … 06 | ID-1, ID-7, ID-8, MD-2 |
| PK-01 … 06 | INT-1 … INT-10, ID-3, CM-1 |
| FL-01 … 12 | US-*, PR-6 … PR-8, DUR-2, IX-1, IX-3 |
| CT-01 … 08 | CM-1 … CM-3, ID-4, IX-1, IX-6, NG-7 |
| OP-01 … 07 | PERF-2, PERF-4, RD-4, INT-6, DUR-3, SEC-4 |
| GATED-1 … 6 | KN-*, GV-*, MD-*, OQ-1, OQ-5 |

*End of External Design v0.1. The load-bearing commitments: the signer is a boundary inside the process and the
only path to keys and to prikk's write verbs; the patch message is the attribution contract readable by any
prikk; reads re-derive and cache by content id; acceptance is atomic from the person's view with an intent
record and recovery behind it. The threat model (`inklkunn-03`) argues the key custody this design assumes;
the internal design (`inklkunn-04`) makes the boundaries properties of the build graph.*
