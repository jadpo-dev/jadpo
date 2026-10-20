//! Personal data sent to a model (area 21, "Personal data sent to a
//! model"; chunk 5.5v5b): what a value holds that is personal data, for
//! the walk that follows each prompt's placeholders and each embedded text
//! back to where they were read ([`crate::policy::flow`]).
//!
//! - **Marked fields.** An entity's or event's field marked `personal:
//!   PersonalData.<category>` (area 15) is that category, and one marked
//!   `to_models: false` is kept from every model whatever `allowed:` lists
//!   ([`marks`]). Both are read where the field is declared
//!   ([`mark_problem`]).
//! - **The standard types' own categories** (area 19): a value of a type
//!   that is personal data whatever holds it (an `NhsNumber`, a
//!   `ChatMessage`), or one built on such a type, is its category, and so
//!   is a field of one.
//! - **`ai: { personal_data: { allowed: } }`** lists the categories the
//!   application's model may be sent ([`read_allowed`]); left out, it
//!   allows none.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_graph::TypeRef;
use jadpo_syntax::ast::{self, AstNode};

use crate::tree::{self, name_text, node_span};
use crate::types::{Const, Def, FieldsBase, Ty, TypeView};

/// `personal:` on a field: its area 15 category.
pub const PERSONAL: &str = "personal";

/// `to_models: false` on a field: kept from every model.
pub const TO_MODELS: &str = "to_models";

/// What an entity's or event's field says of itself.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mark {
    /// Its `personal:` category, when that reads.
    pub category: Option<String>,
    /// `to_models: false`.
    pub refused: bool,
}

/// Every entity's and event's marked field, by its owner's name and its
/// own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    pub fields: BTreeMap<(String, String), Mark>,
}

/// The `personal:` and `to_models:` of every entity's and event's field.
#[salsa::tracked(returns(ref))]
pub fn marks(db: &dyn Db, project: Project) -> Arc<Marks> {
    let mut out = Marks::default();
    for &file in project.files(db) {
        for shape in crate::types::type_shapes(db, file) {
            let crate::types::ShapeKind::Fields {
                base: FieldsBase::Entity | FieldsBase::Event,
                lines,
            } = &shape.kind
            else {
                continue;
            };
            for line in lines {
                let crate::types::Line::Field { name, settings, .. } = line else {
                    continue;
                };
                let mut mark = Mark::default();
                for (key, value) in settings {
                    match key.as_str() {
                        PERSONAL => mark.category = category_of(value),
                        TO_MODELS => mark.refused = *value == Const::Bool(false),
                        _ => {}
                    }
                }
                if mark != Mark::default() {
                    out.fields.insert((shape.name.clone(), name.clone()), mark);
                }
            }
        }
    }
    Arc::new(out)
}

/// The category `PersonalData.<name>` names, when it is one of area 15's.
fn category_of(value: &Const) -> Option<String> {
    match value {
        Const::Path(names) => match names.as_slice() {
            [enum_name, category]
                if enum_name == jadpo_std::ai::PERSONAL_DATA_ENUM && is_category(category) =>
            {
                Some(category.clone())
            }
            _ => None,
        },
        _ => None,
    }
}

fn is_category(name: &str) -> bool {
    jadpo_std::ai::CATEGORIES
        .iter()
        .any(|(known, _)| *known == name)
}

/// Whether area 15 counts `category` as sensitive.
pub fn is_sensitive(category: &str) -> bool {
    jadpo_std::ai::CATEGORIES
        .iter()
        .any(|(known, sensitive)| *known == category && *sensitive)
}

fn category_names() -> Vec<String> {
    jadpo_std::ai::CATEGORIES
        .iter()
        .map(|(name, _)| format!("{}.{name}", jadpo_std::ai::PERSONAL_DATA_ENUM))
        .collect()
}

