# Attachments and the content origin — Developer Handoff v1 (RFC 011; 0.6.0)

| | |
|---|---|
| Document | Attachments served on the content origin through the one reader; embeds that resolve; signed addresses for a restricted wiki; the attachment page; takedown by a per-file hide and by a removal proposal (RFC 011 D-1 … D-7). |
| Status | RFC 011 accepted by the owner 2026-11-28 (Q-1 – Q-4 as recommended, after three checks against the owner's philosophy, the last an independent review). **Assignable after M69** (prikk 0.59.0's re-baseline, maintenance handoff), which comes first. |
| Basis | `rfcs/accepted/011-attachments-and-the-content-origin.md`: D-1 … D-7, acceptance criteria 1 – 10, *How this was checked*, and the review record. Threat model v0.6: C-4c amended, C-6d, C-6e, C-15i, INV-13's surfaces, RR-14 closed. `docs/releases/0.6.0-plan.md` (P-1 – P-5, refinement 5: no migration). |
| Audience | The dev team. The two lenses of handoff 003 apply unchanged, and RFC 011's *How this was checked* is the reviewer's checklist. |
| Handback | One review request per group: **A-T1 – A-T2** (the image shows); **A-T3** (signed addresses); **A-T4 – A-T5** (the attachment page, takedown, the words). Each comes with the usual gates: the whole workspace on prikk 0.55.0 first, shimmed off, on 0.57.0, 0.58.0 and 0.59.0; the floor (0.54.0); mutations named per task; an independent reviewer's read for A-T3 and A-T4 (security). |

**The rules above every task:**
- **Every byte served comes through `file_at`** (RFC 006 J-1). A second way to read a file's bytes, for any surface, is
  a question for the architect before it is code.
- **No store migration.** `SCHEMA_VERSION` stays as it is, and no table, column or checked value is added. A test shows
  that the 0.5.0 binary still opens a store this build has written to, including a per-file hide (A-T4).
- **The application origin never serves a wiki's bytes.**

## A-T1 — The file route, its types and its headers (D-1, D-2)

- **`/{wiki}/-/file/{*path}` on the content origin only,** serving `attachments/{path}` at the default view's tip,
  through `file_at`.
  - **Access:** the page's read check, existence-neutral. An absent wiki, an unreadable one and a missing path give one
    plain not-found, byte for byte.
  - A hidden file answers `410` with *hidden by moderation* (its text in D-7).
  - Not attributable answers `503` with `Retry-After`.
  - The application origin refuses the route as it refuses raw source.
