//! Switching writes on (the sign-in handoff's U6): nothing here is product code. The module holds the end-to-end
//! tests that prove, against a real prikk, a real key store, a real SQLite store and a real socket, that a person
//! who is invited can sign in, propose, comment, review and be accepted into a signed history, through the pages
//! a browser would use and through the write API a script would use.

mod tests;
