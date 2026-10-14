//! The examples cover every built feature (docs/compiler-runtime.md §2.7;
//! decision "Examples teach every feature", 2026-10-14).
//!
//! Each project in `examples/` (a folder holding `app.jadpo`) is loaded
//! and checked as `jadpo check` does. From its parse trees, its graph and
//! its callables' typed bodies, this test lists the features it uses, and
//! fails naming every built feature that no example uses.
//!
//! The list of built features comes from the compiler's own tables
//! wherever one exists, so a feature added to a table demands an example:
//! the grammar's node kinds, the graph's node and edge kinds and their
//! status, the type bases, the constraints, input field settings, failure
//! members and declarable kinds, the copying lines, the standard library's
//! operation table and the enums its operations take, the generated
//! namespaces' built members, and the layout's root files and folders.
//! The typed body's statements, expressions, callees, patterns and locals
//! are listed by exhaustive matches below, so a new variant doesn't
//! compile until it is named here.
//!
//! A feature that can't appear in valid, runnable code yet is exempt, with
//! its reason, in [`EXEMPT`]. An exemption that names no feature, or a
//! feature an example now uses, fails the test, so the list only shrinks
//! as later chunks build what it names.
//!
//! `JADPO_SHOW_COVERAGE=1 cargo test -p jadpo-check --test examples`
//! prints every built feature with the examples that use it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use jadpo_check::graph::graph;
use jadpo_check::types::{
    BASES, CONSTRAINTS, COPY_LINES, FIELD_SETTINGS, callables_of, typed_body,
};
use jadpo_check::{DECLARABLE_KINDS, FAILURE_MEMBERS};
use jadpo_db::{Folder, Layout, Place, RootFile, locate, parse};
use jadpo_graph::body::{
    ArmBody, BinaryOp, Body, Callee, Expr, Fallible, Literal, LocalKind, Node, Pattern, Stmt,
    UnaryOp,
};
use jadpo_graph::{Base, Detail, EdgeKind, Kind, Status, TypeRef};
use jadpo_std::{Builtin, OPERATIONS, Rules};
use jadpo_syntax::SyntaxKind;
use jadpo_syntax::ast::{self, AstNode};

/// The areas built so far (roadmap Phases 1 and 2). A graph kind these
/// areas decide counts once nodes of it are declared.
const BUILT_AREAS: [u8; 5] = [1, 2, 3, 4, 13];

