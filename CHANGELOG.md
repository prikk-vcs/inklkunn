# Changelog

Releases are bare version tags; each section is the release's notes (assembled into the GitHub release by
`release.yml`). Kept by hand, per release.

## [Unreleased]

M0 — foundations (RFC 001, RFC 010). Nothing serves pages yet.

- **The workspace and its gates.** Eight crates in three layers (`inklkunn-prikk`, `inklkunn-signer`,
  `inklkunn-store`; `inklkunn-markup`, `inklkunn-core`, `inklkunn-auth`; `inklkunn-web`, `inklkunn`) and four
  gates as tests: layering, key reachability (only `inklkunn-core` may depend on the signer; its key store
  stays private), prikk spawn (only the driver and the signer spawn prikk; the driver names no write verb),
  markup purity. Pinned build chain: one compiler for CI and release, MSRV 1.88.0 built separately, actions
  by commit SHA, images by digest, the prikk floor 0.54.0 in one place.
- **`inklkunn-markup`.** Frontmatter in the plain YAML subset (anchors and tags refused, YAML's date and
  boolean coercions kept visible), the Portable Wiki Markdown Layer 2 scanner, MKUP-5 link resolution, HTML
  rendering with the MKUP-12 conventions, advisory lint with guideline ids, and `title_to_path` under a
  property test. wiki-commons' conformance corpus (24 scan cases, 10 resolution cases) passes.
- **`inklkunn-prikk`.** The `Storage` trait and `PrikkCli`: eight read verbs with pinned `schema_version`s
  and typed outcomes, bubblewrap with the repository read-only, a server-kept deadline, a `prlimit` ceiling
  from a per-verb cost table, and a light/heavy admission gate.
- **`inklkunn-store`.** The wiki record, validated slugs, opaque time-ordered ids, the `<root>/<shard>/<id>/`
  layout, in-memory and SQLite stores under one invariant suite.
- **`inklkunn-core` and `inklkunn-signer`.** The `authorize()` seam (default deny), the `Accept` trait, the
  `Assurance` values, the egress guard, the bundle layout, and *create wiki* end to end through a bootstrap
  signer (plain-text key directory; RFC 002 replaces it).
- **The two spikes** (RFC 001 D-10), measured on prikk 0.54.1 and reported under `rfcs/handoffs/001-foundations/`:
  the per-verb cost table is inklkunn's own (`diff` 4.6 × the store size is the expensive read; `tree` and `cat`
  1.0 ×; `verify` 2.0 ×; `branch` and `log` flat), and the content-merge spike shows that prikk's merge is
  confluent for 53 of 120 concurrent-edit pairs while a block-aware three-way merge is clean for 99 — the
  justification for inklkunn's own merge in RFC 004. `tools/corpus` generates both corpora from a seed;
  `tools/spikes/` holds the harnesses. prikk refuses repository paths over 1024 bytes, which bounds a wiki's
  hierarchy depth (about 500 one-character levels).
