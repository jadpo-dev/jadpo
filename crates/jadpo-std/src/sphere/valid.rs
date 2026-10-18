//! Whether a polygon's rings are valid on the sphere (area 20, "Polygons
//! are valid when built"): the check behind the rule `rings_valid` and
//! behind refusing a polygon read from GeoJSON ([`super`]).
//!
//! A polygon is valid when each ring is closed with four points or more;
//! no edge joins a point to itself or to the point opposite it, so every
//! edge is shorter than half the globe and follows the one great circle
//! through its ends; no ring crosses or touches itself, except where its
//! last point meets its first, and none doubles back on itself; no two
//! rings share any point; no ring lies on one great circle, so its two
//! sides differ and the polygon is the smaller, under half the globe; each
//! hole lies inside the outer ring; and no hole lies inside another.

use std::cell::Cell;

use super::ring::Ring;
use super::shape::{Position, ring};
use super::sweep::any_overlapping;
use super::vector::{Bounds, Point, arcs_meet, same, side};

/// Which ring of a polygon breaks `rings_valid`: 0 for its outer ring, `n`
/// for its `n`th hole, the lowest when several do; `None` when it is
/// valid.
pub fn rings_broken(outer: &[Position], holes: &[Vec<Position>]) -> Option<usize> {
    let mut given: Vec<&[Position]> = vec![outer];
    given.extend(holes.iter().map(Vec::as_slice));
    let broken: Cell<Option<usize>> = Cell::new(None);
    let note = |ring: usize| {
        broken.set(Some(broken.get().map_or(ring, |was| was.min(ring))));
    };
    let mut rings: Vec<Ring> = Vec::with_capacity(given.len());
    for (k, positions) in given.iter().enumerate() {
        let closed = match (positions.first(), positions.last()) {
            (Some(first), Some(last)) => {
                positions.len() >= 4
                    && same(&super::shape::point(first), &super::shape::point(last))
            }
            _ => false,
        };
        if !closed {
            note(k);
            return broken.get();
        }
        let built = ring(positions);
        if !simple_edges(&built) || built.on_one_great_circle() {
            note(k);
        }
        rings.push(built);
    }
    if broken.get().is_some() {
        return broken.get();
    }
    // Every edge of every ring, swept for pairs that meet: within a ring,
    // two edges that aren't neighbours; across rings, any two.
    let mut edges: Vec<(usize, usize, &Point, &Point)> = Vec::new();
    let mut boxes: Vec<Bounds> = Vec::new();
    for (k, ring) in rings.iter().enumerate() {
        for (i, (a, b, bounds)) in ring.edges().enumerate() {
            edges.push((k, i, a, b));
            boxes.push(*bounds);
        }
    }
    any_overlapping(&boxes, |i, j| {
        let (Some(&(k1, i1, a1, b1)), Some(&(k2, i2, a2, b2))) = (edges.get(i), edges.get(j))
        else {
            return false;
        };
        if k1 == k2 {
            let n = rings.get(k1).map_or(0, |ring| ring.points.len());
            let neighbours = (i1 + 1) % n == i2 || (i2 + 1) % n == i1;
            if !neighbours && arcs_meet(a1, b1, a2, b2) {
                note(k1);
            }
        } else if arcs_meet(a1, b1, a2, b2) {
            note(k1.max(k2));
        }
        false
    });
    if broken.get().is_some() {
        return broken.get();
    }
    // No ring meets another, so one corner says where a whole ring is.
    let Some((outer, holes)) = rings.split_first() else {
        return Some(0);
    };
    for (h, hole) in holes.iter().enumerate() {
        let Some(first_point) = hole.points.first() else {
            note(h + 1);
            continue;
        };
        if !outer.inside(first_point) {
            note(h + 1);
        }
        for (other, around) in holes.iter().enumerate() {
            if other != h && around.inside(first_point) {
                note(h.max(other) + 1);
            }
        }
    }
    broken.get()
}

/// Whether a ring's edges are each a proper edge and it never doubles
/// back: no edge from a point to itself or to its opposite, and no corner
/// where the ring turns straight back along the edge it came by.
fn simple_edges(ring: &Ring) -> bool {
    let n = ring.points.len();
    if n < 3 {
        return false;
    }
    for i in 0..n {
        let (Some(a), Some(b), Some(c)) = (
            ring.points.get(i),
            ring.points.get((i + 1) % n),
            ring.points.get((i + 2) % n),
        ) else {
            return false;
        };
        if same(a, b) || same(a, &b.neg()) {
            return false;
        }
        // At `b`, the edge to `c` on the great circle from `a`, turning
        // back the way it came.
        if side(a, b, c) == 0 && a.v.cross(&b.v).dot(&b.v.cross(&c.v)).signum() < 0 {
            return false;
        }
        if same(a, c) {
            return false;
        }
    }
    true
}
