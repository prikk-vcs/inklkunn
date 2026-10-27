# Read path and rendering — Developer Handoff v1 (RFC 003)

| | |
|---|---|
| Document | The implementation package for RFC 003: tasks in build order, the QA checklist drawn from the RFC's acceptance criteria, and the definition of done and handback. A handoff directs implementation and never redefines its RFC; a design conflict goes back to the RFC first. |
| Status | Inherits RFC 003's: **accepted 2026-10-27**. **T1–T3 are startable once handoff 001's review is closed**; T4 onward also needs RFC 002's T2 (`authorize()` with the visibility rule and the route-enumeration harness); T7 also needs RFC 002's T3–T5 (the key store, `Accept`, the message contract). |
| Basis | RFC 003 (D-1 … D-13); RFC 001 (`Storage`, `inklkunn-markup`, the wiki record, the gates); RFC 002 D-2, D-6; internal design `RD-1 … RD-5`, `MK-1 … MK-6`, `DM-6`, `DM-7`, `ENF-3`, `ENF-5`, `IQ-2`, `IQ-6`; external design `WEB-01`, `WEB-02`, `WEB-06`, `WEB-09`, `WEB-10`, `API-01`, `API-02`, `API-04`, `API-06`, `API-07`, `FL-01`, `FL-08`, `CT-05`, `CT-06`, `CT-07`, `OP-02`, `OP-03`, `OP-06`; threat model `T-6`, `T-7`, `T-10`, `INV-5`, `INV-6`; requirements `RD-1 … RD-11`, `RD-14`, `INC-1 … INC-8`, `IX-1`, `IX-2`, `IX-5`, `IX-6`, `SEC-2`, `PRV-1`, `PRV-4`, `PERF-1`, `OPS-5`; planeter RFC 012 (D-2 … D-5 adopted as rules) and RFC 011 (the admission shape); RFC 010 D-6 (every new crate measured before its commit). |
| Audience | The dev team (the mid-capability model). Read `.git-exclude/roles/mid-capability-model-operating-instructions.md`, then `docs/STATUS.md`, then handoff 001's review request (what M0 is and what it is not), then this. |
| Scope | M1: `inklkunn-web` (routes, templates, catalog, sanitizer, headers, content origin, feeds, OpenAPI), the read service and caches in `inklkunn-core` and `inklkunn-store`, export and `import-folder`, the `inklkunn` binary, and the rendering refinements in `inklkunn-markup` that the pages reveal. **Not** sign-in pages beyond what tests need (RFC 002), **not** proposals (RFC 004), **not** the knowledge or governance surfaces, **not** bundle import (RFC 007). |

## The two lenses every task is reviewed through

The owner's acceptance principles stand above every decision below and are what the review request will be
read against:

1. **Finally clean, safe and secure, robust and sophisticated.** The end state is what counts: nothing
   transitional is left reachable without being named as transitional and given a removal path; no
   failure is silent; no display says more than is known.
2. **No reader, operator or API client is left confused or misled.** Every URL, label, message, status
   and error has one plain meaning in the glossary's words; an API shape or a page state that two readers
   could read two ways is a defect, not a nit.

## Before you start

- M0 is on `main` with its review closed; read `docs/STATUS.md` for the current issue register.
- prikk **0.54.x**, `bwrap` and `prlimit` on the development host; the driver's tests pass with prikk on
  `PATH` and self-skip visibly without it. Run both ways before every handback (RFC 010).
- **The stack is a dependency decision under RFC 010 D-6, measured before the commit that adopts it.** The
  first candidates are planeter's owner-approved standard stack, so that the two projects share one supply
  chain and one set of sanitizer regressions: `axum` 0.8 (`default-features = false`, `http1`, `query`,
  `form`, `tokio`; `json` only when the API task needs it), `tokio` 1 (`rt-multi-thread`, `net`, `macros`,
  `signal`, `time`), `tower` 0.5 (`util`), `askama` 0.13 (`default-features = false`, `derive`, `std`:
  compile-time, auto-escaping templates), `ammonia` 4 (the sanitizer; MPL-2.0 enters through its CSS parser,
  already in the allow-list), `url` 2. planeter's ledger measured this set at roughly +90 crates; expect the
  same order and record the exact delta, deny and audit per crate group, in the review request. A smaller
  stack is welcome if it is measured and justified; a larger one needs the architect first.