- **The M0 review's corrections** (2026-10-27; review requests v2). The driver reads a ceiling hit through
  bubblewrap (exit 128 + n, or bwrap's "Out of memory") as a ceiling, charges the admission gate the real 1.5 ×
  ceiling, checks every argument's shape before prikk runs, binds exactly the caller's scratch directory for
  `materialize` and resolves every mount against symlinks, bounds a call's output, and refuses bubblewrap's own
  failures as prerequisites rather than prikk's refusals. The store validates ids, slugs and display names on
  every path including deserialization, refuses a newer schema before touching the file, and runs each change
  in a transaction. *Create wiki* stores its record last and removes a failed attempt's directories. The signer
  exposes only named operations; the bootstrap signer is behind the `bootstrap` feature. The egress guard
  refuses control characters, port 0, Teredo, ORCHID and more, and resolves through a seam. The markup crate
  bounds frontmatter nesting, takes code and heading ranges from CommonMark's parser, replaces constructs at
  their scanned spans, writes the scanner's anchors as ids, never emits a `javascript:` or `data:` href, and
  is linear on every pathological input in a bounded test. `title_to_path` percent-encodes non-ASCII titles
  (prikk refuses non-ASCII paths) exactly reversibly. The gates read manifests in every spelling, scan tests
  and nested files, require the lints and `forbid(unsafe_code)` in every crate, and keep test features out of
  product crates; `pin-check` understands job-level guards and pins `cargo install`; `floor-check` records
  the prikk assets' checksums; CI counts the shimmed self-skips; the release gates run both test passes, join
  the images under one tag and attest them.
- **Conventions and ledgers** (RFC 001 D-11; RFC 010): README, this changelog, `SECURITY.md` stating RR-1, the
  dependency policy with a generated lock inventory that a test keeps equal to `Cargo.lock`, the upstream
  dependency ledger (PK/PL/WC rows), `docs/UPSTREAM.md`, `docs/RELEASING.md`, the mdBook.

RFC 002 and RFC 003 — the first handoffs after M0, in progress. Still nothing serves pages.

- **The patch message** (RFC 002 D-5; handoff 002 T5). One grammar, in `inklkunn-signer::message` and
  re-exported as `inklkunn-core::message`, writes and reads what every signed change carries: a one-line
  summary of at most 120 characters, then `Proposal`, `Proposed-By`, `Accepted-By`, `Base`, `Kind` and
  `Imported-From` once each in that order, for the four kinds (proposal, rollback, import, system). The parser
  is strict and total: only canonical text parses, and an error never repeats the text it refused. A real
  prikk returns a sealed change's message byte for byte, which a test checks for every kind. The text M0
  wrote for a created wiki still parses.
  *Output change:* `DisplayName` now refuses U+2028 and U+2029 as well as controls, as its error text always
  said: a reader that splits lines on them would otherwise see a forged trailer in a signed message.
- **`authorize()` and the roles** (RFC 002 D-2; handoff 002 T2). The decision is a pure rule (`decide`) over a
  snapshot that an `Authorizer` loads itself, so a handler can no longer say that a wiki is public: a
  resource names its wiki and nothing about it. The ladder is observer, questioner, contributor, reviewer,
  maintainer, admin; a token's scope (`read ⊂ propose ⊂ review ⊂ accept ⊂ admin`) is intersected with the
  account's role. A principal with no grant may read what the wiki's visibility lets it read, and nothing else;
  a wiki that is not there and a restricted one it has no role in give the same answer, the same audit event
  and the same number of lookups. The decision fails closed, records consequential decisions (accept,
  moderate, admin, export, import) allowed or not, and refuses an allowed one it could not record. The
  system principal cannot be built outside `inklkunn-core`. `inklkunn-store` gains the role grants and the
  audit sink trait, and its SQLite file now migrates forward from M0's schema (schema version 2).
  Two gates join the others: routes are registered in the web crate's `routes.rs` only (so the route table is
  complete), and no web source names a system principal or builds an account one.
  *Output change:* `inklkunn_core::authorize` (the free function that denied everything) is replaced by
  `Authorizer::authorize`; `Resource::Wiki` no longer carries a visibility; `Principal::Account` carries the
  credential's scope.
- **The key store** (RFC 002 D-3; handoff 002 T3). Each wiki's author and maintainer seeds are sealed in one
  record, `<wiki id>.key.json`, under a master key that Argon2id derives once at start from the operator's
  secret (a file; 16 to 1024 bytes without its trailing line break) and a store header that detects a wrong
  secret when the store is opened. Records use XChaCha20-Poly1305 with a fresh 192-bit nonce each, and bind
  the wiki, key ids, generation and times as associated data, so none can be edited or moved to another wiki.
  A seed is decrypted only into a new 0700 directory on a verified tmpfs with 0600 files, for one write call,
  overwritten and removed afterwards (and on a panic); a removal that fails is reported, never swallowed.
  Randomness comes from `/dev/urandom` with no fallback. Nothing in the module is public, and a new gate fails
  if any public item of the signer crate mentions a seed, a secret or a key directory. The module is built
  and tested here and is wired to `Accept` by the next task. Dependencies: `argon2`, `chacha20poly1305`,
  `zeroize` (+22 lock entries, measured; see the dependency policy).
