//! A polygon's rings on the sphere ([`super`]): which side of a ring is
//! its inside, whether a point is on a ring, and whether a point is inside.
//!
//! A ring's inside is the smaller of the two sides it bounds, as PostGIS's
//! `geography` reads a ring either way round (area 20, "Polygons are valid
//! when built"); its area is worked out once, in working values, to choose
//! the side, and the ring is listed with its inside on the left. Whether a
//! point is inside is then decided exactly, as S2 decides it: the inside
//! is known at a vertex from the angle there, and the path from that
//! vertex to the point crosses the ring an odd number of times exactly
//! when the point's side differs ([`super::vector::edge_or_vertex_crossing`]).

use super::vector::{
    Bounds, Point, angle_contains_vertex, edge_or_vertex_crossing, on_arc, same, side,
};
use crate::fixed::Real;

/// Where a point is against a ring or a polygon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Inside,
    /// On an edge, which counts as inside (area 20, "Inside includes the
    /// edge").
    Boundary,
    Outside,
}

/// One ring, its inside on the left of its points.
#[derive(Clone, Debug)]
pub struct Ring {
    /// Its corners, the last not repeating the first.
    pub points: Vec<Point>,
    /// The box around each edge, the edge from point `i` to the next.
    pub boxes: Vec<Bounds>,
}

impl Ring {
    /// A ring of `points`, its closing repeat left off, listed so its
    /// inside is on the left: as given when `left`, reversed otherwise.
    pub fn new(mut points: Vec<Point>, left: bool) -> Ring {
        if points.len() > 1 && points.first() == points.last() {
            points.pop();
        }
        if !left {
            points.reverse();
        }
        let boxes = (0..points.len())
            .filter_map(|i| {
                let (a, b) = edge_of(&points, i)?;
                Some(Bounds::arc(a, b))
            })
            .collect();
        Ring { points, boxes }
    }

    /// Edge `i`: from point `i` to the next, the last back to the first.
    pub fn edge(&self, i: usize) -> Option<(&Point, &Point)> {
        edge_of(&self.points, i)
    }

    /// Each edge with its box.
    pub fn edges(&self) -> impl Iterator<Item = (&Point, &Point, &Bounds)> {
        self.boxes
            .iter()
            .enumerate()
            .filter_map(|(i, bounds)| self.edge(i).map(|(a, b)| (a, b, bounds)))
    }

    /// The first edge `x` lies on, ends included.
    pub fn edge_holding(&self, x: &Point) -> Option<usize> {
        let at = Bounds::point(x);
        self.edges()
            .position(|(a, b, bounds)| bounds.overlaps(&at) && on_arc(a, b, x))
    }

    /// Whether `x`, which isn't on the ring, is inside it.
    pub fn inside(&self, x: &Point) -> bool {
        let n = self.points.len();
        if n < 3 {
            return false;
        }
        // From a corner that isn't opposite `x`, whose neighbours differ
        // from it (always, in a valid ring).
        let opposite = x.neg();
        let start = (0..n).find(|&k| {
            let (Some(here), Some(before), Some(after)) = (
                self.points.get(k),
                self.points.get((k + n - 1) % n),
                self.points.get((k + 1) % n),
            ) else {
                return false;
            };
            !same(here, &opposite) && here != before && here != after
        });
        let Some(k) = start else {
            return false;
        };
        let (Some(here), Some(before), Some(after)) = (
            self.points.get(k),
            self.points.get((k + n - 1) % n),
            self.points.get((k + 1) % n),
        ) else {
            return false;
        };
        let mut inside = angle_contains_vertex(before, here, after);
        let path = Bounds::arc(here, x);
        for (c, d, bounds) in self.edges() {
            if bounds.overlaps(&path) && edge_or_vertex_crossing(here, x, c, d) {
                inside = !inside;
            }
        }
        inside
    }

    /// Whether every corner lies on one great circle, so the ring bounds
    /// two halves of the globe exactly and neither is the smaller.
    pub fn on_one_great_circle(&self) -> bool {
        let Some(first) = self.points.first() else {
            return true;
        };
        let other = self
            .points
            .iter()
            .find(|p| !same(p, first) && !same(p, &first.neg()));
        let Some(other) = other else {
            return true;
        };
        self.points.iter().all(|p| side(first, other, p) == 0)
    }
}

/// Edge `i` of a ring's corners.
fn edge_of(points: &[Point], i: usize) -> Option<(&Point, &Point)> {
    let n = points.len();
    if n == 0 {
        return None;
    }
    Some((points.get(i)?, points.get((i + 1) % n)?))
}

