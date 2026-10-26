# inklkunn — dependency policy and supply-chain ledger

| | |
|---|---|
| Document | The rules that produce inklkunn's dependency tree, and the record of every supply-chain decision (accepted with its measured cost, rejected with its reason), so settled battles are not refought and new ones are decided the same way. |
| As of | 2026-10-27 (lock: 69 crates after the markup and prikk crates' dependencies) |
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
5. **`forbid(unsafe_code)` in every crate**; no crate may bring a signing path outside `inklkunn-signer`
   (the key-reachability gate); the layering gate refuses an upward dependency.
6. **`--locked` everywhere**; the lock is committed.

## Ledger — accepted

| Date | Crate(s) | Cost (lock) | Notes |
|---|---|---|---|
| 2026-10-27 | `serde` 1, `serde_json` 1 — runtime of `inklkunn-prikk` (the typed `--format json` reports) | 69 → 69 (+0: both already in the tree for `inklkunn-markup`); deny ok; audit: 0 vulnerabilities | RFC 001 D-3. No crate for the sandbox or the ceiling: `bwrap` and `prlimit` are confined subprocesses (rule 4), as in planeter. |
| 2026-10-27 | `pulldown-cmark` 0.13 (no default features; `html`), `serde` 1 (derive), `serde_json` 1, `unicode-normalization` 0.1 — runtime of `inklkunn-markup`; `proptest` 1 — dev-only | 11 → 69 (+58, of which the runtime set is `pulldown-cmark`, `pulldown-cmark-escape`, `bitflags`, `memchr`, `unicase`, `serde`, `serde_core`, `serde_derive`, `serde_json`, `itoa`, `zmij`, `unicode-ident`, `proc-macro2`, `quote`, `syn`, `unicode-normalization`, `tinyvec`; the rest is `proptest`'s test-only tree: `rand`, `regex-syntax`, `tempfile`, `rustix`, `bit-set`, `rusty-fork`, `wait-timeout`); deny: advisories, bans, licenses, sources ok; audit: 0 vulnerabilities; new licenses: `Unlicense OR MIT` (memchr, taken as MIT), `Zlib OR Apache-2.0 OR MIT` (tinyvec, taken as MIT), `(MIT OR Apache-2.0) AND Unicode-3.0` (unicode-ident, unicode-normalization) | RFC 001 D-7: a CommonMark-conformant parser is a requirement (MKUP-2) and `pulldown-cmark` is the house choice (planeter); NFC normalization is required by MKUP-19 and I18N-1; `proptest` carries the title-to-path fuzz (D-6). `pulldown-cmark`'s default features (`getopts`, `simd`) are off: no binary, no SIMD dependency. |

## Ledger — rejected (do not re-propose without new facts)

| Date | Candidate | Why |
|---|---|---|
| 2026-10-27 | a YAML crate for frontmatter | the vocabulary needs only the plain subset (META-1), and the subset must *refuse* anchors and tags (threat model C-4b); a small in-crate parser (≈ 400 lines, corpus-tested) is smaller than any YAML crate's tree and makes the refusals explicit |
| carried from planeter (2026-09-23, 2026-09-24) | `ssh-key`; in-process `rustls` + `russh`; `jsonwebtoken` on `ring`/`aws-lc`; `reqwest`/`hyper` for outbound HTTPS | each locked an unfixable advisory or a license failure into planeter's lock; the same facts apply here until re-measured against inklkunn's own tree |

## Runtime prerequisites (not crates, but part of the supply chain)

prikk ≥ 0.54.0 (the floor; checksum-verified release asset in the image), bubblewrap, prlimit (util-linux).
