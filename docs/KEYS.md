# A wiki's keys: status, rotation and the compromise runbook

inklkunn signs changes on behalf of its wikis (SECURITY.md; threat model RR-1). Each wiki has an **author key**
and a **maintainer key**, held by the service sealed under the operator's secret (RFC 002 D-3). This page is
for the operator: the commands that show and change them, what each one costs, and what to do when a key may
have leaked. Nothing here prints a secret, and nothing here can.

## The commands

Run on the host that runs the service, as the user that owns its data directory, with the same settings as
the service (`INKLKUNN_DATA_DIR`, `INKLKUNN_SECRET_FILE`, and the optional `INKLKUNN_KEY_DIR`,
`INKLKUNN_SEED_DIR`, `INKLKUNN_PRIKK`; `src/config.rs` documents them):

```sh
inklkunn wiki keys status <wiki>                       # key ids, generation, created and rotated dates
inklkunn wiki keys create <wiki>                       # keys for an initialised repository that has none
inklkunn wiki keys rotate <wiki> --confirm <slug>      # new keys; the old maintainer key stays trusted
inklkunn wiki keys rotate <wiki> --confirm <slug> --retire-old --accept-unverifiable-history
```

`<wiki>` is the slug or the id. **A key directory with no key store in it is never made into one by `status` or `rotate`:** they refuse, naming the path, so a mistyped `INKLKUNN_KEY_DIR` cannot start a second, empty store; only `create` (and creating a wiki) makes the store, and two processes doing so at once end with one. **Re-authentication:** the key store does not open without the operator's
secret file, so these commands are the operator proving possession of it; the web administration surface will
ask for the password again instead. `rotate` also needs the slug typed, so the wrong wiki is not rotated by a
slip. `create` and `rotate` are **audited**: an event is written *before* the act (and the act is refused if it
cannot be written) and another after it (`system:operator-cli`, `keys-create` / `keys-rotate`, with what
became of the old key); a trail that shows "begun" and no ending means the process stopped in the middle. Read
the trail with `inklkunn audit export`. If a retirement fails after the keys rotated, the command exits 1,
says the old key is **still trusted**, and names the `prikk trust maintainer remove` to run by hand.

The seed directory (`INKLKUNN_SEED_DIR`) must be on a tmpfs, owned by the user the service runs as, and not
writable by anyone else (mode 0700 is right); the signer refuses a directory that others can write to, since
they could swap a call's directory for their own. The default `/dev/shm/inklkunn` is made that way.

## What a rotation does

prikk binds a key id to one public key for good and has no key rotation of its own (ledger PK-35), so a
rotation is a **new pair of key ids**: `inklkunn-<wiki id>-author-<generation>-<8 hex>` and `…-maintainer-…`.
New keys are generated on the tmpfs, the new maintainer key is adopted in the wiki's repository, and only then
is the sealed record replaced; a failure before that leaves the wiki exactly as it was. From then on changes
are signed with the new keys. A change being accepted at that moment, in this process or another, holds the wiki's write lock: the rotation is refused at once (`wiki … is being written by process N`) and changes nothing; run it again when that is over.

## Keep the old key, or retire it

This is the choice, and prikk 0.54.1 makes it a hard one (ledger **PK-34**):

| | The old maintainer key is… | History it sealed | A copy of its seed can… |
|---|---|---|---|
| **default** | still trusted in the repository | keeps verifying | still seal changes that verify |
| `--retire-old` | removed from the repository's trust | **stops verifying**: `prikk verify` fails with publication-trust issues and the wiki shows *Verification failed* | seal nothing |

Use the default for a routine rotation (a schedule, a change of operator): the service no longer holds the
old seed, and nothing else is known to. Use `--retire-old` when the old seed may be in someone else's hands,
and accept that the wiki will report *Verification failed* until prikk can keep a retired key's history
verifiable. The second flag exists so that cost cannot be paid by accident. The owner ruled this design
(DR-10, 2026-10-29) after prikk **confirmed** it on the same day: a leaked copy of a still-trusted key can
seal anything that verifies, and a rotation that keeps history verifiable while removing the key **is not
scheduled** in prikk.

### What a default rotation still tells you: seals made after the rotation

