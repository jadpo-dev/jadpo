//! The `ai` capability's settings (area 21, "Connecting a provider" and
//! "How a provider fills it"; chunk 5.5v3).
//!
//! - [`model_connections`]: each connection with `driver:
//!   Driver.openai_compatible`, the built-in driver for a server that
//!   speaks OpenAI's API, and its `url:`, a `Url` setting or an address
//!   written out, plain HTTP only to this machine.
//! - [`settings`]: `application { ai: { connection: models: } }`, the
//!   connection every `ai` call goes through and each model it names, a
//!   `ModelId` setting, `default` among them. The keys later chunks build
//!   (`embedding_dimensions:`, `limits:`, `personal_data:`,
//!   `keep_content:`) are `toolchain.not_built_yet`, and so is a package's
//!   connection, which fills `ai` with chunk 5.5v3b.
//! - [`schema`]: the JSON Schema a model is asked to answer a type in,
//!   the one OpenAPI writes for it with each field's `///` description,
//!   whole: the runtime cuts it to what a provider enforces.

use std::collections::BTreeSet;

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::json::Value;
use jadpo_graph::{Detail, Graph, Key, TypeRef};
use jadpo_syntax::ast::{self, AstNode};

use crate::config::{config, singletons};
use crate::data::ConnectionKind;
use crate::lookups::Lookup;
use crate::tree::{self, name_text, node_span};
use crate::types::{Const, Prim, Ty, TypeView};

/// The keys a connection with `driver: Driver.openai_compatible` takes.
const MODEL_KEYS: [&str; 2] = ["driver", "url"];

/// Where a model connection's address comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ModelUrl {
    /// `config.<name>`, a `Url` setting.
    Setting(String),
    /// An address written out.
    Written(String),
}

/// A connection with `driver: Driver.openai_compatible`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ModelConnection {
    pub name: String,
    /// `None` when it has none that reads.
    pub url: Option<ModelUrl>,
}

/// Every model connection, and the problems with them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelConnections {
    pub list: Vec<ModelConnection>,
    pub problems: Vec<Diagnostic>,
}

/// `application { ai: }`, read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiSettings {
    /// Whether `ai:` is written at all.
    pub written: bool,
    /// The connection every `ai` call goes through, once it names a model
    /// connection.
    pub connection: Option<String>,
    /// Each model `models:` names, with the `ModelId` setting it reads, in
    /// the order written.
    pub models: Vec<(String, String)>,
    pub problems: Vec<Diagnostic>,
}

impl AiSettings {
    /// Whether every part of it read, so a call can be asked through it.
    pub fn complete(&self) -> bool {
        self.connection.is_some() && self.models.iter().any(|(name, _)| name == "default")
    }

    /// The names `models:` gives, in order.
    pub fn model_names(&self) -> Vec<&str> {
        self.models.iter().map(|(name, _)| name.as_str()).collect()
    }
}

/// Each connection with `driver: Driver.openai_compatible`.
#[salsa::tracked(returns(ref))]
pub fn model_connections(db: &dyn Db, project: Project) -> ModelConnections {
    let mut out = ModelConnections::default();
    let lookup = Lookup::new(db, project);
    for found in singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some("connections") {
                continue;
            }
            for (name, entry) in tree::members(member.block()) {
                let Some(connection) = name_text(&name) else {
                    continue;
                };
                if crate::data::connection_kind(entry.block()) != ConnectionKind::Model {
                    continue;
                }
                let mut model = ModelConnection {
                    name: connection.clone(),
                    url: None,
                };
                let mut given = false;
                for (setting, option) in tree::members(entry.block()) {
                    let word = name_text(&setting).unwrap_or_default();
                    if word == "driver" {
                        continue;
                    }
                    if word != "url" {
                        let near = nearest(&word, MODEL_KEYS.iter().copied());
                        let mut problem = Diagnostic::new(
                            &rule::CONNECTION_KEY_INVALID,
                            format!("A connection with `driver: Driver.openai_compatible` takes no `{word}:`"),
                        )
                        .at(node_span(&found.path, setting.syntax()), "not a setting of the driver")
                        .reason("A connection to a server that speaks OpenAI's API gives `url:`, the address its API is under, and nothing else: the model each call asks is a setting `ai: { models: }` names.");
                        problem = match near {
                            Some(near) if near != "driver" => problem
                                .next(format!("Use `{near}:` instead of `{word}:`"))
                                .edit(node_span(&found.path, setting.syntax()), near),
                            _ => problem.next(format!("Remove `{word}:`")),
                        };
                        out.problems.push(problem);
                        continue;
                    }
                    given = true;
                    let span = option.value().map_or_else(
                        || node_span(&found.path, option.syntax()),
                        |value| node_span(&found.path, value.syntax()),
                    );
                    model.url = url(
                        db,
                        project,
                        &lookup,
                        &connection,
                        &option,
                        span,
                        &mut out.problems,
                    );
                }
                if !given {
                    out.problems.push(
                        Diagnostic::new(
                            &rule::CONNECTION_KEY_MISSING,
                            format!("`{connection}` doesn't say which server it reaches"),
                        )
                        .at(node_span(&found.path, name.syntax()), "no `url:`")
                        .reason("A connection with `driver: Driver.openai_compatible` gives `url:`, the address the server's OpenAI API is under, such as `http://localhost:11434/v1` for Ollama on this machine.")
                        .next("Add `url: config.model_url`, declaring `model_url: Url` in `config.jadpo`"),
                    );
                }
                out.list.push(model);
            }
        }
    }
    out.list.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// A model connection's `url:`: a `Url` setting, or an address written