- Every user-facing string goes through the catalog from the first route (T2 before T4). Writing a literal
  "to fix later" is the one habit the catalog test exists to make impossible.
- The glossary (`docs/glossary.md`, T2) reuses planeter's words where the concept is the same (*change*,
  *proposal*, *Verified*, *Verification failed*, *Could not be checked now*) and fixes inklkunn's own
  (*view*, *page*, *wanted page*, *held for review*). The retired words (`sealed`, `commit`, `patch`,
  `block`, `ref`) appear in explanations only.

## Task breakdown, in build order

### T1 — `inklkunn-web` skeleton, safe headers, the two origins (RFC 003 D-1, D-3)
- **Purpose.** A server that serves nothing wrong before it serves anything at all.
- **Change scope.** The crate with the measured stack; the `inklkunn` binary's `serve` command with a
  configuration (bind address, application origin, **content origin**, the store path, the root layout,
  the prikk binary, the admission budget) read from a file and the environment, validated at start with
  every refusal named; the router with the URL scheme's reserved prefix `-/` fixed so no title can collide
  with it; the response headers on every response (`Content-Security-Policy` with `default-src 'none'`, no
  `script-src`, one stylesheet from the application origin; `X-Content-Type-Options: nosniff`;
  `frame-ancestors 'none'`; `Referrer-Policy`; `Permissions-Policy`; `Cache-Control: private, no-cache` with
  `Vary` on dynamic pages); the content-origin router that answers `-/raw/` and attachments as inert
  attachments and nothing else; the start-up refusals: a non-loopback bind whose content origin equals the
  application host refuses to start (planeter's IS-16 rule); **`Sandbox::Unconfined` is refused unless the
  binary is started with an explicit `--dev-unconfined` flag that also prints a banner**, so a hosting
  deployment cannot run prikk unconfined by accident; a `--version` that prints the prikk floor and the
  found prikk version.
- **Non-change scope.** No page content yet; no sign-in.
- **Required tests.** Every header on every response class (200, 304, 404, refused); the content origin
  refuses an application route and the application origin refuses `-/raw/`; the bind/origin refusal; the
  unconfined refusal and the flag; a planted route outside the `-/` prefix that would shadow a title fails a
  test.

### T2 — Catalog, glossary, structural and stylesheet tests (RFC 003 D-6; planeter RFC 012 D-2 … D-5)
- **Purpose.** Foundations first, so every page from T4 on inherits the tests instead of acquiring them.
- **Change scope.** A small catalog module (no new dependency: planeter's rule) keyed by stable identifiers,
  whole sentences with named placeholders and plural forms, English shipped and **Japanese compiled in as a
  preview** (labelled so in the switcher and the accessibility statement until the reviewers named under
  DR-7 sign off); negotiation by `Accept-Language`, a cookie switcher at `/-/lang` with endonyms and `lang`
  attributes, no script; `<html lang>` and `dir`; logical CSS only; `docs/glossary.md`; the structural test
  harness over rendered pages (one `h1`, landmarks, skip link first, labels, `alt`, `lang`/`dir`, heading
  order, table captions and headers, link text, unique `<title>`, 24 px targets, visible focus, contrast in
  both schemes computed from the CSS tokens, no physical-direction properties, reduced motion respected);
  the catalog tests (missing key, orphan key, placeholder mismatch, markup in a message, a literal
  user-facing string in a template or handler outside the catalog, a retired word outside an explanation).
- **Required tests.** Each test fails on its planted violation (the planted cases live beside the tests, as
  the gates do); the Japanese catalog has every key; the switcher keeps its choice without a script.

### T3 — The read service, the title index and the derived cache (RFC 003 D-2; DM-6, DM-7; INV-5)
- **Purpose.** Pages are re-derived from prikk and cached by content id; the service store is never the
  authority for content.
- **Change scope.** In `inklkunn-core`: the read service over `Storage` and `inklkunn-markup` — the view's
  tip from `views()` held for a few seconds per wiki; the title index at a tip (title, aliases, redirect
  targets, page id, path, from the frontmatter of every page at that tip, built with one `tree` and the
  `cat`s it needs, bounded); the page cache keyed by (tip state id, path, catalog language) holding the
  rendered HTML and the link sets; **a per-wiki store-size cache keyed by the tip's state id**, so the
  driver's cost estimate does not re-walk `.prikk` on every call (the driver gains an optional size hint;
  the walk stays the fallback and the truth after a write); the sanitizer pass (`ammonia`) under a policy
  that is exactly the profile's raw-HTML subset (today empty: the manifest says `html_subset: []`) applied
  **before any template sees the HTML**; the counting harness (how many prikk calls a request issued).
  In `inklkunn-store`: schema version 2 with the derived-cache and title-index tables, the migration from
  1 explicit and tested (a database at a newer schema is refused at open, as today); the cache never holds a
  verification result (a type-level rule: the cache record has no field for one). In `inklkunn-markup`:
  block identifiers become `id` attributes on the enclosing block (MKUP-12), and transclusion
  (`![[Page]]`) rendered to depth 2 with a cycle refusal that names the cycle. (`title_to_path` already
  refuses prikk's caps and percent-encodes non-ASCII titles, ledger PK-11 and PK-21; the web layer shows
  titles, never paths.)
- **Non-change scope.** No routes yet beyond what tests need; no search.
- **Required tests.** A page view issues **no prikk call on a hit and at most two on a miss** (the counting
  harness); the cache cleared and every view rebuilt yields identical HTML (ENF-5); the index resolves an
  alias and a redirect and reports *redirected from*; the sanitizer neutralizes a script, an event handler
  attribute, a `javascript:` and a `data:` URL, and an `<iframe>`, in a page body, a callout title, an alias
  and a frontmatter description; a transclusion cycle is refused with the cycle named; a title whose path
  would exceed either cap is refused by the markup crate before anything else sees it; schema 1 → 2
  migration on a copy of an M0 database.

### T4 — Page view and the URL scheme (RFC 003 D-1, D-5; WEB-01, WEB-09; RD-1, RD-2, RD-14)
- **Prerequisite.** RFC 002's T2 landed: `authorize()` with the visibility rule and the route-enumeration
  harness; **every route registers with an `Action` or a stated public reason from this task on** (ENF-3).
- **Change scope.** `/<slug>/<Title>` and `/<slug>/~<view>/<Title>`; the page template (title and place,
  the view when not the default, the body, created and changed and by whom from the CT-03 message, the
  license, the badge, one-step links: history, raw source, backlinks, *Propose a change* as a disabled
  affordance that says proposals arrive with the next release — never a dead link); 404 with *This page
  does not exist yet* and the invitation; percent-encoded UTF-8 titles readable in their own script; the
  dangling-link marker with its accessible description; **the badge** read from the wiki record's last
  verification and never from the cache, in RFC 002 D-6's words; the verification page running `verify` on
  request through the admission gate, showing what was found in plain words with the stage names as data,
  and *Could not be checked now* when the driver answers `Unavailable` (never a verdict); `-/about` with the
  license, the accessibility statement and the site description link; `/-/wikis`.
- **Required tests.** Route enumeration: every route has its `Action` or public reason (a planted bare
  route fails); a private wiki answers 404 identically whether or not it exists; the badge test (a stale
  cache with a changed record shows the record's state); alias, redirect, view and permalink resolution as
  D-1 says; the structural and catalog tests pass on every page state (found, redirected, missing, private,
  verification failed, could not check) in both languages.

### T5 — History, change, diff and permalinks (RFC 003 D-4; WEB-02; RD-3)
- **Change scope.** `-/history/<Title>` from `history()` (paged by `--limit` and the block ids), each row
  with time, the people from the message, the summary, the flags; the change page from `change()` saying
  *Signed into the history* and *signed by the wiki service on behalf of …*; `-/diff/<from>/<to>/<Title>`
  from `diff()` **always restricted to the page's path** (`--path`; a whole-tree diff is never issued from
  the web: the architect's answer to review request 001's open question 3), rendered with `ins`/`del`, a
  text summary and the word-level refinement computed by inklkunn and **labelled as inklkunn's**;
  `-/at/<block>/<Title>` with its banner; rename events shown as events.
- **Required tests.** A diff request issues exactly one `diff` with `--path`; the word-level marks never
  change the line diff's content (a round trip from marks to lines equals prikk's hunks); `ins`/`del` carry
  text alternatives; the permalink banner links to the current page; every page state passes T2's tests.

