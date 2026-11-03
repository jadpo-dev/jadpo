//! A package's public surface (area 11, "The surface"; area 18's
//! `surface` record; chunk 7.1b): what the package offers an app, worked
//! out from its code and written as `surface.jadpo_record` beside its
//! `package.jadpo`.
//!
//! The surface holds the package's description, kind, version, licence
//! and the Jadpo version it needs; for a provider, how its API is reached
//! and each public resource's operations; each public library's
//! functions with their signatures and failures; each public type with
//! its fields, rules or variants; each public failure with its kind and
//! code; and each recipe. Every entry points at its file, relative to the
//! package's folder, so an agent reads the surface first and opens only
//! what it needs. A description is its first paragraph on one line;
//! `jadpo package show` prints the whole of it.
//!
//! Sections come in the record format's order, entries by name, fields by
//! name, repeated facts by value: reordering source changes nothing.
//! [`Surface`] is the one model; the record's text and `jadpo package
//! show`'s screens are both written from it.

use std::collections::BTreeMap;

use jadpo_db::record::{self as format, Entry, Member};
use jadpo_db::{Db, PackageHome, Project};
use jadpo_graph::{Base, Detail, Graph, Key, Kind, Node, TypeRef};
use jadpo_syntax::ast::{self, AstNode, Decl};

use crate::graph::text::{LineIndex, doc};
use crate::tree;
use crate::types::rules::unescape;

/// A package's surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Surface {
    /// Its name and version label, `Postcodes.v2`.
    pub package: String,
    /// Where it is in the project: `packages/postcodes/v2`.
    pub folder: String,
    /// `library` or `provider`.
    pub kind: &'static str,
    /// Installed in `packages/`, rather than the app's own.
    pub installed: bool,
    /// Its index's `///` description, whole.
    pub description: Option<String>,
    /// `mit`, from `Licence.mit`.
    pub licence: Option<String>,
    /// The earliest Jadpo version it needs, `1.0`.
    pub jadpo: Option<String>,
    /// Whether it carries `GUIDE.md`.
    pub guide: bool,
    /// The locales a library package words its text in, as `Locale`'s
    /// variants write them (`en_gb`), in order: one `locale` line each.
    pub locales: Vec<String>,
    /// How its API is reached, for a provider.
    pub provider: Option<Provider>,
    /// Its public declarations, in the record's order.
    pub declarations: Vec<Declaration>,
}

/// How a provider package's API is reached.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Provider {
    pub description: Option<String>,
    pub line: Option<u32>,
    /// The record's lines after its file: `protocol json_http`,
    /// `host live https://…`.
    pub facts: Vec<String>,
    /// Whether it declares an `erasure:` call (area 15), a fact of a
    /// release's that isn't in the record yet.
    pub erasure: bool,
}

/// One public declaration of a package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    /// As the app writes it after the package's name: `uk.parse`,
    /// `UkPostcode`, `bookings.create`.
    pub name: String,
    pub description: Option<String>,
    /// Relative to the package's folder.
    pub file: String,
    /// Where it is declared, from 1.
    pub line: Option<u32>,
    pub what: What,
}

