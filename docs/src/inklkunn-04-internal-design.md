# inklkunn — Internal Design Specification

| | |
|---|---|
| Document | inklkunn Internal Design (the white-box view): the crate decomposition and its gates, the storage driver and the signer, the data model, the read and write paths, the content-level merge, concurrency and recovery, and how the security invariants become properties of the build graph. Detailed enough to write RFCs and handoffs from; it stops where a subsystem's own RFC takes over (§12). |
| Version | v0.1 |
| Date | 2026-10-27 |
| Inputs | Requirements v0.1, External Design v0.1 (BD/WEB/API/AUTH/PK/FL/CT/OP/GATED), Threat Model v0.1 (TB/T/C/INV/RR); the rulings DR-1 … DR-8; prikk 0.54.1 reference pages (data model, patch algebra, concurrency and locking, commands); planeter 0.4.0's internal design, layering gate and dependency ledger as the house precedent; the project rules (Rust 2024, tests apart from implementation, mdBook docs) |
| Not | code, exact schemas, or the per-subsystem RFCs. |
| ID scheme | `CR-` crate · `LAY-` layering rule · `STO-` storage driver · `SGN-` signer · `MK-` markup · `DM-` data model · `RD-` read path · `WR-` write path · `MRG-` merge · `AZ-` authorization and identity internals · `CON-` concurrency and recovery · `ENF-` invariant enforcement · `SEQ-` build sequence · `NR-` next RFC · `IQ-` internal question |

Design stance: **the security invariants are properties of the build graph and the module boundaries, not
habits of careful coding.** The load-bearing result below: **the only crate that can read key material is the
one that runs prikk's write verbs, and the only way into it from the web is a call that takes a recorded,
accepted proposal.**

---

## 1. Workspace and component map (CR-…)

