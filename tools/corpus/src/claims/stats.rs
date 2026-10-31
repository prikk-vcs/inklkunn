//! Small statistics, each hand-checkable.

use std::collections::BTreeMap;

/// Round to four decimals (reports stay short and stable).
pub fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

/// `num / den`, rounded; `None` when there is nothing to divide by.
pub fn share(num: usize, den: usize) -> Option<f64> {
    (den > 0).then(|| round4(num as f64 / den as f64))
}

/// The arithmetic mean.
pub fn mean(xs: &[f64]) -> Option<f64> {
    (!xs.is_empty()).then(|| round4(xs.iter().sum::<f64>() / xs.len() as f64))
}

/// The median of an ascending list (the mean of the two middle values when the count is even).
pub fn median(sorted: &[f64]) -> Option<f64> {
    let n = sorted.len();
    if n == 0 {
        return None;
    }
    let at = |i: usize| sorted.get(i).copied().unwrap_or(0.0);
    Some(round4(if n % 2 == 1 {
        at(n / 2)
    } else {
        (at(n / 2 - 1) + at(n / 2)) / 2.0
    }))
}

/// The nearest-rank percentile of an ascending list: the value at rank `ceil(p/100 * n)`.
pub fn percentile(sorted: &[f64], p: usize) -> Option<f64> {
    let n = sorted.len();
    if n == 0 {
        return None;
    }
    let rank = (p * n).div_ceil(100).max(1);
    sorted.get(rank - 1).copied().map(round4)
}

/// The largest value.
pub fn max(xs: &[f64]) -> Option<f64> {
    xs.iter().copied().reduce(f64::max).map(round4)
}

/// A sorted copy.
pub fn sorted(xs: &[f64]) -> Vec<f64> {
    let mut v = xs.to_vec();
    v.sort_by(f64::total_cmp);
    v
}

/// Cohen's kappa for two raters over the same items: `(po - pe) / (1 - pe)`, where `po` is the share they
/// agree on and `pe` the share expected by chance from each rater's own label frequencies. When chance agreement
/// is already total (`pe = 1`, both raters used one label throughout) kappa is defined as 1. `None` with no items.
pub fn cohen_kappa<T: Ord + Clone>(pairs: &[(T, T)]) -> Option<f64> {
    let n = pairs.len();
    if n == 0 {
        return None;
    }
    let agree = pairs.iter().filter(|(a, b)| a == b).count();
    let mut first: BTreeMap<T, usize> = BTreeMap::new();
    let mut second: BTreeMap<T, usize> = BTreeMap::new();
    for (a, b) in pairs {
        *first.entry(a.clone()).or_default() += 1;
        *second.entry(b.clone()).or_default() += 1;
    }
    let nf = n as f64;
    let po = agree as f64 / nf;
    let pe: f64 = first
        .iter()
        .map(|(l, c)| (*c as f64 / nf) * (second.get(l).copied().unwrap_or(0) as f64 / nf))
        .sum();
    if (1.0 - pe).abs() < 1e-12 {
        return Some(1.0);
    }
    Some(round4((po - pe) / (1.0 - pe)))
}
