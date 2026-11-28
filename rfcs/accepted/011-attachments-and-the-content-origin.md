# RFC 011 — Attachments and the content origin: imported images that show, and can be taken down

**Status.** **Accepted by the owner 2026-11-28** (Q-1 – Q-4 as recommended); handoff `../handoffs/011-attachments-and-the-content-origin/attachments-handoff-v1.md`. Proposed the same day. Checked three times against the owner's philosophy before the rulings:
the architect's own check, a second check against the code, and an independent review at the owner's request. The text
below is the result, and the review record is at the end. This is 0.6.0's R-1, with RFC 012 (`docs/releases/0.6.0-plan.md`,
authorized 2026-11-28 with P-1 – P-5 and six refinements). **Tracks.** ROADMAP T11 (operations before 1.0); requirements
SEC-2, CM-6, RD-10; external design WEB-10; threat model C-4c, C-6c, C-15d, C-15e, RR-14 (IS-18), INV-13; wiki-commons
LIC-4 (attribution), XFER-4 (sidecars). **Touches.**
- `inklkunn-core`: the reader of an attachment's bytes, link resolution of embeds, the tip rule for attachments, a
  proposal that removes an attachment, the per-file hide;
- `inklkunn-web`: the content origin's file route, the attachment page, the application's CSP, signed addresses;
- `inklkunn-markup`: nothing new (`LinkHref::Attachment` and its rendering already exist).

**Not here** (the plan, section 2):
- proposing an attachment (upload, P-4);
- serving an attachment as it was at an older block;
- metrics;
- RFC 012's backup, restore and observability.

**No store migration** (plan refinement 5): nothing below adds a table, a column or a value to a checked set.

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

**Every served attachment can be taken down,** at once by a moderator, or by a proposal:
- **a moderator hides the one file** (and its sidecar). Today a hide covers a whole change, and an import is one change
  holding every page and every file, so no imported image could be hidden while any imported page is current;
- **a proposal removes the file and its sidecar.** Today a proposal refuses every path outside `pages/`.

## Decisions

- **D-1 — The bytes come through the one reader, at the default view's tip.**
  - `/{wiki}/-/file/{*path}`, **on the content origin only**, serves the file at `attachments/{path}` **as the default
    view's tip holds it**, read through `file_at` (RFC 006 J-1). So a hidden file is never served, and a file that cannot
    be attributed is *not checked yet* (`503`, `Retry-After`), as for pages.
  - **Access is the page's:** the same read check, existence-neutral. A wiki that is not there, one the reader may not
    read, and a path with no file all answer the same plain not-found.
  - The application origin refuses the route, as it refuses raw source.
  - **Only the tip is served, and an older page never shows what it did not have.** An older version of a page shows an
    embedded file only when the file is **the same as at that block**: its last writer at that block is its last writer
    at the tip, by the writes index. Otherwise the embed is a note, never the file:
    - *this file has changed since; see the current one*, linking the attachment page, when the tip still holds it;
    - *this file has since been removed*, with no link, when it does not;
    - the *changed since* note, too, when the index cannot decide.

    Serving a file as it was at a block is not in this RFC.
