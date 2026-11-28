# RFC 011 — Attachments and the content origin: imported images that show, and can be taken down

**Status.** **Proposed to the owner 2026-11-28.** This is 0.6.0's R-1, with RFC 012 (`docs/releases/0.6.0-plan.md`,
authorized 2026-11-28 with P-1 – P-5 and six refinements). **Tracks.** ROADMAP T11 (operations before 1.0); requirements
SEC-2, CM-6, RD-10; external design WEB-10; threat model C-4c, C-6c, C-15d, C-15e, RR-14 (IS-18), INV-13; wiki-commons
LIC-4 (attribution), XFER-4 (sidecars). **Touches.**
- `inklkunn-core`: the reader of an attachment's bytes, link resolution of embeds, the tip rule for attachments, a
  proposal that removes an attachment;
- `inklkunn-web`: the content origin's file route, the attachment page, the application's CSP, signed addresses;
- `inklkunn-markup`: nothing new (`LinkHref::Attachment` and its rendering already exist).

**Not here** (the plan, section 2):
- proposing an attachment (upload, P-4);
- serving an attachment as it was at an older block;
- metrics;
- RFC 012's backup, restore and observability.

## Summary

inklkunn has stored attachments since 0.1.0, and 0.5.0 imports them with their licences, but **no release has served
one**. An image in an imported page therefore never shows. Today:
- the content origin's only route is a page's raw source;
- an image embed resolves to a span that is not a link;
- the application's CSP (`img-src 'self'`) would refuse an image from the content origin anyway;
- on a content origin with its own host, a restricted wiki answers *not found* (IS-18), because the sign-in cookie
  does not cross hosts.

This RFC serves an attachment **on the content origin, through the one reader**, so that moderation's hiding holds for
attachments as it does for pages. It makes embeds resolve to that address, opens the application's CSP to the content
origin for images and nothing else, and carries a reader of a restricted wiki across with **short-lived signed
addresses**, which also closes IS-18 for raw source. A small page on the application origin shows an attachment's
licence and attribution.

**Every served attachment can be taken down:**
- **by a proposal that removes it** (removal only; still no upload);
- **by a moderator's hide that reaches an attachment even when it is the tip's.** Today a proposal refuses every path
  outside `pages/`, and the tip rule refuses that hide, so an imported image could be stopped by no one.

## Decisions

- **D-1 — The bytes come through the one reader, at the default view's tip.**
  - `/{wiki}/-/file/{*path}`, **on the content origin only**, serves the file at `attachments/{path}` **as the default
    view's tip holds it**, read through `file_at` (RFC 006 J-1). So a hidden content is never served, and a text that
    cannot be attributed is *not checked yet* (503, `Retry-After`), as for pages.
  - **Only the tip is served.** An older page version that embeds an attachment shows the tip's, and the attachment
    page says so. Serving an attachment as it was at a block is not in this RFC.
  - **Access is the page's:** the same read check, existence-neutral. A wiki that is not there, one the reader may not
    read, and a path with no file all answer the same plain not-found.
  - The application origin refuses the route, as it refuses raw source.
