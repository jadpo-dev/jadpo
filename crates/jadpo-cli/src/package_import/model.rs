//! What an import makes (area 11, "Where packages come from"; chunk 7.2):
//! a provider package in the package format, as plain data, and the Jadpo
//! source each of its files holds.

use std::collections::BTreeMap;

/// A provider package read from a description.
#[derive(Clone, Debug, Default)]
pub struct Package {
    /// `Todos`, in `UpperCamelCase`.
    pub name: String,
    /// `v1`.
    pub version: String,
    /// The index's `///` description.
    pub description: String,
    pub provider: Provider,
    /// By name.
    pub resources: BTreeMap<String, Resource>,
    /// By name.
    pub types: BTreeMap<String, TypeDecl>,
    /// By name.
    pub failures: BTreeMap<String, FailureDecl>,
    /// What the description leaves to check, in the order found.
    pub findings: Vec<Finding>,
    /// Whether it was read exactly, from a Jadpo application's `x-jadpo`
    /// fields.
    pub exact: bool,
    /// Each field mapped to a standard type, in the order found (area 11,
    /// "Mapping imported types to standard types").
    pub mapped: Vec<Mapped>,
}

/// A field mapped to a standard type: by its format, or by its shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mapped {
    /// `Charge.amount`.
    pub at: String,
    /// `Money`.
    pub standard: String,
    /// `format` (the description's own hint, area 11's first layer) or
    /// `shape` (its second, chunk 7.2e).
    pub by: &'static str,
    /// The provider's members a standard type is read from, each its
    /// spelling; empty for one sent as the standard type is.
    pub from: Vec<String>,
}

/// What `provider.jadpo` says.
#[derive(Clone, Debug, Default)]
pub struct Provider {
    pub description: String,
    /// The `live:` host, from the description's first absolute server.
    pub live: Option<String>,
    /// `CredentialScheme.<name>`.
    pub credential: Option<&'static str>,
    pub idempotency: Option<Idempotency>,
    /// Where the provider's error code is in a failed response.
    pub error_code: Option<String>,
    /// The description, as the command was given it, and its digest.
    pub file: String,
    pub digest: String,
    /// The types an agent has reviewed, kept from the file an import
    /// merges into (chunk 7.2e2).
    pub reviewed: Vec<String>,
}

/// The provider's `idempotency:`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Idempotency {
    pub header: String,
    pub window: String,
    pub max_length: Option<u64>,
    pub in_use: Option<String>,
    pub reused: Option<String>,
}

/// One resource and its operations.
#[derive(Clone, Debug, Default)]
pub struct Resource {
    pub doc: Option<String>,
    /// By name.
    pub operations: BTreeMap<String, Operation>,
}

/// One operation of a resource.
#[derive(Clone, Debug, Default)]
pub struct Operation {
    pub doc: Option<String>,
    /// `post`, as `HttpMethod.post` writes it.
    pub method: String,
    pub path: String,
    pub input: Option<String>,
    pub output: Option<String>,
    pub changes: bool,
    pub idempotent: bool,
    pub failures: Vec<String>,
}

/// A type the package declares.
#[derive(Clone, Debug)]
pub enum TypeDecl {
    Object {
        doc: Option<String>,
        fields: Vec<Field>,
    },
    Enum {
        doc: Option<String>,
        variants: Vec<Variant>,
    },
    /// A built-in type with rules: what a constrained item of a list or a
    /// map needs, since a field's type can carry rules only on itself.
    Refined {
        doc: Option<String>,
        base: String,
        rules: Vec<(String, String)>,
    },
}

/// A field of an object type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub ty: Ty,
    /// The provider's spelling, where it differs from `name`: written as
    /// the field's `wire_name:` (area 11, decided 2026-10-20).
    pub wire: Option<String>,
    /// Its standard type's parts and the members each is read from, its
    /// `wire_parts:` (chunk 7.2e).
    pub parts: Vec<(String, String)>,
}

/// A variant of an enumeration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Variant {
    pub name: String,
    /// The provider's spelling, where it differs from `name`: written as
    /// the variant's `wire_name:`.
    pub wire: Option<String>,
}

/// A field's type as written: `List<Tag>`, whether it may be absent, and
/// its own rules, each `name` and its value as Jadpo writes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ty {
    pub text: String,
    pub optional: bool,
    pub rules: Vec<(String, String)>,
}

/// A failure the package declares.
#[derive(Clone, Debug)]
pub struct FailureDecl {
    pub doc: Option<String>,
    /// `FailureKind.<kind>`.
    pub kind: String,
    pub message: Option<String>,
}