- **D-2 — What a file is served as.**
  - **A raster image is believed only when its bytes and its name agree.** The leading bytes are read for the signature
    of PNG, JPEG, GIF, WebP or AVIF. When the signature and the extension agree, the file is served **inline** with that
    type.
  - **An SVG** (a name ending `.svg`) is served inline as `image/svg+xml`, **only under the content origin's sandbox**
    (Q-2), where it is inert (C-4c). Every answer already carries `sandbox` and `nosniff`, and an `<img>` runs no script.
  - **Anything else** (a mismatch, PDF, text, HTML, an unknown type) is served as `application/octet-stream` with
    `Content-Disposition: attachment`. **The file name is never written raw into the header:** it is sent as RFC 6266's
    `filename*=UTF-8''…`, with control and bidirectional characters replaced, beside an ASCII `filename` fallback.
  - **Every answer** carries:
    - `nosniff` and the content origin's CSP (`default-src 'none'; sandbox`);
    - `Referrer-Policy: no-referrer`;
    - `ETag` and `Last-Modified`, with `304` on a match;
    - **`Cache-Control: private, no-cache`**, for every wiki: a browser keeps the file and asks again each time, so **a
      takedown takes effect at the next view**, and no shared cache keeps a copy.
  - **`Cross-Origin-Resource-Policy`** stays **`same-site`** (today's value, chosen so that a third party's page cannot
    embed a wiki's bytes) when the content origin shares the application's registrable name, as the guide recommends. It
    is `cross-origin` for images (raster and SVG) only when it does not, because the application's pages could not show
    them otherwise.
  - **Bounds and load.**
    - A file over `INKLKUNN_FILE_MAX_BYTES` (default: the signer's attachment limit) is not served, and says so.
    - **The file route may hold at most a quarter of the admission budget,** so images can never make page reads
      answer *busy*.
- **D-3 — Embeds resolve, and the application's CSP opens for images only.**
  - An image embed (`![[name.png]]`) and a Markdown image whose `src` is a relative path under `attachments/` resolve to
    `LinkHref::Attachment` with the content origin's address.
  - **A page never shows a broken image for a reason inklkunn knows.** Each embed is decided when the page is rendered,
    from the file's presence at the tip, its authorship (hidden or not, attributable or not) and its name. The bytes are
    read only when served.
    - A file it would serve is an image.
    - A hidden one is a tombstone (D-6).
    - One it cannot attribute yet is a note: *this file is not checked yet*.
    - A file that is not there stays the span it is today, and no invitation to create it is made.
  - A non-image attachment link (`[[name.pdf]]`) resolves to the attachment page (D-5), never to the bytes, so a
    download is always one deliberate step.
  - **The application's CSP** becomes `img-src 'self' <content origin>`, and nothing else in it changes. A test holds
    the whole string.
- **D-4 — Signed addresses for a restricted wiki** (IS-18; Q-1).
  - When the application renders a page of a wiki that a visitor may not read anonymously, every content-origin address
    on it (images, raw source) carries `?sig=<…>&exp=<…>`.
  - **What is signed:** an HMAC-SHA256 under a key **derived from the server's secret with its own label**
    (`inklkunn content address v1`), never the secret itself and never a key of a wiki. It covers, each length-prefixed:
    - the route (`file` or `raw`), so a signature for a file opens no page's source of the same name;
    - the wiki's id;
    - the decoded path;
    - the expiry.
  - **The lifetime** is ten minutes, with the expiry **rounded up to the next five-minute boundary**. So an address lives
    ten to fifteen minutes, and the same image keeps one address across views within that window, which lets a browser's
    `304` work for a restricted wiki too. **An expiry further away than that is refused** as a wrong signature.
  - **The content origin serves a restricted wiki's file or raw source only with a signature that verifies and has not
    expired.**
    - A missing or wrong signature answers the same not-found as an absent wiki.
    - **An expired signature that verifies** answers *this link has expired; reload the page* (`403`). It reveals
      nothing new, since only someone once given access holds one.
    - A public wiki needs none, and ignores one.
  - **Bound to its route, wiki and path; not bound to the person.** The address is a capability that anyone holding it
    may use until it expires.
  - **Stateless:** nothing is stored, so there is no migration, and nothing to revoke beyond the expiry. **So taking a
    reader's access away reaches the files they already hold addresses for within fifteen minutes**, and C-6e and the
    guide say so.
  - **A signed address is never written down:** the content origin's logs never record a query string (RFC 012 D-7).
  - **A page that carries signed addresses is never cached** (`Cache-Control: no-store`), and **its images are never
    lazy-loaded**. So a page brought back from a browser's cache, or scrolled down after the addresses have expired,
    never meets a dead image.
- **D-5 — The attachment page** `/{wiki}/-/attachment/{*path}`, on the application origin.
  - It shows the file's name, size and type.
  - For an image, it shows the image itself from the content origin.
  - **It shows the sidecar's licence and attribution as text** (escaped; an address in them is text, never a link,
    C-17d).
  - It links the change that last wrote the file (its history).
  - For a moderator, it offers the two ways down (D-6). Both are always available for a file the tip holds.
  - **A page's embed links to it,** so a reader can always find what an image is under.
- **D-6 — Every served attachment can be taken down.**
  - **(a) A moderator hides the one file** (Q-3).
    - **The hide is per file.** Today's hide records are already kept per path within a change (a `page-revision` record
      whose reference is the repository path, `governance.rs`), so this needs no migration. The file's sidecar is hidden
      in the same act.
    - **RFC 006 D-8's tip rule is unchanged for pages** (a page whose current text is hidden is not a page) and **does not
      apply to an attachment's path:** an attachment is not a page.
    - **Then:**
      - the content origin answers that file with `410` and a short plain text, *hidden by moderation*;
      - the embed becomes a tombstone in the page, with its reason and role, as RFC 006's tombstones;
      - the attachment page shows the tombstone;
      - an unhide restores all three.
    - The moderator's screen calls it *hide this file*. The record's kind is a storage token, never shown.
  - **(b) A proposal removes a file and its sidecar.**
    - The proposal path rule (`PathOutsidePages`) admits **a removal of `attachments/{path}` together with its
      `{path}.meta.yaml` when there is one**, and nothing else under `attachments/`.
      - **A sidecar is never removed alone,** so a file still served never loses its attribution (LIC-4).
      - A write there is still refused (P-4).
    - It is an ordinary proposal, reviewed and accepted under the rules in force, and protections apply.
    - **Its review names the pages at the tip that embed the file** (the link graph's backlinks, extended to embeds), so
      a reviewer sees what the removal leaves without its image.
  - **The export leaves a hidden file and its sidecar out,** and marks them `suppressed` in the manifest (C-15e). Today
    it would copy the tip's file byte for byte.
- **D-7 — Words.** The new messages, in both languages:
  - the attachment page;
  - the download sentence;
  - *hidden by moderation*;
  - the three notes of D-1 and D-3;
  - *this link has expired; reload the page*;
  - the removal proposal's line;
  - the moderator's two actions.

  They are listed for the owner.

## What "done" means (acceptance criteria)

1. **A BookStack import's page shows its image.** It is served from the content origin, inline, as PNG or JPEG, and
   the application's CSP allows exactly that.
2. **Types:**
   - a file named `.png` whose bytes are HTML is a download;
   - an SVG renders only under the sandbox, and its script never runs;
   - a PDF and a text file are downloads;
   - a download's name with a control or bidirectional character, or a quote, reaches the header encoded.
3. **Access:**
   - a public wiki's file reads without a signature;
   - a restricted wiki's file needs a valid signature;
   - no signature, a wrong one, one for another path, one for another wiki, **one for the raw source of a page of the
     same name**, and one whose expiry is too far away each answer the absent-wiki not-found, byte for byte;
   - an expired one that verifies says it has expired;
   - the same holds for a restricted wiki's raw source (IS-18 closed).
4. **The one reader:** a hidden file is never served (`410`). A file the index cannot attribute answers `503`, and its
   embed is the *not checked yet* note.
5. **Takedown, on the import of criterion 1** (one change holding every page and file):
   - **a moderator hides one imported image while every imported page is current;** the file answers `410`, the embed
     is a tombstone, and an unhide restores it;
   - a proposal that removes a file and its sidecar is accepted, and the file is gone from the content origin;
   - a proposal that removes only a sidecar, or writes anything under `attachments/`, is refused;
   - a hide of a page's path at the tip is still refused, with RFC 006's sentence.
6. **The attachment page** shows the licence and the attribution as text, an image's preview, and the last change. An
   embed links to it. A removal proposal's review names the embedding pages.
7. **An older version of a page** shows an unchanged file, and the right note for a changed or a removed one.
8. **The export** leaves a hidden file and its sidecar out, and says so.
9. **The application's CSP** is the old string with the content origin added to `img-src` and nothing else, and CORP is
   `same-site` for a same-site content origin (a test holds both).
10. **Load:** a page with many images cannot make page reads *busy* (the file route's share of the budget).

## How this was checked against the owner's two lenses

- **Finally clean, safe and secure, robust and sophisticated.**
  - **One reader for every byte served,** so hiding needs no second argument.
  - **Wiki-authored bytes still never come from the origin that holds the cookie.** The CSP widens by one source, for
    images only, and CORP stays `same-site` where it can.
  - **A raster type is believed only when the bytes and the name agree;** SVG only under the sandbox.
  - **A signature opens one route, one wiki and one path,** under a key derived for this use alone, and **nothing is
    stored**, so there is no migration.
  - **A takedown is immediate and complete:** one file, at the next view, and out of the export.
- **APIs and UI that cannot confuse.**
  - **An image always leads to its licence,** and its attribution can never be removed while it is served.
  - **A download is never a surprise:** it is one deliberate step from the attachment page.
  - **No image is ever broken for a reason inklkunn knows:** hidden, not checked yet, changed, removed and expired each
    say what they are.
  - **The way down is offered where a moderator looks,** and both ways always work.

## Alternatives considered

- **Serving attachments on the application origin** (no CSP change, no IS-18). **Rejected:** a wiki-authored file on
  the cookie's origin is what SEC-2 and WEB-10 exist to prevent.
- **Stored, revocable grants instead of signatures.** **Rejected for 0.6.0:** they need a new table, so a migration,
  and the plan has none. Expiry gives what revocation would, within fifteen minutes.
- **The change-wide hide, with the tip rule relaxed for changes that hold only attachments.** **Rejected:** an import is
  one change holding pages too, so it could never hide an imported image.
- **Detecting SVG from its first element.** **Rejected:** the sandbox and `nosniff` already make every `.svg` inert, and
  a parser of XML's prologue adds nothing but code.

## Threat model v0.6 additions (folded on acceptance)

- **C-4c (rewritten):** a file is served inline only when its bytes and its name agree on a raster type, or as SVG
  under the sandbox. Everything else is a download, and its name reaches the header encoded.
- **C-6d (new):** the application's CSP admits the content origin for images only. CORP stays `same-site` for a
  same-site content origin.
- **C-6e (new):** a restricted wiki's content-origin address is a capability of ten to fifteen minutes, bound to its
  route, wiki and path, signed under a derived key. Absent or wrong, it is the absent-wiki answer; expired, it says so.
  It is never logged, and a removed reader's addresses die within fifteen minutes.
- **C-15i (new):** one attachment (and its sidecar) can be hidden at the tip. It is then served as `410` and shown as a
  tombstone, and the export leaves it out. A sidecar is never removed alone.
- **INV-13:** the surfaces gain the file route, the attachment page and the export's attachments.
- **RR-14 (closed):** IS-18.

## Questions for the owner

- **Q-1 — Signed addresses: stateless, ten to fifteen minutes, bound to route, wiki and path, and not to the person.**
  **Recommended.** The alternative is stored grants: revocable, but they need a migration.
- **Q-2 — SVG inline under the sandbox,** where it is inert. **Recommended.** The alternative is SVG as a download
  always: safest, but an imported diagram would not show.
- **Q-3 — A moderator hides one file at the tip, and a removal proposal is the other way.** **Recommended.**
- **Q-4 — Only the tip's file is served.** An older page version shows a file only when it is unchanged since that
  version, and otherwise says that it has changed or been removed (D-1). **Recommended.**

## Sequencing and handoff

On acceptance, the architect folds the threat model additions (v0.6) and writes
`rfcs/handoffs/011-attachments-and-the-content-origin/attachments-handoff-v1.md`. The order is D-1 – D-3 first (the
image shows), then D-4, then D-5 and D-6, then D-7.

## Review record (2026-11-28, before the rulings)

1. **The architect's own check** (two refinements):
   - an older page never shows a file it did not have;
   - a page with signed addresses is never cached.
2. **A second check against the code, at the owner's request** (nine more), among them:
   - the notes in place of broken images;
   - the encoded download name;
   - `no-cache`, so that a takedown is immediate;
   - signed addresses never logged and never lazy-loaded;
   - a removal's review naming the embedding pages;
   - the sidecar leaving the export with its file.
3. **An independent review, at the owner's second request** (adopted after the architect verified each claim in the
   code):
   - **the per-file hide** (the change-wide hide could never reach an import's image: `import.rs` makes one change);
   - the signature covering the route, so a file's and a page source's addresses cannot open each other;
   - the expiry's bound and five-minute rounding, so a restricted wiki's images can be revalidated;
   - the expired link that says so;
   - CORP kept `same-site` (`headers.rs`'s deliberate choice);
   - a sidecar never removed alone;
   - the *removed since* note;
   - embeds decided without reading bytes, and the file route's share of the budget.

   **Cut:** the SVG first-element detection.
