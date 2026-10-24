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
//! status, the isolation levels an action runs at, the type bases, the constraints, input field settings, failure
//! members and declarable kinds, the copying lines, the standard library's
//! operation table and the enums its operations take, the generated
//! namespaces' built members, the values areas 7 to 9's settings take,
//! and the layout's root files and folders.
//! The typed body's statements, expressions, callees, patterns and locals
//! are listed by exhaustive matches below, so a new variant doesn't
//! compile until it is named here.
//!
//! A feature that can't appear in valid, runnable code yet is exempt, with
//! its reason, in [`EXEMPT`]. An exemption that names no feature, or a
//! feature an example now uses, fails the test, so the list only shrinks
//! as later chunks build what it names.
//!
//! The examples are brought up to date once per phase, by an examples chunk
//! at its end (Richard, 2026-10-15), so a built feature no example uses yet
//! is only noted (`note:` lines, which the gate shows) unless
//! `JADPO_REQUIRE_EXAMPLES=1` is set (`tools/verify.py --require-examples`),
//! which fails on it. The other problems below fail either way: they are
//! this test's own lists falling out of step with the compiler's.
//!
//! A closed list a standard operation takes or gives counts as one
//! feature, `closed list Locale`, used when an example names any of its
//! values (`Locale.en_gb`) anywhere in its files: `Locale`, `Zone`,
//! `Country` and `Currency`, and each kind's unit enum (`LengthUnit`),
//! which `units.of`, `units.in` and `text.format` take. Their values are
//! data held to their source tables (CLDR, the IANA zones, ISO 3166 and
//! ISO 4217, the units table) by the standard library's own tests, so an
//! example teaches passing the list, not each value (decided 2026-10-19,
//! chunk 5.7d). Every other enum's values still count one by one.
//!
//! `JADPO_SHOW_COVERAGE=1 cargo test -p jadpo-check --test integration examples::`
//! prints every built feature with the examples that use it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use jadpo_check::data::lifecycle::SECTIONS as LIFECYCLE_SECTIONS;
use jadpo_check::data::migrations::DecisionValue;
use jadpo_check::data::queries::{Condition, Include, SECTIONS};
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
use jadpo_std::batch::BatchOp;
use jadpo_std::write::WriteOp;
use jadpo_std::{Builtin, FieldChange, Generated, OPERATIONS, OnDelete, Rules};
use jadpo_syntax::SyntaxKind;
use jadpo_syntax::ast::{self, AstNode};

/// The areas built so far (roadmap Phases 1 to 6). A graph kind these
/// areas decide counts once nodes of it are declared.
const BUILT_AREAS: [u8; 11] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 13];

