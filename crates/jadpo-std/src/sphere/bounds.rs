//! How far a shape reaches north, south, east and west ([`super`]): the
//! latitudes and longitudes it covers, for `geo.bounding_box` and for
//! `geo.contains` with a `BoundingBox`, whose sides are parallels and
//! meridians, compared directly (area 20, "Operations").
//!
//! A corner's latitude and longitude are exact. An edge reaches further
//! north or south than its ends where its great circle's highest or lowest
//! point lies on it, a latitude worked out in working values; an edge
//! between two meridians half a turn apart goes over a pole. Its
//! longitudes are the shorter way between its ends, as a great circle's
//! longitude only grows or only shrinks along an edge under half the
//! globe. A shape holding a pole, a point there, a line through it or a
//! polygon around or touching it, reaches it at every longitude, since
//! every meridian meets there: its box runs from -180 to 180, and only
//! such a box holds it, whatever longitude its point at the pole is
//! written with (area 20, "Operations").

use std::cmp::Ordering;

use super::ring::Place;
use super::shape::{Position, Shape, point, region};
use super::vector::{Point, extremes_on_arc, from_degrees, same};
use super::wide::Wide;
use crate::Num;
use crate::fixed::Real;

/// The places every exact longitude is compared at: more than any
/// coordinate is held to (38).
const PLACES: u32 = 40;

/// A side of a box: a latitude exactly as held, or worked out, in degrees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Latitude {
    Exact(Num),
    Worked(Real),
}

impl Latitude {
    fn real(&self) -> Real {
        match self {
            Latitude::Exact(n) => super::vector::unit().decimal(*n),
            Latitude::Worked(r) => *r,
        }
    }

    fn compare(&self, other: &Latitude) -> Ordering {
        match (self, other) {
            (Latitude::Exact(a), Latitude::Exact(b)) => a.compare(*b),
            _ => self.real().cmp(&other.real()),
        }
    }
}

/// `10^k` for each `k` up to [`PLACES`], worked out once.
fn powers() -> &'static [Wide] {
    static POWERS: std::sync::OnceLock<Vec<Wide>> = std::sync::OnceLock::new();
    POWERS.get_or_init(|| {
        let base = Wide::from_i64(10);
        let mut out = vec![Wide::from_i64(1)];
        for _ in 0..PLACES {
            let next = out.last().map_or(Wide::from_i64(1), |last| last.mul(&base));
            out.push(next);
        }
        out
    })
}

/// A decimal as a whole number of `10^-40`.
fn exact(n: Num) -> Wide {
    let power = powers()
        .get(PLACES.saturating_sub(n.scale) as usize)
        .cloned()
        .unwrap_or_else(|| Wide::from_i64(1));
    let mantissa = Wide::from_parts(
        n.mantissa < 0,
        &[
            n.mantissa.unsigned_abs() as u64,
            (n.mantissa.unsigned_abs() >> 64) as u64,
        ],
    );
    mantissa.mul(&power)
}

/// `degrees` in whole numbers of `10^-40`.
fn degrees(whole: i64) -> Wide {
    exact(Num {
        mantissa: i128::from(whole),
        scale: 0,
    })
}

/// A longitude with -180 taken as 180, the one antimeridian.
fn east_of_antimeridian(longitude: Num) -> Num {
    if exact(longitude) == degrees(-180) {
        Num {
            mantissa: 180,
            scale: 0,
        }
    } else {
        longitude
    }
}

/// `x` taken into `[0, 360)` by whole turns.
fn turn(mut x: Wide) -> Wide {
    let whole = degrees(360);
    while x.signum() < 0 {
        x = x.add(&whole);
    }
    while x >= whole {
        x = x.sub(&whole);
    }
    x
}

/// What a shape reaches.
#[derive(Clone, Debug)]
pub struct Reach {
    pub south: Latitude,
    pub north: Latitude,
    /// Whether it covers every longitude: it holds a pole.
    pub every_longitude: bool,
    /// The longitudes it covers, each run from its west going east to its
    /// east (the two the same for one meridian).
    pub runs: Vec<(Num, Num)>,
}

impl Reach {
    fn latitude(&mut self, latitude: Latitude) {
        if latitude.compare(&self.south) == Ordering::Less {
            self.south = latitude.clone();
        }
        if latitude.compare(&self.north) == Ordering::Greater {
            self.north = latitude;
        }
    }
}

fn is_pole(latitude: Num) -> bool {
    let size = exact(latitude);
    size == degrees(90) || size == degrees(-90)
}

