//! A package filling `ai` (area 21, "How a provider fills it"; chunk
//! 5.5v3b): its `provides/ai.jadpo`, `provide ai { generate: models: }`,
//! mapping the capability's `generate` onto one of its own operations as
//! data, as `provide receiving` maps a queue's ([`crate::receiving`]).
//!
//! - **`generate:`** calls an operation (`call:`) that changes nothing,
//!   its `input:` made of what Jadpo hands it: `request.model` (the
//!   model's name after its provider), `request.max_output`,
//!   `request.instructions`, `request.messages` mapped `{ each:
//!   request.messages  as: { role: item.role  content: item.content } }`,
//!   a setting the package declares (`settings.<name>`) or a value written
//!   there. It names where the answer's text is (`answer:
//!   response.<field>...`, `first` for a list's first item), where a
//!   refusal is (`refused_when:`), what says the answer was cut off
//!   (`cut_off_when: { path: equals: }`, on the answer as sent), where the
//!   tokens used are (`usage: { input: output: cached: }`), and how far
//!   its structured-output mode goes (`schema: { mode: at: }`).
//! - **`models:`** names the tables in the package's `libraries/`:
//!   `context:`, each model's context window, which `too_long` is checked
//!   against before sending, and `prices:`, read by chunk 5.5v5's cost
//!   estimate.
//! - **`stream:`** and **`embed:`** come with chunks 5.5v6 and 5.5v4.

use std::collections::BTreeMap;

use jadpo_db::{Db, Folder, ItemKind, Ownership, PackageHome, Place, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::ai::SchemaMode;
use jadpo_syntax::ast::{self, AstNode, Decl};

use crate::tree::{self, dotted, name_text, node_span};
use crate::types::{Const, Prim, Ty, TypeView, type_table};

/// What `provide ai` holds.
const KEYS: [&str; 4] = ["generate", "stream", "embed", "models"];

/// What `generate:` holds.
const GENERATE_KEYS: [&str; 7] = [
    "call",
    "input",
    "answer",
    "refused_when",
    "cut_off_when",
    "usage",
    "schema",
];

/// The operations later chunks map, with the chunk that does.
const LATER: [(&str, &str); 2] = [("stream", "5.5v6"), ("embed", "5.5v4")];

/// A value a mapping gives a member of the operation's input.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MappedInput {
    Model,
    MaxOutput,
    Instructions,
    /// One object for each message, each member by its wire spelling.
    Messages(Vec<(String, MappedInput)>),
    Role,
    Content,
    /// `settings.<name>`: the package's setting, which each connection
    /// gives.
    Setting(String),
    Text(String),
    Int(i64),
    Bool(bool),
}

/// Where the tokens used are, by field names.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct UsageModel {
    pub input: Option<Vec<String>>,
    pub output: Option<Vec<String>>,
    pub cached: Option<Vec<String>>,
}

/// A package's `provide ai`, read.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AiMapping {
    pub operation: String,
    pub input: Vec<(String, MappedInput)>,
    pub instructions_apart: bool,
    pub answer: Vec<String>,
    pub refused_when: Option<Vec<String>>,
    pub cut_off_when: Option<(String, String)>,
    pub usage: UsageModel,
    pub schema: SchemaMode,
    pub schema_at: Option<String>,
    /// Each model's context window, by its name after its provider.
    pub contexts: Vec<(String, u64)>,
    /// Each model's prices, from `models: { prices: }` (chunk 5.5v5).
    pub prices: Vec<jadpo_std::ai::AiPrice>,
}

/// Every provider package's `provide ai`, by its folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AiMappings {
    pub of: BTreeMap<String, AiMapping>,
    /// The folders whose mapping doesn't read.
    pub broken: Vec<String>,
    pub problems: Vec<Diagnostic>,
}

const REASON: &str = "`provide ai` maps the capability onto the package's own operations, as data: `generate:` calls an operation that changes nothing with an `input:` made of `request.model`, `request.max_output`, `request.instructions`, `request.messages` (as `{ each: request.messages  as: { role: item.role  content: item.content } }`), `settings.<name>` or a value written there; it names the `answer:`'s text in the operation's output (`response.<field>`), `refused_when:`, `cut_off_when: { path: equals: }`, `usage: { input: output: cached: }` and `schema: { mode: at: }`. `models: { context: }` names the package's table of each model's context window.";

