use super::*;
use crate::cost::{COST_FLOOR, LIGHT_CLASS_MAX_COST, MIB, ceiling_for};

#[test]
fn pools_are_split_and_refusals_are_typed() {
    let gate = Admission::new(400 * MIB, 64 * MIB, Duration::from_millis(10));
    assert_eq!(gate.light_pool(), 100 * MIB);
    assert_eq!(gate.heavy_pool(), 300 * MIB);
    assert!(matches!(gate.admit(301 * MIB), Err(Refusal::TooLarge { pool }) if pool == 300 * MIB));
    let light = ceiling_for(COST_FLOOR);
    let held = gate.admit(light);
    assert!(held.is_ok());
    // The light pool holds 100 MiB; four light ceilings (24 MiB each) fit, a fifth must wait and is then busy.
    let mut permits = vec![held];
    for _ in 0..3 {
        permits.push(gate.admit(light));
    }
    assert!(permits.iter().all(Result::is_ok));
    assert_eq!(gate.admit(light).err(), Some(Refusal::Busy));
    drop(permits);
    assert!(gate.admit(light).is_ok(), "bytes return on drop");
}

#[test]
fn heavy_reads_do_not_block_light_ones() {
    let gate = Admission::new(400 * MIB, 64 * MIB, Duration::from_millis(10));
    let heavy = gate.admit(300 * MIB);
    assert!(heavy.is_ok());
    assert!(
        gate.admit(ceiling_for(COST_FLOOR)).is_ok(),
        "the light pool is separate"
    );
}

#[test]
fn a_light_read_always_fits_the_light_pool_when_the_budget_allows() {
    // A small budget with no minimum: a quarter would be 16 MiB, below one light-class ceiling (48 MiB).
    let gate = Admission::new(128 * MIB, 0, Duration::from_millis(10));
    assert_eq!(gate.light_pool(), ceiling_for(LIGHT_CLASS_MAX_COST));
    assert!(gate.admit(ceiling_for(LIGHT_CLASS_MAX_COST)).is_ok());
    // A budget too small for that keeps the quarter rule, honestly.
    let tiny = Admission::new(64 * MIB, 0, Duration::from_millis(10));
    assert_eq!(tiny.light_pool(), 16 * MIB);
}
