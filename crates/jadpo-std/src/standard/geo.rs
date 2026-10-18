//! The geospatial types (area 19, "The standard types" and "Geospatial: a
//! future area", chunk 5.5s): positions on Earth in WGS84 with `latitude`
//! and `longitude` by name, never a pair; the geometries made of them, sent
//! as GeoJSON (RFC 7946), whose `[longitude, latitude]` order exists only
//! inside `jadpo-value`'s `geo` module; and the cells, tiles and projected
//! coordinates applications pass between them. Storage in PostGIS and the
//! `geo.` operations come with the geospatial area (roadmap 5.5u); until
//! then a geometry's storage is [`Storage::Geography`], which a field
//! can't use yet. What the page left open is area 19's "Chosen in chunk
//! 5.5s".

use super::across::{Check, Order, StandardRule, Verdict, in_order};
use super::{
    Examples, FIRST_RELEASE, FieldMatch, Format, FormatKey, Holds, Json, NO_OPTIONS, Part,
    StandardType, Storage, TextForm, Validator,
};
use crate::read::SqlType;
use crate::{Builtin, Num, QuantityKind};

/// `Latitude`.
pub const LATITUDE: &str = "Latitude";
/// `Longitude`.
pub const LONGITUDE: &str = "Longitude";
/// `Bearing`.
pub const BEARING: &str = "Bearing";
/// `Crs`.
pub const CRS: &str = "Crs";
/// `Geohash`.
pub const GEOHASH: &str = "Geohash";
/// `H3Cell`.
pub const H3_CELL: &str = "H3Cell";
/// `MapTile`.
pub const MAP_TILE: &str = "MapTile";
/// `EastingNorthing`.
pub const EASTING_NORTHING: &str = "EastingNorthing";
/// `Point`.
pub const POINT: &str = "Point";
/// `LineString`.
pub const LINE_STRING: &str = "LineString";
/// `Polygon`.
pub const POLYGON: &str = "Polygon";
/// `MultiPoint`.
pub const MULTI_POINT: &str = "MultiPoint";
/// `MultiLineString`.
pub const MULTI_LINE_STRING: &str = "MultiLineString";
/// `MultiPolygon`.
pub const MULTI_POLYGON: &str = "MultiPolygon";
/// `Geometry`.
pub const GEOMETRY: &str = "Geometry";
/// `BoundingBox`.
pub const BOUNDING_BOX: &str = "BoundingBox";
/// `Circle`.
pub const CIRCLE: &str = "Circle";

/// The deepest zoom a `MapTile` takes: 30, which keeps its `x` and `y`
/// within a 32-bit signed integer, as PostGIS's `ST_TileEnvelope` takes
/// them.
pub const MAX_ZOOM: i64 = 30;

const fn whole(n: i128) -> Num {
    Num {
        mantissa: n,
        scale: 0,
    }
}

const fn examples(
    valid: &'static [(&'static str, &'static str)],
    malformed: &'static [&'static str],
) -> Examples {
    Examples {
        valid,
        format: None,
        malformed,
        of: None,
        settings: "",
    }
}

const CANONICAL: Format = Format {
    key: FormatKey::Canonical,
    options: NO_OPTIONS,
};

const GEOMETRY_FORMAT: Format = Format {
    key: FormatKey::Geometry,
    options: NO_OPTIONS,
};

/// A position's latitude, a `Latitude`.
const LATITUDE_PART: Part = Part::Standard(LATITUDE);
/// A position's longitude, a `Longitude`.
const LONGITUDE_PART: Part = Part::Standard(LONGITUDE);
/// A geometry's positions, each a `Point`.
const POINTS: Part = Part::List(&Part::Standard(POINT));

/// Degrees north of the equator in WGS84, from -90 to 90, held as written.
pub const LATITUDE_ENTRY: StandardType = StandardType {
    name: LATITUDE,
    holds: Holds::Bounded {
        of: Builtin::Decimal,
        min: Some(whole(-90)),
        max: Some(whole(90)),
    },
    text: Some(TextForm::Decimal),
    json: Json::Text,
    storage: Storage::Column(SqlType::Numeric),
    format: Format {
        key: FormatKey::Coordinate {
            positive: "N",
            negative: "S",
        },
        options: NO_OPTIONS,
    },
    ordered: true,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("-90", "90° S"),
            ("-33.8688", "33.8688° S"),
            ("0", "0°"),
            ("51.5072", "51.5072° N"),
            ("90", "90° N"),
        ],
        &["51.5072 N", "N51.5072", "51,5072", "1e1", "+51.5", ""],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// Degrees east of the prime meridian in WGS84, from -180 to 180, held as