- **Types:**
  - **raster** (PNG, JPEG, GIF, WebP, AVIF): inline only when the leading bytes' signature and the extension agree;
  - **`.svg`:** `image/svg+xml` inline. This is safe because every content-origin answer already carries
    `sandbox` and `nosniff`; keep a test that proves both on this route;
  - **everything else:** `application/octet-stream` with `Content-Disposition: attachment`.
    - The name is sent as `filename*=UTF-8''<percent-encoded>`, with control and bidirectional characters replaced
      (as the typed name rule replaces them, people/typed).
    - An ASCII `filename` fallback is sent beside it, with `"` and `\` replaced.
- **Headers on every answer:**
  - `nosniff`, the content origin's CSP, `Referrer-Policy: no-referrer`;
  - `ETag` (from the file's last writer and its path, never its bytes' hash computed per request) and
    `Last-Modified`, with `304` on a match;
  - `Cache-Control: private, no-cache`.
- **CORP** (`headers.rs`):
  - `same-site` when the content origin and the application share a registrable name;
  - `cross-origin`, for images (raster and SVG) only, when they do not.
  - Decide the registrable name once, at configuration, with the rule written beside it. Say in the request how it is
    decided without a public-suffix list. If it cannot be, stop and ask: **"same host suffix after the first label" is
    not a registrable-name rule.**
- **Bounds and load:**
  - `INKLKUNN_FILE_MAX_BYTES` (default: the signer's attachment limit). Over it, a plain answer says the file is too
    large to serve.
  - **The file route may hold at most a quarter of `INKLKUNN_ADMISSION_BUDGET_MIB`.** Beyond it, the route answers
    *busy* with `Retry-After`, and page reads never do because of it.
- **Tests:**
  - the routes on both origins;
  - each type, including `.png` bytes that are HTML (a download) and an SVG with a script (inline, inert: CSP and
    `sandbox` asserted);
  - the not-found equality across the three cases;
  - `410`, `503`, `304`;
  - a hostile name in `Content-Disposition`;
  - CORP both ways;
  - the budget share: images saturating it while a page read still answers.
- **Mutations:** the signature check skipped (type from the extension alone); `no-cache` dropped; the budget share
  removed; the name written raw.

## A-T2 — Embeds, the CSP, and older versions (D-3, D-1)

- **Resolution.** `![[name.png]]`, and a Markdown image whose `src` is relative under `attachments/`, resolve to
  `LinkHref::Attachment` with the content origin's address.
  - **The decision at render reads no bytes:** presence at the tip, authorship through the writes index (hidden or
    not, attributable or not), and the name.
  - The outcomes:
    - servable: `<img>`;
    - hidden: the tombstone (A-T4 makes the hide possible; build the rendering here);
    - not attributable: *this file is not checked yet*;
    - absent: today's span, with no invitation.
  - A non-image link resolves to the attachment page (A-T4), never to the bytes.
  - **Every image links to its attachment page.**
- **The CSP** becomes `img-src 'self' <content origin>`, and nothing else changes. The test holds the whole string.
- **Older versions** (D-1). A file is shown only when its last writer at the version's block is its last writer at the
  tip. Otherwise:
  - *this file has changed since; see the current one*, linking the attachment page;
  - *this file has since been removed*, with no link;
  - the *changed since* note, too, when the index cannot decide.
- **Derived cache.** The rendered page's cache key already holds the tip. Show that a hide, an unhide and a removal
  each change what an embed renders at the next view, with no stale cache (INV-5).
- **Tests:**
  - **acceptance criterion 1 on a BookStack import** (the 0.5.0 readiness fixture);
  - each outcome above, on the current page and on an older version;
  - the CSP string;
  - the cache after a hide.
- **Mutations:** the sameness check on older versions dropped (the tip's file shown); the decision reading bytes; a
  hidden embed rendered as an image.

Hand back A-T1 – A-T2 as `011-attachments-a-t1-a-t2-review-request.md`.

## A-T3 — Signed addresses (D-4)

- **The key:**
  - **derived, never the secret:** `K = HMAC-SHA256(key = secret, message = "inklkunn content address v1")`;
  - derived once at start, held in `Zeroizing`, never logged, never in `Debug`.
- **One new dependency: RustCrypto's `hmac`,** beside the `sha2` and `subtle` already in the tree.
  - It is measured as the dependency policy says: its ledger row, `deny`, `audit`, and the MSRV.
  - RFC 012's backup manifest uses the same crate with its own label.
  - **No hand-written HMAC.**
- **The message:** each field length-prefixed (u32 big-endian) in this order:
  1. the route (`file` or `raw`);
  2. the wiki's id;
  3. the **decoded** path (the same decoding the route uses, so one file has one signature);
  4. `exp` as decimal seconds.
- **The address:** `?sig=<base64url, no padding>&exp=<seconds>`.
  - `exp` = now + 600, rounded **up** to the next multiple of 300.
  - On the content origin, an `exp` greater than now + 900 is a wrong signature.
- **The checks:**
  - comparison in constant time (`hmac`'s `verify_slice`);
  - missing or wrong: the absent-wiki not-found, byte for byte;
  - **verified but expired:** `403` with *this link has expired; reload the page*;
  - a public wiki ignores both parameters.
- **Where they are put:** only when the visitor may not read the wiki anonymously, on every content-origin address of
  the page (images, raw source).
  - Such a page answers `Cache-Control: no-store`.
  - **Its images carry no `loading="lazy"`.**
- **Logs:** the content origin never records a query string. If anything logs a URI today, it is changed here, before
  RFC 012's D-7.
- **Tests:** acceptance criterion 3 in full, including:
  - a signature for a file used on `raw` for a page of the same name;
  - one for another wiki;
  - an `exp` too far;
  - expired and verifying;
  - a percent-encoding variant of the same path (the same answer as its decoded form);
  - IS-18 closed for raw source on a content origin with its own host.
- **Mutations:** the route dropped from the message; the `exp` bound removed; a plain `==` in place of the
  constant-time check; `no-store` dropped; the expired answer made the not-found.
- **Threat model:** C-6e's test line in §7 names these tests.

Hand back A-T3 as `011-attachments-a-t3-review-request.md`, with an independent reviewer's read.

## A-T4 — The attachment page and takedown (D-5, D-6)

- **The attachment page** `/{wiki}/-/attachment/{*path}`, on the application origin, under the page's read check:
  - the name, size and type;
  - an image's preview from the content origin;
  - **the sidecar's licence and attribution as escaped text**, with addresses as text (C-17d);
  - the last change's link;
  - for a moderator, the two actions;
  - a hidden file shows the tombstone.
- **The per-file hide.**
  - The record is today's per-path hide: kind `page-revision`, `item_id` the block that last wrote the file,
    `item_ref` its repository path (`governance.rs`, `tombstone_events_of_id`).
  - **The sidecar is hidden in the same act** (one `TombstoneActId`).
  - **Before writing code, show in the request that `file_at`'s authorship check honours a per-path hide for a path
    under `attachments/`.** If it reads only change-wide hides, or only page paths, that is a change to `read/`: say
    it, with the smallest form, before building.
  - **The tip rule:** RFC 006 D-8 stays for page paths (refused with its sentence). A per-path hide of an attachment
    path or its sidecar is allowed at the tip.
  - An unhide restores the file, the embed and the page.
  - The moderator's screen says *hide this file*, never the record's kind.
- **The removal proposal.**
  - `PathOutsidePages` admits a removal of `attachments/{path}` together with its `{path}.meta.yaml` when one exists,
    and nothing else under `attachments/`.
  - **Refused:** a sidecar's removal alone; a file's removal without its existing sidecar; any write.
  - It is reviewed and accepted under the rules in force, and protections apply.
- **The review names the embedding pages.** The link graph's backlinks (`read/graph.rs`) are extended to embeds of
  attachments. Say in the request what the extension costs at a wiki of the 0.5.0 readiness size.
- **The export:**
  - a hidden file and its sidecar are left out and listed `suppressed` in the manifest;
  - a removed file is simply not at the tip.
- **Tests:**
  - **acceptance criterion 5 on the import's single change:** one image hidden while every page is current; `410`;
    the tombstone; unhide;
  - the removal proposal accepted, and refused for each case above;
  - a page path's hide at the tip still refused;
  - criterion 6 (the page, the escaped attribution, the backlinks);
  - criterion 8 (the export);
  - **the 0.5.0 binary opening the store after a per-file hide.**
- **Mutations:** the sidecar not hidden with its file; a sidecar removable alone; the tip rule relaxed for page paths;
  the export copying a hidden file.

## A-T5 — Words (D-7)

- Every new message, in English and Japanese, from D-7's list:
  - the attachment page;
  - the download sentence;
  - *hidden by moderation*;
  - the three notes;
  - the expired link;
  - the removal proposal's line;
  - *hide this file* and the removal action.
- They go on the owner's Japanese list as `.git-exclude/ja-review/0.6.0-messages-since-0.5.0.md` (DR-7), appended as
  RFC 012's words come.
- The glossary (M65's file) gains *attachment page* and *signed address* if the words appear in the interface; say
  which in the request.

Hand back A-T4 – A-T5 as `011-attachments-a-t4-a-t5-review-request.md`, with an independent reviewer's read of A-T4.

## Not in this handoff

- Proposing an attachment (upload, P-4); serving a file as it was at an older block; metrics.
- Any change to the per-page hide's meaning, or to RFC 006 D-8 for page paths.

## After the build

- **Done means RFC 011's acceptance criteria 1 – 10,** each named by its test in the last review request.
- Threat model §7 gains the v0.6 test lines for C-4c, C-6d, C-6e and C-15i, verified at 0.6.0's readiness (R-5), as
  §7.6 was for 0.5.0.
- The readiness map's end-to-end proof shows a BookStack import's image served, hidden one file at a time, and brought
  back.
