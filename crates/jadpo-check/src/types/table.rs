//! The project's types (area 2), built from every file's shapes and the
//! standard library's stubs: what each name is, each object's final fields
//! after its copying lines, each scalar's rules, and the compatibility
//! rules between types: widening along declared steps only, invariant
//! collections, `none` and `T` into `T?`, and `Error` compatible with
//! everything.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Db, Project};

use super::rules::{self, Rules, Subject};
use super::shape::{FieldsBase, Line, Setting, ShapeKind, type_shapes};
use super::ty::{BASES, Prim, Ty, TypeRef, container_arity};
use crate::std_lib::StdNames;

/// How far a chain of declared steps is followed before giving up: only a
/// cycle of declarations, already wrong, gets this far.
const DEPTH: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FieldDef {
    pub name: String,
    /// The field's declared type, `?` included.
    pub declared: Ty,
    /// The field's own rules, from its settings block.
    pub rules: Rules,
    /// An `Input` field the sender may leave out: reading it gives `T?`.
    pub optional: bool,
    pub default: bool,
    /// Brought in by `include` or `pick`.
    pub copied: bool,
    /// For a written field that replaces one an `include` or `pick` above
    /// brought in: the type that field came from.
    pub replaces: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VariantDef {
    pub name: String,
    /// A payload variant's fields; `None` for a plain variant.
    pub fields: Option<Vec<FieldDef>>,
}

/// What a type name means.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Def {
    /// A refinement: its parent and the rules it adds.
    Scalar {
        parent: Ty,
        rules: Box<Rules>,
    },
    /// An object, input, output, event, email, failure or entity.
    Fields {
        base: FieldsBase,
        fields: Vec<FieldDef>,
        /// When its `include` lines lead back to itself, so its fields
        /// would be copied from themselves: the first name, in name order,
        /// on that loop, where the loop is reported once.
        copies_itself: Option<String>,
    },
    Enum {
        variants: Vec<VariantDef>,
    },
}

/// Every type in the project, by name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct TypeTable {
    defs: BTreeMap<String, Def>,
}

/// The type table of `project`. It reads only the files' shapes, so an
/// edit inside a body never rebuilds it.
#[salsa::tracked(returns(ref))]
pub fn type_table(db: &dyn Db, project: Project) -> TypeTable {
    let std = StdNames::get();
    let mut shapes: BTreeMap<String, ShapeKind> = BTreeMap::new();
    for &file in project.files(db) {
        for shape in type_shapes(db, file) {
            shapes
                .entry(shape.name.clone())
                .or_insert_with(|| shape.kind.clone());
        }
    }
    TypeTable::build(&shapes, std)
}

// --- deriving fields ----------------------------------------------------

/// Where a field comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    Written {
        ty: TypeRef,
        settings: Vec<Setting>,
    },
    /// Copied by `include` or `pick` from `owner.field`.
    Copied {
        host: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawField {
    pub name: String,
    pub source: Source,
    pub optional: bool,
    pub default: bool,
    /// The type a copied field it replaced came from.
    pub replaces: Option<String>,
}

/// What a name in a copying line turned out to be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    Fields(Vec<String>),
    NotFields,
    /// Not declared: resolution has said so.
    Unknown,
}