/// Features no valid, runnable example can use yet, and why.
const EXEMPT: &[(&str, &str)] = &[
    // Documentation and templates: never in a `.jadpo` source file.
    ("syntax SNIPPET", "documentation snippets only"),
    (
        "syntax ELISION",
        "`...` marks code left out of documentation",
    ),
    (
        "syntax ELISION_EXPR",
        "`...` marks code left out of documentation",
    ),
    ("syntax TEMPLATE", "not built: email templates (area 16)"),
    (
        "syntax TEMPLATE_BODY",
        "not built: email templates (area 16)",
    ),
    (
        "syntax TEMPLATE_LITERAL",
        "not built: email templates (area 16)",
    ),
    (
        "syntax TEMPLATE_VALUE",
        "not built: email templates (area 16)",
    ),
    ("syntax TEMPLATE_IF", "not built: email templates (area 16)"),
    (
        "syntax TEMPLATE_IF_TAG",
        "not built: email templates (area 16)",
    ),
    (
        "syntax TEMPLATE_ELSE",
        "not built: email templates (area 16)",
    ),
    (
        "syntax TEMPLATE_ELSE_TAG",
        "not built: email templates (area 16)",
    ),
    (
        "syntax TEMPLATE_FOR",
        "not built: email templates (area 16)",
    ),
    (
        "syntax TEMPLATE_FOR_TAG",
        "not built: email templates (area 16)",
    ),
    (
        "syntax TEMPLATE_END_TAG",
        "not built: email templates (area 16)",
    ),
    (
        "syntax TEMPLATE_INCLUDE",
        "not built: email templates (area 16)",
    ),
    // Later areas' declarations and the syntax only they use.
    ("syntax ENTITY_DECL", "not built: entities (area 5)"),
    ("syntax SEED_DECL", "not built: seeds (area 5)"),
    ("syntax QUERY_DECL", "not built: queries (area 6)"),
    ("syntax ORDERED_EXPR", "not built: a query's order (area 6)"),
    ("syntax ROUTES_DECL", "not built: routes (area 7)"),
    ("syntax ROUTE_ENTRY", "not built: routes (area 7)"),
    ("syntax ROUTE_PATH", "not built: routes (area 7)"),
    ("syntax ROUTE_SEGMENT", "not built: routes (area 7)"),
    ("syntax PATH_PARAM", "not built: routes (area 7)"),
    ("syntax ROUTE_PATH_EXPR", "not built: routes (area 7)"),
    (
        "syntax PACKAGE_DECL",
        "not built: provider packages (area 11)",
    ),
    (
        "syntax RESOURCE_DECL",
        "not built: provider packages (area 11)",
    ),
    (
        "syntax PROVIDE_DECL",
        "not built: provider packages (area 11)",
    ),
    ("syntax SCHEDULE_DECL", "not built: schedules (area 12)"),
    ("syntax SUBSCRIBER_DECL", "not built: subscribers (area 12)"),
    ("syntax EMIT_STMT", "not built: events (area 12)"),
    // Later areas' graph kinds and edges are reserved or declared only
    // for their own areas, so they aren't listed as built here.
    // Bases later areas define.
    ("base Event", "not built: events (area 12)"),
    ("base EmailMessage", "not built: emails (area 16)"),
    // Typed body parts later areas build.
    ("body stmt emit", "not built: events (area 12)"),
    ("body call query", "not built: queries (area 6)"),
    (
        "body call entity operation",
        "not built: entity operations (area 6)",
    ),
    (
        "body operator in",
        "a query's condition (area 6); area 3's operators don't include it",
    ),
    (
        "body operator matches",
        "a query's condition (area 6); area 3's operators don't include it",
    ),
    (
        "body fallible unknown",
        "an entity operation's failures (area 6), or a mistake already reported",
    ),
    // Generated namespaces' members that check but don't run yet.
    (
        "generated clock.now",
        "checks, but the interpreter can't read the clock yet (area 10): a test reaching it faults with not_built_yet",
    ),
    // Layout places later areas fill.
    (
        "root file config.jadpo",
        "not built: configuration (area 10)",
    ),
    (
        "root file authentication.jadpo",
        "not built: sign-in (area 8)",
    ),
    ("root file policy.jadpo", "not built: policy (area 9)"),
    ("root file flags.jadpo", "not built: flags (area 16)"),
    ("domain folder entities", "not built: entities (area 5)"),
    ("domain folder queries", "not built: queries (area 6)"),
    ("domain folder routes", "not built: routes (area 7)"),
    ("domain folder schedules", "not built: schedules (area 12)"),
    ("domain folder events", "not built: events (area 12)"),
    (
        "domain folder subscribers",
        "not built: subscribers (area 12)",
    ),
    ("domain folder emails", "not built: emails (area 16)"),
    ("place monitoring", "not built: monitoring (area 16)"),
    ("place seeds", "not built: seeds (area 5)"),
    ("place migration", "not built: migrations (area 5)"),
    ("place providers", "not built: provider packages (area 11)"),
    ("place acceptances", "not built: acceptances (area 18)"),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Each example project: a folder under `examples/` holding `app.jadpo`.
/// `golden-todo/` holds only the golden app's contract and notes.
fn projects() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(root().join("examples"))
        .expect("examples/ exists")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.join("app.jadpo").is_file())
        .collect();
    out.sort();
    out
}

// --- every built feature ----------------------------------------------------

