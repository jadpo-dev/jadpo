//! The names and declarations the language itself provides
//! (docs/compiler-runtime.md §2.1).
//!
//! The standard library is declared in Jadpo stub files embedded here
//! (`std/*.jadpo`): its enums, and its namespaces as libraries whose
//! functions have signatures and no bodies, each bound to the Rust
//! function of the same qualified name in `jadpo-value`. The types built
//! into the checker ([`Prim`], the bases and the containers) are listed in
//! Rust, since no declaration can say what they are made of. `std/names.txt`
//! still lists, by name only, what later areas have not declared yet.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use jadpo_syntax::ast::{self, AstNode, Decl, MemberItem};

use jadpo_std::{Builtin, Kind};

use crate::tree;
use crate::types::{BASES, Prim, TypeRef, container_arity};

const NAMES: &str = include_str!("../std/names.txt");

/// The stub files, by name.
const STUBS: [(&str, &str); 11] = [
    ("types.jadpo", include_str!("../std/types.jadpo")),
    ("standard.jadpo", include_str!("../std/standard.jadpo")),
    ("text.jadpo", include_str!("../std/text.jadpo")),
    ("list.jadpo", include_str!("../std/list.jadpo")),
    ("set.jadpo", include_str!("../std/set.jadpo")),
    ("map.jadpo", include_str!("../std/map.jadpo")),
    ("range.jadpo", include_str!("../std/range.jadpo")),
    ("decimal.jadpo", include_str!("../std/decimal.jadpo")),
    ("integer.jadpo", include_str!("../std/integer.jadpo")),
    ("locale.jadpo", include_str!("../std/locale.jadpo")),
    ("temporal.jadpo", include_str!("../std/temporal.jadpo")),
];

/// The standard types' stub file (area 19).
const STANDARD_STUB: &str = "standard.jadpo";

/// A stub operation's signature as kinds (see [`StdNames::signatures`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StubSignature {
    pub name: String,
    /// Each parameter's name, its kind, and whether a call may leave it
    /// out (its type ends in `?`).
    pub params: Vec<(String, Option<Kind>, bool)>,
    pub result: Option<Kind>,
}

/// What kind a type written in a stub is; an option's `?` is left aside.
fn kind_of(ty: &TypeRef) -> Option<Kind> {
    let ty = match ty {
        TypeRef::Nullable(inner) => inner,
        other => other,
    };
    let TypeRef::Path(names, args) = ty else {
        return None;
    };
    let [name] = names.as_slice() else {
        return None;
    };
    match (name.as_str(), args.len()) {
        ("List", 1) => Some(Kind::List),
        ("Set", 1) => Some(Kind::Set),
        ("Map", 2) => Some(Kind::Map),
        ("Function", 2) => Some(Kind::Function),
        ("Number", 0) => Some(Kind::Number),
        (variable, 0)
            if variable.len() == 1 && variable.chars().all(|c| c.is_ascii_uppercase()) =>
        {
            Some(Kind::Any)
        }
        (jadpo_std::INSTANT_RANGE, 0) => Some(Kind::Std(jadpo_std::INSTANT_RANGE)),
        (crate::types::TIME_FORMAT, 0) => Some(Kind::Format),
        // A type written out, `as: Email` (area 19); only in stubs.
        ("Type", 0) => Some(Kind::Type),
        (other, 0) => Builtin::from_name(other).map(Kind::Builtin).or_else(|| {
            jadpo_std::enums()
                .into_iter()
                .find(|(enum_name, _)| *enum_name == other)
                .map(|(enum_name, _)| Kind::Enum(enum_name))
        }),
        _ => None,
    }
}

/// A standard-library operation's signature, as declared in its stub.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Operation {
    pub params: Vec<(String, TypeRef)>,
    pub result: TypeRef,
}

/// A standard type's stub declaration (area 19): what it is built on, as
/// written, and an object's fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandardStub {
    /// The name after `=`: its own for a built-in type, `Enum`, `Object`.
    pub base: String,
    /// An enum's variants, or an object's fields with their types, in
    /// declared order.
    pub members: Vec<(String, Option<TypeRef>)>,
}

