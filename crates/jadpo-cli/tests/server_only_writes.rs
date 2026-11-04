//! `FieldAccess.server_only` against the system caller (area 9, "A field no
//! request may touch"; area 12, "Runs as the system"; chunk A23): a
//! subscriber and a schedule write the field and read it, a route reaching
//! it is refused at check, and a caller the record's own list lets in is
//! still refused it at run. Its route test runs compiled beside the
//! interpreter in `compiled.rs`, and on workerd there too, so what the plan's
//! statement says is proved on all three.

use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_host_native::Runtime;
use jadpo_interp::Program;
use jadpo_runtime::{Caller, Executor, Outcome};
use jadpo_std::http::PrincipalKind;
use jadpo_term::Environment;
use jadpo_test::host::Harness;
use jadpo_value::Value;

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let mut env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    if let Ok(url) = std::env::var("JADPO_TEST_DATABASE_URL") {
        env = env.with_var("JADPO_TEST_DATABASE_URL", &url);
    }
    run(&args, &env.with_cwd(cwd))
}

/// A shipping desk: anyone books a shipment, and the system stamps it with a
/// reference no request may touch.
pub fn files() -> Vec<(String, String)> {
    [
        (
            "app.jadpo",
            r#"use shipping/entities { Person }

application {}

principal {
    user: {
        subject: Person.handle
    }
}
"#,
        ),
        (
            "authentication.jadpo",
            r#"authentication {
    browser_session: {
        cookie: "shipping_session"
    }
}
"#,
        ),
        (
            "domains/shipping/actions/book.jadpo",
            r#"use shipping/entities { Shipment }
use shipping/types { NewShipment, ShipmentView }
use shipping/events { ShipmentBooked }

/// Books a shipment; the reference comes later, from the system.
action book(input: NewShipment) -> ShipmentView {
    var shipment: Shipment = Shipment.create {
        set: {
            label: input.label
            reference: none
        }
    }
    emit ShipmentBooked {
        shipment_id: shipment.id
    }
    return ShipmentView {
        id: shipment.id
        label: shipment.label
    }
}
"#,
        ),
        (
            "domains/shipping/actions/book_quietly.jadpo",
            r#"use shipping/entities { Shipment }

/// A shipment with nothing to deliver, for a call to change.
action book_quietly() -> Shipment {
    return Shipment.create {
        set: {
            label: "Crate"
            reference: none
        }
    }
}
"#,
        ),
        (
            "domains/shipping/actions/join.jadpo",
            r#"use shipping/entities { Person }
use shipping/types { Label }
use shipping/failures { HandleTaken }

/// Someone signing up.
action join(handle: Label) fails HandleTaken -> Person {
    return attempt Person.create {
        set: {
            handle: handle
        }
        conflict: {
            Person.handle: HandleTaken
        }
    }
}
"#,
        ),
        (
            "domains/shipping/actions/keep_tally.jadpo",
            r#"use shipping/entities { Shipment }

/// Records a run of the schedule, its reference set as it is created.
action keep_tally() -> Unit {
    var tally: Shipment = Shipment.create {
        set: {
            label: "tally"
            reference: "from the schedule"
        }
    }
    assert tally.label == "tally"
}
"#,
        ),
        (
            "domains/shipping/actions/stamp.jadpo",
            r#"use shipping/entities { Shipment, ShipmentNotFound }
use shipping/actions { keep_tally }

/// Gives a shipment its reference, and keeps a tally of it.
action stamp(shipment_id: Shipment.id) fails ShipmentNotFound -> Unit {
    var booked: Shipment = attempt Shipment.get(shipment_id)
    assert booked.reference == none
    attempt Shipment.update(shipment_id) {
        set: {
            reference: "stamped"
        }
    }
    keep_tally()
}
"#,
        ),
        (
            "domains/shipping/entities/person.jadpo",
            r#"use shipping/types { Label }

/// Someone who signs in.
entity Person {
    id: Uuid
    handle: Label {
        unique: true
    }

    policy: {
        create: [Access.public]
        read: [id]
    }
}
"#,
        ),
        (
            "domains/shipping/entities/shipment.jadpo",
            r#"use shipping/types { Label }

/// A shipment, with a reference no request may touch.
entity Shipment {
    id: Uuid
    label: Label
    reference: Label? {
        policy: FieldAccess.server_only
    }

    policy: {
        create: [Access.public, Access.system]
        read: [Access.public, Access.system]
        update: [Access.authenticated, Access.system]
    }
}
"#,
        ),
        (
            "domains/shipping/events/shipment_booked.jadpo",
            r#"use shipping/entities { Shipment }

/// A shipment was booked.
type ShipmentBooked = Event {
    shipment_id: Shipment.id
}
"#,
        ),
        (
            "domains/shipping/failures/handle_taken.jadpo",
            r#"/// Someone already has this handle.
type HandleTaken = Failure {
    kind: FailureKind.conflict
    message: "That handle is taken."
}
"#,
        ),
        (
            "domains/shipping/queries/all_shipments.jadpo",
            r#"use shipping/entities { Shipment }
use shipping/types { ShipmentView }

/// Every shipment.
query all_shipments() -> List<ShipmentView> {
    from: Shipment
    order: [Shipment.id asc]
    limit: 10
}
"#,
        ),
        (
            "domains/shipping/routes/shipments.jadpo",
            r#"use shipping/types { NewShipment, ShipmentView }
use shipping/actions { book }

/// Booking shipments.
routes Shipments {
    prefix: /shipments

    /// Books a shipment.
    POST / {
        authentication: RouteAuthentication.none
        input: NewShipment
        output: ShipmentView
        status: SuccessStatus.created
        run: book(input)
    }
}
"#,
        ),
        (
            "domains/shipping/routes/shipments.test.jadpo",
            r#"use shipping/entities { Shipment }
use shipping/types { NewShipment }
use shipping/queries { all_shipments }

test "a subscriber writes a field no request may" {
    var response: Response = request {
        method: HttpMethod.post
        path: "/shipments"
        input: NewShipment {
            label: "Crate"
        }
    }
    assert response.status == 201
    assert list.count(all_shipments()) == 2
    match response.output {
        some(view) -> {
            var saved: Shipment = attempt Shipment.get(view.id)
            assert saved.reference == "stamped"
        }
        none -> {
            assert response.status == 0
        }
    }
}
"#,
        ),
        (
            "domains/shipping/schedules/tally.jadpo",
            r#"use shipping/actions { keep_tally }

/// Keeps a tally.
schedule tally {
    every: 1h
    run: keep_tally()
}
"#,
        ),
        (
            "domains/shipping/schedules/tally.test.jadpo",
            r#"use shipping/entities { Shipment }
use shipping/types { ShipmentView }
use shipping/queries { all_shipments }

test "a schedule writes a field no request may as it creates a record" {
    tally.run()
    var seen: List<ShipmentView> = all_shipments()
    match list.first(seen) {
        some(shipment) -> {
            var saved: Shipment = attempt Shipment.get(shipment.id)
            assert saved.reference == "from the schedule"
        }
        none -> {
            assert list.count(seen) == 1
        }
    }
}
"#,
        ),
        (
            "domains/shipping/subscribers/stamp_booked.jadpo",
            r#"use shipping/actions { stamp }
use shipping/events { ShipmentBooked }

/// Stamps each shipment once it is booked.
subscriber stamp_booked {
    on: ShipmentBooked
    run: stamp(event.shipment_id)
}
"#,
        ),
        (
            "domains/shipping/types/label.jadpo",
            r#"/// What a shipment is called, or what it was stamped with.
type Label = Text {
    min_length: 1
    max_length: 80
}
"#,
        ),
        (
            "domains/shipping/types/new_shipment.jadpo",
            r#"use shipping/types { Label }

/// A shipment to book.
type NewShipment = Input {
    label: Label
}
"#,
        ),
        (
            "domains/shipping/types/shipment_view.jadpo",
            r#"use shipping/entities { Shipment }

/// A shipment, as whoever booked it reads it.
type ShipmentView = Output {
    pick: [Shipment.id, Shipment.label]
}
"#,
        ),
    ]
    .into_iter()
    .map(|(path, text)| (path.to_string(), text.to_string()))
    .collect()
}