/// What a declaration is, with what the surface says of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum What {
    /// A provider's operation.
    Operation {
        method: String,
        path: String,
        input: Option<String>,
        output: Option<String>,
        /// The answer as sent and the package's function that decodes it,
        /// as written (chunk 7.2c2): the surface shows both the wire and
        /// the output, so a review sees where decoding ends.
        wire_output: Option<(String, String)>,
        output_at: Option<String>,
        changes: bool,
        idempotent: bool,
        fails: Vec<String>,
        /// `written` or `imported`, and the documentation page.
        source: Option<(String, Option<String>)>,
        /// The recording that confirms it.
        confirmed: Option<String>,
    },
    /// A provider package's public action, called through its connection
    /// (chunk 7.6).
    Action {
        params: Vec<(String, String)>,
        result: Option<String>,
        fails: Vec<String>,
        /// The failures that can follow its own changing call, which end
        /// the calling action for good with something done (area 12,
        /// "Several changing calls in one action"; chunk 7.5e).
        after_effect: Vec<String>,
    },
    /// A public library's function.
    Function {
        params: Vec<(String, String)>,
        result: Option<String>,
        fails: Vec<String>,
    },
    /// A public type.
    Type {
        /// What it is built on: `Object`, `Enum`, `Text`.
        base: String,
        /// A refined type's rules, in area 2's order.
        rules: Vec<(String, String)>,
        /// An object's fields, by name: the name and the type as written.
        fields: Vec<(String, String)>,
        /// An enum's variants, by name, each with its fields.
        variants: Vec<(String, Vec<(String, String)>)>,
    },
    /// A public failure.
    Failure {
        kind: Option<String>,
        code: String,
        message: Option<String>,
        fields: Vec<(String, String)>,
    },
    /// A library package's public message, called as
    /// `Postcodes.translations.<message>(...)` (area 10).
    Message {
        /// Its fields, by name: the call's values.
        fields: Vec<(String, String)>,
    },
    /// A recipe an app copies in.
    Recipe,
}

impl What {
    /// The word for it: `function`, `type`.
    pub fn word(&self) -> &'static str {
        match self {
            What::Operation { .. } => "operation",
            What::Action { .. } => "action",
            What::Function { .. } => "function",
            What::Type { .. } => "type",
            What::Failure { .. } => "failure",
            What::Message { .. } => "message",
            What::Recipe => "recipe",
        }
    }

    /// The record's sections in order (area 18's surface).
    fn section(&self) -> u8 {
        match self {
            What::Operation { .. } => 0,
            What::Action { .. } => 1,
            What::Function { .. } => 2,
            What::Message { .. } => 3,
            What::Type { .. } => 4,
            What::Failure { .. } => 5,
            What::Recipe => 6,
        }
    }
}

impl Declaration {
    /// Its heading in the record: `function uk.parse`, `type UkPostcode =
    /// Text`.
    pub fn heading(&self) -> String {
        match &self.what {
            What::Type { base, .. } => format!("type {} = {base}", self.name),
            what => format!("{} {}", what.word(), self.name),
        }
    }

    /// Its parameters and result as Jadpo writes a signature, without its
    /// failures: `(written: WrittenPostcode) -> UkPostcode`.
    pub fn signature(&self) -> Option<String> {
        let (What::Function { params, result, .. } | What::Action { params, result, .. }) =
            &self.what
        else {
            return None;
        };
        let params: Vec<String> = params
            .iter()
            .map(|(name, ty)| format!("{name}: {ty}"))
            .collect();
        let mut out = format!("({})", params.join(", "));
        if let Some(result) = result {
            out.push_str(&format!(" -> {result}"));
        }
        Some(out)
    }

