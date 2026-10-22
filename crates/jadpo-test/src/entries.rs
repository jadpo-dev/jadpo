//! Compiled code's typed entries held to the codec walk (chunk 8.1p;
//! docs/compiler-runtime.md §2.5, "A typed per-route entry"). The
//! differential run gives every request a compiled route test sends to its
//! route's entry, which only sees the requests the tests send; this gives
//! every route of a compiled program a seeded set of requests made from
//! its codecs, read by its entry and by the codec walk the interpreter
//! reads them with ([`jadpo_runtime::http::read_request`] with and
//! without the entries), and values made the same way, written both ways,
//! and names the first that differs.
//!
//! A request's input is each value its type's codec can hold, written by
//! the codec walk, and the first two of those with a member or an item
//! left out, given again, `null`, of another kind, too long or out of
//! range; with members its type doesn't declare; whole bodies that aren't
//! an object or aren't JSON; the same as query strings, with keys given
//! twice; with and without its header fields; and each placeholder given
//! its own values' text and text of other kinds.

use std::fmt::Write as _;

use jadpo_runtime::Executor;
use jadpo_runtime::compiled::Program;
use jadpo_runtime::http::{Request, read_request};
use jadpo_std::Builtin;
use jadpo_std::http::{Codec, CodecId, RoutePlan, Run, Segment};
use jadpo_value::Value;
use jadpo_value::json::Codecs;

/// Reads and writes every route of `program` with its entries and by the
/// codec walk: how many requests and values were compared, or the first
/// difference, with the request or value and both sides. The entries are
/// those the program answers its routes with as any executor
/// ([`Executor::app`]), which is what a deployable serves with (chunk
/// 8.4c): a route that runs an action, a query or a listing and reads a
/// placeholder or writes an output without one is a difference too.
pub fn compare(program: &Program) -> Result<usize, String> {
    let typed = (program as &dyn Executor).app();
    let walked = program.walked_app();
    let mut compared = 0;
    for (number, route) in walked.table.routes.iter().enumerate() {
        let number = u32::try_from(number).unwrap_or(u32::MAX);
        let Some(entry) = typed.entry(number) else {
            let typed_run = matches!(
                route.run,
                Run::Action(_) | Run::Query(_) | Run::Listing { .. }
            );
            let reads_or_writes = route.output.is_some()
                || route
                    .segments
                    .iter()
                    .any(|segment| matches!(segment, Segment::Param { .. }));
            if typed_run && reads_or_writes {
                return Err(format!(
                    "{} is answered without its typed entry",
                    route.label()
                ));
            }
            continue;
        };
        let made = Made {
            codecs: &walked.codecs,
        };
        for request in made.requests(route) {
            let one = format!("{:?}", read_request(&walked, number, &request));
            let other = format!("{:?}", read_request(&typed, number, &request));
            if one != other {
                return Err(format!(
                    "{} reads this request differently:\n    {}\n  by the codec walk: {one}\n  by its entry:      {other}",
                    route.label(),
                    shown(&request)
                ));
            }
            compared += 1;
        }
        if let (Some(codec), Some(write)) = (route.output, entry.output) {
            for value in made.values(codec, 3) {
                let mut one = String::new();
                walked.codecs.write(codec, &value, &mut one);
                let mut other = String::new();
                write(&typed.codecs, &value, &mut other);
                if one != other {
                    return Err(format!(
                        "{} writes {value:?} differently:\n  by the codec walk: {one}\n  by its entry:      {other}",
                        route.label()
                    ));
                }
                compared += 1;
            }
        }
    }
    Ok(compared)
}

/// A request as a difference names it.
fn shown(request: &Request) -> String {
    let mut out = format!("{} {}", request.method, request.path);
    if let Some(query) = &request.query {
        let _ = write!(out, "?{query}");
    }
    for (name, value) in &request.headers {
        let _ = write!(out, "\n    {name}: {value}");
    }
    if !request.body.is_empty() {
        let _ = write!(out, "\n    {}", String::from_utf8_lossy(&request.body));
    }
    out
}

