//! A release's fingerprints: every public declaration's normalised source,
//! so an advisory can name the packages carrying a copy (area 11, "Yanks,
//! takedowns and advisories"), and each public function's structure, so
//! `jadpo package search --from` and `--to` can filter by it (area 11,
//! "Search").
//!
//! - **The source fingerprint** is a digest of the declaration's tokens as
//!   the formatter writes them, leaving out its description, comments,
//!   visibility, `decline` lines and its own name, with a function's or
//!   action's parameters and variables named by position, as the
//!   similar-helper check names them. A type's fields stay as written:
//!   they are what it is.
//! - **The shape** is its inputs and output reduced to standard types and
//!   their constraints, its failures reduced to their kinds, and its
//!   effects. Names go, so the same structure under other names is the same
//!   shape; a type that holds itself ends in `…` where it comes round again.
//!
//! A recipe is copied whole into an app rather than called, and isn't part
//! of the compiled package, so it has no fingerprint.

use std::collections::BTreeMap;

use jadpo_db::{Db, Project};
use jadpo_graph::{Base, Detail, Graph, Key, Kind, TypeRef};
use jadpo_syntax::ast::{self, AstNode, Decl};
use sha2::{Digest, Sha256};

use crate::surface::{self, What, rule_pairs};
use crate::tree;

/// One public declaration's fingerprint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fingerprint {
    /// As the app writes it after the package's name.
    pub declaration: String,
    /// `function`, `type`.
    pub kind: &'static str,
    /// `sha256:…` of its normalised source.
    pub source: String,
    /// Its structure, for what is called: an operation, action or function.
    pub shape: Option<Shape>,
}

/// A function's structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shape {
    /// Each parameter's type reduced, in order and without its name.
    pub inputs: Vec<String>,
    pub output: Option<String>,
    /// Each failure's kind, sorted.
    pub failures: Vec<String>,
    /// `calls` when it calls out, and `changes` when that changes something.
    pub effects: Vec<&'static str>,
    /// `sha256:…` of the above, for an exact match.
    pub digest: String,
}

/// The fingerprint of every public declaration of the package in `folder`,
/// in the surface's order.
pub fn fingerprints(db: &dyn Db, project: Project, folder: &str) -> Vec<Fingerprint> {
    let Some(surface) = surface::surface(db, project, folder) else {
        return Vec::new();
    };
    let Some(home) = surface::homes(db, project).remove(folder) else {
        return Vec::new();
    };
    let graph = crate::graph::graph(db, project);
    let prefix = format!("{folder}/");
    let own = format!("{}.", home.through);
    // The package's types and callables, by the name the surface gives them.
    let mut types: BTreeMap<String, Key> = BTreeMap::new();
    let mut callables: BTreeMap<String, Key> = BTreeMap::new();
    for node in graph.nodes() {
        let in_package = node
            .place
            .as_ref()
            .is_some_and(|place| place.file.starts_with(&prefix));
        if !in_package {
            continue;
        }
        let name = node.name.strip_prefix(&own).unwrap_or(&node.name);
        match node.kind {
            Kind::Type => {
                types.insert(name.to_string(), node.key.clone());
            }
            Kind::Function | Kind::Action => {
                callables.insert(name.to_string(), node.key.clone());
            }
            _ => {}
        }
    }
    let sources = source_digests(db, project, folder);
    let reducer = Reducer {
        graph,
        types: &types,
    };
    let steps = crate::types::action_steps(db, project);
    surface
        .declarations
        .iter()
        .filter_map(|declaration| {
            let source = sources.get(&declaration.name)?.clone();
            let shape = match &declaration.what {
                What::Operation {
                    input,
                    output,
                    fails,
                    changes,
                    ..
                } => {
                    let mut effects = vec!["calls"];
                    if *changes {
                        effects.push("changes");
                    }
                    Some(Shape::new(
                        input.iter().map(|ty| reducer.written(ty)).collect(),
                        output.as_deref().map(|ty| reducer.written(ty)),
                        fails
                            .iter()
                            .map(|name| reducer.failure_named(name))
                            .collect(),
                        effects,
                    ))
                }
                What::Function { .. } | What::Action { .. } => callables
                    .get(&declaration.name)
                    .and_then(|key| Some((key, graph.get(key)?)))
                    .and_then(|(key, node)| {
                        let Detail::Callable { signature, .. } = &node.detail else {
                            return None;
                        };
                        let mut effects = Vec::new();
                        if let Some(steps) = steps.get(key) {
                            if steps.calls.is_some() {
                                effects.push("calls");
                            }
                            if steps.changing > 0 {
                                effects.push("changes");
                            }
                        }
                        let unit = TypeRef::Builtin(jadpo_std::Builtin::Unit);
                        Some(Shape::new(
                            signature
                                .params
                                .iter()
                                .map(|(_, ty)| reducer.reduce(ty))
                                .collect(),
                            (signature.result != unit).then(|| reducer.reduce(&signature.result)),
                            signature
                                .fails
                                .iter()
                                .map(|ty| reducer.failure(ty))
                                .collect(),
                            effects,
                        ))
                    }),
                _ => None,
            };
            Some(Fingerprint {
                declaration: declaration.name.clone(),
                kind: declaration.what.word(),
                source,
                shape,
            })
        })
        .collect()
}