#[derive(Debug, Default)]
pub struct StdNames {
    /// Built-in types later areas define, known by name only.
    opaque: BTreeSet<&'static str>,
    /// The standard types' stub declarations (`std/standard.jadpo`).
    standard: BTreeMap<String, StandardStub>,
    /// Enums declared in the stubs, with their variants in order.
    enums: BTreeMap<String, Vec<String>>,
    /// Namespaces declared in the stubs, with their operations.
    declared: BTreeMap<String, BTreeMap<String, Operation>>,
    /// Namespaces known by name only, with their operations' names.
    named: BTreeMap<&'static str, BTreeSet<&'static str>>,
    /// Namespaces the compiler fills, with the members built so far.
    generated: BTreeMap<&'static str, BTreeSet<&'static str>>,
    generic: BTreeSet<&'static str>,
    /// Standard namespaces not reserved yet.
    unreserved: BTreeSet<&'static str>,
}

/// What a lower-case qualifier is in the standard library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Namespace {
    /// A standard-library namespace with a fixed list of operations.
    Standard,
    /// A namespace the compiler fills from the project or platform.
    Generated,
}

impl StdNames {
    pub fn get() -> &'static StdNames {
        static NAMES_TABLE: OnceLock<StdNames> = OnceLock::new();
        NAMES_TABLE.get_or_init(|| {
            let mut table = StdNames::parse(NAMES);
            for (name, stub) in STUBS {
                table.declare(stub, name == STANDARD_STUB);
            }
            for (name, variants) in jadpo_std::enums() {
                table.enums.insert(
                    name.to_string(),
                    variants.iter().map(|variant| variant.to_string()).collect(),
                );
            }
            table
        })
    }

    fn parse(text: &'static str) -> StdNames {
        let mut table = StdNames::default();
        let mut section = "";
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name;
                continue;
            }
            match section {
                "types" => table.opaque.extend(line.split_whitespace()),
                "namespaces" => {
                    if let Some((name, operations)) = line.split_once(':') {
                        table
                            .named
                            .entry(name.trim())
                            .or_default()
                            .extend(operations.split_whitespace());
                    }
                }
                "generated" => {
                    let (name, members) = line.split_once(':').unwrap_or((line, ""));
                    table
                        .generated
                        .entry(name.trim())
                        .or_default()
                        .extend(members.split_whitespace());
                }
                "generic" => table.generic.extend(line.split_whitespace()),
                "unreserved" => table.unreserved.extend(line.split_whitespace()),
                _ => {}
            }
        }
        table
    }

    /// Reads one stub file's enums and libraries, and the standard types'
    /// declarations from theirs.
    fn declare(&mut self, stub: &str, standard: bool) {
        let parse = jadpo_syntax::parse_file(stub);
        let Some(source) = ast::SourceFile::cast(parse.syntax()) else {
            return;
        };
        for decl in source.decls() {
            match decl {
                Decl::TypeDecl(decl) => {
                    let Some(name) = decl.name().as_ref().and_then(tree::name_text) else {
                        continue;
                    };
                    let base = match decl.base().map(|base| TypeRef::from_type(&base)) {
                        Some(TypeRef::Path(names, _)) => names.first().cloned().unwrap_or_default(),
                        _ => String::new(),
                    };
                    let mut members: Vec<(String, Option<TypeRef>)> = decl
                        .member_list()
                        .into_iter()
                        .flat_map(|list| list.member_items())
                        .filter_map(|item| match item {
                            MemberItem::BareMember(variant) => {
                                Some((variant.name().as_ref().and_then(tree::name_text)?, None))
                            }
                            _ => None,
                        })
                        .collect();
                    members.extend(tree::members(decl.member_list()).into_iter().filter_map(
                        |(name, field)| {
                            Some((
                                tree::name_text(&name)?,
                                field.value().map(|value| TypeRef::from_expr(&value)),
                            ))
                        },
                    ));
                    // Only an enum's variants are names a program uses as
                    // values; a built-in type or an object is a type only.
                    if base == "Enum" || base.is_empty() {
                        self.enums.insert(
                            name.clone(),
                            members.iter().map(|(variant, _)| variant.clone()).collect(),
                        );
                    }
                    if standard {
                        self.standard.insert(name, StandardStub { base, members });
                    }
                }
                Decl::LibraryDecl(library) => {
                    let Some(namespace) = library.name().as_ref().and_then(tree::name_text) else {
                        continue;
                    };
                    let operations = self.declared.entry(namespace).or_default();
                    for function in tree::library_functions(&library) {
                        let Some(name) = function.name().as_ref().and_then(tree::name_text) else {
                            continue;
                        };
                        let params = function
                            .param_list()
                            .into_iter()
                            .flat_map(|list| list.params())
                            .filter_map(|param| {
                                Some((
                                    param.name().as_ref().and_then(tree::name_text)?,
                                    TypeRef::from_type(&param.ty()?),
                                ))
                            })
                            .collect();
                        let result = function
                            .ret_type()
                            .and_then(|ret| ret.ty())
                            .map_or(TypeRef::Invalid, |ty| TypeRef::from_type(&ty));
                        operations.insert(name, Operation { params, result });
                    }
                }
                _ => {}
            }
        }
    }

    /// Whether `name` is a type the language provides: built in, declared
    /// in a stub, or named for a later area.
    pub fn is_type(&self, name: &str) -> bool {
        Prim::from_name(name).is_some()
            || BASES.contains(&name)
            || container_arity(name).is_some()
            || self.enums.contains_key(name)
            || self.opaque.contains(name)
            || self.standard.contains_key(name)
    }

    /// The standard types' stub declarations, by name (area 19).
    pub fn standard(&self) -> &BTreeMap<String, StandardStub> {
        &self.standard
    }

    pub fn types(&self) -> impl Iterator<Item = &str> + '_ {
        Prim::ALL
            .iter()
            .map(|prim| prim.name())
            .chain(BASES)
            .chain(["List", "Set", "Map"])
            .chain(self.enums.keys().map(String::as_str))
            .chain(self.opaque.iter().copied())
            .chain(self.standard_objects())
    }

    /// The standard object types (`InstantRange`), which the checker
    /// knows by name, as it knows a later area's types, with their fields
    /// read from the data entry where it reads them.
    fn standard_objects(&self) -> impl Iterator<Item = &str> + '_ {
        self.standard
            .iter()
            .filter(|(_, stub)| stub.base == "Object")
            .map(|(name, _)| name.as_str())
    }

    /// A built-in type a later area defines, known by name only, or a
    /// standard object type.
    pub fn is_opaque(&self, name: &str) -> bool {
        self.opaque.contains(name) || self.standard_objects().any(|object| object == name)
    }

    /// The enums the stubs declare, with their variants.
    pub fn enums(&self) -> &BTreeMap<String, Vec<String>> {
        &self.enums
    }

    pub fn namespace(&self, name: &str) -> Option<Namespace> {
        if self.declared.contains_key(name) || self.named.contains_key(name) {
            Some(Namespace::Standard)
        } else if self.generated.contains_key(name) {
            Some(Namespace::Generated)
        } else {
            None
        }
    }

    /// The operations of a standard namespace, in name order.
    pub fn operations(&self, namespace: &str) -> Vec<&str> {
        let mut found: Vec<&str> = self
            .declared
            .get(namespace)
            .into_iter()
            .flat_map(|operations| operations.keys().map(String::as_str))
            .chain(
                self.named
                    .get(namespace)
                    .into_iter()
                    .flat_map(|operations| operations.iter().copied()),
            )
            .collect();
        found.sort_unstable();
        found
    }

    pub fn has_operation(&self, namespace: &str, operation: &str) -> bool {
        self.operations(namespace).contains(&operation)
    }

    /// The declared signature of `namespace.operation`, if its stub exists.
    pub fn operation(&self, namespace: &str, operation: &str) -> Option<&Operation> {
        self.declared.get(namespace)?.get(operation)
    }

    /// Every operation declared in a stub, as `namespace.operation`, with
    /// its parameters' kinds and its result's, for the test that holds the
    /// stubs to `jadpo-std`'s operation table; `None` where no kind
    /// describes the type written.
    pub fn signatures(&self) -> Vec<StubSignature> {
        self.declared
            .iter()
            .flat_map(|(namespace, operations)| {
                operations
                    .iter()
                    .map(move |(name, operation)| StubSignature {
                        name: format!("{namespace}.{name}"),
                        params: operation
                            .params
                            .iter()
                            .map(|(param, ty)| {
                                let optional = matches!(ty, TypeRef::Nullable(_));
                                (param.clone(), kind_of(ty), optional)
                            })
                            .collect(),
                        result: kind_of(&operation.result),
                    })
            })
            .collect()
    }

    /// Whether `namespace.operation` is known by name only: listed for a
    /// later area, with no stub and nothing to run it yet.
    pub fn name_only(&self, namespace: &str, operation: &str) -> bool {
        self.operation(namespace, operation).is_none()
            && self
                .named
                .get(namespace)
                .is_some_and(|operations| operations.contains(operation))
    }

    /// The first namespace, in name order, with an operation called `name`.
    pub fn namespace_of(&self, operation: &str) -> Option<String> {
        let mut namespaces: Vec<&str> = self
            .declared
            .keys()
            .map(String::as_str)
            .chain(self.named.keys().copied())
            .collect();
        namespaces.sort_unstable();
        namespaces
            .into_iter()
            .find(|namespace| self.has_operation(namespace, operation))
            .map(str::to_string)
    }

    /// Whether `namespace.member` of a generated namespace is built.
    pub fn generated_member(&self, namespace: &str, member: &str) -> bool {
        self.generated
            .get(namespace)
            .is_some_and(|members| members.contains(member))
    }

    /// Every reserved namespace name: standard and generated, but for a
    /// standard namespace whose name stays free until a later chunk.
    pub fn reserved(&self, name: &str) -> bool {
        self.namespace(name).is_some() && !self.unreserved.contains(name)
    }

    pub fn is_generic(&self, name: &str) -> bool {
        self.generic.contains(name)
    }

    pub fn generic_names(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.generic.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_names_file_and_stubs_read() {
        let names = StdNames::get();
        assert!(names.is_type("Int") && names.is_type("FailureKind"));
        assert!(names.is_type("TextFormat") && names.is_type("List"));
        assert_eq!(names.namespace("temporal"), Some(Namespace::Standard));
        assert_eq!(names.namespace("list"), Some(Namespace::Standard));
        // `collection` moved to `list`, `set` and `map`, and stays reserved
        // for the habit error (area 19, question 6).
        assert_eq!(names.namespace("collection"), Some(Namespace::Standard));
        assert!(names.reserved("collection") && names.reserved("list"));
        assert!(names.operations("collection").is_empty());
        assert_eq!(names.namespace("clock"), Some(Namespace::Generated));
        assert_eq!(names.namespace("config"), Some(Namespace::Generated));
        assert!(names.generated_member("clock", "now"));
        assert!(!names.generated_member("clock", "whenever"));
        assert_eq!(names.namespace_of("in_zone").as_deref(), Some("temporal"));
        assert!(names.has_operation("list", "count"));
        assert!(names.has_operation("list", "group_by"));
        assert!(names.has_operation("set", "union") && names.has_operation("map", "keys"));
        assert!(names.reserved("set") && names.reserved("map") && names.reserved("range"));
        assert!(names.is_generic("utils"));
        assert_eq!(
            names.enums().get("UuidVersion").map(Vec::len),
            Some(8),
            "v1 to v8"
        );
    }

    #[test]
    fn every_stub_parses_cleanly() {
        for (name, stub) in STUBS {
            let parse = jadpo_syntax::parse_file(stub);
            assert!(parse.errors().is_empty(), "{name}: {:#?}", parse.errors());
        }
    }

    #[test]
    fn stub_signatures_are_read() {
        let count = StdNames::get()
            .operation("list", "count")
            .expect("list.count");
        assert_eq!(count.params.len(), 1);
        assert_eq!(count.result, TypeRef::Path(vec!["Int".into()], Vec::new()));
    }
}
