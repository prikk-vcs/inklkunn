# Review request — handoff 003 (read path and rendering), tasks T1 – T3

| | |
|---|---|
| Handoff | `read-path-and-rendering-handoff-v1.md` (RFC 003, accepted 2026-10-27). **T1, T2 and T3 only**: T4 onward (the page view, history, diff, search, raw and attachments, export and import, the read API, the accessibility statement) is not started, and several of those tasks wait for things named below |
| Implementer | the dev team (the mid-capability model), session of 2026-10-28, built together with handoff 002 (`../002-identity-keys-and-signing/002-identity-keys-and-signing-review-request.md`). T2 and T3 were built by two sub-agents in isolated worktrees under the same rules; I merged their commits, re-ran every gate on the merged tree, and sampled their claims (below) |
| Reviewer | the architect, against RFC 003, the handoff and the owner's two lenses: *finally clean, safe and secure, robust and sophisticated*; *no reader, operator or API client is left confused or misled* |
| Submitted | 2026-10-28 |
| State | local `main`, none pushed |

## What to read first

**1. PERF-1 is met at 76 MiB and is at the line at 212 MiB** (open question 5). The handoff says: *if PERF-1
fails, say so and stop*; it does not fail, and I did not stop, but I did not change the design either, because the
margin is 2 %. A cold page on 10,000 pages (a new service, an empty store, tip + title index + the page) takes
**0.55 s at 76 MiB** and **0.98 s at 212 MiB** (the T3 agent measured 0.97 s, with single runs up to 1.11 s; I
re-measured: five runs, 0.969 – 0.982 s). 0.64 s of the 0.98 s is the title index, built once per tip, so the
first reader after every seal pays it. A cached view takes 0.2 ms at p95. Whether "under 1 s" is a median or a p95
at this shape is yours to say (the requirement calls PERF-1 provisional); the two levers are in the measurements
section (serve the page that triggered the build from the extracted tree before it is removed; build the index
right after a seal).

**2. The sanitizer's policy is the renderer's vocabulary, not "exactly the profile's raw-HTML subset, today empty"**
(open question 6). The handoff asks for a policy that is exactly the subset (empty), applied before any template
sees the HTML. What was built is the strongest policy that is possible *without changing the markup crate's
pre-pass*: ammonia with an allowlist generated from the renderer's own vocabulary (`render::vocabulary`) plus
`HTML_SUBSET`, which is empty. Nothing that runs, embeds or loads survives (scripts, handlers, frames, objects,
forms, remote images, any scheme but http, https, mailto and tel; a drift test holds the allowlist to what the
renderer can write), and `SafeHtml` can be made only by the sanitizer. But the renderer's own output and an
author's HTML reach the sanitizer as the same kind of text, so **an author's HTML that spells an element the
renderer writes (a plain `<a href>`, a `<table>`) survives**, with the renderer's classes, and without
`rel="nofollow"`. Closing that needs unforgeable markers in the markup pre-pass (the agent did exactly that for
transclusion) so that author HTML is escaped when the subset is empty; that is a markup decision and I did not take
it.

**3. A decision I took for you in T2, which the handoff's text does not say** (open question 1): **a preview language
is never chosen from `Accept-Language`; only the reader's cookie selects it.** The handoff says "negotiation by
`Accept-Language`, a cookie switcher at `/-/lang`" and "Japanese compiled in as a preview (labelled so in the
switcher …)". A browser that asks for `ja` would, read literally, be served an unreviewed translation of the trust
labels without ever being told it is a draft; the second lens says no display says more than is known. So Japanese
is offered on every page (the footer switcher and `/-/lang`, with the word "preview" beside its name), and a
reader who chooses it keeps it in one `HttpOnly`, `SameSite=Lax` cookie; a browser asking for `ja` gets English.
Reversal is one line (`.filter(|l| !l.is_preview())` in `Lang::negotiate_in`) plus the test tables; it becomes
automatic when `JA_DEF.review` is set to `Review::Reviewed { by }` at the reviewers' sign-off (DR-7). **Please
confirm or reverse.**

## What landed (hash → task)

