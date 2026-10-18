//! Every standard type through the boundaries that are built (area 19,
//! "Every boundary through the contract"; chunk 5.5w2), made from each
//! type's data entry, so a type added later is tested without a line here.
//! A project with a route echoing each standard type is checked, and
//! `jadpo test` sends each of the entry's valid examples through:
//!
//! - a request's body and the response, through the route codec;
//! - a query string, for each type a query string holds;
//! - `json.write` and `json.read` with the value as a map's key, its
//!   canonical text, for each type with one;
//! - `text.parse` and `text.from`, for each type with a canonical text.
//!
//! The same examples, written by the plan's own codecs, are held to the
//! OpenAPI document's schema for each route's output. Settings are the
//! per-type harness's (`jadpo-interp`'s `standard_types`), which reads each
//! example through the setting's codec; the checker's half refuses a map
//! key, a query string field and a setting of a type the boundary can't
//! carry. Geometries and vectors wait on their areas; a secret value is
//! never written out and a password's hash never leaves the server, so
//! neither has a response.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use jadpo_cli::{Output, run};
use jadpo_std::HttpMethod;
use jadpo_std::http::{Codec, CodecId};
use jadpo_std::standard::{Holds, Json, STANDARD_TYPES, StandardType, Storage, TextForm};
use jadpo_term::Environment;
use jadpo_value::Value;
use jadpo_value::json::{Codecs, Path as JsonPath};

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("parent")).expect("folders");
    fs::write(path, text).expect("write");
}

fn jadpo(cwd: &Path, args: &[&str]) -> Output {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    let env = Environment::empty().with_var("SOURCE_DATE_EPOCH", "1792504800");
    run(&args, &env.with_cwd(cwd))
}

/// `text` as a Jadpo string literal.
fn literal(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// One standard type sent through the boundaries: its entry, its number,
/// the type a field names it by, and how a value is read from its text.
struct Case {
    entry: &'static StandardType,
    at: usize,
    ty: String,
}

impl Case {
    /// Whether it has a canonical text a program reads back.
    fn has_text(&self) -> bool {
        self.entry.text.is_some() && self.entry.text != Some(TextForm::Secret)
    }

    /// Whether it is built on `Text`, so `text.from` has nothing to give
    /// (area 19, "Chosen in chunk 5.5a").
    fn is_text(&self) -> bool {
        match self.entry.holds {
            Holds::Text { .. } => true,
            Holds::Builtin(builtin) => builtin.representation() == jadpo_std::Builtin::Text,
            _ => false,
        }
    }

    /// Whether a query string holds it: a scalar the contract's JSON sends
    /// as a string, a number or a `Bool`, or an enum without data; never an
    /// object, a choice or a collection (area 7), nor what JSON sends as an
    /// object, an amount of money, a quantity or a `ZonedTime`.
    fn in_query(&self) -> bool {
        match self.entry.holds {
            Holds::Builtin(builtin) => !matches!(
                builtin.representation(),
                jadpo_std::Builtin::Money
                    | jadpo_std::Builtin::ZonedTime
                    | jadpo_std::Builtin::Quantity(_)
            ),
            Holds::Bounded { of, .. } => !matches!(
                of.representation(),
                jadpo_std::Builtin::Money
                    | jadpo_std::Builtin::ZonedTime
                    | jadpo_std::Builtin::Quantity(_)
            ),
            Holds::Text { .. } | Holds::Whole { .. } | Holds::Enum(_) | Holds::Bytes => true,
            _ => false,
        }
    }

    /// Whether reading it from text can't fail: any text is `Html` once
    /// it is made safe (chunk 5.5o).
    fn infallible(&self) -> bool {
        self.entry.text == Some(TextForm::Html)
    }

    /// The text examples are read from with `text.parse` or, for a type
    /// with no single text form, `json.read`.
    fn reading(&self) -> String {
        let ty = &self.ty;
        if self.infallible() {
            format!("text.parse(raw, as: {ty})")
        } else if self.has_text() {
            format!("attempt text.parse(raw, as: {ty}) {{\n        invalid: BadExample\n    }}")
        } else {
            format!(
                "attempt json.read(raw, as: {ty}) {{\n        malformed: BadExample\n        mismatch: BadExample\n    }}"
            )
        }
    }
}

fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    for entry in STANDARD_TYPES {
        if matches!(entry.json, Json::GeoJson | Json::Secret)
            || entry.storage == Storage::Vector
            || entry.name == jadpo_std::standard::security::PASSWORD_HASH
        {
            continue;
        }
        let at = out.len();
        let ty = match (entry.holds, entry.examples.of) {
            // A built-in type is named by a type built on it.
            (Holds::Builtin(_), _) => format!("Sent{}", entry.name),
            (_, Some(item)) => format!("{}<{item}>", entry.name),
            _ => entry.name.to_string(),
        };
        out.push(Case { entry, at, ty });
    }
    out
}