fn project(name: &str) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("server-only-{name}"));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files() {
        let path = root.join(path);
        fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        fs::write(path, text).expect("write");
    }
    let migrated = jadpo(&root, &["migrate", "new", "initial"]);
    assert_eq!(migrated.code, 0, "{}{}", migrated.stdout, migrated.stderr);
    let recorded = jadpo(&root, &["records", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    root
}

#[test]
fn a_subscriber_and_a_schedule_write_what_no_request_may() {
    let root = project("writes");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    if !jadpo_test::live() {
        return;
    }
    let tested = jadpo(&root, &["test"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
}

/// A route reaching the field could never succeed, whoever calls it.
#[test]
fn a_route_reaching_the_field_is_refused_at_check() {
    let route = r#"use shipping/actions { stamp }
use shipping/entities { Shipment }

/// Stamping by hand.
routes Stamps {
    prefix: /stamps

    /// Stamps a shipment.
    POST /{shipment_id: Shipment.id} {
        status: SuccessStatus.no_content
        run: stamp(shipment_id)
    }
}
"#;
    let root = project("route");
    fs::write(root.join("domains/shipping/routes/stamps.jadpo"), route).expect("write");
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 1, "{}{}", checked.stdout, checked.stderr);
    assert!(
        checked
            .stdout
            .contains("`POST /stamps/{shipment_id}` sets `Shipment.reference`, which nobody may"),
        "{}",
        checked.stdout
    );
}

fn field(record: &Value, name: &str) -> Value {
    match record {
        Value::Record(record) => record.get(name).cloned().expect("the field"),
        other => panic!("not a record: {other:?}"),
    }
}

/// `update:` lets anyone signed in change a shipment, and the field's own
/// `FieldAccess.server_only` still refuses them, in the same statement.
#[test]
fn a_caller_the_record_lets_in_is_refused_the_field() {
    if !jadpo_test::live() {
        return;
    }
    let root = project("caller");
    let loaded = jadpo_db::load(&root);
    let steps = jadpo_check::data::migrations::store_steps(
        jadpo_check::data::migrations::migrations(&loaded.db, loaded.project),
    );
    let program = Program::load(&loaded.db, loaded.project).expect("the project checks");
    let databases = jadpo_test::database::TestDatabases::new(steps, false);
    let copy = databases.acquire().expect("a test database");
    let (_, url) = copy.stores.get("main").expect("main").clone();
    let host = Harness::postgres(&url);
    let runtime = Runtime::new().expect("runtime");
    runtime.block_on(async {
        let call = async |action: &str, args: Vec<Value>, caller: Option<&Value>| -> Outcome {
            let id = program.plan().find(action).expect("the action");
            let mut ctx = host.ctx(&program);
            ctx.set_caller(caller.map(|person| Caller {
                value: person.clone(),
                key: "user".to_string(),
                session: None,
                sign_in: None,
                multi_factor: false,
            }));
            program.run(&mut ctx, id, args).await
        };
        let Outcome::Value(person) = call("join", vec![Value::text("ann")], None).await else {
            panic!("ann joined");
        };
        let ann = jadpo_runtime::http::identity::principal(
            PrincipalKind::User,
            field(&person, "id"),
            field(&person, "handle"),
        );
        let Outcome::Value(shipment) = call("book_quietly", vec![], None).await else {
            panic!("a shipment booked");
        };
        let refused = call("stamp", vec![field(&shipment, "id")], Some(&ann)).await;
        match refused {
            Outcome::Failed(failure) => assert_eq!(failure.code, "not_permitted"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    });
}