/// The highest or lowest latitude a great circle reaches, in degrees:
/// its pole's angle from the axis, from the edge's ends as unit vectors.
fn top_latitude(p: &[Real; 3], q: &[Real; 3]) -> Real {
    let unit = super::vector::unit();
    let mul = |a: &Real, b: &Real| unit.mul(a, b);
    let x = mul(&p[1], &q[2]).sub(&mul(&p[2], &q[1]));
    let y = mul(&p[2], &q[0]).sub(&mul(&p[0], &q[2]));
    let z = mul(&p[0], &q[1]).sub(&mul(&p[1], &q[0]));
    let radians = unit.atan2(&unit.hypot(&x, &y), &z.abs());
    unit.div(&unit.mul(&radians, &unit.int(180)), unit.pi())
}

/// The edge from `p` to `q` (positions, with their points and unit
/// vectors) added to what a shape reaches.
fn add_edge(
    reach: &mut Reach,
    (p, pp, pu): (&Position, &Point, &[Real; 3]),
    (q, qp, qu): (&Position, &Point, &[Real; 3]),
) {
    reach.latitude(Latitude::Exact(p.0));
    reach.latitude(Latitude::Exact(q.0));
    if same(pp, qp) {
        add_point_longitude(reach, p);
        return;
    }
    if is_pole(p.0) || is_pole(q.0) {
        reach.every_longitude = true;
        return;
    }
    let (west, east) = (east_of_antimeridian(p.1), east_of_antimeridian(q.1));
    let gain = turn(exact(east).sub(&exact(west)));
    let half = degrees(180);
    if gain == half {
        // Two meridians half a turn apart: over the pole their ends lean
        // towards, so through it, unless the ends are opposite, which no
        // edge joins.
        let lean = exact(p.0).add(&exact(q.0));
        let pole = match lean.signum() {
            1 => 90,
            -1 => -90,
            _ => {
                reach.runs.push((west, west));
                reach.runs.push((east, east));
                return;
            }
        };
        reach.latitude(Latitude::Exact(Num {
            mantissa: pole,
            scale: 0,
        }));
        reach.every_longitude = true;
        return;
    }
    if gain.is_zero() {
        reach.runs.push((west, west));
        return;
    }
    if gain < half {
        reach.runs.push((west, east));
    } else {
        reach.runs.push((east, west));
    }
    // The great circle's highest point and its opposite, the lowest.
    let (north_on, south_on) = extremes_on_arc(pp, qp);
    if north_on {
        reach.latitude(Latitude::Worked(top_latitude(pu, qu)));
    }
    if south_on {
        reach.latitude(Latitude::Worked(top_latitude(pu, qu).neg()));
    }
}

fn add_point_longitude(reach: &mut Reach, p: &Position) {
    if is_pole(p.0) {
        reach.every_longitude = true;
    } else {
        let at = east_of_antimeridian(p.1);
        reach.runs.push((at, at));
    }
}

/// What `shape` reaches; `None` for a shape with no points.
pub fn reach(shape: &Shape) -> Option<Reach> {
    let first = shape
        .points
        .first()
        .or_else(|| shape.lines.iter().flatten().next())
        .or_else(|| {
            shape
                .areas
                .iter()
                .flat_map(|area| area.outer.first())
                .next()
        })?;
    let mut reach = Reach {
        south: Latitude::Exact(first.0),
        north: Latitude::Exact(first.0),
        every_longitude: false,
        runs: Vec::new(),
    };
    for p in &shape.points {
        reach.latitude(Latitude::Exact(p.0));
        add_point_longitude(&mut reach, p);
    }
    let mut paths: Vec<&[Position]> = shape.lines.iter().map(Vec::as_slice).collect();
    paths.extend(shape.areas.iter().map(|area| area.outer.as_slice()));
    for path in paths {
        let built: Vec<(Point, [Real; 3])> = path
            .iter()
            .map(|(latitude, longitude)| from_degrees(*latitude, *longitude))
            .collect();
        if let [only] = path {
            reach.latitude(Latitude::Exact(only.0));
            add_point_longitude(&mut reach, only);
        }
        for (pair, points) in path.windows(2).zip(built.windows(2)) {
            let ([p, q], [(pp, pu), (qp, qu)]) = (pair, points) else {
                continue;
            };
            add_edge(&mut reach, (p, pp, pu), (q, qp, qu));
        }
    }
    // A polygon holding a pole reaches it at every longitude; one with
    // the pole on its edge already does, by that edge.
    for area in &shape.areas {
        let region = region(area);
        for (pole, latitude) in [
            (
                90,
                Num {
                    mantissa: 90,
                    scale: 0,
                },
            ),
            (
                -90,
                Num {
                    mantissa: -90,
                    scale: 0,
                },
            ),
        ] {
            let at = point(&(
                Num {
                    mantissa: pole,
                    scale: 0,
                },
                Num {
                    mantissa: 0,
                    scale: 0,
                },
            ));
            if region.place(&at) != Place::Outside {
                reach.latitude(Latitude::Exact(latitude));
                reach.every_longitude = true;
            }
        }
    }
    Some(reach)
}