/// The problem with an entity's or event's `personal:` or `to_models:`,
/// as written (`type.setting_value`): one of area 15's categories, and
/// `true` or `false`.
pub(crate) fn mark_problem(key: &str, value: &Const, span: Span) -> Option<Diagnostic> {
    match key {
        PERSONAL if category_of(value).is_none() => {
            let written = match value {
                Const::Path(names) => names.join("."),
                _ => String::new(),
            };
            let near = match value {
                Const::Path(names) if names.len() == 2 => {
                    nearest(&names[1], jadpo_std::ai::CATEGORIES.iter().map(|(name, _)| *name))
                }
                _ => None,
            };
            let problem = Diagnostic::new(
                &rule::TYPE_SETTING_VALUE,
                "`personal` takes one of area 15's categories",
            )
            .at(
                span.clone(),
                if written.is_empty() {
                    format!("{} here", value.describe())
                } else {
                    format!("`{written}` is no category")
                },
            )
            .reason(format!(
                "`personal:` marks the field as personal data of one category, `PersonalData.<category>`, which decides how carefully its value is treated: the categories are {}.",
                crate::and_list(
                    &category_names()
                        .iter()
                        .map(|name| format!("`{name}`"))
                        .collect::<Vec<_>>()
                )
            ));
            Some(match near {
                Some(near) => {
                    let fix = format!("{}.{near}", jadpo_std::ai::PERSONAL_DATA_ENUM);
                    problem.next(format!("Write `{fix}`")).edit(span, fix)
                }
                None => problem.next("Give `personal` a category, such as `PersonalData.contact`"),
            })
        }
        TO_MODELS if !matches!(value, Const::Bool(_)) => Some(
            Diagnostic::new(&rule::TYPE_SETTING_VALUE, "`to_models` takes `true` or `false`")
                .at(span.clone(), format!("{} here", value.describe()))
                .reason("`to_models: false` keeps the field out of every prompt and every text sent for an embedding, whatever `ai: { personal_data: { allowed: } }` lists.")
                .next("Write `to_models: false`")
                .edit(span, "false"),
        ),
        _ => None,
    }
}

/// Reads `personal_data: { allowed: [PersonalData.<category>, ...] }`
/// from `entry`, the member under `ai:` in `path`: the categories it
/// lists, in order. Anything else in it is `ai.settings_invalid`.
pub(crate) fn read_allowed(
    path: &str,
    entry: &ast::Member,
    out: &mut Vec<Diagnostic>,
) -> Vec<String> {
    let reason = "`personal_data: { allowed: [...] }` lists each personal-data category the application's prompts and embedded texts may send its model, each `PersonalData.<category>` (area 15), so what the provider is given is decided in `app.jadpo`, where a reviewer sees it.";
    let invalid = |title: String, span: Span, label: &str| {
        Diagnostic::new(&rule::AI_SETTINGS_INVALID, title)
            .at(span, label.to_string())
            .reason(reason)
    };
    let Some(block) = entry.block() else {
        out.push(
            invalid(
                "`personal_data:` holds no block".to_string(),
                node_span(path, entry.syntax()),
                "not a block",
            )
            .next("Write `personal_data: { allowed: [PersonalData.contact] }`"),
        );
        return Vec::new();
    };
    let mut allowed: Vec<String> = Vec::new();
    let mut given = false;
    for (key, member) in tree::members(Some(block)) {
        let word = name_text(&key).unwrap_or_default();
        let key_span = node_span(path, key.syntax());
        if word != jadpo_std::ai::ALLOWED {
            out.push(
                invalid(
                    format!("`{word}` isn't a setting of `personal_data:`"),
                    key_span.clone(),
                    "not a setting",
                )
                .next(format!("Use `allowed:` instead of `{word}:`"))
                .edit(key_span, jadpo_std::ai::ALLOWED),
            );
            continue;
        }
        if given {
            out.push(
                invalid(
                    "`personal_data:` gives `allowed:` twice".to_string(),
                    key_span,
                    "given again",
                )
                .next("Remove the second `allowed:`"),
            );
            continue;
        }
        given = true;
        let span = member.value().map_or_else(
            || node_span(path, member.syntax()),
            |value| node_span(path, value.syntax()),
        );
        let Some(Const::List(items)) = member.value().map(|value| Const::from_expr(&value)) else {
            out.push(
                invalid(
                    "`allowed:` must be a list of categories".to_string(),
                    span,
                    "not a list",
                )
                .next("Write `allowed: [PersonalData.contact]`, or `allowed: []` for none"),
            );
            continue;
        };
        for item in &items {
            match category_of(item) {
                Some(category) if allowed.contains(&category) => out.push(
                    invalid(
                        format!("`allowed:` lists `PersonalData.{category}` twice"),
                        span.clone(),
                        "listed again",
                    )
                    .next(format!("Remove the second `PersonalData.{category}`")),
                ),
                Some(category) => allowed.push(category),
                None => {
                    let written = match item {
                        Const::Path(names) => format!("`{}`", names.join(".")),
                        other => other.describe().to_string(),
                    };
                    out.push(
                        invalid(
                            format!("{written} isn't a personal-data category"),
                            span.clone(),
                            "not a category",
                        )
                        .next(format!(
                            "List categories of area 15's, such as `PersonalData.contact`; they are {}",
                            category_names().join(", ")
                        )),
                    );
                }
            }
        }
    }
    if !given {
        out.push(
            invalid(
                "`personal_data:` has no `allowed:`".to_string(),
                node_span(path, entry.syntax()),
                "no `allowed:`",
            )
            .next("Add `allowed: [...]`, the categories the model may be sent, or `allowed: []` for none"),
        );
    }
    allowed
}