/// A JSON document a request is made of, which may name a member twice
/// and hold any text as a value.
#[derive(Clone, Debug)]
enum Json {
    Raw(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    fn of(json: &serde_json::Value) -> Json {
        match json {
            serde_json::Value::Array(items) => Json::Array(items.iter().map(Json::of).collect()),
            serde_json::Value::Object(members) => Json::Object(
                members
                    .iter()
                    .map(|(name, value)| (name.clone(), Json::of(value)))
                    .collect(),
            ),
            other => Json::Raw(other.to_string()),
        }
    }

    fn write(&self, out: &mut String) {
        match self {
            Json::Raw(text) => out.push_str(text),
            Json::Array(items) => {
                out.push('[');
                for (at, item) in items.iter().enumerate() {
                    if at > 0 {
                        out.push(',');
                    }
                    item.write(out);
                }
                out.push(']');
            }
            Json::Object(members) => {
                out.push('{');
                for (at, (name, value)) in members.iter().enumerate() {
                    if at > 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::Value::String(name.clone()).to_string());
                    out.push(':');
                    value.write(out);
                }
                out.push('}');
            }
        }
    }

    fn text(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    /// Every place in it, as the members' and items' places from the top,
    /// up to `depth` deep.
    fn places(&self, depth: usize, at: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if depth == 0 {
            return;
        }
        let children = match self {
            Json::Raw(_) => return,
            Json::Array(items) => items.iter().collect::<Vec<_>>(),
            Json::Object(members) => members.iter().map(|(_, value)| value).collect(),
        };
        for (place, child) in children.into_iter().enumerate() {
            at.push(place);
            out.push(at.clone());
            child.places(depth - 1, at, out);
            at.pop();
        }
    }

    /// The container holding place `at` and the place within it.
    fn parent(&mut self, at: &[usize]) -> Option<(&mut Json, usize)> {
        let (last, path) = at.split_last()?;
        let mut node = self;
        for place in path {
            node = match node {
                Json::Array(items) => items.get_mut(*place)?,
                Json::Object(members) => &mut members.get_mut(*place)?.1,
                Json::Raw(_) => return None,
            };
        }
        Some((node, *last))
    }

    /// The same document with place `at` changed by `change`.
    fn changed(&self, at: &[usize], change: &Change) -> Option<Json> {
        let mut out = self.clone();
        let (parent, place) = out.parent(at)?;
        match (parent, change) {
            (Json::Array(items), Change::Remove) => {
                items.remove(place);
            }
            (Json::Object(members), Change::Remove) => {
                members.remove(place);
            }
            (Json::Array(items), Change::Set(text)) => {
                *items.get_mut(place)? = Json::Raw(text.clone());
            }
            (Json::Object(members), Change::Set(text)) => {
                members.get_mut(place)?.1 = Json::Raw(text.clone());
            }
            (Json::Array(items), Change::Repeat) => {
                let item = items.get(place)?.clone();
                items.push(item);
            }
            (Json::Object(members), Change::Repeat) => {
                let member = members.get(place)?.clone();
                members.push(member);
            }
            (Json::Raw(_), _) => return None,
        }
        Some(out)
    }
}

/// A change made at one place of a document.
enum Change {
    Remove,
    Set(String),
    /// The item, or the member, given again.
    Repeat,
}

/// What a value is changed to: each kind of JSON value, and text and
/// numbers at the edges of what types allow.
fn others() -> Vec<String> {
    let long = format!("\"{}\"", "a".repeat(400));
    [
        "null",
        "true",
        "0",
        "-1",
        "-0",
        "1.5",
        "1e3",
        "9007199254740993",
        "\"\"",
        "\"x\"",
        "\"Abc\"",
        "\"2026-10-21\"",
        "\"2026-10-21T09:30:00Z\"",
        "\"0192F0C4-3B5E-7C1A-9D2E-4F6A8B0C1D2E\"",
        "\"a@example.com\"",
        "\"\\u0061bc\"",
        "[]",
        "[\"x\"]",
        "[1,1]",
        "{}",
        "{\"tag\":\"x\"}",
        "{\"tag\":1}",
    ]
    .into_iter()
    .map(str::to_string)
    .chain([long])
    .collect()
}

/// Values and requests made from a program's codecs.
struct Made<'a> {
    codecs: &'a Codecs<'a>,
}