/// The smallest box of parallels and meridians holding what `reach`
/// covers: its south, west, north and east. Its longitudes are the
/// complement of the widest gap between the runs, taken across the
/// antimeridian only when that box is narrower than the one that isn't;
/// every longitude when there is no gap.
pub fn smallest_box(reach: &Reach) -> (Latitude, Num, Latitude, Num) {
    let all = (
        Num {
            mantissa: -180,
            scale: 0,
        },
        Num {
            mantissa: 180,
            scale: 0,
        },
    );
    let (west, east) = if reach.every_longitude {
        all
    } else {
        widest_gap_complement(&reach.runs).unwrap_or(all)
    };
    (reach.south.clone(), west, reach.north.clone(), east)
}

/// The runs as intervals on `[-180, 180]`, merged, and the box left by
/// the widest gap between them.
fn widest_gap_complement(runs: &[(Num, Num)]) -> Option<(Num, Num)> {
    let minus = Num {
        mantissa: -180,
        scale: 0,
    };
    let plus = Num {
        mantissa: 180,
        scale: 0,
    };
    let mut intervals: Vec<(Num, Num)> = Vec::new();
    for &(west, east) in runs {
        if exact(west) <= exact(east) {
            intervals.push((west, east));
        } else {
            intervals.push((west, plus));
            intervals.push((minus, east));
        }
    }
    intervals.sort_by_key(|interval| exact(interval.0));
    let mut merged: Vec<(Num, Num)> = Vec::new();
    for (start, end) in intervals {
        match merged.last_mut() {
            Some(last) if exact(start) <= exact(last.1) => {
                if exact(end) > exact(last.1) {
                    last.1 = end;
                }
            }
            _ => merged.push((start, end)),
        }
    }
    let (first, last) = (merged.first()?, merged.last()?);
    // The gap across the antimeridian, which leaves a box that doesn't
    // cross it, wins a tie.
    let mut best = exact(first.0).add(&degrees(360)).sub(&exact(last.1));
    let mut chosen = (first.0, last.1);
    for pair in merged.windows(2) {
        let [before, after] = pair else { continue };
        let gap = exact(after.0).sub(&exact(before.1));
        if gap > best {
            best = gap;
            chosen = (after.0, before.1);
        }
    }
    if best.is_zero() {
        return Some((minus, plus));
    }
    Some(chosen)
}

/// Whether the box with these sides holds every point of `shape`.
pub fn box_holds(south: Num, west: Num, north: Num, east: Num, shape: &Shape) -> bool {
    let Some(reach) = reach(shape) else {
        return true;
    };
    if reach.south.compare(&Latitude::Exact(south)) == Ordering::Less
        || reach.north.compare(&Latitude::Exact(north)) == Ordering::Greater
    {
        return false;
    }
    let (w, e) = (exact(west), exact(east));
    let width = if w <= e {
        e.sub(&w)
    } else {
        e.sub(&w).add(&degrees(360))
    };
    if reach.every_longitude {
        return width >= degrees(360);
    }
    reach.runs.iter().all(|&(from, to)| {
        let offset = turn(exact(from).sub(&w));
        let length = turn(exact(to).sub(&exact(from)));
        offset.add(&length) <= width
    })
}

/// The smallest IEEE double at or above `degrees` (`up`), or at or below
/// it, written as its shortest decimal: a worked-out side of a box, rounded
/// outwards so the box still holds the shape, and held as every coordinate
/// is, as a double (area 20, question 2). Rust's own correctly rounded
/// parsing and printing and IEEE's next double, no maths library, so the
/// same on every target.
pub fn outward(degrees: &Real, up: bool) -> Num {
    let unit = super::vector::unit();
    let places = 36;
    let mantissa = degrees
        .rounded(places, crate::Rounding::HalfEven)
        .unwrap_or(0);
    let text = Num {
        mantissa,
        scale: places,
    }
    .to_string();
    let mut double: f64 = text.parse().unwrap_or(0.0);
    let held = |d: f64| Num::parse(&format!("{d}")).map(|n| unit.decimal(n));
    match held(double) {
        Some(value) if up && value < *degrees => double = double.next_up(),
        Some(value) if !up && value > *degrees => double = double.next_down(),
        _ => {}
    }
    let shortest = crate::standard::geo::nearest_double(&format!("{double}"))
        .unwrap_or_else(|| "0".to_string());
    Num::parse(&shortest).unwrap_or(Num {
        mantissa: 0,
        scale: 0,
    })
}