| Commit | Task |
|---|---|
| `837763b` | **T1** the web foundations: a validated configuration (the bind address and the two origins checked together), the security and cache headers set in one function, the `Host` dispatcher between the application and content origins (421 for anything else), the route registry with a test that no route can shadow a title, a server with a graceful stop; `inklkunn serve [--config FILE]` and `--version` (axum, tokio 1.54.1, tower-service: lock 129 → 155) |
| `7b511ef` | the dependencies T2 and T3 would use, **measured before either used them** (RFC 010 D-6): `askama` 0.16, `ammonia` 4, `html5ever` 0.40 (dev); lock 155 → 209 |
| `bf1bc3c`, `cffd092` | **T2** the message catalog, the glossary, the layout and the one stylesheet, and the catalog, structural and stylesheet tests every page inherits; the changelog entry. No new dependency |
| `63b6bf0` | **T3** the read path: the read service (the held tip, the title index from one `extract`, the page cache, the counting harness), the sanitizer and `SafeHtml`, block ids and transclusion in `inklkunn-markup`, store schema 5 and the driver's size hint. A worktree agent's commit, cherry-picked onto `main`; two conflicts, both adjacent lines (core's `lib.rs` exports, one `discard(...)` call), resolved by keeping both sides |
| `c9347b1` | a test that pins what a dropped element takes with it (a planted mutation of mine survived the T3 tests) |
| `b0090ec` | the T3 changelog entry; `EXPECTED_SKIPS` 55 → 57 |

## T1 — the web foundations (RFC 003 D-5)

Two origins on one listener: a request goes to the router of the origin its `Host` names, and a host that names
neither is misdirected (421), so the content origin never answers an application route and the application never
serves wiki-authored bytes. Every response carries the headers (200, 304, 404, 405, 421, errors) from one function a
handler cannot override for the security headers: a Content-Security-Policy with `default-src 'none'` and **no
script source at all**, `style-src` the application origin only, a sandboxed policy on the content origin,
`nosniff`, `Referrer-Policy: same-origin`, `Permissions-Policy`, frame denial, cross-origin policies, HSTS on https,
and `Cache-Control: private, no-cache` with `Vary` unless a handler says otherwise. Routes are registered in
`routes.rs` only (a workspace gate, which the second reading of handoff 002 hardened), each with an `Action` or a
stated public reason, and a test fails any route that could shadow a page title. The binary validates at start and
names every refusal: the bind address and origins together (http only for loopback; the origins differ; on a
reachable address the content origin has its own host), `Sandbox::Unconfined` only with `--dev-unconfined` (a banner;
not settable from the environment), prikk at the floor, the store opens. The only route in T1 was `/-/health`.

## T2 — the catalog, the glossary, the layout and the stylesheet (RFC 003 D-6; planeter RFC 012 D-2 … D-5)

- **The catalog** (`catalog.rs`, `catalog/{en,ja}.rs`; no new dependency): a `Key` enum (22 keys), whole sentences
  with named placeholders, plural forms by a per-language rule, and typed slots (`Text`, `Link`, `InLang`) so a
  message cannot carry markup; `audit()` fails on a missing key, an orphan, a duplicate, placeholders that differ
  between languages, plural forms that disagree, markup, and braces. A missing key renders `[the.key]` and the
  audit fails first.
- **Language at the edge** (`lang.rs`): the `inklkunn_lang` cookie (`SameSite=Lax; HttpOnly`, `Secure` when the
  application origin is https); `Accept-Language` negotiation for the shipped language; `safe_back` (a local address
  only: no `//`, no backslash, no control character, bounded) and `back_for` (the sign-in and switcher queries are
  never copied into another page); `from_this_site` (a `Sec-Fetch-Site` / `Origin` check).
- **The pages** (`pages.rs`, `templates/{base,notice,language}.html`): a `Page` extractor holding the negotiated
  language and the return address; the 404 page (an unknown address on the application origin is now a page in the
  visitor's language, with the same headers as every response), the language page (`GET /-/lang`), the switch
  (`POST /-/lang`: a form per language, no script, a 303 with the cookie, a refusal page for a form from another
  site and for a language this build does not have). The layout has a skip link first, a banner, one `main`, a
  footer with a labelled language switcher, `lang` and `dir` on `<html>`, and no script. The 421 and the content
  origin's 404 are one catalog sentence as plain text (those origins serve no page).
- **The stylesheet** (`static/site.css`, `style.rs`): one sheet, light and dark colour tokens, logical properties
  only, served at `/-/assets/site-<fnv1a-64>.css` with `Cache-Control: public, max-age=31536000, immutable`. The
  address carries a fingerprint of the bytes, because `immutable` is only safe when the name changes with the
  bytes; a stale name gets the 404 page.
- **The glossary** (`docs/glossary.md`): the concepts in both languages, a message table (key, where it appears,
  English, Japanese), the retired words (`seal`, `commit`, `rebase`, `patch`, `block`, `ref`: the handoff's list plus
  `rebase`, INC-3), and how it is used. A test holds it **equal to both catalogs in both directions**.
- **The tests every later page inherits**, each with its planted violation beside it: the catalog audit; unused
  keys; literal text in a Rust source or a template outside an allowlist (`tests/literals.allow`) whose dead lines
  fail the build; `|safe`; the retired words (`tests/retired_words.rs`, read from the glossary so there is one
  source); **nineteen structural rules** over every page state in both languages, parsed with `html5ever` (parse
  errors, `lang`/`dir`, one `h1`, heading order, skip link, landmarks, nav names, form labels, button text, tables,
  images, icon glyphs, `aria-hidden` on focusable elements, link text, a unique `<title>`, a form in phrasing
  content, inline script or style, unique ids, dangling id references); and the stylesheet checks (contrast of every
  foreground/background pair in both schemes **computed from the tokens**, colours only as tokens and every token in
  both schemes, logical properties only and no asymmetric four-value physical shorthand, visible focus, reduced
  motion, 24 px targets, forced colours, and nothing loaded: no `@import`, `@font-face` or `url(`).

### How I checked T2 (it was built by an agent, so its report is a claim)

- The merged tree passes every gate (table below). The agent's own count of self-skips (43) equalled the baseline,
  and mine after the T2 merge was 55 (the baseline after handoff 002's second batch, before T3's +2): T2's tests add none.
- I read `lang.rs`, `pages.rs`, `routes.rs` and `style.rs` in full for the places that matter to the second lens
  (the return address, the form check, the cookie, the cache header).
- **I applied 11 planted mutations of my own choosing** to the web crate in a throwaway worktree and ran its tests
  after each; all 11 were caught, none by a compile error, and the worktree was restored clean: `safe_back`
  accepting `//host`; `same-site` counted as this site; any `Origin` accepted; the cookie losing `HttpOnly`; the form
  body limit raised; the sheet losing `immutable`; a stale sheet name still served; the POST return address
  unchecked; `?next=` unchecked; the sign-in query copied into the switcher; pages losing `Content-Language`.
  (The agent reports 25 of its own, also caught; I did not repeat those.)
- I confirmed the count of structural rules (19) against `RULES` in `tests/structure/rules.rs`.

## T3 — the read service, the title index and the derived cache (RFC 003 D-2; DM-6, DM-7; INV-5)

**Nothing in the product calls the read service yet**: `inklkunn serve` does not construct it, and no route uses it
until T4. It is exercised by its tests, by a real prikk, and by a measurement tool.

- **`inklkunn-core::read`** (`read.rs` and `read/{sanitize,index,tip,counting,links}.rs`): `ReadService::{tip,
  title_index, lookup, page, forget_wiki, clear_derived, …}` over `Storage` and `inklkunn-markup`.
  - **The tip** of a view comes from `views()` and is held for `tip_ttl` (5 s) per wiki: a held answer makes no prikk
    call.
  - **The title index** (title, aliases, redirect target, page id, path; `Lookup::{Found, Missing, Ambiguous,
    BrokenRedirect}` with *redirected from*) is built from **one `extract` of the tip** into the wiki's scratch
    directory (IS-10: never one `cat` per page), bounded (`index_max_bytes` 4 GiB, `index_max_files` 200,000), once
    per tip however many readers meet it (single-flight per wiki; a build refused for a reason that will not change
    is remembered for one `tip_ttl`, so prikk is not spawned for every request). Any unreadable file in the tree
    fails the build; it is never silently an entry. `extract`'s refusal for want of prikk's checkpoint record
    (`no-checkpoint-in-reach` / `not-recorded`) is the ordinary *could not be read now*, names its reason for the
    operator, and flags the wiki for a refresh; **no request path ever passes `--allow-full-replay`** (the driver's
    sources are scanned for the word by a test, and the extract bounds have no field for it).
  - **The page cache** is keyed by (tip state id, path, catalog language, presentation) and holds the rendered
    HTML, the metadata, the link sets and the table of contents. A cache that cannot be read or written is a miss
    and is counted (`store_failures()`), never a failed page. Rows for tips no view holds are dropped after a
    build; at most 64 indexes are kept in memory over all wikis.
  - **`SafeHtml`** has a private field, no constructor, no `Default`, no `From<String>`: only `Sanitizer` makes
    one, and the read service returns every page as one, **including a page read back from the derived cache** (a
    row of the database is input like any other, T-10; a test alters a cached row in SQLite and shows it is cleaned
    on the way out). The cache record has **no field for a verification result** (the type-level rule).
  - **The counting harness** (`CountingStorage`, `Calls`) wraps any `Storage` and counts calls by verb.
  - `examples/measure_index.rs` drives the real product path (`PrikkCli`, `ReadService`, `SqliteStore`) against a
    repository; it is how the numbers below were taken.
- **`inklkunn-store`: schema 5** (v4's accounts, sessions and tokens are kept): `derived_pages`, `title_index_tips`
  (an index is whole when its tip row says so; its entries are written in the same transaction),
  `title_index_entries`, and two columns on `wikis` (`checkpoint_prikk_version`,
  `checkpoint_refresh_requested_at`). `DerivedStore` is the trait (memory and SQLite implementations); `WikiStore`
  gains `request_checkpoint_refresh` and `record_checkpoint_refresh`; `Checkpoint::needs_refresh` is what T7's
  runner will read. A database at 4 migrates forward with its wikis and only once; a database one version ahead is
  refused at open and left as it was.
- **`inklkunn-prikk`**: `StorageError::Refused` carries the refusal's `reason` (only `no-checkpoint-in-reach` sets
  it: `not-recorded`, `signer-not-adopted`, `no-checkpoint`; ledger PK-24); `SizeHints` is a shared, expiring
  stand-in for the store walk that the cost table needs (`PrikkCli::with_size_hints`; the walk stays the fallback
  and the truth after a write; the flat verbs `branch` and `log` no longer walk the store at all).
- **`inklkunn-markup`**: a block identifier becomes the `id` of its enclosing block (MKUP-12; a marker alone on a
  paragraph attaches to the previous block, and an unplaceable one stays as an empty `<span class="block-id">`);
  `![[Page]]` is rendered to **depth 2**, a cycle is refused and the cycle is named ("A → B → A"), with a budget of
  32 pages and 4 MiB per page; included pages get their headings demoted and their ids prefixed; the renderer's
  vocabulary is one module (`render/vocabulary.rs`), from which the sanitizer's allowlist is built, with a drift
  test; `parse_metadata` is new. `RenderLinks` gained `transclusion`, `transclusion_body` and `transclusion_notice`.
  The M0 `render` tests that asserted a block id as a separate span now assert `<p id="^p1">`.

### How I checked T3 (it was built by an agent, so its report is a claim)

- The merged tree passes every gate (below). The agent's two new self-skips (the real-prikk tests of the read path)
  are the +2 in `EXPECTED_SKIPS`.
- **I re-measured PERF-1 myself** with the agent's tool on the same corpora (table below): five runs per corpus; the
  numbers agree with its report (the largest difference, 5 %, is in the walk-and-parse column).
- **I applied 10 planted mutations of my own choosing** to the read path in a throwaway worktree (the agent's, with
  a warm build) and ran the affected crate's tests after each: `javascript:` and `data:` schemes allowed; remote
  images kept; `<script>` not dropped whole; any class kept; the cache key ignoring the presentation; the cache
  key ignoring the language; no single-flight on the index build; the tip never held; transclusion depth 3; and
  `iframe` removed from the dropped-whole list. **Nine were caught. The tenth survived**: every attack case used an
  *empty* frame, so whether the text inside one reaches a reader was untested. I added a test that names the
  fourteen dropped-whole elements itself (a loop over the list it checks cannot notice an entry removed from it)
  and it now catches the removal of thirteen of them; `template` is the exception because the HTML parser never
  puts a template's contents among its children, so removing it changes nothing. Worktree restored clean.
- I read `sanitize.rs` and `ReadService::{tip, page, render, index_at}` in full (the policy, the cache path, the
  sanitize-on-read, the single-flight).

## Gates: commands and exit codes

Run from the repository root on local `main` at **`b0090ec`** (the tree contains handoff 002's work too; this is
the same run as the 002 request's). Exit codes are cargo's own, taken without a pipe in front.

| Command | Exit | Result |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 | |
| `cargo +1.88.0 clippy --workspace --all-targets --locked -- -D warnings` | 0 | the declared minimum compiler |
| `cargo test --workspace --locked` (prikk 0.54.1 and bubblewrap on `PATH`) | 0 | **676 passed, 0 failed, 0 ignored**; no self-skip line. The boundary gates (layering, signer reachability, prikk spawn, markup purity, routes, write verbs, surfaces and the signer) are among them |
| the same with prikk off `PATH` (a local copy of `ci.yml`'s shimmed step: the toolchain's own binaries, `prikk`'s directory removed from `PATH`, `-- --nocapture`, the `skipping: prikk not on PATH` lines counted) | 0 | **676 passed, 0 failed, 0 ignored; 57 self-skips = `EXPECTED_SKIPS`** (`ci.yml` and `release.yml` both say "57") |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` | 0 | |
| `cargo deny check` | 0 | advisories ok, bans ok, licenses ok, sources ok |
| `cargo audit` | 0 | 209 crate dependencies, no vulnerability |

**Not run:** CI itself (the shimmed step above is my copy of it, not the job); the release rehearsal; anything on
aarch64; the Chromium run is the T2 agent's one-off and is not repeatable from the repository.

`EXPECTED_SKIPS` moved 43 → 55 (handoff 002's second batch of real-prikk tests) → 57 (the two real-prikk tests of the
read path, `core/tests/read_path.rs`). One further line, `skipping: running as a user for whom the mode does not
bind`, prints only when the prikk driver's store-walk test runs as root; it is not counted.

## Dependency deltas (RFC 010 D-6; each measured before its commit, rows in `docs/dependency-policy.md`)

| Change | Lock entries | `cargo deny check` | `cargo audit` | Other |
|---|---|---|---|---|
| `axum`, `tokio` 1.54.1, `tower-service` (T1) | 129 → 155 (+26) | 0 | 0 | past RUSTSEC-2026-0386; +1.5 MB release binary |
| `askama` 0.16, `ammonia` 4, `html5ever` 0.40 dev (for T2 / T3) | 155 → 209 (+54) | 0 | 0 | new licences Unicode-3.0 and MPL-2.0, both already allowed |
| T2 | +0 | n/a (no manifest change) | n/a | the handoff's rule: no new dependency for the catalog |
| T3 | +0 | n/a (no manifest or lock change; `Cargo.lock` is byte-identical across the T3 commit) | n/a | `ammonia`, adopted with the row above, is used by `inklkunn-core` for the first time |

## Measurements

**Method.** `cargo run --release -p inklkunn-core --example measure_index -- REPO [--runs N] [--confined]`: the real
product path (`PrikkCli`, `ReadService`, `SqliteStore`), prikk 0.54.1, a 32-core machine, repositories linked into a
scratch root on **tmpfs**. Every run starts cold: a new service and an empty store. Corpora from
`inklkunn-corpus wiki` with spike A's exact arguments (`--depth 500 --blocks 4` / `8`, seeds 1 – 4; the repositories
are kept in the session's scratchpad for re-runs). Seconds, except the last column. The first four rows are the
T3 agent's medians of five; the last two rows are **mine**, medians of five (ranges in brackets), taken after the
merge.

| corpus | pages | store S | `extract` | walk + parse | **title index** (incl. store write) | first page after the index (`cat` + render) | **cold page** (tip + index + page) | cached view p95 |
|---|---|---|---|---|---|---|---|---|
| w-1m-1k | 1,000 | 3.6 MiB | 0.033 | 0.009 | 0.061 | 0.016 | 0.079 | 0.02 ms |
| w-16m-1k | 1,000 | 18.6 MiB | 0.080 | 0.014 | 0.114 | 0.058 | 0.175 | 0.10 ms |
| w-64m-10k | 10,000 | 76.3 MiB | 0.242 | 0.062 | 0.373 | 0.168 | 0.554 | 0.09 ms |
| w-200m-10k | 10,000 | 212.3 MiB | 0.442 | 0.110 | 0.631 | 0.335 | **0.967** | 0.19 ms |
| w-64m-10k (mine) | 10,000 | 76.3 MiB | 0.247 | 0.062 | 0.382 | 0.168 | 0.553 (0.545 – 0.562) | 0.09 ms |
| w-200m-10k (mine) | 10,000 | 212.3 MiB | 0.446 | 0.116 | 0.636 | 0.336 | **0.979** (0.969 – 0.982) | 0.18 – 0.20 ms |

PERF-1: *cached view under 100 ms at p95; cold page under 1 s on a wiki of 10,000 pages.* The cached view passes by
three orders of magnitude (this is service time with the tip held and the cache hit; HTTP and templates are T4's).
The cold page passes at 76 MiB and is **at the line at 212 MiB**: the agent's runs ranged 0.96 – 1.11 s (some single
runs over 1 s), mine 0.969 – 0.982 s. The agent's confined (bubblewrap) run was within noise of its unconfined
ones; I did not repeat it. The index build recurs for the first reader after every new tip, so after every seal.

- **Lever 1** (the one the handoff names): serve the page that triggered the build from the extracted tree, before
  the directory is removed. Estimated to save about the page's own `cat`, 0.34 s at 212 MiB. An estimate, not a
  measurement.
- **Lever 2**: build the index right after a seal, so that no reader pays for it. Unmeasured.
- **Not measured:** disks other than tmpfs; behaviour under concurrent load (PERF-3); the resident size of a
  10,000-entry in-memory index (estimated at a few MiB) and the size of the derived store at scale.

**Prikk calls per request** (counting harness; `read/tests/counts.rs`, and the real-prikk run in
`tests/read_path.rs`): a **hit makes none** while the tip is held (one `views` call when the 5 s hold has expired);
a **miss makes one `cat`** once the title index exists; **the first view at a new tip makes three** (`views`,
`extract`, `cat`: my measurement's `calls` column reads `views 1 extract 1 cat 1`), and however many readers meet
the new tip there is one `extract`. A transcluded page costs one more `cat`. The handoff's "at most two on a miss"
therefore holds with the tip held and the index built, and not for the first miss at a new tip (deviation 16).

**The cache rebuilt is the same cache** (ENF-5): `the_cache_cleared_and_every_view_rebuilt_yields_identical_pages`,
and after a restart the index is found in the derived store and not extracted again. This turned up a
nondeterminism worth knowing about: ammonia emits an attribute it has to *add* in a hash map's iteration order, so
two sanitizer instances disagreed on the order of `type` and `disabled` on a checkbox; the policy now allows both
attributes on `input` so that the renderer's order survives, and a regression test holds the same bytes from every
instance.

## Accessibility: what was checked, by whom

There is no checklist file for this release yet (the accessibility statement is T9, generated from what the tests
checked). What was checked in T1 – T3, each by a tool unless it says otherwise:

| Check | Done by | Where |
|---|---|---|
| One `h1`, heading order, landmarks, skip link first, labels, link text, unique `<title>`, `lang` and `dir`, no script, valid parse | tool: the 19 structural rules over every page state in English and Japanese | `tests/structure/` |
| Contrast of every colour pair in light and dark | tool: computed from the CSS tokens | `tests/stylesheet/` |
| Logical properties only; visible focus; reduced motion; 24 px targets; forced colours | tool: stylesheet checks | `tests/stylesheet/` |
| No sideways scroll at 320 px and 1280 px, in both languages, with and without the WCAG 1.4.12 text-spacing override; the first Tab reaches the skip link; a 2 px focus ring; the dark scheme applies; a real form post from `/-/lang` is accepted and the choice persists | tool: **a one-off headless Chromium 156 run over the DevTools protocol by the T2 agent; a throwaway script, not in the build, so not repeatable from the repository** | none |
| A person using a screen reader or another assistive technology | **nobody. Nothing here claims conformance** | |
| A fluent reader of the Japanese | **nobody.** The Japanese is a draft written by a model; DR-7's reviewers are still unnamed | `src/catalog/ja.rs`, `docs/glossary.md` §2 |
| A real right-to-left language | **not rendered.** A test-only language checks `dir="rtl"` and that every word comes from the catalog, nothing more | |

## Deviations from the handoff or RFC text, and why

1. **A preview language is chosen only by the reader's cookie** (the section at the top).
2. **The stylesheet's address is fingerprinted** (`/-/assets/site-<hash>.css`), not a fixed path: `immutable` is
   only safe when the name changes with the bytes. The handoff does not name the address.
3. **The switcher is a POST form with a same-origin check, not a CSRF token.** The CSRF machinery lives in
   `inklkunn-auth`, which `inklkunn-web` does not depend on, and the form can only set a language cookie from a
   closed list. A form from another site gets a 403 page; a request with neither `Sec-Fetch-Site` nor `Origin`
   (a command-line client, an old browser) is accepted. The cookie is `HttpOnly` (planeter's was not: no script
   ever reads it).
4. **The language page and the refusal pages have no footer switcher** (they carry their own way to choose).
5. **`/-/health` and the 404 handler moved out of `routes.rs`** (`probe.rs`, `pages.rs`), so that `routes.rs` holds
   only the registry and can be a `scope` entry in the literal-text check.
6. **The literal-text allowlist is simpler than planeter's** (no regex mini-engine: kinds `scope`, `exact`, `prefix`
   and `safe`). Scoped files, each with a reason: `catalog.rs`, `config.rs`, `serve.rs`, `headers.rs`, `lang.rs`,
   `routes.rs`. **`config.rs` and `serve.rs` keep their start-up refusals as English literals** (an operator
   reads them, never a visitor): please confirm.
7. **The retired words are read from `docs/glossary.md` §3.1** (one source), adding `rebase` (INC-3) to the
   handoff's five. Only keys beginning `explain.` are exempt. A page-level check that `Key::Explain*` is printed
   only inside `<details>` exists, and is vacuous today (no page has one).
8. **The glossary is checked in both directions**: every catalog message needs a row and every row must match both
   languages; a word marked "in use: yes" in §1 must really appear in both languages.
9. **No shipped message is counted** (the plural machinery is exercised by test-only languages, `qaa` and a
   right-to-left one with different grouping and word order), and **there is no global test-language registry**
   (planeter has one): languages are passed explicitly.
10. **The accessibility statement does not exist yet** (`-/about` arrives with T4 / T9), so "labelled a preview in the
    accessibility statement" is not yet true; the label is in the switcher, and the glossary lists the statement
    among the things to update at sign-off.
11. **Planeter checks not ported**: `no-h1-in-rendered-content` and the rendered-content exemption (no rendered
    content yet; T4 adds them); the gendered-word and blaming deny-lists and the Japanese forbidden-term list beyond
    the loan words (not in the handoff; cheap to add as a second glossary table); the operator preview environment
    variable (different stance); all page-specific tests; the regex allowlist kind.
12. **Schema numbering.** Version 2 holds the role grants and 3 the audit trail (handoff 002), 4 the identity tables;
    the handoff's "schema version 2" for the derived cache is therefore **version 5**.
13. **The page cache key has a fourth part, the presentation** (the link base and the catalog's words): the cached
    HTML embeds hrefs and catalog sentences, so keying on (tip, path, language) alone would serve one surface's
    links to another. A test holds it.
14. **The size cache is keyed by a fingerprint of every open view's state id**, not only the read view's tip: the
    store holds every view, so any view moving changes S. A hint is trusted only while that fingerprint is
    unchanged and for at most 15 minutes.
15. **The checkpoint flag is a time**, `checkpoint_refresh_requested_at`, beside `checkpoint_prikk_version` (the
    handoff names only the version): a flag with no time cannot keep the earliest request or be shown to an operator.
16. **"At most two prikk calls on a miss" holds with the tip held and the index built.** The first view at a new tip
    costs three (`views`, `extract`, `cat`) because IS-10 requires the index to come from one `extract`; a
    transcluded page costs one `cat` more. See the measurements; the handoff's sentence probably wants "three on the
    first miss at a new tip".
17. **A transclusion budget** (32 pages, 4 MiB per page) that the handoff does not state: without it, depth 2 with
    fan-out is a denial-of-service lever (T-9). A refusal is named in the page (`over-budget`).
18. **The sanitizer's policy is the renderer's vocabulary plus the empty subset**, not the empty subset alone (the
    second item at the top). Remote images are dropped (PRV-6); only `http`, `https`, `mailto` and `tel` pass.
19. **A page's alias and description are plain text in `PageView.meta`.** A template must escape them (askama does)
    or use `Sanitizer::text`. T3 tests what the read service hands out; **T4 must test its own template output.**
20. **Earlier crates' public items changed:** `StorageError::Refused` has a `reason` field (two exhaustive patterns in
    prikk's tests were updated); `WikiStore` has two more required methods (every implementor in the workspace
    compiles; one elsewhere would need them); `CreateWikiError::Verification.error` is boxed
    (`clippy::result_large_err`); the M0 `render` tests that asserted a block id as a separate span assert
    `<p id="^p1">` (MKUP-12 is the change).

## Known limitations

- Axum's own **405 and request-extractor rejections still answer with axum's bodies**, not a catalog page in the
  visitor's language (the headers are right; the words are not). Nothing a link leads to reaches them. Worth a
  `method_not_allowed_fallback` in `routes.rs` before T4.
- The English 404 is existence-neutral and has **no link**, because `/` is not served yet; the brand in the header is
  plain text for the same reason. T4 adds `/`, the brand link and the start-page link.
- The 24 px rule is a check over a `TARGETS` list in the stylesheet test; there is no DOM-to-selector cross-check, so
  a new interactive element needs adding there (the heads-up for T4 is in the test's own comments).
- `html5ever` is the only HTML validity check; there is no vnu validator.
- Nothing here is production-reachable as a wiki: `inklkunn serve` serves the health route, the language page, the
  stylesheet and 404s. No wiki page is served until T4.
- **The tip is read, not pinned.** `views()` gives the state id and the page is then read at the view, so a write
  between the two can cache a newer page under the older tip; a later write moves the tip and that row is never read
  again. (Content read is never older than the key says; it can be newer.)
- **The index is built from a whole `extract`**, so attachments count toward its bounds and its time; there is no
  path filter (ledger PK-25). Bundle it into the next letter to prikk if you agree.
- Footnote ids in transcluded pages are not prefixed; a table that carries a block id loses its alignment `style`
  (which the sanitizer and the CSP drop anyway); embeds of images render as missing until attachments are in the
  index (T6).
- **Serving a stale page when prikk is busy (OP-03, CON-5) is not implemented**; it was not in the handoff. Proposal:
  serve a cache hit at the last known tip, flagged stale, and never render on a miss.
- **Wiring left to T4 and T7:** one `SizeHints` handle must go to both `PrikkCli::with_size_hints` and
  `ReadService::new`; the write path must call `forget_wiki` after it seals; T7's refresh runner consumes
  `Checkpoint::needs_refresh` and calls `record_checkpoint_refresh`. The read settings (`tip_ttl` 5 s, the index
  bounds 4 GiB and 200,000 files, `page_max_bytes` 1 MiB) are `ReadConfig` defaults and are **not yet in the
  binary's settings**.
- **Not run or not measured:** the measurements were taken on tmpfs and a 32-core machine, not on production-like
  disks and not under concurrent load; the memory of a 10,000-entry index and the size of the derived store at scale;
  the single-flight and the shared size hints are tested with threads, not with a model checker.
- **Still the handoff-002 limitation that bites T4 first:** the verify-verdict follow-on (IS-7) is not started; T4
  needs it so that the badge never says *Verification failed* for a memory ceiling.

## Open questions for the architect

1. The preview-language decision (top of this request): confirm or reverse.
2. Operator literals in `config.rs` and `serve.rs` stay English: acceptable?
3. Should the gendered-word and blaming deny-lists, and the Japanese forbidden-term list, join the glossary tests
   now (a second table), or with T4's rendered content?
4. Native reviewers (DR-7's names are still outstanding): all 22 Japanese messages in `src/catalog/ja.rs` and the
   glossary's §2 table; the hard terms in §1 (ウィキ, ビュー, 変更/提案 kept apart, レビュー待ちで保持中, 受け入れ済み
   / 見送り済み (not 却下), 未作成のページ, ウィキサービス, 固定リンク, リンク元, ソース, 保存先のパス, and the four
   trust labels reused from planeter's own draft); and the wording of `language.preview_note`, "Cookie" versus
   クッキー, フォーム versus 送信内容 on the refusal page, and `not_found.body`.
5. **PERF-1 at 212 MiB:** accept the median (0.98 s), or take lever 1 or 2 (measurements)? Lever 1 touches the build
   path; I did not do it.
6. **Raw HTML:** change the markup pre-pass to unforgeable markers (so author HTML is escaped when the profile's
   subset is empty) before T4's templates, or accept the vocabulary-limited pass-through (the second item at the
   top)? A third option is to write the weaker guarantee into the manifest's specification of `html_subset`.
7. **Should the handoff's "at most two on a miss" read three for the first miss at a new tip** (deviation 16)?
8. **The next letter to prikk** (bundle it; each costs the owner): an `extract` path filter (PK-25) would take
   attachments out of the index's cost.
9. **Defaults for the settings file:** 4 GiB and 200,000 files for the index, a 5 s tip hold, a 1 MiB page limit.
   The extracted tree is on disk for the duration of a build: is 4 GiB per build acceptable on a hosted
   deployment?
10. **Serve-stale** (OP-03, CON-5): the proposal under the limitations, or leave it to T4.

## QA checklist, T1 – T3 rows

The handoff's checklist, row by row, for what T1 – T3 can answer. **Everything marked "T4 …" or later is not
started.**

| Row | T1 – T3 | Evidence |
|---|---|---|
| URL scheme: aliases, redirects, views, permalinks resolve as D-1; no title can collide with `-/` | **Partly.** No title can collide with `-/` (T1); aliases and redirects resolve in the index and report *redirected from* (T3). Views and permalinks at URL level are T4 | `inklkunn-web` `routes/tests.rs` (the shadowing rule); core `read/index/tests.rs`, `read/tests/cache.rs` |
| A page view: no prikk call on a hit, at most two on a miss (counting test); cache rebuild identical | **Yes, with deviation 16** (three on the first miss at a new tip) | core `read/tests/counts.rs`, `read/tests/cache.rs`, `tests/read_path.rs` (real prikk) |
| The badge is never read from the cache (test); every page shows the honesty words of RFC 002 D-6 | **The type-level half**: the cache record has no field for a verification result. The pages are T4 | `inklkunn-store` `derived.rs` (`DerivedPage`); T4 |
| The sanitizer neutralizes a script, an event handler, `javascript:` and `data:` URLs in a page, a diff and a search snippet; raw source and attachments only on the content origin | **A page: yes** (body, callout title, alias, description; plus the cached row altered in SQLite). A diff and a search snippet are T5 / T6; the content origin exists (T1) and serves nothing yet | core `read/tests/safety.rs`, `read/sanitize/tests.rs`, `tests/sanitizer.rs` |
| Diffs are always per path; the word-level refinement is labelled inklkunn's | not started (T5) | |
| Every catalog and structural test passes on every page state in English and Japanese; Japanese is labelled a preview until DR-7's reviewers sign off | **Yes for the states that exist** (the 404, the language page, the two refusals). The accessibility statement that should carry the label does not exist (deviation 10) | `inklkunn-web` `tests/` |
| Export round trip agrees on text, links, metadata and history; `prikk verify` passes independently | not started (T7) | |
| Route enumeration: every route and endpoint has its `Action` or public reason; a planted bare route fails | **Registration: yes** (the gate and the table, handoff 002). **A request-level harness: no**, it arrives with the first guarded route (T4) | `inklkunn-web` `routes/tests.rs`; `tools/boundary-check` |
| The binary refuses an unconfined sandbox without the explicit flag; the bootstrap feature is not in the binary | **Yes** | `inklkunn` `cli/tests.rs`, `serve/tests.rs`; the test-feature gate |
| Dependency deltas measured per crate group with deny and audit; no licence outside the allow-list | **Yes**, table above | `docs/dependency-policy.md` |
| Both test runs recorded with commands and exit codes; the release rehearsal green | **Both runs: yes** (gates section). **The rehearsal: not run** | |