/// written: -180 and 180 are both the antimeridian, as RFC 7946 takes them.
pub const LONGITUDE_ENTRY: StandardType = StandardType {
    name: LONGITUDE,
    holds: Holds::Bounded {
        of: Builtin::Decimal,
        min: Some(whole(-180)),
        max: Some(whole(180)),
    },
    text: Some(TextForm::Decimal),
    json: Json::Text,
    storage: Storage::Column(SqlType::Numeric),
    format: Format {
        key: FormatKey::Coordinate {
            positive: "E",
            negative: "W",
        },
        options: NO_OPTIONS,
    },
    ordered: true,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("-180", "180° W"),
            ("-0.1276", "0.1276° W"),
            ("0", "0°"),
            ("151.2093", "151.2093° E"),
            ("180", "180° E"),
        ],
        &["0.1276 W", "W0.1276", "-0,1276", "", "east"],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// A compass direction in degrees clockwise from true north, from 0 to
/// 360, as the catalogue sketches it; a wind's direction is the one it
/// comes from.
pub const BEARING_ENTRY: StandardType = StandardType {
    name: BEARING,
    holds: Holds::Bounded {
        of: Builtin::Decimal,
        min: Some(whole(0)),
        max: Some(whole(360)),
    },
    text: Some(TextForm::Decimal),
    json: Json::Text,
    storage: Storage::Column(SqlType::Numeric),
    format: Format {
        key: FormatKey::Suffixed("°"),
        options: NO_OPTIONS,
    },
    ordered: true,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("0", "0°"),
            ("90", "90°"),
            ("247.5", "247.5°"),
            ("359.9", "359.9°"),
        ],
        &["90°", "N", "1.5rad", "", "ninety"],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// A coordinate reference system by its EPSG code, `EPSG:27700`, the