/// What a value holds that is personal data: a marked field, or a type
/// that is personal data whatever holds it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Holder {
    Field { owner: String, field: String },
    Type(String),
}

impl Holder {
    /// As a problem names it, without the domain an event's name is
    /// read with: `` `Patient.notes` ``, `` a `NhsNumber` ``.
    pub fn named(&self) -> String {
        match self {
            Holder::Field { owner, field } => {
                format!("`{}.{field}`", owner.rsplit('.').next().unwrap_or(owner))
            }
            Holder::Type(ty) => format!("a `{ty}`"),
        }
    }

    /// The key a problem is reported once for.
    pub fn key(&self) -> String {
        match self {
            Holder::Field { owner, field } => format!("{owner}.{field}"),
            Holder::Type(ty) => ty.clone(),
        }
    }
}

/// What one holder is: its categories, and whether it is kept from
/// every model.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Holding {
    /// Each category, with the standard type it comes from, or `None`
    /// for the field's own `personal:`.
    pub categories: BTreeMap<String, Option<String>>,
    pub refused: bool,
}

impl Holding {
    fn join(&mut self, other: &Holding) {
        for (category, from) in &other.categories {
            self.categories
                .entry(category.clone())
                .or_insert_with(|| from.clone());
        }
        self.refused |= other.refused;
    }

    fn is_empty(&self) -> bool {
        self.categories.is_empty() && !self.refused
    }
}

/// What `owner`'s field `field` holds: its own marks, and the categories
/// of the standard types its type is or holds.
pub fn field_holding(
    lookup: &dyn TypeView,
    marks: &Marks,
    owner: &str,
    field: &str,
) -> Option<Holding> {
    let mut holding = Holding::default();
    let bare = owner.rsplit('.').next().unwrap_or(owner);
    if let Some(mark) = marks
        .fields
        .get(&(owner.to_string(), field.to_string()))
        .or_else(|| marks.fields.get(&(bare.to_string(), field.to_string())))
    {
        if let Some(category) = &mark.category {
            holding.categories.insert(category.clone(), None);
        }
        holding.refused = mark.refused;
    }
    if let Some(def) = lookup.field(owner, &[field.to_string()]) {
        let mut found = BTreeMap::new();
        deep(
            lookup,
            marks,
            &def.declared,
            &mut found,
            &mut BTreeSet::new(),
        );
        for inner in found.values() {
            for (category, from) in &inner.categories {
                holding
                    .categories
                    .entry(category.clone())
                    .or_insert_with(|| from.clone());
            }
            holding.refused |= inner.refused;
        }
    }
    (!holding.is_empty()).then_some(holding)
}

/// The holders anywhere inside a value of `ty`: each standard type that
/// is personal data, and each marked field of an entity or event it
/// holds, however deep.
pub fn deep(
    lookup: &dyn TypeView,
    marks: &Marks,
    ty: &Ty,
    out: &mut BTreeMap<Holder, Holding>,
    seen: &mut BTreeSet<String>,
) {
    match ty {
        Ty::List(inner) | Ty::Set(inner) | Ty::Nullable(inner) => {
            deep(lookup, marks, inner, out, seen)
        }
        Ty::Map(key, value) => {
            deep(lookup, marks, key, out, seen);
            deep(lookup, marks, value, out, seen);
        }
        Ty::Named(name) | Ty::Opaque(name, _) => {
            if let Ty::Opaque(_, args) = ty {
                for arg in args {
                    deep(lookup, marks, arg, out, seen);
                }
            }
            if let Some(category) = standard_category(name) {
                standard(out, name, category);
            }
            if !seen.insert(name.clone()) {
                return;
            }
            match lookup.def(name) {
                Some(Def::Scalar { parent, .. }) => deep(lookup, marks, parent, out, seen),
                Some(Def::Fields { base, fields, .. }) => {
                    for field in fields {
                        if matches!(base, FieldsBase::Entity | FieldsBase::Event)
                            && let Some(holding) = own_mark(marks, name, &field.name)
                        {
                            out.entry(Holder::Field {
                                owner: name.clone(),
                                field: field.name.clone(),
                            })
                            .or_default()
                            .join(&holding);
                        }
                        deep(lookup, marks, &field.declared, out, seen);
                    }
                }
                Some(Def::Enum { variants }) => {
                    for variant in variants {
                        for field in variant.fields.iter().flatten() {
                            deep(lookup, marks, &field.declared, out, seen);
                        }
                    }
                }
                _ => {}
            }
        }
        Ty::Field(owner, path) => {
            if let [field] = path.as_slice()
                && let Some(holding) = own_mark(marks, owner, field)
            {
                out.entry(Holder::Field {
                    owner: owner.clone(),
                    field: field.clone(),
                })
                .or_default()
                .join(&holding);
            }
            if let Some(def) = lookup.field(owner, path) {
                deep(lookup, marks, &def.declared, out, seen);
            }
        }
        Ty::Variant(name, variant) => {
            for field in lookup
                .variants(name)
                .into_iter()
                .flatten()
                .filter(|known| known.name == *variant)
                .flat_map(|known| known.fields.iter().flatten())
            {
                deep(lookup, marks, &field.declared, out, seen);
            }
        }
        _ => {}
    }
}