A Cargo workspace (Rust 2024, a declared and CI-checked minimum compiler, `forbid(unsafe_code)` in every
crate, lints as prikk's: `unwrap_used`, `expect_used` and `indexing_slicing` denied, `missing_docs` warned).

```
                    ┌──────────── binary ────────────┐
                    │  inklkunn (server)              │
  ── upper ─────────┴─────────────────────────────────┴───────────────────────────────────────
     inklkunn-web   (HTTP: UI + API + feeds; catalog; sanitizer; CSP; content origin)
  ── core ────────────────────────────────────────────────────────────────────────────────────
     inklkunn-core  (domain: wikis, pages, proposals, review, acceptance orchestration,
                     authorize(), honesty, merge, export/import, metrics, egress guard)
     inklkunn-auth  (accounts, sessions, tokens, OpenID Connect, throttles, hashing)
     inklkunn-markup (the Portable Wiki Markdown profile: parse, resolve, render, lint — pure)
  ── lower ───────────────────────────────────────────────────────────────────────────────────
     inklkunn-prikk  (the Storage trait + the sandboxed prikk CLI driver, read verbs only)
     inklkunn-signer (the key store + the write verbs: commit, seal, branch create; depends on inklkunn-prikk)
     inklkunn-store  (service records and the derived cache, behind traits: SQLite first)
  ── tools ─────────────────────────────────────────────────────────────────────────────────────
     tools/boundary-check (layering + key-reachability gates), tools/floor-check (one prikk floor)
```

- **CR-prikk** (`inklkunn-prikk`): the `Storage` trait (STO-1) and its first implementation, a sandboxed
  subprocess driver for prikk's **read** verbs with typed JSON. The only crate that spawns `prikk` for reads.
- **CR-signer** (`inklkunn-signer`): the key store (SGN-1) and the **write** verbs. The only crate that
  touches key material and the only one that spawns `prikk commit`, `seal`, `branch create`, `trust`,
  `key`. Depends on CR-prikk for the sandbox and the typed JSON. Exposes one narrow trait (SGN-3).
- **CR-store** (`inklkunn-store`): service-authoritative records (proposals, revisions, comments, decisions,
  intents, acceptances, roles, accounts' link to identities, audit, metrics) and the derived cache, behind
  traits; SQLite (bundled `rusqlite`, WAL) first, a PostgreSQL path kept.
- **CR-markup** (`inklkunn-markup`): pure, no I/O — frontmatter, the profile's Layer 2 syntax, link
  resolution rules, block identifiers, the degradation ladder, rendering to a semantic HTML tree, lint
  (AUTH-13), and the title-to-path function (CT-01). Tested against wiki-commons' conformance corpus
  (`corpus/`) as fixtures.
- **CR-core** (`inklkunn-core`): the domain. Wikis and views, pages as parsed documents, proposals and
  review, the acceptance orchestration (WR), the content-level merge (MRG), `authorize()` (AZ-2), the
  honesty view-model, export and import, the inclusiveness metrics, the egress guard. Depends on markup,
  store, prikk (reads) and the signer's trait (never its key store).
- **CR-auth** (`inklkunn-auth`): principals from credentials — passwords (Argon2id), sessions, tokens,
  OpenID Connect (code + PKCE), throttles. Output: a principal; `authorize()` is CR-core's.
- **CR-web** (`inklkunn-web`): axum routes, askama templates, the message catalog with the glossary tests,
  the sanitizer, CSP and headers, the content origin host gate, the OpenAPI document generated from the
  router, feeds, the structural accessibility tests over every page state.
- **inklkunn** (binary): wires the crates, reads configuration from environment variables, probes
  prerequisites (prikk at the floor, bubblewrap, `prlimit`), runs recovery at start, serves.

## 2. Layering and boundary rules (LAY-…) — enforced, not drawn

- **LAY-1 — Downward only.** upper → core → lower. A production dependency from a lower or core crate into
  an upper crate fails `boundary-check`, naming the edge (planeter's gate, adopted).
- **LAY-2 — `authorize()` lives in CR-core and every surface calls it** (INV-4). A route-enumeration test in
  CR-web asserts every registered route and API verb either consults it or is registered public with a
  reason (planeter's ENF-4), from the first write handler.
- **LAY-3 — Only CR-prikk and CR-signer spawn prikk**; CR-signer only for writes, CR-prikk only for reads; a
  source gate fails on `prikk` invocation anywhere else.
- **LAY-4 — Only CR-signer depends on the key store** (INV-2). The key store is a module inside CR-signer
  with no public export of key bytes; `boundary-check` fails if any other crate declares a dependency on
  CR-signer's internals or names its key-store module, and a test asserts CR-web's dependency closure
  contains the signer *trait* crate only through CR-core.
- **LAY-5 — One egress guard** in CR-core for every outbound fetch (INV-3).
- **LAY-6 — CR-markup is pure.** No I/O, no clock, no randomness; `boundary-check` fails on an I/O
  dependency.

## 3. The storage driver (STO-…) — CR-prikk

- **STO-1 — The `Storage` trait** names what inklkunn needs of a repository, in inklkunn's words: list views
  and their tips; the tree at a point; a file's bytes at a point; the history of a view; one change's
  content; a diff between two points; materialize a point into a scratch directory; verify. Points are view
  names or block ids. The trait is the seam for a future library or another backend (UD-2).
- **STO-2 — The first implementation drives the prikk CLI**: `branch --format json`, `tree`, `cat`, `log`,
  `show`, `diff`, `extract`, `verify`, each parsed into typed values; the `schema_version` of every report is
  checked against the pinned set and an unknown one is a typed refusal (INT-1). The floor (`0.54.0`) is one
  constant checked by `floor-check` against the CI image and the release notes.
- **STO-3 — Sandbox.** Every call runs under bubblewrap (`--unshare-all`, `--clearenv`, `--die-with-parent`),
  a read-only system view, the repository directory and the call's scratch directory mounted, a wall-time
  bound kept by the server, and a `prlimit` data ceiling from a per-verb cost table measured on the floor
  (planeter RFC 011's shape, with the table re-measured at each re-baseline).
- **STO-4 — Admission.** A gate with a memory budget split into a light pool (floor-cost reads: `branch`,
  `log` paged) and a heavy pool (`extract`, `verify`, `diff` on large points); refusals are `busy` or
  `too_large`, never silence.
- **STO-5 — Reads never take a write lock**; prikk's read verbs create no lock files, and the driver never
  invokes a verb that does.

## 4. The signer (SGN-…) — CR-signer

- **SGN-1 — The key store.** One record per wiki: the author and maintainer seeds encrypted with a key
  derived from the operator's secret (supplied at start through the environment or a file, never stored
  with the records), the key ids, the created and rotated-at times. Decryption yields seed files written to
  a private, mode-0600, tmpfs-backed directory mounted into the sandbox for one write call and removed
  after (OP-07).
- **SGN-2 — Key operations:** create (`prikk key generate` twice, `trust maintainer add` in the wiki's
  repository, record), rotate (new ids, adopt, remove the old from trust, record), status (ids and dates
  only). Each is an audited admin act (ID-8).
- **SGN-3 — The one way in.** A trait `Accept` with one operation: given a wiki, a view, an acceptance
  token (the id of an acceptance record in state *pending seal*, with its intent) and a prepared change
  (the files to write, renames, the CT-03 message), bring the worktree to the view, apply, `mv`, `commit`
  under the author key, `seal` under the maintainer key, `verify`, and return the sealed change's block and
  patch ids. It refuses a change whose acceptance record is not pending or whose message does not name it
  (C-1b). It never exposes a key, a seed path or a signing primitive.
- **SGN-4 — Branch creation** for a new view is the signer's second operation (a maintainer-signed
  publication in prikk), bound to a recorded view-creation act.
- **SGN-5 — Process separation is a documented option**, not v1: the trait is designed so CR-signer can run
  as a separate process on a local socket later (threat model RR-1), with no change to callers.

## 5. Markup (MK-…) — CR-markup

- **MK-1 — Parse** CommonMark + GFM extensions + footnotes with a conformant parser (`pulldown-cmark`, the
  house choice), then the profile's Layer 2 on top: free links in every form of MKUP-4, embeds, block
  identifiers, inline tags, callouts, hidden comments, the interwiki prefix form, generic directives, and
  the snapshot envelope recognized as a container.
- **MK-2 — Frontmatter** in the plain YAML subset (C-4b), validated against the metadata vocabulary;
  unknown keys preserved; `kind`, `visibility` and `system` keys writable only by the service.
- **MK-3 — Resolve links** by the MKUP-5 order against a view's title index (titles, aliases, redirects,
  normalization, hierarchy fallback), producing resolved, dangling and interwiki link sets and the
  backlink edges.
- **MK-4 — Render** to a semantic HTML tree with the MKUP-12 conventions, then through the sanitizer in
  CR-web; headings get deterministic ids; identified blocks get `id` attributes; dangling links get the
  marker and the accessible description.
- **MK-5 — Blocks.** Every top-level block gets a stable address: the `^id` if present, else a derived
  address used only transiently (anchors that must persist are given a `^id` by the acceptance that
  attaches something to them, CM-5).
- **MK-6 — Lint** produces suggestions with the guideline they come from (AUTH-13).
- **MK-7 — Title ↔ path** (CT-01): a pure function and its inverse under test and fuzz: NFC, the forbidden
  characters replaced by a documented substitute, `/` as hierarchy, case-collision suffixes, reserved names
  refused, never escaping `pages/`.

## 6. Data model (DM-…)

- **DM-1 — Wiki record:** opaque id → on-disk path (`<root>/<shard>/<wiki id>/.prikk`), display name, slug
  for URLs, default view, default license, languages, visibility, policy bootstrap, prikk format version,
  verification state and time, key ids (never seeds).
- **DM-2 — Repository content** is the bundle (CT-02); inklkunn reads it through STO and writes it through
  SGN; nothing of inklkunn's lives in `.prikk/`.
- **DM-3 — Proposal, revision, comment, decision, intent, acceptance** as CT-04, append-only where the
  threat model needs it (decisions and acceptances are never updated, only added); ids are UUID v7.
- **DM-4 — Accounts, identities, roles:** account {id, display name, status}, identity {account, kind:
  local | oidc(issuer, subject)}, role grant {account, wiki, role, granted by, at}, token {account, scopes,
  hash, expiry}.
- **DM-5 — Audit log** {at, principal, action, resource, outcome, detail without content}.
- **DM-6 — Derived cache** {wiki, view tip block id, page id or path, catalog language, rendered HTML,
  link sets, computed at}; invalidated by view tip; never holds a verification result.
- **DM-7 — Title index** per view tip: title, aliases, redirect targets, page id, path — derived from the
  frontmatter of all pages at that tip, cached, rebuilt on tip change.

## 7. The read path (RD-…)

- **RD-1** Resolve the wiki by slug → `authorize(principal, read, wiki)` → the view (default unless named)
  → its tip from `branch` (cached briefly) → the title index at that tip (DM-7) → the page path → the
  cache by (tip, path, language); on a miss, `cat` the file at the tip, parse (MK-1 … MK-4), resolve against
  the index, render, store.
- **RD-2** History from `log` of the view (paged) and `show` of a block; diffs from `diff --from --to`
  restricted to the page's path, rendered word-level in inklkunn from prikk's hunks (prikk supplies the
  line diff; the word-level refinement is a presentation step and is labelled as inklkunn's).
- **RD-3** The verification badge from the wiki record's last `verify` (OP-06), re-run on demand when the
  page is the verification page itself; never from DM-6.
- **RD-4** Search: SQLite FTS5 over titles, aliases and (from M2) bodies at the default view's tip, with a
  trigram tokenizer for scripts without word delimiters; rebuilt on tip change incrementally by changed
  paths (from `diff`).
- **RD-5** Feeds and recent changes from `log` across the wiki's views, filtered by `authorize()`.

## 8. The write path (WR-…) — the centerpiece

- **WR-1 — Propose.** CR-core records the proposal and its first revision (DM-3) with the base block id of
  the target view's tip at that moment and the proposed content per page; drafts are revisions in state
  *draft*; nothing touches prikk.
- **WR-2 — Review** adds comments and decisions bound to the revision (DM-3); a new revision clears the
  decisions' applicability (they stay recorded, shown as "on revision N").
- **WR-3 — Accept.** `authorize(principal, accept, proposal)` (policy: role, required approvals, protection)
  → take the wiki's lease (CON-1) → re-read the view's tip → if the tip equals the base, the change is the
  proposal's content; else MRG-1 per page → on an unresolved overlap release the lease and move the
  proposal to *needs a revision* with both texts recorded → else write the acceptance record (*pending
  seal*) and the intent (CON-3) → call `Accept::accept` on the signer (SGN-3) → on success record the block
  and patch ids, mark accepted, complete the intent, invalidate DM-6 and DM-7 for the view, enqueue the
  verification (OP-06) and the notifications → release the lease.
- **WR-4 — Failure before the seal** (merge, write, commit refused): nothing in prikk changed; the
  acceptance record is marked failed with the reason; the proposal stays open.
- **WR-5 — Failure after `commit` but before or during `seal`**, or a crash anywhere: the intent is open;
  the wiki is flagged *needs recovery* and refuses writes (CON-4) until the recovery completes the seal
  (prikk's own queued patch is sealed by the recovery's `seal`, or discarded by `doctor
  --discard-damaged-commits` when its witness is damaged) and reconciles the acceptance record.
- **WR-6 — The message** (CT-03) is composed by CR-core and checked by the signer against the acceptance
  record before it signs anything (C-1b).
- **WR-7 — Import** is a sequence of acceptances under a system principal with `Kind: import` and the import
  report's provenance, in batches bounded by the budget.
- **WR-8 — New view** (M3): a recorded view-creation act → SGN-4 → the view appears; proposals then target
  it like any other.

## 9. Content-level merge (MRG-…)

- **MRG-1 — Three-way, per page, per block.** Base = the page at the proposal's base block (`cat`); theirs =
  the page at the current tip; ours = the proposed page. Frontmatter merges key by key (a key changed on
  both sides with different values is an overlap). Bodies merge by a block-aware three-way diff: blocks are
  the units (MK-5), and within a changed block a line/word three-way merge is attempted; a block changed on
  both sides with no clean merge is an overlap.
- **MRG-2 — Overlap is never auto-resolved** (INT-7, consistent with prikk). Both texts are stored on the
  proposal and shown side by side; the proposer or the acceptor revises.
- **MRG-3 — Renames and deletions** merge by page id: a page renamed on one side and edited on the other
  merges (the edit lands on the renamed file); deleted on one side and edited on the other is an overlap.
- **MRG-4 — The spike (M0, roadmap R2)** measures, on a corpus of wiki-shaped edit histories, how often a
  moved base merges cleanly, how often prikk's own span anchors would have survived, and the cost; its
  numbers calibrate PERF-2 and may simplify MRG-1 (if prikk's `EditText` anchors survive most moved bases,
  the merge can be delegated to prikk's commit against the new tip for the clean case).

## 10. Identity and authorization internals (AZ-…)

- **AZ-1** CR-auth produces a principal: anonymous, account (via session or token with scopes), or a
  system principal (import, recovery). OpenID Connect through the egress guard with a confined `curl`
  (planeter's pattern) or an in-process client if the dependency policy permits one (IQ-3).
- **AZ-2** `authorize(principal, action, resource)` in CR-core: pure over a snapshot of roles, policy and
  the resource's visibility; default deny; existence-neutral; audited for consequential actions (ID-8).
  Policy in M2 is the bootstrap's fixed role set; M4 reads the versioned policy object (GV-1).
- **AZ-3** Scopes intersect roles (`read` ⊂ `propose` ⊂ `review` ⊂ `accept` ⊂ `admin`); protections
  evaluated inside `authorize(_, accept | propose, page)`.

## 11. Concurrency, consistency and recovery (CON-…)

- **CON-1 — One write lease per wiki**, in-process (an async mutex keyed by wiki id) in v1, with a durable
  holder record so a crashed holder is recognized at start; readers never wait for it.
- **CON-2 — prikk's locks are the layer beneath**: a `lock conflict` from prikk is reported, the acceptance
  fails before the seal (WR-4) or flags recovery after it (WR-5); no lock is ever cleared automatically.
- **CON-3 — The intent journal** (DUR-2): written before the signer is called with the acceptance id, the
  wiki, the view, the base and the message digest; completed with the block id; a crash leaves it open.
- **CON-4 — Recovery** at start and after a failed acceptance: read the witnesses (open intents, the lease
  record, prikk `status`, `doctor`, `unlock` listing, foreign processes), then under the lease: complete a
  queued commit by `seal` when the witness matches the intent, or discard it by `doctor` when it does not;
  `unlock` stale locks of the kinds an acceptance leaves; `verify`; reconcile acceptance records against
  blocks (C-10b); clear the flag. Every step recorded; the administration surface shows the plan (planeter
  RFC 004 part 4, adapted).
- **CON-5 — Cache coherence**: DM-6 and DM-7 keyed by view tip; a tip change invalidates; a stale read is
  labelled when prikk is unavailable (OP-03).
- **CON-6 — Idempotence**: an acceptance is keyed by the proposal revision; a retried accept finds the
  record and does not seal twice; an import is keyed by the bundle digest.

## 12. Invariant enforcement (ENF-…)

- **ENF-1 — INV-2:** `boundary-check` asserts only CR-signer names the key-store module and only CR-core
  depends on CR-signer; a test instantiates CR-web's dependency graph and asserts no path to key bytes.
- **ENF-2 — INV-1 / INV-10:** the reconciliation runs in tests with a planted unrecorded seal and a planted
  unsealed acceptance and must report both.
- **ENF-3 — INV-4:** the route-enumeration test (LAY-2), from the first write route.
- **ENF-4 — INV-3:** the title-to-path fuzz; a hostile bundle fixture; an SSRF fixture per fetch site;
  the sanitizer's regression tests.
- **ENF-5 — INV-5 / INV-6:** the derived store is cleared and every view rebuilt and compared; a badge is
  never read from DM-6; an open proposal's content is rendered only on its proposal page with the
  held-for-review label.
- **ENF-6 — INV-7 / INV-8:** no crate exposes an authority path to an automated actor; the removal procedure
  is a documented, rehearsed runbook, not code that rewrites history.
- **ENF-7 — INV-9:** `cargo deny`, `cargo audit`, pinned CI actions, `forbid(unsafe_code)` in every crate,
  the dependency ledger (planeter's, adopted) with measured growth per manifest change.

## 13. Build sequence (SEQ-…) and the RFC map (NR-…)

1. **SEQ-1 (M0)** CR-markup against the wiki-commons corpus; CR-prikk with the sandbox and the pin;
   CR-store; CR-core's `authorize()` seam and honesty view-model; the gates (`boundary-check`, `floor-check`,
   the route-enumeration test harness); the two spikes (STO measurement, MRG-4).
2. **SEQ-2 (M1)** the read path (RD-1 … RD-5), CR-web with the catalog (EN, JA in preview), the accessibility
   tests, export (bundle, archive), the standalone binary.
3. **SEQ-3 (M2)** CR-auth, proposals and review (WR-1, WR-2), CR-signer with the key store and `Accept`,
   acceptance (WR-3 … WR-6), MRG-1 … MRG-3, CON-1 … CON-6, notifications, quotas, metrics; threat model v0.2.
4. **SEQ-4 (M3)** knowledge objects, views (WR-8), provenance — after RFC 005 part 1.
5. **SEQ-5 (M4)** policy as data, moderation, deletion procedure — RFC 006.
6. **SEQ-6 (M5)** import, the write API, discovery, conformance — RFC 007.
7. **SEQ-7** the planeter module (RFC 008, joint), when planeter's OQ-4 is ruled.

| RFC | Subsystem designed here as | Serves |
|---|---|---|
| **NR-1 → RFC 001** Foundations | CR-*, LAY-*, STO-*, MK-7, DM-1, DM-2, CT-01/02, the gates | M0 → M1 |
| **NR-2 → RFC 002** Identity, keys and signing | SGN-*, AZ-*, DM-4, DM-5, CT-03, ENF-1/2 | M0 → M2 |
| **NR-3 → RFC 003** Read path and rendering | MK-1 … MK-6, RD-*, DM-6, DM-7, CR-web's catalog and tests | M1 |
| **NR-4 → RFC 004** Proposals | WR-*, MRG-*, CON-*, DM-3 | M2 |
| **NR-5 → RFC 005** Knowledge layer | `knowledge/` schemas, views, provenance | M3 |
| **NR-6 → RFC 006** Governance and moderation | `governance/` schemas, policy evaluation in AZ-2, tombstones, the removal runbook | M4 |
| **NR-7 → RFC 007** Interchange and API | CT-06 … CT-08, WR-7, the write API | M5 |
| **NR-8 → RFC 008** planeter module (joint) | AUTH-06, MD-*, SGN-5 if separation is wanted | T8 |
| **NR-10 → RFC 010** Upstream and supply chain | STO-2's floor and re-baseline, ENF-7 | all |

## 14. Internal open questions (IQ-…)

- **IQ-1 — Merge engine.** Implement the block-aware three-way merge in CR-core, or adopt a diff3 crate
  behind the dependency policy? Decided in RFC 004 after the MRG-4 spike.
- **IQ-2 — Search engine.** SQLite FTS5 with a trigram tokenizer (no new crate) versus a dedicated engine;
  FTS5 first, measured against PERF-3 (RFC 003).
- **IQ-3 — Outbound HTTPS for OpenID Connect.** A confined `curl` (planeter's shape, no TLS crates) versus an
  in-process client; the dependency policy decides, measured (RFC 002).
- **IQ-4 — Signer separation.** In-process module with the gate (v1) versus a separate process; revisited at
  the threat model v0.2 (RFC 002, SGN-5).
- **IQ-5 — Store backend.** SQLite first; the PostgreSQL path kept by the traits; multi-replica is the
  operations RFC's.
- **IQ-6 — How M1 gets content without proposals.** M1 reads repositories prepared by an operator (an
  import from a folder or a bundle through an admin command under the system principal, WR-7), so the read
  path ships before the proposal loop.

## 15. Traceability

| Internal design | Grounds in |
|---|---|
| CR-*, LAY-* | BD-01 … 06, INT-1, SEC-4, INV-2, INV-4; planeter RFC 001 |
| STO-* | INT-1 … INT-3, PK-02, PK-04, SEC-7; planeter RFC 011 |
| SGN-* | DR-4, ID-3 … ID-5, PK-03, PK-05, OP-07, T-1, INV-1, INV-2 |
| MK-* | CM-2 … CM-5, RD-1, CT-01, C-4a/b, wiki-commons chapters 08–09 |
| DM-* | CM-7, CT-04 … CT-06, ID-8 |
| RD-* | RD-1 … RD-14 of the requirements, FL-01, INV-5 |
| WR-*, MRG-*, CON-* | PR-6 … PR-8, INT-4, INT-7, DUR-2, FL-04, FL-10, OP-01 |
| AZ-* | ID-1, ID-7, AUTH-01 … 05, INV-4 |
| ENF-* | Threat model §4 and §7 |
| SEQ-*, NR-* | ROADMAP §3, §7 |

*End of Internal Design v0.1. Next: RFC 001 (foundations), RFC 002 (identity, keys and signing) and RFC 010
(upstream tracking and supply chain), each with a handoff, in `rfcs/`.*
