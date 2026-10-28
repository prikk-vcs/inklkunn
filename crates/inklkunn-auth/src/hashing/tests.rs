#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

fn cheap() -> Argon2idHasher {
    Argon2idHasher::with_cost(64, 1, 1).unwrap()
}

#[test]
fn a_password_round_trips_and_a_wrong_one_does_not() {
    let h = cheap();
    let stored = h.hash("correct horse battery staple").unwrap();
    assert!(stored.starts_with("$argon2id$v=19$"), "{stored}");
    assert!(!stored.contains("correct horse"));
    assert!(h.verify("correct horse battery staple", &stored));
    assert!(!h.verify("correct horse battery stapl", &stored));
    assert!(!h.verify("", &stored));
}

#[test]
fn two_hashes_of_one_password_differ_by_their_salt_and_both_verify() {
    let h = cheap();
    let (a, b) = (
        h.hash("same password here").unwrap(),
        h.hash("same password here").unwrap(),
    );
    assert_ne!(a, b);
    assert!(h.verify("same password here", &a) && h.verify("same password here", &b));
}

#[test]
fn a_hash_made_at_another_cost_still_verifies_because_the_cost_is_in_the_string() {
    let stored = Argon2idHasher::with_cost(128, 2, 1)
        .unwrap()
        .hash("moving the cost later")
        .unwrap();
    assert!(cheap().verify("moving the cost later", &stored));
}

#[test]
fn the_same_text_in_another_form_is_the_same_password() {
    let h = cheap();
    // A composed é and an e with a combining accent; a full-width "ｐａｓｓ" and "pass".
    let stored = h.hash("caf\u{e9} au lait!").unwrap();
    assert!(h.verify("cafe\u{301} au lait!", &stored));
    let stored = h.hash("ｐａｓｓｗｏｒｄ１２３").unwrap();
    assert!(h.verify("password123", &stored));
}

#[test]
fn a_string_that_cannot_be_verified_is_a_mismatch_never_a_panic() {
    let h = cheap();
    for bad in [
        "",
        "garbage",
        "$argon2id$",
        "$argon2i$v=19$m=64,t=1,p=1$c2FsdHNhbHRzYWx0$aGFzaGhhc2hoYXNoaGFzaGhhc2hoYXNo",
        "$scrypt$ln=4,r=8,p=1$c2FsdA$aGFzaA",
    ] {
        assert!(!h.verify("x", bad), "{bad}");
    }
}

#[test]
fn a_hash_that_asks_for_more_than_the_ceiling_is_refused_before_any_work() {
    let h = cheap();
    // 4 GiB of memory: a damaged or hostile row. It must return at once, not allocate.
    let hostile =
        "$argon2id$v=19$m=4194304,t=2,p=1$c2FsdHNhbHRzYWx0$aGFzaGhhc2hoYXNoaGFzaGhhc2hoYXNo";
    let started = std::time::Instant::now();
    assert!(!h.verify("x", hostile));
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    let many_passes =
        "$argon2id$v=19$m=64,t=1000,p=1$c2FsdHNhbHRzYWx0$aGFzaGhhc2hoYXNoaGFzaGhhc2hoYXNo";
    assert!(!h.verify("x", many_passes));
}

#[test]
fn the_production_cost_is_the_documented_one() {
    let stored = Argon2idHasher::new().hash("a production password").unwrap();
    assert!(stored.contains("m=19456,t=2,p=1"), "{stored}");
}

#[test]
fn new_passwords_are_checked_for_length_in_characters_and_bytes_and_nothing_else() {
    assert_eq!(check_new_password("short"), Err(PasswordRefusal::TooShort));
    assert_eq!(
        check_new_password("123456789"),
        Err(PasswordRefusal::TooShort)
    );
    assert!(
        check_new_password("1234567890").is_ok(),
        "no composition rules"
    );
    assert!(
        check_new_password("これは長いパスワードです").is_ok(),
        "characters, not bytes"
    );
    assert!(check_new_password("password password").is_ok());
    assert_eq!(
        check_new_password(&"a".repeat(PASSWORD_MAX_BYTES + 1)),
        Err(PasswordRefusal::TooLong)
    );
    assert!(check_new_password(&"a".repeat(PASSWORD_MAX_BYTES)).is_ok());
}

#[test]
fn the_production_cost_constants_are_valid() {
    assert_eq!(
        Argon2idHasher::new().cost_text().as_deref(),
        Some("m=19456,t=2,p=1")
    );
    assert!(
        Argon2idHasher::with_cost(1, 1, 1).is_err(),
        "below Argon2's own minimum"
    );
    assert!(Argon2idHasher::with_cost(64, 0, 1).is_err());
}

#[test]
fn only_a_well_formed_argon2id_string_within_the_ceiling_is_verifiable() {
    let h = cheap();
    let good = h.hash("a password to hash").unwrap();
    assert!(h.is_verifiable(&good));
    for unusable in [
        "",
        "!",
        "garbage",
        "$argon2i$v=19$m=64,t=1,p=1$c2FsdHNhbHRzYWx0$aGFzaGhhc2hoYXNoaGFzaGhhc2hoYXNo",
        "$argon2id$v=19$m=4194304,t=2,p=1$c2FsdHNhbHRzYWx0$aGFzaGhhc2hoYXNoaGFzaGhhc2hoYXNo",
    ] {
        assert!(!h.is_verifiable(unusable), "{unusable:?}");
        assert!(!h.verify("x", unusable), "{unusable:?}");
    }
}