impl Made<'_> {
    fn codec(&self, id: CodecId) -> Option<&Codec> {
        self.codecs.codecs.get(id.0 as usize)
    }

    /// Values a codec writes: each kind its type holds, `none`, and values
    /// of other kinds, a record of another type and one of too few fields
    /// among them, nested no more than `depth` deep.
    fn values(&self, id: CodecId, depth: usize) -> Vec<Value> {
        let mut out = vec![Value::None];
        let deeper = |id: CodecId| {
            if depth == 0 {
                vec![Value::None]
            } else {
                self.values(id, depth - 1)
            }
        };
        match self.codec(id) {
            Some(Codec::Nullable(inner)) => out.extend(self.values(*inner, depth)),
            Some(Codec::Wired { contract, .. }) => out.extend(self.values(*contract, depth)),
            Some(Codec::Scalar { builtin, .. }) => out.extend(scalars(*builtin)),
            Some(Codec::Object { shape, fields, .. }) => {
                if let Some(shape) = self.codecs.shapes.get(shape.0 as usize) {
                    let choices: Vec<Vec<Value>> =
                        fields.iter().map(|field| deeper(*field)).collect();
                    // Each field given one of its values, the first few
                    // records none of them `none`, the last any.
                    for pick in 0..4 {
                        let given = choices.iter().enumerate().map(|(slot, values)| {
                            let at = match values.len() {
                                0 | 1 => 0,
                                _ if pick == 3 => (pick + slot) % values.len(),
                                held => 1 + (pick + slot) % (held - 1),
                            };
                            (
                                Some(slot as u32),
                                values.get(at).cloned().unwrap_or(Value::None),
                            )
                        });
                        out.extend(shape.build(given));
                    }
                    // Each field left out, which an `Input` remembers.
                    for left in 0..fields.len() {
                        let given = choices
                            .iter()
                            .enumerate()
                            .filter(|(slot, _)| *slot != left)
                            .map(|(slot, values)| {
                                (
                                    Some(slot as u32),
                                    values.get(1).cloned().unwrap_or(Value::None),
                                )
                            });
                        out.extend(shape.build(given));
                    }
                    let fields: Vec<(std::rc::Rc<str>, Value)> = (0..shape.len())
                        .filter_map(|slot| shape.field(slot))
                        .map(|(name, _)| (std::rc::Rc::from(name), Value::Int(1)))
                        .collect();
                    out.push(Value::record("Another", fields.clone()));
                    out.push(Value::record(
                        shape.ty(),
                        fields.into_iter().skip(1).collect(),
                    ));
                }
                out.push(Value::text("x"));
            }
            Some(Codec::Enum { ty, variants }) => {
                for variant in variants {
                    match &variant.fields {
                        None => out.push(Value::variant(
                            ty,
                            variant.ordinal,
                            &variant.name,
                            Vec::new(),
                        )),
                        Some((shape, fields)) => {
                            if let Some(shape) = self.codecs.shapes.get(shape.0 as usize) {
                                for pick in 0..2 {
                                    let given = fields.iter().enumerate().map(|(slot, field)| {
                                        let values = deeper(*field);
                                        let value = values
                                            .get((pick + 1) % values.len().max(1))
                                            .cloned()
                                            .unwrap_or(Value::None);
                                        (Some(slot as u32), value)
                                    });
                                    out.extend(shape.build(given));
                                }
                            }
                            out.push(Value::variant(
                                ty,
                                variant.ordinal,
                                &variant.name,
                                Vec::new(),
                            ));
                        }
                    }
                }
                out.push(Value::variant(ty, 99, "unheard_of", Vec::new()));
                out.push(Value::text("x"));
            }
            Some(Codec::List { item, .. } | Codec::Set { item, .. }) => {
                let items = deeper(*item);
                out.push(Value::list(Vec::new()));
                out.push(Value::list(items.iter().skip(1).take(3).cloned().collect()));
                out.push(Value::set(items.iter().skip(1).take(2).cloned().collect()));
                out.push(Value::text("x"));
            }
            _ => {
                out.push(Value::text("x"));
                out.push(Value::Int(3));
            }
        }
        out.truncate(24);
        out
    }

    /// The requests route `route` is given: its placeholders' texts and
    /// its input's documents, one changed at a time.
    fn requests(&self, route: &RoutePlan) -> Vec<Request> {
        let params: Vec<(CodecId, Vec<String>)> = route
            .segments
            .iter()
            .filter_map(|segment| match segment {
                Segment::Param { codec, .. } => Some(*codec),
                Segment::Fixed(_) => None,
            })
            .map(|codec| (codec, self.texts(codec)))
            .collect();
        let documents = match &route.input {
            Some(plan) => self.documents(plan.codec),
            None => Vec::new(),
        };
        let valid: Vec<String> = params
            .iter()
            .map(|(_, texts)| texts.get(1).cloned().unwrap_or_default())
            .collect();
        let mut out = Vec::new();
        // Each placeholder given each of its texts, the others their own.
        for (at, (_, texts)) in params.iter().enumerate() {
            for text in texts {
                let mut given = valid.clone();
                if let Some(place) = given.get_mut(at) {
                    place.clone_from(text);
                }
                out.push(self.request(route, &given, documents.first(), Shape::Json));
            }
        }
        for document in &documents {
            out.push(self.request(route, &valid, Some(document), Shape::Json));
        }
        if let Some(document) = documents.first() {
            for shape in [Shape::Plain, Shape::Unsaid, Shape::Problem, Shape::Headed] {
                out.push(self.request(route, &valid, Some(document), shape));
            }
        }
        // A query string or a body the route doesn't read, and bodies that
        // aren't JSON or aren't an object.
        for raw in [
            "",
            "  ",
            "null",
            "[]",
            "1",
            "\"x\"",
            "{",
            "{}",
            "[1,]",
            "{\"a\":1,\"a\":2}",
        ] {
            out.push(self.request(
                route,
                &valid,
                Some(&Json::Raw(raw.to_string())),
                Shape::Json,
            ));
        }
        let mut odd = self.request(route, &valid, documents.first(), Shape::Json);
        odd.query = Some("zz=1&a=%zz".to_string());
        out.push(odd);
        let mut stray = self.request(route, &valid, documents.first(), Shape::Json);
        stray.query = Some("zz=1&zz=2".to_string());
        out.push(stray);
        out
    }

    /// The texts a placeholder of codec `codec` is given: its values as
    /// the codec walk writes them, and text of other kinds.
    fn texts(&self, codec: CodecId) -> Vec<String> {
        let mut texts = vec![String::new()];
        for value in self.values(codec, 1).iter().skip(1) {
            let mut written = String::new();
            self.codecs.write(codec, value, &mut written);
            let text = match serde_json::from_str::<serde_json::Value>(&written) {
                Ok(serde_json::Value::String(text)) => text,
                _ => written,
            };
            texts.push(text);
        }
        texts.extend(
            [
                "x",
                "0",
                "-1",
                "1.5",
                "true",
                "null",
                "small",
                "0192F0C4-3B5E-7C1A-9D2E-4F6A8B0C1D2E",
                "2026-10-21",
                "a b/c",
            ]
            .into_iter()
            .map(str::to_string),
        );
        texts.push("9".repeat(400));
        texts
    }

    /// The documents an input of codec `codec` is given: each value its
    /// codec writes, and the first of them changed at each place.
    fn documents(&self, codec: CodecId) -> Vec<Json> {
        let mut out: Vec<Json> = Vec::new();
        for value in self.values(codec, 3).iter().skip(1) {
            let mut written = String::new();
            self.codecs.write(codec, value, &mut written);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&written) {
                out.push(Json::of(&json));
            }
        }
        let Some(first) = out.first().cloned() else {
            return vec![Json::Object(Vec::new())];
        };
        // The first two changed at each place, one change at a time.
        let others = others();
        for base in out.clone().iter().take(2) {
            let mut places = Vec::new();
            base.places(4, &mut Vec::new(), &mut places);
            for place in places.iter().take(64) {
                for change in [Change::Remove, Change::Repeat]
                    .into_iter()
                    .chain(others.iter().cloned().map(Change::Set))
                {
                    out.extend(base.changed(place, &change));
                }
            }
        }
        // Members the type doesn't declare, at the top.
        if let Json::Object(members) = &first {
            for name in ["zz", "tag", "a"] {
                let mut more = members.clone();
                more.insert(0, (name.to_string(), Json::Raw("1".to_string())));
                out.push(Json::Object(more));
            }
        }
        out
    }

    /// Route `route`'s request, its placeholders `params`, its input
    /// `document` sent as `shape` says.
    fn request(
        &self,
        route: &RoutePlan,
        params: &[String],
        document: Option<&Json>,
        shape: Shape,
    ) -> Request {
        let mut path = String::new();
        let mut given = params.iter();
        for segment in &route.segments {
            path.push('/');
            match segment {
                Segment::Fixed(text) => path.push_str(text),
                Segment::Param { .. } => {
                    path.push_str(&percent_encode(given.next().map_or("", String::as_str)))
                }
            }
        }
        if path.is_empty() {
            path.push('/');
        }
        let mut request = Request {
            method: route.method.word().to_string(),
            path,
            address: Some("entries".to_string()),
            ..Request::default()
        };
        let Some(document) = document else {
            return request;
        };
        let mut document = document.clone();
        let headers: Vec<(String, String)> = route
            .input
            .as_ref()
            .map(|plan| plan.headers.clone())
            .unwrap_or_default();
        // A header field from its member, unless it stays in the body.
        if shape != Shape::Headed
            && let Json::Object(members) = &mut document
        {
            members.retain(|(name, value)| {
                let Some((_, header)) = headers.iter().find(|(field, _)| field == name) else {
                    return true;
                };
                let text = match value {
                    Json::Raw(raw) => match serde_json::from_str::<serde_json::Value>(raw) {
                        Ok(serde_json::Value::String(text)) => text,
                        _ => raw.clone(),
                    },
                    other => other.text(),
                };
                request.headers.push((header.clone(), text));
                false
            });
        }
        if route.input.as_ref().is_some_and(|plan| plan.query) {
            request.query = Some(query_string(&document));
            return request;
        }
        request.body = document.text().into_bytes();
        let content = match shape {
            Shape::Json | Shape::Headed => Some("application/json"),
            Shape::Plain => Some("text/plain"),
            Shape::Problem => Some("application/problem+json; charset=utf-8"),
            Shape::Unsaid => None,
        };
        if let Some(content) = content {
            request
                .headers
                .push(("content-type".to_string(), content.to_string()));
        }
        request
    }
}

