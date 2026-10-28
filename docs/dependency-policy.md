# inklkunn — dependency policy and supply-chain ledger

| | |
|---|---|
| Document | The rules that produce inklkunn's dependency tree, and the record of every supply-chain decision (accepted with its measured cost, rejected with its reason), so settled battles are not refought and new ones are decided the same way. |
| As of | 2026-10-28 (lock: 209 entries — the 12 workspace members and 192 external crates, five of which (`chacha20`, `cpufeatures`, `getrandom`, `rand_core`, `syn`) are locked at two versions) |
| Basis | RFC 010 D-5 (the triage ladder), D-6 (this policy and ledger), D-9 (the pinned build chain); threat model T-11, INV-9; planeter's dependency policy, whose settled rejections are carried in as evidence |

## Rules

1. **Measure before commit.** Any manifest change is a visible, separate step: build with the candidate,
   count the lock (`grep -c '^name = ' Cargo.lock`), run `cargo deny check` and `cargo audit`, and record
   the delta here in the form "N → M crates (+K); deny: …; audit: …; new licenses: …" before the commit
   that adopts it.
2. **Licenses are an allow-list** (`deny.toml`): Apache-2.0, MIT, Apache-2.0 WITH LLVM-exception,
   Unicode-3.0, BSD-3-Clause, ISC, MPL-2.0 (file-level copyleft). Adding one is a decision request to the
   owner with the crate, the path by which it enters, and the compatibility argument.
3. **Advisories are triaged on the ladder** (RFC 010 D-5): upgrade; remove the path; a scoped ignore with
   the reachability shown and a `reason` in both `deny.toml` and `.cargo/audit.toml`; never silent.
   No ignore exists today.
4. **Prefer a confined subprocess to an in-process protocol stack**: prikk runs under bubblewrap; an
   outbound HTTPS client is a confined `curl` through the egress guard unless a measurement justifies an
   in-process client.
5. **`forbid(unsafe_code)` in every crate** and `[lints] workspace = true` in every manifest (the boundary
   gate checks both); no crate may bring a signing path outside `inklkunn-signer` (the key-reachability
   gate, which also keeps the writable sandbox mounts and the bootstrap signer out of every other crate);
   the layering gate refuses an upward dependency and an unlayered crate.
6. **`--locked` everywhere**; the lock is committed. `deny.toml` checks the graph with `all-features`, so
   a crate that is locked but never built is still in what deny scans.
7. **Test-only features never reach a product crate**: `inklkunn-prikk`'s `test-knobs` and
   `inklkunn-signer`'s `bootstrap` are enabled only from dev-dependencies (the boundary gate checks).

## Ledger — accepted

