# Serving sign-in and acceptance — Developer Handoff v1 (RFC 002 D-1, RFC 004; M2's remaining scope)

| | |
|---|---|
| Document | The handoff that makes M2 usable from a browser: people sign in, and the server accepts. Everything it needs is built and reviewed in the crates (`inklkunn-auth`: passwords, sessions with CSRF tokens, tokens, OpenID Connect; `inklkunn-core`: the proposal service and the acceptor; `inklkunn-signer`: the write lease). What is missing is the serving: the pages, a real `Identify`, and the acceptor in `serve`. |
| Status | Written by the architect 2026-10-31 from the review of handoff 004 (decisions D1, D2, D11). **Assignable now**, in the order below; **U6 (switching writes on) waits for handoff 004's corrections P1 – P6 to be approved**; U2 follows DR-11 as the owner ruled it (2026-10-31). |
| Basis | RFC 002 D-1 (principals; constant-work sign-in; throttles; `Secure`, `HttpOnly`, `SameSite` cookies; a CSRF token on every cookie write; scoped tokens), D-2 (`authorize`), D-6 (keys in the process, seeds per call); RFC 004 D-4, D-12, D-13; requirements ID-1, ID-2, AUTH-01, AUTH-02; threat model T-1, T-2, C-1a … C-1c, C-2a, RR-1, RR-4; internal design SGN-5 / IQ-4 (the signer stays in-process in v1, behind the gate); handoff 002's corrections C7 (witnesses), C8 – C10 and follow-ups F2 – F4; handoff 004's P6. |
| Audience | The dev team. The two lenses of handoff 003 apply unchanged: a clean end state, and nothing a person can misread. |
| Handback | One review request per task group (U1 – U4; U5; U6), in `.git-exclude/review-requests/`, with the usual gates, both test runs, the mutation table, and the pages in both languages. |

## U1 — Sign-in pages (local password)

- `/-/sign-in`, `/-/sign-out`, `/-/account` (display name; the sessions with *end this session* each; tokens: create with
  scope and expiry, shown once, revoke). No script. The `Authenticator` does the work; the web layer only renders and
  routes. Sign-in is constant-work and throttled as built (C10's trusted-proxy setting applies); the refusal never says
  whether the account exists.
- The session cookie is `__Host-`, `Secure`, `HttpOnly`, `SameSite=Strict`; every form a signed-in person can submit
  carries the session's CSRF token, checked before the wiki is looked up (this is handoff 004's P6 seen from the
  other side: one mechanism, built once).
- The header shows who is signed in and the way out; a signed-out visitor sees *Sign in* only where it leads somewhere.
- Catalog entries in both languages with glossary rows; the structural and retired-words tests over every new state;
  route enumeration lists each route with its action or public reason.

## U2 — How accounts come into being (DR-11, ruled by the owner 2026-10-31)

One idea for people: **you are invited, or — where the server is open — you join.** Then you choose how you sign in.

