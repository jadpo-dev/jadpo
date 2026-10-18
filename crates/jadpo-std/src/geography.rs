//! A geometry as PostGIS's `geography` column holds it (area 20,
//! "Storage"; chunk 5.5u4): each position its longitude, its latitude and,
//! where it has one, its altitude, each the IEEE double PostGIS keeps, in
//! WGS84 (SRID 4326). The runtime turns a geometry value into a [`Shape`]
//! through the storage codec (`storage::Stored::Shape`), the PostgreSQL
//! driver sends and reads it as EWKB, and the compiler writes it into a
//! statement as EWKT. A coordinate kept as the shortest decimal of a
//! double (area 20, question 2) is that double exactly, so a shape comes
//! back from the column as it went in.

/// The one coordinate system a `geography` column holds: WGS84's.
pub const SRID: u32 = 4326;

/// One position: its longitude and latitude in degrees and its altitude
/// above the ellipsoid in metres where it has one, each a double by its
/// bits, so a shape is compared and ordered exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Coordinates {
    pub longitude: u64,
    pub latitude: u64,
    pub altitude: Option<u64>,
}

impl Coordinates {
    /// From its doubles.
    pub fn new(longitude: f64, latitude: f64, altitude: Option<f64>) -> Coordinates {
        Coordinates {
            longitude: longitude.to_bits(),
            latitude: latitude.to_bits(),
            altitude: altitude.map(f64::to_bits),
        }
    }

    pub fn longitude(&self) -> f64 {
        f64::from_bits(self.longitude)
    }

    pub fn latitude(&self) -> f64 {
        f64::from_bits(self.latitude)
    }

    pub fn altitude(&self) -> Option<f64> {
        self.altitude.map(f64::from_bits)
    }
}

/// One of the six geometries, as `geography` holds it: a polygon's rings
/// the outer first, each closed and in the order its points were given.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Shape {
    Point(Coordinates),
    LineString(Vec<Coordinates>),
    Polygon(Vec<Vec<Coordinates>>),
    MultiPoint(Vec<Coordinates>),
    MultiLineString(Vec<Vec<Coordinates>>),
    MultiPolygon(Vec<Vec<Vec<Coordinates>>>),
}

/// The six geometry types, as PostGIS's `GeometryType` names them, each
/// with the standard type it is (area 19), in `Geometry`'s variants'
/// order.
pub const GEOMETRY_TYPES: [(&str, &str); 6] = [
    ("POINT", crate::standard::geo::POINT),
    ("LINESTRING", crate::standard::geo::LINE_STRING),
    ("POLYGON", crate::standard::geo::POLYGON),
    ("MULTIPOINT", crate::standard::geo::MULTI_POINT),
    ("MULTILINESTRING", crate::standard::geo::MULTI_LINE_STRING),
    ("MULTIPOLYGON", crate::standard::geo::MULTI_POLYGON),
];

/// The geometry types a `geography` column holding standard type
/// `standard` may hold, as `GeometryType` names them: one for each of the
/// six, all six for a `Geometry`; none for any other type.
pub fn geometry_types(standard: &str) -> Vec<&'static str> {
    if standard == crate::standard::geo::GEOMETRY {
        return GEOMETRY_TYPES.iter().map(|(sql, _)| *sql).collect();
    }
    GEOMETRY_TYPES
        .iter()
        .filter(|(_, name)| *name == standard)
        .map(|(sql, _)| *sql)
        .collect()
}

impl Shape {
    /// Its position among the six: 1 for a point to 6 for a
    /// multi-polygon, WKB's own numbering.
    pub fn code(&self) -> u32 {
        match self {
            Shape::Point(_) => 1,
            Shape::LineString(_) => 2,
            Shape::Polygon(_) => 3,
            Shape::MultiPoint(_) => 4,
            Shape::MultiLineString(_) => 5,
            Shape::MultiPolygon(_) => 6,
        }
    }

    /// Its row of [`GEOMETRY_TYPES`].
    fn names(&self) -> (&'static str, &'static str) {
        let [point, line, polygon, points, lines, polygons] = GEOMETRY_TYPES;
        match self {
            Shape::Point(_) => point,
            Shape::LineString(_) => line,
            Shape::Polygon(_) => polygon,
            Shape::MultiPoint(_) => points,
            Shape::MultiLineString(_) => lines,
            Shape::MultiPolygon(_) => polygons,
        }
    }

    /// As `GeometryType` names it: `POINT`.
    pub fn geometry_type(&self) -> &'static str {
        self.names().0
    }

    /// The standard type it is: `Point`.
    pub fn standard_type(&self) -> &'static str {
        self.names().1
    }

    /// Every position, in order.
    pub fn positions(&self) -> Vec<&Coordinates> {
        match self {
            Shape::Point(one) => vec![one],
            Shape::LineString(points) | Shape::MultiPoint(points) => points.iter().collect(),
            Shape::Polygon(rings) | Shape::MultiLineString(rings) => {
                rings.iter().flatten().collect()
            }
            Shape::MultiPolygon(polygons) => polygons.iter().flatten().flatten().collect(),
        }
    }

    /// Whether its positions have an altitude: `Some(true)` when all do,
    /// `Some(false)` when none does, and `None` when some do and some
    /// don't, which a `geography` can't hold (PostGIS: "can not mix
    /// dimensionality in a geometry").
    pub fn has_altitude(&self) -> Option<bool> {
        let positions = self.positions();
        let with = positions.iter().filter(|p| p.altitude.is_some()).count();
        match with {
            0 => Some(false),
            n if n == positions.len() => Some(true),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shape_names_its_type_and_whether_it_has_altitude() {
        let flat = Coordinates::new(-0.1276, 51.5072, None);
        let high = Coordinates::new(86.925, 27.9881, Some(8848.86));
        assert_eq!(Shape::Point(flat).geometry_type(), "POINT");
        assert_eq!(Shape::Point(high).has_altitude(), Some(true));
        assert_eq!(
            Shape::LineString(vec![flat, flat]).has_altitude(),
            Some(false)
        );
        assert_eq!(Shape::MultiPoint(vec![flat, high]).has_altitude(), None);
        assert_eq!(
            Shape::MultiPolygon(Vec::new()).standard_type(),
            "MultiPolygon"
        );
        assert_eq!(geometry_types("Point"), ["POINT"]);
        assert_eq!(geometry_types("Geometry").len(), 6);
        assert!(geometry_types("BoundingBox").is_empty());
    }
}
