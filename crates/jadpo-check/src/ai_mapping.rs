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
//! - **`stream:`** (chunk 5.5v7) calls an operation whose answer is a
//!   stream of server-sent events, its input made as `generate:`'s is,
//!   and names where each event's piece of text is (`pieces:
//!   event.<field>...`), what says the model declined (`refused_when:`),
//!   was cut off (`cut_off_when:`) or is done (`done_when:`).
//! - **`embed:`** (chunk 5.5v7) calls an operation with `request.model`,
//!   `request.texts` and `request.dimensions`, and names the answer's
//!   list of vectors (`vectors: response.<field>`), each item's vector
//!   (`vector: item.<field>`) and its place among the texts (`index:`),
//!   and the most texts the provider takes at once (`batch:`).
//! - **`refused_when:`** is a place that holds anything, or `{ holds:
//!   path: equals: }`, any of them saying so; **`schema: { envelope: }`**
//!   names the provider's own shape for the structured mode
//!   (`AiEnvelope.openai`, the default, or `AiEnvelope.anthropic`).

use std::collections::BTreeMap;

use jadpo_db::{Db, Folder, ItemKind, Ownership, PackageHome, Place, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::ai::{AiEnvelope, AiMarker, EACH, SchemaMode};
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

/// What `stream:` holds (chunk 5.5v7).
const STREAM_KEYS: [&str; 7] = [
    "call",
    "input",
    "pieces",
    "refused_when",
    "cut_off_when",
    "done_when",
    "usage",
];

/// What `embed:` holds (chunk 5.5v7).
const EMBED_KEYS: [&str; 7] = [
    "call", "input", "vectors", "vector", "index", "batch", "usage",
];

/// Which of the capability's operations a mapping's input is made for:
/// what `request.<part>` it may read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Asked {
    Generate,
    Stream,
    Embed,
}

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
    /// `request.texts`, onto a list of text (`embed:`).
    Texts,
    /// `request.dimensions`, `ai: { embedding_dimensions: }`, onto a whole
    /// number (`embed:`).
    Dimensions,
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
    /// By field names, for a [`AiMarker::Holds`].
    pub refused_when: Vec<AiMarker>,
    pub cut_off_when: Option<(String, String)>,
    pub usage: UsageModel,
    pub schema: SchemaMode,
    pub schema_at: Option<String>,
    pub envelope: AiEnvelope,
    /// Each model's context window, by its name after its provider.
    pub contexts: Vec<(String, u64)>,
    pub stream: Option<StreamMapping>,
    pub embed: Option<EmbedMapping>,
}

/// A package's `stream:`, read (chunk 5.5v7): each place by spellings on
/// the wire, `0` for a list's first item.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StreamMapping {
    pub operation: String,
    pub input: Vec<(String, MappedInput)>,
    pub instructions_apart: bool,
    pub pieces: Vec<String>,
    pub refused_when: Vec<AiMarker>,
    pub cut_off_when: Option<(String, String)>,
    pub done_when: Option<(String, String)>,
    pub usage: UsageModel,
}

/// A package's `embed:`, read (chunk 5.5v7): each place by spellings on
/// the wire.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EmbedMapping {
    pub operation: String,
    pub input: Vec<(String, MappedInput)>,
    pub vectors: Vec<String>,
    pub vector: Vec<String>,
    pub index: Option<Vec<String>>,
    pub batch: u32,
    pub usage: UsageModel,
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

