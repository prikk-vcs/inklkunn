#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn the_ladder_is_the_order() {
    for pair in Role::ALL.windows(2) {
        let [lower, higher] = pair else {
            panic!("windows(2) yields pairs")
        };
        assert!(lower < higher, "{lower:?} < {higher:?}");
    }
    assert!(Role::Admin > Role::Maintainer && Role::Maintainer > Role::Reviewer);
    assert!(Role::Observer < Role::Questioner);
    for pair in Scope::ALL.windows(2) {
        let [narrower, wider] = pair else {
            panic!("windows(2) yields pairs")
        };
        assert!(narrower < wider, "{narrower:?} < {wider:?}");
    }
}

#[test]
fn names_round_trip_and_nothing_else_parses() {
    for r in Role::ALL {
        assert_eq!(Role::parse(r.as_str()), Some(r));
    }
    for s in Scope::ALL {
        assert_eq!(Scope::parse(s.as_str()), Some(s));
    }
    for bad in [
        "",
        "Admin",
        "ADMIN",
        "owner",
        "visitor",
        " admin",
        "admin ",
        "read-only",
    ] {
        assert_eq!(Role::parse(bad), None, "{bad:?}");
    }
    for bad in ["", "Read", "write", "propose ", "root"] {
        assert_eq!(Scope::parse(bad), None, "{bad:?}");
    }
    // The serialized form is the stored name.
    assert_eq!(
        serde_json::to_string(&Role::Reviewer).unwrap(),
        "\"reviewer\""
    );
    assert_eq!(serde_json::to_string(&Scope::Accept).unwrap(), "\"accept\"");
    assert!(serde_json::from_str::<Role>("\"root\"").is_err());
}
