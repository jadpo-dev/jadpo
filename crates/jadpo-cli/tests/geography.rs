//! Geometries in PostGIS (area 20, "Storage"; chunk 5.5u4), against live
//! PostgreSQL with PostGIS: an entity stores and reads back every
//! geometry, with and without altitude, a `Geometry` of each kind, a
//! `BoundingBox` and a `Circle`, unchanged, its first migration creating
//! the extension and each `geography` column held to its type and to
//! WGS84 by a check; an application without geometry never creates it;
//! and `jadpo migrate apply` refuses, before anything runs, a database
//! whose server doesn't offer PostGIS or whose role can't create it.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools, which must have PostGIS (CI's
//! image does, `tools/postgres/Dockerfile`; so does Postgres.app);
//! `JADPO_SKIP_POSTGRES=1` skips the live part.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_runtime::{Database, SqlValue};
use jadpo_term::Environment;

use crate::support;

fn skipped() -> bool {
    let skip = std::env::var("JADPO_SKIP_POSTGRES").is_ok_and(|value| value == "1");
    if skip {
        println!("skipped: JADPO_SKIP_POSTGRES is set");
    }
    skip
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    let mut output = run(&args, &env.with_cwd(cwd));
    output.stdout = support::without_run_time(&output.stdout);
    output
}

