//! Who holds which role, and who is restricted (RFC 006 D-6 and D-7's restriction half; handoff G-T2; threat model C-3g,
//! C-14f, C-14h): the people page of a wiki, and the forms that change a role, restrict a person, renew and lift a
//! restriction. **Web only** (RFC 006 D-12): nothing here is reachable with a token, and the API only reads.
//!
//! **Every handler begins as every handler of a wiki does** ([`Ctx::open`](crate::proposals::Ctx)): who is asking, which
//! wiki the slug names, whether that principal may read it. A wiki that is absent and a wiki that may not be read are one
//! page. The forms need an account, the CSRF token of the session, and a post from this site; the act itself is the core's
//! ([`PeopleService`](inklkunn_core::PeopleService)), which authorizes it, applies the rule, records it, and refuses a
//! person who is restricted with the reason and the end. This crate writes no record and holds no key (INV-4).
//!
//! The pieces: `roster` (the people page), `acts` (the four forms), `sentences` (the role names and what each refusal says),
//! `paths` (the addresses) and `fields` (the forms' field names).

pub(crate) mod acts;
pub(crate) mod fields;
pub(crate) mod paths;
pub(crate) mod roster;
pub(crate) mod sentences;

/// The body bound of every form of the people pages: a person, a role or a number of days, and one line.
pub(crate) const FORM_LIMIT: usize = crate::proposals::SMALL_FORM_LIMIT;
