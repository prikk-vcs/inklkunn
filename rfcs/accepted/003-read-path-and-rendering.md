# RFC 003 — Read path and rendering: re-derive from prikk, render the profile safely, show the truth

**Status.** **Accepted** by the owner on 2026-10-27 ("RFC 003 is accepted."); proposed the same day by the
architect. Handoff: `../handoffs/003-read-path-and-rendering/read-path-and-rendering-handoff-v1.md`.
**Concretized by the architect on 2026-10-28**, within the accepted intent, from measurements on prikk 0.54.1 (issues
IS-8 … IS-10 in `docs/STATUS.md`): D-2 builds the title index from one materialized tip; D-11 runs `archive export`
in the signer crate's keyless writable mode, beside the checkpoint refresh. **And on 2026-10-29**, after the review of
handoff 003's T1 – T3: D-2 reads at the tip's block id and counts three calls on the first miss at a new tip; D-3 escapes
author HTML while the profile's subset is empty (IS-14); D-6 chooses a preview language by the reader's cookie only.
**Tracks.** ROADMAP M1 (0.1.0), theme T3. Requirements `RD-1 … RD-11`, `RD-14`, `CM-3 … CM-5`, `CM-9`,
`INC-1 … INC-8`, `IX-1`, `IX-2`, `IX-5`, `IX-6`, `IX-8`, `SEC-2`, `PRV-1`, `PRV-4`, `PERF-1`, `OPS-5`;
external design `WEB-01`, `WEB-02`, `WEB-06`, `WEB-09`, `WEB-10`, `API-01`, `API-02`, `API-06`, `API-07`,
`FL-01`, `FL-08`, `CT-05`, `CT-06`, `CT-07`, `OP-02`, `OP-03`, `OP-06`; internal design `RD-*`, `DM-6`,
`DM-7`, `MK-1 … MK-6`; threat model `T-6`, `T-7`, `T-10`, `INV-5`, `INV-6`.
**Touches.** `inklkunn-web` (routes, templates, catalog, sanitizer, headers, content origin, feeds, OpenAPI),
`inklkunn-core` (the read service, the title index, the cache, export), `inklkunn-store` (the derived cache
tables), `inklkunn-markup` (rendering refinements found by the pages). **Not here:** sign-in pages beyond
what RFC 002 needs for tests, proposals (RFC 004), the knowledge and governance surfaces, import (RFC 007).

## Summary

M1 is a real, browsable, honest, accessible wiki over a prikk repository that an operator filled. This RFC
fixes how a page is found and shown: **re-derived from prikk and cached by content id, never authoritative in
the service store** (INV-5); **rendered from the Portable Wiki Markdown profile through a sanitizer under a
strict policy, with raw bytes on another origin** (T-6); **honest about verification** (INV-6); **readable by
anyone, in two languages, without a script** (INC). It establishes the URL scheme, the catalog and the
accessibility tests every later page inherits, and the export that makes the wiki portable from the first
release.

## Decisions

- **D-1 — URL scheme.** `/<wiki slug>/<Title>` is a page in the default view, the title readable in its own
  script and percent-encoded as UTF-8 (I18N-2); `/<wiki slug>/~<view>/<Title>` a page in a named view;
  `/<wiki slug>/-/…` the wiki's own surfaces, so no title can collide with them: `-/history/<Title>`,
  `-/at/<block id>/<Title>` (a permalink), `-/diff/<from>/<to>/<Title>`, `-/raw/<Title>` (served only on the
  content origin), `-/changes` (and `.atom`), `-/search`, `-/health/{wanted,orphans,dead-ends,redirects,
  wants-help,stale}`, `-/links/<Title>` (backlinks), `-/proposals/…` (RFC 004), `-/about` (the license, the
  statement, the site description link). A title that resolves through an alias or a redirect answers with
  the page and says *redirected from*. A missing title answers 404 with *This page does not exist yet* and
  *Propose it* (NAV-3). `/-/` at the root is the instance's own: `/-/wikis`, `/-/sign-in`, `/-/lang`.
