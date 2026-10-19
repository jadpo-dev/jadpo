//! Area 20's interop (chunk 5.5u6), in the checker: an application's own
//! list of points is the standard `LineString` or `Polygon`, unless
//! declined; `geo.from_wkt` gives a `Geometry` and fails `malformed` or
//! `out_of_range`, which its `attempt` maps; `geo.to_wkt` writes any
//! geometry and nothing else.

use crate::support;

use jadpo_db::Layout;

const NAMES: &str = "type Exported = Text {}\n\ntype Unreadable = Failure {\n    kind: FailureKind.invalid_value\n}\n\n";

fn rules(source: &str) -> Vec<String> {
    support::rules(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

fn problems(source: &str) -> Vec<String> {
    support::problems(
        Layout::Loose,
        &[("main.jadpo", &format!("{NAMES}{source}"))],
    )
}

const NONE: &[&str] = &[];

#[test]
fn an_application_s_own_list_of_points_is_a_line_string_or_a_polygon() {
    for (fields, extra) in [
        ("points: List<Point>", ""),
        ("coordinates: List<Point>", ""),
        // A named list of points.
        (
            "points: Stops",
            "/// A van's stops.\ntype Stops = List<Point> {\n    min_length: 2\n}\n\n",
        ),
    ] {
        let source =
            format!("{extra}/// A van's round.\ntype VanRound = Object {{\n    {fields}\n}}\n");
        assert_eq!(rules(&source), ["type.standard_exists"], "{fields}");
        let found = problems(&source);
        assert!(
            found.iter().any(|problem| problem.contains(
                "`VanRound` is the standard `LineString` or `Polygon` under another name"
            )),
            "{found:?}"
        );
    }
    // A list of the application's own point: both are the standard types.
    let own = "/// Where a van was.\ntype VanPosition = Object {\n    lat: Decimal\n    lng: Decimal\n}\n\n/// A van's round.\ntype VanRound = Object {\n    points: List<VanPosition>\n}\n";
    assert_eq!(rules(own), ["type.standard_exists", "type.standard_exists"]);
    // Declined, with the reason review reads.
    assert_eq!(
        rules(
            "/// Pins a person dropped, in no order.\ndecline type.standard_exists: \"Pins in no order, not a path or a ring\"\ntype DroppedPins = Object {\n    points: List<Point>\n}\n"
        ),
        NONE
    );
}

#[test]
fn other_lists_are_other_shapes() {
    for fields in [
        // Another field beside the list.
        "points: List<Point>\n    name: Exported",
        // Not a list of points.
        "points: List<Decimal>",
        "points: Set<Point>",
        "points: List<Point?>",
        // Not one of the list's names.
        "stops: List<Point>",
        // Optional.
        "points: List<Point>?",
    ] {
        let source = format!("/// A van's round.\ntype VanRound = Object {{\n    {fields}\n}}\n");
        assert_eq!(rules(&source), NONE, "{fields}");
    }
}

#[test]
fn wkt_is_read_into_a_geometry_whose_conditions_are_mapped() {
    let read = |arms: &str| {
        rules(&format!(
            "function shape(cell: Exported) fails Unreadable -> Geometry {{\n    return attempt geo.from_wkt(cell) {{\n{arms}    }}\n}}\n"
        ))
    };
    assert_eq!(
        read("        malformed: Unreadable\n        out_of_range: Unreadable\n"),
        NONE
    );
    assert_eq!(read("        invalid: Unreadable\n"), NONE);
    assert_ne!(
        read("        malformed: Unreadable\n"),
        NONE,
        "out_of_range unmapped"
    );
    assert_ne!(
        rules("function shape(cell: Exported) -> Geometry {\n    return geo.from_wkt(cell)\n}\n"),
        NONE,
        "a call that can fail, outside `attempt`"
    );
    assert_ne!(
        rules(
            "function shape(cell: Exported) fails Unreadable -> Polygon {\n    return attempt geo.from_wkt(cell) {\n        invalid: Unreadable\n    }\n}\n"
        ),
        NONE,
        "a `Geometry`, not a `Polygon`"
    );
}

#[test]
fn any_geometry_is_written_as_wkt_and_nothing_else() {
    for ty in [
        "Point",
        "LineString",
        "Polygon",
        "MultiPoint",
        "MultiLineString",
        "MultiPolygon",
        "Geometry",
    ] {
        assert_eq!(
            rules(&format!(
                "function sent(shape: {ty}) -> Exported {{\n    return Exported(geo.to_wkt(shape))\n}}\n"
            )),
            NONE,
            "{ty}"
        );
    }
    for ty in ["BoundingBox", "Circle", "Exported"] {
        assert_eq!(
            rules(&format!(
                "function sent(shape: {ty}) -> Exported {{\n    return Exported(geo.to_wkt(shape))\n}}\n"
            )),
            ["type.mismatch"],
            "{ty}"
        );
    }
}

const LAYER: &str = "/// What a delivery patch is called.
type PatchName = Text {
    max_length: 80
}

/// What the map shows beside a patch.
type PatchProperties = Object {
    name: PatchName
    note: Exported?
}

/// One delivery patch, as the map gets it.
type DeliveryPatch = Object {
    name: PatchName
    outline: Polygon
}

/// The map's delivery layer, as a mapping library reads it.
function patch_layer(patches: List<DeliveryPatch>) -> FeatureCollection<Polygon, PatchProperties> {
    return FeatureCollection {
        features: list.map(patches, patch -> Feature {
            geometry: patch.outline
            properties: PatchProperties {
                name: patch.name
                note: none
            }
        })
    }
}

";

#[test]
fn a_feature_collection_is_built_and_read_by_its_fields() {
    assert_eq!(rules(LAYER), NONE);
    let read = format!(
        "{LAYER}/// The first patch's name on the map.\nfunction names(layer: FeatureCollection<Polygon, PatchProperties>) -> List<PatchName> {{\n    return list.map(layer.features, feature -> feature.properties.name)\n}}\n\n/// Every outline on the map.\nfunction outlines(layer: FeatureCollection<Polygon, PatchProperties>) -> List<Polygon> {{\n    return list.map(layer.features, feature -> feature.geometry)\n}}\n"
    );
    assert_eq!(rules(&read), NONE);
    // A feature built where its type is known, and one whose geometry is
    // any geometry.
    let known = format!(
        "{LAYER}/// A patch as a feature of any geometry.\nfunction any_feature(patch: DeliveryPatch) -> Feature<Geometry, PatchProperties> {{\n    return Feature {{\n        geometry: Geometry.polygon {{\n            polygon: patch.outline\n        }}\n        properties: PatchProperties {{\n            name: patch.name\n            note: none\n        }}\n    }}\n}}\n"
    );
    assert_eq!(rules(&known), NONE);
    // A field a feature hasn't, and an `id`, which properties carry.
    let id = format!(
        "{LAYER}/// A patch's feature with an id.\nfunction with_id(patch: DeliveryPatch) -> Feature<Polygon, PatchProperties> {{\n    return Feature {{\n        id: 7\n        geometry: patch.outline\n        properties: PatchProperties {{\n            name: patch.name\n            note: none\n        }}\n    }}\n}}\n"
    );
    assert_eq!(rules(&id), ["type.unknown_field"]);
    let unknown = format!(
        "{LAYER}/// A feature's id.\nfunction id_of(feature: Feature<Polygon, PatchProperties>) -> Exported {{\n    return feature.id\n}}\n"
    );
    assert_eq!(rules(&unknown), ["type.unknown_field"]);
}

#[test]
fn a_feature_s_type_arguments_are_a_geometry_and_an_object() {
    let typed = |ty: &str| {
        rules(&format!(
            "{LAYER}/// The layer as it arrives.\nfunction layer(given: {ty}) -> {ty} {{\n    return given\n}}\n"
        ))
    };
    assert_eq!(typed("Feature<Point, PatchProperties>"), NONE);
    assert_eq!(typed("FeatureCollection<Geometry, PatchProperties>"), NONE);
    assert_eq!(
        typed("Feature<Polygon>"),
        ["type.type_argument_count", "type.type_argument_count"]
    );
    assert_eq!(
        typed("Feature<BoundingBox, PatchProperties>"),
        ["type.mismatch", "type.mismatch"]
    );
    assert_eq!(
        typed("Feature<Polygon, PatchName>"),
        ["type.mismatch", "type.mismatch"]
    );
    assert_eq!(
        typed("FeatureCollection<Polygon, Polygon>"),
        ["type.mismatch", "type.mismatch"]
    );
    // Built from values that aren't a geometry and an object.
    assert_eq!(
        rules(&format!(
            "{LAYER}/// A name as a feature.\nfunction named(patch: DeliveryPatch) -> Exported {{\n    return Exported(json.write(Feature {{\n        geometry: patch.name\n        properties: patch\n    }}))\n}}\n"
        )),
        ["type.mismatch"]
    );
}

#[test]
fn a_feature_crosses_as_json_and_has_no_text_or_column() {
    let source = format!(
        "{LAYER}/// The layer as the map is sent it.\nfunction sent(layer: FeatureCollection<Polygon, PatchProperties>) -> Exported {{\n    return Exported(json.write(layer))\n}}\n\n/// A layer the planner drew.\nfunction drawn(given: Exported) fails Unreadable -> FeatureCollection<Polygon, PatchProperties> {{\n    return attempt json.read(given, as: FeatureCollection<Polygon, PatchProperties>) {{\n        invalid: Unreadable\n    }}\n}}\n"
    );
    assert_eq!(rules(&source), NONE);
    assert_eq!(
        rules(&format!(
            "{LAYER}/// A layer as text.\nfunction shown(layer: FeatureCollection<Polygon, PatchProperties>) -> Exported {{\n    return Exported(text.from(layer))\n}}\n"
        )),
        ["type.mismatch"]
    );
    assert_eq!(
        rules(&format!(
            "{LAYER}/// A layer for people.\nfunction shown(layer: FeatureCollection<Polygon, PatchProperties>, reader: Locale) -> PresentationText {{\n    return text.format(layer, locale: reader)\n}}\n"
        )),
        ["type.mismatch"]
    );
    // Storing one is area 5's structured JSON (chunk 5.9b).
    let stored = support::rules(
        Layout::Project,
        &[
            ("app.jadpo", "application {}\n"),
            (
                "domains/maps/types/pin_properties.jadpo",
                "use maps/types { PinLabel }\n\n/// What the map shows beside a pin.\ntype PinProperties = Object {\n    label: PinLabel\n}\n",
            ),
            (
                "domains/maps/types/pin_label.jadpo",
                "/// A pin's label.\ntype PinLabel = Text {\n    max_length: 80\n}\n",
            ),
            (
                "domains/maps/entities/pin.jadpo",
                "use maps/types { PinProperties }\n\n/// A pin on the map, as a feature.\nentity Pin {\n    id: Uuid\n    feature: Feature<Point, PinProperties>\n}\n",
            ),
        ],
    );
    assert_eq!(stored, Vec::<String>::new());
}

/// A route sending a layer (area 20): the API record names it with its
/// type arguments, and OpenAPI describes RFC 7946's `FeatureCollection`
/// with each feature's geometry and properties.
#[test]
fn a_route_s_layer_is_described_as_geojson() {
    let files: &[(&str, &str)] = &[
        ("app.jadpo", "application {}\n"),
        (
            "domains/maps/types/pin_label.jadpo",
            "/// A pin's label.\ntype PinLabel = Text {\n    max_length: 80\n}\n",
        ),
        (
            "domains/maps/types/pin_properties.jadpo",
            "use maps/types { PinLabel }\n\n/// What the map shows beside a pin.\ntype PinProperties = Object {\n    label: PinLabel\n}\n",
        ),
        (
            "domains/maps/types/pin_layer.jadpo",
            "use maps/types { PinProperties }\n\n/// The pins, as the map draws them.\ntype PinLayer = Output {\n    layer: FeatureCollection<Point, PinProperties>\n}\n",
        ),
        (
            "domains/maps/actions/map_pins.jadpo",
            "use maps/types { PinLabel, PinLayer, PinProperties }\n\n/// The map's pins.\naction map_pins() -> PinLayer {\n    return PinLayer {\n        layer: FeatureCollection {\n        features: [\n            Feature {\n                geometry: Point {\n                    latitude: 51.5072\n                    longitude: -0.1276\n                    altitude: none\n                }\n                properties: PinProperties {\n                    label: PinLabel(\"Crumbs\")\n                }\n            }\n        ]\n    }\n    }\n}\n",
        ),
        (
            "domains/maps/routes/pins.jadpo",
            "use maps/actions { map_pins }\nuse maps/types { PinLayer }\n\n/// The map's layers.\nroutes Pins {\n    prefix: /pins\n\n    /// Every pin, as the map draws them.\n    POST / {\n        authentication: RouteAuthentication.none\n        output: PinLayer\n        run: map_pins()\n    }\n}\n",
        ),
    ];
    // The record and the policy are the project's own business here.
    assert_eq!(
        support::rules(Layout::Project, files),
        ["record.out_of_date", "policy.effect_ungranted"]
    );
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, files);
    let bodies = jadpo_check::routes::record::record_texts(&db, project);
    let record = bodies.get("main").expect("a record");
    assert!(
        record.contains("layer: FeatureCollection<Point, PinProperties>"),
        "{record}"
    );
    let document = jadpo_check::routes::openapi::document(&db, project, "main", "pins", "1")
        .expect("a document");
    let schema = document
        .get("components")
        .and_then(|components| components.get("schemas")?.get("PinLayer"))
        .expect("the layer's schema");
    let shown = format!("{schema:?}");
    for part in [
        "FeatureCollection",
        "features",
        "\"Feature\"",
        "coordinates",
        "PinProperties",
    ] {
        assert!(shown.contains(part), "{part}: {shown}");
    }
}

/// The files of a project with a route sending the map's pins as a layer
/// typed `output`, run by `map_pins`, whose properties are
/// `PinProperties` with `fields` beside its label.
fn layer_project(output: &str, fields: &str) -> Vec<(&'static str, String)> {
    vec![
        ("app.jadpo", "application {}\n".to_string()),
        (
            "domains/maps/types/pin_label.jadpo",
            "/// A pin's label.\ntype PinLabel = Text {\n    max_length: 80\n}\n".to_string(),
        ),
        (
            "domains/maps/types/pin_properties.jadpo",
            format!(
                "use maps/types {{ PinLabel }}\n\n/// What the map shows beside a pin.\ntype PinProperties = Object {{\n    label: PinLabel{fields}\n}}\n"
            ),
        ),
        (
            "domains/maps/actions/map_pins.jadpo",
            format!(
                "use maps/types {{ PinLabel, PinProperties }}\n\n/// The map's pins.\naction map_pins() -> {output} {{\n    return FeatureCollection {{\n        features: [\n            Feature {{\n                geometry: Point {{\n                    latitude: 51.5072\n                    longitude: -0.1276\n                    altitude: none\n                }}\n                properties: PinProperties {{\n                    label: PinLabel(\"Crumbs\")\n                }}\n            }}\n        ]\n    }}\n}}\n"
            ),
        ),
        (
            "domains/maps/routes/pins.jadpo",
            format!(
                "use maps/actions {{ map_pins }}\nuse maps/types {{ PinProperties }}\n\n/// The map's layers.\nroutes Pins {{\n    prefix: /pins\n\n    /// Every pin, as the map draws them.\n    POST / {{\n        authentication: RouteAuthentication.none\n        output: {output}\n        run: map_pins()\n    }}\n}}\n"
            ),
        ),
    ]
}

fn borrowed<'a>(files: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
    files
        .iter()
        .map(|(path, text)| (*path, text.as_str()))
        .collect()
}