### T6 — Recent changes, feeds, search, health, discovery (RFC 003 D-7 … D-10; WEB-06; RD-5 … RD-7, RD-9)
- **Change scope.** `-/changes` and `.atom` across the wiki's views filtered by `authorize()`, by
  namespace, view, minor and automated, saying when results are filtered; Atom feeds for the wiki, a
  namespace and a page; SQLite FTS5 over titles and aliases with a trigram tokenizer rebuilt per tip change
  (IQ-2: measured against PERF-3 and recorded), results with title and snippet, *create "…"* with
  similar-title suggestions first, the OpenSearch description; the health views from the link graph at the
  tip (wanted by inbound links, orphans, dead ends, broken and double redirects, `wip`/`draft`/stub, stale),
  each row with its resolving action; `-/links/<Title>` backlinks with context; `-/raw/<Title>` on the
  content origin as `text/markdown; charset=UTF-8` with `ETag` and `Last-Modified`; the link relations in
  every head; schema.org JSON-LD per META-12; the site description document; `sitemap.xml` and
  `robots.txt` for public wikis, `noindex` for the others.
- **Required tests.** Feeds validate (a schema test, not a visual check); a restricted wiki's changes never
  reach a public feed; search folds width variants and diacritics and finds a Japanese title by a
  substring; every health view lists its planted case; `-/raw/` answers byte for byte with the stored file
  and only on the content origin; the sanitizer test of T3 repeated on a search snippet.

