//! A point on the sphere as an exact direction, and the predicates on
//! points every great-circle test is built from ([`super`]).
//!
//! A position's direction is `(cos φ cos λ, cos φ sin λ, sin φ)` with each
//! sine and cosine a working value of the fixed module, worked out in
//! degrees so a quarter turn is exact ([`crate::fixed::Unit::sin_cos_degrees`]),
//! and each product kept whole, not cut: the components are whole numbers
//! over `2^272`. Points on one meridian are then exactly in one plane with
//! the poles, points on the equator exactly in the plane `z = 0`, a pole is
//! `(0, 0, ±1)` whatever its longitude, and antipodes are exact negations,
//! so the places where a test is decided by a point lying on an edge are
//! decided exactly. Every predicate is a sign of a polynomial in the
//! components, worked out in whole numbers as wide as it needs
//! ([`super::wide`]), so it is never wrong and the same on every target.
//! A 40-bit unit vector beside each point settles most signs at once; the
//! whole numbers decide what it can't.
//!
//! Where three points are exactly on one great circle, [`sign`] breaks the
//! tie by Simulation of Simplicity (H. Edelsbrunner and E. P. Mücke, 1990),
//! as Google's S2 library does (`s2predicates.cc`, Apache 2.0), so that
//! counting the edges a path crosses is consistent; the tests that a point
//! lies on an edge ([`on_arc`]) or that two edges meet ([`arcs_meet`]) are
//! exact and closed, with no tie broken.

use std::cmp::Ordering;
use std::sync::OnceLock;

use super::wide::Wide;
use crate::Num;
use crate::fixed::{Real, Unit};

/// The working constants, worked out once.
pub(super) fn unit() -> &'static Unit {
    static UNIT: OnceLock<Unit> = OnceLock::new();
    UNIT.get_or_init(Unit::new)
}

/// The bits a point's approximate unit vector keeps after the point.
const APPROX: u32 = 40;

/// How far an approximate coordinate may be from the exact direction's, in
/// units of `2^-40`, with room: the normalising below cuts three times.
const SLACK: i64 = 4;

/// A determinant of three approximate vectors further than this from 0
/// has the exact one's sign: each coordinate's error is at most [`SLACK`]
/// in `2^40`, so the determinant's is under `3 × 7 × 2^80 < 2^86`.
const DET_TRUSTED: i128 = 1 << 86;

/// A direction from the centre of the sphere, its components exact whole
/// numbers; any positive multiple is the same point, and every predicate
/// below is unchanged by one.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Vector {
    pub x: Wide,
    pub y: Wide,
    pub z: Wide,
}

impl Vector {
    pub fn cross(&self, other: &Vector) -> Vector {
        Vector {
            x: self.y.mul(&other.z).sub(&self.z.mul(&other.y)),
            y: self.z.mul(&other.x).sub(&self.x.mul(&other.z)),
            z: self.x.mul(&other.y).sub(&self.y.mul(&other.x)),
        }
    }

    pub fn dot(&self, other: &Vector) -> Wide {
        self.x
            .mul(&other.x)
            .add(&self.y.mul(&other.y))
            .add(&self.z.mul(&other.z))
    }

    pub fn neg(&self) -> Vector {
        Vector {
            x: self.x.neg(),
            y: self.y.neg(),
            z: self.z.neg(),
        }
    }

    pub fn add(&self, other: &Vector) -> Vector {
        Vector {
            x: self.x.add(&other.x),
            y: self.y.add(&other.y),
            z: self.z.add(&other.z),
        }
    }

    pub fn is_zero(&self) -> bool {
        self.x.is_zero() && self.y.is_zero() && self.z.is_zero()
    }