/// `text` as a Jadpo string literal.
fn literal(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Each geometry's GeoJSON, with every position at altitude `z` (metres)
/// or none: a point, a line, a polygon with a hole, and the three
/// multi-geometries.
fn shapes(z: Option<&str>) -> [(&'static str, String); 6] {
    let p = |lon: &str, lat: &str| match z {
        Some(alt) => format!("[{lon},{lat},{alt}]"),
        None => format!("[{lon},{lat}]"),
    };
    let ring = format!(
        "[{},{},{},{},{}]",
        p("-0.2", "51.49"),
        p("-0.05", "51.49"),
        p("-0.05", "51.54"),
        p("-0.2", "51.54"),
        p("-0.2", "51.49")
    );
    let hole = format!(
        "[{},{},{},{},{}]",
        p("-0.15", "51.5"),
        p("-0.15", "51.52"),
        p("-0.1", "51.52"),
        p("-0.1", "51.5"),
        p("-0.15", "51.5")
    );
    let line = format!(
        "[{},{},{}]",
        p("-0.1276", "51.5072"),
        p("2.3522", "48.8566"),
        p("151.2093", "-33.8688")
    );
    let geojson =
        |ty: &str, coordinates: String| format!(r#"{{"type":"{ty}","coordinates":{coordinates}}}"#);
    [
        ("Point", geojson("Point", p("-0.1276", "51.5072"))),
        ("LineString", geojson("LineString", line.clone())),
        ("Polygon", geojson("Polygon", format!("[{ring},{hole}]"))),
        (
            "MultiPoint",
            geojson(
                "MultiPoint",
                format!("[{},{}]", p("0", "0"), p("-180", "-90")),
            ),
        ),
        (
            "MultiLineString",
            geojson("MultiLineString", format!("[{line},{line}]")),
        ),
        (
            "MultiPolygon",
            geojson("MultiPolygon", format!("[[{ring},{hole}],[{ring}]]")),
        ),
    ]
}

/// A project whose entity holds every geometry, a `Geometry`, a
/// `BoundingBox` and a `Circle`, with a test that stores them with and
/// without altitude, and a `Geometry` of each kind, and reads them back.
fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("geography-{name}"));
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/maps/failures/bad_example.jadpo",
        "/// An example that isn't its type.\ntype BadExample = Failure {\n    kind: FailureKind.invalid_value\n}\n",
    );
    write(
        &root,
        "domains/maps/entities/survey.jadpo",
        "/// Every geometry, kept.\nentity Survey {\n    id: Uuid\n    point: Point\n    line: LineString\n    polygon: Polygon\n    points: MultiPoint\n    lines: MultiLineString\n    polygons: MultiPolygon\n    shape: Geometry\n    view: BoundingBox\n    reach: Circle\n    maybe_point: Point?\n    maybe_reach: Circle?\n\n    indexes: {\n        by_point: {\n            fields: [point]\n            kind: IndexKind.geographic\n        }\n    }\n}\n",
    );
    write(
        &root,
        "domains/maps/actions/keep_survey.jadpo",
        "use maps/entities { Survey }\n\n/// Keeps a survey's geometries.\naction keep_survey(point: Point, line: LineString, polygon: Polygon, points: MultiPoint, lines: MultiLineString, polygons: MultiPolygon, shape: Geometry, view: BoundingBox, reach: Circle, maybe_point: Point?, maybe_reach: Circle?) -> Survey {\n    return Survey.create {\n        set: {\n            point: point\n            line: line\n            polygon: polygon\n            points: points\n            lines: lines\n            polygons: polygons\n            shape: shape\n            view: view\n            reach: reach\n            maybe_point: maybe_point\n            maybe_reach: maybe_reach\n        }\n    }\n}\n",
    );
    let read = |var: &str, ty: &str, json: &str| {
        format!(
            "    var {var}: {ty} = attempt json.read({}, as: {ty}) {{\n        malformed: BadExample\n        mismatch: BadExample\n    }}\n",
            literal(json)
        )
    };
    let mut body = String::new();
    for (tag, z) in [("flat", None), ("high", Some("8848.86"))] {
        for (ty, json) in shapes(z) {
            body.push_str(&read(&format!("{tag}_{}", ty.to_lowercase()), ty, &json));
        }
        let centre = match z {
            Some(alt) => format!(r#"{{"type":"Point","coordinates":[-0.1276,51.5072,{alt}]}}"#),
            None => r#"{"type":"Point","coordinates":[-0.1276,51.5072]}"#.to_string(),
        };
        body.push_str(&read(
            &format!("{tag}_reach"),
            "Circle",
            &format!(r#"{{"centre":{centre},"radius":{{"value":"1500.25","unit":"metre"}}}}"#),
        ));
    }
    body.push_str(&read(
        "view",
        "BoundingBox",
        r#"{"south":"-20","west":"170.123456789012","north":"-10","east":"-170"}"#,
    ));
    for (tag, z) in [("flat", None), ("high", Some("8848.86"))] {
        let v = |ty: &str| format!("{tag}_{}", ty.to_lowercase());
        body.push_str(&format!(
            "    var kept_{tag}: Survey = keep_survey({}, {}, {}, {}, {}, {}, Geometry.polygon {{\n        polygon: {}\n    }}, view, {tag}_reach, {}, {tag}_reach)\n",
            v("Point"),
            v("LineString"),
            v("Polygon"),
            v("MultiPoint"),
            v("MultiLineString"),
            v("MultiPolygon"),
            v("Polygon"),
            v("Point"),
        ));
        body.push_str(&format!(
            "    var back_{tag}: Survey = attempt Survey.get(kept_{tag}.id)\n    assert back_{tag} == kept_{tag}\n    assert back_{tag}.point == {}\n    assert back_{tag}.line == {}\n    assert back_{tag}.polygon == {}\n    assert back_{tag}.points == {}\n    assert back_{tag}.lines == {}\n    assert back_{tag}.polygons == {}\n    assert back_{tag}.view == view\n    assert back_{tag}.reach == {tag}_reach\n    assert back_{tag}.maybe_point == {}\n",
            v("Point"),
            v("LineString"),
            v("Polygon"),
            v("MultiPoint"),
            v("MultiLineString"),
            v("MultiPolygon"),
            v("Point"),
        ));
        let _ = z;
    }
    // A `Geometry` of each kind, and the optional fields left out.
    for (at, ty) in [
        ("point", "Point"),
        ("line_string", "LineString"),
        ("multi_point", "MultiPoint"),
        ("multi_line_string", "MultiLineString"),
        ("multi_polygon", "MultiPolygon"),
    ] {
        let value = format!("high_{}", ty.to_lowercase());
        body.push_str(&format!(
            "    var as_{at}: Survey = keep_survey(flat_point, flat_linestring, flat_polygon, flat_multipoint, flat_multilinestring, flat_multipolygon, Geometry.{at} {{\n        {at}: {value}\n    }}, view, flat_reach, none, none)\n    var as_{at}_back: Survey = attempt Survey.get(as_{at}.id)\n    assert as_{at}_back.shape == Geometry.{at} {{\n        {at}: {value}\n    }}\n    assert as_{at}_back.maybe_point == none\n    assert as_{at}_back.maybe_reach == none\n",
        ));
    }
    write(
        &root,
        "domains/maps/actions/keep_survey.test.jadpo",
        &format!(
            "use maps/entities {{ Survey }}\nuse maps/failures {{ BadExample }}\n\ntest \"every geometry is stored and read back unchanged, with and without altitude\" {{\n{body}}}\n\ntest \"a ring wound clockwise is kept as it was given\" {{\n    var clockwise: Polygon = Polygon {{\n        outer: [\n            Point {{\n                latitude: 0.0\n                longitude: 0.0\n                altitude: none\n            }},\n            Point {{\n                latitude: 1.0\n                longitude: 0.0\n                altitude: none\n            }},\n            Point {{\n                latitude: 1.0\n                longitude: 1.0\n                altitude: none\n            }},\n            Point {{\n                latitude: 0.0\n                longitude: 0.0\n                altitude: none\n            }}\n        ]\n        holes: []\n    }}\n    var view: BoundingBox = attempt json.read({view}, as: BoundingBox) {{\n        malformed: BadExample\n        mismatch: BadExample\n    }}\n    var reach: Circle = attempt json.read({reach}, as: Circle) {{\n        malformed: BadExample\n        mismatch: BadExample\n    }}\n    var corner: Point = Point {{\n        latitude: 1.0\n        longitude: 0.0\n        altitude: none\n    }}\n    var round: LineString = LineString {{\n        points: clockwise.outer\n    }}\n    var kept: Survey = keep_survey(corner, round, clockwise, MultiPoint {{\n        points: clockwise.outer\n    }}, MultiLineString {{\n        lines: [round]\n    }}, MultiPolygon {{\n        polygons: [clockwise]\n    }}, Geometry.polygon {{\n        polygon: clockwise\n    }}, view, reach, none, none)\n    var back: Survey = attempt Survey.get(kept.id)\n    assert back.polygon == clockwise\n    assert back.polygons == MultiPolygon {{\n        polygons: [clockwise]\n    }}\n}}\n",
            view = literal(r#"{"south":"0","west":"0","north":"1","east":"1"}"#),
            reach = literal(
                r#"{"centre":{"type":"Point","coordinates":[0,0]},"radius":{"value":"0","unit":"metre"}}"#
            ),
        ),
    );
    root
}

/// The one migration's SQL.
fn migration_sql(root: &Path) -> (String, PathBuf) {
    let folder = fs::read_dir(root.join("database/migrations"))
        .expect("migrations")
        .map(|entry| entry.expect("entry").path())
        .next()
        .expect("a migration");
    let path = folder.join("postgres.sql");
    (fs::read_to_string(&path).expect("postgres.sql"), path)
}

#[test]
fn every_geometry_is_stored_and_read_back_unchanged() {
    let root = project("stored");
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let (sql, _) = migration_sql(&root);
    // PostGIS first; each geometry an untyped `geography` held to its type
    // and to WGS84; a box's sides and a circle's radius in named columns,
    // its centre a point's `geography`.
    assert!(
        sql.contains("-- store: main\nCREATE EXTENSION IF NOT EXISTS \"postgis\";\nCREATE TABLE "),
        "{sql}"
    );
    for expected in [
        "\"point\" geography NOT NULL",
        "\"polygons\" geography NOT NULL",
        "\"shape\" geography NOT NULL",
        "\"maybe_point\" geography,",
        "\"view_south\" numeric NOT NULL",
        "\"view_west\" numeric NOT NULL",
        "\"view_north\" numeric NOT NULL",
        "\"view_east\" numeric NOT NULL",
        "\"reach_centre\" geography NOT NULL",
        "\"reach_radius\" numeric(38, 6) NOT NULL",
        "\"maybe_reach_centre\" geography,",
        "ALTER TABLE \"survey\" ADD CONSTRAINT \"survey_point_check\" CHECK (ST_SRID(\"point\") = 4326 AND GeometryType(\"point\") = 'POINT');",
        "ALTER TABLE \"survey\" ADD CONSTRAINT \"survey_lines_check\" CHECK (ST_SRID(\"lines\") = 4326 AND GeometryType(\"lines\") = 'MULTILINESTRING');",
        "ALTER TABLE \"survey\" ADD CONSTRAINT \"survey_shape_check\" CHECK (ST_SRID(\"shape\") = 4326 AND GeometryType(\"shape\") IN ('POINT', 'LINESTRING', 'POLYGON', 'MULTIPOINT', 'MULTILINESTRING', 'MULTIPOLYGON'));",
        "ALTER TABLE \"survey\" ADD CONSTRAINT \"survey_reach_centre_check\" CHECK (ST_SRID(\"reach_centre\") = 4326 AND GeometryType(\"reach_centre\") = 'POINT');",
        "CREATE INDEX \"survey_by_point_idx\" ON \"survey\" USING gist (\"point\");",
    ] {
        assert!(sql.contains(expected), "`{expected}` in:\n{sql}");
    }
    if skipped() {
        return;
    }
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("2 passed"), "{}", tested.stdout);
}

#[test]
fn an_application_without_geometry_never_creates_postgis() {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("geography-none");
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/maps/entities/place_name.jadpo",
        "/// A place by name, its position in two numbers.\nentity PlaceName {\n    id: Uuid\n    latitude: Latitude\n    longitude: Longitude\n}\n",
    );
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let (sql, _) = migration_sql(&root);
    assert!(!sql.contains("EXTENSION"), "{sql}");
}

/// Runs `sql` with `params` on the database at `url`, committed.
fn rows(url: &str, sql: &str, params: &[SqlValue]) -> Result<Vec<Vec<SqlValue>>, String> {
    let store = jadpo_host_native::postgres(url).map_err(|e| format!("{e:?}"))?;
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let mut transaction = store.begin().await.map_err(|e| format!("{e:?}"))?;
        let found = transaction.query(sql, params).await;
        match found {
            Ok(found) => {
                transaction.commit().await.map_err(|e| format!("{e:?}"))?;
                Ok(found)
            }
            Err(error) => {
                let _ = transaction.rollback().await;
                Err(format!("{error:?}"))
            }
        }
    })
}