- **`Accept` and `create_view`** (RFC 002 D-4; handoff 002 T4). `Accept` is now defined in `inklkunn-signer`, the
  crate that implements and enforces it, and re-exported by `inklkunn-core`; `Signer` implements it. Before a byte
  is written the signer reads the acceptance record the token names (through an `AcceptanceLedger`) and
  refuses unless it is pending seal for this wiki and view and the message says exactly what the record says
  (proposal, proposer, acceptor, base, kind). Then: nothing unsealed and a clean worktree, the view's worktree,
  the renames, writes and removals (paths validated whole beforehand: ASCII, prikk's rules, the bundle's
  roots), `commit`, `seal`, read the block back and check it is this change, `verify`. A failure before
  `commit` puts the worktree back and says so; a failure after is `Incomplete` for recovery. A view is created
  only for a recorded act. The first change of a new wiki goes through `RepositoryBootstrap`, which now
  refuses anything but a `system` message to a repository with no history. A start-up sweep removes the
  seed directories a killed server left behind.
  *Output change:* `inklkunn_core::accept` re-exports the signer's types; `PreparedChange.message` is the text
  of a `Message`; `SignerConfig` is built with `SignerConfig::new`.
- **The audit trail and the reconciliation** (RFC 002 D-7, D-9; handoff 002 T6). The audit log is a SQLite
  table that the database itself refuses to update or delete from (two triggers abort with *the audit log is
  append-only*, so a stray `UPDATE` or `DELETE` fails even from a hand-opened connection); the store's schema
  is now version 3 and migrates forward from M0's schema. Every sink cleans each field again on the way in
  (control characters and U+2028/U+2029 become `?`, at most 200 characters), so an event built by hand cannot
  forge a second line. `AuditLog` reads the trail in pages after a position, and `export_jsonl` writes it, oldest
  first, one object per line with fixed keys. The reconciliation (`inklkunn_core::reconcile`) reads every
  view's whole history, a page of 500 blocks at a time, and compares it with the acceptance records and the
  system-act records: a block whose message cannot be read, a seal no record explains, a system change no act
  explains, a record whose block is not in the history, and a record that disagrees with the message it names
  (kind, proposal, proposer, acceptor, base, source) or puts the change in a view that does not hold it. It
  only reads: it repairs nothing, and `record_findings` writes ids and
  a fixed word to the audit trail (outcome *reported*), never a message, a name or a path. Tested against a
  fake storage for every finding and against a real prikk history, where it also leaves the history and the
  current view as they were.
- **Principals and credentials** (RFC 002 D-1; handoff 002 T1; the owner's ruling DR-9). `inklkunn-auth` turns a
  presented credential into a principal. *Local passwords* are Argon2id (19 MiB, two passes) in the PHC format,
  NFKC-normalised, with a cost ceiling on what a stored hash may ask for; a new password has ten characters
  and no composition rule. *Sign-in is constant-work*: past the throttles every attempt does exactly one
  verification (against a dummy hash made at start, and of the same cost, when there is nothing usable to
  check, a stored string nobody can verify included), after taking a permit from a bounded gate, and unknown
  name, wrong password, disabled account and correct-password-for-a-disabled-account are one answer. *Throttles
  are counted before they are judged*: an attempt is checked against the lock and counted in one step, so a
  burst of simultaneous guesses is held to the limit exactly as a sequence is (sixty at once at one name: five
  judged, fifty-five turned away); an attempt that was right, or that the service was too busy to judge, gives
  its count back. They count failures per sign-in name as typed (so a made-up name locks exactly like a real
  one) and per client address (an IPv4 address or an IPv6 /64), five and thirty failures lock for fifteen
  minutes, a correct password is refused while locked, a lock is written to the audit trail (no name, no
  address), and the address comes from `X-Forwarded-For` only when the peer is a declared trusted proxy (a
  trusted proxy whose header cannot be read says so). *Sessions* are opaque random ids kept only as a hash, in
  `__Host-` cookies that are `Secure`, `HttpOnly` and `SameSite=Strict`, each with its own CSRF token (a
  double-submit cookie for the sign-in form), a new one at every sign-in; a request that changes anything goes
  through `principal_for_write`, which cannot be answered without the token. *Changing a password* proves the
  old one (throttled like a sign-in) and, like an administrator's reset or a disabling, ends every session
  **and revokes every API token**; enabling an account again does not bring them back. *API tokens* are `ink_`
  plus 256 random bits, kept as a hash, with a scope, a lifetime of at most a year and a day, at most a hundred
  live per account, and a revocation; a token's principal carries its scope, which `authorize()` intersects with
  the account's role (a test runs every scope, role, visibility and action). *OpenID Connect* is the
  authorization code flow with PKCE, owned by the authenticator: starting one spends the client address's
  budget; the state is one-shot, server-side and must also be in the browser's state cookie, which the
  authenticator reads from the `Cookie` header itself (login CSRF); ID tokens are verified by `kid` against
  the provider's keys with `jsonwebtoken` on the `aws_lc_rs` backend (RS256, RS384, RS512, ES256, ES384 and
  EdDSA only; the key must be published for signatures; issuer, audience, `azp`, nonce and times checked,
  `iat` required and at most fifteen minutes old; every refusal carries the reason for the log); the provider's
  keys are refetched for an unknown `kid` at most once a minute, and a provider that is down is not asked again
  for five seconds. An identity nobody linked signs nobody in, and an e-mail address is only a hint. The
  network is reached through one seam, `HttpsFetcher`, and the token verifier is a seam with a test double.
  The store gains accounts, linked identities, sessions and tokens (schema version 4, with `CHECK` and
  foreign-key rules in the database, and a password hash that is not an Argon2id string refused). Dependencies:
  `jsonwebtoken` (`aws_lc_rs`) and `sha2` (+27 lock entries, measured, `rsa` and `crypto-bigint` absent, no
  advisory ignored; see the dependency policy). *Changed after an independent reading of this task:* the
  throttle ordering above, the cost of an unverifiable stored hash, the credential revocations, the typed
  state-cookie path, the audit of locks, the reasons, the redacted `Debug` output of the OpenID Connect types,
  the token cap, the configuration checks, and `Sessions`, `Tokens`, the throttles and the gate becoming
  private to the crate.
