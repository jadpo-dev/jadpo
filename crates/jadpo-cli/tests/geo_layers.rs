//! A query giving a map layer (area 6, chunk 5.5u8), against live
//! PostgreSQL with PostGIS: a `GET` route running a query that returns a
//! `FeatureCollection<G, P>` answers with plain RFC 7946 GeoJSON, each
//! feature its record's geometry field and its properties the record's
//! fields `P` names; a layer query keeps area 20's `geo.` conditions and
//! its order by `geo.distance`; and the project's own route test, run by
//! `jadpo test`, reads the layer back.
//!
//! The server is the one `JADPO_TEST_DATABASE_URL` names or one started
//! from the machine's PostgreSQL tools, which must have PostGIS (CI's image
//! does, `tools/fixtures/postgres/Dockerfile`; so does Postgres.app);
//! `JADPO_SKIP_POSTGRES=1` skips the live part.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_interp::Program;
use jadpo_runtime::{Clock, Ctx, Database, Env, Executor, Outcome, SqlValue};
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::{Quantity, Value};

use crate::support;

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

/// A farm's drop points, a layer of them for the map behind `GET
/// /drop-points`, and a layer of those within a walk, nearest first, which
/// an action returns; a route test fetches the layer.
fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("geo-layers-{name}"));
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/maps/types/pin_name.jadpo",
        "/// A drop point's name on the map.\ntype PinName = Text {\n    max_length: 80\n}\n",
    );
    write(
        &root,
        "domains/maps/types/walk_reach.jadpo",
        "/// How far a customer walks.\ntype WalkReach = Length {\n    min: 0metre\n    max: 10km\n}\n",
    );
    write(
        &root,
        "domains/maps/entities/drop_point.jadpo",
        "use maps/types { PinName }\n\n/// Where boxes are left.\nentity DropPoint {\n    id: Uuid\n    name: PinName\n    spot: Point\n\n    policy: {\n        read: [Access.public]\n    }\n}\n",
    );
    write(
        &root,
        "domains/maps/types/pin.jadpo",
        "use maps/entities { DropPoint }\nuse maps/types { PinName }\n\n/// What a pin on the map says.\ntype Pin = Object {\n    id: DropPoint.id\n    name: PinName\n}\n",
    );
    write(
        &root,
        "domains/maps/queries/drop_point_layer.jadpo",
        "use maps/entities { DropPoint }\nuse maps/types { Pin }\n\n/// Every drop point, as the map's layer.\nquery drop_point_layer() -> FeatureCollection<Point, Pin> {\n    from: DropPoint\n    geometry: DropPoint.spot\n    order: [DropPoint.name asc]\n    limit: 500\n}\n",
    );
    write(
        &root,
        "domains/maps/queries/walkable_layer.jadpo",
        "use maps/entities { DropPoint }\nuse maps/types { Pin, WalkReach }\n\n/// The drop points within a walk, nearest first, as a layer.\nquery walkable_layer(doorstep: Point, reach: WalkReach) -> FeatureCollection<Point, Pin> {\n    from: DropPoint\n    geometry: DropPoint.spot\n    where: geo.within(DropPoint.spot, doorstep, distance: reach)\n    order: [geo.distance(DropPoint.spot, doorstep) asc]\n    limit: 10\n}\n",
    );
    write(
        &root,
        "domains/maps/actions/walkable_pins.jadpo",
        "use maps/types { Pin, WalkReach }\nuse maps/queries { walkable_layer }\n\n/// The drop points within a walk of a door.\naction walkable_pins(doorstep: Point, reach: WalkReach) -> FeatureCollection<Point, Pin> {\n    return walkable_layer(doorstep, reach)\n}\n",
    );
    write(
        &root,
        "domains/maps/actions/open_drop_point.jadpo",
        "use maps/entities { DropPoint }\nuse maps/types { PinName }\n\n/// Opens a drop point.\naction open_drop_point(name: PinName, spot: Point) -> DropPoint {\n    return DropPoint.create {\n        set: {\n            name: name\n            spot: spot\n        }\n    }\n}\n",
    );
    write(
        &root,
        "domains/maps/routes/drop_points.jadpo",
        "use maps/types { Pin }\nuse maps/queries { drop_point_layer }\n\n/// Drop points, for the map.\nroutes DropPoints {\n    prefix: /drop-points\n\n    /// Every drop point, as GeoJSON.\n    GET / {\n        authentication: RouteAuthentication.none\n        output: FeatureCollection<Point, Pin>\n        run: drop_point_layer()\n    }\n}\n",
    );
    write(
        &root,
        "domains/maps/routes/drop_points.test.jadpo",
        "use maps/entities { DropPoint }\nuse maps/types { Pin }\nuse maps/actions { open_drop_point }\nuse maps/queries { drop_point_layer }\n\ntest \"the map fetches the drop points as GeoJSON\" {\n    var market: DropPoint = open_drop_point(\n        \"Keswick Market\",\n        Point {\n            latitude: 54.6013\n            longitude: -3.1347\n            altitude: none\n        }\n    )\n    var stores: DropPoint = open_drop_point(\n        \"Braithwaite Stores\",\n        Point {\n            latitude: 54.6036\n            longitude: -3.1925\n            altitude: none\n        }\n    )\n    var response: Response = request {\n        method: HttpMethod.get\n        path: \"/drop-points\"\n    }\n    assert response.status == 200\n    assert response.output == drop_point_layer()\n    assert response.output == FeatureCollection {\n        features: [\n            Feature {\n                geometry: stores.spot\n                properties: Pin {\n                    id: stores.id\n                    name: stores.name\n                }\n            },\n            Feature {\n                geometry: market.spot\n                properties: Pin {\n                    id: market.id\n                    name: market.name\n                }\n            }\n        ]\n    }\n}\n",
    );
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["api", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    root
}