### T7 — Export, `import-folder`, `wiki create` in the binary (RFC 003 D-11, D-12; FL-08, FL-11; CT-06)
- **Prerequisite.** RFC 002's T3 (the key store), T4 (`Accept`) and T5 (the message contract) landed: the
  binary's writes go through `Accept` under the real key store. **The M0 bootstrap signer is not a
  production path**: it stays behind the `bootstrap` cargo feature, which the `inklkunn` binary never
  enables; a test that greps the binary's dependency features fails if it does.
- **Change scope.** `inklkunn wiki create <slug> --name …` through RFC 001's `create_wiki` with the real
  signer; `inklkunn wiki import-folder <slug> <dir>` committing a folder as one accepted change under the
  system principal (`Kind: import`), dry-runnable with a report (what would be written, what is refused and
  why: a path over the caps, a non-UTF-8 file, a frontmatter the parser refuses); `inklkunn wiki export
  <slug> --bundle <dir>` at fidelity level 3 (files as stored, `history/<page id>.jsonl` from `history()` and
  `change()` with the message's people as authors, `users.yaml`, the manifest with fidelity, interwiki map and
  checksums) and `--archive <file>` through `prikk archive export` (a read verb added to the driver with its
  cost rule and schema pin); both as audited admin acts, offered from `-/admin` as downloads from the
  content origin.
- **Required tests.** The export round trip (export, import into a fresh wiki, export) agrees on text,
  links, metadata and history; a dry run writes nothing; an import of the hostile-bundle fixture (ENF-4) is
  refused with every reason named; `prikk verify` passes on the imported repository independently of
  inklkunn; the feature test above.