/// The project: for each case an `Input` and an `Output` holding one
/// value, an action echoing it, a `POST` route and, where a query string
/// holds the type, a `GET` one; a reader for its examples; and a test per
/// case sending each example through every boundary that carries it.
fn project() -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("standard-boundaries");
    let _ = fs::remove_dir_all(&root);
    write(&root, "app.jadpo", "application {}\n");
    write(
        &root,
        "domains/edge/types/raw_text.jadpo",
        "/// An example's text.\ntype RawText = Text {}\n",
    );
    write(
        &root,
        "domains/edge/types/count.jadpo",
        "/// A map's value.\ntype Count = Int {\n    min: 0\n    max: 1000\n}\n",
    );
    write(
        &root,
        "domains/edge/failures/bad_example.jadpo",
        "/// An example that isn't its type.\ntype BadExample = Failure {\n    kind: FailureKind.invalid_value\n}\n",
    );
    let mut sent = Vec::new();
    let mut readers = String::new();
    let mut routes = String::new();
    let mut actions = Vec::new();
    let mut tests = String::new();
    let mut fixtures = Vec::new();
    for case in cases() {
        let (at, ty) = (case.at, &case.ty);
        let used = if let Holds::Builtin(_) = case.entry.holds {
            write(
                &root,
                &format!("domains/edge/types/{}.jadpo", snake(ty)),
                &format!(
                    "/// A `{name}`, sent.\ntype {ty} = {name} {{}}\n",
                    name = case.entry.name
                ),
            );
            sent.push(ty.clone());
            format!("use edge/types {{ {ty} }}\n\n")
        } else {
            String::new()
        };
        readers.push_str(&format!(
            "    /// The `{ty}` `raw` is.\n    function read_{at}(raw: RawText) {fails}-> {ty} {{\n        return {reading}\n    }}\n\n",
            fails = if case.infallible() { "" } else { "fails BadExample " },
            reading = case.reading().replace("\n", "\n    "),
        ));
        if case.has_text() {
            readers.push_str(&format!(
                "    /// The map of `{ty}`s `raw` is.\n    function keyed_{at}(raw: RawText) fails BadExample -> Map<{ty}, Count> {{\n        return attempt json.read(raw, as: Map<{ty}, Count>) {{\n            malformed: BadExample\n            mismatch: BadExample\n        }}\n    }}\n\n"
            ));
        }
        write(
            &root,
            &format!("domains/edge/actions/echo_{at}.jadpo"),
            &format!(
                "{used}/// One `{ty}` sent.\ntype Send{at} = Input {{\n    value: {ty}\n}}\n\n/// One `{ty}` sent back.\ntype Echo{at} = Output {{\n    value: {ty}\n}}\n\n/// Sends back what it was sent.\naction echo_{at}(input: Send{at}) -> Echo{at} {{\n    return Echo{at} {{\n        value: input.value\n    }}\n}}\n"
            ),
        );
        actions.push(format!("Send{at}, Echo{at}, echo_{at}"));
        routes.push_str(&format!(
            "\n    POST /t{at} {{\n        authentication: RouteAuthentication.none\n        input: Send{at}\n        output: Echo{at}\n        run: echo_{at}(input)\n    }}\n"
        ));
        if case.in_query() {
            routes.push_str(&format!(
                "\n    DELETE /t{at} {{\n        authentication: RouteAuthentication.none\n        input: Send{at}\n        output: Echo{at}\n        run: echo_{at}(input)\n    }}\n"
            ));
        }
        let mut body = String::new();
        for (n, (text, _)) in case.entry.examples.valid.iter().enumerate() {
            let fixture = format!("example_{at}_{n}");
            write(
                &root,
                &format!("domains/edge/fixtures/{fixture}.jadpo"),
                &format!(
                    "use edge/libraries {{ examples }}\n{used}\n/// Example {n} of `{ty}`.\nfixture {fixture}() -> {ty} {{\n    return {attempt}examples.read_{at}({raw})\n}}\n",
                    used = used.trim_end(),
                    attempt = if case.infallible() { "" } else { "attempt " },
                    raw = literal(text)
                ),
            );
            fixtures.push(fixture.clone());
            body.push_str(&format!("    var value_{n}: {ty} = {fixture}()\n"));
            let mut methods = vec!["post"];
            if case.in_query() {
                methods.push("delete");
            }
            for method in methods {
                body.push_str(&format!(
                    "    var {method}_{n}: Response = request {{\n        method: HttpMethod.{method}\n        path: \"/edge/t{at}\"\n        input: Send{at} {{\n            value: value_{n}\n        }}\n    }}\n    assert {method}_{n}.status == 200\n    assert {method}_{n}.output == Echo{at} {{\n        value: value_{n}\n    }}\n"
                ));
            }
            if case.has_text() {
                if !case.is_text() {
                    body.push_str(&format!(
                        "    assert text.from(value_{n}) == {raw}\n",
                        raw = literal(text)
                    ));
                }
                let mut key = String::from("{");
                jadpo_value::json::write_string(text, &mut key);
                key.push_str(":1}");
                let keyed = format!("keyed_{at}_{n}");
                write(
                    &root,
                    &format!("domains/edge/fixtures/{keyed}.jadpo"),
                    &format!(
                        "use edge/libraries {{ examples }}\nuse edge/types {{ Count{sent_one} }}\n\n/// Example {n} of `{ty}` as a map's key, read back.\nfixture {keyed}() -> Map<{ty}, Count> {{\n    return attempt examples.keyed_{at}({key})\n}}\n",
                        sent_one = if used.is_empty() {
                            String::new()
                        } else {
                            format!(", {ty}")
                        },
                        key = literal(&key),
                    ),
                );
                fixtures.push(keyed.clone());
                body.push_str(&format!(
                    "    var none_yet_{n}: Map<{ty}, Count> = []\n    var keyed_{n}: Map<{ty}, Count> = map.with(none_yet_{n}, value_{n}, 1)\n    assert json.write(keyed_{n}) == {key}\n    assert {keyed}() == keyed_{n}\n",
                    key = literal(&key),
                ));
            }
        }
        tests.push_str(&format!(
            "test \"a {ty} crosses every boundary that carries it\" {{\n{body}}}\n\n"
        ));
    }
    let sent_list = sent.iter().map(|ty| format!(", {ty}")).collect::<String>();
    write(
        &root,
        "domains/edge/libraries/examples.jadpo",
        &format!(
            "use edge/failures {{ BadExample }}\nuse edge/types {{ Count, RawText{sent_list} }}\n\n/// Each standard type's examples, read from their text.\nlibrary examples {{\n{}\n}}\n",
            readers.trim_end()
        ),
    );
    let action_list = actions.join(", ");
    write(
        &root,
        "domains/edge/routes/edge.jadpo",
        &format!(
            "use edge/actions {{ {action_list} }}\n\n/// Every standard type, sent back.\nroutes Edge {{\n    prefix: /edge\n{routes}}}\n"
        ),
    );
    let types_list = sent.iter().map(|ty| format!(", {ty}")).collect::<String>();
    let senders = cases()
        .iter()
        .map(|case| format!("Send{0}, Echo{0}", case.at))
        .collect::<Vec<_>>()
        .join(", ");
    write(
        &root,
        "domains/edge/routes/edge.test.jadpo",
        &format!(
            "use edge/actions {{ {senders} }}\nuse edge/fixtures {{ {} }}\nuse edge/types {{ Count{types_list} }}\n\n{}\n",
            fixtures.join(", "),
            tests.trim_end()
        ),
    );
    let granted = cases()
        .iter()
        .map(|case| format!("        edge.echo_{}: [Access.public]\n", case.at))
        .collect::<String>();
    write(
        &root,
        "policy.jadpo",
        &format!("use edge\n\npolicy {{\n    actions: {{\n{granted}    }}\n}}\n"),
    );
    root
}