/// A route sends a layer bare (area 7, "A map layer as output", chunk
/// 5.5u7), so a mapping library fetches it as the whole response: the API
/// record names it with its type arguments, and OpenAPI's response is RFC
/// 7946's `FeatureCollection`, not an object holding one.
#[test]
fn a_route_sends_a_layer_bare() {
    let files = layer_project("FeatureCollection<Point, PinProperties>", "");
    let files = borrowed(&files);
    // The record and the policy are the project's own business here.
    assert_eq!(
        support::rules(Layout::Project, &files),
        ["record.out_of_date", "policy.effect_ungranted"]
    );
    let db = jadpo_db::Database::default();
    let project = support::project(&db, Layout::Project, &files);
    let bodies = jadpo_check::routes::record::record_texts(&db, project);
    let record = bodies.get("main").expect("a record");
    assert!(
        record.contains("output FeatureCollection<Point, PinProperties>"),
        "{record}"
    );
    assert!(
        record.contains("object PinProperties\n    label: Text"),
        "{record}"
    );
    let document = jadpo_check::routes::openapi::document(&db, project, "main", "pins", "1")
        .expect("a document");
    let schema = document
        .get("paths")
        .and_then(|paths| {
            paths
                .get("/pins")?
                .get("post")?
                .get("responses")?
                .get("200")
        })
        .and_then(|ok| ok.get("content")?.get("application/json")?.get("schema"))
        .expect("the response's schema");
    let shown = format!("{schema:?}");
    for part in [
        "\"FeatureCollection\"",
        "features",
        "\"Feature\"",
        "coordinates",
        "PinProperties",
    ] {
        assert!(shown.contains(part), "{part}: {shown}");
    }
}

