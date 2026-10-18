//! Finding the pairs of edges whose boxes overlap without trying every
//! pair ([`super`]): the boxes sorted by their least `x`, and each compared
//! only with those that start before it ends, so a ring of `n` short edges
//! costs about `n log n`, the "sweep over the edges" area 20 estimates for
//! checking a polygon.

use super::vector::Bounds;

/// The order of `boxes` by their least `x`.
fn by_start(boxes: &[Bounds]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by_key(|&i| boxes.get(i).map_or(i64::MAX, |b| b.low[0]));
    order
}

/// Calls `visit(i, j)` for each pair `i < j` of `boxes` that overlap, in
/// a fixed order, until it gives `true`; whether one did.
pub fn any_overlapping(boxes: &[Bounds], mut visit: impl FnMut(usize, usize) -> bool) -> bool {
    let order = by_start(boxes);
    for (at, &i) in order.iter().enumerate() {
        let Some(first) = boxes.get(i) else { continue };
        for &j in order.iter().skip(at + 1) {
            let Some(second) = boxes.get(j) else { continue };
            if second.low[0] > first.high[0] {
                break;
            }
            if first.overlaps(second) && visit(i.min(j), i.max(j)) {
                return true;
            }
        }
    }
    false
}

/// Whether `test(i, j)` holds for some box `i` of `a` and box `j` of `b`
/// that overlap; only those are tried.
pub fn any_pair(a: &[Bounds], b: &[Bounds], mut test: impl FnMut(usize, usize) -> bool) -> bool {
    let mut all: Vec<Bounds> = a.to_vec();
    all.extend_from_slice(b);
    let split = a.len();
    any_overlapping(&all, |i, j| i < split && j >= split && test(i, j - split))
}