### T8 — The read API and its description (RFC 003 D-13; API-01, API-02, API-04, API-07)
- **Change scope.** The GET half of IX-4 under a versioned path: pages (list with cursor pagination and
  filters, get as metadata plus rendered fragment, raw, fragment), history, revision, diff (with `path`
  required: no whole-tree diff on the API in M1 either), backlinks, health, search, changes; problem details
  (RFC 9457) with stable `type` codes and `message`/`hint` following `Accept-Language`; `ETag` and `304`;
  bounded pages; the OpenAPI document generated from the router and served at the documented path,
  advertised by `rel="service-desc"`; `authorize()` on every endpoint through the same registry as the web
  routes; existence-neutral 404s.
- **Required tests.** The OpenAPI document validates and names every route the router serves (a drift test
  both ways); every endpoint is in the route registry; a conditional request answers 304; a private wiki's
  endpoints answer 404 identically whether or not it exists; a problem detail's `type` is stable across
  languages while `message` changes.

### T9 — Threat model v0.1 re-verified, the accessibility statement, the 0.1.0 readiness report
- **Change scope.** The threat model's §7 tests re-run and the document's "verified on" line updated; the
  accessibility statement generated from what the tests checked, naming that no person using assistive
  technology has tested yet (unless one has: then who and when); the single-large-attachment measurement
  shape added to the spike A corpus and the cost table re-measured (RFC 010 D-1 step 5; cost.rs's `show`,
  `tree`/`cat`, `verify` multiples are planeter's until this runs); `docs/STATUS.md`, the CHANGELOG's 0.1.0
  section, the readiness report for the owner (what is green, what was done by a person, what is deferred).
- **Required tests.** All of the above green in both runs; the release rehearsal green on the candidate
  commit.

## QA checklist (from RFC 003's acceptance criteria and AC-M1)

- [ ] URL scheme: aliases, redirects, views, permalinks resolve as D-1; no title can collide with `-/`.
- [ ] A page view: no prikk call on a hit, at most two on a miss (counting test); cache rebuild identical.
- [ ] The badge is never read from the cache (test); every page shows the honesty words of RFC 002 D-6.
- [ ] The sanitizer neutralizes a script, an event handler, `javascript:` and `data:` URLs in a page, a
      diff and a search snippet; raw source and attachments only on the content origin.
- [ ] Diffs are always per path; the word-level refinement is labelled inklkunn's.
- [ ] Every catalog and structural test passes on every page state in English and Japanese; Japanese is
      labelled a preview until DR-7's reviewers sign off.
- [ ] Export round trip agrees on text, links, metadata and history; `prikk verify` passes independently.
- [ ] Route enumeration: every route and endpoint has its `Action` or public reason; a planted bare route
      fails.
- [ ] The binary refuses an unconfined sandbox without the explicit flag; the bootstrap feature is not in
      the binary.
- [ ] Dependency deltas measured per crate group with deny and audit; no license outside the allow-list.
- [ ] Both test runs (prikk on PATH, shimmed off) recorded with commands and exit codes; the release
      rehearsal green.

## Definition of done and handback

Done when every task's tests exist and pass in both runs, the gates are green in CI, the threat model is
re-verified, and the review request is filed as `003-read-path-and-rendering-review-request.md` beside this
file with: the implementation summary; the commits (hash → what landed); every gate's command and exit code,
both runs; the dependency-tree delta with deny and audit results for every manifest change; the
measurements (prikk calls per request, FTS5 against PERF-3, the re-measured cost table); the accessibility
checklist with each step marked done by a person or by a tool; deviations from this handoff and why; known
limitations; open questions for the architect. The architect reviews against RFC 003, the two lenses above,
correctness, security and evidence, and answers Approved, Conditionally Approved, Corrections Required,
Design Reconsideration Required, Requirements Clarification Required or Owner Decision Required. The 0.1.0
cut itself is proposed to the owner with the readiness report and executed only on the owner's
authorization (ROADMAP §5).