/// out that a call may be sent to, HTTPS or plain HTTP to this machine.
fn url(
    db: &dyn Db,
    project: Project,
    lookup: &Lookup<'_>,
    connection: &str,
    option: &ast::Member,
    span: Span,
    out: &mut Vec<Diagnostic>,
) -> Option<ModelUrl> {
    let invalid = |title: String, label: &str| {
        Diagnostic::new(&rule::CONNECTION_VALUE_INVALID, title).at(span.clone(), label.to_string())
    };
    let reason = "`url:` is the address the server's OpenAI API is under, `http://localhost:11434/v1`: a `Url` setting, `config.<name>`, read from each deployment's environment, or an address written out. Plain HTTP reaches only this machine (`localhost`, `127.x.x.x`, `[::1]`); anywhere else is HTTPS.";
    let value = option.value()?;
    if let Some(field) = reads(&value) {
        let setting = config(db, project).setting(&field)?;
        if lookup.base_prim(&setting.ty) == Some(Prim::Url) {
            return Some(ModelUrl::Setting(field));
        }
        out.push(
            invalid(
                format!("`{connection}`'s `url:` must be an address"),
                &format!("`{field}` is `{}`", setting.ty),
            )
            .reason(reason)
            .next(format!("Declare `{field}: Url` in `config.jadpo`")),
        );
        return None;
    }
    match Const::from_expr(&value) {
        Const::Str(text) if jadpo_std::provider::sendable(&text).is_some() => {
            Some(ModelUrl::Written(text))
        }
        Const::Str(text) if text.starts_with("http://") => {
            out.push(
                invalid(
                    format!("`{connection}`'s `url:` is plain HTTP to another machine"),
                    "not this machine",
                )
                .reason(reason)
                .next("Use the server's `https://` address, or run it on this machine"),
            );
            None
        }
        _ => {
            out.push(
                invalid(
                    format!("`{connection}`'s `url:` must be an address"),
                    "not an address",
                )
                .reason(reason)
                .next("Write `url: \"http://localhost:11434/v1\"`, or `url: config.model_url`"),
            );
            None
        }
    }
}

/// `config.<name>` as the name it reads.
fn reads(value: &ast::Expr) -> Option<String> {
    let (first, rest) = tree::dotted(value)?;
    if tree::name_ref_text(&first).as_deref() != Some("config") || rest.len() != 1 {
        return None;
    }
    tree::name_ref_text(&rest[0])
}

fn invalid(title: impl Into<String>, span: Span, label: &str) -> Diagnostic {
    Diagnostic::new(&rule::AI_SETTINGS_INVALID, title).at(span, label.to_string())
}

const REASON: &str = "`ai:` names `connection:`, the connection every `ai` call goes through, and `models:`, each model a call may ask by name, a `ModelId` setting, `config.<name>`, with `default` among them.";

/// Whether `ty` is a `ModelId`, or a type built on one.
fn is_model(lookup: &Lookup<'_>, ty: &Ty) -> bool {
    matches!(
        lookup.structure(ty),
        Ty::Opaque(name, args)
            if args.is_empty() && name == jadpo_std::standard::records::MODEL_ID
    )
}