/// Something the description leaves to check: where, and what to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// `todos.create`, `TodoView.created_at`, `provider`.
    pub at: String,
    pub next: String,
}

impl Package {
    /// The package's folder: `shared/providers/todos/v1`.
    pub fn folder(&self) -> String {
        format!(
            "shared/{}/{}/{}",
            jadpo_db::PROVIDERS,
            snake(&self.name),
            self.version
        )
    }

    /// Every file, by its path in the package, with its source.
    pub fn files(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        out.insert(jadpo_db::PACKAGE_INDEX.to_string(), self.index());
        out.insert(jadpo_db::PROVIDER_FILE.to_string(), self.provider_file());
        for (name, resource) in &self.resources {
            out.insert(
                format!("resources/{name}.jadpo"),
                resource_file(name, resource),
            );
        }
        for (name, ty) in &self.types {
            out.insert(format!("types/{}.jadpo", snake(name)), type_file(name, ty));
        }
        for (name, failure) in &self.failures {
            out.insert(
                format!("failures/{}.jadpo", snake(name)),
                failure_file(name, failure),
            );
        }
        out
    }

    fn index(&self) -> String {
        let mut out = doc(Some(&self.description), "");
        out.push_str(&format!(
            "package {}.{} {{\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}}\n",
            self.name, self.version
        ));
        out
    }

    fn provider_file(&self) -> String {
        let provider = &self.provider;
        let mut out = doc(Some(&provider.description), "");
        out.push_str("provider {\n    protocol: Protocol.json_http\n");
        if let Some(live) = &provider.live {
            out.push_str(&format!(
                "    hosts: {{\n        live: {}\n    }}\n",
                quote(live)
            ));
        }
        if let Some(credential) = provider.credential {
            out.push_str(&format!("    credential: CredentialScheme.{credential}\n"));
        }
        if let Some(idempotency) = &provider.idempotency {
            out.push_str("    idempotency: {\n");
            out.push_str(&format!("        header: {}\n", quote(&idempotency.header)));
            out.push_str(&format!("        window: {}\n", idempotency.window));
            if let Some(max) = idempotency.max_length {
                out.push_str(&format!("        max_length: {max}\n"));
            }
            if let Some(code) = &idempotency.in_use {
                out.push_str(&format!("        in_use: {}\n", quote(code)));
            }
            if let Some(code) = &idempotency.reused {
                out.push_str(&format!("        reused: {}\n", quote(code)));
            }
            out.push_str("    }\n");
        }
        if let Some(path) = &provider.error_code {
            out.push_str(&format!("    error_code: {}\n", quote(path)));
        }
        out.push_str(&format!(
            "    imported: {{\n        file: {}\n        digest: {}\n",
            quote(&provider.file),
            quote(&provider.digest)
        ));
        if !provider.reviewed.is_empty() {
            out.push_str(&format!(
                "        reviewed: [{}]\n",
                provider.reviewed.join(", ")
            ));
        }
        out.push_str("    }\n}\n");
        out
    }
}

fn resource_file(name: &str, resource: &Resource) -> String {
    let mut out = doc(resource.doc.as_deref(), "");
    out.push_str(&format!("public resource {name} {{\n"));
    let mut first = true;
    for (operation_name, operation) in &resource.operations {
        if !first {
            out.push('\n');
        }
        first = false;
        out.push_str(&doc(operation.doc.as_deref(), "    "));
        out.push_str(&format!("    {operation_name}: {{\n"));
        out.push_str(&format!(
            "        method: HttpMethod.{}\n",
            operation.method
        ));
        out.push_str(&format!("        path: {}\n", quote(&operation.path)));
        if let Some(input) = &operation.input {
            out.push_str(&format!("        input: {input}\n"));
        }
        if let Some(output) = &operation.output {
            out.push_str(&format!("        output: {output}\n"));
        }
        out.push_str(&format!("        changes: {}\n", operation.changes));
        if operation.idempotent {
            out.push_str("        idempotent: true\n");
        }
        if !operation.failures.is_empty() {
            out.push_str(&format!(
                "        failures: [{}]\n",
                operation.failures.join(", ")
            ));
        }
        out.push_str("        source: PackageSource.imported\n    }\n");
    }
    out.push_str("}\n");
    out
}