/// Whether the package connection `connection` maps `what` (`stream`,
/// `embed`) in its `provide ai`; `None` when it isn't a package's
/// connection whose mapping reads.
pub fn maps(db: &dyn Db, project: Project, connection: &str, what: &str) -> Option<bool> {
    let folder = crate::connections::package_connections(db, project)
        .connections
        .iter()
        .find(|known| known.name == connection)
        .map(|known| known.folder.clone())?;
    let mapping = mappings(db, project).of.get(&folder)?;
    Some(match what {
        "stream" => mapping.stream.is_some(),
        _ => mapping.embed.is_some(),
    })
}

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
    /// A `Vector`, or a list of numbers.
    Vector,
    /// A list of objects.
    Objects,
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
        let stream = match members.get("stream") {
            Some(stream) => {
                let read = self.stream(stream);
                ok &= read.is_some();
                read
            }
            None => None,
        };
        let embed = match members.get("embed") {
            Some(embed) => {
                let read = self.embed(embed);
                ok &= read.is_some();
                read
            }
            None => None,
        };
        let (Some(contexts), Some(mut mapping)) = (contexts, generate) else {
            return None;
        };
        if !ok {
            return None;
        }
        mapping.contexts = contexts;
        mapping.stream = stream;
        mapping.embed = embed;
        Some(mapping)
    }

    /// The members of `member`'s block, each one of `keys`, and every one
    /// of `needed` among them; `what` names the block.
    fn block(
        &mut self,
        member: &ast::Member,
        what: &str,
        keys: &[&str],
        needed: &[(&str, &str)],
        shape: &str,
    ) -> Option<BTreeMap<String, ast::Member>> {
        let Some(block) = member.block() else {
            self.invalid(
                self.span(member),
                format!("`{what}:` isn't a block"),
                "not a block",
                shape.to_string(),
            );
            return None;
        };
        let mut members: BTreeMap<String, ast::Member> = BTreeMap::new();
        for (key, inner) in tree::members(Some(block)) {
            let written = name_text(&key).unwrap_or_default();
            if !keys.contains(&written.as_str()) {
                let next = match nearest(&written, keys.iter().copied()) {
                    Some(near) => format!("Use `{near}:` instead of `{written}:`"),
                    None => format!("Remove `{written}:`"),
                };
                self.invalid(
                    self.span(&key),
                    format!("`{what}:` has no `{written}:`"),
                    &format!("not part of `{what}`"),
                    next,
                );
                return None;
            }
            members.insert(written, inner);
        }
        for (key, next) in needed {
            if !members.contains_key(*key) {
                self.invalid(
                    self.span(member),
                    format!("`{what}:` doesn't give `{key}:`"),
                    &format!("no `{key}:`"),
                    (*next).to_string(),
                );
                return None;
            }
        }
        Some(members)
    }

    fn generate(&mut self, member: &ast::Member) -> Option<AiMapping> {
        let members = self.block(
            member,
            "generate",
            &GENERATE_KEYS,
            &[
                ("call", "Add `call: <resource>.<operation>`, the operation that asks the model"),
                ("input", "Add `input: { model: request.model  ... }`"),
                ("answer", "Add `answer: response.<field>`, where the answer's text is"),
                ("usage", "Add `usage: { input: response.<field>  output: response.<field> }`"),
                ("schema", "Add `schema: { mode: AiSchema.none }`, or the mode the provider's structured output has"),
            ],
            "Write `generate: { call: <resource>.<operation>  input: { }  answer: response.<field> }`",
        )?;
        let operation = self.operation(&members["call"])?;
        let table = type_table(self.db, self.project);
        let (input, instructions_apart) =
            self.input(&members["input"], &operation, Asked::Generate, table)?;
        let output = operation.output.clone();
        let (answer, _, _) = self.place(
            &members["answer"],
            "response",
            &output,
            Ends::Text,
            true,
            table,
        )?;
        let refused_when = match members.get("refused_when") {
            Some(refused) => self.markers(refused, "response", &output, false, table)?,
            None => Vec::new(),
        };
        let cut_off_when = match members.get("cut_off_when") {
            Some(cut) => Some(self.cut_off(cut, &output, table)?),
            None => None,
        };
        let usage = self.usage(&members["usage"], "response", &output, table)?;
        let (schema, schema_at, envelope) = self.schema(&members["schema"])?;
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
            envelope,
            contexts: Vec::new(),
            stream: None,
            embed: None,
        })
    }

    /// `stream:` (chunk 5.5v7): an operation whose answer is a stream of
    /// events, each read as its output type says, by spellings on the
    /// wire.
    fn stream(&mut self, member: &ast::Member) -> Option<StreamMapping> {
        let members = self.block(
            member,
            "stream",
            &STREAM_KEYS,
            &[
                ("call", "Add `call: <resource>.<operation>`, the operation whose answer is the model's stream"),
                ("input", "Add `input: { model: request.model  ...  stream: true }`"),
                ("pieces", "Add `pieces: event.<field>`, where each event's piece of text is"),
            ],
            "Write `stream: { call: <resource>.<operation>  input: { }  pieces: event.<field> }`",
        )?;
        let operation = self.operation(&members["call"])?;
        let table = type_table(self.db, self.project);
        let (input, instructions_apart) =
            self.input(&members["input"], &operation, Asked::Stream, table)?;
        let event = operation.output.clone();
        let (_, pieces, _) = self.place(
            &members["pieces"],
            "event",
            &event,
            Ends::Text,
            false,
            table,
        )?;
        let refused_when = match members.get("refused_when") {
            Some(refused) => self.markers(refused, "event", &event, true, table)?,
            None => Vec::new(),
        };
        let cut_off_when = match members.get("cut_off_when") {
            Some(cut) => Some(self.cut_off(cut, &event, table)?),
            None => None,
        };
        let done_when = match members.get("done_when") {
            Some(done) => Some(self.cut_off(done, &event, table)?),
            None => None,
        };
        let usage = match members.get("usage") {
            Some(usage) => self.usage(usage, "event", &event, table)?,
            None => UsageModel::default(),
        };
        Some(StreamMapping {
            operation: operation.full_name(),
            input,
            instructions_apart,
            pieces,
            refused_when,
            cut_off_when,
            done_when,
            usage,
        })
    }

    /// `embed:` (chunk 5.5v7): an operation sent `request.texts`, the list
    /// of vectors in its answer, each item's vector and its place, and the
    /// most texts the provider takes at once.
    fn embed(&mut self, member: &ast::Member) -> Option<EmbedMapping> {
        let members = self.block(
            member,
            "embed",
            &EMBED_KEYS,
            &[
                ("call", "Add `call: <resource>.<operation>`, the operation that embeds the texts"),
                ("input", "Add `input: { model: request.model  input: request.texts }`"),
                ("vectors", "Add `vectors: response.<field>`, the answer's list of embeddings"),
                ("vector", "Add `vector: item.<field>`, where each item's vector is"),
                ("batch", "Add `batch: <n>`, the most texts the provider embeds in one request"),
                ("usage", "Add `usage: { input: response.<field>  output: 0 }`"),
            ],
            "Write `embed: { call: <resource>.<operation>  input: { }  vectors: response.<field>  vector: item.<field>  batch: <n>  usage: { } }`",
        )?;
        let operation = self.operation(&members["call"])?;
        let table = type_table(self.db, self.project);
        let (input, _) = self.input(&members["input"], &operation, Asked::Embed, table)?;
        if !input
            .iter()
            .any(|(_, mapped)| *mapped == MappedInput::Texts)
        {
            self.invalid(
                self.span(&members["input"]),
                "`embed:` doesn't send the texts".to_string(),
                "no `request.texts`",
                "Map the field the provider takes its texts in to `request.texts`".to_string(),
            );
            return None;
        }
        let output = operation.output.clone();
        let (_, vectors, item) = self.place(
            &members["vectors"],
            "response",
            &output,
            Ends::Objects,
            false,
            table,
        )?;
        let (_, vector, _) = self.place(
            &members["vector"],
            "item",
            &item,
            Ends::Vector,
            false,
            table,
        )?;
        let index = match members.get("index") {
            Some(index) => Some(
                self.place(index, "item", &item, Ends::Count, false, table)?
                    .1,
            ),
            None => None,
        };
        let batch_member = &members["batch"];
        let batch = match batch_member.value().as_ref().map(Const::from_expr) {
            Some(Const::Number {
                value,
                decimal: false,
            }) if value.scale == 0 => u32::try_from(value.mantissa).ok().filter(|n| *n >= 1),
            _ => None,
        };
        let Some(batch) = batch else {
            self.invalid(
                self.span(batch_member),
                "`batch:` isn't a whole number of texts".to_string(),
                "not a count",
                "Write `batch: <n>`, the most texts the provider embeds in one request, such as `batch: 2048`".to_string(),
            );
            return None;
        };
        if (batch as usize) < jadpo_std::embed::MOST_TEXTS {
            self.out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("Embedding fewer than {} texts a request isn't built yet", jadpo_std::embed::MOST_TEXTS),
                )
                .at(self.span(batch_member), "decided, not built")
                .reason(format!(
                    "Area 21 has the package split an `ai.embed_all` of up to {} texts into the provider's batch size; this compiler sends them in one request, so it needs a provider that takes at least that many.",
                    jadpo_std::embed::MOST_TEXTS
                ))
                .next(format!("Use a `batch:` of at least {}", jadpo_std::embed::MOST_TEXTS)),
            );
            return None;
        }
        let usage = self.usage(&members["usage"], "response", &output, table)?;
        Some(EmbedMapping {
            operation: operation.full_name(),
            input,
            vectors,
            vector,
            index,
            batch,
            usage,
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
        asked: Asked,
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
        let input = self.object(member, &host, false, &mut apart, asked, table)?;
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
        asked: Asked,
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
            let mapped = self.value(value, &field.declared, item, apart, asked, table)?;
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
        asked: Asked,
        table: &dyn TypeView,
    ) -> Option<MappedInput> {
        if member.block().is_some() {
            if asked == Asked::Embed {
                self.invalid(
                    self.span(member),
                    "An embedding's input maps no list of messages".to_string(),
                    "not an embedding's input",
                    "Give `request.model`, `request.texts`, `request.dimensions`, `settings.<name>` or a value".to_string(),
                );
                return None;
            }
            return self.each(member, ty, apart, asked, table);
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
            let chat = asked != Asked::Embed;
            let (mapped, fits, what) = match (first.as_str(), rest.as_slice()) {
                ("request", [name]) if name == "texts" && !chat => (
                    MappedInput::Texts,
                    matches!(ty.strip_nullable(), Ty::List(inner) if table.base_prim(inner) == Some(Prim::Text)),
                    "a list of text",
                ),
                ("request", [name]) if name == "dimensions" && !chat => (
                    MappedInput::Dimensions,
                    table.base_prim(ty) == Some(Prim::Int),
                    "a whole number",
                ),
                ("request", [name]) if name == "model" && !item => (
                    MappedInput::Model,
                    table.base_prim(ty) == Some(Prim::Text),
                    "text",
                ),
                ("request", [name]) if name == "max_output" && !item && chat => (
                    MappedInput::MaxOutput,
                    table.base_prim(ty) == Some(Prim::Int),
                    "a whole number",
                ),
                ("request", [name]) if name == "instructions" && !item && chat => {
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
                ("request", [name]) if name == "messages" && !item && chat => {
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
                    } else if chat {
                        "Give `request.model`, `request.max_output`, `request.instructions`, `settings.<name>` or a value"
                    } else {
                        "Give `request.model`, `request.texts`, `request.dimensions`, `settings.<name>` or a value"
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
        asked: Asked,
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
        let parts = self.object(&shaped, &item, true, apart, asked, table)?;
        Some(MappedInput::Messages(parts))
    }

    /// `<root>.<field>...` walked through `ty`, `first` stepping into a
    /// list's first item and, where `join` allows, a field after a list
    /// read from every item: the field names and the spellings on the
    /// wire, `0` for `first` and [`EACH`] for every item, and the type it
    /// ends at.
    #[allow(clippy::type_complexity)]
    fn place(
        &mut self,
        member: &ast::Member,
        root: &str,
        ty: &Ty,
        ends: Ends,
        join: bool,
        table: &dyn TypeView,
    ) -> Option<(Vec<String>, Vec<String>, Ty)> {
        let value = member.value()?;
        let written = Self::written(&value);
        let span = self.span(&value);
        let parts = dotted(&value).map(|(first, rest)| {
            std::iter::once(first)
                .chain(rest)
                .filter_map(|part| tree::name_ref_text(&part))
                .collect::<Vec<_>>()
        });
        let Some(parts) = parts
            .filter(|parts| parts.first().map(String::as_str) == Some(root) && parts.len() > 1)
        else {
            let what = if root == "event" { "event" } else { "answer" };
            self.invalid(
                span,
                format!("`{written}` isn't a place in the {what}"),
                "not a place",
                format!("Write `{root}.<field>`, `first` for a list's first item"),
            );
            return None;
        };
        let mut at = ty.clone();
        let mut names = Vec::new();
        let mut wire = Vec::new();
        for name in &parts[1..] {
            let mut inner = at.strip_nullable().clone();
            if let Ty::List(item) = &inner {
                if name == "first" {
                    at = (**item).clone();
                    names.push("0".to_string());
                    wire.push("0".to_string());
                    continue;
                }
                if join {
                    inner = item.strip_nullable().clone();
                    names.push(EACH.to_string());
                    wire.push(EACH.to_string());
                }
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
            wire.push(field.wire_name.clone().unwrap_or_else(|| name.clone()));
            names.push(name.clone());
        }
        let fits = match ends {
            Ends::Text => table.base_prim(&at) == Some(Prim::Text),
            Ends::Count => count(table, &at),
            Ends::Anything => true,
            Ends::Vector => vector(table, &at),
            Ends::Objects => matches!(
                at.strip_nullable(),
                Ty::List(item) if matches!(item.strip_nullable(), Ty::Named(name) if table.fields(name).is_some())
            ),
        };
        if !fits {
            let what = match ends {
                Ends::Text => "text",
                Ends::Vector => "a vector",
                Ends::Objects => "a list of objects",
                _ => "a count of tokens",
            };
            self.invalid(
                span,
                format!("`{written}` isn't {what}"),
                &format!("not {what}"),
                format!("Name a field of the answer that is {what}"),
            );
            return None;
        }
        let at = match (ends, at.strip_nullable()) {
            (Ends::Objects, Ty::List(item)) => item.strip_nullable().clone(),
            _ => at,
        };
        Some((names, wire, at))
    }

    /// `refused_when:`: a place that holds anything, or `{ holds: <place>
    /// path: "<dotted>"  equals: <value> }`, any of them saying so; a
    /// place by spellings on the wire where `wire`, by field names
    /// otherwise.
    fn markers(
        &mut self,
        member: &ast::Member,
        root: &str,
        output: &Ty,
        wire: bool,
        table: &dyn TypeView,
    ) -> Option<Vec<AiMarker>> {
        let shape = format!(
            "Write `refused_when: {root}.<field>`, or `refused_when: {{ holds: {root}.<field>  path: \"<member>\"  equals: <value> }}`"
        );
        if member.block().is_none() {
            let (names, spelled, _) =
                self.place(member, root, output, Ends::Anything, false, table)?;
            return Some(vec![AiMarker::Holds(if wire { spelled } else { names })]);
        }
        let mut out = Vec::new();
        let mut path = None;
        let mut equals = None;
        for (key, inner) in tree::members(member.block()) {
            match name_text(&key).as_deref() {
                Some("holds") => {
                    let (names, spelled, _) =
                        self.place(&inner, root, output, Ends::Anything, false, table)?;
                    out.push(AiMarker::Holds(if wire { spelled } else { names }));
                }
                Some("path") => path = Some(inner),
                Some("equals") => equals = Some(inner),
                _ => {
                    self.invalid(
                        self.span(&key),
                        "`refused_when:` takes `holds:`, `path:` and `equals:` alone".to_string(),
                        "not part of `refused_when`",
                        shape,
                    );
                    return None;
                }
            }
        }
        match (path, equals) {
            (Some(path), Some(equals)) => {
                let marker = self.path_equals(member, &path, &equals, output, table, &shape)?;
                out.push(AiMarker::Equals(marker.0, marker.1));
            }
            (None, None) if !out.is_empty() => {}
            _ => {
                self.invalid(
                    self.span(member),
                    "`refused_when:` needs a `path:` with the value it `equals:`, or a place it `holds:`".to_string(),
                    "incomplete",
                    shape,
                );
                return None;
            }
        }
        Some(out)
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
        let what = member
            .member_key()
            .and_then(|key| key.names().next())
            .and_then(|name| name_text(&name))
            .unwrap_or_else(|| "cut_off_when".to_string());
        let shape = if what == "done_when" {
            "Write `done_when: { path: \"type\"  equals: \"message_stop\" }`".to_string()
        } else {
            format!("Write `{what}: {{ path: \"choices.0.finish_reason\"  equals: \"length\" }}`")
        };
        let mut path = None;
        let mut equals = None;
        for (key, inner) in tree::members(member.block()) {
            match name_text(&key).as_deref() {
                Some("path") => path = Some(inner),
                Some("equals") => equals = Some(inner),
                _ => {
                    self.invalid(
                        self.span(&key),
                        format!("`{what}:` takes `path:` and `equals:` alone"),
                        &format!("not part of `{what}`"),
                        shape,
                    );
                    return None;
                }
            }
        }
        let (Some(path), Some(equals)) = (path, equals) else {
            self.invalid(
                self.span(member),
                format!("`{what}:` needs a `path:` and the value it `equals:`"),
                "incomplete",
                shape,
            );
            return None;
        };
        self.path_equals(member, &path, &equals, output, table, &shape)
    }

    /// `path: "<dotted>"  equals: <value>`, on the answer as sent: its
    /// members by their spelling on the wire, a list's items by number;
    /// the value as JSON.
    fn path_equals(
        &mut self,
        member: &ast::Member,
        path: &ast::Member,
        equals: &ast::Member,
        output: &Ty,
        table: &dyn TypeView,
        shape: &str,
    ) -> Option<(String, String)> {
        let span = self.span(path);
        let written = path
            .value()
            .as_ref()
            .map(Const::from_expr)
            .and_then(|c| match c {
                Const::Str(text) if !text.is_empty() => Some(text),
                _ => None,
            });
        let value = equals
            .value()
            .as_ref()
            .map(Const::from_expr)
            .and_then(|c| match c {
                Const::Str(text) => {
                    Some(jadpo_graph::json::Value::from(text.as_str()).to_document())
                }
                Const::Bool(flag) => Some(flag.to_string()),
                Const::Number {
                    value: number,
                    decimal: false,
                } if number.scale == 0 => Some(number.mantissa.to_string()),
                _ => None,
            });
        let (Some(path), Some(value)) = (written, value) else {
            self.invalid(
                self.span(member),
                "A `path:` is dotted text, and the value it `equals:` text, a whole number or a flag".to_string(),
                "not a marker",
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
        Some((path, value))
    }

    fn usage(
        &mut self,
        member: &ast::Member,
        root: &str,
        output: &Ty,
        table: &dyn TypeView,
    ) -> Option<UsageModel> {
        let shape = format!(
            "Write `usage: {{ input: {root}.<field>  output: {root}.<field> }}`, with `cached:` where the provider says"
        );
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
                _ => Some(
                    self.place(&inner, root, output, Ends::Count, false, table)?
                        .0,
                ),
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

    /// `schema: { mode: AiSchema.<mode>  at: "<dotted>"  envelope:
    /// AiEnvelope.<shape> }`: `at:` where a mode with an envelope writes
    /// it, never for `none`, and the envelope's shape, OpenAI's unless it
    /// says.
    #[allow(clippy::type_complexity)]
    fn schema(&mut self, member: &ast::Member) -> Option<(SchemaMode, Option<String>, AiEnvelope)> {
        let shape = "Write `schema: { mode: AiSchema.strict_subset  at: \"response_format\" }`, or `mode: AiSchema.none` for a provider with no structured output";
        let mut mode = None;
        let mut at = None;
        let mut envelope: Option<(AiEnvelope, Span)> = None;
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
                Some("envelope") => {
                    let written = inner.value().map(|v| Self::written(&v)).unwrap_or_default();
                    let shaped = match written.as_str() {
                        "AiEnvelope.openai" => AiEnvelope::OpenAi,
                        "AiEnvelope.anthropic" => AiEnvelope::Anthropic,
                        _ => {
                            self.invalid(
                                self.span(&inner),
                                format!("`{written}` isn't an envelope"),
                                "not an `AiEnvelope`",
                                "Use `AiEnvelope.openai` or `AiEnvelope.anthropic`".to_string(),
                            );
                            return None;
                        }
                    };
                    envelope = Some((shaped, self.span(&inner)));
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
                        "`schema:` takes `mode:`, `at:` and `envelope:` alone".to_string(),
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
        if let Some((shaped, span)) = envelope.clone() {
            let refused = match (mode, shaped) {
                (SchemaMode::None, _) => {
                    Some("`AiSchema.none` sends no envelope, so it has no shape to name")
                }
                (SchemaMode::JsonOnly, AiEnvelope::Anthropic) => Some(
                    "Anthropic's structured outputs take a schema; they have no mode that promises JSON alone",
                ),
                _ => None,
            };
            if let Some(reason) = refused {
                self.invalid(
                    span,
                    "This envelope doesn't go with the schema's mode".to_string(),
                    "no such envelope for the mode",
                    format!("Remove `envelope:`, or change `mode:`: {reason}"),
                );
                return None;
            }
        }
        let envelope = envelope.map(|(shaped, _)| shaped).unwrap_or_default();
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
            (SchemaMode::None, None) => Some((mode, None, envelope)),
            (_, Some((at, _))) => Some((mode, Some(at), envelope)),
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
    /// each model's context window, from the first.
    fn models(&mut self, member: &ast::Member) -> Option<Vec<(String, u64)>> {
        let shape = "Write `models: { context: <library>.<table> }`, a constant of the package's `libraries/` listing each model's `model:` and its context window in `tokens:`";
        let mut context = None;
        let mut ok = true;
        for (key, inner) in tree::members(member.block()) {
            match name_text(&key).as_deref() {
                Some("context") => context = Some(inner),
                Some("prices") => ok &= self.constant(&inner).is_some(),
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
        ok.then_some(out)
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

/// Whether `ty` counts tokens: a whole number, or a `TokenCount`.
fn count(table: &dyn TypeView, ty: &Ty) -> bool {
    table.base_prim(ty) == Some(Prim::Int)
        || matches!(
            ty.strip_nullable(),
            Ty::Opaque(name, args)
                if args.is_empty() && name == jadpo_std::standard::ai::TOKEN_COUNT
        )
}

/// Whether `ty` is a vector: a `Vector`, or a list of numbers.
fn vector(table: &dyn TypeView, ty: &Ty) -> bool {
    match table.structure(ty.strip_nullable()) {
        Ty::Opaque(name, _) => name == jadpo_std::standard::ai::VECTOR,
        Ty::List(item) => matches!(table.base_prim(&item), Some(Prim::Decimal | Prim::Int)),
        _ => false,
    }
}

/// A type's name as its own package writes it, without the package's.
fn own_name(host: &str) -> &str {
    let shown = crate::types::shown(host);
    shown.rsplit_once('.').map_or(shown, |(_, own)| own)
}