/// Every feature the compiler builds today, from its own tables.
fn built() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    // The grammar's node kinds, and the two kinds of comment.
    let first_node = SyntaxKind::SOURCE_FILE as u16;
    for raw in first_node..u16::MAX {
        let kind = SyntaxKind::from_raw(raw);
        if kind as u16 != raw {
            break;
        }
        out.insert(format!("syntax {kind:?}"));
    }
    out.insert(format!("syntax {:?}", SyntaxKind::COMMENT));
    out.insert(format!("syntax {:?}", SyntaxKind::DOC_COMMENT));
    // The graph's kinds: built ones, and those the built areas declare.
    for kind in Kind::ALL {
        let declared = kind.status() == Status::Declared && BUILT_AREAS.contains(&kind.area());
        if kind.status() == Status::Built || declared {
            out.insert(format!("graph node {}", kind.word()));
        }
    }
    for edge in EdgeKind::ALL {
        if edge.status() == Status::Built {
            out.insert(format!("graph edge {}", edge.word()));
        }
    }
    // Declarations and their settings.
    out.extend(BASES.iter().map(|base| format!("base {base}")));
    out.insert("base refinement of a named type".to_string());
    out.extend(Builtin::ALL.iter().map(|b| format!("builtin {}", b.name())));
    out.extend(
        ["list", "set", "map", "nullable", "field type"]
            .iter()
            .map(|shape| format!("type shape {shape}")),
    );
    out.extend(CONSTRAINTS.iter().map(|rule| format!("rule {rule}")));
    out.extend(FIELD_SETTINGS.iter().map(|s| format!("field setting {s}")));
    out.extend(
        ["default literal", "default variant"]
            .iter()
            .map(|s| format!("field setting {s}")),
    );
    out.extend(
        FAILURE_MEMBERS
            .iter()
            .map(|m| format!("failure member {m}")),
    );
    out.extend(DECLARABLE_KINDS.iter().map(|k| format!("failure kind {k}")));
    out.extend(COPY_LINES.iter().map(|line| format!("copy line {line}")));
    out.insert("copy line written field replaces a copied one".to_string());
    // The standard library: every operation, every value of the enums its
    // operations take, and the generated namespaces' built members.
    for operation in OPERATIONS {
        out.insert(format!("std {}", operation.qualified()));
        for param in operation.params {
            if let jadpo_std::Kind::Enum(name) = param.kind {
                let values = jadpo_std::enums()
                    .into_iter()
                    .find(|(enum_name, _)| *enum_name == name)
                    .map(|(_, values)| values)
                    .unwrap_or_default();
                for value in values {
                    out.insert(format!("std {name}.{value}"));
                }
            }
        }
    }
    out.extend(generated_members());
    // The typed body.
    out.extend(BODY.iter().map(|feature| format!("body {feature}")));
    // The layout.
    out.extend(
        RootFile::ALL
            .iter()
            .map(|file| format!("root file {}", file.file_name())),
    );
    out.extend(
        Folder::ALL
            .iter()
            .map(|folder| format!("domain folder {}", folder.name())),
    );
    out.extend(
        Folder::SHARED
            .iter()
            .map(|folder| format!("shared folder {}", folder.name())),
    );
    out.extend(PLACES.iter().map(|place| format!("place {place}")));
    out
}

/// The members of the generated namespaces built so far, from the
/// standard library's names list (`clock: now`).
fn generated_members() -> Vec<String> {
    let names = std::fs::read_to_string(root().join("crates/jadpo-check/std/names.txt"))
        .expect("names.txt");
    let mut out = Vec::new();
    let mut section = "";
    for line in names.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            section = line;
            continue;
        }
        if section != "[generated]" {
            continue;
        }
        if let Some((namespace, members)) = line.split_once(':') {
            for member in members.split_whitespace() {
                out.push(format!("generated {}.{member}", namespace.trim()));
            }
        }
    }
    out
}

/// Every typed-body feature [`body_features`] can name.
const BODY: &[&str] = &[
    "stmt var",
    "stmt var mut",
    "stmt assign",
    "stmt return value",
    "stmt return bare",
    "stmt if",
    "stmt if with else if",
    "stmt if with else",
    "stmt for",
    "stmt emit",
    "stmt assert",
    "stmt assert fails",
    "stmt expression",
    "literal whole number",
    "literal negative number",
    "literal decimal",
    "literal duration",
    "literal text",
    "literal bool",
    "literal none",
    "expr local",
    "expr field",
    "call function",
    "call action",
    "call query",
    "call fixture",
    "call library function",
    "call std",
    "call std with a named argument",
    "call construct",
    "call construct field type",
    "call entity operation",
    "build object",
    "build variant with fields",
    "build plain variant",
    "expr list",
    "operator or",
    "operator and",
    "operator ==",
    "operator !=",
    "operator in",
    "operator matches",
    "operator <",
    "operator <=",
    "operator >",
    "operator >=",
    "operator +",
    "operator -",
    "operator *",
    "operator /",
    "operator %",
    "operator not",
    "operator negate",
    "match value",
    "match outcome",
    "attempt call",
    "attempt construction with invalid",
    "attempt construction with invalid and fields",
    "reject with fields",
    "reject bare",
    "propagate",
    "inline function",
    "fallible fails",
    "fallible construction",
    "fallible unknown",
    "pattern variant",
    "pattern variant with binding",
    "pattern true",
    "pattern false",
    "pattern some",
    "pattern none",
    "pattern literal",
    "pattern wildcard",
    "pattern success",
    "pattern failure",
    "arm value",
    "arm block",
    "arm empty block",
    "local parameter",
    "local loop item",
    "local binding",
    "local inline parameter",
];