/// A problem with one line, or one entry of a line's list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineProblem {
    pub line: usize,
    pub entry: Option<usize>,
    pub kind: LineKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineKind {
    /// A field written twice.
    Duplicate(String),
    /// `include` on an `Input`.
    CopyIntoInput,
    /// `include` of something without fields.
    NotFields(String),
    /// `pick` of a field its owner doesn't have.
    UnknownField {
        host: String,
        field: String,
        fields: Vec<String>,
    },
    /// A `pick` entry that isn't `Owner.field`.
    NotAFieldPath,
    /// `omit` or `optional` naming a field not present at that point.
    Absent { field: String, word: &'static str },
    /// A copy that would replace a field already present.
    CopyClash { field: String, earlier: String },
    /// `optional` anywhere but an `Input`.
    OptionalOutsideInput,
}

/// Builds a body's fields from its lines, top to bottom (area 2, "Building
/// a type from other types"), asking `find` for the fields of the types it
/// copies from.
pub fn derive(
    base: FieldsBase,
    lines: &[Line],
    find: &mut dyn FnMut(&str) -> Found,
) -> (Vec<RawField>, Vec<LineProblem>) {
    let mut fields: Vec<RawField> = Vec::new();
    let mut problems = Vec::new();
    let input = base == FieldsBase::Input;
    for (line, item) in lines.iter().enumerate() {
        let mut problem = |entry: Option<usize>, kind: LineKind| {
            problems.push(LineProblem { line, entry, kind });
        };
        match item {
            Line::Field { name, ty, settings } => {
                let field = RawField {
                    name: name.clone(),
                    source: Source::Written {
                        ty: ty.clone(),
                        settings: settings.clone(),
                    },
                    optional: input && setting_is_true(settings, "optional"),
                    default: input && settings.iter().any(|(key, _)| key == "default"),
                    replaces: None,
                };
                match fields.iter().position(|f| f.name == *name) {
                    Some(at) if matches!(fields[at].source, Source::Copied { .. }) => {
                        let replaces = match &fields[at].source {
                            Source::Copied { host } => Some(host.clone()),
                            Source::Written { .. } => None,
                        };
                        fields[at] = RawField { replaces, ..field };
                    }
                    Some(_) => problem(None, LineKind::Duplicate(name.clone())),
                    None => fields.push(field),
                }
            }
            Line::Include(entries) => {
                if input {
                    problem(None, LineKind::CopyIntoInput);
                }
                for (entry, ty) in entries.iter().enumerate() {
                    let TypeRef::Path(names, _) = ty else {
                        continue;
                    };
                    let host = names.join(".");
                    let found = if names.len() == 1 {
                        find(&host)
                    } else {
                        Found::NotFields
                    };
                    match found {
                        Found::Fields(names) => {
                            for name in names {
                                copy(&mut fields, &host, &name, &mut |kind| {
                                    problem(Some(entry), kind)
                                });
                            }
                        }
                        Found::NotFields => problem(Some(entry), LineKind::NotFields(host)),
                        Found::Unknown => {}
                    }
                }
            }
            Line::Pick(entries) => {
                for (entry, path) in entries.iter().enumerate() {
                    let [host, field] = path.as_slice() else {
                        problem(Some(entry), LineKind::NotAFieldPath);
                        continue;
                    };
                    match find(host) {
                        Found::Fields(names) if names.contains(field) => {
                            copy(&mut fields, host, field, &mut |kind| {
                                problem(Some(entry), kind)
                            });
                        }
                        Found::Fields(names) => problem(
                            Some(entry),
                            LineKind::UnknownField {
                                host: host.clone(),
                                field: field.clone(),
                                fields: names,
                            },
                        ),
                        Found::NotFields => problem(Some(entry), LineKind::NotFields(host.clone())),
                        Found::Unknown => {}
                    }
                }
            }
            Line::Omit(entries) => {
                for (entry, path) in entries.iter().enumerate() {
                    let Some(field) = path.last() else {
                        problem(Some(entry), LineKind::NotAFieldPath);
                        continue;
                    };
                    match fields.iter().position(|f| f.name == *field) {
                        Some(at) => {
                            fields.remove(at);
                        }
                        None => problem(
                            Some(entry),
                            LineKind::Absent {
                                field: field.clone(),
                                word: "omit",
                            },
                        ),
                    }
                }
            }
            Line::Optional(entries) => {
                if !input {
                    problem(None, LineKind::OptionalOutsideInput);
                    continue;
                }
                match entries {
                    None => fields.iter_mut().for_each(|f| f.optional = true),
                    Some(entries) => {
                        for (entry, path) in entries.iter().enumerate() {
                            let Some(field) = path.last() else {
                                problem(Some(entry), LineKind::NotAFieldPath);
                                continue;
                            };
                            match fields.iter_mut().find(|f| f.name == *field) {
                                Some(found) => found.optional = true,
                                None => problem(
                                    Some(entry),
                                    LineKind::Absent {
                                        field: field.clone(),
                                        word: "optional",
                                    },
                                ),
                            }
                        }
                    }
                }
            }
            Line::NotAList(_) | Line::Reserved(_) | Line::Other => {}
        }
    }
    (fields, problems)
}

fn copy(fields: &mut Vec<RawField>, host: &str, name: &str, problem: &mut dyn FnMut(LineKind)) {
    if let Some(existing) = fields.iter().find(|f| f.name == name) {
        let earlier = match &existing.source {
            Source::Copied { host } => format!("`{host}`"),
            Source::Written { .. } => "a field written above".to_string(),
        };
        problem(LineKind::CopyClash {
            field: name.to_string(),
            earlier,
        });
        return;
    }
    fields.push(RawField {
        name: name.to_string(),
        source: Source::Copied {
            host: host.to_string(),
        },
        optional: false,
        default: false,
        replaces: None,
    });
}

fn setting_is_true(settings: &[Setting], name: &str) -> bool {
    settings
        .iter()
        .any(|(key, value)| key == name && *value == rules::Const::Bool(true))
}

// --- building the table ---------------------------------------------------

struct Deriver<'a> {
    shapes: &'a BTreeMap<String, ShapeKind>,
    /// Each body's fields; `None` while being computed, which stops a cycle.
    done: BTreeMap<String, Option<Vec<RawField>>>,
    /// The bodies being computed, outermost first.
    stack: Vec<String>,
    /// Every body on a cycle of `include` lines, with the cycle's first
    /// name.
    cyclic: BTreeMap<String, String>,
}

