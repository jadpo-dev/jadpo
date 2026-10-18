//! Whether one shape covers another, and whether two share a point
//! ([`super`]), for any mix of points, lines and polygons.
//!
//! A shape is a set: its points, the edges of its lines, and its polygons
//! with their insides, edges included. `b` is covered by `a` when no point
//! of `b` is outside `a`. Each edge of `b` is cut wherever an edge of `a`
//! meets it, and a point inside each piece is tested, which is all of the
//! piece, since nothing changes along a piece that no edge of `a` meets.
//! For a polygon of `b`, every edge of either shape's polygons is cut
//! where the others meet it, and on each piece the step to either side is
//! tested: if a step to a side is inside `b` and outside `a`, a part of `b`
//! is uncovered, and any uncovered part of `b` is bordered by some such
//! piece. Two shapes share a point when an edge of one meets an edge of
//! the other, or else when one corner of a part of either is in the
//! other, since a part no edge meets is wholly in or wholly out.

use std::cmp::Ordering;

use super::ring::{Place, Region, Ring, left_is_smaller};
use super::vector::{Bounds, Point, Vector, along, arcs_meet, from_degrees, meeting, on_arc, same};
use crate::Num;

/// A position, its latitude then its longitude, in degrees as held.
pub type Position = (Num, Num);

/// A polygon's rings, each closed (its last point its first).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Area {
    pub outer: Vec<Position>,
    pub holes: Vec<Vec<Position>>,
}

/// Any geometry, or several together: its points, its lines and its
/// polygons.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shape {
    pub points: Vec<Position>,
    pub lines: Vec<Vec<Position>>,
    pub areas: Vec<Area>,
}

/// An edge and its box.
#[derive(Clone, Debug)]
struct Edge {
    a: Point,
    b: Point,
    bounds: Bounds,
}

impl Edge {
    fn new(a: &Point, b: &Point) -> Edge {
        Edge {
            a: a.clone(),
            b: b.clone(),
            bounds: Bounds::arc(a, b),
        }
    }
}

/// A shape's points as directions, its polygons' rings oriented.
#[derive(Clone, Debug, Default)]
pub(super) struct Built {
    points: Vec<Point>,
    lines: Vec<Vec<Point>>,
    regions: Vec<Region>,
}

/// A position as a point.
pub(super) fn point(position: &Position) -> Point {
    from_degrees(position.0, position.1).0
}

/// A ring's points, its inside on the left, the smaller side.
pub(super) fn ring(positions: &[Position]) -> Ring {
    let (points, units): (Vec<Point>, Vec<_>) = positions
        .iter()
        .map(|(latitude, longitude)| from_degrees(*latitude, *longitude))
        .unzip();
    let left = left_is_smaller(&points, &units).unwrap_or(true);
    Ring::new(points, left)
}

/// A polygon's rings, oriented.
pub(super) fn region(area: &Area) -> Region {
    Region {
        outer: ring(&area.outer),
        holes: area.holes.iter().map(|hole| ring(hole)).collect(),
    }
}

impl Built {
    pub(super) fn new(shape: &Shape) -> Built {
        Built {
            points: shape.points.iter().map(point).collect(),
            lines: shape
                .lines
                .iter()
                .map(|line| line.iter().map(point).collect())
                .collect(),
            regions: shape.areas.iter().map(region).collect(),
        }
    }

    /// The edges of its lines.
    fn line_edges(&self) -> Vec<Edge> {
        self.lines
            .iter()
            .flat_map(|line| line.windows(2))
            .filter_map(|pair| match pair {
                [a, b] => Some(Edge::new(a, b)),
                _ => None,
            })
            .collect()
    }

    /// The edges of its polygons' rings.
    fn ring_edges(&self) -> Vec<Edge> {
        self.regions
            .iter()
            .flat_map(Region::rings)
            .flat_map(|ring| {
                ring.edges().map(|(a, b, bounds)| Edge {
                    a: a.clone(),
                    b: b.clone(),
                    bounds: *bounds,
                })
            })
            .collect()
    }

    /// Whether `x` is one of its points, on one of its lines, or in or on
    /// one of its polygons.
    fn covers_point(&self, x: &Point) -> bool {
        let at = Bounds::point(x);
        self.points.iter().any(|p| same(p, x))
            || self.lines.iter().any(|line| {
                line.windows(2).any(|pair| match pair {
                    [a, b] => Bounds::arc(a, b).overlaps(&at) && on_arc(a, b, x),
                    _ => false,
                })
            })
            || self
                .regions
                .iter()
                .any(|region| region.place(x) != Place::Outside)
    }