/// A layer's properties are held to what an `Output` may hold, so only
/// fields declared for clients leave the server; and only a collection is
/// sent bare, never a list of them or a lone feature.
#[test]
fn a_layer_sends_only_what_an_output_may() {
    for (output, fields, why) in [
        (
            "FeatureCollection<Point, PinProperties>",
            "\n    owner_hash: PasswordHash",
            "a password's hash",
        ),
        (
            "FeatureCollection<Point, PinProperties>",
            "\n    owner_password: Password",
            "a secret",
        ),
        (
            "FeatureCollection<Point, PinProperties>",
            "\n    owner: Principal.user",
            "a caller",
        ),
        ("Feature<Point, PinProperties>", "", "a lone feature"),
        (
            "List<FeatureCollection<Point, PinProperties>>",
            "",
            "a list of layers",
        ),
    ] {
        let files = layer_project(output, fields);
        let found = support::problems(Layout::Project, &borrowed(&files));
        assert!(
            found
                .iter()
                .any(|problem| problem.contains(" route.output_not_output: ")),
            "{why}: {found:#?}"
        );
    }
}

/// Inside an `Output`, a layer's properties are held to the same: what the
/// output holds is followed into them.
#[test]
fn an_output_holding_a_layer_holds_what_its_properties_hold() {
    let found = problems(
        "type PinProperties = Object {\n    owner_hash: PasswordHash\n}\n\ntype PinLayer = Output {\n    layer: FeatureCollection<Point, PinProperties>\n}\n",
    );
    assert!(
        found
            .iter()
            .any(|problem| problem.contains(" type.password_hash_exposed: ")),
        "{found:#?}"
    );
}