- **D-2 — The read pipeline** (internal design RD-1). Slug → wiki record → `authorize(read)` → the view's tip
  from `branch` (held for a few seconds) → the title index at that tip (rebuilt from frontmatter on tip
  change, from **one `extract` of the tip** and never one `cat` per page, since each `cat` replays the whole tree;
  stored in the derived cache; an `extract` refused for want of prikk's checkpoint record is *could not be read now*
  and asks for the checkpoint refresh of D-11, never a full replay from a request) → the path → the cache by (tip, path, language, presentation) → on a miss, `cat` at
  the tip **by its block id** (so the key is the content read), parse, resolve, render, store. A page view issues a
  bounded number of prikk calls: none on a hit, one on a miss, **three on the first miss at a new tip** (`views`,
  `extract`, `cat`).
- **D-3 — Rendering and safety.** The markup tree from `inklkunn-markup` is serialized to HTML and passed
  through an allowlist sanitizer (`ammonia`, the house choice) before any template sees it; the raw-HTML
  subset of the profile is the sanitizer's allowlist and nothing more. **While that subset is empty, raw HTML an
  author wrote is rendered as escaped, visible text** (the profile's hidden comments stay hidden): the markup crate marks
  what the renderer writes so that an author's HTML can never pass as it, a forged wiki link included. A strict CSP (`default-src 'none'`,
  `script-src` absent, one stylesheet from the app origin), `nosniff`, `frame-ancestors 'none'`, a
  `Referrer-Policy`, `Permissions-Policy`, and `Cache-Control: private, no-cache` with `Vary` on dynamic
  pages. `-/raw/` and attachments answer only on the configured content origin, as inert attachments with
  `nosniff`; a non-loopback bind whose content origin is the application's host refuses to start (planeter's
  IS-16 rule, adopted).
- **D-4 — History, diff and permalinks.** The history page lists changes from `log` with time, the people
  from the message (RFC 002 D-5), the summary and the flags; a change page shows what the change did from
  `show`; the diff page renders prikk's `diff --from --to` hunks for the page's path with a word-level
  refinement computed by inklkunn and labelled as inklkunn's, with `ins`/`del` and a text summary (A11Y-5);
  `-/at/<block>/<Title>` renders the page at that point with a banner linking to the current one. Rename
  events show as events.
- **D-5 — The badge and its truth.** Every page carries the repository's last verification state (OP-06) in
  RFC 002 D-6's words, re-derived from the wiki record's last `verify` and never from the page cache; the
  verification page runs `verify` on request and shows what was found in plain words, the stage names as
  data. A change page says *Signed into the history* and *signed by the wiki service on behalf of …*.
