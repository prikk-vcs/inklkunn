# Review request — handoff 001 (foundations), v2

| | |
|---|---|
| Supersedes | `001-foundations-review-request.md` (v1, 2026-10-27), which stays as the record of what was first handed back and of the open questions |
| Handoff | `foundations-handoff-v1.md` (RFC 001, accepted 2026-10-27), tasks T1 … T7 |
| Implementer | the architect session, at the owner's request |
| Reviewer | the owner read v1 and asked the architect to review it itself against two principles: *finally clean, safe and secure, robust and sophisticated design*; *APIs and UI/UX that cannot confuse or mislead*. The architect ran four fresh reviewing agents, one per area (driver; signer and core; store, gates and workflows; markup), each reading only `main` and the requests, and reviewed the security-critical sources itself. **Every finding below was corrected on `main` before this request**; what was deferred says to which handoff and why. |
| Submitted | 2026-10-27 |
| Disposition | **Approved by the owner, 2026-10-28** (*"Your recommendations are approved."*, on the architect's recommendation of *Approved*). The RFC moved to `rfcs/done/`. |

## What the review found and what changed

The review produced about eighty findings. The ones that mattered, by principle:

### Safety and security (defects a user or an operator would have met)

- **A ceiling hit in production was misreported.** Under bubblewrap a memory-ceiling death reaches the
  server as exit `128 + n` (139), or as bwrap's own exit 1 "Out of memory"; the driver read the first as a
  *damaged repository* and the second as *prikk's refusal*. Both are now `CeilingExceeded`; a confined
  ceiling test runs at five ceilings and accepts only a ceiling hit or a verdict (ledger PK-9).
- **The 1.5 × ceiling headroom was dead code**: the ceiling was clamped to the admitted bytes, so a repository
  slightly off the corpus shape would have hit a spurious ceiling. The gate is now charged the ceiling, and
  `ceiling_for(cost)` has no clamp to lose (a test asserts the headroom for every verb).
- **`materialize` bound an arbitrary parent directory read-write** into prikk's sandbox. The trait now takes
  the scratch directory and a plain target name; the driver binds exactly the scratch, refuses a name that
  is not one plain component, and resolves every mount on the host before binding so a symlink cannot land
  a scratch inside the repository (both tested).
- **Arguments reached prikk's parser unchecked.** A bare `main`, `../x`, a short id or `--help` as a block id
  are refused before anything runs (`StorageError::InvalidArgument`, with the rule and never the value); a
  test with a counting shim proves prikk is not called.
- **A wiki's display name could inject trailers into its signed creation message.** `DisplayName` is a
  validated type (1–120 characters, no control characters, trimmed); the system message is composed in RFC
  002 D-5's shape (summary cut to 120 characters, `Proposed-By`/`Accepted-By`/`Kind: system`; the former
  `Wiki:` trailer, which D-5 forbids, is gone); the manifest quotes every scalar; the help page escapes
  the name; the help page's id is a fresh UUID per wiki, not one hand-typed non-hex constant.
- **A failed creation left a slug taken with no wiki.** *Create wiki* now builds the record, makes the
  repository, seals and verifies, and stores the record **last**; a failure removes the directories it made
  and says so (`Cleanup::Removed` or `LeftBehind { repo, error }`); a test injects a failing seal and
  checks that no record and no directory remain and the slug is free.
- **A `WikiId` or `Slug` could be deserialized unvalidated** (`#[serde(transparent)]`), so `../../etc` could
  have reached `RootLayout::repo_dir`. Deserialization now goes through the validators (tested).
- **SQLite ran its schema DDL before reading the version** and treated garbage as fresh; `create`/`rename`
  were check-then-write without a transaction. The version is read first and a newer or unreadable one is
  refused as `StoreError::SchemaVersion` with nothing touched; every change runs in an immediate
  transaction; a UNIQUE failure is reported as `SlugTaken`/`IdTaken`, never as backend text; WAL mode is
  asserted for file-backed stores.
