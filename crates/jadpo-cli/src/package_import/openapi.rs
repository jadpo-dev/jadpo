//! The OpenAPI importer (area 11, "Where packages come from"; chunk 7.2):
//! an OpenAPI 3 document, read once, as a provider package.
//!
//! What it reads, and how (each choice is in area 11's "Chosen in chunk
//! 7.2"):
//!
//! - **Operations** by path, then `get`, `put`, `post`, `delete` and
//!   `patch`, grouped into resources by their first tag, or else by their
//!   path's first fixed segment that isn't a version (`v1`). An operation
//!   on the resource's own path is `list` or `create`, on that path and a
//!   placeholder `get`, `update` (`replace` for a `PUT` beside a `PATCH`)
//!   or `delete`; any other is named from its `operationId`, or for a
//!   Jadpo application's from its method and fixed segments. A Jadpo
//!   application's generated routes (`x-jadpo-generated`) are its
//!   runtime's, and left out.
//! - **An operation's input** is its body's type when it has no
//!   placeholders or query parameters, else a type of its own,
//!   `<Resource><Operation>Input`, holding its placeholders, its query
//!   parameters and its body's fields. Header and cookie parameters, and
//!   bodies that aren't JSON, are left out and listed.
//! - **Its output** is its lowest `2xx` answer's JSON body.
//! - **Types** are the schemas the operations reach, in `UpperCamelCase`,
//!   one renamed `<Name>Type` where it would take a built-in type's name;
//!   an object inside one is a type of its own, `<Parent><Field>`. A field
//!   or variant is `lower_snake_case`; where that isn't the provider's own
//!   name, or a reserved word had to be avoided, it is listed.
//! - **Area 11's exact mappings** (its first layer): `date-time` is
//!   `Instant`, `date` `CalendarDate`, `email` `Email`, `uri` `Url`, `uuid`
//!   `Uuid`, `ipv4` and `ipv6` `IpAddress`, `byte` `Bytes`, `duration`
//!   `Duration`, and an enumeration of ISO 4217 codes `Currency`. A shape
//!   with no Jadpo form (`oneOf`, an object of any value) is left out and
//!   listed. Shape and name detection comes with chunk 7.2e.
//! - **A Jadpo application's document is read exactly**, from its `x-jadpo`
//!   fields (area 7): each field's type from `x-jadpo-constraints`, which
//!   operations change something and which are safe to repeat, its
//!   failures with their codes and kinds, and its idempotency key.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::model::{
    Field, Finding, FailureDecl, Idempotency, Operation, Package, Provider, Resource, Ty,
    TypeDecl, camel, snake,
};
use super::notation::{self, Written};

/// The methods an operation can have, in the order they are read.
const METHODS: [&str; 5] = ["get", "put", "post", "delete", "patch"];

/// Why a document isn't one the importer reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invalid(pub String);

/// The package `document` describes, named `name` at `version`, with the
/// file it came from and that file's digest.
pub fn read(
    document: &Value,
    name: &str,
    version: &str,
    file: &str,
    digest: &str,
) -> Result<Package, Invalid> {
    let declared = document
        .get("openapi")
        .and_then(Value::as_str)
        .ok_or_else(|| Invalid("it has no `openapi` version, so it isn't an OpenAPI document".to_string()))?;
    if !declared.starts_with("3.") {
        return Err(Invalid(format!(
            "it is OpenAPI {declared}, and the importer reads OpenAPI 3.0 and 3.1"
        )));
    }
    let paths = document
        .get("paths")
        .and_then(Value::as_object)
        .ok_or_else(|| Invalid("it has no `paths`".to_string()))?;
    let exact = paths
        .values()
        .filter_map(Value::as_object)
        .flat_map(|item| item.values())
        .any(|operation| operation.get("x-jadpo-changes").is_some());
    let title = document
        .pointer("/info/title")
        .and_then(Value::as_str)
        .unwrap_or(name)
        .to_string();
    let mut reader = Reader {
        document,
        exact,
        names: BTreeMap::new(),
        taken: BTreeSet::new(),
        types: BTreeMap::new(),
        failures: BTreeMap::new(),
        findings: Vec::new(),
        queue: Vec::new(),
        done: BTreeSet::new(),
    };
    let mut resources: BTreeMap<String, Resource> = BTreeMap::new();
    let mut read: Vec<(String, Read)> = Vec::new();
    for (path, item) in paths {
        let Some(item) = item.as_object() else {
            continue;
        };
        for method in METHODS {
            let Some(operation) = item.get(method).and_then(Value::as_object) else {
                continue;
            };
            if operation.get("x-jadpo-generated") == Some(&Value::Bool(true)) {
                continue;
            }
            let resource = resource_of(operation, path);
            read.push((
                resource,
                Read {
                    method,
                    path: path.clone(),
                    operation: operation.clone(),
                    shared: item
                        .get("parameters")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default(),
                },
            ));
        }
    }
    // Each operation's name in its resource, then what it takes and gives.
    let named = names(&read, exact);
    for ((resource, read), name) in read.iter().zip(named) {
        let operation = reader.operation(resource, &name, read);
        resources
            .entry(resource.clone())
            .or_default()
            .operations
            .insert(name, operation);
    }
    reader.drain();
    // Each resource's description is its tag's.
    if let Some(tags) = document.get("tags").and_then(Value::as_array) {
        for tag in tags {
            let (Some(name), Some(description)) = (
                tag.get("name").and_then(Value::as_str),
                tag.get("description").and_then(Value::as_str),
            ) else {
                continue;
            };
            if let Some(resource) = resources.get_mut(&resource_name(name)) {
                resource.doc = Some(description.to_string());
            }
        }
    }
    let provider = reader.provider(&title, file, digest);
    let description = document
        .pointer("/info/description")
        .and_then(Value::as_str)
        .and_then(|text| text.lines().find(|line| !line.trim().is_empty()))
        .map_or_else(
            || format!("{title}, imported from its OpenAPI document."),
            |line| line.trim().to_string(),
        );
    Ok(Package {
        name: name.to_string(),
        version: version.to_string(),
        description,
        provider,
        resources,
        types: reader.types,
        failures: reader.failures,
        findings: reader.findings,
        exact,
    })
}

