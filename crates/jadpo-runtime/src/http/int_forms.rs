//! An `Int` is read in the form Jadpo writes it (area 2, "Numbers in
//! JSON"; chunk 10.15c): in a request's body, a number, or a string for
//! an `Int` whose declared range goes past 2^53 - 1, and the other form is
//! refused as `type`, whether it is a route's input field or a field of a
//! type inside it. A query string's value and a header are text either
//! way. Read by the codec walk, as the interpreter reads a request, and by
//! a typed entry, as compiled code reads one.

use jadpo_std::http::{ApiPlan, Codec, CodecId, InputPlan, RoutePlan, RouteTable, Run, Segment};
use jadpo_std::types::{FieldPlan, ShapeId, ShapePlan, ValidatorId};
use jadpo_std::{Builtin, HttpMethod, Idempotency, SuccessStatus};
use jadpo_value::Value;
use jadpo_value::json::reading::{At, Field, Got, Reader, Reading};
use jadpo_value::json::{Codecs, Item, Members, Problem};
use jadpo_value::shape::Shape;

use super::{App, Entry, Read, Request, Routes, read_request};

/// An `Int` written as a number, and one whose range passes 2^53 - 1,
/// written as a string.
const INT: CodecId = CodecId(0);
const BIG: CodecId = CodecId(1);

/// Each route, its input's codec and whether it reads the query string:
/// `POST /narrow` takes `Narrow { small: Int }`, `POST /wide` takes
/// `Wide { large: Int { max: 2^62 } }`, `POST /outer` takes
/// `Outer { inner: Inner { small: Int } }`, `POST /paged` takes
/// `Paged { page: Int }` from the `X-Page` header, and `GET /listed` takes
/// `Narrow` from the query string.
const ROUTES: [(&str, u32, bool); 5] = [
    ("narrow", 2, false),
    ("wide", 3, false),
    ("outer", 5, false),
    ("paged", 6, false),
    ("listed", 2, true),
];

fn shapes() -> Vec<Shape> {
    [
        ("Narrow", "small"),
        ("Wide", "large"),
        ("Inner", "small"),
        ("Outer", "inner"),
        ("Paged", "page"),
    ]
    .iter()
    .map(|(ty, name)| {
        Shape::new(&ShapePlan {
            ty: (*ty).to_string(),
            open: false,
            variant: None,
            fields: vec![FieldPlan {
                key: format!("field {ty}.{name}"),
                name: (*name).to_string(),
                validator: ValidatorId(0),
                omittable: false,
                absent: None,
                none_left_out: false,
                section: None,
                hashed: false,
                wire: None,
            }],
        })
    })
    .collect()
}

fn codecs() -> Vec<Codec> {
    let object = |shape: u32, field: u32| Codec::Object {
        shape: ShapeId(shape),
        fields: vec![CodecId(field)],
        validator: None,
    };
    let int = |int_as_number: bool| Codec::Scalar {
        builtin: Builtin::Int,
        validator: ValidatorId(0),
        int_as_number,
    };
    vec![
        int(true),
        int(false),
        object(0, INT.0),
        object(1, BIG.0),
        object(2, INT.0),
        object(3, 4),
        object(4, INT.0),
    ]
}

fn table() -> RouteTable {
    let route = |(name, codec, query): (&str, u32, bool)| RoutePlan {
        cache: None,
        key: format!("route POST /{name}"),
        method: if query {
            HttpMethod::Get
        } else {
            HttpMethod::Post
        },
        segments: vec![Segment::Fixed(name.into())],
        api: 0,
        input: Some(InputPlan {
            codec: CodecId(codec),
            query,
            headers: if name == "paged" {
                vec![("page".into(), "x-page".into())]
            } else {
                Vec::new()
            },
        }),
        output: None,
        status: SuccessStatus::NoContent,
        deadline_ms: 10_000,
        run: Run::Action(0),
        settle: None,
        stream: false,
        sends: None,
        immutable: false,
        links: false,
        keys: None,
        args: Vec::new(),
        authentication: jadpo_std::http::Authentication::Public,
        caller: None,
        confirm: false,
        rate_limit: None,
        file: format!("domains/forms/routes/{name}.jadpo"),
        line: 1,
    };
    RouteTable {
        apis: vec![ApiPlan {
            name: "main".into(),
            prefix: Vec::new(),
            idempotency: Idempotency::Optional,
            methods: Vec::new(),
        }],
        routes: ROUTES.into_iter().map(route).collect(),
        ..RouteTable::default()
    }
}

/// A scalar read as a generated reader reads one.
fn int(reading: &mut Reading<'_>, item: Item<'_>, at: &At<'_>) -> Option<Value> {
    if !reading.given(item, at) {
        return None;
    }
    reading.scalar(INT.0, Builtin::Int, 0, item, at)
}

fn big(reading: &mut Reading<'_>, item: Item<'_>, at: &At<'_>) -> Option<Value> {
    if !reading.given(item, at) {
        return None;
    }
    reading.scalar(BIG.0, Builtin::Int, 0, item, at)
}