/// Whether the side to the left of a ring, its corners in order (the
/// closing repeat left off or not), is the smaller of the two: its area,
/// by Van Oosterom and Strackee's formula for each triangle from a
/// reference point to an edge, summed and taken modulo the sphere's 4π.
/// `None` when the two sides come out exactly equal.
pub fn left_is_smaller(points: &[Point], units: &[[Real; 3]]) -> Option<bool> {
    let unit = super::vector::unit();
    let n = units.len();
    if n < 3 {
        return Some(true);
    }
    // A reference point opposite no corner: a corner, or else an axis.
    let axes = [
        [Real::zero(), Real::zero(), unit.int(1)],
        [unit.int(1), Real::zero(), Real::zero()],
        [Real::zero(), unit.int(1), Real::zero()],
    ];
    let opposite_none = |candidate: &[Real; 3]| {
        units.iter().all(|u| {
            !(u[0] == candidate[0].neg()
                && u[1] == candidate[1].neg()
                && u[2] == candidate[2].neg())
        })
    };
    let exact_opposite_none = |k: usize| {
        points
            .get(k)
            .is_some_and(|p| points.iter().all(|q| !same(q, &p.neg())))
    };
    let reference = (0..n.min(3))
        .find(|&k| exact_opposite_none(k))
        .and_then(|k| units.get(k).cloned())
        .or_else(|| axes.iter().find(|axis| opposite_none(axis)).cloned())?;
    let mul = |a: &Real, b: &Real| unit.mul(a, b);
    let dot = |a: &[Real; 3], b: &[Real; 3]| {
        mul(&a[0], &b[0])
            .add(&mul(&a[1], &b[1]))
            .add(&mul(&a[2], &b[2]))
    };
    let det = |a: &[Real; 3], b: &[Real; 3], c: &[Real; 3]| {
        let x = mul(&b[1], &c[2]).sub(&mul(&b[2], &c[1]));
        let y = mul(&b[2], &c[0]).sub(&mul(&b[0], &c[2]));
        let z = mul(&b[0], &c[1]).sub(&mul(&b[1], &c[0]));
        mul(&a[0], &x).add(&mul(&a[1], &y)).add(&mul(&a[2], &z))
    };
    let one = unit.int(1);
    let mut sum = Real::zero();
    for i in 0..n {
        let (Some(a), Some(b)) = (units.get(i), units.get((i + 1) % n)) else {
            continue;
        };
        let numerator = det(&reference, a, b);
        let denominator = one
            .add(&dot(&reference, a))
            .add(&dot(a, b))
            .add(&dot(b, &reference));
        sum = sum.add(&unit.atan2(&numerator, &denominator).times(2));
    }
    let whole = unit.pi().times(4);
    let half = unit.pi().times(2);
    while sum.is_negative() {
        sum = sum.add(&whole);
    }
    while sum >= whole {
        sum = sum.sub(&whole);
    }
    if sum == half { None } else { Some(sum < half) }
}

/// A polygon: its outer ring and its holes, each with its inside on the
/// left.
#[derive(Clone, Debug)]
pub struct Region {
    pub outer: Ring,
    pub holes: Vec<Ring>,
}

impl Region {
    /// Every ring, the outer first.
    pub fn rings(&self) -> impl Iterator<Item = &Ring> {
        std::iter::once(&self.outer).chain(self.holes.iter())
    }

    /// Where `x` is against the polygon: on a ring's edge is the boundary.
    pub fn place(&self, x: &Point) -> Place {
        if self.rings().any(|ring| ring.edge_holding(x).is_some()) {
            return Place::Boundary;
        }
        if !self.outer.inside(x) || self.holes.iter().any(|hole| hole.inside(x)) {
            Place::Outside
        } else {
            Place::Inside
        }
    }

    /// Whether the point an infinitesimal step from `m` towards `towards`
    /// is inside: `m` is a point inside an edge (never a corner) of any
    /// ring it lies on, and `towards` is at right angles to that edge, so
    /// on such a ring the step goes into its inside exactly when it points
    /// to the edge's left.
    pub fn beside(&self, m: &Point, towards: &super::vector::Vector) -> bool {
        let inside = |ring: &Ring| match ring.edge_holding(m).and_then(|i| ring.edge(i)) {
            Some((a, b)) => a.v.cross(&b.v).dot(towards).signum() > 0,
            None => ring.inside(m),
        };
        inside(&self.outer) && !self.holes.iter().any(inside)
    }
}