/// An operation as found, before it is named.
struct Read {
    method: &'static str,
    path: String,
    operation: serde_json::Map<String, Value>,
    /// The parameters its path item gives every operation.
    shared: Vec<Value>,
}

/// The resource an operation belongs to: its first tag, else its path's
/// first fixed segment that isn't a version.
fn resource_of(operation: &serde_json::Map<String, Value>, path: &str) -> String {
    if let Some(tag) = operation
        .get("tags")
        .and_then(Value::as_array)
        .and_then(|tags| tags.first())
        .and_then(Value::as_str)
    {
        return resource_name(tag);
    }
    path.split('/')
        .filter(|segment| !segment.is_empty() && !segment.starts_with('{'))
        .find(|segment| !is_version(segment))
        .map_or_else(|| "root".to_string(), resource_name)
}

fn resource_name(text: &str) -> String {
    let name = snake(text);
    if name.is_empty() {
        "root".to_string()
    } else if name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("r_{name}")
    } else {
        name
    }
}

fn is_version(segment: &str) -> bool {
    segment
        .strip_prefix('v')
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
}

/// Each operation's name in its resource, in the order given.
fn names(read: &[(String, Read)], exact: bool) -> Vec<String> {
    // A resource's own path: the shortest of its paths that ends in a
    // fixed segment.
    let mut base: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (resource, read) in read {
        let segments: Vec<&str> = read.path.split('/').filter(|s| !s.is_empty()).collect();
        if segments.last().is_some_and(|last| !last.starts_with('{')) {
            let entry = base.entry(resource).or_insert_with(|| segments.clone());
            if segments.len() < entry.len() {
                *entry = segments;
            }
        }
    }
    let crud: Vec<Option<&str>> = read
        .iter()
        .map(|(resource, read)| {
            let base = base.get(resource.as_str())?;
            let segments: Vec<&str> = read.path.split('/').filter(|s| !s.is_empty()).collect();
            let item = segments.len() == base.len() + 1
                && segments.starts_with(base)
                && segments.last().is_some_and(|last| last.starts_with('{'));
            if segments == *base {
                match read.method {
                    "get" => Some("list"),
                    "post" => Some("create"),
                    _ => None,
                }
            } else if item {
                match read.method {
                    "get" => Some("get"),
                    "patch" => Some("update"),
                    "put" => Some("update"),
                    "delete" => Some("delete"),
                    _ => None,
                }
            } else {
                None
            }
        })
        .collect();
    // A `PUT` beside a `PATCH` on one path replaces.
    let crud: Vec<Option<&str>> = crud
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let (resource, one) = &read[i];
            if *name == Some("update")
                && one.method == "put"
                && read.iter().any(|(other_resource, other)| {
                    other_resource == resource && other.path == one.path && other.method == "patch"
                })
            {
                Some("replace")
            } else {
                *name
            }
        })
        .collect();
    let fallback = |read: &Read| -> String {
        let id = read
            .operation
            .get("operationId")
            .and_then(Value::as_str)
            .map(snake)
            .filter(|id| !id.is_empty());
        let derived = || {
            let fixed: Vec<String> = read
                .path
                .split('/')
                .filter(|segment| !segment.is_empty() && !segment.starts_with('{'))
                .skip(1)
                .map(snake)
                .collect();
            if fixed.is_empty() {
                read.method.to_string()
            } else {
                format!("{}_{}", read.method, fixed.join("_"))
            }
        };
        match (exact, id) {
            (false, Some(id)) => id,
            _ => derived(),
        }
    };
    let mut chosen: Vec<String> = read
        .iter()
        .zip(&crud)
        .map(|((_, read), crud)| crud.map_or_else(|| fallback(read), str::to_string))
        .collect();
    // A name taken twice in one resource falls back to the operation's id,
    // then to its method and whole path.
    for round in 0..2 {
        let mut seen: BTreeMap<(String, String), usize> = BTreeMap::new();
        for (i, (resource, _)) in read.iter().enumerate() {
            *seen
                .entry((resource.clone(), chosen[i].clone()))
                .or_default() += 1;
        }
        for (i, (resource, one)) in read.iter().enumerate() {
            if seen[&(resource.clone(), chosen[i].clone())] < 2 {
                continue;
            }
            chosen[i] = match round {
                0 => one
                    .operation
                    .get("operationId")
                    .and_then(Value::as_str)
                    .map(snake)
                    .filter(|id| !id.is_empty())
                    .unwrap_or_else(|| whole(one)),
                _ => whole(one),
            };
        }
    }
    chosen.into_iter().map(|name| safe_name(&name)).collect()
}