A copy of the repository cannot prove that a block was sealed by the old key *after* the new one was adopted
(adoption is local and recorded nowhere a copy can check). inklkunn can, because it knows the moment. At the
commit point of every rotation the key record keeps, for the replaced maintainer key, **the newest block of
every view**, and `inklkunn wiki keys status <wiki>` (and every rotation, which also writes a `keys-late-seal`
event to the audit trail for each finding) lists the superseded keys and **reports every block one of them
sealed that is not in the history up to those points**: *sealed by a superseded key after the rotation*. It
changes nothing by itself, and `status` exits non-zero when it finds one.

**What it covers:** a leaked seed used against the wiki's own repository while the service's records are intact
(a leaked backup, a leaked seed file). **What it does not:** a host that has been taken over (the records are
the attacker's then; that is what `--retire-old` is for), a view's tip recorded before it existed (an
empty tip: every block of that view is then "after"), and a repository that no longer verifies (after
`--retire-old` prikk prints no list of who sealed what, ledger PK-39, so the check says it could not run).

## If a wiki's keys may have leaked (the compromise runbook)

1. **Look first** (`inklkunn wiki keys status <wiki>`): a block sealed by a superseded key after its rotation
   is evidence that an old seed is in use. It is evidence of a leaked *copy* only if the service's own records
   are intact; if the host was taken over, trust nothing this report says.
2. **Decide whether the seeds could have left the host.** The seeds are decrypted only for one call, on a
   tmpfs, and stored sealed otherwise; an attacker needs the key records **and** the operator's secret, or a
   running service to ride. If only the operator's secret leaked and the key records did not, rotate (default)
   and change the secret. If the records and the secret both leaked, or the host was taken over, the seeds are
   gone: continue.
3. **Stop signing** (stop the service, or the wiki's writes) and note the time the compromise could have started.
4. **Rotate with `--retire-old --accept-unverifiable-history`.** The old key can seal nothing more.
5. **Read what happened.** Export the audit trail (`inklkunn audit export`: JSON Lines, oldest first) and read
   the wiki's history with `prikk log` in its repository. Because the history the old key sealed no longer
   verifies, `prikk verify` cannot be the check any more. The reconciliation (RFC 002 D-7: every sealed change
   against an acceptance record, and the reverse) is the check that fits, **but it is a library function today
   and not yet a command or a schedule**, and the acceptance records it reads are RFC 004's, which do not exist
   yet: until they do, the comparison is by hand, change by change, against the proposals the wiki shows.
6. **State what was signed during the window**, to the wiki's maintainers.
7. **Change the operator's secret** (it re-seals nothing; plan a re-key of the key records — not yet a command).

**After a retirement the service reports every later change as sealed but unconfirmed.** Each acceptance still
seals, and the wiki's history still grows, but the check that closes every acceptance (`prikk verify`) fails for
the old key's history, so the signer answers `SealedUnconfirmed` with the new block's ids instead of success
(a test pins this). Whatever records the acceptance (RFC 004) must treat that answer as "sealed, record the
block", never as a reason to retry.

An *author* key cannot be revoked (prikk has no author trust list): retiring it means only that the service
stops using it. That is why the maintainer key is the one that matters.

## Making a wiki, filling it, taking a copy out

```sh
inklkunn wiki create <slug> --name "<name>"            # the keys are made with the wiki; the deployment too, if it is the first
inklkunn wiki import-folder <wiki> <dir> --dry-run     # what would be written, what is refused and why; writes nothing
inklkunn wiki import-folder <wiki> <dir>               # one accepted change, Kind: import, under the system principal
inklkunn wiki export <wiki> --bundle <dir> --archive <file>
```

`create` is the act that is asked to bring a key store into being: it makes one when the key directory is new or
empty (and refuses a directory of other files, saying so). A creation that is killed leaves a mark, and the next
`create` or `serve` removes what it made, key record included, and says so on standard error. `import-folder`
refuses the whole folder when any entry is refused and names every reason; it never follows a link. An export's
destination must not exist, and both outputs are readable by you alone (they hold private pages too). The archive
is prikk's own (`prikk archive verify <file>` checks it without inklkunn). Every one of these is audited like the key
commands. A seed never appears in any of their output, in a bundle or in an archive (a test looks for the real seeds
in every file of a deployment).

## When the key store is lost

Restore it from a backup of the key directory (keep that backup **apart from** the operator's secret). There
is no command that makes new keys for a wiki that already had some: prikk refuses to adopt the original key id
with a different key (PK-35), and `create` says so rather than hiding it. If no backup exists, the wiki's
repository can still be read and exported; signing new changes needs a recovery design that RFC 002 does not
yet have (raised in the review request).