/// `application { ai: }`.
#[salsa::tracked(returns(ref))]
pub fn settings(db: &dyn Db, project: Project) -> AiSettings {
    let mut out = AiSettings::default();
    let connections = crate::data::connections(db, project);
    let lookup = Lookup::new(db, project);
    for found in singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some("ai") {
                continue;
            }
            out.written = true;
            let head = node_span(&found.path, key.syntax());
            let Some(block) = member.block() else {
                out.problems.push(
                    invalid("`ai:` holds no block", node_span(&found.path, member.syntax()), "not a block")
                        .reason(REASON)
                        .next("Write `ai: { connection: local_model  models: { default: config.drafting_model } }`"),
                );
                continue;
            };
            let mut given: BTreeSet<String> = BTreeSet::new();
            for (setting, entry) in tree::members(Some(block)) {
                let word = name_text(&setting).unwrap_or_default();
                let key_span = node_span(&found.path, setting.syntax());
                if let Some((_, chunk)) = jadpo_std::ai::LATER_KEYS
                    .iter()
                    .find(|(later, _)| *later == word)
                {
                    given.insert(word.clone());
                    out.problems.push(
                        Diagnostic::new(
                            &rule::TOOLCHAIN_NOT_BUILT_YET,
                            format!("`ai: {{ {word}: }}` isn't built yet"),
                        )
                        .at(key_span, "decided, not built")
                        .reason(format!(
                            "Area 21 decides `{word}:`, and roadmap chunk {chunk} builds it; this compiler can't keep to it yet, so an application that gives it couldn't run as written."
                        ))
                        .next(format!("Leave `{word}:` out for now")),
                    );
                    continue;
                }
                if !jadpo_std::ai::KEYS.contains(&word.as_str()) {
                    let near = nearest(
                        &word,
                        jadpo_std::ai::KEYS
                            .iter()
                            .copied()
                            .chain(jadpo_std::ai::LATER_KEYS.iter().map(|(key, _)| *key)),
                    );
                    let mut problem = invalid(
                        format!("`{word}` isn't one of `ai:`'s settings"),
                        key_span.clone(),
                        "not a setting",
                    )
                    .reason(REASON);
                    problem = match near {
                        Some(near) => problem
                            .next(format!("Use `{near}:` instead of `{word}:`"))
                            .edit(key_span, near),
                        None => problem.next(format!("Remove `{word}:`")),
                    };
                    out.problems.push(problem);
                    continue;
                }
                if !given.insert(word.clone()) {
                    out.problems.push(
                        invalid(
                            format!("`ai:` gives `{word}:` twice"),
                            key_span,
                            "given again",
                        )
                        .reason(REASON)
                        .next(format!("Remove the second `{word}:`")),
                    );
                    continue;
                }
                let span = entry.value().map_or_else(
                    || node_span(&found.path, entry.syntax()),
                    |value| node_span(&found.path, value.syntax()),
                );
                if word == "connection" {
                    let written = entry
                        .value()
                        .map(|value| tree::written(value.syntax()))
                        .unwrap_or_default();
                    match connections.kind(&written) {
                        Some(ConnectionKind::Model) => out.connection = Some(written),
                        Some(ConnectionKind::Package) => out.problems.push(
                            Diagnostic::new(
                                &rule::TOOLCHAIN_NOT_BUILT_YET,
                                "A package filling `ai` isn't built yet",
                            )
                            .at(span, "a package's connection")
                            .reason(format!(
                                "`{written}` is a connection to a package, and a package fills `ai` through its `provides/ai.jadpo`, which roadmap chunk 5.5v3b builds. This compiler's `ai` goes through a server that speaks OpenAI's API, `driver: Driver.openai_compatible`."
                            ))
                            .next("Name a connection with `driver: Driver.openai_compatible`, such as a model on this machine, for now"),
                        ),
                        found => {
                            let models: Vec<String> = connections
                                .declared
                                .iter()
                                .filter(|(_, kind)| *kind == ConnectionKind::Model)
                                .map(|(name, _)| format!("`{name}`"))
                                .collect();
                            let what = match found {
                                Some(_) => format!("`{written}` is a connection that answers no model"),
                                None => format!("`{written}` is no connection"),
                            };
                            let reason = if models.is_empty() {
                                format!(
                                    "`ai: {{ connection: }}` names the connection every `ai` call goes through, one with `driver: Driver.openai_compatible`; {what}, and `app.jadpo` declares none."
                                )
                            } else {
                                format!(
                                    "`ai: {{ connection: }}` names the connection every `ai` call goes through, one with `driver: Driver.openai_compatible`; {what}. The application's are {}.",
                                    crate::and_list(&models)
                                )
                            };
                            out.problems.push(
                                invalid(format!("`connection: {written}` answers no model"), span, "not a model connection")
                                    .reason(reason)
                                    .next("Declare a connection with `driver: Driver.openai_compatible` and `url:` in `connections:`, and name it here"),
                            );
                        }
                    }
                    continue;
                }
                // `models:`: each a `ModelId` setting.
                let Some(models) = entry.block() else {
                    out.problems.push(
                        invalid("`models:` holds no block", span, "not a block")
                            .reason(REASON)
                            .next("Write `models: { default: config.drafting_model }`"),
                    );
                    continue;
                };
                let mut read: Vec<(String, String)> = Vec::new();
                let mut named_default = false;
                for (model, value) in tree::members(Some(models)) {
                    let name = name_text(&model).unwrap_or_default();
                    named_default |= name == jadpo_std::ai::DEFAULT_MODEL;
                    let model_span = node_span(&found.path, model.syntax());
                    if read.iter().any(|(known, _)| *known == name) {
                        out.problems.push(
                            invalid(
                                format!("`models:` names `{name}` twice"),
                                model_span,
                                "named again",
                            )
                            .reason(REASON)
                            .next(format!("Remove the second `{name}:`")),
                        );
                        continue;
                    }
                    let value_span = value.value().map_or_else(
                        || model_span.clone(),
                        |value| node_span(&found.path, value.syntax()),
                    );
                    let field = value.value().as_ref().and_then(reads);
                    let Some(field) = field else {
                        out.problems.push(
                            invalid(format!("Model `{name}` must be a setting"), value_span, "not a setting")
                                .reason("Each model is a `ModelId` setting, `config.<name>`, so moving to a newer model is a configuration change reviewed like any other; a model written out in `ai:` couldn't change without a new build.")
                                .next(format!("Declare `{name}_model: ModelId {{ default: \"<provider>/<name>\" }}` in `config.jadpo`, and write `{name}: config.{name}_model`")),
                        );
                        continue;
                    };
                    // An unknown setting is reported where it is read.
                    let Some(setting) = config(db, project).setting(&field) else {
                        continue;
                    };
                    if !is_model(&lookup, &setting.ty) {
                        out.problems.push(
                            invalid(format!("Model `{name}` must be a `ModelId` setting"), value_span, &format!("`{field}` is `{}`", setting.ty))
                                .reason("Each model is a `ModelId` setting, its provider and name as Hugging Face writes a repository, `openai/gpt-4.1-mini`, so a model is never anything else.")
                                .next(format!("Declare `{field}: ModelId` in `config.jadpo`")),
                        );
                        continue;
                    }
                    read.push((name, field));
                }
                if !named_default {
                    out.problems.push(
                        invalid("`models:` has no `default:`", span, "no `default:`")
                            .reason("`default` is the model a call asks when it names none with `model:`, so every application that asks a model names one.")
                            .next("Add `default: config.<name>`, a `ModelId` setting"),
                    );
                }
                out.models = read;
            }
            for needed in jadpo_std::ai::KEYS {
                if !given.contains(needed) {
                    out.problems.push(
                        invalid(
                            format!("`ai:` has no `{needed}:`"),
                            head.clone(),
                            "incomplete",
                        )
                        .reason(REASON)
                        .next(format!("Add `{needed}:` to `ai:`")),
                    );
                }
            }
        }
    }
    out
}