    /// The record's lines after its description and file.
    pub fn facts(&self) -> Vec<Member> {
        let line = |text: String| Member {
            line: text,
            nested: Vec::new(),
        };
        let mut out = Vec::new();
        match &self.what {
            What::Operation {
                method,
                path,
                input,
                output,
                wire_output,
                output_at,
                changes,
                idempotent,
                fails,
                source,
                confirmed,
            } => {
                out.push(line(format!("request {method} {path}")));
                if let Some(input) = input {
                    out.push(line(format!("input {input}")));
                }
                if let Some(output) = output {
                    out.push(line(format!("output {output}")));
                }
                if let Some((wire, decode)) = wire_output {
                    out.push(line(format!("wire_output {wire} decode {decode}")));
                }
                if let Some(at) = output_at {
                    out.push(line(format!("output_at {}", format::quote(at))));
                }
                if *changes {
                    out.push(line("changes".into()));
                }
                if *idempotent {
                    out.push(line("idempotent".into()));
                }
                out.extend(fails.iter().map(|f| line(format!("fails {f}"))));
                if let Some((how, docs)) = source {
                    out.push(line(match docs {
                        Some(docs) => format!("source {how} {docs}"),
                        None => format!("source {how}"),
                    }));
                }
                if let Some(confirmed) = confirmed {
                    out.push(line(format!("confirmed {confirmed}")));
                }
            }
            What::Function { fails, .. } => {
                out.push(line(format!(
                    "signature {}",
                    self.signature().unwrap_or_default()
                )));
                out.extend(fails.iter().map(|f| line(format!("fails {f}"))));
            }
            What::Action {
                fails,
                after_effect,
                ..
            } => {
                out.push(line(format!(
                    "signature {}",
                    self.signature().unwrap_or_default()
                )));
                out.extend(fails.iter().map(|f| line(format!("fails {f}"))));
                out.extend(
                    after_effect
                        .iter()
                        .map(|f| line(format!("after_effect {f}"))),
                );
            }
            What::Type {
                rules,
                fields,
                variants,
                ..
            } => {
                let mut sorted: Vec<&(String, String)> = rules.iter().collect();
                sorted.sort();
                out.extend(sorted.iter().map(|(n, v)| line(format!("{n}: {v}"))));
                out.extend(fields.iter().map(|(n, t)| line(format!("{n}: {t}"))));
                for (variant, fields) in variants {
                    out.push(Member {
                        line: format!("variant {variant}"),
                        nested: fields.iter().map(|(n, t)| format!("{n}: {t}")).collect(),
                    });
                }
            }
            What::Failure {
                kind,
                code,
                message,
                fields,
            } => {
                if let Some(kind) = kind {
                    out.push(line(format!("kind {kind}")));
                }
                out.push(line(format!("code {code}")));
                if let Some(message) = message {
                    out.push(line(format!("message {}", format::quote(message))));
                }
                out.extend(fields.iter().map(|(n, t)| line(format!("{n}: {t}"))));
            }
            What::Message { fields } => {
                out.extend(fields.iter().map(|(n, t)| line(format!("{n}: {t}"))));
            }
            What::Recipe => {}
        }
        out
    }

    /// Its entry in the record.
    fn entry(&self) -> Entry {
        let mut entry = Entry::new(self.heading());
        if let Some(first) = self.description.as_deref().and_then(first_paragraph) {
            entry.push(format!("/// {first}"));
        }
        entry.push(format!("file {}", self.file));
        entry.members.extend(self.facts());
        entry
    }
}

/// A description's first paragraph, joined onto one line (the record
/// format's open question 3, as recommended).
pub fn first_paragraph(description: &str) -> Option<String> {
    let lines: Vec<&str> = description
        .lines()
        .skip_while(|line| line.trim().is_empty())
        .take_while(|line| !line.trim().is_empty())
        .map(str::trim)
        .collect();
    (!lines.is_empty()).then(|| lines.join(" "))
}

impl Surface {
    /// The record's entries: the package's, the provider's, then each
    /// declaration's, with `version` when it is known.
    pub fn entries(&self, version: Option<&str>) -> Vec<Entry> {
        let mut package = Entry::new(format!("package {}", self.package));
        if let Some(first) = self.description.as_deref().and_then(first_paragraph) {
            package.push(format!("/// {first}"));
        }
        package.push(format!("file {}", jadpo_db::PACKAGE_INDEX));
        package.push(format!("kind {}", self.kind));
        if let Some(version) = version {
            package.push(format!("version {version}"));
        }
        if let Some(licence) = &self.licence {
            package.push(format!("licence {licence}"));
        }
        if let Some(jadpo) = &self.jadpo {
            package.push(format!("jadpo {jadpo}"));
        }
        for locale in &self.locales {
            package.push(format!("locale {locale}"));
        }
        if self.guide {
            package.push(format!("guide {GUIDE}"));
        }
        let mut out = vec![package];
        if let Some(provider) = &self.provider {
            let mut entry = Entry::new("provider");
            if let Some(first) = provider.description.as_deref().and_then(first_paragraph) {
                entry.push(format!("/// {first}"));
            }
            entry.push(format!("file {}", jadpo_db::PROVIDER_FILE));
            for fact in &provider.facts {
                entry.push(fact.clone());
            }
            out.push(entry);
        }
        out.extend(self.declarations.iter().map(Declaration::entry));
        out
    }