| Date | Crate(s) | Cost (lock) | Notes |
|---|---|---|---|
| 2026-10-28 | `askama` 0.16 (`derive`, `std`; no default features) — runtime of `inklkunn-web`; `ammonia` 4 — runtime of `inklkunn-core`; `html5ever` 0.40 — dev-only in `inklkunn-web` (handoff 003 T2 and T3, adopted together so both tasks start from one measured tree) | 155 → 209 (+54: `askama`, `askama_derive`, `askama_macros`, `askama_parser`; `ammonia` and its HTML stack `html5ever`, `markup5ever`, `web_atoms`, `string_cache`, `string_cache_codegen`, `phf`, `phf_codegen`, `phf_generator`, `phf_shared`, `siphasher`, `tendril`, `utf8_iter`, `new_debug_unreachable`, `precomputed-hash`, `parking_lot`, `parking_lot_core`, `lock_api`, `scopeguard`, `redox_syscall`, `log`, `maplit`, `cssparser`, `dtoa`, `dtoa-short`, `synstructure`, `displaydoc`; and `url`, `idna`, `idna_adapter`, `stable_deref_trait` with the Unicode data crates they need: `icu_collections`, `icu_locale_core`, `icu_normalizer`, `icu_normalizer_data`, `icu_properties`, `icu_properties_data`, `icu_provider`, `litemap`, `potential_utf`, `tinystr`, `writeable`, `yoke`, `yoke-derive`, `zerofrom`, `zerofrom-derive`, `zerotrie`, `zerovec`, `zerovec-derive`; `rustc-hash` was already locked); deny: advisories, bans, licenses, sources ok (exit 0); audit: 0 vulnerabilities, 1,380 advisories, 209 crates (exit 0); new licenses: `Unicode-3.0` (the ICU crates; already allowed for `unicode-ident`), `MPL-2.0` (`cssparser`, `dtoa-short`: file-level weak copyleft, in the allow-list and now encountered), the rest MIT or Apache-2.0 | RFC 003 D-3 requires every rendered page to pass a strict HTML sanitizer before any template sees it (threat model T-6); `ammonia` is the house choice (planeter) and its tree is the cost, mostly the Unicode tables behind `url`. RFC 003 D-5/D-6 want compiled, auto-escaping templates: `askama` with the `config` feature off (it would pull a TOML parser this project has no use for) is +4. The structural tests (D-6) parse rendered pages with `html5ever`, which `ammonia` already locks, so it adds nothing beyond the sanitizer's own tree. No Markdown change: `pulldown-cmark` stays the one renderer. **MSRV** — `cargo +1.88.0 check --workspace --all-targets` passes. |
| 2026-10-28 | `axum` 0.8 (`http1`, `query`, `form`, `tokio`; no default features), `tokio` 1 (`rt-multi-thread`, `net`, `macros`, `sync`, `time`, `signal`), `tower-service` 0.3 — runtime of `inklkunn-web`; `tower` 0.5 (`util`) — dev-only (handoff 003 T1) | 129 → 155 (+26: `axum`, `axum-core`, `hyper`, `hyper-util`, `http`, `http-body`, `http-body-util`, `httparse`, `httpdate`, `mime`, `matchit`, `tower`, `tower-layer`, `tower-service`, `sync_wrapper`, `atomic-waker`, `tokio`, `tokio-macros`, `mio`, `socket2`, `signal-hook-registry`, `form_urlencoded`, `percent-encoding`, `serde_urlencoded`, `serde_path_to_error`, `ryu`); deny: advisories, bans, licenses, sources ok (exit 0); audit: 0 vulnerabilities, 1,380 advisories, 155 crates (exit 0); **`tokio` is 1.54.1, past RUSTSEC-2026-0386's 1.53.3**; new licenses: none outside the allow-list (MIT for axum, hyper, tokio, tower; `matchit` MIT AND BSD-3-Clause; `ryu` Apache-2.0 OR BSL-1.0; `sync_wrapper` Apache-2.0; the rest MIT or Apache-2.0) | RFC 003 D-5 and planeter's measured choice (STD-3): the standard async stack, HTTP/1.1 only behind the operator's TLS proxy. Features are trimmed to what the foundations use: no `json` extractor, no `ws`, no `multipart`, no `tracing`, no `tower-http` (the headers are one function, tested), no HTTP/2. `tower-service` is axum's own dependency, named so the origin dispatcher can hold two routers. Measured: **build time** — a clean debug build of `inklkunn-web` after `inklkunn-core` takes 3.7 s wall (14.5 s CPU); a clean release build of the `inklkunn` binary 26 s wall (70 s CPU against 48 s CPU before). **Binary size** — release `inklkunn` 3.41 MB (3.03 MB stripped) before, 4.90 MB (4.01 MB stripped) after: +1.5 MB (+1.0 MB stripped). **MSRV** — `cargo +1.88.0 check -p inklkunn-web` passes. `askama` 0.16 (+5 crates, measured on the same tree, deny and audit exit 0) is *not* added here: the task that writes the first template adds it with its own row. |
| 2026-10-28 | `inklkunn-core` and `inklkunn-store` as runtime dependencies of the `inklkunn` binary (handoff 002 T7, the key commands) | 129 → 129 (+0: workspace-internal edges); deny: advisories, bans, licenses, sources ok (exit 0); audit: 0 vulnerabilities, 1,380 advisories, 129 crates (exit 0); new licenses: none | The commands open the real signer (re-exported by `inklkunn-core`; only core may depend on the signer, and the binary does not), the SQLite store and its audit trail. No command-line or configuration crate: the grammar is four subcommands and five variables, hand-parsed and tested, as planeter does. |
| 2026-10-28 | `jsonwebtoken` 11 (`aws_lc_rs` backend, no default features), `sha2` 0.10, and `subtle` 2, `base64ct` 1, `unicode-normalization` 0.1 as direct dependencies — runtime of `inklkunn-auth` (handoff 002 T1; the owner's ruling DR-9); `argon2` 0.5 gains its `password-hash` feature there | 102 → 129 (+27: `jsonwebtoken`, `aws-lc-rs`, `aws-lc-sys`, `untrusted`, `signature`, `zeroize_derive`, `base64`, `getrandom` 0.2.17 (a second version), `sha2`; build-only `cc`-side helpers `jobserver`, `cmake`, `dunce`, `fs_extra`; and, locked for `wasm32` targets this workspace never builds, `js-sys`, `wasm-bindgen`, `wasm-bindgen-macro`, `wasm-bindgen-macro-support`, `wasm-bindgen-shared`, `bumpalo`, `rustversion`, `wasi`, `futures-core`, `futures-task`, `futures-util`, `pin-project-lite`, `slab`, `syn` 2.0.119 (a second version); `subtle`, `base64ct`, `unicode-normalization` and `password-hash` were already locked: +0); deny: advisories, bans, licenses, sources ok (exit 0); audit: 0 vulnerabilities, 1,380 advisories, 129 crates (exit 0); **`rsa`, `crypto-bigint`, `ring`, `p256`, `p384` and `num-bigint-dig` are not in the lock** (`grep -c '^name = "rsa"' Cargo.lock` is 0 for each), and no advisory is ignored; new licenses: none outside the allow-list (`aws-lc-rs` ISC AND (Apache-2.0 OR ISC); `aws-lc-sys` ISC AND (Apache-2.0 OR ISC) AND Apache-2.0 AND MIT AND BSD-3-Clause; `untrusted` ISC; the rest MIT or Apache-2.0, `dunce` CC0-1.0 OR MIT-0 OR Apache-2.0) | RFC 002 D-1 needs OpenID Connect ID-token verification against the provider's keys (RS256, ES256, ES384, EdDSA; never `none` or `HS*`). The RustCrypto backend of `jsonwebtoken` fails both supply-chain gates (RUSTSEC-2026-0387 `crypto-bigint`, RUSTSEC-2023-0071 `rsa`); the owner ruled the `aws_lc_rs` backend (DR-9, 2026-10-28; planeter D-41, ledger PL-10), which drops them. Measured here: **build time** — a clean debug build of `inklkunn-core` takes 4.4 s wall (14.6 s CPU) on the measuring machine, and the auth stack adds 5.1 s wall (31.4 s CPU, most of it compiling AWS-LC); a clean release build of a probe using argon2, sha2 and `jsonwebtoken` takes 28 s wall (91 s CPU). **Binary size** — a release probe that hashes a password and verifies a token with all four algorithms is 3.98 MB (3.51 MB stripped) against 0.54 MB (0.42 MB stripped) without `jsonwebtoken`: +3.4 MB (+3.1 MB stripped). **MSRV** — `cargo +1.88.0 check -p inklkunn-auth` passes. **No `cmake` binary is needed** (the machine measured on has none: AWS-LC builds through the `cc` crate, as the release job's compiler already does for SQLite); **both release architectures** are built on native GitHub runners (`ubuntu-24.04`, `ubuntu-24.04-arm`) that have a C toolchain, but only x86_64 was built here, so the first CI run is what confirms aarch64. `jsonwebtoken` 10.4.0 resolves to the same 26 packages and was not preferred: 11.1.0 is current. The PEM features are off (`use_pem` would add `pem` and `simple_asn1`): verification takes JWKs, and the tests sign with a DER key. `sha2` is also the PKCE `S256` digest. No `getrandom` use of ours: salts, nonces, session ids and tokens are read from `/dev/urandom` with no fallback (`random.rs`). |
| 2026-10-28 | `inklkunn-signer`'s `bootstrap` and `test-knobs` features, enabled from `inklkunn-core`'s dev-dependency (handoff 002 T6, the reconciliation's end-to-end test) | 102 → 102 (+0: a feature of a workspace crate, no new package); deny: advisories, bans, licenses, sources ok (exit 0); audit: 0 vulnerabilities, 1,380 advisories, 102 crates (exit 0); new licenses: none | The test opens the real `Signer` (cheapest key derivation, `test-knobs`) and creates its wiki through the bootstrap signer, then compares a real prikk history with the acceptance records. Both features stay behind dev-dependencies (rule 7; the boundary gate checks). |
| 2026-10-28 | `argon2` 0.5 (`alloc` only), `chacha20poly1305` 0.10 (`alloc` only), `zeroize` 1, and `serde`/`serde_json` as direct dependencies — runtime of `inklkunn-signer` (handoff 002 T3, the key store) | 80 → 102 (+22: `aead`, `argon2`, `base64ct`, `blake2`, `block-buffer`, `chacha20`, `chacha20poly1305`, `cipher`, `cpufeatures`, `crypto-common`, `digest`, `generic-array`, `inout`, `opaque-debug`, `password-hash`, `poly1305`, `rand_core`, `subtle`, `typenum`, `universal-hash`, `version_check`, `zeroize`; `chacha20`, `cpufeatures` and `rand_core` are second versions beside the dev-only ones `proptest` locks, which `cargo deny` reports as a warning and does not fail); deny: advisories, bans, licenses, sources ok (exit 0); audit: 0 vulnerabilities, 1,380 advisories, 102 crates (exit 0); new licenses: none (every new crate is MIT or Apache-2.0 or both, `subtle` is BSD-3-Clause, already allowed) | RFC 002 D-3 asks for an AEAD and a memory-hard derivation. Argon2id (RFC 9106) derives the master key once at start from the operator's secret; XChaCha20-Poly1305 seals each wiki's two seeds under a 192-bit random nonce, so a repeated nonce is not a practical concern; `zeroize` clears seeds, the secret and the master key when dropped. All three are RustCrypto crates, as planeter carries for its auth (`argon2`: +15 there with `sha2`, `subtle`, `base64ct`). Smaller alternatives were not found: an AEAD is not hand-rolled, and AES-GCM would add a comparable tree. No `getrandom` crate: nonces and salts are read from `/dev/urandom` with no fallback (a weak source would silently weaken the cipher). The derivation costs 56 ms and 64 MiB in a release build, once per process. `serde` and `serde_json` were already locked for other crates. |
| 2026-10-28 | `inklkunn-store` as a runtime dependency of `inklkunn-signer`; `proptest` 1 (dev-only) in `inklkunn-signer` — handoff 002 T5 | 80 → 80 (+0: a workspace-internal edge, and a crate already locked for `inklkunn-markup`'s tests); deny: advisories, bans, licenses, sources ok (exit 0); audit: 0 vulnerabilities, 1,380 advisories, 80 crates (exit 0); new licenses: none | The signer composes and re-parses the patch message (RFC 002 D-5) and, from T3 on, names key files by wiki id: both need the validated identifiers and `DisplayName`, which live in the lower `inklkunn-store`. Both crates are lower-layer, so the layering gate allows the edge; the key-reachability gate constrains who depends on the signer and is unaffected. |
| 2026-10-27 | `inklkunn-signer` and `inklkunn-core`: no external crate | 79 → 79 (+0) | The signer drives prikk's write verbs through the same sandbox; the egress guard parses URLs and classifies addresses with `std::net` alone (no `url` crate: the guard needs scheme, host and port, and refuses userinfo and anything it cannot read, which a lenient parser would accept). |
| 2026-10-27 | `rusqlite` 0.40 (`bundled`, no default features) — runtime of `inklkunn-store` | 69 → 79 (+10: `rusqlite`, `libsqlite3-sys`, `fallible-iterator`, `fallible-streaming-iterator`, `smallvec`; build-time `cc`, `find-msvc-tools`, `pkg-config`, `shlex`, `vcpkg`); deny ok; audit: 0 vulnerabilities; new licenses: none (MIT, MIT OR Apache-2.0) | RFC 001 D-8, internal design IQ-5: SQLite first, the PostgreSQL path kept by the trait. `bundled` compiles SQLite from source so the floor does not depend on the host's library (planeter's choice, 2026-09-23). `rusqlite`'s own tree is 5 crates; the other 5 are the C build. |
| 2026-10-27 | `serde` 1, `serde_json` 1 — runtime of `inklkunn-prikk` (the typed `--format json` reports) | 69 → 69 (+0: both already in the tree for `inklkunn-markup`); deny ok; audit: 0 vulnerabilities | RFC 001 D-3. No crate for the sandbox or the ceiling: `bwrap` and `prlimit` are confined subprocesses (rule 4), as in planeter. |
| 2026-10-27 | `pulldown-cmark` 0.13 (no default features; `html`), `serde` 1 (derive), `serde_json` 1, `unicode-normalization` 0.1 — runtime of `inklkunn-markup`; `proptest` 1 — dev-only | 11 → 69 (+58, of which the runtime set is `pulldown-cmark`, `pulldown-cmark-escape`, `bitflags`, `memchr`, `unicase`, `serde`, `serde_core`, `serde_derive`, `serde_json`, `itoa`, `zmij`, `unicode-ident`, `proc-macro2`, `quote`, `syn`, `unicode-normalization`, `tinyvec`; the rest is `proptest`'s test-only tree: `rand`, `regex-syntax`, `tempfile`, `rustix`, `bit-set`, `rusty-fork`, `wait-timeout`); deny: advisories, bans, licenses, sources ok; audit: 0 vulnerabilities; new licenses: `Unlicense OR MIT` (memchr, taken as MIT), `Zlib OR Apache-2.0 OR MIT` (tinyvec, taken as MIT), `(MIT OR Apache-2.0) AND Unicode-3.0` (unicode-ident, unicode-normalization) | RFC 001 D-7: a CommonMark-conformant parser is a requirement (MKUP-2) and `pulldown-cmark` is the house choice (planeter); NFC normalization is required by MKUP-19 and I18N-1; `proptest` carries the title-to-path fuzz (D-6). `pulldown-cmark`'s default features (`getopts`, `simd`) are off: no binary, no SIMD dependency. |

## Ledger — rejected (do not re-propose without new facts)

| Date | Candidate | Why |
|---|---|---|
| 2026-10-27 | a YAML crate for frontmatter | the vocabulary needs only the plain subset (META-1), and the subset must *refuse* anchors and tags (threat model C-4b); a small in-crate parser (≈ 400 lines, corpus-tested) is smaller than any YAML crate's tree and makes the refusals explicit |
| carried from planeter (2026-09-23, 2026-09-24) | `ssh-key`; in-process `rustls` + `russh`; `jsonwebtoken` on `ring`/`aws-lc` (**overturned for `aws-lc` by DR-9, 2026-10-28**: re-measured against inklkunn's own tree, see the accepted row of that date; `ring` stays rejected); `reqwest`/`hyper` for outbound HTTPS | each locked an unfixable advisory or a license failure into planeter's lock; the same facts apply here until re-measured against inklkunn's own tree |

## Runtime prerequisites (not crates, but part of the supply chain)

prikk ≥ 0.54.0 (the floor; checksum-verified release asset in the image), bubblewrap, prlimit (util-linux).

## Appendix — lock inventory

Generated by `python3 tools/spikes/lock-inventory.py` from `Cargo.lock` and `cargo tree`; the test
`tools/pin-check/tests/ledger.rs` fails when this table and the lock disagree. A crate marked *dev-only* or
*build-only* never ships in the binary; one marked *locked but never built* is in the lock because Cargo
resolves optional dependencies of dependencies ahead of any feature choice, and is never compiled here.

192 crates.

| crate | version | enters through |
|---|---|---|
| `aead` | 0.5.2 | `chacha20poly1305` via `inklkunn-signer` |
| `ammonia` | 4.2.4 | `ammonia` via `inklkunn-core` |
| `argon2` | 0.5.3 | `argon2` via `inklkunn-auth`; `argon2` via `inklkunn-signer` |
| `askama` | 0.16.1 | `askama` via `inklkunn-web` |
| `askama_derive` | 0.16.1 | `askama` via `inklkunn-web` |
| `askama_macros` | 0.16.1 | `askama` via `inklkunn-web` |
| `askama_parser` | 0.16.1 | `askama` via `inklkunn-web` |
| `atomic-waker` | 1.1.2 | `axum` via `inklkunn-web` |
| `autocfg` | 1.5.1 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `aws-lc-rs` | 1.18.2 | `jsonwebtoken` via `inklkunn-auth` |
| `aws-lc-sys` | 0.46.0 | `jsonwebtoken` via `inklkunn-auth` |
| `axum` | 0.8.9 | `axum` via `inklkunn-web` |
| `axum-core` | 0.5.6 | `axum` via `inklkunn-web` |
| `base64` | 0.22.1 | `jsonwebtoken` via `inklkunn-auth` |
| `base64ct` | 1.8.3 | `argon2` via `inklkunn-auth`; `argon2` via `inklkunn-signer`; +1 more |
| `bit-set` | 0.11.1 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `bit-vec` | 0.10.1 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `bitflags` | 2.13.2 | `ammonia` via `inklkunn-core`; `pulldown-cmark` via `inklkunn-markup`; +1 more |
| `blake2` | 0.10.6 | `argon2` via `inklkunn-auth`; `argon2` via `inklkunn-signer` |
| `block-buffer` | 0.10.4 | `argon2` via `inklkunn-auth`; `argon2` via `inklkunn-signer`; +1 more |
| `borsh` | 1.8.1 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `borsh-derive` | 1.8.1 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `bumpalo` | 3.20.3 | `jsonwebtoken` via `inklkunn-auth` |
| `bytes` | 1.12.1 | `axum` via `inklkunn-web` |
| `cc` | 1.10.0 | `jsonwebtoken` via `inklkunn-auth` (build-only); `rusqlite` via `inklkunn-store` (build-only) |
| `cfg-if` | 1.0.5 | `ammonia` via `inklkunn-core`; `chacha20poly1305` via `inklkunn-signer`; +2 more |
| `cfg_aliases` | 0.2.2 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `chacha20` | 0.10.2 | `chacha20poly1305` via `inklkunn-signer` |
| `chacha20poly1305` | 0.10.1 | `chacha20poly1305` via `inklkunn-signer` |
| `cipher` | 0.4.4 | `chacha20poly1305` via `inklkunn-signer` |
| `cmake` | 0.1.58 | `jsonwebtoken` via `inklkunn-auth` (build-only) |
| `core_detect` | 1.0.0 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `cpufeatures` | 0.3.1 | `argon2` via `inklkunn-auth`; `argon2` via `inklkunn-signer`; +2 more |
| `crypto-common` | 0.1.7 | `argon2` via `inklkunn-auth`; `argon2` via `inklkunn-signer`; +2 more |
| `cssparser` | 0.38.0 | `ammonia` via `inklkunn-core` |
| `digest` | 0.10.7 | `argon2` via `inklkunn-auth`; `argon2` via `inklkunn-signer`; +1 more |
| `displaydoc` | 0.2.7 | `ammonia` via `inklkunn-core` |
| `dtoa` | 1.0.11 | `ammonia` via `inklkunn-core` |
| `dtoa-short` | 0.3.5 | `ammonia` via `inklkunn-core` |
| `dunce` | 1.0.5 | `jsonwebtoken` via `inklkunn-auth` (build-only) |
| `equivalent` | 1.0.3 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `errno` | 0.3.14 | `axum` via `inklkunn-web`; `tokio` via `inklkunn-web` |
| `fallible-iterator` | 0.3.0 | `rusqlite` via `inklkunn-store` |
| `fallible-streaming-iterator` | 0.1.9 | `rusqlite` via `inklkunn-store` |
| `fastrand` | 2.5.0 | `ammonia` via `inklkunn-core` (build-only) |
| `find-msvc-tools` | 0.1.15 | `jsonwebtoken` via `inklkunn-auth` (build-only); `rusqlite` via `inklkunn-store` (build-only) |
| `fnv` | 1.0.7 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `form_urlencoded` | 1.2.2 | `ammonia` via `inklkunn-core`; `axum` via `inklkunn-web` |
| `fs_extra` | 1.3.0 | `jsonwebtoken` via `inklkunn-auth` (build-only) |
| `futures-core` | 0.3.34 | `axum` via `inklkunn-web`; `jsonwebtoken` via `inklkunn-auth` |
| `futures-task` | 0.3.34 | `axum` via `inklkunn-web`; `jsonwebtoken` via `inklkunn-auth` |
| `futures-util` | 0.3.34 | `axum` via `inklkunn-web`; `jsonwebtoken` via `inklkunn-auth` |
| `generic-array` | 0.14.7 | `argon2` via `inklkunn-auth`; `argon2` via `inklkunn-signer`; +2 more |
| `getrandom` | 0.4.3 | `jsonwebtoken` via `inklkunn-auth` |
| `hashbrown` | 0.17.1 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `html5ever` | 0.40.1 | `ammonia` via `inklkunn-core` |
| `http` | 1.5.0 | `axum` via `inklkunn-web` |
| `http-body` | 1.1.0 | `axum` via `inklkunn-web` |
| `http-body-util` | 0.1.5 | `axum` via `inklkunn-web` |
| `httparse` | 1.10.1 | `axum` via `inklkunn-web` |
| `httpdate` | 1.0.3 | `axum` via `inklkunn-web` |
| `hyper` | 1.12.0 | `axum` via `inklkunn-web` |
| `hyper-util` | 0.1.21 | `axum` via `inklkunn-web` |
| `icu_collections` | 2.4.0 | `ammonia` via `inklkunn-core` |
| `icu_locale_core` | 2.4.0 | `ammonia` via `inklkunn-core` |
| `icu_normalizer` | 2.4.0 | `ammonia` via `inklkunn-core` |
| `icu_normalizer_data` | 2.4.0 | `ammonia` via `inklkunn-core` |
| `icu_properties` | 2.4.0 | `ammonia` via `inklkunn-core` |
| `icu_properties_data` | 2.4.0 | `ammonia` via `inklkunn-core` |
| `icu_provider` | 2.4.0 | `ammonia` via `inklkunn-core` |
| `idna` | 1.1.0 | `ammonia` via `inklkunn-core` |
| `idna_adapter` | 1.2.2 | `ammonia` via `inklkunn-core` |
| `indexmap` | 2.14.2 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `inout` | 0.1.4 | `chacha20poly1305` via `inklkunn-signer` |
| `itoa` | 1.0.18 | `ammonia` via `inklkunn-core`; `askama` via `inklkunn-web`; +8 more |
| `jobserver` | 0.1.35 | `jsonwebtoken` via `inklkunn-auth` (build-only); `rusqlite` via `inklkunn-store` (build-only) |
| `js-sys` | 0.3.106 | `jsonwebtoken` via `inklkunn-auth` |
| `jsonwebtoken` | 11.1.0 | `jsonwebtoken` via `inklkunn-auth` |
| `libc` | 0.2.191 | `ammonia` via `inklkunn-core`; `argon2` via `inklkunn-auth`; +6 more |
| `libsqlite3-sys` | 0.38.2 | `rusqlite` via `inklkunn-store` |
| `linux-raw-sys` | 0.12.1 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `litemap` | 0.8.3 | `ammonia` via `inklkunn-core` |
| `lock_api` | 0.4.14 | `ammonia` via `inklkunn-core` |
| `log` | 0.4.34 | `ammonia` via `inklkunn-core` |
| `maplit` | 1.0.2 | `ammonia` via `inklkunn-core` |
| `markup5ever` | 0.40.0 | `ammonia` via `inklkunn-core` |
| `matchit` | 0.8.4 | `axum` via `inklkunn-web` |
| `memchr` | 2.8.3 | `ammonia` via `inklkunn-core`; `askama` via `inklkunn-web`; +9 more |
| `mime` | 0.3.17 | `axum` via `inklkunn-web` |
| `mio` | 1.2.4 | `axum` via `inklkunn-web`; `tokio` via `inklkunn-web` |
| `new_debug_unreachable` | 1.0.6 | `ammonia` via `inklkunn-core` |
| `num-traits` | 0.2.19 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `once_cell` | 1.21.4 | `jsonwebtoken` via `inklkunn-auth` |
| `opaque-debug` | 0.3.1 | `chacha20poly1305` via `inklkunn-signer` |
| `parking_lot` | 0.12.5 | `ammonia` via `inklkunn-core` |
| `parking_lot_core` | 0.9.12 | `ammonia` via `inklkunn-core` |
| `password-hash` | 0.5.0 | `argon2` via `inklkunn-auth`; `argon2` via `inklkunn-signer` |
| `percent-encoding` | 2.3.2 | `ammonia` via `inklkunn-core`; `axum` via `inklkunn-web` |
| `phf` | 0.14.0 | `ammonia` via `inklkunn-core` |
| `phf_codegen` | 0.14.0 | `ammonia` via `inklkunn-core` (build-only) |
| `phf_generator` | 0.14.0 | `ammonia` via `inklkunn-core` (build-only) |
| `phf_shared` | 0.14.0 | `ammonia` via `inklkunn-core` |
| `pin-project-lite` | 0.2.17 | `axum` via `inklkunn-web`; `jsonwebtoken` via `inklkunn-auth`; +1 more |
| `pkg-config` | 0.3.34 | `jsonwebtoken` via `inklkunn-auth` (build-only); `rusqlite` via `inklkunn-store` (build-only) |
| `poly1305` | 0.8.0 | `chacha20poly1305` via `inklkunn-signer` |
| `potential_utf` | 0.1.6 | `ammonia` via `inklkunn-core` |
| `precomputed-hash` | 0.1.1 | `ammonia` via `inklkunn-core` |
| `proc-macro-crate` | 3.5.0 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `proc-macro2` | 1.0.107 | `ammonia` via `inklkunn-core`; `askama` via `inklkunn-web`; +17 more |
| `proptest` | 1.12.0 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `pulldown-cmark` | 0.13.5 | `pulldown-cmark` via `inklkunn-markup` |
| `pulldown-cmark-escape` | 0.11.0 | `pulldown-cmark` via `inklkunn-markup` |
| `quick-error` | 1.2.3 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `quote` | 1.0.47 | `ammonia` via `inklkunn-core`; `askama` via `inklkunn-web`; +17 more |
| `r-efi` | 6.0.0 | `jsonwebtoken` via `inklkunn-auth` (build-only); `rusqlite` via `inklkunn-store` (build-only) |
| `rand` | 0.10.3 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `rand_core` | 0.10.1 | `argon2` via `inklkunn-auth`; `argon2` via `inklkunn-signer` |
| `rand_xorshift` | 0.5.0 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `redox_syscall` | 0.5.18 | `ammonia` via `inklkunn-core` |
| `regex-syntax` | 0.8.11 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `rusqlite` | 0.40.2 | `rusqlite` via `inklkunn-store` |
| `rustc-hash` | 2.1.3 | `askama` via `inklkunn-web` |
| `rustix` | 1.1.5 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `rustversion` | 1.0.23 | `jsonwebtoken` via `inklkunn-auth` (build-only) |
| `rusty-fork` | 0.3.1 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `ryu` | 1.0.23 | `axum` via `inklkunn-web` |
| `scopeguard` | 1.2.0 | `ammonia` via `inklkunn-core` |
| `serde` | 1.0.229 | `axum` via `inklkunn-web`; `jsonwebtoken` via `inklkunn-auth`; +12 more |
| `serde_core` | 1.0.229 | `axum` via `inklkunn-web`; `jsonwebtoken` via `inklkunn-auth`; +12 more |
| `serde_derive` | 1.0.229 | `axum` via `inklkunn-web`; `jsonwebtoken` via `inklkunn-auth`; +12 more |
| `serde_json` | 1.0.154 | `jsonwebtoken` via `inklkunn-auth`; `serde_json` via `inklkunn-auth`; +5 more |
| `serde_path_to_error` | 0.1.20 | `axum` via `inklkunn-web` |
| `serde_urlencoded` | 0.7.1 | `axum` via `inklkunn-web` |
| `sha2` | 0.10.9 | `sha2` via `inklkunn-auth` |
| `shlex` | 2.0.1 | `jsonwebtoken` via `inklkunn-auth` (build-only); `rusqlite` via `inklkunn-store` (build-only) |
| `signal-hook-registry` | 1.4.8 | `axum` via `inklkunn-web`; `tokio` via `inklkunn-web` |
| `signature` | 2.2.0 | `jsonwebtoken` via `inklkunn-auth` |
| `siphasher` | 1.0.4 | `ammonia` via `inklkunn-core` |
| `slab` | 0.4.12 | `axum` via `inklkunn-web`; `jsonwebtoken` via `inklkunn-auth` |
| `smallvec` | 1.16.4 | `ammonia` via `inklkunn-core`; `axum` via `inklkunn-web`; +1 more |
| `socket2` | 0.6.5 | `axum` via `inklkunn-web`; `tokio` via `inklkunn-web` |
| `stable_deref_trait` | 1.2.1 | `ammonia` via `inklkunn-core` |
| `string_cache` | 0.11.0 | `ammonia` via `inklkunn-core` |
| `string_cache_codegen` | 0.11.2 | `ammonia` via `inklkunn-core` (build-only) |
| `subtle` | 2.6.1 | `argon2` via `inklkunn-auth`; `argon2` via `inklkunn-signer`; +3 more |
| `syn` | 3.0.9 | `ammonia` via `inklkunn-core`; `askama` via `inklkunn-web`; +17 more |
| `sync_wrapper` | 1.0.2 | `axum` via `inklkunn-web` |
| `synstructure` | 0.14.0 | `ammonia` via `inklkunn-core` |
| `tempfile` | 3.27.0 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `tendril` | 0.5.1 | `ammonia` via `inklkunn-core` |
| `tinystr` | 0.8.5 | `ammonia` via `inklkunn-core` |
| `tinyvec` | 1.13.3 | `unicode-normalization` via `inklkunn-auth`; `unicode-normalization` via `inklkunn-markup` |
| `tokio` | 1.54.1 | `axum` via `inklkunn-web`; `tokio` via `inklkunn-web` |
| `tokio-macros` | 2.7.2 | `axum` via `inklkunn-web`; `tokio` via `inklkunn-web` |
| `toml_datetime` | 1.1.2+spec-1.1.0 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `toml_edit` | 0.25.17+spec-1.1.0 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `toml_parser` | 1.1.5+spec-1.1.0 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `tower` | 0.5.3 | `axum` via `inklkunn-web` |
| `tower-layer` | 0.3.3 | `axum` via `inklkunn-web` |
| `tower-service` | 0.3.3 | `axum` via `inklkunn-web`; `tower-service` via `inklkunn-web` |
| `typenum` | 1.20.1 | `argon2` via `inklkunn-auth`; `argon2` via `inklkunn-signer`; +2 more |
| `unarray` | 0.1.4 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `unicase` | 2.10.0 | `pulldown-cmark` via `inklkunn-markup` |
| `unicode-ident` | 1.0.27 | `ammonia` via `inklkunn-core`; `askama` via `inklkunn-web`; +17 more |
| `unicode-normalization` | 0.1.26 | `unicode-normalization` via `inklkunn-auth`; `unicode-normalization` via `inklkunn-markup` |
| `universal-hash` | 0.5.1 | `chacha20poly1305` via `inklkunn-signer` |
| `untrusted` | 0.7.1 | `jsonwebtoken` via `inklkunn-auth` |
| `url` | 2.5.8 | `ammonia` via `inklkunn-core` |
| `utf8_iter` | 1.0.4 | `ammonia` via `inklkunn-core` |
| `vcpkg` | 0.2.15 | `rusqlite` via `inklkunn-store` (build-only) |
| `version_check` | 0.9.5 | `argon2` via `inklkunn-auth` (build-only); `argon2` via `inklkunn-signer` (build-only); +2 more |
| `wait-timeout` | 0.2.1 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `wasi` | 0.11.1+wasi-snapshot-preview1 | `axum` via `inklkunn-web`; `jsonwebtoken` via `inklkunn-auth`; +1 more |
| `wasm-bindgen` | 0.2.129 | `jsonwebtoken` via `inklkunn-auth` |
| `wasm-bindgen-macro` | 0.2.129 | `jsonwebtoken` via `inklkunn-auth` |
| `wasm-bindgen-macro-support` | 0.2.129 | `jsonwebtoken` via `inklkunn-auth` |
| `wasm-bindgen-shared` | 0.2.129 | `jsonwebtoken` via `inklkunn-auth` |
| `web_atoms` | 0.3.0 | `ammonia` via `inklkunn-core` |
| `windows-link` | 0.2.1 | `ammonia` via `inklkunn-core`; `axum` via `inklkunn-web`; +1 more |
| `windows-sys` | 0.61.2 | `axum` via `inklkunn-web`; `tokio` via `inklkunn-web` |
| `winnow` | 1.0.4 | `askama` via `inklkunn-web` |
| `writeable` | 0.6.4 | `ammonia` via `inklkunn-core` |
| `yoke` | 0.8.4 | `ammonia` via `inklkunn-core` |
| `yoke-derive` | 0.8.4 | `ammonia` via `inklkunn-core` |
| `zerofrom` | 0.1.8 | `ammonia` via `inklkunn-core` |
| `zerofrom-derive` | 0.1.8 | `ammonia` via `inklkunn-core` |
| `zeroize` | 1.9.1 | `chacha20poly1305` via `inklkunn-signer`; `jsonwebtoken` via `inklkunn-auth`; +1 more |
| `zeroize_derive` | 1.5.0 | `chacha20poly1305` via `inklkunn-signer`; `jsonwebtoken` via `inklkunn-auth`; +1 more |
| `zerotrie` | 0.2.5 | `ammonia` via `inklkunn-core` |
| `zerovec` | 0.11.9 | `ammonia` via `inklkunn-core` |
| `zerovec-derive` | 0.11.7 | `ammonia` via `inklkunn-core` |
| `zmij` | 1.0.23 | `jsonwebtoken` via `inklkunn-auth`; `serde_json` via `inklkunn-auth`; +5 more |