impl Deriver<'_> {
    fn fields(&mut self, name: &str) -> Found {
        match self.shapes.get(name) {
            Some(ShapeKind::Fields { base, lines }) => {
                if let Some(done) = self.done.get(name) {
                    if done.is_none()
                        && let Some(at) = self.stack.iter().position(|n| n == name)
                    {
                        let members = &self.stack[at..];
                        let first = members
                            .iter()
                            .chain(members.iter().filter_map(|n| self.cyclic.get(n)))
                            .min()
                            .cloned()
                            .unwrap_or_default();
                        for member in members {
                            self.cyclic.insert(member.clone(), first.clone());
                        }
                    }
                    return Found::Fields(done.iter().flatten().map(|f| f.name.clone()).collect());
                }
                self.done.insert(name.to_string(), None);
                self.stack.push(name.to_string());
                let (fields, _) = derive(*base, lines, &mut |other| self.fields(other));
                self.stack.pop();
                let names = fields.iter().map(|f| f.name.clone()).collect();
                self.done.insert(name.to_string(), Some(fields));
                Found::Fields(names)
            }
            Some(_) => Found::NotFields,
            None if StdNames::get().is_type(name) => Found::NotFields,
            None => Found::Unknown,
        }
    }
}

impl TypeTable {
    fn build(shapes: &BTreeMap<String, ShapeKind>, std: &StdNames) -> TypeTable {
        let mut deriver = Deriver {
            shapes,
            done: BTreeMap::new(),
            stack: Vec::new(),
            cyclic: Default::default(),
        };
        for name in shapes.keys() {
            deriver.fields(name);
        }
        let derived = deriver.done;
        let cyclic = deriver.cyclic;
        let placeholder = |raw: &RawField| FieldDef {
            name: raw.name.clone(),
            declared: Ty::Error,
            rules: Rules::default(),
            optional: raw.optional,
            default: raw.default,
            copied: matches!(raw.source, Source::Copied { .. }),
            replaces: raw.replaces.clone(),
        };
        let variant_fields = |lines: &[Line]| -> Vec<RawField> {
            derive(FieldsBase::Object, lines, &mut |_| Found::Unknown).0
        };

        // First every name with its kind and field names, so types can be
        // resolved; then the types and rules themselves.
        let mut table = TypeTable::default();
        for (name, variants) in std.enums() {
            table.defs.insert(
                name.clone(),
                Def::Enum {
                    variants: variants
                        .iter()
                        .map(|variant| VariantDef {
                            name: variant.clone(),
                            fields: None,
                        })
                        .collect(),
                },
            );
        }
        for (name, kind) in shapes {
            let def = match kind {
                ShapeKind::Scalar { .. } => Def::Scalar {
                    parent: Ty::Error,
                    rules: Box::default(),
                },
                ShapeKind::Fields { base, .. } => Def::Fields {
                    copies_itself: cyclic.get(name).cloned(),
                    base: *base,
                    fields: derived
                        .get(name)
                        .into_iter()
                        .flatten()
                        .flatten()
                        .map(placeholder)
                        .collect(),
                },
                ShapeKind::Enum { variants } => Def::Enum {
                    variants: variants
                        .iter()
                        .map(|variant| VariantDef {
                            name: variant.name.clone(),
                            fields: variant.fields.as_ref().map(|lines| {
                                variant_fields(lines).iter().map(placeholder).collect()
                            }),
                        })
                        .collect(),
                },
            };
            table.defs.insert(name.clone(), def);
        }

        // Declared types and parents.
        let mut resolved: BTreeMap<String, Def> = BTreeMap::new();
        for (name, kind) in shapes {
            let resolve_fields = |raws: &[RawField]| -> Vec<FieldDef> {
                raws.iter()
                    .map(|raw| FieldDef {
                        declared: match &raw.source {
                            Source::Written { ty, .. } => table.resolve(ty),
                            Source::Copied { host } => {
                                Ty::Field(host.clone(), vec![raw.name.clone()])
                            }
                        },
                        ..placeholder(raw)
                    })
                    .collect()
            };
            let def = match kind {
                ShapeKind::Scalar { parent, .. } => Def::Scalar {
                    parent: table.resolve(parent),
                    rules: Box::default(),
                },
                ShapeKind::Fields { base, .. } => Def::Fields {
                    copies_itself: cyclic.get(name).cloned(),
                    base: *base,
                    fields: resolve_fields(
                        derived
                            .get(name)
                            .and_then(Option::as_ref)
                            .map_or(&[][..], Vec::as_slice),
                    ),
                },
                ShapeKind::Enum { variants } => Def::Enum {
                    variants: variants
                        .iter()
                        .map(|variant| VariantDef {
                            name: variant.name.clone(),
                            fields: variant
                                .fields
                                .as_ref()
                                .map(|lines| resolve_fields(&variant_fields(lines))),
                        })
                        .collect(),
                },
            };
            resolved.insert(name.clone(), def);
        }
        table.defs.extend(resolved);

        // Rules, which need each type's representation.
        let mut ruled: BTreeMap<String, Def> = BTreeMap::new();
        for (name, kind) in shapes {
            let Some(def) = table.defs.get(name) else {
                continue;
            };
            let field_rules = |fields: &[FieldDef], raws: &[RawField]| -> Vec<FieldDef> {
                fields
                    .iter()
                    .map(|field| {
                        let settings = raws
                            .iter()
                            .find(|raw| raw.name == field.name)
                            .and_then(|raw| match &raw.source {
                                Source::Written { settings, .. } => Some(settings.as_slice()),
                                Source::Copied { .. } => None,
                            })
                            .unwrap_or(&[]);
                        FieldDef {
                            rules: table.rules_from(&field.declared, settings),
                            ..field.clone()
                        }
                    })
                    .collect()
            };
            let def = match (def, kind) {
                (Def::Scalar { parent, .. }, ShapeKind::Scalar { settings, .. }) => Def::Scalar {
                    parent: parent.clone(),
                    rules: Box::new(table.rules_from(parent, settings)),
                },
                (
                    Def::Fields {
                        base,
                        fields,
                        copies_itself,
                    },
                    _,
                ) => Def::Fields {
                    copies_itself: copies_itself.clone(),
                    base: *base,
                    fields: field_rules(
                        fields,
                        derived
                            .get(name)
                            .and_then(Option::as_ref)
                            .map_or(&[][..], Vec::as_slice),
                    ),
                },
                (Def::Enum { variants }, ShapeKind::Enum { variants: shapes }) => Def::Enum {
                    variants: variants
                        .iter()
                        .zip(shapes)
                        .map(|(variant, shape)| VariantDef {
                            name: variant.name.clone(),
                            fields: variant.fields.as_ref().map(|fields| {
                                let raws = shape
                                    .fields
                                    .as_ref()
                                    .map(|lines| variant_fields(lines))
                                    .unwrap_or_default();
                                field_rules(fields, &raws)
                            }),
                        })
                        .collect(),
                },
                (other, _) => other.clone(),
            };
            ruled.insert(name.clone(), def);
        }
        table.defs.extend(ruled);
        table
    }
}