    /// The record's text.
    pub fn text(&self, version: Option<&str>) -> String {
        format::write(
            &format::SURFACE,
            format::SURFACE_FROM,
            &self.entries(version),
        )
    }

    /// The public declaration the app writes as `name`.
    pub fn declaration(&self, name: &str) -> Option<&Declaration> {
        self.declarations.iter().find(|d| d.name == name)
    }
}

/// The package's guide, beside its index.
pub const GUIDE: &str = "GUIDE.md";

/// The surface of the package in `folder`, with `version` when it is
/// known; `None` when no package is there.
pub fn text(db: &dyn Db, project: Project, folder: &str, version: Option<&str>) -> Option<String> {
    Some(surface(db, project, folder)?.text(version))
}

/// Every package the project holds, by folder.
pub fn homes(db: &dyn Db, project: Project) -> BTreeMap<String, PackageHome> {
    let mut out = BTreeMap::new();
    for &file in project.files(db) {
        if let jadpo_db::Ownership::Package(home) = &jadpo_db::location(db, project, file).ownership
        {
            out.entry(home.folder.clone())
                .or_insert_with(|| home.clone());
        }
    }
    out
}

/// The surface of the package in `folder`.
pub fn surface(db: &dyn Db, project: Project, folder: &str) -> Option<Surface> {
    let home = homes(db, project).remove(folder)?;
    let prefix = format!("{folder}/");
    let files: Vec<jadpo_db::File> = project
        .files(db)
        .iter()
        .copied()
        .filter(|file| file.path(db).starts_with(&prefix))
        .collect();
    let data: Vec<&str> = project
        .records(db)
        .iter()
        .filter_map(|record| record.path.strip_prefix(&prefix))
        .collect();
    let file_at = |relative: &str| {
        files
            .iter()
            .copied()
            .find(|file| *file.path(db) == format!("{prefix}{relative}"))
    };
    let mut surface = Surface {
        package: home.versioned(),
        folder: folder.to_string(),
        kind: if file_at(jadpo_db::PROVIDER_FILE).is_some() {
            "provider"
        } else {
            "library"
        },
        installed: home.installed,
        description: None,
        licence: None,
        jadpo: None,
        guide: data.contains(&GUIDE),
        locales: crate::translations::packages::analyse_package(
            db,
            project,
            crate::lookups::Name::new(db, home.wording_key()),
        )
        .locales
        .iter()
        .filter_map(|&locale| jadpo_std::LOCALES.get(locale as usize))
        .map(|(variant, _)| (*variant).to_string())
        .collect(),
        provider: None,
        declarations: Vec::new(),
    };
    if let Some(index) = file_at(jadpo_db::PACKAGE_INDEX) {
        read_index(db, index, &mut surface);
    }
    if let Some(provider) = file_at(jadpo_db::PROVIDER_FILE) {
        surface.provider = read_provider(db, provider);
    }
    let graph = crate::graph::graph(db, project);
    let mut declarations = Vec::new();
    for &file in &files {
        let path = file.path(db);
        let relative = &path[prefix.len()..];
        if relative.ends_with(".test.jadpo") {
            continue;
        }
        if jadpo_db::location(db, project, file).place
            == jadpo_db::Place::Folder(jadpo_db::Folder::Resources)
        {
            operations(db, file, relative, &data, false, &mut declarations);
        }
    }
    let lines: BTreeMap<String, LineIndex> = files
        .iter()
        .map(|file| (file.path(db).clone(), LineIndex::new(file.text(db))))
        .collect();
    for node in graph.nodes() {
        let Some(place) = &node.place else { continue };
        let Some(relative) = place.file.strip_prefix(&prefix) else {
            continue;
        };
        if relative.ends_with(".test.jadpo") {
            continue;
        }
        let line = lines.get(&place.file).map(|index| index.line(place.start));
        // A package's library is known through its package
        // (`Postcodes.uk`); the surface names it as the app writes it
        // after the package's name.
        let own = format!("{}.", home.through);
        let name = node.name.strip_prefix(&own).unwrap_or(&node.name);
        let made = |what: What| Declaration {
            name: name.to_string(),
            description: node.doc.clone(),
            file: relative.to_string(),
            line,
            what,
        };
        match (&node.kind, &node.detail) {
            (
                Kind::Type,
                Detail::Type {
                    base,
                    rules,
                    failure,
                },
            ) if node.public => {
                if let Some(failure) = failure {
                    declarations.push(made(What::Failure {
                        kind: failure.kind.clone(),
                        code: failure.code.clone(),
                        message: failure.message.clone(),
                        fields: fields(graph, node),
                    }));
                    continue;
                }
                let base_text = match base {
                    Base::Object | Base::Input | Base::Output | Base::Enum => {
                        base.render(&|key| name_of(graph, key))
                    }
                    Base::Refines(parent) => parent.render(&|key| name_of(graph, key)),
                    // A library package's message is called through its
                    // `translations` (area 10), and named as the app calls
                    // it.
                    Base::Message if relative.starts_with("translations/") => {
                        declarations.push(Declaration {
                            name: format!(
                                "{}.{}",
                                crate::translations::TRANSLATIONS,
                                crate::case::to_snake(name)
                            ),
                            ..made(What::Message {
                                fields: fields(graph, node),
                            })
                        });
                        continue;
                    }
                    // A package's events come with chunk 7.6; emails and
                    // prompts are the app's own.
                    _ => continue,
                };
                let variants = if *base == Base::Enum {
                    variants(graph, node)
                } else {
                    Vec::new()
                };
                declarations.push(made(What::Type {
                    base: base_text,
                    rules: rule_pairs(rules),
                    fields: if matches!(base, Base::Refines(_)) {
                        Vec::new()
                    } else {
                        fields(graph, node)
                    },
                    variants,
                }));
            }
            // A provider package's public action (chunk 7.6).
            (Kind::Action, Detail::Callable { signature, .. }) if node.public => {
                let name = |key: &Key| name_of(graph, key);
                let mut fails: Vec<String> =
                    signature.fails.iter().map(|ty| ty.render(&name)).collect();
                fails.sort();
                fails.dedup();
                let mut after_effect: Vec<String> = crate::types::action_steps(db, project)
                    .get(&node.key)
                    .map(|steps| steps.after_effect.iter().map(name).collect())
                    .unwrap_or_default();
                after_effect.sort();
                after_effect.dedup();
                declarations.push(made(What::Action {
                    after_effect,
                    params: signature
                        .params
                        .iter()
                        .map(|(param, ty)| (param.clone(), ty.render(&name)))
                        .collect(),
                    result: (signature.result != TypeRef::Builtin(jadpo_std::Builtin::Unit))
                        .then(|| signature.result.render(&name)),
                    fails,
                }));
            }
            (Kind::Function, Detail::Callable { signature, .. }) => {
                let library_public = node
                    .parent
                    .as_ref()
                    .and_then(|parent| graph.get(parent))
                    .is_some_and(|library| library.kind == Kind::Library && library.public);
                if !library_public {
                    continue;
                }
                let name = |key: &Key| name_of(graph, key);
                let mut fails: Vec<String> =
                    signature.fails.iter().map(|ty| ty.render(&name)).collect();
                fails.sort();
                fails.dedup();
                declarations.push(made(What::Function {
                    params: signature
                        .params
                        .iter()
                        .map(|(param, ty)| (param.clone(), ty.render(&name)))
                        .collect(),
                    result: (signature.result != TypeRef::Builtin(jadpo_std::Builtin::Unit))
                        .then(|| signature.result.render(&name)),
                    fails,
                }));
            }
            _ => {}
        }
    }
    let mut recipes: Vec<&str> = data
        .iter()
        .filter_map(|path| {
            let rest = path.strip_prefix("recipes/")?;
            let (name, _) = rest.split_once('/')?;
            (!name.is_empty()).then_some(name)
        })
        .collect();
    recipes.dedup();
    for recipe in recipes {
        declarations.push(Declaration {
            name: recipe.to_string(),
            description: None,
            file: format!("recipes/{recipe}/"),
            line: None,
            what: What::Recipe,
        });
    }
    declarations.sort_by(|a, b| (a.what.section(), &a.name).cmp(&(b.what.section(), &b.name)));
    surface.declarations = declarations;
    Some(surface)
}

