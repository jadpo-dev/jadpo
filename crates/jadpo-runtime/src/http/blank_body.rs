//! A request with no body (area 7, "A request with no body"; chunk
//! 10.7b): a blank body, empty or only whitespace, on a route whose input
//! has no field the body must send reads as `{}`, whatever its content
//! type says; on a route whose input has one, it is refused as before,
//! with the same failure. Read by the codec walk, as the interpreter reads
//! a request, and by a typed entry, as compiled code reads one.

use jadpo_std::http::{ApiPlan, Codec, CodecId, InputPlan, RoutePlan, RouteTable, Run, Segment};
use jadpo_std::types::{FieldPlan, ShapePlan, ValidatorId};
use jadpo_std::{Builtin, HttpMethod, Idempotency, SuccessStatus};
use jadpo_value::Value;
use jadpo_value::json::reading::{At, Field, Got, Reading};
use jadpo_value::json::{Codecs, Members, Problem};
use jadpo_value::shape::Shape;

use super::{App, BODY_NOT_JSON, Entry, INVALID_REQUEST, Read, Request, Routes, read_request};

const TEXT: CodecId = CodecId(0);

/// Each route's input: `POST /solve` takes `MayBeBlank`, whose one field,
/// `note`, may be left out; `POST /reply` takes `MustSend`, whose `text`
/// must be sent; `POST /tagged` takes `FromHeader`, whose `tag` comes from
/// the `X-Tag` header and must be sent, and whose `note` may be left out.
const ROUTES: [(&str, u32); 3] = [("solve", 1), ("reply", 2), ("tagged", 3)];

fn field(ty: &str, name: &str, optional: bool) -> FieldPlan {
    FieldPlan {
        key: format!("field {ty}.{name}"),
        name: name.to_string(),
        validator: ValidatorId(0),
        omittable: optional,
        absent: optional.then_some(jadpo_std::Constant::None),
        none_left_out: false,
        section: None,
        hashed: false,
        wire: None,
    }
}

fn shapes() -> Vec<Shape> {
    [
        ("MayBeBlank", vec![("note", true)]),
        ("MustSend", vec![("text", false)]),
        ("FromHeader", vec![("note", true), ("tag", false)]),
    ]
    .iter()
    .map(|(ty, fields)| {
        Shape::new(&ShapePlan {
            ty: (*ty).to_string(),
            open: false,
            variant: None,
            fields: fields
                .iter()
                .map(|(name, optional)| field(ty, name, *optional))
                .collect(),
        })
    })
    .collect()
}

fn codecs() -> Vec<Codec> {
    let object = |shape: u32, fields: usize| Codec::Object {
        shape: jadpo_std::types::ShapeId(shape),
        fields: vec![TEXT; fields],
        validator: None,
    };
    vec![
        Codec::Scalar {
            builtin: Builtin::Text,
            validator: ValidatorId(0),
            int_as_number: false,
        },
        object(0, 1),
        object(1, 1),
        object(2, 2),
    ]
}