impl TypeView for TypeTable {
    fn def(&self, name: &str) -> Option<&Def> {
        self.defs.get(name)
    }
}

/// Reading types: what each name means, through [`TypeView::def`], and the
/// compatibility rules built on that. The whole [`TypeTable`] is one view;
/// a body reads through the per-name lookups instead, so it depends only
/// on the declarations it reads (docs/compiler-runtime.md §2.4).
pub trait TypeView {
    /// What `name` means, if it is a type of the project or the stubs.
    fn def(&self, name: &str) -> Option<&Def>;

    /// Whether `name` is declared in the project or the stubs.
    fn contains(&self, name: &str) -> bool {
        self.def(name).is_some()
    }

    /// The rules a block of settings adds to values of `ty`; settings that
    /// don't apply are left out here and reported by the declaration check.
    fn rules_from(&self, ty: &Ty, settings: &[Setting]) -> Rules {
        let subject = self.subject(ty);
        let mut out = Rules::default();
        for (name, value) in settings {
            let _ = rules::constraint(subject, name, value, &mut out);
        }
        out
    }

    /// The fields of an object, input, output, event, email, failure or
    /// entity.
    fn fields(&self, name: &str) -> Option<&[FieldDef]> {
        match self.def(name)? {
            Def::Fields { fields, .. } => Some(fields),
            _ => None,
        }
    }