    /// Whether the step from `m` towards `towards` is inside one of its
    /// polygons.
    fn beside(&self, m: &Point, towards: &Vector) -> bool {
        self.regions.iter().any(|region| region.beside(m, towards))
    }

    /// One corner of each part: each point, and the first corner of each
    /// line and each ring.
    fn corners(&self) -> Vec<Point> {
        let mut corners = self.points.clone();
        corners.extend(self.lines.iter().filter_map(|line| line.first().cloned()));
        corners.extend(
            self.regions
                .iter()
                .flat_map(Region::rings)
                .filter_map(|ring| ring.points.first().cloned()),
        );
        corners
    }
}

/// The edge from `a` to `b` cut wherever one of `cutters` meets it: its
/// ends and every meeting point, in order along it, each once.
fn pieces(a: &Point, b: &Point, cutters: &[Edge]) -> Vec<Point> {
    let bounds = Bounds::arc(a, b);
    let mut cuts = vec![a.clone(), b.clone()];
    for cutter in cutters {
        if cutter.bounds.overlaps(&bounds) {
            cuts.extend(meeting(a, b, &cutter.a, &cutter.b));
        }
    }
    let pole = a.v.cross(&b.v);
    cuts.sort_by(|x, y| along(&pole, x, y));
    cuts.dedup_by(|x, y| along(&pole, x, y) == Ordering::Equal);
    cuts
}

/// Whether no point of `b` lies outside `a`.
pub fn covers(a: &Shape, b: &Shape) -> bool {
    let a = Built::new(a);
    let b = Built::new(b);
    if !b.points.iter().all(|p| a.covers_point(p)) {
        return false;
    }
    let mut cutters = a.line_edges();
    let a_rings = a.ring_edges();
    cutters.extend(a_rings.iter().cloned());
    for line in &b.lines {
        for pair in line.windows(2) {
            let [p, q] = pair else { continue };
            if same(p, q) {
                if !a.covers_point(p) {
                    return false;
                }
                continue;
            }
            let cuts = pieces(p, q, &cutters);
            for piece in cuts.windows(2) {
                let [x, y] = piece else { continue };
                if !a.covers_point(&x.between(y)) {
                    return false;
                }
            }
        }
        if let [only] = line.as_slice()
            && !a.covers_point(only)
        {
            return false;
        }
    }
    if b.regions.is_empty() {
        return true;
    }
    if a.regions.is_empty() {
        return false;
    }
    let b_rings = b.ring_edges();
    let mut borders = a_rings;
    borders.extend(b_rings);
    for edge in &borders {
        if same(&edge.a, &edge.b) {
            continue;
        }
        let pole = edge.a.v.cross(&edge.b.v);
        let away = pole.neg();
        let cuts = pieces(&edge.a, &edge.b, &borders);
        for piece in cuts.windows(2) {
            let [x, y] = piece else { continue };
            let m = x.between(y);
            for towards in [&pole, &away] {
                if b.beside(&m, towards) && !a.beside(&m, towards) {
                    return false;
                }
            }
        }
    }
    true
}

/// Whether the two shapes share at least one point, edges included.
pub fn intersects(a: &Shape, b: &Shape) -> bool {
    let a = Built::new(a);
    let b = Built::new(b);
    let mut a_edges = a.line_edges();
    a_edges.extend(a.ring_edges());
    let mut b_edges = b.line_edges();
    b_edges.extend(b.ring_edges());
    let met = super::sweep::any_pair(
        &a_edges.iter().map(|e| e.bounds).collect::<Vec<_>>(),
        &b_edges.iter().map(|e| e.bounds).collect::<Vec<_>>(),
        |i, j| match (a_edges.get(i), b_edges.get(j)) {
            (Some(e), Some(f)) => arcs_meet(&e.a, &e.b, &f.a, &f.b),
            _ => false,
        },
    );
    if met {
        return true;
    }
    // No edges meet: each part is wholly in or wholly out of the other, so
    // one corner of it says which, and each point is its own part.
    b.corners().iter().any(|p| a.covers_point(p)) || a.corners().iter().any(|p| b.covers_point(p))
}