- **The signer exposed a run-any-verb primitive** (`Writes::verb`) with the key directory mounted, and the
  bootstrap signer was an always-compiled type core named in product code. The runner and the key directory
  are private; only named operations exist; the bootstrap signer is behind the `bootstrap` cargo feature
  (enabled only from dev-dependencies; the boundary gate fails if a product crate enables it); the
  `RepositoryBootstrap` trait moved to the signer crate so core never names the bootstrap type; the key
  directory is created 0700; the post-seal `log` runs read-only without keys; the `public key:` line is
  parsed by its label (a seed is also 64 hex); a non-UTF-8 or relative key path is refused; writes run under
  a 1 GiB ceiling (RFC 002's handoff measures); a stopped `commit` is reported as *state unknown, read before
  retrying*; a failure after a successful seal is `SealedButUnread`, not "needs recovery". The signer gained
  its first tests (refused, incomplete, sealed; no seed in the repository, the ids or any error text).
- **The egress guard** admitted control characters and whitespace in URLs, port 0, a second colon, Teredo,
  ORCHID, the 6to4 relay range and the local-use NAT64 prefix, and could not be tested without a network.
  All refused; name resolution is behind a `Resolver` seam with a double in tests; a mapped IPv4 literal is
  pinned as IPv4; a non-ASCII host is refused with its own reason (A-labels required).
- **The markup crate** could be killed by a deeply nested frontmatter value (stack overflow: now
  `YamlError::TooDeep` at 32 levels) and had six quadratic passes (now linear: a line index, the parser's
  own token ranges for code and links, `O(1)` closer lookups, one-pass attribute parsing, set-based
  duplicate checks, amortized anchor de-duplication). Its code-region model disagreed with CommonMark
  (indented code, multi-line spans, fence lengths), so link HTML could be injected into code; it now takes
  code, HTML and heading ranges from `pulldown-cmark` itself. The renderer re-found tags, directives and
  block ids by text search and could rewrite the wrong span (a Markdown link destroyed, HTML inside a code
  span); it now uses the scanner's byte spans. Scan anchors and rendered ids disagreed; they are one
  reduction now, and the renderer writes the scanner's anchor on every ATX heading (setext headings get a
  unique id too). An `href` with a `javascript:` or `data:` scheme is never emitted as a link, from any
  source. `Rendered.html` is `Rendered.unsanitized_html`.
