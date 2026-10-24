//! The plan read from a checked project (docs/compiler-runtime.md §2.5):
//! type plans in declared order, failure shapes, constants decoded once,
//! numbered callables and the sites decoded for both executors.

use jadpo_db::{Database, File, Layout, Project};
use jadpo_graph::body::{Expr, Node};
use jadpo_graph::{Key, Kind, Section, TypeRef};
use jadpo_plan::{Constant, Decoded, Plan};
use jadpo_std::Builtin;

const SOURCE: &str = r#"type Pence = Decimal {
    min: 0.00
    scale: 2
}

type Size = Enum {
    small
    medium
    large
}

type Line = Object {
    size: Size
    price: Pence
}

type Order = Input {
    size: Size {
        default: Size.medium
    }
    note: Text {
        optional: true
    }
    count: Int {
        default: 1
    }
}

type Ref = Uuid {}

type TooSmall = Failure {
    kind: FailureKind.invalid_value
    client: {
        least: Pence
    }
    internal: {
        given: Pence
    }
}

function total(line: Line) -> Pence {
    return Pence(line.price + 1.00)
}

function reference() -> Ref {
    return "0192F0C4-3B5E-7C1A-9D2E-4F6A8B0C1D2E"
}

function check(price: Pence) fails TooSmall -> Pence {
    if (price < 1.00) {
        reject TooSmall {
            least: 1.5
            given: price
        }
    }
    return price
}
"#;

fn plan() -> Plan {
    let db = Database::default();
    let file = File::new(&db, "main.jadpo".to_string(), SOURCE.to_string());
    let project = Project::new(
        &db,
        vec![file],
        Vec::new(),
        Layout::Loose,
        Default::default(),
        Vec::new(),
    );
    let problems: Vec<String> = jadpo_check::check(&db, project)
        .iter()
        .map(|d| format!("{}: {}", d.rule.id, d.message))
        .collect();
    assert!(problems.is_empty(), "{problems:#?}");
    Plan::build(&db, project)
}

#[test]
fn shapes_keep_declared_order_defaults_and_variants_places() {
    let plan = plan();
    let types = plan.types();
    let line = types
        .shape_plan(&Key::new(Kind::Type, "Line"))
        .expect("an object");
    let names: Vec<&str> = line.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["size", "price"], "as declared, not by key");
    let large = types
        .shape_plan(&Key::new(Kind::Variant, "Size.large"))
        .expect("a variant");
    assert_eq!(large.ty, "Size");
    assert_eq!(
        large.variant.map(|v| (v.name, v.ordinal)),
        Some(("large".to_string(), 2))
    );
    let order = types
        .shape_plan(&Key::new(Kind::Type, "Order"))
        .expect("an input");
    let absent: Vec<Option<Constant>> = order.fields.iter().map(|f| f.absent.clone()).collect();
    assert_eq!(
        absent,
        [
            Some(Constant::Variant {
                ty: "Size".into(),
                ordinal: 1,
                name: "medium".into(),
                fields: Vec::new(),
            }),
            Some(Constant::None),
            Some(Constant::Int(1)),
        ]
    );
}

#[test]
fn failures_keep_their_sections_and_code() {
    let plan = plan();
    let failure = plan
        .types()
        .failure_plan(&Key::new(Kind::Type, "TooSmall"))
        .expect("a failure");
    assert_eq!(failure.code, "too_small");
    assert_eq!(failure.kind.as_deref(), Some("invalid_value"));
    let fields: Vec<(&str, Option<Section>)> = failure
        .fields
        .iter()
        .map(|f| (f.name.as_str(), f.section))
        .collect();
    assert_eq!(
        fields,
        [
            ("least", Some(Section::Client)),
            ("given", Some(Section::Internal))
        ]
    );
}

#[test]
fn validators_follow_the_chain_most_specific_first() {
    let plan = plan();
    let types = plan.types();
    let price = types.validator_plan(&TypeRef::Field(Key::new(Kind::Field, "Line.price")));
    assert_eq!(price.builtin, Some(Builtin::Decimal));
    assert_eq!(price.scale(), Some(2));
    assert_eq!(price.rules.len(), 2, "the field's own, then `Pence`'s");
}