- **D-2 — What a file is served as.**
  - **The media type comes from the bytes, checked against the name.** The leading bytes are read for a raster image's
    signature: PNG, JPEG, GIF, WebP, AVIF.
    - When the signature and the extension agree, it is served **inline** with that type.
    - **Anything else** (a mismatch, SVG, PDF, text, HTML, an unknown type) is served as
      `application/octet-stream` with `Content-Disposition: attachment` and its file name.
  - **SVG is served inline only under the content origin's sandbox** (Q-2), so it is inert (C-4c).
  - **Every answer** carries `nosniff`, the content origin's CSP (`default-src 'none'; sandbox`), `Cross-Origin-Resource-Policy:
    cross-origin` for images only (so the application's pages may embed them), `ETag` and `Last-Modified` (`304` on a
    match), and `Cache-Control: private` within a signed address's life (D-4).
  - **Bounds:** the page cap's sibling for files (`INKLKUNN_FILE_MAX_BYTES`, default the signer's attachment limit).
- **D-3 — Embeds resolve, and the application's CSP opens for images only.**
  - An image embed (`![[name.png]]`) and a Markdown image whose `src` is a relative path under `attachments/` resolve to
    `LinkHref::Attachment` with the content origin's address, when the tip holds that file.
  - A file that is not there stays the span it is today, and no invitation to create it is made.
  - A non-image attachment link (`[[name.pdf]]`) resolves to the attachment page (D-5), never to the bytes, so a
    download is always one deliberate step.
  - **The application's CSP** becomes `img-src 'self' <content origin>`, and nothing else in it changes. A test holds
    the whole string.
- **D-4 — Signed addresses for a restricted wiki** (IS-18; Q-1).
  - When the application renders a page of a wiki that a visitor may not read anonymously, every content-origin address
    on it (images, raw source) carries `?sig=<…>&exp=<…>`.
  - **The signature** is an HMAC-SHA256 over the wiki's id, the exact path and the expiry, under a key **derived from the
    server's secret with its own label** (`inklkunn content address v1`). It is never the secret itself, and never a key
    of a wiki.
  - **The content origin serves a restricted wiki's file or raw source only with a signature that verifies and has not
    expired.** A missing, wrong or expired one answers the same not-found as an absent wiki. A public wiki needs none,
    and ignores one.
  - **The lifetime** is ten minutes (Q-1).
  - **Bound to the path:** a signature for one file opens no other.
  - **Not bound to the person:** the address is a capability that anyone holding it may use until it expires. The page
    says nothing of it, and a reader who shares a page's HTML shares addresses that die within ten minutes.
  - **Stateless:** nothing is stored, so there is no migration, and nothing to revoke beyond the expiry.
- **D-5 — The attachment page** `/{wiki}/-/attachment/{*path}`, on the application origin.
  - It shows the file's name, size and type.
  - For an image, it shows the image itself from the content origin.
  - **It shows the sidecar's licence and attribution as text** (escaped; an address in them is text, never a link,
    C-17d).
  - It links the change that last wrote the file (its history).
  - For a moderator it offers the two ways down (D-6).
  - **A page's embed links to it,** so a reader can always find what an image is under.
- **D-6 — Every served attachment can be taken down.**
  - **(a) A proposal may remove an attachment and its sidecar.**
    - The proposal path rule (`PathOutsidePages`) admits **a removal of `attachments/{path}` and, with it, its
      `{path}.meta.yaml`**, and no other write under `attachments/`. A write there is still refused (P-4).
    - It is an ordinary proposal, reviewed and accepted under the rules in force, and protections apply.
  - **(b) A moderator's hide reaches an attachment at the tip.**
    - RFC 006 D-8's tip rule exists because a page whose current text is hidden is not a page. An attachment is not a
      page: when the only files of a change that a tip still holds are attachments (or their sidecars), **the content
      hide is allowed**.
    - The content origin then answers that file with `410` and a short plain text, *hidden by moderation*. The embed
      becomes a tombstone in the page (its reason and role, as RFC 006's tombstones). The attachment page shows the
      tombstone.
    - A change that also wrote a page the tip still holds is refused as today, with RFC 006's sentence.
  - **The export leaves a hidden attachment out** and marks it `suppressed` in the manifest (C-15e). Today it would copy
    the tip's file byte for byte.
- **D-7 — Words.** The new messages, in both languages:
  - the attachment page;
  - the download sentence;
  - *hidden by moderation*;
  - the removal proposal's line;
  - the moderator's two actions.

  They are listed for the owner.

## What "done" means (acceptance criteria)

1. **A BookStack import's page shows its image.** It is served from the content origin, inline, as PNG or JPEG, and
   the application's CSP allows exactly that.
2. **Types:**
   - a file named `.png` whose bytes are HTML is a download;
   - an SVG renders only under the sandbox, and its script never runs;
   - a PDF and a text file are downloads.
3. **Access:**
   - a public wiki's file reads without a signature;
   - a restricted wiki's file needs a valid signature;
   - no signature, a wrong one, an expired one, one for another path, and one for another wiki each answer the
     absent-wiki not-found, byte for byte;
   - the same holds for a restricted wiki's raw source (IS-18 closed).
4. **The one reader:** a file whose last writer's content is hidden is never served (410 at the tip, after D-6(b)). A
   file the index cannot attribute answers 503.
5. **Takedown:**
   - a proposal that removes an attachment and its sidecar is accepted, and the file is gone from the content origin;
   - a proposal that writes under `attachments/` is still refused;
   - a hide of a change whose only tip-held file is an attachment is allowed, and the embed becomes a tombstone;
   - one that also wrote a tip page is refused.
6. **The attachment page** shows the licence and the attribution as text, an image's preview, and the last change.
   An embed links to it.
7. **The export** leaves a hidden attachment out and says so.
8. **The application's CSP** is the old string with the content origin added to `img-src` and nothing else (a test
   holds it).

## How this was checked against the owner's two lenses

- **Finally clean, safe and secure, robust and sophisticated.**
  - **One reader for every byte served,** so hiding needs no second argument.
  - **Wiki-authored bytes still never come from the origin that holds the cookie.** The CSP widens by one source, for
    images only.
  - **A type is believed only when the bytes and the name agree.**
  - **Nothing is stored for a signed address,** so there is no migration (plan refinement 5), and its key is derived
    for this use alone.
- **APIs and UI that cannot confuse.**
  - **An image always leads to its licence.**
  - **A download is never a surprise:** it is one deliberate step from the attachment page.
  - **A hidden image is a tombstone with its reason,** not a broken image.
  - **The way down is offered where a moderator looks:** the attachment page.

## Alternatives considered

- **Serving attachments on the application origin** (no CSP change, no IS-18). **Rejected:** a wiki-authored file on
  the cookie's origin is what SEC-2 and WEB-10 exist to prevent.
- **Stored, revocable grants instead of signatures.** **Rejected for 0.6.0:** they need a new table, so a migration,
  and the plan has none. Expiry gives what revocation would, within ten minutes.
- **Hiding attachments under the page tip rule unchanged.** **Rejected:** no one could stop an image.

## Threat model v0.6 additions (folded on acceptance)

- **C-4c (rewritten):** a file is served inline only when its bytes and its name agree on a raster type, or as SVG
  under the sandbox. Everything else is a download.
- **C-6d (new):** the application's CSP admits the content origin for images only.
- **C-6e (new):** a restricted wiki's content-origin address is a ten-minute capability, bound to its path and wiki,
  signed under a derived key. Absent, wrong or expired, it is the absent-wiki answer.
- **C-15i (new):** an attachment's content can be hidden at the tip. It is then served as `410` and shown as a
  tombstone, and the export leaves it out.
- **INV-13:** the surfaces gain the file route, the attachment page and the export's attachments.
- **RR-14 (closed):** IS-18.

## Questions for the owner

- **Q-1 — Signed addresses, stateless, ten minutes, bound to path and wiki and not to the person.** **Recommended.** The
  alternative is stored grants: revocable, but they need a migration.
- **Q-2 — SVG inline under the sandbox,** where it is inert. **Recommended.** The alternative is SVG as a download
  always: safest, but an imported diagram would not show.
- **Q-3 — A moderator's hide reaches an attachment at the tip,** and a removal proposal is the other way. **Recommended.**
- **Q-4 — Only the tip's attachment is served.** An older page version shows the current image, and says so.
  **Recommended.**

## Sequencing and handoff

On acceptance, the architect folds the threat model additions (v0.6) and writes
`rfcs/handoffs/011-attachments-and-the-content-origin/attachments-handoff-v1.md`. The order is D-1 – D-3 first (the
image shows), then D-4, then D-5 and D-6, then D-7.
