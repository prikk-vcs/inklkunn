use super::*;
use crate::cost::{COST_FLOOR, MIB};

#[test]
fn pools_are_split_and_refusals_are_typed() {
    let gate = Admission::new(400 * MIB, 64 * MIB, Duration::from_millis(10));
    assert_eq!(gate.light_pool(), 100 * MIB);
    assert_eq!(gate.heavy_pool(), 300 * MIB);
    assert!(matches!(gate.admit(301 * MIB), Err(Refusal::TooLarge { pool }) if pool == 300 * MIB));
    let held = gate.admit(COST_FLOOR);
    assert!(held.is_ok());
    // The light pool holds 100 MiB; six floors fit, a seventh must wait and is then busy.
    let mut permits = vec![held];
    for _ in 0..5 {
        permits.push(gate.admit(COST_FLOOR));
    }
    assert!(permits.iter().all(Result::is_ok));
    assert_eq!(gate.admit(COST_FLOOR).err(), Some(Refusal::Busy));
    drop(permits);
    assert!(gate.admit(COST_FLOOR).is_ok(), "bytes return on drop");
}

#[test]
fn heavy_reads_do_not_block_light_ones() {
    let gate = Admission::new(400 * MIB, 64 * MIB, Duration::from_millis(10));
    let heavy = gate.admit(300 * MIB);
    assert!(heavy.is_ok());
    assert!(gate.admit(COST_FLOOR).is_ok(), "the light pool is separate");
}
