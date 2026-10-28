# inklkunn — dependency policy and supply-chain ledger

| | |
|---|---|
| Document | The rules that produce inklkunn's dependency tree, and the record of every supply-chain decision (accepted with its measured cost, rejected with its reason), so settled battles are not refought and new ones are decided the same way. |
| As of | 2026-10-28 (lock: 102 entries — the 12 workspace members and 87 external crates, three of which (`chacha20`, `cpufeatures`, `rand_core`) are locked at two versions) |
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
| carried from planeter (2026-09-23, 2026-09-24) | `ssh-key`; in-process `rustls` + `russh`; `jsonwebtoken` on `ring`/`aws-lc`; `reqwest`/`hyper` for outbound HTTPS | each locked an unfixable advisory or a license failure into planeter's lock; the same facts apply here until re-measured against inklkunn's own tree |

## Runtime prerequisites (not crates, but part of the supply chain)

prikk ≥ 0.54.0 (the floor; checksum-verified release asset in the image), bubblewrap, prlimit (util-linux).

## Appendix — lock inventory

Generated by `python3 tools/spikes/lock-inventory.py` from `Cargo.lock` and `cargo tree`; the test
`tools/pin-check/tests/ledger.rs` fails when this table and the lock disagree. A crate marked *dev-only* or
*build-only* never ships in the binary; one marked *locked but never built* is in the lock because Cargo
resolves optional dependencies of dependencies ahead of any feature choice, and is never compiled here.

87 crates.