/// An object of shape `shape` whose one field `name` is read by `read`.
fn object(
    reading: &mut Reading<'_>,
    members: &Members<'_>,
    shape: u32,
    (name, read): (&str, Reader),
    at: &At<'_>,
) -> Option<Value> {
    let mut slots: Vec<Option<Value>> = vec![None];
    let member = At::Member(at, name);
    if let Got::Read(item) = reading.field(members, name, Field::default(), &member) {
        slots[0] = read(reading, item, &member);
    }
    reading.unknown(members, &[name], at);
    reading.build(shape, &mut slots)
}

fn inner(reading: &mut Reading<'_>, item: Item<'_>, at: &At<'_>) -> Option<Value> {
    let members = reading.members(item, at)?;
    object(reading, &members, 2, ("small", int), at)
}

fn read_narrow(reading: &mut Reading<'_>, members: &Members<'_>) -> Option<Value> {
    object(reading, members, 0, ("small", int), &At::Top)
}

fn read_wide(reading: &mut Reading<'_>, members: &Members<'_>) -> Option<Value> {
    object(reading, members, 1, ("large", big), &At::Top)
}

fn read_outer(reading: &mut Reading<'_>, members: &Members<'_>) -> Option<Value> {
    object(reading, members, 3, ("inner", inner), &At::Top)
}

fn read_paged(reading: &mut Reading<'_>, members: &Members<'_>) -> Option<Value> {
    object(reading, members, 4, ("page", int), &At::Top)
}

static ENTRIES: [Entry; 5] = [
    Entry {
        params: &[],
        input: Some(read_narrow),
        output: None,
    },
    Entry {
        params: &[],
        input: Some(read_wide),
        output: None,
    },
    Entry {
        params: &[],
        input: Some(read_outer),
        output: None,
    },
    Entry {
        params: &[],
        input: Some(read_paged),
        output: None,
    },
    Entry {
        params: &[],
        input: Some(read_narrow),
        output: None,
    },
];

fn entries(number: u32) -> Option<&'static Entry> {
    ENTRIES.get(number as usize)
}

/// What `request` reads as by the codec walk and by the typed entries,
/// which must agree.
fn read(request: &Request) -> String {
    let table = table();
    let routes = Routes::new(&table, &[]);
    let codecs = codecs();
    let shapes = shapes();
    let app = |entries: Option<super::Entries>| App {
        table: &table,
        routes: &routes,
        codecs: Codecs {
            codecs: &codecs,
            validators: &[],
            shapes: &shapes,
            coder: None,
        },
        failures: &[],
        json: crate::JsonCodecs::NONE,
        entries,
    };
    let number = ROUTES
        .iter()
        .position(|(name, _, _)| request.path == format!("/{name}"))
        .expect("a route") as u32;
    let walked = format!("{:?}", read_request(&app(None), number, request));
    let typed = format!("{:?}", read_request(&app(Some(entries)), number, request));
    assert_eq!(walked, typed, "the codec walk and the typed entry differ");
    walked
}

fn post(path: &str, body: &str) -> Request {
    Request {
        method: "POST".into(),
        path: path.into(),
        headers: vec![("content-type".into(), "application/json".into())],
        body: body.as_bytes().to_vec(),
        ..Request::default()
    }
}

/// What a request refused for `rule` at `field` reads as: a 422.
fn refused(field: &str, rule: &str) -> String {
    format!(
        "{:?}",
        Read::Problems(vec![Problem {
            field: field.into(),
            rule: rule.into(),
        }])
    )
}

fn accepted(request: &Request) {
    let read = read(request);
    assert!(read.starts_with("Values"), "{} {read}", request.path);
}

#[test]
fn a_route_s_int_is_a_number_and_text_is_refused() {
    accepted(&post("/narrow", r#"{"small": 24}"#));
    assert_eq!(
        read(&post("/narrow", r#"{"small": "24"}"#)),
        refused("small", "type")
    );
}

#[test]
fn an_int_past_the_safe_integers_is_a_string_and_a_number_is_refused() {
    accepted(&post("/wide", r#"{"large": "9007199254740993"}"#));
    assert_eq!(
        read(&post("/wide", r#"{"large": 9007199254740993}"#)),
        refused("large", "type")
    );
    assert_eq!(
        read(&post("/wide", r#"{"large": 3}"#)),
        refused("large", "type"),
        "a small value too is sent as the string Jadpo writes"
    );
}

#[test]
fn a_type_s_int_field_inside_the_body_refuses_text() {
    accepted(&post("/outer", r#"{"inner": {"small": 2}}"#));
    assert_eq!(
        read(&post("/outer", r#"{"inner": {"small": "2"}}"#)),
        refused("inner.small", "type")
    );
}

#[test]
fn a_query_string_s_int_and_a_header_s_are_text() {
    accepted(&Request {
        method: "GET".into(),
        path: "/listed".into(),
        query: Some("small=24".into()),
        ..Request::default()
    });
    let mut paged = post("/paged", "{}");
    paged.headers.push(("x-page".into(), "3".into()));
    accepted(&paged);
    let mut unreadable = post("/paged", "{}");
    unreadable.headers.push(("x-page".into(), "three".into()));
    assert_eq!(read(&unreadable), refused("page", "type"));
}