/// A key's name as the package writes it.
fn name_of(graph: &Graph, key: &Key) -> String {
    match graph.get(key) {
        Some(node) if matches!(node.kind, Kind::Type | Kind::Entity) => node.name.clone(),
        _ => key.written().to_string(),
    }
}

/// A rule's value as the surface writes it: a pattern as a Jadpo string.
pub(crate) fn rule_pairs(rules: &jadpo_std::Rules) -> Vec<(String, String)> {
    jadpo_graph::rule_pairs(rules)
        .into_iter()
        .map(|(name, value)| {
            let value = if name == "pattern" {
                format::quote(&value)
            } else {
                value
            };
            (name.to_string(), value)
        })
        .collect()
}

/// A field's type as the surface writes it: its declared type, its own
/// rules in braces, and a default.
fn field_type(graph: &Graph, field: &Node) -> Option<String> {
    let Detail::Field {
        ty, default, rules, ..
    } = &field.detail
    else {
        return None;
    };
    let name = |key: &Key| name_of(graph, key);
    let mut out = ty.render(&name);
    let pairs = rule_pairs(rules);
    if !pairs.is_empty() {
        let inner: Vec<String> = pairs.iter().map(|(n, v)| format!("{n}: {v}")).collect();
        let (base, nullable) = match out.strip_suffix('?') {
            Some(base) => (base.to_string(), "?"),
            None => (out.clone(), ""),
        };
        out = format!("{base} {{ {} }}{nullable}", inner.join(", "));
    }
    if let Some(default) = default {
        out.push_str(&format!(" default {}", default.render(&name)));
    }
    Some(out)
}

