# `wiki-repo` — the driver's test fixture

A tiny Portable Wiki Bundle in a prikk repository, built on 2026-10-27 with prikk 0.54.1 and a
**throwaway key pair** created for the purpose (its seeds were discarded; only the public keys are in the
repository, as in any prikk repository). Two sealed blocks:

1. `Create the fixture wiki` — `wiki-bundle.yaml`, `pages/Home.md`, `pages/Guides/Getting Started.md`,
   `attachments/diagram.svg` and its `.meta.yaml` sidecar.
2. `Proposal: 019a0000-…aaaa` (CT-03 trailers) — `pages/Home.md` edited, `pages/Glossary.md` added.

It is committed as files so that the driver's tests need **no write verb anywhere in this crate** (the
spawn gate's rule) and run anywhere a prikk at the floor exists; tests copy it to a temporary directory
before reading it. Empty directories are not tracked by git; prikk 0.54.1 reads a copy without them.

Recipe (in a scratch directory, with `HOME` pointed at an empty directory so no real keys are touched):

```sh
export HOME="$(mktemp -d)"; mkdir wiki-repo && cd wiki-repo && prikk setup .
# write the files of block 1, then:
prikk commit -m "Create the fixture wiki" && prikk seal --allow-no-audit
# write the files of block 2, then:
prikk commit -m "$(printf 'Proposal: 019a0000-0000-7000-8000-00000000aaaa\nProposed-By: fixture\nAccepted-By: fixture\nKind: edit\n\nEdit Home; add the Glossary')"
prikk seal --allow-no-audit
```

Rebuild it when the prikk floor moves to a release whose on-disk format differs (RFC 010 re-baseline), and
record the date, the prikk version and the block messages here.