/// `<method>_<every segment>`, placeholders by name.
fn whole(read: &Read) -> String {
    let segments: Vec<String> = read
        .path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| snake(segment.trim_matches(|c| c == '{' || c == '}')))
        .collect();
    format!("{}_{}", read.method, segments.join("_"))
}

/// `name` made a Jadpo name: a reserved word, or one that doesn't start
/// with a letter, gets `_value` or `n_`.
fn safe_name(name: &str) -> String {
    let name = if name.is_empty() {
        "value".to_string()
    } else if name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("n_{name}")
    } else {
        name.to_string()
    };
    if fits_as_field(&name) {
        name
    } else {
        format!("{name}_value")
    }
}

/// Whether `name` reads as a field's name.
fn fits_as_field(name: &str) -> bool {
    let text = format!("type T = Object {{\n    {name}: Text\n}}\n");
    !jadpo_syntax::parse_file(&text).has_errors()
}

/// Whether `name` reads as a variant's name; `all` is the enum's own list
/// (area 19, question 22).
fn fits_as_variant(name: &str) -> bool {
    let text = format!("type T = Enum {{\n    {name}\n}}\n");
    name != "all" && !jadpo_syntax::parse_file(&text).has_errors()
}

/// A type's name that would take a built-in or standard type's.
fn is_taken_type(name: &str) -> bool {
    jadpo_std::Builtin::from_name(name).is_some()
        || jadpo_std::standard_type(name).is_some()
        || [
            "Object", "Enum", "Failure", "Input", "Output", "Event", "List", "Set", "Map", "Page",
            "Currency", "Country", "Language", "Locale", "Zone", "Json",
        ]
        .contains(&name)
}

struct Reader<'d> {
    document: &'d Value,
    exact: bool,
    /// Each schema's name in the package.
    names: BTreeMap<String, String>,
    /// Every type and failure name given.
    taken: BTreeSet<String>,
    types: BTreeMap<String, TypeDecl>,
    failures: BTreeMap<String, FailureDecl>,
    findings: Vec<Finding>,
    /// Schemas named but not yet read.
    queue: Vec<String>,
    done: BTreeSet<String>,
}