    fn base(&self, name: &str) -> Option<FieldsBase> {
        match self.def(name)? {
            Def::Fields { base, .. } => Some(*base),
            _ => None,
        }
    }

    fn variants(&self, name: &str) -> Option<&[VariantDef]> {
        match self.def(name)? {
            Def::Enum { variants } => Some(variants),
            _ => None,
        }
    }

    /// The field `owner.path`: an object's or entity's field, or a payload
    /// field `Enum.variant.field`.
    fn field(&self, host: &str, path: &[String]) -> Option<&FieldDef> {
        match (self.def(host)?, path) {
            (Def::Fields { fields, .. }, [name]) => fields.iter().find(|f| f.name == *name),
            (Def::Enum { variants }, [variant, name]) => variants
                .iter()
                .find(|v| v.name == *variant)?
                .fields
                .as_ref()?
                .iter()
                .find(|f| f.name == *name),
            _ => None,
        }
    }

    /// The type a written type stands for; `Error` for anything not a type,
    /// which the declaration check or resolution reports.
    fn resolve(&self, written: &TypeRef) -> Ty {
        self.resolve_with(written, false)
    }

    /// As [`TypeTable::resolve`], reading single capital letters and
    /// `Function<T, U>` as a standard-library signature does.
    fn resolve_std(&self, written: &TypeRef) -> Ty {
        self.resolve_with(written, true)
    }

    fn resolve_with(&self, written: &TypeRef, stub: bool) -> Ty {
        match written {
            TypeRef::Invalid => Ty::Error,
            TypeRef::Nullable(inner) => self.resolve_with(inner, stub).nullable(),
            TypeRef::Path(names, args) => {
                let args: Vec<Ty> = args
                    .iter()
                    .map(|arg| self.resolve_with(arg, stub))
                    .collect();
                match names.as_slice() {
                    [name] => self.resolve_name(name, args, stub),
                    [host, rest @ ..] if self.field(host, rest).is_some() => {
                        Ty::Field(host.clone(), rest.to_vec())
                    }
                    _ => Ty::Error,
                }
            }
        }
    }

    fn resolve_name(&self, name: &str, mut args: Vec<Ty>, stub: bool) -> Ty {
        if stub && name.len() == 1 && name.chars().all(|c| c.is_ascii_uppercase()) {
            return Ty::Var(name.to_string());
        }
        if stub && name == "Number" && args.is_empty() {
            return Ty::Number;
        }
        if stub && name == "Function" && args.len() == 2 {
            let to = args.pop().unwrap_or(Ty::Error);
            let from = args.pop().unwrap_or(Ty::Error);
            return Ty::Function(Box::new(from), Box::new(to));
        }
        if let Some(arity) = container_arity(name) {
            if args.len() != arity {
                return Ty::Error;
            }
            let mut args = args.into_iter();
            let mut next = || Box::new(args.next().unwrap_or(Ty::Error));
            return match name {
                "List" => Ty::List(next()),
                "Set" => Ty::Set(next()),
                _ => {
                    let key = next();
                    Ty::Map(key, next())
                }
            };
        }
        if let Some(prim) = Prim::from_name(name) {
            return if args.is_empty() {
                Ty::Prim(prim)
            } else {
                Ty::Error
            };
        }
        if BASES.contains(&name) {
            return Ty::Error;
        }
        if self.def(name).is_some() {
            return if args.is_empty() {
                Ty::Named(name.to_string())
            } else {
                Ty::Error
            };
        }
        if StdNames::get().is_opaque(name) {
            return Ty::Opaque(name.to_string(), args);
        }
        Ty::Error
    }