fn table() -> RouteTable {
    let route = |(name, codec): (&str, u32)| RoutePlan {
        cache: None,
        key: format!("route POST /{name}"),
        method: HttpMethod::Post,
        segments: vec![Segment::Fixed(name.into())],
        api: 0,
        input: Some(InputPlan {
            codec: CodecId(codec),
            query: false,
            headers: if name == "tagged" {
                vec![("tag".into(), "x-tag".into())]
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
        file: format!("domains/blank/routes/{name}.jadpo"),
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

/// An object's text fields read from `members` as a generated reader reads
/// them, each `(name, optional)`, then the members it doesn't declare.
fn read_fields(
    reading: &mut Reading<'_>,
    members: &Members<'_>,
    shape: u32,
    fields: &[(&str, bool)],
) -> Option<Value> {
    let mut slots: Vec<Option<Value>> = vec![None; fields.len()];
    for (slot, (name, optional)) in fields.iter().enumerate() {
        let how = Field {
            absent: *optional,
            omittable: *optional,
            ..Field::default()
        };
        let at = At::Member(&At::Top, name);
        match reading.field(members, name, how, &at) {
            Got::Read(item) => slots[slot] = reading.scalar(0, Builtin::Text, 0, item, &at),
            Got::None => slots[slot] = Some(Value::None),
            Got::Left => {}
        }
    }
    let declared: Vec<&str> = fields.iter().map(|(name, _)| *name).collect();
    reading.unknown(members, &declared, &At::Top);
    reading.build(shape, &mut slots)
}

fn read_solve(reading: &mut Reading<'_>, members: &Members<'_>) -> Option<Value> {
    read_fields(reading, members, 0, &[("note", true)])
}

fn read_reply(reading: &mut Reading<'_>, members: &Members<'_>) -> Option<Value> {
    read_fields(reading, members, 1, &[("text", false)])
}

fn read_tagged(reading: &mut Reading<'_>, members: &Members<'_>) -> Option<Value> {
    read_fields(reading, members, 2, &[("note", true), ("tag", false)])
}

static ENTRIES: [Entry; 3] = [
    Entry {
        params: &[],
        input: Some(read_solve),
        output: None,
    },
    Entry {
        params: &[],
        input: Some(read_reply),
        output: None,
    },
    Entry {
        params: &[],
        input: Some(read_tagged),
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
        .position(|(name, _)| request.path == format!("/{name}"))
        .expect("a route") as u32;
    let walked = format!("{:?}", read_request(&app(None), number, request));
    let typed = format!("{:?}", read_request(&app(Some(entries)), number, request));
    assert_eq!(walked, typed, "the codec walk and the typed entry differ");
    walked
}

fn post(path: &str, content_type: Option<&str>, body: &str) -> Request {
    Request {
        method: "POST".into(),
        path: path.into(),
        headers: content_type
            .map(|value| vec![("content-type".to_string(), value.to_string())])
            .unwrap_or_default(),
        body: body.as_bytes().to_vec(),
        ..Request::default()
    }
}

fn refused(failure: (u16, &'static str, &'static str)) -> String {
    format!("{:?}", Read::Refused(failure))
}

const BLANKS: [&str; 3] = ["", "  ", "\r\n\t "];
const CONTENT_TYPES: [Option<&str>; 3] = [None, Some("application/json"), Some("text/plain")];

#[test]
fn a_blank_body_reads_as_an_empty_object_when_no_field_must_be_sent() {
    let empty = read(&post("/solve", Some("application/json"), "{}"));
    assert!(empty.starts_with("Values"), "{empty}");
    for body in BLANKS {
        for content_type in CONTENT_TYPES {
            let read = read(&post("/solve", content_type, body));
            assert_eq!(read, empty, "{content_type:?} {body:?}");
        }
    }
}

#[test]
fn a_blank_body_is_refused_as_before_when_a_field_must_be_sent() {
    for body in BLANKS {
        assert_eq!(
            read(&post("/reply", Some("application/json"), body)),
            refused(INVALID_REQUEST),
            "{body:?}"
        );
        for content_type in [None, Some("text/plain")] {
            assert_eq!(
                read(&post("/reply", content_type, body)),
                refused(BODY_NOT_JSON),
                "{content_type:?} {body:?}"
            );
        }
    }
}

#[test]
fn a_body_that_isnt_blank_still_needs_json() {
    for content_type in [None, Some("text/plain")] {
        assert_eq!(
            read(&post("/solve", content_type, "{}")),
            refused(BODY_NOT_JSON),
            "{content_type:?}"
        );
    }
}

#[test]
fn a_field_read_from_a_header_is_not_the_bodys_to_send() {
    let mut tagged = post("/tagged", None, "");
    tagged.headers.push(("x-tag".into(), "urgent".into()));
    let read_tagged = read(&tagged);
    assert!(read_tagged.starts_with("Values"), "{read_tagged}");
    // Without its header, the field is missing, as with any body.
    let untagged = read(&post("/tagged", None, ""));
    assert_eq!(
        untagged,
        format!(
            "{:?}",
            Read::Problems(vec![Problem {
                field: "tag".into(),
                rule: "required".into(),
            }])
        )
    );
}
