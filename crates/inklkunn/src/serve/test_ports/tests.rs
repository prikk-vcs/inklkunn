use super::two_free_ports;

#[test]
fn two_ports_taken_together_never_coincide() {
    for _ in 0..200 {
        let (a, b) = two_free_ports();
        assert_ne!(a, b);
    }
}