impl Reader<'_> {
    fn find(&mut self, at: impl Into<String>, next: impl Into<String>) {
        let finding = Finding {
            at: at.into(),
            next: next.into(),
        };
        if !self.findings.contains(&finding) {
            self.findings.push(finding);
        }
    }

    fn schema(&self, name: &str) -> Option<&Value> {
        self.document
            .pointer(&format!("/components/schemas/{}", name.replace('~', "~0").replace('/', "~1")))
    }

    /// What `$ref` names, resolved through `components`.
    fn resolve<'v>(&'v self, value: &'v Value) -> &'v Value {
        let mut value = value;
        for _ in 0..16 {
            let Some(reference) = value.get("$ref").and_then(Value::as_str) else {
                return value;
            };
            match reference
                .strip_prefix('#')
                .and_then(|pointer| self.document.pointer(pointer))
            {
                Some(found) => value = found,
                None => return value,
            }
        }
        value
    }

    /// The package's name for schema `name`, unique, claimed once.
    fn type_name(&mut self, schema: &str) -> String {
        if let Some(name) = self.names.get(schema) {
            return name.clone();
        }
        let mut name = camel(schema);
        if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
            name = format!("Schema{name}");
        }
        if is_taken_type(&name) {
            let renamed = format!("{name}Type");
            self.find(
                renamed.clone(),
                format!("Named `{renamed}`, since `{name}` is Jadpo's own type: check whether it is the standard `{name}`, and use that if so"),
            );
            name = renamed;
        }
        let name = self.unique(name);
        self.names.insert(schema.to_string(), name.clone());
        self.queue.push(schema.to_string());
        name
    }

    fn unique(&mut self, name: String) -> String {
        let mut candidate = name.clone();
        let mut n = 2;
        while self.taken.contains(&candidate) {
            candidate = format!("{name}{n}");
            n += 1;
        }
        self.taken.insert(candidate.clone());
        candidate
    }

    /// Reads every schema named so far, and the ones they name.
    fn drain(&mut self) {
        while let Some(schema) = self.queue.pop() {
            if !self.done.insert(schema.clone()) {
                continue;
            }
            let name = self.names[&schema].clone();
            let Some(value) = self.schema(&schema).cloned() else {
                continue;
            };
            if let Some(decl) = self.declaration(&name, &value) {
                self.types.insert(name, decl);
            }
        }
    }

    /// A named schema as a type: an object's fields or an enum's variants.
    fn declaration(&mut self, name: &str, schema: &Value) -> Option<TypeDecl> {
        let schema = self.resolve(schema).clone();
        let doc = schema
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(values) = schema.get("enum").and_then(Value::as_array) {
            let variants = self.variants(name, values);
            return Some(TypeDecl::Enum { doc, variants });
        }
        if schema.get("properties").is_some() || types_of(&schema).contains(&"object") {
            let fields = self.fields(name, &schema);
            return Some(TypeDecl::Object { doc, fields });
        }
        // A named scalar: its type, with its rules.
        match self.ty(&schema, name, &format!("{name}Value")) {
            Some(ty) if ty.text.chars().all(|c| c.is_ascii_alphanumeric()) => {
                Some(TypeDecl::Refined {
                    doc,
                    base: ty.text,
                    rules: ty.rules,
                })
            }
            _ => {
                self.find(
                    name.to_string(),
                    "Left out: its schema has no Jadpo form; declare it from the provider's documentation",
                );
                None
            }
        }
    }

    fn variants(&mut self, at: &str, values: &[Value]) -> Vec<String> {
        let mut out = Vec::new();
        for value in values {
            let Some(text) = value.as_str() else {
                self.find(
                    at.to_string(),
                    "A value of its enumeration isn't text, so it was left out",
                );
                continue;
            };
            let mut variant = snake(text);
            if variant.is_empty() || variant.starts_with(|c: char| c.is_ascii_digit()) {
                variant = format!("v_{variant}");
            }
            if !fits_as_variant(&variant) {
                variant = format!("{variant}_value");
            }
            if variant != text {
                self.find(
                    format!("{at}.{variant}"),
                    format!("The provider sends `{text}`; until a variant can name its wire value, this one is decoded by its name"),
                );
            }
            if !out.contains(&variant) {
                out.push(variant);
            }
        }
        out
    }

    /// An object schema's fields, in name order.
    fn fields(&mut self, parent: &str, schema: &Value) -> Vec<Field> {
        let schema = self.resolve(schema).clone();
        let mut fields = Vec::new();
        // `allOf` objects merge.
        if let Some(parts) = schema.get("allOf").and_then(Value::as_array) {
            for part in parts {
                for field in self.fields(parent, part) {
                    if !fields.iter().any(|known: &Field| known.name == field.name) {
                        fields.push(field);
                    }
                }
            }
        }
        let required: BTreeSet<&str> = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|all| all.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
            return fields;
        };
        for (wire, property) in properties {
            let mut name = snake(wire);
            if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
                name = format!("n_{name}");
            }
            if !fits_as_field(&name) {
                name = format!("{name}_value");
            }
            let at = format!("{parent}.{name}");
            if name != *wire {
                self.find(
                    at.clone(),
                    format!("The provider names it `{wire}`; until a field can name its wire name, this one is decoded by its name"),
                );
            }
            let synthesised = format!("{parent}{}", camel(&name));
            let Some(mut ty) = self.ty(property, &at, &synthesised) else {
                continue;
            };
            if !required.contains(wire.as_str()) {
                ty.optional = true;
            }
            if fields.iter().any(|known: &Field| known.name == name) {
                continue;
            }
            fields.push(Field { name, ty });
        }
        fields
    }

    /// A schema as a field's type; `None`, with a finding, where it has no
    /// Jadpo form. `at` names it for findings, and `synthesised` is the
    /// name an object inside it would get.
    fn ty(&mut self, schema: &Value, at: &str, synthesised: &str) -> Option<Ty> {
        if let Some(text) = schema.get("x-jadpo-constraints").and_then(Value::as_str) {
            if let Some(written) = notation::parse(text) {
                return Some(self.written(&written, synthesised));
            }
        }
        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            let Some(schema_name) = reference.strip_prefix("#/components/schemas/") else {
                self.find(
                    at.to_string(),
                    format!("Left out: it refers to `{reference}`, outside the document's schemas"),
                );
                return None;
            };
            let name = self.type_name(schema_name);
            return Some(Ty {
                text: name,
                ..Ty::default()
            });
        }
        // `nullable` (3.0), a type list with `null` (3.1), or one of a
        // type and `null`.
        let mut optional = schema.get("nullable") == Some(&Value::Bool(true));
        for key in ["anyOf", "oneOf", "allOf"] {
            let Some(options) = schema.get(key).and_then(Value::as_array) else {
                continue;
            };
            let real: Vec<&Value> = options
                .iter()
                .filter(|option| types_of(option) != ["null"])
                .collect();
            if real.len() < options.len() {
                optional = true;
            }
            if let [one] = real.as_slice() {
                let mut ty = self.ty(one, at, synthesised)?;
                ty.optional |= optional;
                return Some(ty);
            }
            if key == "allOf" && schema.get("properties").is_none() {
                return self.inline(schema, at, synthesised, optional);
            }
            self.find(
                at.to_string(),
                format!("Left out: it is `{key}` of {} shapes, which Jadpo has no type for; declare it from the provider's documentation", real.len()),
            );
            return None;
        }
        let kinds = types_of(schema);
        if kinds.contains(&"null") {
            optional = true;
        }
        let kind = kinds.iter().find(|kind| **kind != "null").copied();
        let mut ty = match kind {
            Some("string") => self.string(schema, at, synthesised)?,
            Some("integer") => Ty {
                text: "Int".to_string(),
                rules: bounds(schema),
                ..Ty::default()
            },
            Some("number") => Ty {
                text: "Decimal".to_string(),
                rules: bounds(schema),
                ..Ty::default()
            },
            Some("boolean") => Ty {
                text: "Bool".to_string(),
                ..Ty::default()
            },
            Some("array") => {
                let items = schema.get("items").unwrap_or(&Value::Null);
                let item = self.item(items, at, &format!("{synthesised}Item"))?;
                let mut rules = Vec::new();
                if let Some(min) = schema.get("minItems").and_then(Value::as_u64) {
                    rules.push(("min_length".to_string(), min.to_string()));
                }
                if let Some(max) = schema.get("maxItems").and_then(Value::as_u64) {
                    rules.push(("max_length".to_string(), max.to_string()));
                }
                let collection = if schema.get("uniqueItems") == Some(&Value::Bool(true)) {
                    "Set"
                } else {
                    "List"
                };
                Ty {
                    text: format!("{collection}<{item}>"),
                    rules,
                    ..Ty::default()
                }
            }
            Some("object") | None if schema.get("properties").is_some() => {
                return self.inline(schema, at, synthesised, optional);
            }
            Some("object") => match schema.get("additionalProperties") {
                Some(value) if value.is_object() && value.as_object().is_some_and(|o| !o.is_empty()) => {
                    let item = self.item(value, at, &format!("{synthesised}Value"))?;
                    Ty {
                        text: format!("Map<Text, {item}>"),
                        ..Ty::default()
                    }
                }
                _ => {
                    self.find(
                        at.to_string(),
                        "Left out: it is an object of any fields, which Jadpo has no type for; declare its fields from the provider's documentation",
                    );
                    return None;
                }
            },
            None if schema.get("enum").is_some() => self.string(schema, at, synthesised)?,
            _ => {
                self.find(
                    at.to_string(),
                    "Left out: its schema gives no type; declare it from the provider's documentation",
                );
                return None;
            }
        };
        ty.optional |= optional;
        Some(ty)
    }

    /// A list's or map's item: a plain type's name, or a type of its own
    /// for one with rules or that may be absent.
    fn item(&mut self, schema: &Value, at: &str, synthesised: &str) -> Option<String> {
        let ty = self.ty(schema, at, synthesised)?;
        if ty.rules.is_empty() && !ty.optional {
            return Some(ty.text);
        }
        if ty.rules.is_empty() {
            return Some(format!("{}?", ty.text));
        }
        let name = self.unique(synthesised.to_string());
        self.types.insert(
            name.clone(),
            TypeDecl::Refined {
                doc: None,
                base: ty.text,
                rules: ty.rules,
            },
        );
        Some(if ty.optional { format!("{name}?") } else { name })
    }

    /// An object inside another: a type of its own.
    fn inline(&mut self, schema: &Value, at: &str, synthesised: &str, optional: bool) -> Option<Ty> {
        let name = self.unique(synthesised.to_string());
        let fields = self.fields(&name, schema);
        if fields.is_empty() {
            self.taken.remove(&name);
            self.find(
                at.to_string(),
                "Left out: an object with no fields Jadpo can type; declare it from the provider's documentation",
            );
            return None;
        }
        let doc = schema
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string);
        self.types.insert(name.clone(), TypeDecl::Object { doc, fields });
        Some(Ty {
            text: name,
            optional,
            ..Ty::default()
        })
    }

    /// A string schema: its format's type (area 11's exact mappings), an
    /// enumeration, or `Text` with its rules.
    fn string(&mut self, schema: &Value, at: &str, synthesised: &str) -> Option<Ty> {
        let plain = |text: &str| Ty {
            text: text.to_string(),
            ..Ty::default()
        };
        if let Some(values) = schema.get("enum").and_then(Value::as_array) {
            let texts: Vec<&str> = values.iter().filter_map(Value::as_str).collect();
            if !texts.is_empty()
                && texts.len() == values.len()
                && texts.iter().all(|code| {
                    code.len() == 3
                        && code.chars().all(|c| c.is_ascii_uppercase())
                        && jadpo_std::money::ordinal_of_code(code).is_some()
                })
            {
                return Some(plain("Currency"));
            }
            let name = self.unique(synthesised.to_string());
            let doc = schema
                .get("description")
                .and_then(Value::as_str)
                .map(str::to_string);
            let variants = self.variants(&name, values);
            self.types
                .insert(name.clone(), TypeDecl::Enum { doc, variants });
            return Some(plain(&name));
        }
        let format = schema.get("format").and_then(Value::as_str).unwrap_or("");
        let mapped = match format {
            "date-time" => Some("Instant"),
            "date" => Some("CalendarDate"),
            "email" | "idn-email" => Some("Email"),
            "uri" | "url" | "iri" => Some("Url"),
            "uuid" => Some("Uuid"),
            "ipv4" | "ipv6" => Some("IpAddress"),
            "byte" => Some("Bytes"),
            "duration" => Some("Duration"),
            "binary" => {
                self.find(
                    at.to_string(),
                    "Left out: it is a file's bytes, which a JSON package doesn't send",
                );
                return None;
            }
            _ => None,
        };
        if let Some(mapped) = mapped {
            return Some(plain(mapped));
        }
        let mut rules = Vec::new();
        if let Some(min) = schema.get("minLength").and_then(Value::as_u64) {
            rules.push(("min_length".to_string(), min.to_string()));
        }
        if let Some(max) = schema.get("maxLength").and_then(Value::as_u64) {
            rules.push(("max_length".to_string(), max.to_string()));
        }
        if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
            let pattern = pattern
                .strip_prefix('^')
                .and_then(|rest| rest.strip_suffix('$'))
                .unwrap_or(pattern);
            rules.push(("pattern".to_string(), super::model::quote(pattern)));
        }
        Some(Ty {
            text: "Text".to_string(),
            rules,
            ..Ty::default()
        })
    }

    /// A type as a Jadpo application's record writes it, as written in
    /// source: a constrained item of a list or map becomes a type of its
    /// own.
    fn written(&mut self, written: &Written, synthesised: &str) -> Ty {
        let rules = written
            .rules
            .iter()
            .map(|(rule, value)| (rule.clone(), notation::source_value(rule, value)))
            .collect();
        let text = if written.args.is_empty() {
            self.named(&written.name)
        } else {
            let args: Vec<String> = written
                .args
                .iter()
                .enumerate()
                .map(|(i, arg)| {
                    let suffix = match (written.name.as_str(), i) {
                        ("Map", 0) => "Key",
                        ("Map", _) => "Value",
                        _ => "Item",
                    };
                    let inner = self.written(arg, &format!("{synthesised}{suffix}"));
                    if inner.rules.is_empty() {
                        if inner.optional {
                            format!("{}?", inner.text)
                        } else {
                            inner.text
                        }
                    } else {
                        let name = self.unique(format!("{synthesised}{suffix}"));
                        self.types.insert(
                            name.clone(),
                            TypeDecl::Refined {
                                doc: None,
                                base: inner.text,
                                rules: inner.rules,
                            },
                        );
                        if inner.optional { format!("{name}?") } else { name }
                    }
                })
                .collect();
            format!("{}<{}>", written.name, args.join(", "))
        };
        Ty {
            text,
            optional: written.optional,
            rules,
        }
    }

    /// A name the record writes: a built-in or standard type as it is, an
    /// object or enum by its schema's name in the package.
    fn named(&mut self, name: &str) -> String {
        if self.schema(name).is_some() {
            return self.type_name(name);
        }
        name.to_string()
    }

    /// One operation: what it takes, gives and fails with.
    fn operation(&mut self, resource: &str, name: &str, read: &Read) -> Operation {
        let at = format!("{resource}.{name}");
        let operation = &read.operation;
        let doc = operation
            .get("summary")
            .and_then(Value::as_str)
            .or_else(|| operation.get("description").and_then(Value::as_str))
            .and_then(|text| text.split("\n\n").next())
            .map(|text| text.trim().to_string());
        let stem = format!("{}{}", camel(resource), camel(name));
        // Its placeholders and query parameters, the operation's own over
        // its path's.
        let mut parameters: BTreeMap<(String, String), Value> = BTreeMap::new();
        for parameter in read
            .shared
            .iter()
            .chain(operation.get("parameters").and_then(Value::as_array).into_iter().flatten())
        {
            let parameter = self.resolve(parameter).clone();
            let (Some(place), Some(wire)) = (
                parameter.get("in").and_then(Value::as_str),
                parameter.get("name").and_then(Value::as_str),
            ) else {
                continue;
            };
            parameters.insert((place.to_string(), wire.to_string()), parameter.clone());
        }
        let input_name = format!("{stem}Input");
        let mut fields: Vec<Field> = Vec::new();
        for ((place, wire), parameter) in &parameters {
            match place.as_str() {
                "path" | "query" => {}
                "header" if wire.eq_ignore_ascii_case("idempotency-key") => continue,
                _ => {
                    self.find(
                        at.clone(),
                        format!("Left out its {place} parameter `{wire}`: a package sends placeholders, query parameters and a JSON body"),
                    );
                    continue;
                }
            }
            let name = snake(wire);
            let name = if fits_as_field(&name) {
                name
            } else {
                format!("{name}_value")
            };
            if name != *wire {
                self.find(
                    format!("{input_name}.{name}"),
                    format!("The provider names it `{wire}`; until a field can name its wire name, this one is sent by its name"),
                );
            }
            let schema = parameter.get("schema").cloned().unwrap_or(Value::Null);
            let Some(mut ty) = self.ty(
                &schema,
                &format!("{input_name}.{name}"),
                &format!("{input_name}{}", camel(&name)),
            ) else {
                continue;
            };
            if place == "query" && parameter.get("required") != Some(&Value::Bool(true)) {
                ty.optional = true;
            }
            fields.push(Field { name, ty });
        }
        // Its body.
        let body = operation
            .get("requestBody")
            .map(|body| self.resolve(body).clone());
        let mut body_type: Option<String> = None;
        if let Some(body) = &body {
            let content = body.get("content").and_then(Value::as_object);
            match content.and_then(|content| json_content(content)) {
                Some(schema) => {
                    let schema = schema.clone();
                    let reference = schema
                        .get("$ref")
                        .and_then(Value::as_str)
                        .and_then(|reference| reference.strip_prefix("#/components/schemas/"))
                        .map(str::to_string);
                    match reference {
                        Some(schema_name) if fields.is_empty() => {
                            body_type = Some(self.type_name(&schema_name));
                        }
                        _ => {
                            let target = self.resolve(&schema).clone();
                            if target.get("properties").is_some() || target.get("allOf").is_some() {
                                for field in self.fields(&input_name, &target) {
                                    if !fields.iter().any(|known| known.name == field.name) {
                                        fields.push(field);
                                    }
                                }
                            } else {
                                self.find(
                                    at.clone(),
                                    "Left out its body: it isn't an object, so it can't sit beside the operation's other input",
                                );
                            }
                        }
                    }
                }
                None => {
                    let kinds: Vec<&String> = content.map(|c| c.keys().collect()).unwrap_or_default();
                    self.find(
                        at.clone(),
                        format!(
                            "Left out its body, sent as {}: the package sends JSON",
                            kinds
                                .iter()
                                .map(|kind| format!("`{kind}`"))
                                .collect::<Vec<_>>()
                                .join(" or ")
                        ),
                    );
                }
            }
        }
        let input = match body_type {
            Some(name) => Some(name),
            None if fields.is_empty() => None,
            None => {
                let name = self.unique(input_name);
                self.types.insert(
                    name.clone(),
                    TypeDecl::Object {
                        doc: None,
                        fields,
                    },
                );
                Some(name)
            }
        };
        // Its lowest 2xx answer.
        let responses = operation.get("responses").and_then(Value::as_object);
        let success = responses.and_then(|responses| {
            responses
                .iter()
                .filter(|(status, _)| status.starts_with('2'))
                .min_by_key(|(status, _)| (*status).clone())
                .map(|(_, response)| self.resolve(response).clone())
        });
        let output = success.and_then(|response| {
            let content = response.get("content").and_then(Value::as_object)?;
            let schema = json_content(content)?.clone();
            let ty = self.ty(&schema, &at, &format!("{stem}Output"))?;
            if ty.rules.is_empty() {
                Some(ty.text)
            } else {
                let name = self.unique(format!("{stem}Output"));
                self.types.insert(
                    name.clone(),
                    TypeDecl::Refined {
                        doc: None,
                        base: ty.text,
                        rules: ty.rules,
                    },
                );
                Some(name)
            }
        });
        // What it changes, and whether a repeat is safe.
        let (changes, idempotent) = if self.exact {
            let changes = operation.get("x-jadpo-changes") == Some(&Value::Bool(true));
            let keeps = operation
                .get("x-jadpo-idempotency")
                .and_then(|idempotency| idempotency.get("keeps"))
                .and_then(Value::as_str);
            (changes, changes && keeps == Some("response"))
        } else {
            let changes = !matches!(read.method, "get");
            if changes {
                self.find(
                    at.clone(),
                    "Check against the provider's documentation that it changes something, and mark it `idempotent: true` if repeating it is safe",
                );
            }
            (changes, false)
        };
        // A Jadpo application's failures, by their codes and kinds.
        let mut failures: Vec<String> = Vec::new();
        if let Some(responses) = responses {
            for (status, response) in responses {
                if status.starts_with('2') {
                    continue;
                }
                let response = self.resolve(response).clone();
                let Some(schema) = response
                    .get("content")
                    .and_then(Value::as_object)
                    .and_then(|content| json_content(content))
                    .cloned()
                else {
                    continue;
                };
                let options: Vec<Value> = match schema.get("oneOf").and_then(Value::as_array) {
                    Some(options) => options.clone(),
                    None => vec![schema],
                };
                for option in options {
                    let Some(schema_name) = option
                        .get("$ref")
                        .and_then(Value::as_str)
                        .and_then(|reference| reference.strip_prefix("#/components/schemas/"))
                    else {
                        continue;
                    };
                    if let Some(name) = self.failure(schema_name) {
                        if !failures.contains(&name) {
                            failures.push(name);
                        }
                    }
                }
            }
        }
        failures.sort();
        Operation {
            doc,
            method: read.method.to_string(),
            path: read.path.clone(),
            input,
            output,
            changes,
            idempotent,
            failures,
        }
    }

    /// The failure schema `schema` declares, by its `x-jadpo-failure`.
    fn failure(&mut self, schema: &str) -> Option<String> {
        let value = self.schema(schema)?.clone();
        let marked = value.get("x-jadpo-failure")?;
        let code = marked.get("code").and_then(Value::as_str)?;
        let kind = marked
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("conflict");
        let name = camel(code);
        if !self.failures.contains_key(&name) {
            if self.taken.contains(&name) {
                self.find(
                    name.clone(),
                    format!("A type and the failure `{code}` share this name; rename one"),
                );
            }
            self.taken.insert(name.clone());
            let message = value
                .get("description")
                .and_then(Value::as_str)
                .filter(|message| *message != code)
                .map(str::to_string);
            self.failures.insert(
                name.clone(),
                FailureDecl {
                    doc: message.clone(),
                    kind: kind.to_string(),
                    message,
                },
            );
        }
        Some(name)
    }

    /// How the API is reached and called.
    fn provider(&mut self, title: &str, file: &str, digest: &str) -> Provider {
        let document = self.document;
        let mut provider = Provider {
            description: format!("How {title} is reached and called."),
            file: file.to_string(),
            digest: digest.to_string(),
            ..Provider::default()
        };
        let schemes = document
            .pointer("/components/securitySchemes")
            .and_then(Value::as_object);
        let bearer = schemes.is_some_and(|schemes| {
            schemes.values().any(|scheme| {
                scheme.get("type").and_then(Value::as_str) == Some("http")
                    && scheme
                        .get("scheme")
                        .and_then(Value::as_str)
                        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("bearer"))
            })
        });
        let basic = schemes.is_some_and(|schemes| {
            schemes.values().any(|scheme| {
                scheme.get("scheme").and_then(Value::as_str).is_some_and(|scheme| scheme.eq_ignore_ascii_case("basic"))
            })
        });
        provider.credential = if bearer {
            Some("bearer")
        } else if basic {
            Some("basic")
        } else {
            None
        };
        if provider.credential.is_none() && schemes.is_some_and(|schemes| !schemes.is_empty()) {
            let what = if self.exact {
                "It signs callers in only with a browser session, which another application can't hold; give the API service keys (area 8) and import it again"
            } else {
                "Its sign-in isn't a bearer token or basic authentication; write `credential:` from the provider's documentation"
            };
            self.find("provider", what);
        }
        if self.exact {
            // The other application's own: the connection gives `host:`.
            let idempotency = document
                .get("paths")
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|paths| paths.values())
                .filter_map(Value::as_object)
                .flat_map(|item| item.values())
                .find_map(|operation| operation.get("x-jadpo-idempotency"));
            if let Some(idempotency) = idempotency {
                let text = |key: &str| {
                    idempotency
                        .get(key)
                        .and_then(Value::as_str)
                        .map(str::to_string)
                };
                provider.idempotency = Some(Idempotency {
                    header: text("header").unwrap_or_else(|| "Idempotency-Key".to_string()),
                    window: text("window").unwrap_or_else(|| "1d".to_string()),
                    max_length: idempotency.get("max_length").and_then(Value::as_u64),
                    in_use: text("in_use"),
                    reused: text("reused"),
                });
            }
            provider.error_code = Some("error.code".to_string());
        } else {
            provider.live = document
                .get("servers")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|server| server.get("url").and_then(Value::as_str))
                .find(|url| url.starts_with("https://") || url.starts_with("http://"))
                .map(|url| url.trim_end_matches('/').to_string());
            match &provider.live {
                Some(_) => self.find(
                    "provider",
                    "Add `hosts: { sandbox: }` from the provider's documentation, or the live host again when the sandbox is a test credential",
                ),
                None => self.find(
                    "provider",
                    "The document names no host: add `hosts:` from the provider's documentation, or leave it out and give `host:` on the connection",
                ),
            }
            self.find(
                "provider",
                "Add the provider's failures and `error_code:`, where its error code is in a failed response, and `idempotency:` if it takes a key",
            );
        }
        provider
    }
}

/// A content map's JSON schema: `application/json`, or a `+json` type.
fn json_content(content: &serde_json::Map<String, Value>) -> Option<&Value> {
    content
        .iter()
        .find(|(kind, _)| {
            let kind = kind.split(';').next().unwrap_or("").trim();
            kind == "application/json" || kind.ends_with("+json")
        })
        .and_then(|(_, media)| media.get("schema"))
}

/// A schema's `type`, one or a list.
fn types_of(schema: &Value) -> Vec<&str> {
    match schema.get("type") {
        Some(Value::String(one)) => vec![one.as_str()],
        Some(Value::Array(all)) => all.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

/// A number's `minimum` and `maximum` as `min:` and `max:`.
fn bounds(schema: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (key, rule) in [("minimum", "min"), ("maximum", "max")] {
        if let Some(number) = schema.get(key).filter(|value| value.is_number()) {
            out.push((rule.to_string(), number.to_string()));
        }
    }
    out
}