/// Every layout place [`place`] can name.
const PLACES: &[&str] = &[
    "domain folder",
    "test beside its subject",
    "tests",
    "monitoring",
    "seeds",
    "migration",
    "providers",
    "acceptances",
];

// --- what the examples use --------------------------------------------------

fn place(place: &Place) -> Option<&'static str> {
    match place {
        Place::Folder(_) => Some("domain folder"),
        Place::Tests => Some("tests"),
        Place::Monitoring => Some("monitoring"),
        Place::Seeds => Some("seeds"),
        Place::Migration => Some("migration"),
        Place::Providers => Some("providers"),
        Place::Acceptances => Some("acceptances"),
        // A root file is counted by name; loose and stray files aren't
        // features (an example with one fails to check).
        Place::RootFile(_) | Place::Loose | Place::Stray(_) => None,
    }
}

fn shapes(ty: &TypeRef, used: &mut BTreeSet<String>) {
    let mut add = |name: &str| {
        used.insert(format!("type shape {name}"));
    };
    match ty {
        TypeRef::Builtin(builtin) => {
            used.insert(format!("builtin {}", builtin.name()));
        }
        TypeRef::Field(_) => add("field type"),
        TypeRef::List(inner) => {
            add("list");
            shapes(inner, used);
        }
        TypeRef::Set(inner) => {
            add("set");
            shapes(inner, used);
        }
        TypeRef::Map(key, value) => {
            add("map");
            shapes(key, used);
            shapes(value, used);
        }
        TypeRef::Nullable(inner) => {
            add("nullable");
            shapes(inner, used);
        }
        TypeRef::Std(_, args) => args.iter().for_each(|arg| shapes(arg, used)),
        // A declared type, a payload binding's data, `none` and a mistake
        // already reported are no shape of their own.
        TypeRef::Node(_) | TypeRef::Variant(_) | TypeRef::None | TypeRef::Written(_) => {}
    }
}

fn rules(rules: &Rules, used: &mut BTreeSet<String>) {
    let Rules {
        min,
        max,
        min_length,
        max_length,
        pattern,
        format,
        scale,
        version,
        min_duration,
        max_duration,
    } = rules;
    let set = [
        ("min", min.is_some() || min_duration.is_some()),
        ("max", max.is_some() || max_duration.is_some()),
        ("min_length", min_length.is_some()),
        ("max_length", max_length.is_some()),
        ("pattern", pattern.is_some()),
        ("format", format.is_some()),
        ("scale", scale.is_some()),
        ("version", version.is_some()),
    ];
    for (rule, present) in set {
        if present {
            used.insert(format!("rule {rule}"));
        }
    }
}

fn literal(value: &Literal) -> &'static str {
    match value {
        Literal::Number { decimal: true, .. } => "literal decimal",
        Literal::Number { value, .. } if value.compare(jadpo_std::Num::whole(0)).is_lt() => {
            "literal negative number"
        }
        Literal::Number { .. } => "literal whole number",
        Literal::Duration(_) => "literal duration",
        Literal::Text(_) => "literal text",
        Literal::Bool(_) => "literal bool",
        Literal::None => "literal none",
    }
}

fn binary(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Or => "operator or",
        BinaryOp::And => "operator and",
        BinaryOp::Equal => "operator ==",
        BinaryOp::NotEqual => "operator !=",
        BinaryOp::In => "operator in",
        BinaryOp::Matches => "operator matches",
        BinaryOp::Less => "operator <",
        BinaryOp::LessEqual => "operator <=",
        BinaryOp::Greater => "operator >",
        BinaryOp::GreaterEqual => "operator >=",
        BinaryOp::Add => "operator +",
        BinaryOp::Subtract => "operator -",
        BinaryOp::Multiply => "operator *",
        BinaryOp::Divide => "operator /",
        BinaryOp::Remainder => "operator %",
    }
}

fn fallible(fallible: &Fallible) -> Option<&'static str> {
    match fallible {
        Fallible::Never => None,
        Fallible::Fails(_) => Some("fallible fails"),
        Fallible::Construction { .. } => Some("fallible construction"),
        Fallible::Unknown => Some("fallible unknown"),
    }
}