/// How a request's input is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// As JSON, saying so.
    Json,
    /// Saying it is plain text.
    Plain,
    /// Without a content type.
    Unsaid,
    /// As another JSON type.
    Problem,
    /// As JSON with its header fields left in the body.
    Headed,
}

/// A document's top members as a query string: each value's text, a
/// list's items as the key given again.
fn query_string(document: &Json) -> String {
    let Json::Object(members) = document else {
        return document.text();
    };
    let text = |json: &Json| match json {
        Json::Raw(raw) => match serde_json::from_str::<serde_json::Value>(raw) {
            Ok(serde_json::Value::String(text)) => text,
            _ => raw.clone(),
        },
        other => other.text(),
    };
    let mut pairs = Vec::new();
    for (name, value) in members {
        match value {
            Json::Array(items) => {
                for item in items {
                    pairs.push(format!(
                        "{}={}",
                        percent_encode(name),
                        percent_encode(&text(item))
                    ));
                }
            }
            Json::Raw(raw) if raw == "null" => {}
            other => pairs.push(format!(
                "{}={}",
                percent_encode(name),
                percent_encode(&text(other))
            )),
        }
    }
    pairs.join("&")
}

/// `text` with everything but unreserved characters percent-encoded.
fn percent_encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Scalars of `builtin`'s representation, two of a kind it holds and one
/// of another.
fn scalars(builtin: Builtin) -> Vec<Value> {
    let mut out = match builtin.representation() {
        Builtin::Bool => vec![Value::Bool(true), Value::Bool(false)],
        Builtin::Int => vec![Value::Int(7), Value::Int(-9_007_199_254_740_993)],
        Builtin::Decimal => Value::decimal("1.50").into_iter().collect(),
        Builtin::Uuid => vec![Value::uuid("0192f0c4-3b5e-7c1a-9d2e-4f6a8b0c1d2e")],
        Builtin::Instant => vec![Value::Instant(1_792_504_800_123)],
        Builtin::CalendarDate => vec![Value::Date(20_000)],
        Builtin::Duration => vec![Value::Duration(1_500)],
        Builtin::Bytes => vec![Value::Bytes(std::rc::Rc::from(&b"hi"[..]))],
        _ => vec![Value::text("abc"), Value::text("a\"b\\c\n\u{1}é")],
    };
    out.push(Value::text("x"));
    out
}