- **Key lifecycle** (RFC 002 D-8; handoff 002 T7). `inklkunn wiki keys status|create|rotate <wiki>` shows a
  wiki's key ids, generation and dates (never a secret), makes keys for an initialised repository that has
  none, and rotates them: new author and maintainer keys under **new ids** (prikk binds an id to one key for
  good), the new maintainer key adopted first, the sealed record replaced last, so a failure leaves the wiki as
  it was. Rotation needs the wiki's slug typed (`--confirm`) and is audited. **The old maintainer key stays
  trusted by default** and its history keeps verifying; `--retire-old --accept-unverifiable-history` removes it
  from the repository's trust, after which it can seal nothing and **prikk 0.54.1 cannot verify what it sealed**
  (ledger PK-34, a test that fails the day prikk changes this; RFC 002 D-8's "the repository still verifies"
  cannot hold with retirement until then). The binary gains its configuration (`INKLKUNN_DATA_DIR`,
  `INKLKUNN_SECRET_FILE`, `INKLKUNN_KEY_DIR`, `INKLKUNN_SEED_DIR`, `INKLKUNN_PRIKK`, and the `--dev-unconfined`
  flag, which prints a banner and is not settable from the environment). `docs/KEYS.md` is the runbook,
  including the compromise procedure and what a lost key store means.

- **Changes after an independent reading of T2 – T7** (the second of the two readings of handoff 002).
  *The reconciliation read only the newest page of a long history*: it paged by block id, which real prikk
  answers with an empty list and exit 0 (ledger PK-36), so past 500 blocks it saw a prefix and called the rest
  missing or clean. It now reads each view whole (one `log --limit`, repeated four times larger while it comes
  back full, an error at a stated bound), the driver refuses a block point for `history`, and a standing finding
  is reported once, not at every run. *Acceptance outcomes say what state the repository is in*:
  `Queued { step }` (one patch queued, nothing sealed), `SealedUnconfirmed { sealed, why }` (sealed; do not
  retry), `WorktreeDirty` (a failure that could not be undone), beside `Refused`, `Unavailable` and
  `SeedsNotRemoved`; after a retirement every acceptance is `SealedUnconfirmed { VerifyFailed }`, which a test
  pins. *Two acceptances on one wiki in one process no longer interleave* (a lock per wiki; they once sealed one
  change's message over the other's file). A rotation whose retirement fails is a `Rotation` that says the old
  key is still trusted, the command line audits an act before and after it and refuses to begin one it cannot
  record, and the wiki record follows the key store. A failed wiki creation now removes its sealed key record.
  The audit table also refuses `INSERT OR REPLACE` over an existing position. Seed paths are scrubbed from
  prikk's words before they become errors; the seed directory must be owned by the service's user and not
  writable by others, and a live call touches it so a long acceptance is not taken for a dead one. The gates
  catch `Principal as P`, glob and group imports, function-call spellings of route registration and
  `routes.rs` at any depth, and a surface naming the signer; their matcher is a lexer now, so `impl X` or
  `dyn X` cannot hide `X`. `Decision` is `#[must_use]`. `inklkunn audit export` writes the trail as JSON
  Lines. The write ceiling of the signer's prikk calls is **still the fixed 1 GiB** the handoff asked to
  measure and replace; that measurement was not done (see the review request).