/// What one typed body uses, by the names in [`BODY`].
fn body_features(body: &Body, out: &mut BTreeSet<&'static str>, std: &mut BTreeSet<String>) {
    for local in &body.locals {
        out.insert(match local.kind {
            LocalKind::Param => "local parameter",
            LocalKind::Var => "stmt var",
            LocalKind::VarMut => "stmt var mut",
            LocalKind::Item => "local loop item",
            LocalKind::Binding => "local binding",
            LocalKind::Inline => "local inline parameter",
        });
    }
    for site in &body.sites {
        match &site.node {
            Node::Stmt(stmt) => match stmt {
                // A `var`'s kind is counted from its local, above.
                Stmt::Var { .. } => {}
                Stmt::Assign { .. } => {
                    out.insert("stmt assign");
                }
                Stmt::Return { value } => {
                    out.insert(if value.is_some() {
                        "stmt return value"
                    } else {
                        "stmt return bare"
                    });
                }
                Stmt::If {
                    arms, otherwise, ..
                } => {
                    out.insert("stmt if");
                    if arms.len() > 1 {
                        out.insert("stmt if with else if");
                    }
                    if !otherwise.is_empty() {
                        out.insert("stmt if with else");
                    }
                }
                Stmt::For { .. } => {
                    out.insert("stmt for");
                }
                Stmt::Emit { .. } => {
                    out.insert("stmt emit");
                }
                Stmt::Assert { fails, .. } => {
                    out.insert(if fails.is_some() {
                        "stmt assert fails"
                    } else {
                        "stmt assert"
                    });
                }
                Stmt::Expr { .. } => {
                    out.insert("stmt expression");
                }
            },
            Node::Expr(expr) => match expr {
                Expr::Literal(value) => {
                    out.insert(literal(value));
                }
                Expr::Local(_) => {
                    out.insert("expr local");
                }
                Expr::Field { .. } => {
                    out.insert("expr field");
                }
                Expr::Call {
                    callee,
                    fallible: can_fail,
                    ..
                } => {
                    if let Some(name) = fallible(can_fail) {
                        out.insert(name);
                    }
                    let name = match callee {
                        Callee::Function(_) => "call function",
                        Callee::Action(_) => "call action",
                        Callee::Query(_) => "call query",
                        Callee::Fixture(_) => "call fixture",
                        Callee::LibraryFunction(_) => "call library function",
                        Callee::Std(op) => {
                            std.insert(format!("std {}", op.operation().qualified()));
                            "call std"
                        }
                        Callee::Construct(TypeRef::Field(_)) => "call construct field type",
                        Callee::Construct(_) => "call construct",
                        Callee::Entity { .. } => "call entity operation",
                        // Objects and variants are built, below; an
                        // unknown callee is a mistake already reported.
                        Callee::Object(_) | Callee::Variant(_) | Callee::Unknown => continue,
                    };
                    out.insert(name);
                }
                Expr::Build {
                    callee,
                    fields,
                    fallible: can_fail,
                } => {
                    if let Some(name) = fallible(can_fail) {
                        out.insert(name);
                    }
                    match callee {
                        Callee::Object(_) => {
                            out.insert("build object");
                        }
                        Callee::Variant(key) => {
                            // A standard-library enum's value, such as
                            // `Rounding.up`, is a plain variant too.
                            let owner = key.name().split('.').next().unwrap_or("");
                            if jadpo_std::enums().iter().any(|(name, _)| *name == owner) {
                                std.insert(format!("std {}", key.name()));
                            }
                            out.insert(if fields.is_empty() {
                                "build plain variant"
                            } else {
                                "build variant with fields"
                            });
                        }
                        _ => {}
                    }
                }
                Expr::List { .. } => {
                    out.insert("expr list");
                }
                Expr::Binary { op, .. } => {
                    out.insert(binary(*op));
                }
                Expr::Unary { op, .. } => {
                    out.insert(match op {
                        UnaryOp::Not => "operator not",
                        UnaryOp::Negate => "operator negate",
                    });
                }
                Expr::Match { outcome, arms, .. } => {
                    out.insert(if *outcome {
                        "match outcome"
                    } else {
                        "match value"
                    });
                    for arm in arms {
                        out.insert(match &arm.pattern {
                            Pattern::Variant(_) if arm.binding.is_some() => {
                                "pattern variant with binding"
                            }
                            Pattern::Variant(_) => "pattern variant",
                            Pattern::True => "pattern true",
                            Pattern::False => "pattern false",
                            Pattern::Some => "pattern some",
                            Pattern::None => "pattern none",
                            Pattern::Literal(_) => "pattern literal",
                            Pattern::Wildcard => "pattern wildcard",
                            Pattern::Success => "pattern success",
                            Pattern::Failure(_) => "pattern failure",
                            Pattern::Unknown => continue,
                        });
                        out.insert(match &arm.body {
                            ArmBody::Value(_) => "arm value",
                            ArmBody::Block(block) if block.is_empty() => "arm empty block",
                            ArmBody::Block(_) => "arm block",
                            ArmBody::Empty => "arm empty block",
                        });
                    }
                }
                Expr::Attempt {
                    invalid, fields, ..
                } => {
                    out.insert(if invalid.is_some() {
                        "attempt construction with invalid"
                    } else {
                        "attempt call"
                    });
                    if !fields.is_empty() {
                        out.insert("attempt construction with invalid and fields");
                    }
                }
                Expr::Reject { fields, .. } => {
                    out.insert(if fields.is_empty() {
                        "reject bare"
                    } else {
                        "reject with fields"
                    });
                }
                Expr::Propagate { .. } => {
                    out.insert("propagate");
                }
                Expr::Inline { .. } => {
                    out.insert("inline function");
                }
                Expr::Unchecked => {}
            },
        }
    }
}