fn type_file(name: &str, ty: &TypeDecl) -> String {
    match ty {
        TypeDecl::Object {
            doc: written,
            fields,
        } => {
            let mut out = doc(written.as_deref(), "");
            out.push_str(&format!("public type {name} = Object {{\n"));
            for field in fields {
                out.push_str(&format!("    {}: {}", field.name, field.ty.text));
                if field.ty.optional {
                    out.push('?');
                }
                // Its spelling on the wire first, then its rules.
                let mut settings: Vec<(String, String)> = field
                    .wire
                    .iter()
                    .map(|wire| ("wire_name".to_string(), quote(wire)))
                    .collect();
                settings.extend(field.ty.rules.iter().cloned());
                if field.parts.is_empty() {
                    out.push_str(&rules_block(&settings, "    "));
                } else {
                    // Read from its parts among the object's members.
                    out.push_str(" {\n        wire_parts: {\n");
                    for (part, member) in &field.parts {
                        out.push_str(&format!("            {part}: {}\n", quote(member)));
                    }
                    out.push_str("        }\n    }");
                }
                out.push('\n');
            }
            out.push_str("}\n");
            out
        }
        TypeDecl::Enum {
            doc: written,
            variants,
        } => {
            let mut out = doc(written.as_deref(), "");
            out.push_str(&format!("public type {name} = Enum {{\n"));
            for variant in variants {
                match &variant.wire {
                    Some(wire) => out.push_str(&format!(
                        "    {}: {{\n        wire_name: {}\n    }}\n",
                        variant.name,
                        quote(wire)
                    )),
                    None => out.push_str(&format!("    {}\n", variant.name)),
                }
            }
            out.push_str("}\n");
            out
        }
        TypeDecl::Refined {
            doc: written,
            base,
            rules,
        } => {
            let mut out = doc(written.as_deref(), "");
            out.push_str(&format!("public type {name} = {base}"));
            if rules.is_empty() {
                out.push_str(" {}\n");
            } else {
                out.push_str(&rules_block(rules, ""));
                out.push('\n');
            }
            out
        }
    }
}

fn failure_file(name: &str, failure: &FailureDecl) -> String {
    let mut out = doc(failure.doc.as_deref(), "");
    out.push_str(&format!(
        "public type {name} = Failure {{\n    kind: FailureKind.{}\n",
        failure.kind
    ));
    if let Some(message) = &failure.message {
        out.push_str(&format!("    message: {}\n", quote(message)));
    }
    out.push_str("}\n");
    out
}

/// ` {\n<indent>    rule: value\n<indent>}`, or nothing.
fn rules_block(rules: &[(String, String)], indent: &str) -> String {
    if rules.is_empty() {
        return String::new();
    }
    let mut out = " {\n".to_string();
    for (name, value) in rules {
        out.push_str(&format!("{indent}    {name}: {value}\n"));
    }
    out.push_str(&format!("{indent}}}"));
    out
}

/// A `///` description, one line per line of `text`, at `indent`.
fn doc(text: Option<&str>, indent: &str) -> String {
    let Some(text) = text.map(str::trim).filter(|text| !text.is_empty()) else {
        return String::new();
    };
    text.lines()
        .map(|line| {
            let line = line.trim_end();
            if line.is_empty() {
                format!("{indent}///\n")
            } else {
                format!("{indent}/// {line}\n")
            }
        })
        .collect()
}

/// `text` as a Jadpo string literal.
pub fn quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `AcmeCouriers` as `acme_couriers`; already snake text is kept.
pub fn snake(text: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if c.is_ascii_alphanumeric() {
            if c.is_ascii_uppercase() {
                let previous = i.checked_sub(1).map(|j| chars[j]);
                let next = chars.get(i + 1);
                let boundary = previous.is_some_and(|p| {
                    p.is_ascii_lowercase()
                        || p.is_ascii_digit()
                        || (p.is_ascii_uppercase() && next.is_some_and(char::is_ascii_lowercase))
                });
                if boundary && !out.ends_with('_') {
                    out.push('_');
                }
                out.push(c.to_ascii_lowercase());
            } else {
                out.push(*c);
            }
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_string()
}

/// `acme couriers`, `acme_couriers` or `acmeCouriers` as `AcmeCouriers`.
pub fn camel(text: &str) -> String {
    snake(text)
        .split('_')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_change_case_as_jadpo_writes_them() {
        assert_eq!(snake("AcmeCouriers"), "acme_couriers");
        assert_eq!(snake("createdAt"), "created_at");
        assert_eq!(snake("HTTPServer"), "http_server");
        assert_eq!(snake("in-progress"), "in_progress");
        assert_eq!(snake("V1Todos"), "v1_todos");
        assert_eq!(camel("acme couriers"), "AcmeCouriers");
        assert_eq!(camel("payment_intent"), "PaymentIntent");
        assert_eq!(camel("todos"), "Todos");
    }

    #[test]
    fn strings_are_quoted_as_jadpo_reads_them() {
        assert_eq!(quote("a\"b\\c"), "\"a\\\"b\\\\c\"");
    }
}