/// `geo.from_geojson` (area 19, "Shared vocabulary types"; area 20,
/// "Interop"; chunk 5.11): GeoJSON text read into the `Geometry` of the
/// kind it names, as `geo.from_wkt` reads WKT, failing `malformed` or
/// `out_of_range`, which its `attempt` maps.
#[test]
fn geojson_text_is_read_into_a_geometry_whose_conditions_are_mapped() {
    let read = |arms: &str| {
        rules(&format!(
            "function shape(cell: Exported) fails Unreadable -> Geometry {{\n    return attempt geo.from_geojson(cell) {{\n{arms}    }}\n}}\n"
        ))
    };
    assert_eq!(
        read("        malformed: Unreadable\n        out_of_range: Unreadable\n"),
        NONE
    );
    assert_eq!(read("        invalid: Unreadable\n"), NONE);
    assert_ne!(
        read("        malformed: Unreadable\n"),
        NONE,
        "out_of_range unmapped"
    );
    assert_ne!(
        read("        out_of_range: Unreadable\n"),
        NONE,
        "malformed unmapped"
    );
    assert_ne!(
        rules(
            "function shape(cell: Exported) -> Geometry {\n    return geo.from_geojson(cell)\n}\n"
        ),
        NONE,
        "a call that can fail, outside `attempt`"
    );
    assert_ne!(
        rules(
            "function shape(cell: Exported) fails Unreadable -> Polygon {\n    return attempt geo.from_geojson(cell) {\n        invalid: Unreadable\n    }\n}\n"
        ),
        NONE,
        "a `Geometry`, not a `Polygon`"
    );
    assert_eq!(
        rules(
            "type Count = Int {}\n\nfunction shape(cell: Count) fails Unreadable -> Geometry {\n    return attempt geo.from_geojson(cell) {\n        invalid: Unreadable\n    }\n}\n"
        ),
        ["type.mismatch"],
        "text only"
    );
}