#[test]
fn each_column_s_check_holds_it_to_its_type_and_wgs84() {
    if skipped() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server
        .create_database("geography_checks")
        .expect("database");
    let root = project("checks");
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    let insert = |point: &str| {
        rows(
            &url,
            &format!(
                "INSERT INTO \"survey\" (\"id\", \"point\", \"line\", \"polygon\", \"points\", \"lines\", \"polygons\", \"shape\", \"view_south\", \"view_west\", \"view_north\", \"view_east\", \"reach_centre\", \"reach_radius\") VALUES (gen_random_uuid(), '{point}', 'LINESTRING(0 0, 1 1)', 'POLYGON((0 0, 1 0, 1 1, 0 0))', 'MULTIPOINT(0 0)', 'MULTILINESTRING((0 0, 1 1))', 'MULTIPOLYGON(((0 0, 1 0, 1 1, 0 0)))', 'POINT(0 0)', 0, 0, 1, 1, 'POINT(0 0)', 5)"
            ),
            &[],
        )
    };
    // A point, with an altitude or without, goes in; a line in a point's
    // column, or a point in ETRS89 (SRID 4258), doesn't.
    insert("SRID=4326;POINT(-0.1276 51.5072)").expect("a point");
    insert("SRID=4326;POINT Z(-0.1276 51.5072 35)").expect("a point with an altitude");
    let refused = insert("LINESTRING(0 0, 1 1)").expect_err("a line in a point's column");
    assert!(refused.contains("survey_point_check"), "{refused}");
    let refused = insert("SRID=4258;POINT(1 2)").expect_err("a point in ETRS89");
    assert!(refused.contains("survey_point_check"), "{refused}");
    // The declared geographic index serves PostGIS's distance test.
    let planned = jadpo_host_native::Runtime::new().expect("runtime").block_on(async {
        let store = jadpo_host_native::postgres(&url).expect("store");
        let mut transaction = store.begin().await.expect("begin");
        transaction.batch("SET LOCAL enable_seqscan = off").await.expect("set");
        let found = transaction
            .query(
                "EXPLAIN SELECT \"id\" FROM \"survey\" WHERE ST_DWithin(\"point\", 'SRID=4326;POINT(-0.1276 51.5072)'::geography, 1000)",
                &[],
            )
            .await
            .expect("explained");
        let _ = transaction.rollback().await;
        found
    });
    let text = format!("{planned:?}");
    assert!(text.contains("survey_by_point_idx"), "{text}");
}