/// Every provider package's `provides/ai.jadpo`, read and checked against
/// its operations, its settings and its libraries.
#[salsa::tracked(returns(ref))]
pub fn mappings(db: &dyn Db, project: Project) -> AiMappings {
    let mut out = AiMappings::default();
    for &file in project.files(db) {
        let location = jadpo_db::location(db, project, file);
        let (Ownership::Package(home), Place::Folder(Folder::Provides)) =
            (&location.ownership, &location.place)
        else {
            continue;
        };
        if location.stem != "ai" || location.test {
            continue;
        }
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        let Some(decl) = source.decls().find_map(|decl| match decl {
            Decl::ProvideDecl(decl) => Some(decl),
            _ => None,
        }) else {
            continue;
        };
        let path = file.path(db).clone();
        let before = out.problems.len();
        let mut reader = Reader {
            db,
            project,
            path: &path,
            home,
            settings: crate::connections::reach(db, project, home)
                .map(|reach| {
                    reach
                        .settings
                        .iter()
                        .map(|(name, _)| name.clone())
                        .collect()
                })
                .unwrap_or_default(),
            out: &mut out.problems,
        };
        match reader.mapping(&decl) {
            Some(mapping) if out.problems.len() == before => {
                out.of.insert(home.folder.clone(), mapping);
            }
            _ => out.broken.push(home.folder.clone()),
        }
    }
    out
}

struct Reader<'a> {
    db: &'a dyn Db,
    project: Project,
    path: &'a str,
    home: &'a PackageHome,
    /// The settings its package declares.
    settings: Vec<String>,
    out: &'a mut Vec<Diagnostic>,
}

/// How a value walked to in the operation's output must end.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ends {
    Text,
    Count,
    Anything,
}