/// A type's fields, by name.
fn fields(graph: &Graph, node: &Node) -> Vec<(String, String)> {
    let Some(id) = graph.find(&node.key) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = graph
        .fields(id)
        .into_iter()
        .map(|field| graph.node(field))
        .filter_map(|field| {
            let name = field.name.rsplit('.').next().unwrap_or(&field.name);
            Some((name.to_string(), field_type(graph, field)?))
        })
        .collect();
    out.sort();
    out
}

/// An enum's variants, by name, each with its fields.
fn variants(graph: &Graph, node: &Node) -> Vec<(String, Vec<(String, String)>)> {
    let Some(id) = graph.find(&node.key) else {
        return Vec::new();
    };
    let mut out: Vec<(String, Vec<(String, String)>)> = graph
        .children(id)
        .map(|child| graph.node(child))
        .filter(|child| matches!(child.detail, Detail::Variant { .. }))
        .map(|variant| {
            let name = variant
                .name
                .rsplit('.')
                .next()
                .unwrap_or(&variant.name)
                .to_string();
            (name, fields(graph, variant))
        })
        .collect();
    out.sort();
    out
}

/// The index's description, licence and Jadpo version.
fn read_index(db: &dyn Db, file: jadpo_db::File, surface: &mut Surface) {
    let Some(source) = tree::source(db, file) else {
        return;
    };
    let Some(decl) = source.decls().find_map(|decl| match decl {
        Decl::PackageDecl(decl) => Some(decl),
        _ => None,
    }) else {
        return;
    };
    surface.description = doc(decl.syntax());
    for (name, member) in tree::members(decl.member_list()) {
        let value = member
            .value()
            .map(|value| tree::written(value.syntax()))
            .unwrap_or_default();
        match tree::name_text(&name).as_deref() {
            Some("licence") => {
                surface.licence = value.strip_prefix("Licence.").map(str::to_string);
            }
            Some("jadpo") => surface.jadpo = Some(unescape(&value)),
            _ => {}
        }
    }
}