    /// Whether a value of this type may be `none`: a `T?`, or a field type
    /// whose declared type is one.
    fn is_nullable(&self, ty: &Ty) -> bool {
        let mut current = ty.clone();
        for _ in 0..DEPTH {
            match &current {
                Ty::Nullable(_) | Ty::None => return true,
                Ty::Field(host, path) => match self.field(host, path) {
                    Some(field) => current = field.declared.clone(),
                    None => return false,
                },
                _ => return false,
            }
        }
        false
    }

    /// One declared widening step (area 2): a field type to its declared
    /// type, a named type to the named type it builds on. Never from a
    /// named type to a built-in representation.
    fn parent(&self, ty: &Ty) -> Option<Ty> {
        match ty {
            Ty::Field(host, path) => {
                let declared = self.field(host, path)?.declared.strip_nullable().clone();
                (!declared.is_error()).then_some(declared)
            }
            Ty::Named(name) => match self.def(name)? {
                Def::Scalar { parent, .. } => match parent {
                    Ty::Prim(prim) if prim.is_representation() => None,
                    Ty::Error => None,
                    other => Some(other.clone()),
                },
                _ => None,
            },
            _ => None,
        }
    }

    /// The built-in type at the bottom of a scalar's chain: `Email` for a
    /// type built on `Email`, `Int` for `Customer.age`.
    fn base_prim(&self, ty: &Ty) -> Option<Prim> {
        let mut current = ty.strip_nullable().clone();
        for _ in 0..DEPTH {
            match &current {
                Ty::Prim(prim) => return Some(*prim),
                Ty::Field(host, path) => {
                    current = self.field(host, path)?.declared.strip_nullable().clone();
                }
                Ty::Named(name) => match self.def(name)? {
                    Def::Scalar { parent, .. } => current = parent.strip_nullable().clone(),
                    _ => return None,
                },
                _ => return None,
            }
        }
        None
    }

    /// The type with field types and refinements followed to what they
    /// are built from: `List<Tag>` for a field declared `List<Tag>`.
    fn structure(&self, ty: &Ty) -> Ty {
        let mut current = ty.strip_nullable().clone();
        for _ in 0..DEPTH {
            let next = match &current {
                Ty::Field(host, path) => self
                    .field(host, path)
                    .map(|f| f.declared.strip_nullable().clone()),
                Ty::Named(name) => match self.def(name) {
                    Some(Def::Scalar { parent, .. }) => Some(parent.strip_nullable().clone()),
                    _ => None,
                },
                _ => None,
            };
            match next {
                Some(next) => current = next,
                None => return current,
            }
        }
        current
    }

    /// What a value of this type is made of: the validated text types are
    /// text.
    fn representation(&self, ty: &Ty) -> Option<Prim> {
        self.base_prim(ty).map(Prim::representation)
    }

    /// What constraints on this type apply to.
    fn subject(&self, ty: &Ty) -> Subject {
        let mut current = ty.strip_nullable().clone();
        for _ in 0..DEPTH {
            match &current {
                Ty::List(_) | Ty::Set(_) | Ty::Map(..) => return Subject::Collection,
                Ty::Prim(prim) => return Subject::Prim(*prim),
                Ty::Field(host, path) => match self.field(host, path) {
                    Some(field) => current = field.declared.strip_nullable().clone(),
                    None => return Subject::Other,
                },
                Ty::Named(name) => match self.def(name) {
                    Some(Def::Scalar { parent, .. }) => current = parent.strip_nullable().clone(),
                    _ => return Subject::Other,
                },
                _ => return Subject::Other,
            }
        }
        Subject::Other
    }