Corrections to handoff 002 (the architect's review of 2026-10-29, `identity-keys-and-signing-corrections-handoff-v1`).

- **A key store is never made by accident, and its creation is one critical section** (C4, IS-17).
  `Signer::open` refuses a key directory with no store and names the path; `Signer::open_creating_store` (what
  `inklkunn wiki keys create` uses) makes one. Creation takes an exclusive `flock` across the emptiness check and
  the header's link, so the loser of a race opens the winner's store instead of being told its directory "is not
  a key store", and a crash's leftover temporary file no longer blocks creation for good. `rustix` (`fs`) joins
  the signer for `flock(2)` (no new lock entry); `std::fs::File::lock` needs Rust 1.89 and the minimum is 1.88.
- **A memory ceiling is never a refusal or an unconfirmed seal** (C1, threat model C-9a). Every call the signer
  makes runs under a ceiling from a costed rule per verb, sized from the store and the bytes of the change
  (measured on prikk 0.55.0, ledger PK-38), replacing a fixed 1 GiB that `verify` of one 300 MiB attachment
  exceeded; a 300 MiB attachment is now accepted, and so is the change after it. A death by the ceiling uses the
  driver's own classifier and is `CeilingExceeded` (nothing committed, worktree as it was), `Queued { ceiling }`
  (a patch is queued) or `SealedUnconfirmed { CeilingExceeded }` (sealed, not confirmed; never "verify failed"); a
  `verify` stage that failed with the whole message `i/o error: out of memory` is the ceiling too.
- **Each write verb sees only the seed it signs with** (C2). `commit` the author seed; `seal` and `branch create`
  the maintainer seed; `init`, `trust`, `mv`, `branch switch`, `verify` and `log` none (a stand-in lists what each
  can see).
- **One writer per wiki across processes** (C3). Every act that writes a wiki's repository or key record takes an
  exclusive `flock` on `<root>/<shard>/<id>.lock` (never inside `.prikk/`); another process, such as the command
  line beside a running server, is refused at once with `Busy`, naming the process that holds it. The kernel drops
  the lock with a killed holder.
- **The signer checks the files, not only the message** (C5). A SHA-256 over the prepared change (files sorted,
  removals, renames in order) joins `ExpectedMessage`; a change altered after the record is `ChangeMismatch`
  before one prikk call, seed or file. `sha2` joins the signer (no new lock entry).
- **Seed directories are swept by liveness, not age** (C6). A seed directory is born locked and held for its
  call; a start removes every directory it can lock, at once, and none a live call holds; removal is by handle
  (`O_NOFOLLOW`), never following a link. The heartbeat is gone.
- **Principals and key operations need a witness** (C7). An account principal is built only with the process's one
  `IdentityAuthority` (held by the authenticator) and has private fields; `create_keys`, `rotate_keys` and
  `create_repository` (so `create_wiki`) take the process's one `KeyAuthority`. Compile-fail doctests show the
  refusals. *API changes:* `Principal::Account` holds an `AccountPrincipal`; `Authenticator::new` takes the
  authority; `create_wiki` takes the key authority; `RepositoryBootstrap::create_repository` takes it.
- **Outbound HTTPS is a confined `curl`** (C8, IQ-3). New lower-layer crate `inklkunn-fetch` runs `curl` in its
  own bubblewrap vector (network shared; no repository, key or seed; `/usr` and the CA bundle read-only; fresh
  `/tmp`; cleared environment) at exactly the addresses the egress guard pinned: https only, no redirects,
  bounded time and bytes, no proxy, no config file. The `Authorization` header and the form body travel on
  curl's standard input and appear in no argument, error, `Debug` output or log. No TLS crate enters the lock
  (209 → 210: the new member). The boundary gate makes it the third crate allowed to spawn (`bwrap` or `curl`
  only), depended on by the binary alone, and the only one that may name `curl`. The binary carries the
  `HttpsFetcher` adapter (guard + fetcher), unused until OpenID Connect is switched on in M2.
- **The OIDC round trip survives the browser** (C9). The callback is answered with a `Landing`, a 200 page that
  moves the browser on with a same-site request (a `SameSite=Strict` session cookie is withheld after a redirect
  from a cross-site-initiated navigation); the login-state cookie is `Lax`, `__Host-`, `HttpOnly`, `Secure`, ten
  minutes, and removed by the callback. Checked once in headless Chromium 156. *API change:*
  `Authenticator::finish_oidc` takes the path to land on and returns `OidcSignedIn { signed_in, landing }`.
- **Smaller items from the readings** (C10). Store schema **6**: `role_grants` has foreign keys to `accounts`
  (the migration rebuilds the table and refuses by name, rolling back, when a grant names an account that does
  not exist); `inklkunn serve` warns on a non-loopback bind with no declared trusted proxy (no setting exists
  yet); an account holds at most 100 live sessions (a sign-in beyond that ends the oldest, never refused).
- **A rotation records where the old key stopped, and a seal after that point is reported** (C11, DR-10). The
  key record (now format 2; format 1 still opens) keeps, for every maintainer key a rotation replaced, the
  newest block of every view at that moment and whether the key was retired. `inklkunn wiki keys status` (and
  every rotation, into the audit trail) reports every block such a key sealed that is not in the history up to
  those points: a leaked seed used against the repository while the service's records are intact. It changes
  nothing, `status` exits non-zero on a finding, and it cannot run on a repository that does not verify (after
  `--retire-old`; ledger PK-39: prikk prints who sealed what only in `verify`'s text).

RFC 003 — the read path, in progress (handoff 003 T1 – T3). Still no wiki page is served; the pages that exist
are the language switcher and the notices.

- **The web foundations** (RFC 003 D-5; handoff 003 T1). `inklkunn-web` holds what every later page inherits.
  *Two origins, one listener:* a request goes to the router of the origin its `Host` names, and a host that
  names neither is misdirected (421), so the content origin never answers an application route and the
  application never serves wiki-authored bytes. *Headers on every response* (200, 304, 404, 405, 421, errors),
  set in one function that a handler cannot override for the security headers: a Content-Security-Policy with
  `default-src 'none'` and **no script source at all** (one stylesheet from the application origin), a
  sandboxed policy on the content origin, `nosniff`, `Referrer-Policy: same-origin`, `Permissions-Policy`,
  frame denial, cross-origin policies, HSTS on https, and `Cache-Control: private, no-cache` with `Vary` unless
  a handler says otherwise. *One route registry:* routes are registered in `routes.rs` only (a gate), each with
  an action or a stated public reason, and a test fails any route that could shadow a page title (nothing but a
  wiki's slug and the reserved prefix `-/` may precede one). *Validated at start, every refusal named:* the
  bind address and the two origins are checked together (http only for loopback; the origins differ; on a
  reachable address the content origin has its own host), `Sandbox::Unconfined` needs the `--dev-unconfined`
  flag and prints a banner, prikk must be found at the floor, the store must open. `inklkunn serve [--config
  FILE]` reads `NAME=value` lines (a typo is refused with its line number) under the environment;
  `--version` prints the floor and the prikk found. The only route is `/-/health`. Dependencies: `axum`,
  `tokio` (1.54.1, past RUSTSEC-2026-0386) and `tower-service` (+26 lock entries, measured).

- **The words, the layout and the stylesheet** (RFC 003 D-6; handoff 003 T2; no new dependency). Every sentence
  a page says is a message in a catalog (`inklkunn-web::catalog`): stable keys, whole sentences with named
  placeholders, plural forms, and typed slots for a link or for a name in another language, so a message cannot
  carry markup. English is the source; Japanese is compiled in as a **preview**: the language switcher offers it
  with the word "preview" beside it, and it is chosen only by the reader (one `HttpOnly`, `SameSite=Lax` cookie
  holding a language code), never from a browser's `Accept-Language`, until the reviewers named under DR-7
  sign it off. The switcher at `/-/lang` is a form per language that needs no script, answers a form from
  another site with a refusal page, and returns only to a local address. `docs/glossary.md` fixes the words in
  both languages (a test holds it equal to both catalogs) and retires the version-control vocabulary (`seal`,
  `commit`, `rebase`, `patch`, `block`, `ref`) outside explanations a reader opens on demand. One base layout (a
  skip link first, a banner, one `main`, a footer, `lang` and `dir` on `<html>`) and one stylesheet with light
  and dark colour tokens, served from the application origin under a name made of its own bytes with a
  year-long `immutable` cache lifetime. The tests every later page inherits, each with its planted violation:
  the catalog audit (missing, orphan, duplicate, mismatched placeholders, plural forms, markup, braces),
  literal text in a template or a source file outside an allowlist whose dead lines fail, `|safe`, the retired
  words, nineteen structural rules over every page state in both languages parsed with `html5ever`, and the
  stylesheet checks (contrast of every pair in both schemes computed from the tokens, colours only as tokens,
  logical properties only, focus, reduced motion, 24 px targets, forced colours, nothing loaded). A one-off
  headless-Chromium run, which is not part of the build, found no sideways scroll at 320 px, with or without
  the text-spacing override, in either language; no person using assistive technology has tested any of it,
  and no fluent reader has read the Japanese. *Output change:* an unknown address on the application origin is
  now a 404 page in the visitor's language, not the text `Not found.`; the content origin's 404 and the 421 are
  one catalog sentence of plain text; `/-/health` and the 404 handler moved out of `routes.rs`.

- **The read path** (RFC 003 D-2; handoff 003 T3; no new dependency). `inklkunn-core::read` serves a page from a
  held tip: the title index (titles, aliases, redirects, page ids) is built from **one `extract` of the tip** into
  the wiki's scratch directory, once per tip however many readers meet it, and rendered pages are cached by the
  tip's state id, the path, the catalog language and the link base. A cached view makes no prikk call and a miss
  makes one once the index is there (a counting harness holds both). Everything a page carries reaches a template
  only as `SafeHtml`, which only the `ammonia` sanitizer can construct (a policy built from the markup's own
  vocabulary plus the profile's raw-HTML subset, empty today; a cached row is sanitized again on the way out); the
  cache has no field for a verification result. `extract`'s refusal for want of prikk's checkpoint record is the
  ordinary "could not be read now" and flags the wiki for a refresh; a request never passes
  `--allow-full-replay` (a test scans the driver's sources). The store gains **schema 5** (derived pages, the title
  index, and the wiki's checkpoint refresh; a database at 4 migrates forward with its data) and the driver an
  optional size hint that stands in for the store walk (flat verbs no longer walk the store). `inklkunn-markup`
  writes a block identifier as the `id` of its block (MKUP-12) and renders `![[Page]]` to depth 2, refusing a cycle
  and naming it (and a budget of 32 pages and 4 MiB per page, which the handoff does not state). Measured on
  spike A's corpora with prikk 0.54.1, a cold page on 10,000 pages takes about 0.55 s at 76 MiB and **about
  0.98 s at 212 MiB** (0.38 s and 0.64 s of it the index build), which is at the 1 s PERF-1 draws; a cached view
  takes 0.2 ms. *Not done and stated:* an author's own HTML that spells an element the renderer writes (a plain
  `<a href>`) passes the sanitizer, without `rel="nofollow"`; a stale page is not served when prikk is busy
  (OP-03). *Output change:* a block identifier is the `id` of its paragraph or block (`<p id="^p1">`), no longer a
  separate empty `<span>`; `![[Page]]` of a page shows that page (it was a link). *API changes:*
  `StorageError::Refused` carries the refusal's `reason`; `WikiStore` has two more methods
  (`request_checkpoint_refresh`, `record_checkpoint_refresh`); `RenderLinks` has three more for transclusion.