/// The settings written in a type's block: the failure members and the
/// copying lines, which the graph records only by their effect.
fn written_settings(source: &ast::SourceFile, used: &mut BTreeSet<String>) {
    for decl in source
        .syntax()
        .descendants()
        .filter_map(ast::TypeDecl::cast)
    {
        let base = decl
            .base()
            .map(|base| base.syntax().text().to_string())
            .unwrap_or_default();
        let Some(members) = decl.member_list() else {
            continue;
        };
        for member in members.syntax().children().filter_map(ast::Member::cast) {
            let Some(key) = member.member_key().map(|k| k.syntax().text().to_string()) else {
                continue;
            };
            if base == "Failure" && FAILURE_MEMBERS.contains(&key.as_str()) {
                used.insert(format!("failure member {key}"));
            }
            if base != "Failure" && COPY_LINES.contains(&key.as_str()) {
                used.insert(format!("copy line {key}"));
            }
        }
    }
}

/// Every feature one example project uses.
fn used_by(project_dir: &Path) -> BTreeSet<String> {
    let loaded = jadpo_db::load(project_dir);
    let (db, project) = (&loaded.db, loaded.project);
    let problems = jadpo_check::check(db, project);
    assert!(
        problems.is_empty(),
        "{} doesn't check cleanly: {:?}",
        project_dir.display(),
        problems
            .iter()
            .map(|d| d.rule.id.to_string())
            .collect::<Vec<_>>()
    );
    let mut used = BTreeSet::new();
    let mut body = BTreeSet::new();
    for &file in project.files(db) {
        let path = file.path(db);
        let tree = parse(db, file);
        let syntax = tree.syntax();
        for element in syntax.descendants_with_tokens() {
            let kind = element.kind();
            if element.as_node().is_some()
                || matches!(kind, SyntaxKind::COMMENT | SyntaxKind::DOC_COMMENT)
            {
                used.insert(format!("syntax {kind:?}"));
            }
        }
        if let Some(source) = ast::SourceFile::cast(syntax) {
            written_settings(&source, &mut used);
        }
        let location = locate(path, Layout::Project);
        if let Place::RootFile(file) = location.place {
            used.insert(format!("root file {}", file.file_name()));
        }
        if let Some(name) = place(&location.place) {
            used.insert(format!("place {name}"));
        }
        if let Some(folder) = location.folder() {
            let shared = location.ownership == jadpo_db::Ownership::Shared;
            let side = if shared { "shared" } else { "domain" };
            used.insert(format!("{side} folder {}", folder.name()));
        }
        if location.test && location.folder().is_some() {
            used.insert("place test beside its subject".to_string());
        }
        for callable in callables_of(db, file) {
            let checked = typed_body(db, project, callable);
            body_features(&checked.body, &mut body, &mut used);
            named_arguments(&checked.body, file.text(db), &mut body);
        }
    }
    used.extend(body.into_iter().map(|feature| format!("body {feature}")));
    let graph = graph(db, project);
    for node in graph.nodes() {
        used.insert(format!("graph node {}", node.kind.word()));
        match &node.detail {
            Detail::Type {
                base,
                rules: own,
                failure,
            } => {
                match base {
                    Base::Refines(TypeRef::Node(_)) => {
                        used.insert("base refinement of a named type".to_string());
                    }
                    Base::Refines(parent) => shapes(parent, &mut used),
                    other => {
                        used.insert(format!("base {}", other.render(&|key| key.to_string())));
                    }
                }
                rules(own, &mut used);
                if let Some(kind) = failure.as_ref().and_then(|f| f.kind.as_ref()) {
                    used.insert(format!("failure kind {kind}"));
                }
            }
            Detail::Field {
                ty,
                optional,
                default,
                replaces,
                rules: own,
                ..
            } => {
                shapes(ty, &mut used);
                rules(own, &mut used);
                if *optional {
                    used.insert("field setting optional".to_string());
                }
                if let Some(default) = default {
                    used.insert("field setting default".to_string());
                    used.insert(match default {
                        jadpo_graph::FieldDefault::Literal(_) => {
                            "field setting default literal".to_string()
                        }
                        jadpo_graph::FieldDefault::Variant(_) => {
                            "field setting default variant".to_string()
                        }
                    });
                }
                if replaces.is_some() {
                    used.insert("copy line written field replaces a copied one".to_string());
                }
            }
            Detail::Callable(signature) => {
                for (_, ty) in &signature.params {
                    shapes(ty, &mut used);
                }
                shapes(&signature.result, &mut used);
            }
            Detail::Variant { .. } | Detail::None => {}
        }
    }
    for edge in graph.edges() {
        used.insert(format!("graph edge {}", edge.kind.word()));
    }
    // A generated member is used where a body names it, as written.
    for member in generated_members() {
        let written = member.trim_start_matches("generated ");
        if project
            .files(db)
            .iter()
            .any(|file| file.text(db).contains(written))
        {
            used.insert(member);
        }
    }
    used
}