/// Features no valid, runnable example can use yet, and why.
const EXEMPT: &[(&str, &str)] = &[
    // Documentation only.
    ("syntax SNIPPET", "documentation snippets only"),
    (
        "syntax ELISION",
        "`...` marks code left out of documentation",
    ),
    (
        "syntax ELISION_EXPR",
        "`...` marks code left out of documentation",
    ),
    // Later areas' declarations and the syntax only they use.
    ("syntax SEED_DECL", "not built: seeds (area 5)"),
    (
        "graph node seed",
        "not built: `jadpo seed`, which runs a seed's actions (area 5)",
    ),
    (
        "data migration decision FieldChange.dropped",
        "not built: a drop needs a person's acceptance (area 18)",
    ),
    // Later areas' graph kinds and edges are reserved or declared only
    // for their own areas, so they aren't listed as built here.
    // Bases later areas define.
    // Typed body parts later areas build.
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
        "a mistake already reported, which no example has",
    ),
    // Values of the settings areas 7 to 9 read that no example can write
    // yet, or never needs to.
    (
        "setting SuccessStatus.ok",
        "the status a route answers when it says none",
    ),
    (
        "setting Idempotency.optional",
        "what an API does when it says nothing",
    ),
    (
        "setting AuthenticationMethod.external_token",
        "provisioned external-provider authentication; teaching examples are updated in the phase 9 examples chunk (area 8)",
    ),
    (
        "setting AuthenticationMethod.oauth_token",
        "not built: acting as an OAuth provider, after the first build (area 8)",
    ),
    // Layout places later areas fill.
    ("root file flags.jadpo", "not built: flags (area 16)"),
    ("place monitoring", "not built: monitoring (area 16)"),
    ("place seeds", "not built: seeds (area 5)"),
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
    // operations take or give, and the generated namespaces' built members.
    // An enum an operation gives counts too (`locale.direction`'s
    // `TextDirection`): a program builds or matches its values. A closed
    // list counts once, as the list ([`closed_lists`]).
    for operation in OPERATIONS {
        out.insert(format!("std {}", operation.qualified()));
        let result = std::iter::once(operation.result);
        for kind in operation
            .params
            .iter()
            .map(|param| param.kind)
            .chain(result)
        {
            if kind == jadpo_std::Kind::QuantityUnit {
                out.extend(
                    jadpo_std::QuantityKind::ALL
                        .iter()
                        .map(|kind| format!("closed list {}", kind.unit_enum())),
                );
            }
            if let jadpo_std::Kind::Enum(name) = kind
                && closed_lists().iter().any(|(list, _)| *list == name)
            {
                out.insert(format!("closed list {name}"));
            } else if let jadpo_std::Kind::Enum(name) = kind {
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
        Folder::DOMAIN
            .iter()
            .map(|folder| format!("domain folder {}", folder.name())),
    );
    out.extend(
        Folder::PACKAGE
            .iter()
            .map(|folder| format!("package folder {}", folder.name())),
    );
    out.extend(
        Folder::SHARED
            .iter()
            .map(|folder| format!("shared folder {}", folder.name())),
    );
    out.extend(PLACES.iter().map(|place| format!("place {place}")));
    // Area 5: the field settings' values and a migration's decisions.
    out.extend(
        Generated::NAMES
            .iter()
            .map(|v| format!("data generated {v}")),
    );
    out.extend(
        OnDelete::NAMES
            .iter()
            .map(|v| format!("data on_delete {v}")),
    );
    out.insert("data field unique".to_string());
    // Area 5's lifecycle: its sections, a guarded transition and a
    // partial index.
    out.extend(
        LIFECYCLE_SECTIONS
            .iter()
            .map(|section| format!("data lifecycle {section}")),
    );
    out.insert("data lifecycle transition when".to_string());
    out.insert("data lifecycle partial index".to_string());
    // Area 6: a query's sections, how many records it returns, its
    // includes and the shapes of its conditions.
    out.extend(
        SECTIONS
            .iter()
            .map(|section| format!("query section {section}")),
    );
    out.extend(
        ["one", "optional", "list", "page"]
            .iter()
            .map(|c| format!("query result {c}")),
    );
    out.extend(
        ["parent", "children", "child", "nested"]
            .iter()
            .map(|k| format!("query include {k}")),
    );
    out.extend(
        [
            "comparison",
            "in",
            "none",
            "and",
            "or",
            "not",
            "present",
            "distance",
            "geo",
            // A standard operation with an exact SQL form (chunk 6.3b).
            "operation",
        ]
        .iter()
        .map(|c| format!("query condition {c}")),
    );
    // Area 21: a nearest-neighbour query (chunk 5.5v1b).
    out.insert("query result nearest".to_string());
    // Area 6: an aggregate query's aggregates and time buckets (chunk
    // 5.6c).
    out.extend(
        ["count", "sum", "min", "max", "average"]
            .iter()
            .map(|a| format!("query aggregate {a}")),
    );
    out.insert("query group bucket".to_string());
    // Area 6's isolation levels (chunk 3.7), each on an action that writes.
    out.extend(
        jadpo_std::Isolation::ALL
            .iter()
            .map(|level| format!("isolation {}", level.word())),
    );
    // Areas 7 to 9: the values a route, an API, a grant or a membership
    // names, and the methods a route test sends.
    out.extend(SETTINGS.iter().flat_map(|(name, values)| {
        values
            .iter()
            .map(move |value| format!("setting {name}.{value}"))
    }));
    out.insert("data migration decision backfill".to_string());
    // Area 12: what a subscriber and a schedule hold, a subscriber limited
    // to its event's tenant, the outbox's store, the event record, a
    // route that waits for follow-on work, a batch limited to a tenant
    // (chunks 6.1b to 6.4c), an entity's built-in purge schedule and a
    // schedule test, which `--kind schedule` runs (chunk 6.2c).
    out.extend(
        jadpo_check::events::SUBSCRIBER_ITEMS
            .iter()
            .map(|item| format!("subscriber item {item}")),
    );
    out.extend(
        jadpo_check::schedules::SCHEDULE_ITEMS
            .iter()
            .map(|item| format!("schedule item {item}")),
    );
    out.extend(AREA_12.iter().map(|feature| feature.to_string()));
    out.extend(
        FieldChange::NAMES
            .iter()
            .map(|v| format!("data migration decision FieldChange.{v}")),
    );
    out
}

/// Area 12's features that no table lists: each is named where
/// [`used_by`] finds it.
const AREA_12: &[&str] = &[
    "subscriber limited to its event's tenant",
    "events store",
    "record events",
    "route settle",
    "batch limited to a tenant",
    "purge schedule",
    "schedule test kind",
];

/// The enums whose values a route, an API, `authentication`, a grant or a
/// membership names (areas 7 to 9), or a route test's `method:` takes.
const SETTINGS: [(&str, &[&str]); 9] = [
    (
        jadpo_std::SuccessStatus::NAME,
        jadpo_std::SuccessStatus::NAMES,
    ),
    (jadpo_std::HttpMethod::NAME, jadpo_std::HttpMethod::NAMES),
    (jadpo_std::Idempotency::NAME, jadpo_std::Idempotency::NAMES),
    (
        jadpo_std::RouteAuthentication::NAME,
        jadpo_std::RouteAuthentication::NAMES,
    ),
    (
        jadpo_std::AuthenticationMethod::NAME,
        jadpo_std::AuthenticationMethod::NAMES,
    ),
    (jadpo_std::Access::NAME, jadpo_std::Access::NAMES),
    (jadpo_std::Tenancy::NAME, jadpo_std::Tenancy::NAMES),
    (jadpo_std::FieldAccess::NAME, jadpo_std::FieldAccess::NAMES),
    (
        jadpo_std::MembershipScope::NAME,
        jadpo_std::MembershipScope::NAMES,
    ),
];

/// The closed lists, each with its values: the enums whose values are an
/// outside authority's table, which the standard library's tests hold
/// them to, so a feature is the list, not each value (chunk 5.7d).
fn closed_lists() -> Vec<(String, Vec<String>)> {
    let tables = [
        jadpo_std::LOCALE,
        jadpo_std::ZONE,
        jadpo_std::standard::places::COUNTRY,
        jadpo_std::CURRENCY,
    ];
    let enums = jadpo_std::enums();
    let mut out: Vec<(String, Vec<String>)> = tables
        .iter()
        .map(|name| {
            let values = enums
                .iter()
                .find(|(enum_name, _)| enum_name == name)
                .map(|(_, values)| values.iter().map(|v| v.to_string()).collect())
                .unwrap_or_default();
            (name.to_string(), values)
        })
        .collect();
    out.extend(jadpo_std::QuantityKind::ALL.iter().map(|kind| {
        let values = kind
            .units()
            .iter()
            .map(|unit| unit.name.to_string())
            .collect();
        (kind.unit_enum().to_string(), values)
    }));
    for (name, values) in &out {
        assert!(!values.is_empty(), "closed list {name} has no values");
    }
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
    "literal quantity",
    "literal percentage",
    "literal text",
    "literal bool",
    "literal none",
    "expr local",
    "expr field",
    "expr request",
    "expr setting",
    "expr constant",
    "call function",
    "call action",
    "call query",
    "call fixture",
    "call library function",
    "call std",
    "call std with a named argument",
    "call construct",
    "call construct field type",
    "call construct named collection",
    "call construct named set from a list",
    "call entity operation",
    "call caller from a record",
    "call session",
    "call batch each",
    "call batch each with finished",
    "call batch each with key",
    "call batch status",
    "call batch cancel",
    "call batch read a file's rows",
    "call batch write rows into a file",
    "call files put",
    "call files read",
    "call provider operation",
    "call package action",
    "script a provider's fake",
    "include an email partial",
    "read the emails a test sent",
    "advance the test clock",
    "run a schedule in a test",
    "call translation",
    "render a prompt",
    // Area 21's `ai` calls and their fake (chunk 7.8c).
    "ask a model",
    "stream a model's answer",
    "ask a model for embeddings",
    "script a model's answers",
    "read what a model was asked",
    "write create",
    "write update",
    "write update by id",
    "write update with patch",
    "write update with when_supplied",
    "write delete",
    "write delete by id",
    "write transition",
    "write transition by id",
    "write delete runs a transition",
    "write transition conflict arm",
    "write conflict arm",
    "write conflict arm with fields",
    "expr stored",
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
    "attempt construction with rule arms",
    "attempt construction with invalid alone",
    "attempt construction with rule arms and invalid",
    "attempt construction arm with fields",
    "reject with fields",
    "reject bare",
    "propagate",
    "inline function",
    "fallible fails",
    "fallible construction",
    "fallible unknown",
    "construction proven by a condition",
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
    "package index",
    "provider",
    "installed package",
    "own integration",
    "acceptances",
    "root translations",
];

// --- what the examples use --------------------------------------------------

fn place(place: &Place) -> Option<&'static str> {
    match place {
        Place::Folder(_) => Some("domain folder"),
        Place::Tests => Some("tests"),
        Place::Monitoring => Some("monitoring"),
        Place::Seeds => Some("seeds"),
        Place::Migration => Some("migration"),
        Place::PackageIndex => Some("package index"),
        Place::Provider => Some("provider"),
        Place::Acceptances => Some("acceptances"),
        Place::RootTranslations => Some("root translations"),
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
        currency,
        rounding,
        rate_from,
        rate_to,
        dimensions,
        distance,
        // Set only by the standard library, never written in a block.
        standard: _,
        double: _,
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
        ("currency", currency.is_some()),
        ("rounding", rounding.is_some()),
        ("from", rate_from.is_some()),
        ("to", rate_to.is_some()),
        ("dimensions", dimensions.is_some()),
        ("distance", distance.is_some()),
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
        Literal::Quantity(_) => "literal quantity",
        Literal::Percentage(_) => "literal percentage",
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

/// What one typed body uses, by the names in [`BODY`]. `constructed`
/// gathers each construction's type and its value's type, which only the
/// graph can say are collections.
fn body_features(
    body: &Body,
    out: &mut BTreeSet<&'static str>,
    std: &mut BTreeSet<String>,
    constructed: &mut Vec<(TypeRef, Option<TypeRef>)>,
) {
    if !body.proofs.is_empty() {
        out.insert("construction proven by a condition");
    }
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
                Expr::Request { .. } => {
                    out.insert("expr request");
                }
                Expr::Config { .. } => {
                    out.insert("expr setting");
                }
                Expr::Call {
                    callee,
                    args,
                    fallible: can_fail,
                } => {
                    if let Some(name) = fallible(can_fail) {
                        out.insert(name);
                    }
                    if let Callee::Construct(ty) = callee {
                        let value = args
                            .first()
                            .copied()
                            .flatten()
                            .and_then(|id| body.sites.get(id.0 as usize))
                            .map(|site| site.ty.clone());
                        constructed.push((ty.clone(), value));
                    }
                    let name = match callee {
                        // A template's `{{ include }}` (chunk 7.3c).
                        Callee::Function(key)
                            if key.name().starts_with(jadpo_check::emails::PARTIAL_LABEL) =>
                        {
                            "include an email partial"
                        }
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
                        Callee::Caller(_) => "call caller from a record",
                        Callee::Session(_) => "call session",
                        Callee::Batch(op) => match op {
                            BatchOp::Each => {
                                // `each`'s arguments: `from:`, `each:`,
                                // `finished:` and `key:`, in that order.
                                if args.get(2).is_some_and(Option::is_some) {
                                    out.insert("call batch each with finished");
                                }
                                if args.get(3).is_some_and(Option::is_some) {
                                    out.insert("call batch each with key");
                                }
                                "call batch each"
                            }
                            BatchOp::Status => "call batch status",
                            BatchOp::Cancel => "call batch cancel",
                            // The file helpers share one machinery, so
                            // one format's read, or write, teaches them all.
                            BatchOp::Read(_) => "call batch read a file's rows",
                            BatchOp::Write(_) => "call batch write rows into a file",
                        },
                        Callee::Files { op, .. } => match op {
                            jadpo_std::files::FilesOp::Put => "call files put",
                            jadpo_std::files::FilesOp::Read => "call files read",
                        },
                        Callee::Provider { .. } => "call provider operation",
                        Callee::PackageAction { .. } => "call package action",
                        Callee::Fake { .. } => "script a provider's fake",
                        Callee::AdvanceClock => "advance the test clock",
                        Callee::Schedule { .. } => "run a schedule in a test",
                        Callee::Translation(_) => "call translation",
                        Callee::Prompt(_) => "render a prompt",
                        Callee::Generate { .. } => "ask a model",
                        Callee::Stream { .. } => "stream a model's answer",
                        Callee::Embed { .. } => "ask a model for embeddings",
                        Callee::AiFake { .. } => "script a model's answers",
                        Callee::AiRequests { .. } => "read what a model was asked",
                        // What an HTML template's value is written out
                        // through, which lowering makes, never source.
                        Callee::Escape => continue,
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
                        // Matching a standard-library enum's value, such as
                        // `PasswordCheck.correct`, uses it as building one does.
                        if let Pattern::Variant(key) = &arm.pattern {
                            let owner = key.name().split('.').next().unwrap_or("");
                            if jadpo_std::enums().iter().any(|(name, _)| *name == owner) {
                                std.insert(format!("std {}", key.name()));
                            }
                        }
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
                Expr::Attempt { arms, .. } => {
                    let rules = arms.iter().any(|arm| arm.rule.is_some());
                    let invalid = arms.iter().any(|arm| arm.rule.is_none());
                    out.insert(match (rules, invalid) {
                        (false, false) => "attempt call",
                        (true, false) => "attempt construction with rule arms",
                        (false, true) => "attempt construction with invalid alone",
                        (true, true) => "attempt construction with rule arms and invalid",
                    });
                    if arms.iter().any(|arm| !arm.fields.is_empty()) {
                        out.insert("attempt construction arm with fields");
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
                Expr::Write {
                    op,
                    transition,
                    by_id,
                    patch,
                    when_supplied,
                    conflicts,
                    fallible: can_fail,
                    ..
                } => {
                    if let Some(name) = fallible(can_fail) {
                        out.insert(name);
                    }
                    out.insert(match (op, by_id, transition.is_some()) {
                        (WriteOp::Create, _, _) => "write create",
                        (WriteOp::Delete, _, true) => "write delete runs a transition",
                        (_, false, true) => "write transition",
                        (_, true, true) => "write transition by id",
                        (WriteOp::Update, false, false) => "write update",
                        (WriteOp::Update, true, false) => "write update by id",
                        (WriteOp::Delete, false, false) => "write delete",
                        (WriteOp::Delete, true, false) => "write delete by id",
                    });
                    if conflicts
                        .iter()
                        .any(|arm| arm.rule.kind() == Some(Kind::Transition))
                    {
                        out.insert("write transition conflict arm");
                    }
                    if patch.is_some() {
                        out.insert("write update with patch");
                    }
                    if !when_supplied.is_empty() {
                        out.insert("write update with when_supplied");
                    }
                    if !conflicts.is_empty() {
                        out.insert("write conflict arm");
                    }
                    if conflicts.iter().any(|arm| !arm.fields.is_empty()) {
                        out.insert("write conflict arm with fields");
                    }
                }
                Expr::Stored { .. } => {
                    out.insert("expr stored");
                }
                Expr::Constant { .. } => {
                    out.insert("expr constant");
                }
                Expr::SentEmails => {
                    out.insert("read the emails a test sent");
                }
                Expr::Now | Expr::Unchecked => {}
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

/// The list, set or map `ty` is, or the named type or field type `ty` is
/// built on.
fn collection<'g>(graph: &'g jadpo_graph::Graph, ty: &'g TypeRef) -> Option<&'g TypeRef> {
    let mut current = ty;
    for _ in 0..64 {
        current = match current {
            TypeRef::List(_) | TypeRef::Set(_) | TypeRef::Map(..) => return Some(current),
            TypeRef::Nullable(inner) => inner,
            TypeRef::Node(key) | TypeRef::Field(key) => match &graph.get(key)?.detail {
                Detail::Type {
                    base: Base::Refines(parent),
                    ..
                } => parent,
                Detail::Field { ty, .. } => ty,
                _ => return None,
            },
            _ => return None,
        };
    }
    None
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
    let mut constructed = Vec::new();
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
            let side = match &location.ownership {
                jadpo_db::Ownership::Shared => "shared",
                jadpo_db::Ownership::Package(_) => "package",
                _ => "domain",
            };
            used.insert(format!("{side} folder {}", folder.name()));
        }
        if let Some(home) = location.ownership.package() {
            let which = if home.installed {
                "installed package"
            } else {
                "own integration"
            };
            used.insert(format!("place {which}"));
        }
        if location.test && location.folder().is_some() {
            used.insert("place test beside its subject".to_string());
        }
        for callable in callables_of(db, file) {
            let checked = typed_body(db, project, callable);
            body_features(&checked.body, &mut body, &mut used, &mut constructed);
            named_arguments(&checked.body, file.text(db), &mut body);
        }
    }
    // Each email template's syntax and typed body (area 16, chunk 7.3).
    for path in jadpo_check::emails::template_paths(db, project) {
        let Some(text) = project
            .records(db)
            .iter()
            .find(|record| record.path == path)
            .and_then(|record| record.text.as_ref().ok())
        else {
            continue;
        };
        for element in jadpo_syntax::parse_template(text)
            .syntax()
            .descendants_with_tokens()
        {
            if element.as_node().is_some() {
                used.insert(format!("syntax {:?}", element.kind()));
            }
        }
        let checked = jadpo_check::types::template_body(
            db,
            project,
            jadpo_check::lookups::Name::new(db, path.clone()),
        );
        body_features(&checked.body, &mut body, &mut used, &mut constructed);
    }
    let graph = graph(db, project);
    for (ty, value) in &constructed {
        let into = collection(graph, ty);
        if into.is_some() {
            body.insert("call construct named collection");
        }
        let from = value.as_ref().and_then(|value| collection(graph, value));
        if matches!(into, Some(TypeRef::Set(_))) && matches!(from, Some(TypeRef::List(_))) {
            body.insert("call construct named set from a list");
        }
    }
    used.extend(body.into_iter().map(|feature| format!("body {feature}")));
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
                    // A declared time format: the graph has no base of its
                    // own for `TimeFormat`, so keeps it as written.
                    Base::Refines(TypeRef::Written(written))
                        if BASES.contains(&written.as_str()) =>
                    {
                        used.insert(format!("base {written}"));
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
                header,
                column,
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
                if header.is_some() {
                    used.insert("field setting header".to_string());
                }
                if column.is_some() {
                    used.insert("field setting column".to_string());
                }
            }
            Detail::Callable { signature, .. } => {
                for (_, ty) in &signature.params {
                    shapes(ty, &mut used);
                }
                shapes(&signature.result, &mut used);
            }
            Detail::Constant { ty, .. } => shapes(ty, &mut used),
            Detail::Variant { .. }
            | Detail::Rule { .. }
            | Detail::None
            | Detail::Entity { .. }
            | Detail::Columns { .. }
            | Detail::Reference { .. }
            | Detail::Placement { .. }
            | Detail::SystemTable { .. }
            | Detail::Lifecycle { .. }
            | Detail::Transition { .. }
            | Detail::Grant { .. }
            | Detail::FieldRestriction { .. }
            | Detail::Tenancy { .. }
            | Detail::ActionException { .. }
            | Detail::Prompt { .. }
            | Detail::Api { .. } => {}
            Detail::Route { input, output, .. } => {
                for ty in input.iter().chain(output) {
                    shapes(ty, &mut used);
                }
            }
        }
    }
    // Area 10's settings: no graph node holds a setting's type, so a key
    // or an address type a setting declares is read from `config { }`.
    for setting in &jadpo_check::config::config(db, project).settings {
        if let jadpo_check::types::Ty::Prim(builtin) = &setting.ty {
            used.insert(format!("builtin {}", builtin.name()));
        }
    }
    // Area 5's settings and decisions, as the checker read them.
    let model = jadpo_check::data::data_model(db, project);
    for entity in &model.entities {
        for field in &entity.fields {
            if let Some(generated) = field.generated {
                used.insert(format!("data generated {}", generated.name()));
            }
            if field.unique {
                used.insert("data field unique".to_string());
            }
            if let Some(on_delete) = field.reference.as_ref().and_then(|r| r.on_delete) {
                used.insert(format!("data on_delete {}", on_delete.name()));
            }
        }
        if let Some(lifecycle) = &entity.lifecycle {
            if lifecycle.visible.is_some() {
                used.insert("data lifecycle visible".to_string());
            }
            if !lifecycle.initial.is_empty() {
                used.insert("data lifecycle initial".to_string());
            }
            if !lifecycle.transitions.is_empty() {
                used.insert("data lifecycle transitions".to_string());
            }
            if lifecycle.transitions.iter().any(|t| t.when.is_some()) {
                used.insert("data lifecycle transition when".to_string());
            }
            if lifecycle.purge.is_some() {
                used.insert("data lifecycle purge".to_string());
            }
        }
    }
    let schema = jadpo_check::data::schema(db, project);
    if schema.stores.values().any(|store| {
        store.tables.values().any(|table| {
            table
                .indexes
                .values()
                .any(|index| index.predicate.is_some())
        })
    }) {
        used.insert("data lifecycle partial index".to_string());
    }
    // Area 6's queries, as the checker read them.
    let queries = jadpo_check::data::queries::query_models(db, project);
    for query in &queries.queries {
        used.insert("query section from".to_string());
        used.insert(format!("query result {}", query.cardinality.word()));
        if query
            .filter
            .iter()
            .any(|c| !matches!(c, Condition::Present { .. }))
        {
            used.insert("query section where".to_string());
        }
        if query.order.iter().any(|o| !o.added)
            || query.nearest.is_some()
            || query.distance.is_some()
        {
            used.insert("query section order".to_string());
        }
        if query.nearest.is_some() {
            used.insert("query result nearest".to_string());
        }
        if query.limit.is_some() {
            let section = if query.after.is_some() {
                "page"
            } else {
                "limit"
            };
            used.insert(format!("query section {section}"));
        }
        if !query.includes.is_empty() {
            used.insert("query section include".to_string());
        }
        if query.missing.is_some() {
            used.insert("query section missing".to_string());
        }
        // A map layer's `geometry:` (chunk 5.5u8).
        if query.geometry.is_some() {
            used.insert("query section geometry".to_string());
        }
        fn conditions(all: &[Condition], used: &mut BTreeSet<String>) {
            for condition in all {
                let word = match condition {
                    Condition::Compare { .. } => "comparison",
                    Condition::In { .. } => "in",
                    Condition::IsNone { .. } => "none",
                    Condition::And(_) => "and",
                    Condition::Or(_) => "or",
                    Condition::Not(_) => "not",
                    Condition::Present { .. } => {
                        used.insert("query section where_present".to_string());
                        "present"
                    }
                    Condition::Call { call, .. } => match call.op {
                        jadpo_std::Op::AiDistance => "distance",
                        jadpo_std::Op::GeoWithin
                        | jadpo_std::Op::GeoContains
                        | jadpo_std::Op::GeoIntersects => "geo",
                        _ => "operation",
                    },
                };
                used.insert(format!("query condition {word}"));
                match condition {
                    Condition::And(inner) | Condition::Or(inner) => conditions(inner, used),
                    Condition::Not(inner)
                    | Condition::Present {
                        condition: inner, ..
                    } => conditions(std::slice::from_ref(inner), used),
                    _ => {}
                }
            }
        }
        conditions(&query.filter, &mut used);
        fn includes(all: &[Include], level: usize, used: &mut BTreeSet<String>) {
            for include in all {
                used.insert(format!("query include {}", include.kind.word()));
                if level > 1 {
                    used.insert("query include nested".to_string());
                }
                includes(&include.includes, level + 1, used);
            }
        }
        includes(&query.includes, 1, &mut used);
        // An aggregate query's groups and aggregates (chunk 5.6c).
        if let Some(grouping) = &query.grouping {
            use jadpo_check::data::queries::{Entry, GroupKey};
            if grouping.order.iter().any(|o| !o.added) {
                used.insert("query section order".to_string());
            }
            for (_, entry) in &grouping.entries {
                match entry {
                    Entry::Group(key) => {
                        used.insert("query section group".to_string());
                        if matches!(key, GroupKey::Bucket { .. }) {
                            used.insert("query group bucket".to_string());
                        }
                    }
                    Entry::Aggregate { kind, .. } => {
                        used.insert("query section aggregate".to_string());
                        used.insert(format!("query aggregate {}", kind.word()));
                    }
                }
            }
        }
    }
    for &file in project.files(db) {
        let path = file.path(db);
        if !path.ends_with("/changes.jadpo") {
            continue;
        }
        let Some(source) = ast::SourceFile::cast(parse(db, file).syntax()) else {
            continue;
        };
        let (decisions, _) = jadpo_check::data::migrations::read_decisions(path, &source);
        for decision in decisions {
            used.insert(match decision.value {
                DecisionValue::Backfill(..) => "data migration decision backfill".to_string(),
                DecisionValue::Change(change) => {
                    format!("data migration decision FieldChange.{}", change.name())
                }
                DecisionValue::Invalid(..) | DecisionValue::Pending => continue,
            });
        }
    }
    for edge in graph.edges() {
        used.insert(format!("graph edge {}", edge.kind.word()));
    }
    // Area 12: what each subscriber and schedule holds, as read.
    for subscriber in jadpo_check::events::subscribers(db, project).iter() {
        used.insert("subscriber item on".to_string());
        used.insert("subscriber item run".to_string());
        if subscriber.retry != jadpo_check::events::DEFAULT_RETRY {
            used.insert("subscriber item retry".to_string());
        }
        if matches!(subscriber.tenant, jadpo_check::events::Tenant::Field(_)) {
            used.insert("subscriber limited to its event's tenant".to_string());
        }
    }
    // A declared schedule holds its items; an entity's built-in purge
    // schedule is written as `purge:` (chunk 6.2c).
    for schedule in jadpo_check::schedules::schedules(db, project).iter() {
        match schedule.run {
            jadpo_check::schedules::ScheduleRun::Action(_) => {
                used.insert("schedule item every".to_string());
                used.insert("schedule item run".to_string());
            }
            jadpo_check::schedules::ScheduleRun::Purge(_) => {
                used.insert("purge schedule".to_string());
            }
        }
    }
    // A test that runs a schedule is a schedule test, which `jadpo test
    // --kind schedule` picks.
    if used.contains("body run a schedule in a test") {
        used.insert("schedule test kind".to_string());
    }
    if jadpo_check::events::events_settings(db, project)
        .store
        .is_some()
    {
        used.insert("events store".to_string());
    }
    if std::fs::read_dir(project_dir.join("domains")).is_ok_and(|domains| {
        domains
            .filter_map(Result::ok)
            .any(|domain| domain.path().join("events/events.jadpo_record").is_file())
    }) {
        used.insert("record events".to_string());
    }
    if jadpo_check::routes::groups(db, project)
        .groups
        .iter()
        .flat_map(|group| &group.routes)
        .any(|route| route.settle.is_some())
    {
        used.insert("route settle".to_string());
    }
    // A batch in an application with a tenant is limited to the tenant
    // its saved values name (chunk 6.4c).
    if used.contains("body call batch each")
        && jadpo_check::batches::tenant_id(db, project).is_some()
    {
        used.insert("batch limited to a tenant".to_string());
    }
    // An isolation level counts on an action or fixture that writes: one
    // that reaches no store teaches nothing about either.
    for id in graph.ids() {
        if let Some(level) = graph.node(id).isolation()
            && graph.outgoing(id, EdgeKind::Writes).next().is_some()
        {
            used.insert(format!("isolation {}", level.word()));
        }
    }
    // A setting's value is used where a file names it, as written; a
    // route test's `HttpMethod.post` is a typed body's variant too, which
    // [`body_features`] counts as `std`.
    for (name, values) in SETTINGS {
        for value in values {
            let written = format!("{name}.{value}");
            let named = project.files(db).iter().any(|file| {
                file.text(db).match_indices(&written).any(|(at, _)| {
                    !file.text(db)[at + written.len()..]
                        .starts_with(|c: char| c.is_alphanumeric() || c == '_')
                })
            });
            if named || used.remove(&format!("std {written}")) {
                used.insert(format!("setting {written}"));
            }
        }
    }
    // A closed list is used where a file names any of its values, as
    // written: `Locale.en_gb` in a test or `currency: Currency.gbp` in a
    // type alike.
    for (list, values) in closed_lists() {
        let prefix = format!("{list}.");
        let named = project.files(db).iter().any(|file| {
            let text = file.text(db);
            text.match_indices(&prefix).any(|(at, _)| {
                let before = text[..at].chars().next_back();
                let rest = &text[at + prefix.len()..];
                let value: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                !before.is_some_and(|c| c.is_alphanumeric() || c == '_') && values.contains(&value)
            })
        });
        if named {
            used.insert(format!("closed list {list}"));
        }
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
    let required = std::env::var("JADPO_REQUIRE_EXAMPLES").is_ok_and(|value| value == "1");
    let mut problems = Vec::new();
    let mut unused = Vec::new();
    for feature in &built {
        match (used.get(feature), exempt.get(feature.as_str())) {
            (None, None) if required => problems.push(format!(
                "{feature}: no example uses it; add it to an example in examples/, or exempt it in crates/jadpo-check/tests/examples.rs with the reason it can't run yet"
            )),
            (None, None) => unused.push(feature.as_str()),
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
    // A standard enum no operation takes or gives (`CardBrand`, a unit
    // enum such as `LengthUnit`, which `units.of` takes as a unit of any
    // kind) has values that are data, not features of their own, so an
    // example naming one isn't asked to list it.
    let std_enum_value = |feature: &str| {
        feature
            .strip_prefix("std ")
            .and_then(|rest| rest.split_once('.'))
            .is_some_and(|(owner, _)| jadpo_std::enums().iter().any(|(name, _)| *name == owner))
    };
    for feature in used.keys() {
        if !built.contains(feature) && !feature.starts_with("graph ") && !std_enum_value(feature) {
            problems.push(format!(
                "{feature}: an example uses it, but the built features don't list it; add it to the list in crates/jadpo-check/tests/examples.rs"
            ));
        }
    }
    if !unused.is_empty() {
        println!(
            "note: {} built feature(s) no example uses yet; the phase's examples chunk adds them \
             (JADPO_REQUIRE_EXAMPLES=1 fails on them):",
            unused.len()
        );
        for feature in &unused {
            println!("note:   {feature}");
        }
    }
    assert!(
        problems.is_empty(),
        "{} problem(s) with the examples' coverage:\n  {}",
        problems.len(),
        problems.join("\n  ")
    );
}
