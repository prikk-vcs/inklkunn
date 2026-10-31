use crate::claims::stats::{cohen_kappa, max, mean, median, percentile, share, sorted};

#[test]
fn the_simple_statistics_match_hand_arithmetic() {
    assert_eq!(share(1, 3), Some(0.3333));
    assert_eq!(share(0, 0), None);
    let xs = [4.0, 1.0, 3.0, 2.0];
    assert_eq!(mean(&xs), Some(2.5));
    assert_eq!(
        median(&sorted(&xs)),
        Some(2.5),
        "even count: the two middle values"
    );
    assert_eq!(median(&[1.0, 5.0, 9.0]), Some(5.0));
    assert_eq!(max(&xs), Some(4.0));
    assert_eq!(mean(&[]), None);
    assert_eq!(median(&[]), None);
    assert_eq!(max(&[]), None);
}

#[test]
fn the_percentile_is_the_nearest_rank() {
    let ten: Vec<f64> = (1..=10).map(f64::from).collect();
    assert_eq!(percentile(&ten, 90), Some(9.0), "rank ceil(0.9 * 10) = 9");
    assert_eq!(percentile(&ten, 100), Some(10.0));
    assert_eq!(percentile(&ten, 50), Some(5.0));
    assert_eq!(percentile(&[7.0], 90), Some(7.0));
    assert_eq!(percentile(&[1.0, 3.0], 90), Some(3.0), "rank ceil(1.8) = 2");
    assert_eq!(percentile(&[], 90), None);
}

#[test]
fn kappa_of_a_two_by_two_table_by_hand() {
    // 20 items. Both say yes: 10; both no: 5; first yes / second no: 3; first no / second yes: 2.
    // po = 15/20 = 0.75; first yes 13/20, second yes 12/20; pe = .65*.60 + .35*.40 = .39 + .14 = .53;
    // kappa = (.75 - .53) / (1 - .53) = .22 / .47 = 0.4681.
    let mut pairs = Vec::new();
    pairs.extend(std::iter::repeat_n(("yes", "yes"), 10));
    pairs.extend(std::iter::repeat_n(("no", "no"), 5));
    pairs.extend(std::iter::repeat_n(("yes", "no"), 3));
    pairs.extend(std::iter::repeat_n(("no", "yes"), 2));
    assert_eq!(cohen_kappa(&pairs), Some(0.4681));
}

#[test]
fn kappa_at_the_edges() {
    assert_eq!(
        cohen_kappa(&[("a", "a"), ("b", "b")]),
        Some(1.0),
        "perfect agreement"
    );
    assert_eq!(
        cohen_kappa(&[("a", "b"), ("b", "a")]),
        Some(-1.0),
        "perfect disagreement"
    );
    assert_eq!(
        cohen_kappa(&[("a", "a"), ("a", "a")]),
        Some(1.0),
        "one label throughout: defined as 1"
    );
    assert_eq!(cohen_kappa::<&str>(&[]), None);
    // Agreement exactly at chance is 0: a splits 1/2, b splits 1/2, agree on half.
    assert_eq!(
        cohen_kappa(&[("a", "a"), ("a", "b"), ("b", "a"), ("b", "b")]),
        Some(0.0)
    );
}