const READINGS: &str =
    "type Reading = Decimal {\n    scale: 15\n}\n\ntype WholeReading = Int {}\n\n";

/// `geo.from_lng_lat` and `geo.from_lat_lng` (chunk 5.11): a pair, or a
/// triple with an altitude, in the order the name says, made a `Point`;
/// each fails `malformed` or `out_of_range`, which its `attempt` maps.
#[test]
fn a_pair_of_numbers_is_a_point_in_the_order_its_name_says() {
    for name in ["from_lng_lat", "from_lat_lng"] {
        let read = |given: &str, arms: &str| {
            rules(&format!(
                "{READINGS}function spot(pair: {given}) fails Unreadable -> Point {{\n    return attempt geo.{name}(pair) {{\n{arms}    }}\n}}\n"
            ))
        };
        let both = "        malformed: Unreadable\n        out_of_range: Unreadable\n";
        assert_eq!(read("List<Reading>", both), NONE, "{name}");
        assert_eq!(
            read("List<WholeReading>", both),
            NONE,
            "{name}: whole degrees"
        );
        assert_eq!(
            read("List<Reading>", "        invalid: Unreadable\n"),
            NONE,
            "{name}"
        );
        assert_ne!(
            read("List<Reading>", "        malformed: Unreadable\n"),
            NONE,
            "{name}: out_of_range unmapped"
        );
        assert_ne!(
            read("List<Reading>", "        out_of_range: Unreadable\n"),
            NONE,
            "{name}: malformed unmapped"
        );
        for given in ["List<Exported>", "Reading", "List<Reading?>"] {
            assert_eq!(
                read(given, both),
                ["type.mismatch"],
                "{name}: {given} isn't a list of numbers"
            );
        }
        // A list written out is attempted too: what it holds is known
        // only when the program runs, as a `Point` from values is.
        assert_eq!(
            rules(&format!(
                "function spot() fails Unreadable -> Point {{\n    return attempt geo.{name}([51.5072, -0.1276]) {{\n        invalid: Unreadable\n    }}\n}}\n"
            )),
            NONE,
            "{name}"
        );
        assert_ne!(
            rules(&format!(
                "{READINGS}function spot(pair: List<Reading>) -> Point {{\n    return geo.{name}(pair)\n}}\n"
            )),
            NONE,
            "{name}: a call that can fail, outside `attempt`"
        );
    }
}
