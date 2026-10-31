# Serving sign-in and acceptance — Developer Handoff v1 (RFC 002 D-1, RFC 004; M2's remaining scope)

| | |
|---|---|
| Document | The handoff that makes M2 usable from a browser: people sign in, and the server accepts. Everything it needs is built and reviewed in the crates (`inklkunn-auth`: passwords, sessions with CSRF tokens, tokens, OpenID Connect; `inklkunn-core`: the proposal service and the acceptor; `inklkunn-signer`: the write lease). What is missing is the serving: the pages, a real `Identify`, and the acceptor in `serve`. |
| Status | Written by the architect 2026-10-31 from the review of handoff 004 (decisions D1, D2, D11). **Assignable now**, in the order below; **U6 (switching writes on) waits for handoff 004's corrections P1 – P6 to be approved**; U2's account-creation policy waits for the owner's **DR-11** (the rest of U2 does not). |
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

## U2 — How accounts come into being

- **Always (no ruling needed):** an operator path on the command line — `inklkunn account create <login>` (prints a
  one-time set-password link, never a password), `account disable`, `account grant <login> <role> <wiki>`,
  `account revoke-grant`, each audited before the act. The first administrator is made this way.
- **The policy (DR-11, the owner's):** whether people may also make their own account. Build what the ruling says;
  until it lands, only the operator path exists and the sign-in page says how to get an account in one plain sentence.

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