- **An invitation** is a one-time, expiring link (default 7 days), made by the operator on the command line
  (`inklkunn account invite [--role <role> --wiki <slug>]`; later also by a wiki's administrator) and shown once. The
  person who opens it chooses a password **or** signs in with the configured OpenID Connect provider, and the account
  is bound to that credential. No password is ever sent or printed; no mail server is needed (the operator passes the
  link on as they like). The first administrator is made this way.
- **Joining** is one server setting, `INKLKUNN_JOIN = off | provider | anyone`, **default `off`**. `provider`: a first
  sign-in through the configured provider makes an account (for an organization's own provider; a public provider makes
  this open registration, and the setting's documentation says so in one sentence). `anyone`: also a local account with
  a password. A joined account starts with the new-account quotas and no role beyond what the wiki gives everyone.
- **One person, one account.** Accounts are never matched or linked by email. Adding a second credential (a password
  to a provider account, or the reverse) is the person's own act while signed in, or the operator's.
- `account disable`, `account grant <login> <role> <wiki>` and `account revoke-grant` on the command line; every act
  audited before it is done.
- **The sign-in page shows only what exists:** the provider button if one is configured; *Join* only if joining is
  open; otherwise one sentence, *Ask the person who runs this wiki for an invitation.*

## U3 — OpenID Connect switched on

- Handoff 002's follow-ups **F2** (the real-browser run committed under `rfcs/handoffs/002-identity-keys-and-signing/evidence/`),
  **F3** (`prlimit` on the `curl` vector; `curl` and `ca-certificates` in the `Dockerfile`), **F4** (the landing page
  rendered by the web layer from the catalog).
- `/-/sign-in/oidc` and the callback; providers from the settings file (issuer, client id, the secret from a file, the
  scopes); first-sign-in linking only where the wiki's policy says (RFC 002 D-1), otherwise an administrative link.
- Tested against the stand-in provider the crate already has, and once against a real provider recorded as F2 is.

## U4 — A real `Identify`

- The binary's `Identify` replaces `AnonymousOnly`: a session cookie resolves through `Authenticator::principal_for_session`
  for a read and **only** through `principal_for_write` (the cookie and the form's CSRF token checked together) for a
  write;
  a bearer token through `principal_for_token` (hashed lookup, scope ∩ role). **A request that presents a bearer token
  is judged by the token alone**: an invalid token is *invalid token*, never a fallback to the cookie (the reading's
  F7). The `IdentityAuthority` is claimed once by the binary (C7).
- Tests: cookie only, token only, both (token wins, cookie ignored), an invalid token with a valid cookie (refused), an
  expired session, a revoked token.

## U5 — The server's acceptor (D1)

- `serve` opens the signer as the command line does (the key directory and the operator's secret file from the
  settings, bubblewrap required, the seeds decrypted per call onto the tmpfs only) and builds the `ProposalAcceptor`
  with `Signer::with_leases` over the **same** `WriteLeases` the CLI acts use, `StoreLedger` as its ledger, the
  reconciliation and the notifier as built. `Services.acceptor` is set.
- A server started without the secret file still serves reads and refuses every write with one plain sentence (and one
  operator line at start); it never asks for the secret on a terminal.
- The threat model's RR-1 paragraph gains the sentence that the serving process now opens the key store (it did not in
  0.1.0); SECURITY.md's operator notes say where the secret file lives and who may read it.

## U6 — Switching writes on (after handoff 004's P1 – P6 are approved)

- The web Accept, the write API and the forms answer for signed-in people per their roles. An end-to-end test against
  real prikk: sign in, propose, comment, approve, accept; the page shows the change, *Signed into the history*, the
  badge from a fresh check; a second person's moved-base proposal is re-authored; an overlap stops with both texts.
- The 0.2.0 readiness report for the owner (green; done by a person; deferred and why; risks), and the release
  rehearsal on the candidate. The cut is the owner's.

## Non-change scope

No new crate (the auth stack is in the lock); no second signer process (IQ-4 stays v1); no anonymous proposing (ID-2:
off by default, a later policy option); no change to the acceptance path itself beyond wiring.

## Corrections from the review of 2026-11-01 (binding)

U1 – U4 are **Conditionally Approved** (`.git-exclude/reviewed/004-serve-sign-in-u1-u4-review-result.md`) and U5 **Approved**
(`.git-exclude/reviewed/004-serve-acceptor-u5-review-result.md`). **U6 may start once handoff 004's P2′ is approved**; G1 – G10
and A1 – A4 land before 0.2.0 is cut. Hand back as one request, `004-serve-sign-in-corrections-review-request.md` (U6 has its
own).

- **G1.** The proxy warning also fires for a loopback bind with no trusted proxy declared, and the server logs once, at run
  time, when a loopback peer sends `X-Forwarded-For` while no proxy is trusted (*every client counts as one address*).
  SECURITY.md's deployment notes lead with `INKLKUNN_TRUSTED_PROXIES`.
- **G2.** A provider sign-in start keeps its client key with the pending login and is refunded when the callback succeeds;
  the start requires `Sec-Fetch-Mode: navigate` and `Sec-Fetch-Dest: document` when the browser sends them; the pending
  logins are bounded per address as well as overall, so one network cannot evict everyone's.
- **G3.** Redeeming an invitation is throttled like joining, and checks that the login is free before hashing (the
  transaction checks again).
- **G4.** The CSRF layer decides with `for_write` (which fails closed) and passes only a request that is signed out; the
  principal it decided on reaches the handlers through the request's extensions, so no handler resolves it twice.
- **G5.** Sign-out with a bearer token does nothing to any cookie session (or answers *invalid token*).
- **G6.** Minting a token asks for the password again (or a fresh provider sign-in), as the key commands do.
- **G7.** `X-Forwarded-For` is read from the right, stopping at the first valid hop that is not a trusted proxy; junk further
  left is ignored.
- **G8.** Signing in while signed in ends the session presented.
- **G9.** The OpenAPI document describes cookie clients' `X-CSRF-Token` (with its drift test); one term, *the person who runs
  this wiki*, replaces *server* in the needs-account messages (glossary).
- **G10.** A password change on the account page (the current password asked; every other session of the account ended).
- **A1 – A4** (U5's follow-ups): the server made non-dumpable at start (`PR_SET_DUMPABLE` 0 through `rustix`) and
  `LimitCORE=0` in the operator notes; a start-up warning for a secret file readable by group or others or owned by another
  user than the server's or root; the docs say the secret is read *at start* and that a server without a signer refuses
  *every change to the history*; the follow-up worker survives a panicking job and logs once if its channel closes.


## Corrections from the review of 2026-11-03 (binding; **before 0.2.0**)

G1 – G10 and A1 – A4 are **Conditionally Approved** (`.git-exclude/reviewed/004-serve-sign-in-corrections-review-result.md`; no
path lets a cookie write past the CSRF check) and U6 is **Approved** (`.git-exclude/reviewed/004-serve-writes-on-u6-review-result.md`).
Hand back H1 – H6 and E1 – E2 as one request, `004-serve-sign-in-corrections-2-review-request.md`, with the readiness report
refreshed to the final tree (numbers, section 6, the retirement row once W1 – W8 of handoff 004 land).

- **H1 — SECURITY.md says only what is true.** `INKLKUNN_TRUSTED_PROXIES` lists **only the proxies' own addresses, never a network
  clients are in** (say why in one sentence: a client inside it would choose its own address); drop the `10.0.0.0/8` example.
  Signing in while signed in ends the presented session **except through the provider** (the `Strict` cookie is not sent on its
  callback; end it from the account page). A provider start cannot be spent by another site's images, scripts or frames (not *by
  any page*). The owner of an account whose sign-in is locked by someone at the account page ends that session from the account
  page, which needs no password. One sentence on the /48 limit of pending provider logins.
- **H2.** A declared proxy whose `X-Forwarded-For` cannot be read (`fell_back()`) is logged once at run time, as G1's line is.
- **H3.** With no decision attached, `Decided` is never a signed-in principal: `principal` answers `Anonymous` and
  `write_identity` refuses. A test that every write route sits behind the layer.
- **H4.** A successful invitation redemption gives its join budget back (a failed one still spends it).
- **H5.** A password change rotates the current session (end every session, start a new one, set its cookie) and still revokes
  every token; the credential, the sessions and the tokens change in one store transaction, or the message says the password may
  have changed.
- **H6 — words.** The OpenAPI text says where a cookie client finds the CSRF token (the hidden field of the page's forms); the
  stale comments in `api/write.rs` and `main.rs`; remove the unused `account:<id>` throttle key or use it, and make decision 4's
  sentence true either way.
- **E1.** The decide form is not shown to the proposer; the existing sentence `refused.own_proposal` takes its place.
- **E2.** The health page's wanted-pages rows gain *Propose this page* (the new-page form, default view only) beside *who links
  here*; `health.rs`'s module note says so. Catalog en / ja and the structural tests.