/// A field's own `personal:` and `to_models:`, as a holding.
fn own_mark(marks: &Marks, owner: &str, field: &str) -> Option<Holding> {
    let bare = owner.rsplit('.').next().unwrap_or(owner);
    let mark = marks
        .fields
        .get(&(owner.to_string(), field.to_string()))
        .or_else(|| marks.fields.get(&(bare.to_string(), field.to_string())))?;
    let mut holding = Holding {
        categories: BTreeMap::new(),
        refused: mark.refused,
    };
    if let Some(category) = &mark.category {
        holding.categories.insert(category.clone(), None);
    }
    (!holding.is_empty()).then_some(holding)
}

fn standard(out: &mut BTreeMap<Holder, Holding>, name: &str, category: &str) {
    out.entry(Holder::Type(name.to_string()))
        .or_default()
        .categories
        .insert(category.to_string(), Some(name.to_string()));
}

/// The category of the standard type `name`, when it is personal data
/// whatever holds it (area 19).
pub fn standard_category(name: &str) -> Option<&'static str> {
    jadpo_std::standard::standard_type(name).and_then(|entry| entry.personal())
}

/// The standard types a value of `ty` is that are personal data whatever
/// holds them, each with its category: `ty` itself, a type built on one,
/// or a list, set, map or option of one. A field of an object is its
/// own value, so an object's are not looked into.
pub fn shallow(lookup: &dyn TypeView, ty: &TypeRef) -> Vec<(String, String)> {
    let mut out = Vec::new();
    shallow_ref(lookup, ty, &mut out, 0);
    out
}

fn shallow_ref(lookup: &dyn TypeView, ty: &TypeRef, out: &mut Vec<(String, String)>, depth: usize) {
    if depth > 16 {
        return;
    }
    match ty {
        TypeRef::List(inner) | TypeRef::Set(inner) | TypeRef::Nullable(inner) => {
            shallow_ref(lookup, inner, out, depth + 1)
        }
        TypeRef::Map(key, value) => {
            shallow_ref(lookup, key, out, depth + 1);
            shallow_ref(lookup, value, out, depth + 1);
        }
        TypeRef::Std(name, args) => {
            if let Some(category) = standard_category(name) {
                push(out, name, category);
            }
            for arg in args {
                shallow_ref(lookup, arg, out, depth + 1);
            }
        }
        TypeRef::Node(key) => {
            shallow_ty(lookup, &Ty::Named(key.name().to_string()), out, depth + 1)
        }
        TypeRef::Field(key) => {
            if let Some((owner, field)) = key.name().rsplit_once('.')
                && let Some(def) = lookup.field(owner, &[field.to_string()])
            {
                shallow_ty(lookup, &def.declared, out, depth + 1);
            }
        }
        _ => {}
    }
}

fn shallow_ty(lookup: &dyn TypeView, ty: &Ty, out: &mut Vec<(String, String)>, depth: usize) {
    if depth > 16 {
        return;
    }
    match ty {
        Ty::List(inner) | Ty::Set(inner) | Ty::Nullable(inner) => {
            shallow_ty(lookup, inner, out, depth + 1)
        }
        Ty::Map(key, value) => {
            shallow_ty(lookup, key, out, depth + 1);
            shallow_ty(lookup, value, out, depth + 1);
        }
        Ty::Named(name) | Ty::Opaque(name, _) => {
            if let Some(category) = standard_category(name) {
                push(out, name, category);
                return;
            }
            if let Ty::Opaque(_, args) = ty {
                for arg in args {
                    shallow_ty(lookup, arg, out, depth + 1);
                }
            }
            if let Some(Def::Scalar { parent, .. }) = lookup.def(name) {
                shallow_ty(lookup, parent, out, depth + 1);
            }
        }
        Ty::Field(owner, path) => {
            if let Some(def) = lookup.field(owner, path) {
                shallow_ty(lookup, &def.declared, out, depth + 1);
            }
        }
        _ => {}
    }
}

fn push(out: &mut Vec<(String, String)>, name: &str, category: &str) {
    let entry = (name.to_string(), category.to_string());
    if !out.contains(&entry) {
        out.push(entry);
    }
}
