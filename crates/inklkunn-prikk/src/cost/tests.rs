use super::*;

#[test]
fn the_table_is_bound_to_the_floor() {
    let floor = crate::MIN_PRIKK_VERSION;
    let mut parts = floor
        .split('.')
        .map(|p| p.parse::<u64>().unwrap_or(u64::MAX));
    let parsed = (
        parts.next().unwrap_or(u64::MAX),
        parts.next().unwrap_or(u64::MAX),
        parts.next().unwrap_or(u64::MAX),
    );
    assert_eq!(
        BOUND_TO, parsed,
        "re-measure the cost table when the floor moves (RFC 010 re-baseline)"
    );
}

#[test]
fn costs_have_floors_and_multiples() {
    assert_eq!(ReadVerb::Branch.cost(1 << 30), COST_FLOOR);
    assert_eq!(ReadVerb::Tree.cost(100 * MIB), 210 * MIB);
    assert_eq!(ReadVerb::Show.cost(0), SHOW_FLOOR);
    assert_eq!(ReadVerb::Verify.cost(10 * MIB), 41 * MIB);
    assert_eq!(ReadVerb::Diff.cost(100 * MIB), 500 * MIB);
    assert!(ReadVerb::is_light(COST_FLOOR));
    assert!(!ReadVerb::is_light(3 * COST_FLOOR));
    for v in ReadVerb::ALL {
        assert!(v.cost(0) >= COST_FLOOR, "{}", v.name());
    }
}

#[test]
fn ceilings_carry_real_headroom() {
    assert_eq!(ceiling_for(100), 150);
    assert_eq!(ceiling_for(COST_FLOOR), COST_FLOOR + COST_FLOOR / 2);
    assert!(ceiling_for(u64::MAX) == u64::MAX, "saturates");
    for v in ReadVerb::ALL {
        let c = v.cost(100 * MIB);
        assert!(
            ceiling_for(c) > c,
            "{}: the ceiling is above the cost",
            v.name()
        );
    }
}
