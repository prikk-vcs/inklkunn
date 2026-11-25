use super::two_free_ports;

#[test]
fn two_ports_taken_together_never_coincide() {
    for _ in 0..200 {
        let (a, b) = two_free_ports();
        assert_ne!(a, b);
    }
}

#[test]
fn a_port_is_never_handed_out_twice_by_one_process() {
    // 1,500 pairs are 3,000 of the ~28,000 ports the system lends: without the memory, about a hundred would repeat.
    let mut seen = std::collections::HashSet::new();
    for _ in 0..1_500 {
        let (a, b) = two_free_ports();
        assert!(seen.insert(a), "{a} was handed out twice");
        assert!(seen.insert(b), "{b} was handed out twice");
    }
}