    /// The direction as a unit vector in whole numbers over `2^40`, each
    /// coordinate within [`SLACK`] of the exact one: the components cut to
    /// their top 62 bits together, then divided by their length, a whole
    /// square root.
    fn approximate(&self) -> [i64; 3] {
        let top = self.x.bits().max(self.y.bits()).max(self.z.bits());
        let shift = top.saturating_sub(62);
        let cut = |w: &Wide| -> i128 {
            let size = i128::from(w.bits_from(shift) & (u64::MAX >> 2));
            if w.signum() < 0 { -size } else { size }
        };
        let (x, y, z) = (cut(&self.x), cut(&self.y), cut(&self.z));
        let square = (x * x) as u128 + (y * y) as u128 + (z * z) as u128;
        let length = square.isqrt() as i128;
        if length == 0 {
            return [0; 3];
        }
        let scaled = |c: i128| ((c << APPROX) / length) as i64;
        [scaled(x), scaled(y), scaled(z)]
    }
}

/// A point: its exact direction, and the approximate unit vector that
/// settles most signs and draws the boxes edges are sorted by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Point {
    pub v: Vector,
    pub a: [i64; 3],
}

impl Point {
    pub fn new(v: Vector) -> Point {
        let a = v.approximate();
        Point { v, a }
    }

    pub fn neg(&self) -> Point {
        Point {
            v: self.v.neg(),
            a: [-self.a[0], -self.a[1], -self.a[2]],
        }
    }

    /// A point strictly between two points that aren't opposite, on the
    /// shorter arc joining them: their sum, which needs no square root.
    pub fn between(&self, other: &Point) -> Point {
        Point::new(self.v.add(&other.v))
    }
}

/// A position's latitude and longitude in degrees, as a coordinate is
/// held, as the point's exact direction, and as a unit vector of working
/// values for the measures that need no exactness (a ring's area, a
/// centroid).
pub fn from_degrees(latitude: Num, longitude: Num) -> (Point, [Real; 3]) {
    let unit = unit();
    let (sin_lat, cos_lat) = unit.sin_cos_degrees(latitude);
    let (sin_lon, cos_lon) = unit.sin_cos_degrees(longitude);
    let whole = |r: &Real| {
        let (negative, limbs) = r.parts();
        Wide::from_parts(negative, &limbs)
    };
    let (k, s, c, z) = (
        whole(&cos_lat),
        whole(&sin_lon),
        whole(&cos_lon),
        whole(&sin_lat),
    );
    let v = Vector {
        x: k.mul(&c),
        y: k.mul(&s),
        z: z.shl(crate::fixed::FRACTION),
    };
    let real = [
        unit.mul(&cos_lat, &cos_lon),
        unit.mul(&cos_lat, &sin_lon),
        sin_lat,
    ];
    (Point::new(v), real)
}

/// A position's unit vector in working values alone, for the measures
/// that need no exactness.
pub fn unit_vector(latitude: Num, longitude: Num) -> [Real; 3] {
    let unit = unit();
    let (sin_lat, cos_lat) = unit.sin_cos_degrees(latitude);
    let (sin_lon, cos_lon) = unit.sin_cos_degrees(longitude);
    [
        unit.mul(&cos_lat, &cos_lon),
        unit.mul(&cos_lat, &sin_lon),
        sin_lat,
    ]
}

/// The determinant of three approximate vectors, in whole numbers over
/// `2^120`.
fn approximate_det(a: &[i64; 3], b: &[i64; 3], c: &[i64; 3]) -> i128 {
    let w = |n: i64| i128::from(n);
    w(a[0]) * (w(b[1]) * w(c[2]) - w(b[2]) * w(c[1]))
        - w(a[1]) * (w(b[0]) * w(c[2]) - w(b[2]) * w(c[0]))
        + w(a[2]) * (w(b[0]) * w(c[1]) - w(b[1]) * w(c[0]))
}

/// The sign of `det(a, b, c)`, the side of the great circle from `a` to
/// `b` that `c` is on: 1 to the left, -1 to the right, 0 on it. Exact.
pub fn side(a: &Point, b: &Point, c: &Point) -> i32 {
    let near = approximate_det(&a.a, &b.a, &c.a);
    if near > DET_TRUSTED {
        return 1;
    }
    if near < -DET_TRUSTED {
        return -1;
    }
    a.v.dot(&b.v.cross(&c.v)).signum()
}