    /// Every rule a value of this type must follow, with the declaration
    /// that adds it, most specific first: a field's own rules, then its
    /// declared type's, then the types that one builds on.
    fn rule_chain(&self, ty: &Ty) -> Vec<(String, &Rules)> {
        let mut chain = Vec::new();
        let mut current = ty.strip_nullable().clone();
        for _ in 0..DEPTH {
            match &current {
                Ty::Field(host, path) => {
                    let Some(field) = self.field(host, path) else {
                        break;
                    };
                    chain.push((current.to_string(), &field.rules));
                    current = field.declared.strip_nullable().clone();
                }
                Ty::Named(name) => match self.def(name) {
                    Some(Def::Scalar { parent, rules }) => {
                        chain.push((name.clone(), rules));
                        current = parent.strip_nullable().clone();
                    }
                    _ => break,
                },
                _ => break,
            }
        }
        chain
    }

    /// Whether a value of type `actual` may be used where `expected` is
    /// required, without a constructor (area 2, "When one type can stand in
    /// for another").
    fn assignable(&self, actual: &Ty, expected: &Ty) -> bool {
        // A type variable no argument fixed accepts anything, but the
        // containers around it are still checked.
        if expected.has_unknown() && !self.same_shape(actual, expected) {
            return false;
        }
        if actual.has_error() || expected.has_error() || actual == expected {
            return true;
        }
        if matches!(expected, Ty::Var(_)) {
            return true;
        }
        if *actual == Ty::None {
            return self.is_nullable(expected);
        }
        if self.is_nullable(actual) && !self.is_nullable(expected) {
            return false;
        }
        let expected = expected.strip_nullable();
        match (actual.strip_nullable(), expected) {
            (Ty::List(a), Ty::List(b)) | (Ty::Set(a), Ty::Set(b)) => {
                return a == b || a.has_error() || b.has_error();
            }
            (Ty::Map(ka, va), Ty::Map(kb, vb)) => {
                return (ka == kb || ka.has_error() || kb.has_error())
                    && (va == vb || va.has_error() || vb.has_error());
            }
            _ => {}
        }
        self.widens_to(actual.strip_nullable(), expected)
    }

    /// Whether `actual` is the container `expected` is, all the way down to
    /// the positions a type variable no argument fixed holds.
    fn same_shape(&self, actual: &Ty, expected: &Ty) -> bool {
        if actual.has_error() {
            return true;
        }
        match (self.structure(actual), expected.strip_nullable()) {
            (Ty::List(a), Ty::List(e)) | (Ty::Set(a), Ty::Set(e)) => self.same_shape(&a, e),
            (Ty::Map(ak, av), Ty::Map(ek, ev)) => {
                self.same_shape(&ak, ek) && self.same_shape(&av, ev)
            }
            (_, Ty::List(_) | Ty::Set(_) | Ty::Map(..)) => false,
            _ => true,
        }
    }

    fn widens_to(&self, from: &Ty, to: &Ty) -> bool {
        let mut current = from.clone();
        for _ in 0..DEPTH {
            if current == *to {
                return true;
            }
            match self.parent(&current) {
                Some(next) => current = next,
                None => return false,
            }
        }
        false
    }

    /// The types `ty` widens to, itself first.
    fn ancestors(&self, ty: &Ty) -> Vec<Ty> {
        let mut found = vec![ty.clone()];
        let mut current = ty.clone();
        for _ in 0..DEPTH {
            match self.parent(&current) {
                Some(next) => {
                    found.push(next.clone());
                    current = next;
                }
                None => break,
            }
        }
        found
    }

    /// Whether two field types refine a common type, as `Customer.id` and
    /// `Supplier.id` both refine `Uuid`: passing one for the other is a
    /// sibling mismatch.
    fn siblings(&self, actual: &Ty, expected: &Ty) -> bool {
        let (a, b) = (actual.strip_nullable(), expected.strip_nullable());
        if !matches!(a, Ty::Field(..)) || !matches!(b, Ty::Field(..)) {
            return false;
        }
        let theirs: BTreeSet<Ty> = self.ancestors(b).into_iter().skip(1).collect();
        self.ancestors(a).iter().skip(1).any(|t| theirs.contains(t))
    }
}