impl Reader<'_> {
    fn invalid(&mut self, span: Span, message: String, label: &str, next: String) {
        self.out.push(
            Diagnostic::new(&rule::PACKAGE_PROVIDER_INVALID, message)
                .at(span, label.to_string())
                .reason(REASON)
                .next(next),
        );
    }

    fn span(&self, node: &impl AstNode) -> Span {
        node_span(self.path, node.syntax())
    }

    fn written(node: &impl AstNode) -> String {
        tree::written(node.syntax())
    }

    fn mapping(&mut self, decl: &ast::ProvideDecl) -> Option<AiMapping> {
        let head = decl
            .name()
            .map_or_else(|| self.span(decl), |name| self.span(&name));
        let mut members: BTreeMap<String, ast::Member> = BTreeMap::new();
        let mut ok = true;
        for (key, member) in tree::members(decl.member_list()) {
            let written = name_text(&key).unwrap_or_default();
            if let Some((_, chunk)) = LATER.iter().find(|(later, _)| *later == written) {
                ok = false;
                self.out.push(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_NOT_BUILT_YET,
                        format!("`provide ai {{ {written}: }}` isn't built yet"),
                    )
                    .at(self.span(&key), "decided, not built")
                    .reason(format!(
                        "Area 21 decides how a package maps `ai.{written}`, and roadmap chunk {chunk} builds it; this compiler can't run it yet."
                    ))
                    .next(format!("Leave `{written}:` out for now")),
                );
                continue;
            }
            if !KEYS.contains(&written.as_str()) {
                ok = false;
                let next = match nearest(&written, KEYS.iter().copied()) {
                    Some(near) => format!("Use `{near}:` instead of `{written}:`"),
                    None => format!("Remove `{written}:`"),
                };
                self.invalid(
                    self.span(&key),
                    format!("`provide ai` has no `{written}:`"),
                    "not part of `ai`",
                    next,
                );
                continue;
            }
            members.insert(written, member);
        }
        let (Some(generate), Some(models)) = (members.get("generate"), members.get("models"))
        else {
            let missing = if members.contains_key("generate") {
                "models"
            } else {
                "generate"
            };
            self.invalid(
                head,
                format!("`provide ai` doesn't map `{missing}:`"),
                "incomplete",
                match missing {
                    "models" => "Add `models: { context: <library>.<table> }`, the package's table of each model's context window".to_string(),
                    _ => "Add `generate: { call: <resource>.<operation>  input: { }  answer: response.<field> }`".to_string(),
                },
            );
            return None;
        };
        let contexts = self.models(models);
        let generate = self.generate(generate);
        let (Some(contexts), Some(mut mapping)) = (contexts, generate) else {
            return None;
        };
        if !ok {
            return None;
        }
        (mapping.contexts, mapping.prices) = contexts;
        Some(mapping)
    }

    fn generate(&mut self, member: &ast::Member) -> Option<AiMapping> {
        let Some(block) = member.block() else {
            self.invalid(
                self.span(member),
                "`generate:` isn't a block".to_string(),
                "not a block",
                "Write `generate: { call: <resource>.<operation>  input: { }  answer: response.<field> }`".to_string(),
            );
            return None;
        };
        let mut members: BTreeMap<String, ast::Member> = BTreeMap::new();
        for (key, inner) in tree::members(Some(block)) {
            let written = name_text(&key).unwrap_or_default();
            if !GENERATE_KEYS.contains(&written.as_str()) {
                let next = match nearest(&written, GENERATE_KEYS.iter().copied()) {
                    Some(near) => format!("Use `{near}:` instead of `{written}:`"),
                    None => format!("Remove `{written}:`"),
                };
                self.invalid(
                    self.span(&key),
                    format!("`generate:` has no `{written}:`"),
                    "not part of `generate`",
                    next,
                );
                return None;
            }
            members.insert(written, inner);
        }
        for needed in ["call", "input", "answer", "usage", "schema"] {
            if !members.contains_key(needed) {
                self.invalid(
                    self.span(member),
                    format!("`generate:` doesn't give `{needed}:`"),
                    &format!("no `{needed}:`"),
                    match needed {
                        "call" => "Add `call: <resource>.<operation>`, the operation that asks the model".to_string(),
                        "input" => "Add `input: { model: request.model  ... }`".to_string(),
                        "answer" => "Add `answer: response.<field>`, where the answer's text is".to_string(),
                        "usage" => "Add `usage: { input: response.<field>  output: response.<field> }`".to_string(),
                        _ => "Add `schema: { mode: AiSchema.none }`, or the mode the provider's structured output has".to_string(),
                    },
                );
                return None;
            }
        }
        let operation = self.operation(&members["call"])?;
        let table = type_table(self.db, self.project);
        let (input, instructions_apart) = self.input(&members["input"], &operation, table)?;
        let output = operation.output.clone();
        let answer = self.walk(&members["answer"], &output, Ends::Text, table)?;
        let refused_when = match members.get("refused_when") {
            Some(refused) => Some(self.walk(refused, &output, Ends::Anything, table)?),
            None => None,
        };
        let cut_off_when = match members.get("cut_off_when") {
            Some(cut) => Some(self.cut_off(cut, &output, table)?),
            None => None,
        };
        let usage = self.usage(&members["usage"], &output, table)?;
        let (schema, schema_at) = self.schema(&members["schema"])?;
        Some(AiMapping {
            operation: operation.full_name(),
            input,
            instructions_apart,
            answer,
            refused_when,
            cut_off_when,
            usage,
            schema,
            schema_at,
            contexts: Vec::new(),
            prices: Vec::new(),
        })
    }

    /// `call:`: one of the package's operations, which changes nothing
    /// and whose answer its types read.
    fn operation(&mut self, call: &ast::Member) -> Option<crate::providers::Operation> {
        let written = call.value().map(|v| Self::written(&v)).unwrap_or_default();
        let span = call
            .value()
            .map_or_else(|| self.span(call), |value| self.span(&value));
        let operations = crate::providers::operations(self.db, self.project);
        let found = written
            .split_once('.')
            .and_then(|(resource, name)| operations.find(&self.home.folder, resource, name))
            .cloned();
        let Some(operation) = found else {
            let known: Vec<String> = operations
                .of(&self.home.folder)
                .map(crate::providers::Operation::full_name)
                .collect();
            let next = match nearest(&written, known.iter().map(String::as_str)) {
                Some(near) => format!("Call `{near}`"),
                None => {
                    "Call one of the package's operations, `<resource>.<operation>`".to_string()
                }
            };
            self.invalid(
                span,
                format!("`{written}` isn't one of the package's operations"),
                "no such operation",
                next,
            );
            return None;
        };
        if operation.changes {
            self.invalid(
                span,
                format!("`{written}` changes something at the provider"),
                "a changing operation",
                format!("Map an operation that only answers, or mark `{written}` `changes: false` if asking it changes nothing"),
            );
            return None;
        }
        if operation.decode.is_some() {
            self.invalid(
                span,
                format!("`{written}`'s answer is read by the package's own `decode:`"),
                "read by a function",
                "Map an operation whose output is read by its types".to_string(),
            );
            return None;
        }
        Some(operation)
    }

    /// `input:`: each member of the operation's input, and whether
    /// `request.instructions` is one of them.
    fn input(
        &mut self,
        member: &ast::Member,
        operation: &crate::providers::Operation,
        table: &dyn TypeView,
    ) -> Option<(Vec<(String, MappedInput)>, bool)> {
        let host = match &operation.input {
            Some((Ty::Named(name), _)) => Some(name.clone()),
            _ => None,
        };
        let Some(host) = host else {
            self.invalid(
                self.span(member),
                format!("`{}` takes no input", operation.full_name()),
                "nothing to map",
                "Map an operation whose input the model's request is sent as".to_string(),
            );
            return None;
        };
        let mut apart = false;
        let input = self.object(member, &host, false, &mut apart, table)?;
        Some((input, apart))
    }

    /// The members of `member`'s block, each a field of `host`, and every
    /// field `host` requires among them; `item` inside `as:`.
    fn object(
        &mut self,
        member: &ast::Member,
        host: &str,
        item: bool,
        apart: &mut bool,
        table: &dyn TypeView,
    ) -> Option<Vec<(String, MappedInput)>> {
        let Some(block) = member.block() else {
            self.invalid(
                self.span(member),
                "An input's mapping isn't a block".to_string(),
                "not a block",
                "Write `input: { <field>: <value> }`".to_string(),
            );
            return None;
        };
        let fields = table.fields(host).map(<[_]>::to_vec).unwrap_or_default();
        let given = tree::members(Some(block));
        let mut out = Vec::new();
        for (key, value) in &given {
            let name = name_text(key).unwrap_or_default();
            let Some(field) = fields.iter().find(|field| field.name == name) else {
                let next = match nearest(&name, fields.iter().map(|field| field.name.as_str())) {
                    Some(near) => format!("Use `{near}:` instead of `{name}:`"),
                    None => format!("Map only the fields of `{host}`"),
                };
                self.invalid(
                    self.span(key),
                    format!("`{}` has no field `{name}`", own_name(host)),
                    "not an input field",
                    next,
                );
                return None;
            };
            let mapped = self.value(value, &field.declared, item, apart, table)?;
            out.push((field.wire_name.clone().unwrap_or(name), mapped));
        }
        for field in &fields {
            let mapped = given
                .iter()
                .any(|(key, _)| name_text(key).as_deref() == Some(field.name.as_str()));
            if !mapped && !matches!(field.declared, Ty::Nullable(_)) && !field.optional {
                self.invalid(
                    self.span(member),
                    format!("`{}`'s field `{}` isn't mapped", own_name(host), field.name),
                    "a required field left out",
                    format!("Add `{}:`", field.name),
                );
                return None;
            }
        }
        Some(out)
    }

    /// One member's value, for a field declared `ty`.
    fn value(
        &mut self,
        member: &ast::Member,
        ty: &Ty,
        item: bool,
        apart: &mut bool,
        table: &dyn TypeView,
    ) -> Option<MappedInput> {
        if member.block().is_some() {
            return self.each(member, ty, apart, table);
        }
        let value = member.value()?;
        let written = Self::written(&value);
        let span = self.span(&value);
        let text_like = |ty: &Ty| {
            table.base_prim(ty) == Some(Prim::Text)
                || matches!(ty.strip_nullable(), Ty::Named(name) if table.variants(name).is_some())
        };
        if let Some((first, rest)) = dotted(&value) {
            let first = tree::name_ref_text(&first).unwrap_or_default();
            let rest: Vec<String> = rest.iter().filter_map(tree::name_ref_text).collect();
            let (mapped, fits, what) = match (first.as_str(), rest.as_slice()) {
                ("request", [name]) if name == "model" && !item => (
                    MappedInput::Model,
                    table.base_prim(ty) == Some(Prim::Text),
                    "text",
                ),
                ("request", [name]) if name == "max_output" && !item => (
                    MappedInput::MaxOutput,
                    table.base_prim(ty) == Some(Prim::Int),
                    "a whole number",
                ),
                ("request", [name]) if name == "instructions" && !item => {
                    *apart = true;
                    (
                        MappedInput::Instructions,
                        table.base_prim(ty) == Some(Prim::Text),
                        "text",
                    )
                }
                ("item", [name]) if name == "role" && item => {
                    (MappedInput::Role, text_like(ty), "text or an enum")
                }
                ("item", [name]) if name == "content" && item => (
                    MappedInput::Content,
                    table.base_prim(ty) == Some(Prim::Text),
                    "text",
                ),
                ("settings", [name]) if self.settings.contains(name) => {
                    return Some(MappedInput::Setting(name.clone()));
                }
                ("request", [name]) if name == "messages" && !item => {
                    self.invalid(
                        span,
                        "`request.messages` is mapped one message at a time".to_string(),
                        "a list of `ChatMessage`s",
                        "Write `{ each: request.messages  as: { role: item.role  content: item.content } }`".to_string(),
                    );
                    return None;
                }
                _ => {
                    let next = if item {
                        "Give `item.role`, `item.content`, `settings.<name>` or a value"
                    } else {
                        "Give `request.model`, `request.max_output`, `request.instructions`, `settings.<name>` or a value"
                    };
                    self.invalid(
                        span,
                        format!("`{written}` isn't a value a mapping can give"),
                        "not mapped data",
                        next.to_string(),
                    );
                    return None;
                }
            };
            if !fits {
                self.invalid(
                    span,
                    format!("`{written}` is {what}, and the field isn't"),
                    "doesn't fit the field",
                    format!("Map `{written}` to a field of {what}"),
                );
                return None;
            }
            return Some(mapped);
        }
        match Const::from_expr(&value) {
            Const::Str(text) => Some(MappedInput::Text(text)),
            Const::Bool(flag) => Some(MappedInput::Bool(flag)),
            Const::Number {
                value: number,
                decimal: false,
            } if number.scale == 0 => i64::try_from(number.mantissa).ok().map(MappedInput::Int),
            _ => {
                self.invalid(
                    span,
                    format!("`{written}` isn't a value a mapping can give"),
                    "not mapped data",
                    "Give `request.<part>`, `settings.<name>`, or text, a whole number or `true`"
                        .to_string(),
                );
                None
            }
        }
    }

    /// `{ each: request.messages  as: { ... } }`, for a list field of
    /// objects.
    fn each(
        &mut self,
        member: &ast::Member,
        ty: &Ty,
        apart: &mut bool,
        table: &dyn TypeView,
    ) -> Option<MappedInput> {
        let shape =
            "Write `{ each: request.messages  as: { role: item.role  content: item.content } }`";
        let mut each = None;
        let mut shaped = None;
        for (key, inner) in tree::members(member.block()) {
            match name_text(&key).as_deref() {
                Some("each") => each = inner.value().map(|value| Self::written(&value)),
                Some("as") => shaped = Some(inner),
                _ => {
                    self.invalid(
                        self.span(&key),
                        "A list of messages is mapped with `each:` and `as:` alone".to_string(),
                        "not part of a list's mapping",
                        shape.to_string(),
                    );
                    return None;
                }
            }
        }
        let item = match ty.strip_nullable() {
            Ty::List(item) => match item.strip_nullable() {
                Ty::Named(name) if table.fields(name).is_some() => Some(name.clone()),
                _ => None,
            },
            _ => None,
        };
        let (Some("request.messages"), Some(shaped), Some(item)) = (each.as_deref(), shaped, item)
        else {
            self.invalid(
                self.span(member),
                "Only `request.messages` is mapped as a list, onto a list of objects".to_string(),
                "not a list of messages",
                shape.to_string(),
            );
            return None;
        };
        let parts = self.object(&shaped, &item, true, apart, table)?;
        Some(MappedInput::Messages(parts))
    }

    /// `response.<field>...` walked through the operation's output, `first`
    /// stepping into a list's first item: the field names, `0` for
    /// `first`.
    fn walk(
        &mut self,
        member: &ast::Member,
        output: &Ty,
        ends: Ends,
        table: &dyn TypeView,
    ) -> Option<Vec<String>> {
        let value = member.value()?;
        let written = Self::written(&value);
        let span = self.span(&value);
        let parts = dotted(&value).map(|(first, rest)| {
            std::iter::once(first)
                .chain(rest)
                .filter_map(|part| tree::name_ref_text(&part))
                .collect::<Vec<_>>()
        });
        let Some(parts) = parts.filter(|parts| {
            parts.first().map(String::as_str) == Some("response") && parts.len() > 1
        }) else {
            self.invalid(
                span,
                format!("`{written}` isn't a place in the answer"),
                "not a place",
                "Write `response.<field>`, `first` for a list's first item".to_string(),
            );
            return None;
        };
        let mut at = output.clone();
        let mut path = Vec::new();
        for name in &parts[1..] {
            let inner = at.strip_nullable().clone();
            if let Ty::List(item) = &inner
                && name == "first"
            {
                at = (**item).clone();
                path.push("0".to_string());
                continue;
            }
            let field = match &inner {
                Ty::Named(host) => table
                    .fields(host)
                    .and_then(|fields| fields.iter().find(|field| field.name == *name).cloned()),
                _ => None,
            };
            let Some(field) = field else {
                self.invalid(
                    span,
                    format!("`{written}` has no field `{name}`"),
                    "no such field",
                    "Name a field of the operation's output, `first` for a list's first item"
                        .to_string(),
                );
                return None;
            };
            at = field.declared.clone();
            path.push(name.clone());
        }
        let fits = match ends {
            Ends::Text => table.base_prim(&at) == Some(Prim::Text),
            Ends::Count => count(table, &at),
            Ends::Anything => true,
        };
        if !fits {
            let what = if ends == Ends::Text {
                "text"
            } else {
                "a count of tokens"
            };
            self.invalid(
                span,
                format!("`{written}` isn't {what}"),
                &format!("not {what}"),
                format!("Name a field of the answer that is {what}"),
            );
            return None;
        }
        Some(path)
    }

    /// `cut_off_when: { path: "<dotted>"  equals: <value> }`, on the answer
    /// as sent: its members by their spelling on the wire, a list's items
    /// by number.
    fn cut_off(
        &mut self,
        member: &ast::Member,
        output: &Ty,
        table: &dyn TypeView,
    ) -> Option<(String, String)> {
        let shape =
            "Write `cut_off_when: { path: \"choices.0.finish_reason\"  equals: \"length\" }`";
        let mut path = None;
        let mut equals = None;
        for (key, inner) in tree::members(member.block()) {
            let value = inner.value();
            match name_text(&key).as_deref() {
                Some("path") => {
                    path = value.as_ref().map(Const::from_expr).and_then(|c| match c {
                        Const::Str(text) => Some((text, self.span(&inner))),
                        _ => None,
                    })
                }
                Some("equals") => {
                    equals = value.as_ref().map(Const::from_expr).and_then(|c| match c {
                        Const::Str(text) => {
                            Some(jadpo_graph::json::Value::from(text.as_str()).to_document())
                        }
                        Const::Bool(flag) => Some(flag.to_string()),
                        Const::Number {
                            value: number,
                            decimal: false,
                        } if number.scale == 0 => Some(number.mantissa.to_string()),
                        _ => None,
                    })
                }
                _ => {
                    self.invalid(
                        self.span(&key),
                        "`cut_off_when:` takes `path:` and `equals:` alone".to_string(),
                        "not part of `cut_off_when`",
                        shape.to_string(),
                    );
                    return None;
                }
            }
        }
        let (Some((path, span)), Some(equals)) = (path, equals) else {
            self.invalid(
                self.span(member),
                "`cut_off_when:` needs a `path:` and the value it `equals:`".to_string(),
                "incomplete",
                shape.to_string(),
            );
            return None;
        };
        // Each step a member of the answer as sent, or a list's item.
        let mut at = output.clone();
        for step in path.split('.') {
            let inner = at.strip_nullable().clone();
            if let Ty::List(item) = &inner
                && step.parse::<usize>().is_ok()
            {
                at = (**item).clone();
                continue;
            }
            let field = match &inner {
                Ty::Named(host) => table.fields(host).and_then(|fields| {
                    fields
                        .iter()
                        .find(|field| field.wire_name.as_deref().unwrap_or(&field.name) == step)
                        .cloned()
                }),
                _ => None,
            };
            let Some(field) = field else {
                self.invalid(
                    span,
                    format!("The answer has nothing at `{path}`"),
                    &format!("no `{step}` there"),
                    "Name members of the answer as the provider sends them, a list's items by number"
                        .to_string(),
                );
                return None;
            };
            at = field.declared.clone();
        }
        Some((path, equals))
    }

    fn usage(
        &mut self,
        member: &ast::Member,
        output: &Ty,
        table: &dyn TypeView,
    ) -> Option<UsageModel> {
        let shape = "Write `usage: { input: response.<field>  output: response.<field> }`, with `cached:` where the provider says";
        let mut out = UsageModel::default();
        let mut given = (false, false);
        for (key, inner) in tree::members(member.block()) {
            let word = name_text(&key).unwrap_or_default();
            if !["input", "output", "cached"].contains(&word.as_str()) {
                self.invalid(
                    self.span(&key),
                    format!("`usage:` has no `{word}:`"),
                    "not a count",
                    shape.to_string(),
                );
                return None;
            }
            // `0` for a count the provider never has.
            let path = match inner.value().as_ref().map(Const::from_expr) {
                Some(Const::Number { value, .. }) if value.mantissa == 0 => None,
                _ => Some(self.walk(&inner, output, Ends::Count, table)?),
            };
            match word.as_str() {
                "input" => {
                    given.0 = true;
                    out.input = path;
                }
                "output" => {
                    given.1 = true;
                    out.output = path;
                }
                _ => out.cached = path,
            }
        }
        if !(given.0 && given.1) {
            self.invalid(
                self.span(member),
                "`usage:` needs the `input:` and `output:` tokens".to_string(),
                "incomplete",
                shape.to_string(),
            );
            return None;
        }
        Some(out)
    }

    /// `schema: { mode: AiSchema.<mode>  at: "<dotted>" }`: `at:` where a
    /// mode with an envelope writes it, never for `none`.
    fn schema(&mut self, member: &ast::Member) -> Option<(SchemaMode, Option<String>)> {
        let shape = "Write `schema: { mode: AiSchema.strict_subset  at: \"response_format\" }`, or `mode: AiSchema.none` for a provider with no structured output";
        let mut mode = None;
        let mut at = None;
        for (key, inner) in tree::members(member.block()) {
            match name_text(&key).as_deref() {
                Some("mode") => {
                    let written = inner.value().map(|v| Self::written(&v)).unwrap_or_default();
                    mode = match written.as_str() {
                        "AiSchema.strict_subset" => Some(SchemaMode::StrictSubset),
                        "AiSchema.json_only" => Some(SchemaMode::JsonOnly),
                        "AiSchema.none" => Some(SchemaMode::None),
                        _ => {
                            self.invalid(
                                self.span(&inner),
                                format!("`{written}` isn't a schema mode"),
                                "not an `AiSchema`",
                                "Use `AiSchema.strict_subset`, `AiSchema.json_only` or `AiSchema.none`".to_string(),
                            );
                            return None;
                        }
                    };
                }
                Some("at") => match inner.value().as_ref().map(Const::from_expr) {
                    Some(Const::Str(text))
                        if !text.is_empty() && text.split('.').all(|part| !part.is_empty()) =>
                    {
                        at = Some((text, self.span(&inner)));
                    }
                    _ => {
                        self.invalid(
                            self.span(&inner),
                            "`at:` isn't a place in the request".to_string(),
                            "not a place",
                            "Write the member as text, dotted for one inside another, such as `at: \"response_format\"`".to_string(),
                        );
                        return None;
                    }
                },
                _ => {
                    self.invalid(
                        self.span(&key),
                        "`schema:` takes `mode:` and `at:` alone".to_string(),
                        "not part of `schema`",
                        shape.to_string(),
                    );
                    return None;
                }
            }
        }
        let Some(mode) = mode else {
            self.invalid(
                self.span(member),
                "`schema:` doesn't give `mode:`".to_string(),
                "no `mode:`",
                shape.to_string(),
            );
            return None;
        };
        match (mode, at) {
            (SchemaMode::None, Some((_, span))) => {
                self.invalid(
                    span,
                    "`AiSchema.none` sends no schema in the request".to_string(),
                    "nowhere to write it",
                    "Remove `at:`; with no structured mode the schema goes in the instructions"
                        .to_string(),
                );
                None
            }
            (SchemaMode::None, None) => Some((mode, None)),
            (_, Some((at, _))) => Some((mode, Some(at))),
            (_, None) => {
                self.invalid(
                    self.span(member),
                    "`schema:` doesn't say where in the request its mode goes".to_string(),
                    "no `at:`",
                    "Add `at:`, the member the provider takes its structured-output setting in, such as `at: \"response_format\"`".to_string(),
                );
                None
            }
        }
    }

    /// `models: { context: <library>.<constant>  prices: <library>.<constant> }`:
    /// each model's context window, from the first, and its prices, from
    /// the second.
    #[allow(clippy::type_complexity)]
    fn models(
        &mut self,
        member: &ast::Member,
    ) -> Option<(Vec<(String, u64)>, Vec<jadpo_std::ai::AiPrice>)> {
        let shape = "Write `models: { context: <library>.<table> }`, a constant of the package's `libraries/` listing each model's `model:` and its context window in `tokens:`";
        let mut context = None;
        let mut prices = Vec::new();
        let mut ok = true;
        for (key, inner) in tree::members(member.block()) {
            match name_text(&key).as_deref() {
                Some("context") => context = Some(inner),
                Some("prices") => match self.prices(&inner) {
                    Some(read) => prices = read,
                    None => ok = false,
                },
                _ => {
                    ok = false;
                    self.invalid(
                        self.span(&key),
                        "`models:` takes `context:` and `prices:` alone".to_string(),
                        "not part of `models`",
                        shape.to_string(),
                    );
                }
            }
        }
        let Some(context) = context else {
            self.invalid(
                self.span(member),
                "`models:` doesn't give `context:`".to_string(),
                "no `context:`",
                shape.to_string(),
            );
            return None;
        };
        let table = self.constant(&context)?;
        let mut out = Vec::new();
        let read = match &table {
            crate::constants::Value::List(items) => items.iter().try_for_each(|item| {
                let crate::constants::Value::Object(_, fields) = item else {
                    return Err(());
                };
                let model = fields.iter().find_map(|(name, value)| match value {
                    crate::constants::Value::Text(text) if name == "model" => Some(text.clone()),
                    _ => None,
                });
                let tokens = fields.iter().find_map(|(name, value)| match value {
                    crate::constants::Value::Number { value, .. } if name == "tokens" => {
                        u64::try_from(value.mantissa)
                            .ok()
                            .filter(|_| value.scale == 0)
                    }
                    _ => None,
                });
                match (model, tokens) {
                    (Some(model), Some(tokens)) => {
                        out.push((model, tokens));
                        Ok(())
                    }
                    _ => Err(()),
                }
            }),
            _ => Err(()),
        };
        if read.is_err() {
            self.invalid(
                self.span(&context),
                "`context:` isn't a table of models and their context windows".to_string(),
                "not each model's context",
                shape.to_string(),
            );
            return None;
        }
        ok.then_some((out, prices))
    }

    /// `models: { prices: <library>.<constant> }` (chunk 5.5v5): a table
    /// of each model's price per million tokens, as objects of `model:`,
    /// its name after its provider; `currency:`, a `Currency`; `input:`,
    /// `cached:` and `output:`, decimals in the currency's major unit to
    /// six places; and `from:`, the day the price holds from.
    fn prices(&mut self, member: &ast::Member) -> Option<Vec<jadpo_std::ai::AiPrice>> {
        use crate::constants::Value;
        let table = self.constant(member)?;
        let micros = |value: &Value| match value {
            Value::Number { value, .. } if value.mantissa >= 0 && value.scale <= 6 => {
                u64::try_from(value.mantissa)
                    .ok()?
                    .checked_mul(10u64.pow(6 - value.scale))
            }
            _ => None,
        };
        let row = |item: &Value| -> Option<jadpo_std::ai::AiPrice> {
            let Value::Object(_, fields) = item else {
                return None;
            };
            let field = |name: &str| {
                fields
                    .iter()
                    .find(|(field, _)| field == name)
                    .map(|(_, value)| value)
            };
            let model = match field("model")? {
                Value::Text(text) => text.clone(),
                _ => return None,
            };
            let currency = match field("currency")? {
                Value::Variant(_, variant, data) if data.is_empty() => jadpo_std::money::CURRENCIES
                    .iter()
                    .find(|(name, _)| name == variant)
                    .map(|(_, code)| (*code).to_string())?,
                _ => return None,
            };
            let from = match field("from")? {
                Value::Text(text) if is_day(text) => text.clone(),
                _ => return None,
            };
            Some(jadpo_std::ai::AiPrice {
                model,
                from,
                currency,
                input: micros(field("input")?)?,
                cached: micros(field("cached")?)?,
                output: micros(field("output")?)?,
            })
        };
        let read = match &table {
            Value::List(items) => items.iter().map(row).collect::<Option<Vec<_>>>(),
            _ => None,
        };
        if read.is_none() {
            let span = member
                .value()
                .map_or_else(|| self.span(member), |value| self.span(&value));
            self.invalid(
                span,
                "`prices:` isn't a table of models and their prices".to_string(),
                "not each model's prices",
                "Write `models: { prices: <library>.<table> }`, a constant listing each model's `model:`, `currency:`, its price per million `input:`, `cached:` and `output:` tokens to six places, and `from:`, the day the price holds from".to_string(),
            );
        }
        read
    }

    /// `<library>.<constant>` in the package's own libraries, worked out.
    fn constant(&mut self, member: &ast::Member) -> Option<crate::constants::Value> {
        let value = member.value()?;
        let written = Self::written(&value);
        let span = self.span(&value);
        let parts: Vec<String> = dotted(&value)
            .map(|(first, rest)| {
                std::iter::once(first)
                    .chain(rest)
                    .filter_map(|part| tree::name_ref_text(&part))
                    .collect()
            })
            .unwrap_or_default();
        if let [library, name] = parts.as_slice() {
            for &file in self.project.files(self.db) {
                let location = jadpo_db::location(self.db, self.project, file);
                if location.ownership.package() != Some(self.home) {
                    continue;
                }
                for item in &jadpo_db::item_tree(self.db, file).items {
                    if item.kind != ItemKind::Library || item.name != *library {
                        continue;
                    }
                    let Some(constant) = item
                        .functions
                        .iter()
                        .find(|member| member.kind == ItemKind::Constant && member.name == *name)
                    else {
                        continue;
                    };
                    let path = file.path(self.db).clone();
                    // A constant that doesn't work out is reported where
                    // it is declared.
                    return crate::constants::evaluated(
                        self.db,
                        self.project,
                        &path,
                        item.index,
                        Some(constant.index),
                    )
                    .and_then(|evaluated| evaluated.value.clone());
                }
            }
        }
        self.invalid(
            span,
            format!("`{written}` isn't a constant of the package's libraries"),
            "no such table",
            "Name a constant of a library in the package's `libraries/`, `<library>.<constant>`"
                .to_string(),
        );
        None
    }
}

/// Whether `text` is a day as ISO 8601 writes it, `2026-04-14`.
fn is_day(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(at, byte)| match at {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

/// Whether `ty` counts tokens: a whole number, or a `TokenCount`.
fn count(table: &dyn TypeView, ty: &Ty) -> bool {
    table.base_prim(ty) == Some(Prim::Int)
        || matches!(
            ty.strip_nullable(),
            Ty::Opaque(name, args)
                if args.is_empty() && name == jadpo_std::standard::ai::TOKEN_COUNT
        )
}

/// A type's name as its own package writes it, without the package's.
fn own_name(host: &str) -> &str {
    let shown = crate::types::shown(host);
    shown.rsplit_once('.').map_or(shown, |(_, own)| own)
}