/// Everything wrong with `ai:` and the model connections.
#[salsa::tracked(returns(ref))]
pub fn ai_problems(db: &dyn Db, project: Project) -> Vec<Diagnostic> {
    let mut out = model_connections(db, project).problems.clone();
    out.extend(settings(db, project).problems.iter().cloned());
    out
}

/// The type each prompt's answer is decoded into, by the prompt's name:
/// every `ai.generate` call's `as:` in the application's actions, each
/// type once, in the order first met. A test's `fakes: { ai.<prompt>: }`
/// scripts a value of it.
#[salsa::tracked(returns(ref))]
pub fn answer_types(db: &dyn Db, project: Project) -> std::collections::BTreeMap<String, Vec<Ty>> {
    let mut out: std::collections::BTreeMap<String, Vec<Ty>> = std::collections::BTreeMap::new();
    for &file in project.files(db) {
        let location = jadpo_db::location(db, project, file);
        if location.ownership.package().is_some() || location.test {
            continue;
        }
        for item in &jadpo_db::item_tree(db, file).items {
            if item.kind != jadpo_db::ItemKind::Action {
                continue;
            }
            let callable = crate::types::Callable::new(db, file, item.index, None);
            for (prompt, ty) in &crate::types::typed_body(db, project, callable).generates {
                let types = out.entry(prompt.clone()).or_default();
                if !types.contains(ty) {
                    types.push(ty.clone());
                }
            }
        }
    }
    out
}

