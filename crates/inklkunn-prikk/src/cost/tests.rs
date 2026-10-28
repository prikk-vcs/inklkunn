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

#[test]
fn a_hint_is_returned_until_forgotten_and_never_for_another_repository() {
    let hints = SizeHints::new();
    let a = Path::new("/srv/wikis/a");
    let b = Path::new("/srv/wikis/b");
    assert_eq!(hints.get(a), None, "nothing is hinted until it is set");
    hints.set(a, 4096);
    assert_eq!(hints.get(a), Some(4096));
    assert_eq!(hints.get(b), None);
    // A clone is another handle on the same figures.
    let clone = hints.clone();
    clone.set(b, 7);
    assert_eq!(hints.get(b), Some(7));
    // A new figure replaces the old; forgetting drops it, for the repository named and no other.
    hints.set(a, 9000);
    assert_eq!(clone.get(a), Some(9000));
    clone.forget(a);
    assert_eq!(hints.get(a), None);
    assert_eq!(hints.get(b), Some(7));
}

#[test]
fn a_hint_older_than_its_maximum_age_is_not_trusted() {
    let hints = SizeHints::with_max_age(Duration::from_millis(20));
    let repo = Path::new("/srv/wikis/old");
    hints.set(repo, 1);
    assert_eq!(hints.get(repo), Some(1));
    std::thread::sleep(Duration::from_millis(80));
    assert_eq!(hints.get(repo), None, "the walk is the fallback again");
    // Setting it again makes it fresh.
    hints.set(repo, 2);
    assert_eq!(hints.get(repo), Some(2));
    assert_eq!(
        SizeHints::default().get(repo),
        None,
        "a default set is empty, and its maximum age is the documented one"
    );
    assert_eq!(SizeHints::DEFAULT_MAX_AGE, Duration::from_secs(15 * 60));
}