/// authority every GIS reads; WGS84 is `EPSG:4326`.
pub const CRS_ENTRY: StandardType = StandardType {
    name: CRS,
    holds: Holds::Text {
        pattern: "EPSG:[1-9][0-9]{0,5}",
        personal: None,
    },
    text: Some(TextForm::Checked),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("EPSG:4326", "EPSG:4326"),
            ("EPSG:27700", "EPSG:27700"),
            ("EPSG:3857", "EPSG:3857"),
        ],
        &[
            "epsg:4326",
            "EPSG:",
            "EPSG:04326",
            "WGS84",
            "EPSG:4326 ",
            "urn:ogc:def:crs:EPSG::4326",
            "",
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// A geohash cell (geohash.org): 1 to 12 characters of its base 32, in
/// lower case, each one a finer cell.
pub const GEOHASH_ENTRY: StandardType = StandardType {
    name: GEOHASH,
    holds: Holds::Text {
        pattern: "[0-9b-hjkmnp-z]{1,12}",
        personal: None,
    },
    text: Some(TextForm::Checked),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: CANONICAL,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            ("g", "g"),
            ("gcpvj0duq", "gcpvj0duq"),
            ("u09tvw0f6szy", "u09tvw0f6szy"),
        ],
        &["gcpvja", "GCPVJ", "gcpvj0duq5x7z", "gcpv j", ""],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// An H3 cell (Uber's H3 index, mode 1) as its 15 lower-case hexadecimal
/// digits, `8928308280fffff`: its resolution, base cell and digits valid.
pub const H3_CELL_ENTRY: StandardType = StandardType {
    name: H3_CELL,
    holds: Holds::Text {
        pattern: "8[0-9a-f]{14}",
        personal: None,
    },
    text: Some(TextForm::Checked),
    json: Json::Text,
    storage: Storage::Column(SqlType::Text),
    format: CANONICAL,
    ordered: false,
    validator: Some(Validator::H3Cell),
    shape: None,
    examples: examples(
        &[
            ("8001fffffffffff", "8001fffffffffff"),
            ("85283473fffffff", "85283473fffffff"),
            ("8928308280fffff", "8928308280fffff"),
            ("8f2830828052d25", "8f2830828052d25"),
        ],
        &[
            // Base cell 122, a digit 7 within its resolution, a digit
            // beyond it, upper case, a 64-bit index with its leading zero,
            // a pentagon's deleted subsequence.
            "80f5fffffffffff",
            "892830828ffffff",
            "8928308287fffff",
            "8928308280FFFFF",
            "08928308280fffff",
            "81087ffffffffff",
            "",
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// Whether `text` is an H3 cell's index: mode 1, reserved bits clear, a
/// resolution from 0 to 15, one of the 122 base cells, a digit from 0 to 6
/// for each resolution it has and 7 for each it hasn't, and, on one of the
/// 12 pentagons, no first digit 1 (H3's deleted subsequence).
pub fn is_h3_cell(text: &str) -> bool {
    const PENTAGONS: [u64; 12] = [4, 14, 24, 38, 49, 58, 63, 72, 83, 97, 107, 117];
    if text.len() != 15 || !text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return false;
    }
    let Ok(index) = u64::from_str_radix(text, 16) else {
        return false;
    };
    if index >> 63 != 0 || (index >> 59) & 0xf != 1 || (index >> 56) & 0x7 != 0 {
        return false;
    }
    let resolution = (index >> 52) & 0xf;
    let base = (index >> 45) & 0x7f;
    if base >= 122 {
        return false;
    }
    let digit = |at: u64| (index >> (3 * (15 - at))) & 0x7;
    if !(1..=15).all(|at| (digit(at) == 7) == (at > resolution)) {
        return false;
    }
    let first = (1..=resolution).map(digit).find(|digit| *digit != 0);
    !(PENTAGONS.contains(&base) && first == Some(1))
}

/// `MapTile`'s parts: a zoom, and a column and row each below 2 to the
/// zoom (OpenStreetMap's slippy map tiles, XYZ: the row counts down from
/// the north).
pub const MAP_TILE_FIELDS: [(&str, Part); 3] = [
    ("zoom", Part::Whole(0, MAX_ZOOM)),
    ("x", Part::Whole(0, (1 << MAX_ZOOM) - 1)),
    ("y", Part::Whole(0, (1 << MAX_ZOOM) - 1)),
];

/// A web map tile, by its zoom, column and row on OpenStreetMap's XYZ
/// scheme, sent as named fields, stored a column each and shown as a tile
/// URL writes it, `10/511/340`.
pub const MAP_TILE_ENTRY: StandardType = StandardType {
    name: MAP_TILE,
    holds: Holds::Object(&MAP_TILE_FIELDS),
    text: None,
    json: Json::Fields(&["zoom", "x", "y"]),
    storage: Storage::Columns {
        parts: &[("zoom", "smallint"), ("x", "integer"), ("y", "integer")],
        fixed: &[],
    },
    format: GEOMETRY_FORMAT,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (r#"{"zoom":0,"x":0,"y":0}"#, "0/0/0"),
            (r#"{"zoom":10,"x":511,"y":340}"#, "10/511/340"),
            (r#"{"zoom":18,"x":130981,"y":87168}"#, "18/130981/87168"),
        ],
        &[
            // A column or a row past the zoom's last, a zoom past 30, a
            // tile as its path.
            r#"{"zoom":1,"x":2,"y":0}"#,
            r#"{"zoom":1,"x":0,"y":2}"#,
            r#"{"zoom":31,"x":0,"y":0}"#,
            r#"{"zoom":10,"x":511}"#,
            r#""10/511/340""#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `EastingNorthing`'s parts: projected coordinates in the units of their
/// reference system, by name, so neither is read as the other.
pub const EASTING_NORTHING_FIELDS: [(&str, Part); 3] = [
    ("easting", Part::Of(Builtin::Decimal)),
    ("northing", Part::Of(Builtin::Decimal)),
    ("crs", Part::Standard(CRS)),
];

/// Projected coordinates, an easting and a northing in their reference
/// system (the British National Grid's `EPSG:27700`, a UTM zone), sent as
/// named fields and stored a column each.
pub const EASTING_NORTHING_ENTRY: StandardType = StandardType {
    name: EASTING_NORTHING,
    holds: Holds::Object(&EASTING_NORTHING_FIELDS),
    text: None,
    json: Json::Fields(&["easting", "northing", "crs"]),
    storage: Storage::Columns {
        parts: &[
            ("easting", "numeric"),
            ("northing", "numeric"),
            ("crs", "text"),
        ],
        fixed: &[],
    },
    format: GEOMETRY_FORMAT,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"easting":"530000","northing":"180000","crs":"EPSG:27700"}"#,
                "easting 530,000, northing 180,000 (EPSG:27700)",
            ),
            (
                r#"{"easting":"448251.795","northing":"5411932.678","crs":"EPSG:32630"}"#,
                "easting 448,251.795, northing 5,411,932.678 (EPSG:32630)",
            ),
        ],
        &[
            r#"{"easting":"530000","northing":"180000","crs":"OSGB36"}"#,
            r#"{"easting":"530000","crs":"EPSG:27700"}"#,
            r#"{"easting":"east","northing":"180000","crs":"EPSG:27700"}"#,
            r#""530000 180000""#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `Point`'s parts: a latitude and a longitude by name, and the height
/// above the WGS84 ellipsoid where it has one (RFC 7946's third
/// coordinate), as a `Length` of either sign.
pub const POINT_FIELDS: [(&str, Part); 3] = [
    ("latitude", LATITUDE_PART),
    ("longitude", LONGITUDE_PART),
    (
        "altitude",
        Part::Optional(&Part::Of(Builtin::Quantity(QuantityKind::Length))),
    ),
];

/// What `type.standard_exists` matches for `Point`: a latitude and a
/// longitude by their common names, each built on `Int` or `Decimal`
/// (area 19, "Packages use the standard types").
const POINT_SHAPE: &[FieldMatch] = &[
    FieldMatch {
        meaning: "a latitude",
        names: &["latitude", "lat"],
        built_on: &[Builtin::Int, Builtin::Decimal],
        or_enum: false,
        optional: false,
    },
    FieldMatch {
        meaning: "a longitude",
        names: &["longitude", "lng", "lon", "long"],
        built_on: &[Builtin::Int, Builtin::Decimal],
        or_enum: false,
        optional: false,
    },
];

/// A position on Earth in WGS84, by its `latitude` and `longitude`, never
/// a pair: sent as a GeoJSON `Point`, whose `[longitude, latitude]` exists
/// only on the wire, and shown with its hemispheres, `51.5072° N, 0.1276°
/// W`.
pub const POINT_ENTRY: StandardType = StandardType {
    name: POINT,
    holds: Holds::Object(&POINT_FIELDS),
    text: None,
    json: Json::GeoJson,
    storage: Storage::Geography,
    format: GEOMETRY_FORMAT,
    ordered: false,
    validator: None,
    shape: Some(POINT_SHAPE),
    examples: examples(
        &[
            (
                r#"{"type":"Point","coordinates":[-0.1276,51.5072]}"#,
                "51.5072° N, 0.1276° W",
            ),
            (
                r#"{"type":"Point","coordinates":[151.2093,-33.8688]}"#,
                "33.8688° S, 151.2093° E",
            ),
            (r#"{"type":"Point","coordinates":[0,0]}"#, "0°, 0°"),
            (
                r#"{"type":"Point","coordinates":[-180,-90]}"#,
                "90° S, 180° W",
            ),
            (
                r#"{"type":"Point","coordinates":[86.925,27.9881,8848.86]}"#,
                "27.9881° N, 86.925° E, 8,848.86 m",
            ),
        ],
        &[
            // A latitude or a longitude out of range, too few or too many
            // coordinates, coordinates as text, another geometry, a
            // reference system other than WGS84, a member GeoJSON hasn't,
            // a point by its names, a point as WKT.
            r#"{"type":"Point","coordinates":[0,91]}"#,
            r#"{"type":"Point","coordinates":[181,0]}"#,
            r#"{"type":"Point","coordinates":[0]}"#,
            r#"{"type":"Point","coordinates":[0,0,0,0]}"#,
            r#"{"type":"Point","coordinates":["0","0"]}"#,
            r#"{"type":"Point"}"#,
            r#"{"type":"LineString","coordinates":[[0,0],[1,1]]}"#,
            r#"{"type":"Point","coordinates":[0,0],"crs":{"type":"name","properties":{"name":"EPSG:27700"}}}"#,
            r#"{"type":"Point","coordinates":[0,0],"name":"Null Island"}"#,
            r#"{"latitude":"51.5072","longitude":"-0.1276"}"#,
            r#""POINT (-0.1276 51.5072)""#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// A path of two points or more, in order: a GeoJSON `LineString`.
pub const LINE_STRING_ENTRY: StandardType = StandardType {
    name: LINE_STRING,
    holds: Holds::Object(&[("points", POINTS)]),
    text: None,
    json: Json::GeoJson,
    storage: Storage::Geography,
    format: GEOMETRY_FORMAT,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"type":"LineString","coordinates":[[-0.1276,51.5072],[2.3522,48.8566]]}"#,
                "(51.5072° N, 0.1276° W; 48.8566° N, 2.3522° E)",
            ),
            (
                r#"{"type":"LineString","coordinates":[[0,0],[0.5,0.5,10],[1,0]]}"#,
                "(0°, 0°; 0.5° N, 0.5° E, 10 m; 0°, 1° E)",
            ),
        ],
        &[
            // One point, none, a point out of range, a position that
            // isn't one.
            r#"{"type":"LineString","coordinates":[[0,0]]}"#,
            r#"{"type":"LineString","coordinates":[]}"#,
            r#"{"type":"LineString","coordinates":[[0,0],[0,95]]}"#,
            r#"{"type":"LineString","coordinates":[[0,0],1]}"#,
            r#"{"type":"Point","coordinates":[0,0]}"#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `Polygon`'s parts: its outer ring, then its holes, each ring closed (its
/// last point its first) with four points or more.
pub const POLYGON_FIELDS: [(&str, Part); 2] = [("outer", POINTS), ("holes", Part::List(&POINTS))];

/// An area with any holes: a GeoJSON `Polygon`, each ring closed, written
/// by RFC 7946's right-hand rule (the outer ring counterclockwise, each
/// hole clockwise) and read in either winding, rewound to it.
pub const POLYGON_ENTRY: StandardType = StandardType {
    name: POLYGON,
    holds: Holds::Object(&POLYGON_FIELDS),
    text: None,
    json: Json::GeoJson,
    storage: Storage::Geography,
    format: GEOMETRY_FORMAT,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}"#,
                "((0°, 0°; 0°, 1° E; 1° N, 1° E; 1° N, 0°; 0°, 0°))",
            ),
            (
                r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]],[[0.25,0.25],[0.25,0.75],[0.75,0.75],[0.75,0.25],[0.25,0.25]]]}"#,
                "((0°, 0°; 0°, 1° E; 1° N, 1° E; 1° N, 0°; 0°, 0°), (0.25° N, 0.25° E; 0.75° N, 0.25° E; 0.75° N, 0.75° E; 0.25° N, 0.75° E; 0.25° N, 0.25° E))",
            ),
        ],
        &[
            // A ring not closed, a closed ring of three points, no ring, a
            // point out of range, a ring as a position.
            r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1]]]}"#,
            r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[0,0]]]}"#,
            r#"{"type":"Polygon","coordinates":[]}"#,
            r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,91],[0,0]]]}"#,
            r#"{"type":"Polygon","coordinates":[[0,0]]}"#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// Points with no order between them, at least one: a GeoJSON