/// The domains whose actions ask a model, in name order: what the
/// network report lists beside the model's connection.
#[salsa::tracked(returns(ref))]
pub fn asking_domains(db: &dyn Db, project: Project) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for &file in project.files(db) {
        let location = jadpo_db::location(db, project, file);
        let Some(domain) = location.ownership.domain() else {
            continue;
        };
        if location.test || out.iter().any(|known| known == domain) {
            continue;
        }
        let asks = jadpo_db::item_tree(db, file).items.iter().any(|item| {
            item.kind == jadpo_db::ItemKind::Action
                && !crate::types::typed_body(
                    db,
                    project,
                    crate::types::Callable::new(db, file, item.index, None),
                )
                .generates
                .is_empty()
        });
        if asks {
            out.push(domain.to_string());
        }
    }
    out.sort();
    out
}

// --- the schema ---------------------------------------------------------------

/// The JSON Schema a model is asked to answer `ty` in (area 21, "Typed
/// output", step 1): the schema OpenAPI writes for it (area 7), whole,
/// each object's and enum's as a definition under `$defs` that the rest
/// refers to, with each field's `///` description. Rules JSON Schema says
/// are kept as it says them; the runtime cuts them to what a provider's
/// structured mode enforces. With it, whether its JSON is an object. `None`
/// for a type built on `Text`, which asks for plain text.
pub fn schema(
    graph: &Graph,
    locales: Option<&jadpo_std::LocalePlan>,
    ty: &TypeRef,
) -> Option<(String, bool)> {
    let wire = crate::routes::wire::Wire { graph, locales };
    let shape = wire.shape(ty);
    if let crate::routes::wire::Shape::Scalar(builtin, _) = &shape
        && builtin.representation() == jadpo_std::Builtin::Text
    {
        return None;
    }
    // An object type of the app's is an object; a standard type, an enum,
    // a list or a scalar isn't, whatever its JSON.
    let object = matches!(&shape, crate::routes::wire::Shape::Object(_));
    let mut refs: BTreeSet<Key> = BTreeSet::new();
    let mut root = wire.schema(ty, &mut refs);
    let mut defs = Value::object();
    let mut done: BTreeSet<Key> = BTreeSet::new();
    while let Some(key) = refs.iter().find(|key| !done.contains(*key)).cloned() {
        done.insert(key.clone());
        let mut schema = crate::routes::openapi::component_schema(&wire, &key, &mut refs);
        described(graph, &key, &mut schema);
        defs.set(&wire.name(&key), schema);
    }
    if !done.is_empty() {
        root.set("$defs", defs);
    }
    without_openapi_words(&mut root);
    Some((root.to_document(), object))
}

/// Each field's `///` description on its property, where the field's own
/// schema says nothing of its own.
fn described(graph: &Graph, key: &Key, schema: &mut Value) {
    let Some(id) = graph.find(key) else {
        return;
    };
    let Value::Object(members) = schema else {
        return;
    };
    let Some(Value::Object(properties)) = members.get_mut("properties") else {
        return;
    };
    for child in graph.children(id) {
        let node = graph.node(child);
        let (Detail::Field { .. }, Some(doc)) = (&node.detail, node.doc.as_deref()) else {
            continue;
        };
        let name = node.name.rsplit('.').next().unwrap_or(&node.name);
        if let Some(Value::Object(property)) = properties.get_mut(name) {
            property
                .entry("description".to_string())
                .or_insert_with(|| Value::from(doc));
        }
    }
}

/// OpenAPI's own words taken out, and references pointed at `$defs`.
fn without_openapi_words(json: &mut Value) {
    match json {
        Value::Object(members) => {
            members.retain(|name, _| !name.starts_with("x-"));
            if let Some(Value::Str(target)) = members.get_mut("$ref")
                && let Some(name) = target.strip_prefix("#/components/schemas/")
            {
                *target = format!("#/$defs/{name}");
            }
            for member in members.values_mut() {
                without_openapi_words(member);
            }
        }
        Value::Array(items) => {
            for item in items {
                without_openapi_words(item);
            }
        }
        _ => {}
    }
}