/// Whether a standard-library call in `body` names an argument
/// (`rounding: Rounding.up`), read from the call's source text.
fn named_arguments(body: &Body, text: &str, out: &mut BTreeSet<&'static str>) {
    for site in &body.sites {
        if let Node::Expr(Expr::Call {
            callee: Callee::Std(op),
            ..
        }) = &site.node
        {
            let written = &text[site.start as usize..site.end as usize];
            if op
                .operation()
                .params
                .iter()
                .any(|param| written.contains(&format!("{}:", param.name)))
            {
                out.insert("call std with a named argument");
            }
        }
    }
}

#[test]
fn the_examples_use_every_built_feature() {
    let projects = projects();
    assert!(projects.len() >= 8, "examples found: {projects:?}");
    let mut used: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for project in &projects {
        let name = project
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        for feature in used_by(project) {
            used.entry(feature).or_default().push(name.clone());
        }
    }
    let built = built();
    if std::env::var_os("JADPO_SHOW_COVERAGE").is_some() {
        for feature in &built {
            let by = used
                .get(feature)
                .map_or_else(|| "-".to_string(), |by| by.join(" "));
            eprintln!("{feature:<50} {by}");
        }
    }
    let exempt: BTreeMap<&str, &str> = EXEMPT.iter().copied().collect();
    let mut problems = Vec::new();
    for feature in &built {
        match (used.get(feature), exempt.get(feature.as_str())) {
            (None, None) => problems.push(format!(
                "{feature}: no example uses it; add it to an example in examples/, or exempt it in crates/jadpo-check/tests/examples.rs with the reason it can't run yet"
            )),
            (Some(by), Some(_)) => problems.push(format!(
                "{feature}: exempt, but {} now uses it; remove the exemption",
                by.join(", ")
            )),
            _ => {}
        }
    }
    for feature in exempt.keys() {
        if !built.contains(*feature) {
            problems.push(format!(
                "{feature}: exempt, but it isn't a built feature; remove the exemption"
            ));
        }
    }
    for feature in used.keys() {
        if !built.contains(feature) && !feature.starts_with("graph ") {
            problems.push(format!(
                "{feature}: an example uses it, but the built features don't list it; add it to the list in crates/jadpo-check/tests/examples.rs"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "{} problem(s) with the examples' coverage:\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
}