/// Whether two points are the same place: the same direction.
pub fn same(a: &Point, b: &Point) -> bool {
    a == b || (a.v.cross(&b.v).is_zero() && a.v.dot(&b.v).signum() > 0)
}

/// Whether `x` lies on the shorter arc from `a` to `b`, ends included:
/// on its great circle, at or past `a` towards `b`, and at or before `b`.
/// Exact. An edge from a point to itself is that point.
pub fn on_arc(a: &Point, b: &Point, x: &Point) -> bool {
    if same(a, b) {
        return same(a, x);
    }
    if side(a, b, x) != 0 {
        return false;
    }
    let normal = a.v.cross(&b.v);
    a.v.cross(&x.v).dot(&normal).signum() >= 0 && x.v.cross(&b.v).dot(&normal).signum() >= 0
}

/// Whether the highest point of the great circle through `a` and `b`
/// lies on the shorter arc between them, and whether the lowest does,
/// ends included. With `n = a × b`, the highest point is `t = z(n·n) −
/// n(n·z)`, so `t·a = (n·n) a_z` and `t·b = (n·n) b_z`, and `t` is past `a`
/// towards `b`, `(a × t)·n ≥ 0`, exactly when `(a·a) b_z ≥ (a·b) a_z`, and
/// before `b` when `(b·b) a_z ≥ (a·b) b_z`; the lowest point is `-t`.
pub fn extremes_on_arc(a: &Point, b: &Point) -> (bool, bool) {
    let ab = a.v.dot(&b.v);
    let past_a = a.v.dot(&a.v).mul(&b.v.z).sub(&ab.mul(&a.v.z)).signum();
    let before_b = b.v.dot(&b.v).mul(&a.v.z).sub(&ab.mul(&b.v.z)).signum();
    (past_a >= 0 && before_b >= 0, past_a <= 0 && before_b <= 0)
}

/// Where the edges `a`–`b` and `c`–`d` meet, as points on `a`–`b`, ends
/// included: none, the one point where their great circles cross when
/// both edges reach it, or, for two edges on one great circle, the ends of
/// `c`–`d` that lie on `a`–`b` (with `a` and `b` when `c`–`d` covers
/// them, which the caller's own ends already are).
pub fn meeting(a: &Point, b: &Point, c: &Point, d: &Point) -> Vec<Point> {
    if same(c, d) {
        return if on_arc(a, b, c) {
            vec![c.clone()]
        } else {
            Vec::new()
        };
    }
    if same(a, b) {
        return if on_arc(c, d, a) {
            vec![a.clone()]
        } else {
            Vec::new()
        };
    }
    let (sc, sd) = (side(a, b, c), side(a, b, d));
    if sc == sd && sc != 0 {
        return Vec::new();
    }
    let (sa, sb) = (side(c, d, a), side(c, d, b));
    if sa == sb && sa != 0 {
        return Vec::new();
    }
    let crossing = a.v.cross(&b.v).cross(&c.v.cross(&d.v));
    if crossing.is_zero() {
        return [c, d]
            .into_iter()
            .filter(|end| on_arc(a, b, end))
            .cloned()
            .collect();
    }
    let crossing = Point::new(crossing);
    for candidate in [crossing.clone(), crossing.neg()] {
        if on_arc(a, b, &candidate) && on_arc(c, d, &candidate) {
            return vec![candidate];
        }
    }
    Vec::new()
}

/// Whether the edges `a`–`b` and `c`–`d` share a point, ends included.
pub fn arcs_meet(a: &Point, b: &Point, c: &Point, d: &Point) -> bool {
    !meeting(a, b, c, d).is_empty()
}