/// A UUID from a number, as version 4 lays one out.
fn uuid(n: u8) -> [u8; 16] {
    let mut id = [n; 16];
    id[6] = (id[6] & 0x0f) | 0x40;
    id[8] = (id[8] & 0x3f) | 0x80;
    id
}

fn point(latitude: &str, longitude: &str) -> Value {
    let text = format!(r#"{{"type":"Point","coordinates":[{longitude},{latitude}]}}"#);
    let json: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    jadpo_value::geo::read("Point", &json).unwrap_or_else(|e| panic!("{text}: {e:?}"))
}

fn metres(amount: i128) -> Value {
    Value::Quantity(Quantity {
        kind: jadpo_std::QuantityKind::Length,
        amount: amount * 1_000_000,
    })
}

/// The layer `GET /drop-points` sends: plain RFC 7946 GeoJSON, longitude
/// first, sorted by name, each feature's identity in its properties.
const SENT: &str = r#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[-3.1925,54.6036]},"properties":{"id":"02020202-0202-4202-8202-020202020202","name":"Braithwaite Stores"}},{"type":"Feature","geometry":{"type":"Point","coordinates":[-3.1347,54.6013]},"properties":{"id":"01010101-0101-4101-8101-010101010101","name":"Keswick Market"}},{"type":"Feature","geometry":{"type":"Point","coordinates":[-3.0558,54.6195]},"properties":{"id":"03030303-0303-4303-8303-030303030303","name":"Threlkeld Village Hall"}}]}"#;

#[test]
fn a_get_route_sends_a_query_s_layer_as_plain_geojson() {
    let root = project("route");
    let sql = jadpo(&root, &["sql", "drop_point_layer"]);
    assert_eq!(sql.code, 0, "{}{}", sql.stdout, sql.stderr);
    assert!(
        sql.stdout
            .contains("a map layer, each feature's geometry its spot and properties a Pin"),
        "{}",
        sql.stdout
    );
    if !jadpo_test::live() {
        return;
    }
    let server = jadpo_test::database::server().unwrap_or_else(|e| panic!("{e}"));
    let url = server
        .create_database("geo_layers_route")
        .expect("database");
    let applied = jadpo(&root, &["migrate", "apply", "--url", &url]);
    assert_eq!(applied.code, 0, "{}{}", applied.stdout, applied.stderr);
    let loaded = jadpo_db::load(&root);
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let host = Harness::postgres(&url);
    let runtime = jadpo_host_native::Runtime::new().expect("runtime");
    runtime.block_on(async {
        let store = jadpo_host_native::postgres(&url).expect("store");
        let mut transaction = store.begin().await.expect("begin");
        for (n, name, spot) in [
            (1, "Keswick Market", "POINT(-3.1347 54.6013)"),
            (2, "Braithwaite Stores", "POINT(-3.1925 54.6036)"),
            (3, "Threlkeld Village Hall", "POINT(-3.0558 54.6195)"),
        ] {
            transaction
                .execute(
                    "INSERT INTO drop_point (id, name, spot) VALUES ($1, $2, ST_GeogFromText('SRID=4326;' || $3))",
                    &[
                        SqlValue::Uuid(uuid(n)),
                        SqlValue::Text(name.to_string()),
                        SqlValue::Text(spot.to_string()),
                    ],
                )
                .await
                .expect("insert");
        }
        transaction.commit().await.expect("commit");
    });
    let env = || Env {
        seed: Some(1),
        clock: Clock::TEST,
    };
    // A mapping library's fetch of the layer.
    let response = runtime.block_on(async {
        let mut ctx = Ctx::of(&program, &host, env());
        let request = jadpo_runtime::http::Request {
            method: "GET".to_string(),
            path: "/drop-points".to_string(),
            ..Default::default()
        };
        jadpo_runtime::http::handle(&program.app(), &program, &mut ctx, &request, "req_1").await
    });
    let body = String::from_utf8_lossy(&response.body);
    assert_eq!(response.status, 200, "{body}");
    assert_eq!(body, SENT);
    assert!(
        response
            .headers
            .iter()
            .any(|(name, value)| name == "content-type" && value == "application/json"),
        "{:?}",
        response.headers
    );
    // Within a walk of a door in Keswick, nearest first: area 20's
    // condition and order, rechecked in Jadpo's own code.
    let names = |reach: i128| {
        let id = program.plan().find("walkable_pins").expect("the action");
        let found = runtime.block_on(async {
            let mut ctx = Ctx::of(&program, &host, env());
            program
                .run(&mut ctx, id, vec![point("54.6", "-3.15"), metres(reach)])
                .await
        });
        let Outcome::Value(Value::Record(layer)) = found else {
            panic!("expected a layer, got {found:?}");
        };
        let Some(Value::List(features)) = layer.get("features") else {
            panic!("no features: {layer:?}");
        };
        features
            .iter()
            .map(|feature| {
                let Value::Record(feature) = feature else {
                    panic!("not a feature: {feature:?}");
                };
                let Some(Value::Record(pin)) = feature.get("properties") else {
                    panic!("no properties: {feature:?}");
                };
                match pin.get("name") {
                    Some(Value::Text(name)) => name.to_string(),
                    other => panic!("no name: {other:?}"),
                }
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(names(5_000), ["Keswick Market", "Braithwaite Stores"]);
    assert_eq!(names(1_000), ["Keswick Market"]);
    assert_eq!(names(100), Vec::<String>::new());
    // The project's own route test, on a disposable database.
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    assert!(tested.stdout.contains("1 passed"), "{}", tested.stdout);
}