impl Shape {
    fn new(
        inputs: Vec<String>,
        output: Option<String>,
        mut failures: Vec<String>,
        effects: Vec<&'static str>,
    ) -> Shape {
        failures.sort();
        failures.dedup();
        let text = format!(
            "({}) -> {} fails [{}] effects [{}]",
            inputs.join(", "),
            output.as_deref().unwrap_or(""),
            failures.join(", "),
            effects.join(", "),
        );
        Shape {
            inputs,
            output,
            failures,
            effects,
            digest: sha256(text.as_bytes()),
        }
    }
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("sha256:{hex}")
}

/// A declaration's normalised tokens, as a digest.
fn digest_of(node: &jadpo_syntax::SyntaxNode, own: Option<&ast::Name>, locals: bool) -> String {
    let tokens = crate::graph::similar::normalised(node, own.map(AstNode::syntax), locals);
    sha256(tokens.join("\0").as_bytes())
}

/// Every declaration's source digest in the package's own files, by the
/// name the surface gives it.
fn source_digests(db: &dyn Db, project: Project, folder: &str) -> BTreeMap<String, String> {
    let prefix = format!("{folder}/");
    let mut out = BTreeMap::new();
    for &file in project.files(db) {
        let path = file.path(db);
        let Some(relative) = path.strip_prefix(&prefix) else {
            continue;
        };
        if relative.ends_with(".test.jadpo") {
            continue;
        }
        let Some(source) = tree::source(db, file) else {
            continue;
        };
        for decl in source.decls() {
            match &decl {
                Decl::LibraryDecl(library) => {
                    let Some(library_name) = library.name().as_ref().and_then(tree::name_text)
                    else {
                        continue;
                    };
                    for member in tree::library_members(library) {
                        let Decl::FunctionDecl(function) = &member else {
                            continue;
                        };
                        let own = function.name();
                        if let Some(name) = own.as_ref().and_then(tree::name_text) {
                            out.insert(
                                format!("{library_name}.{name}"),
                                digest_of(function.syntax(), own.as_ref(), true),
                            );
                        }
                    }
                }
                Decl::TypeDecl(ty) => {
                    let own = ty.name();
                    if let Some(name) = own.as_ref().and_then(tree::name_text) {
                        // A public message is named as the app calls it.
                        let name = if relative.starts_with("translations/") {
                            format!(
                                "{}.{}",
                                crate::translations::TRANSLATIONS,
                                crate::case::to_snake(&name)
                            )
                        } else {
                            name
                        };
                        out.insert(name, digest_of(ty.syntax(), own.as_ref(), false));
                    }
                }
                Decl::ActionDecl(action) => {
                    let own = action.name();
                    if let Some(name) = own.as_ref().and_then(tree::name_text) {
                        out.insert(name, digest_of(action.syntax(), own.as_ref(), true));
                    }
                }
                Decl::ResourceDecl(resource) => {
                    let Some(resource_name) = resource.name().as_ref().and_then(tree::name_text)
                    else {
                        continue;
                    };
                    for (key, member) in tree::members(resource.member_list()) {
                        if let Some(operation) = tree::name_text(&key) {
                            out.insert(
                                format!("{resource_name}.{operation}"),
                                digest_of(member.syntax(), Some(&key), false),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// Reduces a package's types to standard types and their constraints.
struct Reducer<'a> {
    graph: &'a Graph,
    types: &'a BTreeMap<String, Key>,
}

/// A type's text so far, and the constraints it carries.
type Refined = (String, BTreeMap<String, String>);

impl Reducer<'_> {
    fn reduce(&self, ty: &TypeRef) -> String {
        shown(self.refined(ty, &mut Vec::new()))
    }

    /// A type as an operation's `input:` or `output:` writes it, which the
    /// surface keeps as written.
    fn written(&self, text: &str) -> String {
        if let Some(inner) = text.strip_suffix('?') {
            return format!("{}?", self.written(inner));
        }
        for wrapper in ["List", "Set"] {
            if let Some(inner) = text
                .strip_prefix(wrapper)
                .and_then(|rest| rest.strip_prefix('<'))
                .and_then(|rest| rest.strip_suffix('>'))
            {
                return format!("{wrapper}<{}>", self.written(inner));
            }
        }
        match self.types.get(text) {
            Some(key) => shown(self.node(key, &mut Vec::new())),
            None => text.to_string(),
        }
    }

    fn refined(&self, ty: &TypeRef, stack: &mut Vec<Key>) -> Refined {
        let whole = |text: String| (text, BTreeMap::new());
        match ty {
            TypeRef::Node(key) => self.node(key, stack),
            TypeRef::Field(key) => self.field(key, stack),
            TypeRef::Nullable(inner) => whole(format!("{}?", shown(self.refined(inner, stack)))),
            TypeRef::List(inner) => whole(format!("List<{}>", shown(self.refined(inner, stack)))),
            TypeRef::Set(inner) => whole(format!("Set<{}>", shown(self.refined(inner, stack)))),
            TypeRef::Map(key, value) => {
                let key = shown(self.refined(key, stack));
                let value = shown(self.refined(value, stack));
                whole(format!("Map<{key}, {value}>"))
            }
            TypeRef::Std(name, args) if !args.is_empty() => {
                let args: Vec<String> = args
                    .iter()
                    .map(|arg| shown(self.refined(arg, stack)))
                    .collect();
                whole(format!("{name}<{}>", args.join(", ")))
            }
            other => whole(other.render(&|key| key.written().to_string())),
        }
    }

    fn node(&self, key: &Key, stack: &mut Vec<Key>) -> Refined {
        let Some(node) = self.graph.get(key) else {
            return (key.written().to_string(), BTreeMap::new());
        };
        if stack.contains(key) {
            return ("…".to_string(), BTreeMap::new());
        }
        let Detail::Type { base, rules, .. } = &node.detail else {
            return (node.name.clone(), BTreeMap::new());
        };
        stack.push(key.clone());
        let out = match base {
            Base::Refines(parent) => {
                let (text, mut constraints) = self.refined(parent, stack);
                constraints.extend(rule_pairs(rules));
                (text, constraints)
            }
            Base::Object | Base::Input | Base::Output => (self.fields(key, stack), BTreeMap::new()),
            Base::Enum => (self.variants(key, stack), BTreeMap::new()),
            _ => (node.name.clone(), BTreeMap::new()),
        };
        stack.pop();
        out
    }

    fn field(&self, key: &Key, stack: &mut Vec<Key>) -> Refined {
        let Some(node) = self.graph.get(key) else {
            return (key.written().to_string(), BTreeMap::new());
        };
        let Detail::Field { ty, rules, .. } = &node.detail else {
            return (node.name.clone(), BTreeMap::new());
        };
        // A field's own rules go inside its `?`, as the surface writes them.
        let (inner, nullable) = match ty {
            TypeRef::Nullable(inner) => (inner.as_ref(), "?"),
            other => (other, ""),
        };
        let (text, mut constraints) = self.refined(inner, stack);
        constraints.extend(rule_pairs(rules));
        (
            format!("{}{nullable}", shown((text, constraints))),
            BTreeMap::new(),
        )
    }

    /// `{ note: Text?, outward: Text { max_length: 4 } }`, by field name.
    fn fields(&self, key: &Key, stack: &mut Vec<Key>) -> String {
        let Some(id) = self.graph.find(key) else {
            return "{}".to_string();
        };
        let mut fields: Vec<(String, String)> = self
            .graph
            .fields(id)
            .into_iter()
            .map(|field| self.graph.node(field))
            .map(|field| {
                let name = field.name.rsplit('.').next().unwrap_or(&field.name);
                (name.to_string(), shown(self.field(&field.key, stack)))
            })
            .collect();
        fields.sort();
        if fields.is_empty() {
            return "{}".to_string();
        }
        let inner: Vec<String> = fields
            .iter()
            .map(|(name, ty)| format!("{name}: {ty}"))
            .collect();
        format!("{{ {} }}", inner.join(", "))
    }

    /// `Enum { england, wales }`, a variant's fields after its name.
    fn variants(&self, key: &Key, stack: &mut Vec<Key>) -> String {
        let Some(id) = self.graph.find(key) else {
            return "Enum".to_string();
        };
        let mut variants: Vec<String> = self
            .graph
            .children(id)
            .map(|child| self.graph.node(child))
            .filter(|child| matches!(child.detail, Detail::Variant { .. }))
            .map(|variant| {
                let name = variant.name.rsplit('.').next().unwrap_or(&variant.name);
                let has_fields = self
                    .graph
                    .find(&variant.key)
                    .is_some_and(|id| !self.graph.fields(id).is_empty());
                if has_fields {
                    format!("{name} {}", self.fields(&variant.key, stack))
                } else {
                    name.to_string()
                }
            })
            .collect();
        variants.sort();
        format!("Enum {{ {} }}", variants.join(", "))
    }

    /// A failure's kind.
    fn failure(&self, ty: &TypeRef) -> String {
        match ty {
            TypeRef::Node(key) => self.failure_of(key),
            other => other.render(&|key| key.written().to_string()),
        }
    }

    fn failure_named(&self, name: &str) -> String {
        match self.types.get(name) {
            Some(key) => self.failure_of(key),
            None => name.to_string(),
        }
    }

    fn failure_of(&self, key: &Key) -> String {
        match self.graph.get(key).map(|node| &node.detail) {
            Some(Detail::Type {
                failure: Some(failure),
                ..
            }) => failure
                .kind
                .clone()
                .unwrap_or_else(|| "failure".to_string()),
            _ => "failure".to_string(),
        }
    }
}

/// A type's text with its constraints sorted by name.
fn shown((text, constraints): Refined) -> String {
    if constraints.is_empty() {
        return text;
    }
    let inner: Vec<String> = constraints
        .iter()
        .map(|(name, value)| format!("{name}: {value}"))
        .collect();
    format!("{text} {{ {} }}", inner.join(", "))
}