#[test]
fn a_database_without_postgis_is_refused_before_anything_runs() {
    if skipped() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("postgis_missing").expect("database");
    let root = project("missing");
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    // A server whose PostGIS is missing, as a name no server offers.
    let (sql, path) = migration_sql(&root);
    fs::write(
        &path,
        sql.replace(
            "CREATE EXTENSION IF NOT EXISTS \"postgis\"",
            "CREATE EXTENSION IF NOT EXISTS \"jadpo_missing_postgis\"",
        ),
    )
    .expect("write");
    let refused = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert!(
        refused
            .stderr
            .starts_with("error: `main` has no `jadpo_missing_postgis` extension\n"),
        "{}",
        refused.stderr
    );
    let status = jadpo(&root, &["migrate", "status", "--url", &url]);
    assert!(
        status.stdout.contains("`main`: 0 applied, 1 pending"),
        "{}",
        status.stdout
    );
    // PostGIS itself is offered, and its migration creates it.
    fs::write(&path, sql).expect("write");
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    let installed = rows(
        &url,
        "SELECT extname FROM pg_extension WHERE extname = 'postgis'",
        &[],
    );
    assert_eq!(installed, Ok(vec![vec![SqlValue::Text("postgis".into())]]));
}

/// `url` connecting as `role` with `password` instead of its own user.
fn as_role(url: &str, role: &str, password: &str) -> String {
    if url.contains("://") {
        let joined = if url.contains('?') { '&' } else { '?' };
        format!("{url}{joined}user={role}&password={password}")
    } else {
        format!("{url} user={role} password={password}")
    }
}