#[test]
fn callables_are_numbered_by_key_with_their_labels() {
    let plan = plan();
    let labels: Vec<&str> = plan.callables().iter().map(|c| c.label.as_str()).collect();
    assert_eq!(
        labels,
        ["function check", "function reference", "function total"]
    );
    assert_eq!(plan.find("total"), Some(2));
    assert_eq!(plan.find("function check"), Some(0));
    let line = SOURCE
        .lines()
        .position(|line| line.starts_with("function total"))
        .map(|at| at as u32 + 1);
    assert_eq!(plan.callable(2).map(|c| c.line), line);
}

#[test]
fn literals_are_decoded_once_as_their_types_hold_them() {
    let plan = plan();
    let constants = plan.constants();
    assert!(
        constants.contains(&Constant::Uuid(
            "0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e".into()
        )),
        "lowercase: {constants:?}"
    );
    assert!(
        constants.contains(&Constant::Decimal {
            mantissa: 150,
            scale: 2
        }),
        "`least: 1.5` kept at `Pence`'s scale: {constants:?}"
    );
}

#[test]
fn sites_are_decoded_with_anchors_and_slots() {
    let plan = plan();
    let total = plan.callable(2).expect("total");
    let field = total
        .body
        .sites
        .iter()
        .position(|site| matches!(site.node, Node::Expr(Expr::Field { .. })))
        .expect("a field read");
    assert_eq!(
        total.sites[field].decoded,
        Decoded::Field {
            name: "price".into()
        }
    );
    let text = plan.text("main.jadpo").expect("its text");
    let anchor = total.sites[field].anchor as usize;
    assert_eq!(&text[anchor..anchor + 5], "price", "the field's name");
    let check = plan.callable(0).expect("check");
    let reject = check
        .sites
        .iter()
        .find_map(|site| match &site.decoded {
            Decoded::Reject { slots, failure } => Some((slots.clone(), failure.is_some())),
            _ => None,
        })
        .expect("a reject");
    assert_eq!(reject, (vec![Some(0), Some(1)], true));
}

#[test]
fn a_connected_delivery_keeps_the_provider_and_receipt_write_plans() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/bench/fixtures/connected-deliveries");
    let loaded = jadpo_db::load(&root);
    for contested in [false, true] {
        let db = Database::default();
        let files = loaded
            .project
            .files(&loaded.db)
            .iter()
            .map(|file| {
                let path = file.path(&loaded.db);
                let text = file.text(&loaded.db);
                let (path, text) = if contested {
                    (
                        path.replace("checked_ticket.jadpo", "ticket.jadpo"),
                        text.replace("CheckedTicket", "Ticket"),
                    )
                } else {
                    (path.clone(), text.clone())
                };
                File::new(&db, path, text)
            })
            .collect();
        let project = Project::new(
            &db,
            files,
            loaded.project.other_files(&loaded.db).clone(),
            Layout::Project,
            loaded.project.identities(&loaded.db).clone(),
            loaded.project.records(&loaded.db).clone(),
        );
        let problems: Vec<_> = jadpo_check::check(&db, project)
            .into_iter()
            .filter(jadpo_check::blocks_running)
            .collect();
        assert!(problems.is_empty(), "contested={contested}: {problems:#?}");
        let model = jadpo_check::data::data_model(&db, project);
        let receipt = model
            .entities
            .iter()
            .find(|entity| entity.name == "Receipt")
            .unwrap();
        let ticket = receipt
            .fields
            .iter()
            .find(|field| field.name == "ticket")
            .unwrap();
        assert_eq!(
            ticket.sql_type.as_deref(),
            Some("text"),
            "contested={contested}: {ticket:#?}"
        );
        let plan = Plan::build(&db, project);
        let record = plan
            .callables()
            .iter()
            .find(|c| c.key.name() == "record")
            .unwrap();
        let writes: Vec<_> = record
            .sites
            .iter()
            .filter_map(|site| match &site.decoded {
                Decoded::Write { plan, .. } => Some(plan),
                _ => None,
            })
            .collect();
        assert_eq!(writes.len(), 1);
        assert!(
            writes[0].is_some(),
            "contested={contested}: missing receipt write"
        );
        assert!(
            record
                .sites
                .iter()
                .any(|site| matches!(site.decoded, Decoded::Provider(_)))
        );
    }
}