/// `MultiPoint`.
pub const MULTI_POINT_ENTRY: StandardType = StandardType {
    name: MULTI_POINT,
    holds: Holds::Object(&[("points", POINTS)]),
    text: None,
    json: Json::GeoJson,
    storage: Storage::Geography,
    format: GEOMETRY_FORMAT,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"type":"MultiPoint","coordinates":[[-0.1276,51.5072],[2.3522,48.8566]]}"#,
                "(51.5072° N, 0.1276° W; 48.8566° N, 2.3522° E)",
            ),
            (r#"{"type":"MultiPoint","coordinates":[[0,0]]}"#, "(0°, 0°)"),
        ],
        &[
            r#"{"type":"MultiPoint","coordinates":[]}"#,
            r#"{"type":"MultiPoint","coordinates":[[0,-91]]}"#,
            r#"{"type":"MultiPoint","coordinates":[0,0]}"#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// Paths, at least one: a GeoJSON `MultiLineString`.
pub const MULTI_LINE_STRING_ENTRY: StandardType = StandardType {
    name: MULTI_LINE_STRING,
    holds: Holds::Object(&[("lines", Part::List(&Part::Standard(LINE_STRING)))]),
    text: None,
    json: Json::GeoJson,
    storage: Storage::Geography,
    format: GEOMETRY_FORMAT,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"type":"MultiLineString","coordinates":[[[0,0],[1,1]],[[2,2],[3,3]]]}"#,
                "((0°, 0°; 1° N, 1° E), (2° N, 2° E; 3° N, 3° E))",
            ),
            (
                r#"{"type":"MultiLineString","coordinates":[[[0,0],[1,1]]]}"#,
                "((0°, 0°; 1° N, 1° E))",
            ),
        ],
        &[
            r#"{"type":"MultiLineString","coordinates":[]}"#,
            r#"{"type":"MultiLineString","coordinates":[[[0,0]]]}"#,
            r#"{"type":"MultiLineString","coordinates":[[0,0],[1,1]]}"#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// Areas, at least one: a GeoJSON `MultiPolygon`, each by `Polygon`'s
/// rules.
pub const MULTI_POLYGON_ENTRY: StandardType = StandardType {
    name: MULTI_POLYGON,
    holds: Holds::Object(&[("polygons", Part::List(&Part::Standard(POLYGON)))]),
    text: None,
    json: Json::GeoJson,
    storage: Storage::Geography,
    format: GEOMETRY_FORMAT,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,1],[0,0]]],[[[2,2],[3,2],[3,3],[2,3],[2,2]]]]}"#,
                "(((0°, 0°; 0°, 1° E; 1° N, 1° E; 1° N, 0°; 0°, 0°)), ((2° N, 2° E; 2° N, 3° E; 3° N, 3° E; 3° N, 2° E; 2° N, 2° E)))",
            ),
            (
                r#"{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,1],[0,0]]]]}"#,
                "(((0°, 0°; 0°, 1° E; 1° N, 1° E; 1° N, 0°; 0°, 0°)))",
            ),
        ],
        &[
            r#"{"type":"MultiPolygon","coordinates":[]}"#,
            r#"{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,1]]]]}"#,
            r#"{"type":"MultiPolygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}"#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `Geometry`'s variants: one of the six geometries, each holding it by
/// its own name.
pub const GEOMETRY_VARIANTS: &[(&str, &[(&str, Part)])] = &[
    ("point", &[("point", Part::Standard(POINT))]),
    (
        "line_string",
        &[("line_string", Part::Standard(LINE_STRING))],
    ),
    ("polygon", &[("polygon", Part::Standard(POLYGON))]),
    (
        "multi_point",
        &[("multi_point", Part::Standard(MULTI_POINT))],
    ),
    (
        "multi_line_string",
        &[("multi_line_string", Part::Standard(MULTI_LINE_STRING))],
    ),
    (
        "multi_polygon",
        &[("multi_polygon", Part::Standard(MULTI_POLYGON))],
    ),
];

/// The GeoJSON `type` each of `Geometry`'s variants is sent as, in their
/// order.
pub const GEOJSON_TYPES: [(&str, &str); 6] = [
    ("point", POINT),
    ("line_string", LINE_STRING),
    ("polygon", POLYGON),
    ("multi_point", MULTI_POINT),
    ("multi_line_string", MULTI_LINE_STRING),
    ("multi_polygon", MULTI_POLYGON),
];

/// Any one geometry: a choice of the six, sent as the GeoJSON geometry it
/// holds, its `type` saying which, never in area 2's tagged form.
pub const GEOMETRY_ENTRY: StandardType = StandardType {
    name: GEOMETRY,
    holds: Holds::Choice(GEOMETRY_VARIANTS),
    text: None,
    json: Json::GeoJson,
    storage: Storage::Geography,
    format: GEOMETRY_FORMAT,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"type":"Point","coordinates":[-0.1276,51.5072]}"#,
                "51.5072° N, 0.1276° W",
            ),
            (
                r#"{"type":"LineString","coordinates":[[0,0],[1,1]]}"#,
                "(0°, 0°; 1° N, 1° E)",
            ),
            (
                r#"{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]]]}"#,
                "((0°, 0°; 0°, 1° E; 1° N, 1° E; 1° N, 0°; 0°, 0°))",
            ),
            (
                r#"{"type":"MultiPoint","coordinates":[[0,0],[1,1]]}"#,
                "(0°, 0°; 1° N, 1° E)",
            ),
            (
                r#"{"type":"MultiLineString","coordinates":[[[0,0],[1,1]]]}"#,
                "((0°, 0°; 1° N, 1° E))",
            ),
            (
                r#"{"type":"MultiPolygon","coordinates":[[[[0,0],[1,0],[1,1],[0,1],[0,0]]]]}"#,
                "(((0°, 0°; 0°, 1° E; 1° N, 1° E; 1° N, 0°; 0°, 0°)))",
            ),
        ],
        &[
            // A collection, a feature, no such geometry, area 2's tagged
            // form.
            r#"{"type":"GeometryCollection","geometries":[]}"#,
            r#"{"type":"Feature","geometry":{"type":"Point","coordinates":[0,0]},"properties":null}"#,
            r#"{"type":"Circle","coordinates":[0,0]}"#,
            r#"{"tag":"point","point":{"type":"Point","coordinates":[0,0]}}"#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `BoundingBox`'s sides by name, as RFC 7946 §5 defines them: its south
/// at or below its north; a west east of its east crosses the antimeridian
/// (§5.2).
pub const BOUNDING_BOX_FIELDS: [(&str, Part); 4] = [
    ("south", LATITUDE_PART),
    ("west", LONGITUDE_PART),
    ("north", LATITUDE_PART),
    ("east", LONGITUDE_PART),
];

/// The rule across a bounding box's sides.
pub const BOUNDING_BOX_RULES: &[StandardRule] = &[StandardRule {
    name: "south_not_above_north",
    written: "south <= north",
    label: "south of the south",
    reason: "A `BoundingBox`'s `north` is its northernmost latitude, so it is at or above its `south` (RFC 7946, §5).",
    next: "Write a `north` at or above the `south`",
    check: in_order(&[Order::at_most("south", "north")], "north"),
}];

/// `MapTile`'s rule across its parts (chunk 5.5t2): its column and row
/// each below 2 to its zoom.
pub const MAP_TILE_RULES: &[StandardRule] = &[StandardRule {
    name: "within_zoom",
    written: "x and y are each below 2 to the zoom",
    label: "past its zoom's tiles",
    reason: "A `MapTile` at zoom `z` is one of 2 to the `z` columns and as many rows, so its `x` and `y` are each below 2 to the `z` (the XYZ tile scheme).",
    next: "Write a column and a row below 2 to the zoom, or a deeper zoom",
    check: Check::By(|parts| {
        let whole = |name: &str| parts.part(name).value().and_then(|n| n.whole());
        let Some(tiles) = whole("zoom")
            .and_then(|zoom| u32::try_from(zoom).ok())
            .and_then(|zoom| 1i64.checked_shl(zoom))
        else {
            return Verdict::Unknown;
        };
        let mut verdict = Verdict::Holds;
        for name in ["x", "y"] {
            match whole(name) {
                Some(n) if n >= tiles => {
                    return Verdict::Breaks(if name == "x" { "x" } else { "y" });
                }
                Some(_) => {}
                None => verdict = Verdict::Unknown,
            }
        }
        verdict
    }),
}];

/// A rectangle on the map by its four sides, named, never RFC 7946's
/// positional `bbox` array, and shown from its south-west corner to its
/// north-east one.
pub const BOUNDING_BOX_ENTRY: StandardType = StandardType {
    name: BOUNDING_BOX,
    holds: Holds::Object(&BOUNDING_BOX_FIELDS),
    text: None,
    json: Json::Fields(&["south", "west", "north", "east"]),
    storage: Storage::Geography,
    format: GEOMETRY_FORMAT,
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"south":"51.28","west":"-0.51","north":"51.69","east":"0.33"}"#,
                "51.28° N, 0.51° W – 51.69° N, 0.33° E",
            ),
            (
                r#"{"south":"-20","west":"170","north":"-10","east":"-170"}"#,
                "20° S, 170° E – 10° S, 170° W",
            ),
        ],
        &[
            // Its south north of its north, a latitude out of range, a
            // side left out, RFC 7946's positional array.
            r#"{"south":"51.69","west":"-0.51","north":"51.28","east":"0.33"}"#,
            r#"{"south":"51.28","west":"-0.51","north":"91","east":"0.33"}"#,
            r#"{"south":"51.28","west":"-0.51","north":"51.69"}"#,
            "[-0.51,51.28,0.33,51.69]",
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// `Circle`'s parts: a centre, and a radius never below 0.
pub const CIRCLE_FIELDS: [(&str, Part); 2] = [
    ("centre", Part::Standard(POINT)),
    (
        "radius",
        Part::Bounded {
            of: Builtin::Quantity(QuantityKind::Length),
            min: Some(whole(0)),
            max: None,
        },
    ),
];

/// A centre and a radius on the ground, sent as named fields, its centre a
/// GeoJSON `Point`, and shown as the radius around the centre.
pub const CIRCLE_ENTRY: StandardType = StandardType {
    name: CIRCLE,
    holds: Holds::Object(&CIRCLE_FIELDS),
    text: None,
    json: Json::Fields(&["centre", "radius"]),
    storage: Storage::Geography,
    format: Format {
        key: FormatKey::Parts {
            parts: &["radius", "centre"],
            separator: " around ",
        },
        options: NO_OPTIONS,
    },
    ordered: false,
    validator: None,
    shape: None,
    examples: examples(
        &[
            (
                r#"{"centre":{"type":"Point","coordinates":[-0.1276,51.5072]},"radius":{"value":"1000","unit":"metre"}}"#,
                "1,000 m around 51.5072° N, 0.1276° W",
            ),
            (
                r#"{"centre":{"type":"Point","coordinates":[0,0]},"radius":{"value":"0","unit":"metre"}}"#,
                "0 m around 0°, 0°",
            ),
        ],
        &[
            // A radius below 0, a centre by its names, a radius left out.
            r#"{"centre":{"type":"Point","coordinates":[0,0]},"radius":{"value":"-1","unit":"metre"}}"#,
            r#"{"centre":{"latitude":"0","longitude":"0"},"radius":{"value":"1","unit":"metre"}}"#,
            r#"{"centre":{"type":"Point","coordinates":[0,0]}}"#,
        ],
    ),
    release: FIRST_RELEASE,
    signals: crate::standard::NO_SIGNALS,
};

/// The GeoJSON geometry `type` a standard type is sent as, for the five
/// that are one; `Geometry` is sent as its variant's.
pub fn geojson_type(name: &str) -> Option<&'static str> {
    GEOJSON_TYPES
        .iter()
        .map(|(_, ty)| *ty)
        .find(|ty| *ty == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn h3_cells_are_checked_bit_by_bit() {
        for valid in [
            "8001fffffffffff",
            "8009fffffffffff",
            "80f3fffffffffff",
            "8108bffffffffff",
        ] {
            assert!(is_h3_cell(valid), "{valid}");
        }
        for invalid in [
            "80f5fffffffffff",
            "81087ffffffffff",
            "8009ffffffffff",
            "9001fffffffffff",
        ] {
            assert!(!is_h3_cell(invalid), "{invalid}");
        }
    }

    #[test]
    fn every_geometry_has_its_geojson_type() {
        for (variant, ty) in GEOJSON_TYPES {
            assert!(GEOMETRY_VARIANTS.iter().any(|(name, _)| *name == variant));
            assert_eq!(geojson_type(ty), Some(ty));
        }
        assert_eq!(geojson_type(GEOMETRY), None);
    }
}
