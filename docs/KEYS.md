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

`<wiki>` is the slug or the id. **Re-authentication:** the key store does not open without the operator's
secret file, so these commands are the operator proving possession of it; the web administration surface will
ask for the password again instead. `rotate` also needs the slug typed, so the wrong wiki is not rotated by a
slip. `create` and `rotate` are **audited** (`system:operator-cli`, `keys-create` / `keys-rotate`, allowed or
failed); read the trail with the audit export.

## What a rotation does

prikk binds a key id to one public key for good and has no key rotation of its own (ledger PK-35), so a
rotation is a **new pair of key ids**: `inklkunn-<wiki id>-author-<generation>-<8 hex>` and `…-maintainer-…`.
New keys are generated on the tmpfs, the new maintainer key is adopted in the wiki's repository, and only then
is the sealed record replaced; a failure before that leaves the wiki exactly as it was. From then on changes
are signed with the new keys. Do it while the wiki is quiet: a change being accepted at that moment signs with
the keys it read first (RFC 004's lease will exclude the two).

## Keep the old key, or retire it

This is the choice, and prikk 0.54.1 makes it a hard one (ledger **PK-34**):

| | The old maintainer key is… | History it sealed | A copy of its seed can… |
|---|---|---|---|
| **default** | still trusted in the repository | keeps verifying | still seal changes that verify |
| `--retire-old` | removed from the repository's trust | **stops verifying**: `prikk verify` fails with publication-trust issues and the wiki shows *Verification failed* | seal nothing |

Use the default for a routine rotation (a schedule, a change of operator): the service no longer holds the
old seed, and nothing else is known to. Use `--retire-old` when the old seed may be in someone else's hands,
and accept that the wiki will report *Verification failed* until prikk can keep a retired key's history
verifiable (a request to prikk is open). The second flag exists so that cost cannot be paid by accident.

## If a wiki's keys may have leaked (the compromise runbook)

1. **Decide whether the seeds could have left the host.** The seeds are decrypted only for one call, on a
   tmpfs, and stored sealed otherwise; an attacker needs the key records **and** the operator's secret, or a
   running service to ride. If only the operator's secret leaked and the key records did not, rotate (default)
   and change the secret. If the records and the secret both leaked, or the host was taken over, the seeds are
   gone: continue.
2. **Stop signing** (stop the service, or the wiki's writes) and note the time the compromise could have started.
3. **Rotate with `--retire-old --accept-unverifiable-history`.** The old key can seal nothing more.
4. **Re-verify** and read what happened in the window: run the reconciliation (it reports every sealed change no
   acceptance explains and every acceptance with no change; RFC 002 D-7) and export the audit trail. Because
   history the old key sealed no longer verifies, the reconciliation and the audit trail — not `verify` — are
   what say which changes were legitimate.
5. **State what was signed during the window**, from the reconciliation, to the wiki's maintainers.
6. **Change the operator's secret** (it re-seals nothing; plan a re-key of the key records — not yet a command).

An *author* key cannot be revoked (prikk has no author trust list): retiring it means only that the service
stops using it. That is why the maintainer key is the one that matters.

## When the key store is lost

Restore it from a backup of the key directory (keep that backup **apart from** the operator's secret). There
is no command that makes new keys for a wiki that already had some: prikk refuses to adopt the original key id
with a different key (PK-35), and `create` says so rather than hiding it. If no backup exists, the wiki's
repository can still be read and exported; signing new changes needs a recovery design that RFC 002 does not
yet have (raised in the review request).