/// How two points on the shorter arc from `a` to `b` are ordered along it:
/// `x` before `y` when the turn from `x` to `y` is the arc's own.
pub fn along(normal: &Vector, x: &Point, y: &Point) -> Ordering {
    match x.v.cross(&y.v).dot(normal).signum() {
        1 => Ordering::Less,
        -1 => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

// --- Simulation of Simplicity, as S2 orders points and breaks ties ---

/// The orientation of three points, never 0 for three different points:
/// 1 when `a`, `b`, `c` turn counterclockwise, -1 when clockwise, and 0
/// only when two are the same. Three points exactly on one great circle
/// are given the orientation an infinitesimal perturbation of each,
/// ordered by the points' components, would give (S2's `Sign`).
pub fn sign(a: &Point, b: &Point, c: &Point) -> i32 {
    if a == b || b == c || c == a {
        return 0;
    }
    let exact = side(a, b, c);
    if exact != 0 {
        return exact;
    }
    // Sorted by their components, the permutation's sign kept.
    let mut sorted = [&a.v, &b.v, &c.v];
    let mut permutation = 1;
    if sorted[0] > sorted[1] {
        sorted.swap(0, 1);
        permutation = -permutation;
    }
    if sorted[1] > sorted[2] {
        sorted.swap(1, 2);
        permutation = -permutation;
    }
    if sorted[0] > sorted[1] {
        sorted.swap(0, 1);
        permutation = -permutation;
    }
    let [a, b, c] = sorted;
    permutation * perturbed(a, b, c, &b.cross(c))
}

/// S2's `SymbolicallyPerturbedSign`, for `a < b < c` exactly on one great
/// circle: the sign of the first term of the perturbed determinant's
/// expansion that isn't 0.
fn perturbed(a: &Vector, b: &Vector, c: &Vector, b_cross_c: &Vector) -> i32 {
    let terms = [
        b_cross_c.z.signum(),
        b_cross_c.y.signum(),
        b_cross_c.x.signum(),
        c.x.mul(&a.y).sub(&c.y.mul(&a.x)).signum(),
        c.x.signum(),
        -c.y.signum(),
        c.z.mul(&a.x).sub(&c.x.mul(&a.z)).signum(),
        c.z.signum(),
        a.x.mul(&b.y).sub(&a.y.mul(&b.x)).signum(),
        -b.x.signum(),
        b.y.signum(),
        a.x.signum(),
    ];
    terms.into_iter().find(|&term| term != 0).unwrap_or(1)
}

/// A direction at right angles to `a`, the same for every multiple of it
/// and the opposite for `-a`: S2's `Ortho`, with S2's constants as whole
/// numbers over 100,000.
pub fn reference(a: &Point) -> Point {
    let abs = |w: &Wide| if w.signum() < 0 { w.neg() } else { w.clone() };
    let (x, y, z) = (abs(&a.v.x), abs(&a.v.y), abs(&a.v.z));
    let largest = if x > y {
        if x > z { 0 } else { 2 }
    } else if y > z {
        1
    } else {
        2
    };
    let k = if largest == 0 { 2 } else { largest - 1 };
    let mut temp = [1200i64, 530, 457];
    if let Some(slot) = temp.get_mut(k) {
        *slot = 100_000;
    }
    let temp = Vector {
        x: Wide::from_i64(temp[0]),
        y: Wide::from_i64(temp[1]),
        z: Wide::from_i64(temp[2]),
    };
    Point::new(a.v.cross(&temp))
}

/// Whether the edges `o`–`a`, `o`–`b` and `o`–`c` are met in that order
/// sweeping counterclockwise around `o` (S2's `OrderedCCW`): true when `a`
/// is `b` or `b` is `c`, false when only `a` is `c`.
pub fn ordered_ccw(a: &Point, b: &Point, c: &Point, o: &Point) -> bool {
    let mut sum = 0;
    if sign(b, o, a) >= 0 {
        sum += 1;
    }
    if sign(c, o, b) >= 0 {
        sum += 1;
    }
    if sign(a, o, c) > 0 {
        sum += 1;
    }
    sum >= 2
}

/// Whether the angle `a`–`b`–`c` holds its vertex `b`, so that of the
/// polygons meeting at a vertex exactly one holds it (S2's
/// `AngleContainsVertex`): whether the reference direction at `b` lies in
/// the wedge from `b`–`c` counterclockwise round to `b`–`a`.
pub fn angle_contains_vertex(a: &Point, b: &Point, c: &Point) -> bool {
    !ordered_ccw(&reference(b), c, a, b)
}

/// Whether `a`–`b` and `c`–`d` cross at a point inside both (1), share a
/// vertex (0), or neither (-1), ties broken as [`sign`] breaks them (S2's
/// `CrossingSign`).
pub fn crossing_sign(a: &Point, b: &Point, c: &Point, d: &Point) -> i32 {
    if a == c || a == d || b == c || b == d {
        return 0;
    }
    if a == b || c == d {
        return -1;
    }
    let acb = -sign(a, b, c);
    let bda = sign(a, b, d);
    if bda != acb {
        return -1;
    }
    let cbd = -sign(c, d, b);
    if cbd != acb {
        return -1;
    }
    let dac = sign(c, d, a);
    if dac != acb { -1 } else { 1 }
}

/// For two edges sharing a vertex, whether the crossing is counted, so a
/// path through a vertex is counted once whichever edges meet there (S2's
/// `VertexCrossing`).
pub fn vertex_crossing(a: &Point, b: &Point, c: &Point, d: &Point) -> bool {
    if a == b || c == d {
        return false;
    }
    if a == c {
        return b == d || ordered_ccw(&reference(a), d, b, a);
    }
    if b == d {
        return ordered_ccw(&reference(b), c, a, b);
    }
    if a == d {
        return b == c || ordered_ccw(&reference(a), c, b, a);
    }
    if b == c {
        return ordered_ccw(&reference(b), d, a, b);
    }
    false
}

/// Whether the path `a`–`b` crosses the edge `c`–`d`, a shared vertex
/// counted by [`vertex_crossing`] (S2's `EdgeOrVertexCrossing`).
pub fn edge_or_vertex_crossing(a: &Point, b: &Point, c: &Point, d: &Point) -> bool {
    match crossing_sign(a, b, c, d) {
        1 => true,
        0 => vertex_crossing(a, b, c, d),
        _ => false,
    }
}

/// A box around part of the sphere, in whole numbers over `2^40`, drawn
/// wider than what it holds, so two boxes apart prove what they hold
/// apart and nothing else is concluded from them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub low: [i64; 3],
    pub high: [i64; 3],
}

impl Bounds {
    /// Around a point.
    pub fn point(p: &Point) -> Bounds {
        Bounds {
            low: p.a.map(|c| c - SLACK),
            high: p.a.map(|c| c + SLACK),
        }
    }

    /// Around the shorter arc from `a` to `b`: its ends' box, widened by
    /// the most the arc bows out from its chord, under half the chord's
    /// square.
    pub fn arc(a: &Point, b: &Point) -> Bounds {
        let chord: i128 = (0..3)
            .map(|i| {
                let d = i128::from(a.a[i]) - i128::from(b.a[i]);
                d * d
            })
            .sum();
        let bow = i64::try_from((chord >> (APPROX + 1)) + 1).unwrap_or(i64::MAX / 4);
        let reach = bow + 2 * SLACK;
        let mut low = [0i64; 3];
        let mut high = [0i64; 3];
        for i in 0..3 {
            low[i] = a.a[i].min(b.a[i]) - reach;
            high[i] = a.a[i].max(b.a[i]) + reach;
        }
        Bounds { low, high }
    }

    pub fn overlaps(&self, other: &Bounds) -> bool {
        (0..3).all(|i| self.low[i] <= other.high[i] && other.low[i] <= self.high[i])
    }
}