#[test]
fn a_role_that_can_t_create_postgis_is_refused_before_anything_runs() {
    if skipped() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("postgis_role").expect("database");
    let database = rows(&url, "SELECT current_database()", &[]).expect("its name");
    let Some(Some(SqlValue::Text(database))) = database.first().map(|row| row.first()) else {
        panic!("{database:?}");
    };
    // A role of its own, owning the database, and no superuser.
    let role = format!("{database}_owner");
    server
        .admin(&format!(
            "CREATE ROLE \"{role}\" LOGIN PASSWORD 'jadpo_5_5u4'; ALTER DATABASE \"{database}\" OWNER TO \"{role}\""
        ))
        .expect("a role");
    let plain = as_role(&url, &role, "jadpo_5_5u4");
    let root = project("role");
    assert_eq!(jadpo(&root, &["migrate", "new", "initial"]).code, 0);
    let refused = jadpo(&root, &["migrate", "apply", "--url", &plain]);
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert_eq!(
        refused.stderr,
        "error: `main` has no `postgis` extension, and its role can't create it\n  reason: `postgis` is PostGIS, which keeps the project's geometries in `geography` columns; there is no fallback, since a geometry kept any other way would be measured and compared differently. The project's migrations create it, and its database server offers it, but the role applying migrations isn't allowed to create it, so nothing was applied. PostGIS isn't a trusted extension: a superuser, or the provider's own admin role, creates it.\n  next: Have a role allowed to create it run `CREATE EXTENSION postgis` once in this database, then run the command again (human)\n  rule: migration.extension_missing\n  help: docs/language/14-tooling.html#command-errors\n"
    );
    let tables = rows(
        &url,
        "SELECT count(*)::int FROM pg_tables WHERE schemaname = 'public'",
        &[],
    );
    assert_eq!(tables, Ok(vec![vec![SqlValue::Int4(0)]]));
    // Once a role allowed to create it has, the plain role applies them.
    rows(&url, "CREATE EXTENSION postgis", &[]).expect("created");
    let applied = jadpo(&root, &["migrate", "apply", "--url", &plain]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    server
        .admin(&format!("DROP DATABASE \"{database}\" WITH (FORCE)"))
        .expect("dropped");
    server
        .admin(&format!("DROP ROLE \"{role}\""))
        .expect("dropped");
}

/// WKT through PostGIS (area 20, "Interop"; chunk 5.5u6): each geometry,
/// a patch with a hole among them, written by `geo.to_wkt`'s writer, is
/// read by `ST_GeogFromText`, as WKT and as EWKT, and written back by
/// `ST_AsText` exactly as Jadpo wrote it, and Jadpo's reader gives back the
/// value it started from: longitude first both ways, rings in the order
/// given, every coordinate the double it was.
#[test]
fn a_wkt_polygon_round_trips_through_postgis_unchanged() {
    if skipped() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server.create_database("geography_wkt").expect("database");
    rows(&url, "CREATE EXTENSION IF NOT EXISTS postgis", &[]).expect("PostGIS");
    let patch = "POLYGON((-0.2 51.49,-0.05 51.49,-0.05 51.54,-0.2 51.54,-0.2 51.49),(-0.15 51.5,-0.15 51.52,-0.1 51.52,-0.1 51.5,-0.15 51.5))";
    for wkt in [
        patch,
        // The same patch wound the other way, kept as it was given.
        "POLYGON((-0.2 51.49,-0.2 51.54,-0.05 51.54,-0.05 51.49,-0.2 51.49))",
        "POLYGON Z ((-0.2 51.49 35.5,-0.05 51.49 35.5,-0.05 51.54 35.5,-0.2 51.54 35.5,-0.2 51.49 35.5))",
        "POINT(-0.1276 51.50720000001235)",
        "POINT Z (151.2093 -33.8688 -12.000001)",
        "LINESTRING(-0.1276 51.5072,2.3522 48.8566,151.2093 -33.8688)",
        "MULTIPOINT((0 0),(-180 -90))",
        "MULTILINESTRING((0 0,1 1),(2 2,3 3))",
        "MULTIPOLYGON(((0 0,1 0,1 1,0 0)),((2 2,3 2,3 3,2 2)))",
    ] {
        let value = jadpo_value::geo::wkt::read(wkt).unwrap_or_else(|e| panic!("{wkt}: {e:?}"));
        let written = jadpo_value::geo::wkt::write(&value).expect("WKT");
        assert_eq!(written, wkt);
        for sql in [
            "SELECT ST_AsText(ST_GeogFromText($1))",
            "SELECT ST_AsText(ST_GeogFromText('SRID=4326;' || $1))",
        ] {
            let back = rows(&url, sql, &[SqlValue::Text(written.clone())])
                .unwrap_or_else(|e| panic!("{wkt}: {e}"));
            let Some(Some(SqlValue::Text(back))) = back.first().map(|row| row.first()) else {
                panic!("{wkt}: {back:?}");
            };
            assert_eq!(back, &written, "{sql}");
            assert_eq!(jadpo_value::geo::wkt::read(back).as_ref(), Ok(&value));
        }
    }
}