- **D-6 — The catalog, the glossary and the accessibility tests.** A message catalog keyed by stable
  identifiers, whole sentences with named placeholders and plural forms, English shipped and Japanese
  compiled in as a preview until its reviewers sign off (DR-7); negotiation by `Accept-Language` among the
  **reviewed** languages and a cookie switcher at `/-/lang` with no script (a preview language is chosen only by the
  reader's cookie, so no browser setting serves an unreviewed translation unannounced); `<html lang>` and `dir`; logical CSS only. A glossary in
  `docs/glossary.md` fixes every term, reusing planeter's where the concept is the same; tests fail on a
  missing key, an orphan key, a placeholder mismatch, markup in a message, a literal user-facing string
  outside the catalog, a retired word (`sealed`, `commit`, `patch`, `block`, `ref` outside explanations).
  Structural tests over every page state: one `h1`, landmarks, a skip link, labels, alt text, `lang`/`dir`,
  heading order, table captions and headers, link text, unique titles; stylesheet tests: contrast in both
  schemes, visible focus, 24 px targets, no physical-direction properties, reduced motion. An accessibility
  statement at `/-/about` names what was checked by tools and that no person using assistive technology has
  tested yet.
- **D-7 — Recent changes and feeds.** `-/changes` lists changes across the wiki's views from `log`, filtered
  by `authorize()`, by namespace, view, minor and automated; Atom feeds for the wiki, a namespace and a page,
  each entry with the people, the summary and links to the page and the diff.
- **D-8 — Search.** SQLite FTS5 over titles and aliases (M1) with a trigram tokenizer, rebuilt per tip
  change; results with title and snippet; *create "…"* offered when nothing matches exactly, with
  similar-title suggestions first (NAV-5); an OpenSearch description document.
- **D-9 — Health views** from the link graph at the tip: wanted pages ranked by inbound links, orphans, dead
  ends, broken and double redirects, pages with `status: wip | draft` or a stub marker, pages unchanged
  longest; each row with its resolving action (NAV-15).
- **D-10 — Discovery.** `-/raw/<Title>` as `text/markdown; charset=UTF-8` with the profile in the manifest
  named, `ETag` and `Last-Modified`; link relations in every head (`alternate` text/markdown and atom,
  `canonical`, `bookmark`, `edit` (RFC 004), `version-history`, `license`, `replies`, `describedby`,
  `service-desc`, `search`); schema.org JSON-LD per META-12; the site description document; `sitemap.xml`
  and `robots.txt` for public wikis, `noindex` for the others.
- **D-11 — Export.** `inklkunn wiki export <slug> --bundle <dir>` builds a Portable Wiki Bundle at fidelity
  level 3: the files as stored, `history/<page id>.jsonl` from `log` and `show` with the message's people
  as authors, `users.yaml` (pseudonymizable), the manifest with fidelity, interwiki map and checksums; `--
  archive <file>` runs `prikk archive export`, which writes prikk's own lock files and refuses on a read-only
  repository, so it runs in `inklkunn-signer`'s **keyless writable mode** (the repository writable, no key mounted),
  serialized with the wiki's writes. The two exports are admin acts, audited; the web surface offers them from
  `-/admin` in M1 as downloads from the content origin. The same keyless mode's only other operation is the
  **checkpoint refresh** (a `verify` with the repository writable), which keeps prikk's checkpoint record current
  for `extract` after a prikk upgrade or a restore; the binary runs it at start where needed, and it is recorded in
  the audit log like any act on a repository.
- **D-12 — Filling an M1 wiki.** `inklkunn wiki import-folder <slug> <dir>` commits a folder of pages and
  attachments as one accepted change under the system principal (`Kind: import`) through RFC 002's `Accept`,
  so the read path ships with content before the proposal loop exists (internal design IQ-6).
- **D-13 — The read API.** The GET half of IX-4, described in an OpenAPI document generated from the router:
  pages (list, get, raw, fragment), history, revision, diff, backlinks, health, search, changes; problem
  details with stable codes; `ETag`/`304`; bounded pages. Writes arrive with RFC 004.

## What "done" means (acceptance criteria)

AC-M1 of the requirements, plus: the URL scheme resolves aliases, redirects, views and permalinks as D-1
says; a page view issues no prikk call on a cache hit and at most two on a miss (a counting test); the
cache-cleared rebuild yields identical pages (ENF-5); the badge is never read from the cache (a test);
the sanitizer neutralizes a script, an event handler and a `javascript:` URL in a page, a diff and a search
snippet; the raw route answers only on the content origin; every catalog and structural test passes on every
page state in both languages; the export's round trip (export, import into a fresh wiki, export) agrees on
text, links, metadata and history.

## Alternatives considered

- **Identifier URLs with a slug.** Rejected for readers: titles are the wiki's address (NAV-1); ids are in
  frontmatter and in the API.
- **Client-side rendering.** Rejected: reading without scripts is a requirement (INC-4, WEB-1).
- **Index history into the service store.** Rejected: INV-5; the cache is keyed by prikk's ids and rebuilt.
- **A JavaScript diff.** Rejected: the diff is server-rendered and accessible; a script may enhance later.

## Open questions

- Whether `-/at/<block>` permalinks should also exist per patch (a change within a block): decided in the
  handoff after `show`'s shape is measured.
- The first catalog's Japanese reviewer (DR-7): **the owner** (2026-10-29). The dev team files the catalog as a
  review request (every message in English and Japanese, with where it appears); the owner's sign-off ends the preview.

## Sequencing and handoff

On RFC 001's and RFC 002's base (the driver, the markup crate, the wiki record, the signer for D-12). On
acceptance the architect writes the handoff; the dev team builds M1 after M0's review.