| crate | version | enters through |
|---|---|---|
| `aead` | 0.5.2 | `chacha20poly1305` via `inklkunn-signer` |
| `argon2` | 0.5.3 | `argon2` via `inklkunn-signer` |
| `autocfg` | 1.5.1 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `base64ct` | 1.8.3 | `argon2` via `inklkunn-signer` |
| `bit-set` | 0.11.1 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `bit-vec` | 0.10.1 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `bitflags` | 2.13.2 | `pulldown-cmark` via `inklkunn-markup`; `rusqlite` via `inklkunn-store` |
| `blake2` | 0.10.6 | `argon2` via `inklkunn-signer` |
| `block-buffer` | 0.10.4 | `argon2` via `inklkunn-signer` |
| `borsh` | 1.8.1 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `borsh-derive` | 1.8.1 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `bytes` | 1.12.1 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `cc` | 1.10.0 | `rusqlite` via `inklkunn-store` (build-only) |
| `cfg-if` | 1.0.5 | `chacha20poly1305` via `inklkunn-signer` |
| `cfg_aliases` | 0.2.2 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `chacha20` | 0.10.2 | `chacha20poly1305` via `inklkunn-signer` |
| `chacha20poly1305` | 0.10.1 | `chacha20poly1305` via `inklkunn-signer` |
| `cipher` | 0.4.4 | `chacha20poly1305` via `inklkunn-signer` |
| `core_detect` | 1.0.0 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `cpufeatures` | 0.3.1 | `argon2` via `inklkunn-signer`; `chacha20poly1305` via `inklkunn-signer` |
| `crypto-common` | 0.1.7 | `argon2` via `inklkunn-signer`; `chacha20poly1305` via `inklkunn-signer` |
| `digest` | 0.10.7 | `argon2` via `inklkunn-signer` |
| `equivalent` | 1.0.3 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `errno` | 0.3.14 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `fallible-iterator` | 0.3.0 | `rusqlite` via `inklkunn-store` |
| `fallible-streaming-iterator` | 0.1.9 | `rusqlite` via `inklkunn-store` |
| `fastrand` | 2.5.0 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `find-msvc-tools` | 0.1.15 | `rusqlite` via `inklkunn-store` (build-only) |
| `fnv` | 1.0.7 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `generic-array` | 0.14.7 | `argon2` via `inklkunn-signer`; `chacha20poly1305` via `inklkunn-signer` |
| `getrandom` | 0.4.3 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `hashbrown` | 0.17.1 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `indexmap` | 2.14.2 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `inout` | 0.1.4 | `chacha20poly1305` via `inklkunn-signer` |
| `itoa` | 1.0.18 | `serde_json` via `inklkunn-corpus`; `serde_json` via `inklkunn-markup`; +3 more |
| `libc` | 0.2.191 | `argon2` via `inklkunn-signer`; `chacha20poly1305` via `inklkunn-signer` |
| `libsqlite3-sys` | 0.38.2 | `rusqlite` via `inklkunn-store` |
| `linux-raw-sys` | 0.12.1 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `memchr` | 2.8.3 | `pulldown-cmark` via `inklkunn-markup`; `serde_json` via `inklkunn-corpus`; +4 more |
| `num-traits` | 0.2.19 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `once_cell` | 1.21.4 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `opaque-debug` | 0.3.1 | `chacha20poly1305` via `inklkunn-signer` |
| `password-hash` | 0.5.0 | `argon2` via `inklkunn-signer` |
| `pkg-config` | 0.3.34 | `rusqlite` via `inklkunn-store` (build-only) |
| `poly1305` | 0.8.0 | `chacha20poly1305` via `inklkunn-signer` |
| `proc-macro-crate` | 3.5.0 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `proc-macro2` | 1.0.107 | `serde_json` via `inklkunn-corpus`; `serde_json` via `inklkunn-markup`; +8 more |
| `proptest` | 1.12.0 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `pulldown-cmark` | 0.13.5 | `pulldown-cmark` via `inklkunn-markup` |
| `pulldown-cmark-escape` | 0.11.0 | `pulldown-cmark` via `inklkunn-markup` |
| `quick-error` | 1.2.3 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `quote` | 1.0.47 | `serde_json` via `inklkunn-corpus`; `serde_json` via `inklkunn-markup`; +8 more |
| `r-efi` | 6.0.0 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `rand` | 0.10.3 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `rand_core` | 0.10.1 | `argon2` via `inklkunn-signer` |
| `rand_xorshift` | 0.5.0 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `regex-syntax` | 0.8.11 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `rusqlite` | 0.40.2 | `rusqlite` via `inklkunn-store` |
| `rustix` | 1.1.5 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `rusty-fork` | 0.3.1 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `serde` | 1.0.229 | `serde_json` via `inklkunn-corpus`; `serde_json` via `inklkunn-markup`; +8 more |
| `serde_core` | 1.0.229 | `serde_json` via `inklkunn-corpus`; `serde_json` via `inklkunn-markup`; +8 more |
| `serde_derive` | 1.0.229 | `serde_json` via `inklkunn-corpus`; `serde_json` via `inklkunn-markup`; +8 more |
| `serde_json` | 1.0.154 | `serde_json` via `inklkunn-corpus`; `serde_json` via `inklkunn-markup`; +3 more |
| `shlex` | 2.0.1 | `rusqlite` via `inklkunn-store` (build-only) |
| `smallvec` | 1.16.4 | `rusqlite` via `inklkunn-store` |
| `subtle` | 2.6.1 | `argon2` via `inklkunn-signer`; `chacha20poly1305` via `inklkunn-signer` |
| `syn` | 3.0.9 | `serde_json` via `inklkunn-corpus`; `serde_json` via `inklkunn-markup`; +8 more |
| `tempfile` | 3.27.0 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `tinyvec` | 1.13.3 | `unicode-normalization` via `inklkunn-markup` |
| `toml_datetime` | 1.1.2+spec-1.1.0 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `toml_edit` | 0.25.17+spec-1.1.0 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `toml_parser` | 1.1.5+spec-1.1.0 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `typenum` | 1.20.1 | `argon2` via `inklkunn-signer`; `chacha20poly1305` via `inklkunn-signer` |
| `unarray` | 0.1.4 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `unicase` | 2.10.0 | `pulldown-cmark` via `inklkunn-markup` |
| `unicode-ident` | 1.0.27 | `serde_json` via `inklkunn-corpus`; `serde_json` via `inklkunn-markup`; +8 more |
| `unicode-normalization` | 0.1.26 | `unicode-normalization` via `inklkunn-markup` |
| `universal-hash` | 0.5.1 | `chacha20poly1305` via `inklkunn-signer` |
| `vcpkg` | 0.2.15 | `rusqlite` via `inklkunn-store` (build-only) |
| `version_check` | 0.9.5 | `argon2` via `inklkunn-signer` (build-only); `chacha20poly1305` via `inklkunn-signer` (build-only) |
| `wait-timeout` | 0.2.1 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `windows-link` | 0.2.1 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `windows-sys` | 0.61.2 | `proptest` via `inklkunn-markup` (dev-only); `proptest` via `inklkunn-signer` (dev-only) |
| `winnow` | 1.0.4 | locked but never built: an optional dependency of a dependency that no feature of this workspace enables (still scanned by deny and audit) |
| `zeroize` | 1.9.1 | `chacha20poly1305` via `inklkunn-signer`; `zeroize` via `inklkunn-signer` |
| `zmij` | 1.0.23 | `serde_json` via `inklkunn-corpus`; `serde_json` via `inklkunn-markup`; +3 more |