/// `provider.jadpo`'s facts: its protocol, hosts or container, credential
/// scheme and idempotency key.
fn read_provider(db: &dyn Db, file: jadpo_db::File) -> Option<Provider> {
    let source = tree::source(db, file)?;
    let decl = source.decls().find_map(|decl| match decl {
        Decl::SingletonDecl(decl)
            if decl
                .keyword()
                .is_some_and(|keyword| keyword.text() == "provider") =>
        {
            Some(decl)
        }
        _ => None,
    })?;
    let index = LineIndex::new(file.text(db));
    let mut provider = Provider {
        description: doc(decl.syntax()),
        line: Some(index.line(u32::from(decl.syntax().text_range().start()))),
        facts: Vec::new(),
        erasure: false,
    };
    let value = |member: &ast::Member| {
        member
            .value()
            .map(|value| tree::written(value.syntax()))
            .unwrap_or_default()
    };
    let (mut protocol, mut hosts, mut runs, mut credential, mut idempotency) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (name, member) in tree::members(decl.member_list()) {
        match tree::name_text(&name).as_deref() {
            Some("protocol") => {
                let written = value(&member);
                let word = written.strip_prefix("Protocol.").unwrap_or(&written);
                protocol.push(format!("protocol {word}"));
            }
            Some("hosts") => {
                for (host, inner) in tree::members(member.block()) {
                    if let Some(host) = tree::name_text(&host) {
                        hosts.push(format!("host {host} {}", unescape(&value(&inner))));
                    }
                }
            }
            Some("runs") => {
                for (key, inner) in tree::members(member.block()) {
                    let Some(key) = tree::name_text(&key) else {
                        continue;
                    };
                    let written = value(&inner);
                    if key == "asks" {
                        let list = written.trim_start_matches('[').trim_end_matches(']');
                        for host in list.split(',').filter(|host| !host.is_empty()) {
                            runs.push(format!("runs asks {}", unescape(host)));
                        }
                    } else {
                        runs.push(format!("runs {key} {}", unescape(&written)));
                    }
                }
            }
            Some("erasure") => provider.erasure = true,
            Some("credential") => {
                let written = value(&member);
                let word = written
                    .strip_prefix("CredentialScheme.")
                    .unwrap_or(&written);
                credential.push(format!("credential {word}"));
            }
            Some("credential_header") => {
                credential.push(format!("credential header {}", unescape(&value(&member))));
            }
            Some("headers") => {
                let written = value(&member);
                let list = written.trim_start_matches('[').trim_end_matches(']');
                for line in list.split(',').filter(|line| !line.is_empty()) {
                    credential.push(format!("header {}", unescape(line)));
                }
            }
            Some("idempotency") => {
                for (key, inner) in tree::members(member.block()) {
                    match tree::name_text(&key).as_deref() {
                        Some("header") => idempotency
                            .push(format!("idempotency header {}", unescape(&value(&inner)))),
                        Some("window") => {
                            idempotency.push(format!("idempotency window {}", value(&inner)));
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    for group in [
        &mut protocol,
        &mut hosts,
        &mut runs,
        &mut credential,
        &mut idempotency,
    ] {
        group.sort();
        provider.facts.append(group);
    }
    Some(provider)
}

/// Every operation of the package in `folder`, those of a resource it keeps
/// to itself too, by name: what a release's facts cover (area 11, "What a
/// release shows"), which the surface doesn't list.
pub fn all_operations(db: &dyn Db, project: Project, folder: &str) -> Vec<Declaration> {
    let prefix = format!("{folder}/");
    let data: Vec<&str> = project
        .records(db)
        .iter()
        .filter_map(|record| record.path.strip_prefix(&prefix))
        .collect();
    let mut out = Vec::new();
    for &file in project.files(db) {
        let path = file.path(db);
        let Some(relative) = path.strip_prefix(&prefix) else {
            continue;
        };
        if !relative.ends_with(".test.jadpo")
            && jadpo_db::location(db, project, file).place
                == jadpo_db::Place::Folder(jadpo_db::Folder::Resources)
        {
            operations(db, file, relative, &data, true, &mut out);
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// A resource's operations, from its file: only a public resource's unless
/// `private` says otherwise.
fn operations(
    db: &dyn Db,
    file: jadpo_db::File,
    relative: &str,
    data: &[&str],
    private: bool,
    out: &mut Vec<Declaration>,
) {
    let Some(source) = tree::source(db, file) else {
        return;
    };
    let index = LineIndex::new(file.text(db));
    for decl in source.decls() {
        let Decl::ResourceDecl(resource) = decl else {
            continue;
        };
        if resource.visibility().is_none() && !private {
            continue;
        }
        let Some(resource_name) = resource.name().as_ref().and_then(tree::name_text) else {
            continue;
        };
        for (name, member) in tree::members(resource.member_list()) {
            let Some(operation) = tree::name_text(&name) else {
                continue;
            };
            let mut method = String::new();
            let mut path = String::new();
            let (mut input, mut output, mut output_at) = (None, None, None);
            let (mut wire, mut decode) = (None, None);
            let (mut changes, mut idempotent) = (false, false);
            let mut fails = Vec::new();
            let (mut how, mut docs) = (None, None);
            for (key, inner) in tree::members(member.block()) {
                let written = inner
                    .value()
                    .map(|value| tree::written(value.syntax()))
                    .unwrap_or_default();
                match tree::name_text(&key).as_deref() {
                    Some("method") => {
                        method = written
                            .strip_prefix("HttpMethod.")
                            .unwrap_or(&written)
                            .to_uppercase();
                    }
                    Some("path") => path = unescape(&written),
                    Some("input") => input = Some(written),
                    Some("output") => output = Some(written),
                    Some("wire_output") => wire = Some(written),
                    Some("decode") => decode = Some(written),
                    Some("output_at") => output_at = Some(unescape(&written)),
                    Some("changes") => changes = written == "true",
                    Some("idempotent") => idempotent = written == "true",
                    Some("failures") => {
                        let list = written.trim_start_matches('[').trim_end_matches(']');
                        fails.extend(
                            list.split(',')
                                .filter(|name| !name.is_empty())
                                .map(str::to_string),
                        );
                    }
                    Some("source") => {
                        how = Some(
                            written
                                .strip_prefix("PackageSource.")
                                .unwrap_or(&written)
                                .to_string(),
                        );
                    }
                    Some("docs") => docs = Some(unescape(&written)),
                    _ => {}
                }
            }
            fails.sort();
            fails.dedup();
            let full = format!("{resource_name}.{operation}");
            let recording = format!("recordings/{full}.json");
            out.push(Declaration {
                name: full,
                description: doc(member.syntax()),
                file: relative.to_string(),
                line: Some(index.line(u32::from(member.syntax().text_range().start()))),
                what: What::Operation {
                    method,
                    path,
                    input,
                    output,
                    wire_output: wire.zip(decode),
                    output_at,
                    changes,
                    idempotent,
                    fails,
                    source: how.map(|how| (how, docs)),
                    confirmed: data.contains(&recording.as_str()).then_some(recording),
                },
            });
        }
    }
}