/// `PascalCase` as `snake_case`.
fn snake(name: &str) -> String {
    let mut out = String::new();
    for (at, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if at > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else if c.is_ascii_alphanumeric() {
            out.push(c);
        }
    }
    out
}

// --- A JSON Schema check, as far as the OpenAPI document uses it -----------

/// Whether `value` meets `schema`, following `$ref`s into `components`;
/// `Err` names where it doesn't.
fn meets(
    value: &serde_json::Value,
    schema: &serde_json::Value,
    components: &serde_json::Value,
    at: &str,
) -> Result<(), String> {
    use serde_json::Value as J;
    let fail = |why: &str| Err(format!("{at}: {why} ({value} against {schema})"));
    if let Some(J::String(reference)) = schema.get("$ref") {
        let name = reference.rsplit('/').next().unwrap_or_default();
        let found = components
            .get(name)
            .ok_or_else(|| format!("{at}: no schema {name}"))?;
        return meets(value, found, components, at);
    }
    for (key, kind) in [("anyOf", false), ("oneOf", true)] {
        if let Some(J::Array(options)) = schema.get(key) {
            let met = options
                .iter()
                .filter(|option| meets(value, option, components, at).is_ok())
                .count();
            if met == 0 || (kind && met > 1) {
                return fail(&format!("{met} of `{key}` met"));
            }
        }
    }
    if let Some(ty) = schema.get("type") {
        let kinds: Vec<&str> = match ty {
            J::String(one) => vec![one.as_str()],
            J::Array(many) => many.iter().filter_map(J::as_str).collect(),
            _ => Vec::new(),
        };
        let is = |kind: &str| match kind {
            "string" => value.is_string(),
            "integer" => value.is_i64() || value.is_u64(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "object" => value.is_object(),
            "array" => value.is_array(),
            "null" => value.is_null(),
            _ => false,
        };
        if !kinds.iter().any(|kind| is(kind)) {
            return fail("not its type");
        }
    }
    if let Some(J::Array(allowed)) = schema.get("enum")
        && !allowed.contains(value)
    {
        return fail("not in `enum`");
    }
    if let Some(constant) = schema.get("const")
        && constant != value
    {
        return fail("not its `const`");
    }
    if let (Some(J::String(pattern)), J::String(text)) = (schema.get("pattern"), value) {
        let pattern = regex::Regex::new(pattern).map_err(|e| format!("{at}: {e}"))?;
        if !pattern.is_match(text) {
            return fail("doesn't match `pattern`");
        }
    }
    if let J::Number(number) = value {
        let n = number.as_f64().unwrap_or_default();
        if schema
            .get("minimum")
            .and_then(J::as_f64)
            .is_some_and(|min| n < min)
        {
            return fail("below `minimum`");
        }
        if schema
            .get("maximum")
            .and_then(J::as_f64)
            .is_some_and(|max| n > max)
        {
            return fail("above `maximum`");
        }
    }
    if let J::Object(members) = value {
        let properties = schema.get("properties").and_then(J::as_object);
        if let Some(J::Array(required)) = schema.get("required") {
            for name in required.iter().filter_map(J::as_str) {
                if !members.contains_key(name) {
                    return fail(&format!("`{name}` is required"));
                }
            }
        }
        for (name, member) in members {
            match properties.and_then(|properties| properties.get(name)) {
                Some(property) => meets(member, property, components, &format!("{at}.{name}"))?,
                None => match schema.get("additionalProperties") {
                    Some(J::Bool(false)) => return fail(&format!("`{name}` isn't allowed")),
                    Some(other @ J::Object(_)) => {
                        meets(member, other, components, &format!("{at}.{name}"))?
                    }
                    _ => {}
                },
            }
            if let Some(names) = schema.get("propertyNames") {
                meets(
                    &J::String(name.clone()),
                    names,
                    components,
                    &format!("{at} key"),
                )?;
            }
        }
    }
    if let (J::Array(items), Some(item)) = (value, schema.get("items")) {
        for (n, entry) in items.iter().enumerate() {
            meets(entry, item, components, &format!("{at}.{n}"))?;
        }
    }
    Ok(())
}

#[test]
fn every_standard_type_crosses_every_built_boundary() {
    let root = project();
    let recorded = jadpo(&root, &["api", "update"]);
    assert_eq!(recorded.code, 0, "{}{}", recorded.stdout, recorded.stderr);
    let checked = jadpo(&root, &["check"]);
    assert_eq!(checked.code, 0, "{}{}", checked.stdout, checked.stderr);
    let tested = jadpo(&root, &["test", "--seed", "1"]);
    assert_eq!(tested.code, 0, "{}{}", tested.stdout, tested.stderr);
    let count = cases().len();
    assert!(
        tested.stdout.contains(&format!("{count} passed")),
        "{}",
        tested.stdout
    );

    // OpenAPI's shape: each example as the plan's codec writes it meets the
    // schema the document gives the route's output.
    let loaded = jadpo_db::load(&root);
    let documents = jadpo_check::routes::openapi::documents(&loaded.db, loaded.project, "edge");
    let document: BTreeMap<String, serde_json::Value> = documents
        .into_iter()
        .map(|(name, value)| {
            (
                name,
                serde_json::from_str(&value.to_document()).expect("JSON"),
            )
        })
        .collect();
    let document = document.values().next().expect("an OpenAPI document");
    let components = &document["components"]["schemas"];
    let prepared = jadpo_interp::Prepared::load(&loaded.db, loaded.project).expect("checks");
    let program = jadpo_interp::Program::new(prepared);
    let app = program.app();
    let codecs: Codecs<'_> = app.codecs;
    for case in cases() {
        let path = format!("/edge/t{}", case.at);
        let route = app
            .table
            .routes
            .iter()
            .find(|route| {
                route.method == HttpMethod::Post
                    && jadpo_std::http::path_text(&route.segments) == path
            })
            .unwrap_or_else(|| panic!("{path}"));
        let output = route.output.expect("an output");
        let Some(Codec::Object { fields, .. }) = codecs.codecs.get(output.0 as usize) else {
            panic!("{path}: an object");
        };
        let field: CodecId = fields[0];
        let schema = &components[format!("Echo{}", case.at)]["properties"]["value"];
        assert!(
            schema.as_object().is_some_and(|schema| !schema.is_empty()),
            "{}: a schema ({schema})",
            case.ty
        );
        // Nothing meets a schema by having no shape: an array of nothing
        // meets only a list's.
        let collection = matches!(case.entry.holds, Holds::List(_));
        assert_eq!(
            meets(&serde_json::json!([]), schema, components, &case.ty).is_ok(),
            collection,
            "{}: {schema}",
            case.ty
        );
        for (text, _) in case.entry.examples.valid {
            let value: Value = if case.has_text() {
                let read_as = match case.entry.examples.of {
                    Some(item) => format!("{}<{item}>", case.entry.name),
                    None => case.entry.name.to_string(),
                };
                jadpo_value::standard::parse(text, &read_as)
                    .unwrap_or_else(|fault| panic!("{}: {text} ({fault:?})", case.ty))
            } else {
                let json: serde_json::Value = serde_json::from_str(text).expect("JSON");
                let mut problems = Vec::new();
                codecs
                    .read(field, &json, &JsonPath::default(), &mut problems)
                    .unwrap_or_else(|| panic!("{}: {text} ({problems:?})", case.ty))
            };
            let mut written = String::new();
            codecs.write(field, &value, &mut written);
            let json: serde_json::Value = serde_json::from_str(&written).expect("JSON");
            meets(&json, schema, components, &case.ty)
                .unwrap_or_else(|why| panic!("{}: {why}", case.ty));
        }
    }
}

// --- No boundary has an encoder of its own (chunk 5.5w3) -------------------

/// The source files that encode a value at a boundary: the JSON reader
/// and writer (requests, responses, `json.read`, `json.write`, query
/// strings, headers, settings and map keys all read through them) and
/// OpenAPI's schemas. The plan's codecs name only which codec a type
/// takes, never how it is written.
const BOUNDARIES: &[&str] = &[
    "crates/jadpo-value/src/json.rs",
    "crates/jadpo-check/src/routes/wire.rs",
];

/// A file's code, without its comments and its tests: what it runs.
fn code_of(text: &str) -> String {
    let text = text
        .split("#[cfg(test)]\nmod tests")
        .next()
        .unwrap_or_default();
    text.lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

/// No boundary encodes a standard type of its own (area 19, "Every
/// boundary through the contract"): none names a standard type a built-in
/// type holds, the runtime value that holds it, or the parts the contract
/// sends one as, so each reads the type's entry and calls the contract's
/// JSON or text. The names are made from the entries, so a type added
/// later is held to it without a line here.
#[test]
fn no_boundary_has_an_encoder_of_its_own() {
    let mut names = std::collections::BTreeSet::new();
    for entry in STANDARD_TYPES {
        let Holds::Builtin(builtin) = entry.holds else {
            continue;
        };
        let shown = format!("{builtin:?}");
        names.insert(format!(
            "Builtin::{}",
            shown.split('(').next().unwrap_or_default()
        ));
        // The runtime value one of its examples is, unless that is text,
        // which a `Text` is too.
        if let Some((example, _)) = entry.examples.valid.first() {
            let value = jadpo_value::standard::read(entry, example)
                .unwrap_or_else(|fault| panic!("{}: {example} ({fault:?})", entry.name));
            let shown = format!("{value:?}");
            let variant = shown
                .split(|c: char| !c.is_ascii_alphanumeric())
                .next()
                .unwrap_or_default();
            if variant != "Text" {
                names.insert(format!("Value::{variant}"));
            }
        }
    }
    // The parts the contract sends an object of, and its writers of text.
    names.extend(
        [
            "MONEY_FIELDS",
            "QUANTITY_FIELDS",
            "format_instant",
            "format_date",
            "iso_duration",
            "zone_name",
        ]
        .map(String::from),
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut found = Vec::new();
    for file in BOUNDARIES {
        let text = fs::read_to_string(root.join(file)).expect("a boundary's source");
        let code = code_of(&text);
        for name in &names {
            if code.contains(name.as_str()) {
                found.push(format!("{file}: {name}"));
            }
        }
    }
    assert!(
        found.is_empty(),
        "a boundary encodes a standard type of its own:\n{}",
        found.join("\n")
    );
}