- **prikk refuses non-ASCII repository paths** (found while probing `title_to_path`: "deferred until
  Unicode NFC normalization is implemented"). `title_to_path` now percent-encodes every non-ASCII or
  forbidden character per component (and a leading or trailing `.`, Windows device names), refuses prikk's
  caps, and `path_to_title` recovers any title exactly (property-tested). Ledger PK-21; the one letter to
  prikk (below).

### API clarity (readings two people could take two ways)

- `Storage` is `Send + Sync`, says that it blocks, takes `&Point` for `history` like its siblings, and
  gives `history` a default limit (100) instead of "everything". `StorageError::Refused.message` is
  documented as operator text; driver-detected refusals carry `driver:`-prefixed kinds, never prikk's
  namespace; an empty refusal says "prikk gave no reason"; `TooLarge` names `needed` and `pool`.
- `Sandbox::Unconfined` is documented as a development policy the binary must refuse without an explicit
  flag (RFC 003's handoff T1); the driver's test knobs are behind the `test-knobs` feature.
- `Resolution::resolved()` excludes an ambiguous link; `Via::Self_` is `Via::SamePage`; the index's
  `namespaces` says it is lint-only; an interwiki template without `{title}` appends it; `by_path` says it
  takes the path with its extension.
- `AcceptanceToken` and the new `ViewCreationAct` have private fields; `PreparedFile.path` says the signer
  validates it; `authorize()`'s docs name the two gaps RFC 002's handoff closes (`Principal::System`
  constructible; visibility caller-asserted).
- `inklkunn --version` names the commit and the prikk floor, as `RELEASING.md` promised.
- Reserved slugs: the reserved answer wins over the character answer, and a slug may not have the shape of
  a wiki id; a slug later added to the reserved list stays readable (`Slug::from_stored`).

### Gates and supply chain (RFC 010; detailed in the 010 request v2)

The gates were bypassable in several spellings and the workflows had drift from their documentation; both
requests' v2 list the changes. The counted shimmed run (`EXPECTED_SKIPS`) and the recorded prikk asset
checksums are the two that change a developer's routine.

## Answers to v1's open questions

1. **Bootstrap signer:** behind the `bootstrap` cargo feature, now, not later; the boundary gate fails if a
   product crate enables it. Today nothing in a product build can create a wiki, which is honest for M0.
2. **`authorize()` public read:** stays default-deny. The read path (RFC 003's handoff T4) lands after RFC
   002's T2 brings the visibility rule and the route-enumeration harness; the 003 handoff says so.
3. **Whole-tree diffs on the web:** forbidden. RFC 003's handoff T5 and T8 always pass `--path`; the API
   requires `path` too in M1.

## Commits (hash → what landed)

v1's table, then the review's corrections as one commit per concern on `main` (hashes in `git log` after
`91865e1`; this request is in the last of them).

## Gates: commands and exit codes

Final commit, 2026-10-27, local toolchain 1.99.1 with 1.88.0 for the MSRV run, prikk 0.54.1, bubblewrap
0.13.1 and prlimit present.

| Command | Exit |
|---|---|
| `cargo fmt --all --check` | 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 |
| `cargo +1.88.0 clippy --workspace --all-targets --locked -- -D warnings` | 0 |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` | 0 |
| `cargo test --workspace --locked` (prikk on `PATH`) | 0 — **138 passed, 0 failed** (35 markup unit, 9 agreement, 3 corpus, 5 path properties, 6 robustness; 16 + 12 driver; 6 + 6 store; 3 signer; 8 + 4 core; 13 gates; 6 + 2 pin; 5 floor) |
| the same with prikk shimmed off `PATH` | 0 — every suite green, **13 `skipping: prikk not on PATH` lines**, the number CI now requires |
| `cargo deny check` | 0 (advisories, bans, licenses, sources ok; `[graph] all-features = true`) |
| `cargo audit` | 0 (80 crates, 1,379 advisories loaded, 0 vulnerabilities) |
| `cargo build -p inklkunn` and `inklkunn --version` | 0; `inklkunn 0.0.0 (commit unknown; requires prikk >= 0.54.0, bubblewrap, prlimit)` |
| `mdbook build docs` | 0 |

## Dependency-tree deltas

None: no new crate. Two self dev-dependencies (`inklkunn-prikk` with `test-knobs`, `inklkunn-signer` with
`bootstrap`) and one intra-workspace dependency (`inklkunn` → `inklkunn-prikk`) are lock-neutral.

## Deviations from the handoff, and why (cumulative)

v1's eight, plus: the review widened the markup crate's own structure (the `structure` module over
`pulldown-cmark`'s ranges) beyond "a scanner restating the reference rules", because two tokenizers
disagreeing is a correctness defect the corpus cannot see; and `title_to_path` percent-encodes instead of
substituting, because prikk's ASCII-only rule left no other reversible mapping.

## Known limitations

- The store size is walked on every driver call; RFC 003's handoff T3 caches it per tip.
- `Sandbox::Unconfined` is reachable until the binary refuses it (RFC 003 T1).
- The egress guard's OS resolver is blocking and untimed; RFC 002's handoff decides the client (IQ-3).
- The write verbs' ceiling is a constant until RFC 002's handoff measures them.
- Paths of non-ASCII titles are percent-encoded; prikk answered on 2026-10-28 (design round in 0.56.0,
  implementation ≥ 0.57.0), so this is M1's rule and native paths are a later migration (PK-21).

## Independence

The author corrected its own work after four independent reads. What a reviewer can check without trusting
the author: every finding named above has a test that fails on the old behaviour (the confined ceiling
shapes, the symlinked scratch, the counting shim, the failing-seal cleanup, the deserialization refusals,
the schema-version refusal, the robustness bounds, the anchor agreement, the blocked schemes, the planted
gate violations in every spelling).

## Open questions for the owner

1. **Tag signing.** `RELEASING.md` said releases are *signed* tags; nothing verified a signature. The claim is
   withdrawn until you name a signing key; then `release.yml` verifies the tag against its fingerprint.
2. **The prikk letter** on non-ASCII paths (`.git-exclude/upstream/prikk/send/draft/`): the one ask worth your
   time, because its answer decides whether M1's first wikis store encoded or native paths.
