//! Types as the graph holds them (§2.3: a typed shape, not string labels):
//! a declared type, entity or field by its identity key, so a reference
//! survives a rename the registry records, a built-in type, or a
//! standard-library type by name. The manifest prints each as Jadpo writes
//! it, with the names the keys have now.

use std::fmt;

use jadpo_std::{Builtin, Rules};

use crate::Key;

/// A type.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TypeRef {
    Builtin(Builtin),
    /// A declared type or entity: `type Count`, `entity Todo`.
    Node(Key),
    /// A field's type, a refinement of its declared type: `field Note.id`.
    Field(Key),
    /// The data a payload variant carries, bound by a `match` arm:
    /// `variant PaymentOutcome.paid`.
    Variant(Key),
    /// A standard-library type by name, with its arguments: an enum such as
    /// `Rounding`, or a type a later area defines, such as `Zone`.
    Std(String, Vec<TypeRef>),
    List(Box<TypeRef>),
    Set(Box<TypeRef>),
    Map(Box<TypeRef>, Box<TypeRef>),
    Nullable(Box<TypeRef>),
    /// The type of `none`.
    None,
    /// Not resolved: a mistake already reported, as written.
    Written(String),
}

impl TypeRef {
    /// Every key it names, outermost first.
    pub fn keys(&self) -> Vec<&Key> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect<'a>(&'a self, out: &mut Vec<&'a Key>) {
        match self {
            TypeRef::Node(key) | TypeRef::Field(key) | TypeRef::Variant(key) => out.push(key),
            TypeRef::Std(_, args) => args.iter().for_each(|arg| arg.collect(out)),
            TypeRef::List(inner) | TypeRef::Set(inner) | TypeRef::Nullable(inner) => {
                inner.collect(out)
            }
            TypeRef::Map(key, value) => {
                key.collect(out);
                value.collect(out);
            }
            TypeRef::Builtin(_) | TypeRef::None | TypeRef::Written(_) => {}
        }
    }

    /// As Jadpo writes it, naming each key by `name`: `List<Tag>`,
    /// `Note.id?`.
    pub fn render(&self, name: &dyn Fn(&Key) -> String) -> String {
        match self {
            TypeRef::Builtin(builtin) => builtin.name().to_string(),
            TypeRef::Node(key) | TypeRef::Field(key) | TypeRef::Variant(key) => name(key),
            TypeRef::Std(written, args) if args.is_empty() => written.clone(),
            TypeRef::Std(written, args) => {
                let args: Vec<String> = args.iter().map(|arg| arg.render(name)).collect();
                format!("{written}<{}>", args.join(", "))
            }
            TypeRef::List(inner) => format!("List<{}>", inner.render(name)),
            TypeRef::Set(inner) => format!("Set<{}>", inner.render(name)),
            TypeRef::Map(key, value) => {
                format!("Map<{}, {}>", key.render(name), value.render(name))
            }
            TypeRef::Nullable(inner) => format!("{}?", inner.render(name)),
            TypeRef::None => "none".to_string(),
            TypeRef::Written(written) => written.clone(),
        }
    }
}

impl TypeRef {
    /// Its canonical text (chunk 6.8): as Jadpo writes it, naming each key
    /// by the key itself, which survives a rename, never by a name it has
    /// now. What a release keeps for a later one to compare against, a
    /// batch run's shape, is built from it rather than from `Debug`.
    pub fn canonical(&self) -> String {
        self.render(&|key| key.as_str().to_string())
    }
}

impl fmt::Display for TypeRef {
    /// With each key's born name as code writes it; the manifest names
    /// keys as they are now.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render(&|key| key.written().to_string()))
    }
}

/// A declaration's rules, by name and canonical value, in the order area 2
/// lists them: what the manifest and the language server print.
pub fn rule_pairs(rules: &Rules) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    if let Some(min) = rules.min {
        out.push(("min", min.to_string()));
    }
    if let Some(max) = rules.max {
        out.push(("max", max.to_string()));
    }
    if let Some(min) = rules.min_duration {
        out.push(("min", min.to_string()));
    }
    if let Some(max) = rules.max_duration {
        out.push(("max", max.to_string()));
    }
    if let Some(min) = rules.min_length {
        out.push(("min_length", min.to_string()));
    }
    if let Some(max) = rules.max_length {
        out.push(("max_length", max.to_string()));
    }
    if let Some(pattern) = &rules.pattern {
        out.push(("pattern", pattern.clone()));
    }
    if let Some(format) = rules.format {
        out.push(("format", format.name().to_string()));
    }
    if let Some(scale) = rules.scale {
        out.push(("scale", scale.to_string()));
    }
    if let Some(version) = rules.version {
        out.push(("version", version.digit().to_string()));
    }
    if let Some(currency) = rules.currency {
        out.push((
            "currency",
            jadpo_std::money::code(currency).unwrap_or("?").to_string(),
        ));
    }
    if let Some(rounding) = rules.rounding {
        out.push(("rounding", rounding.name().to_string()));
    }
    if let Some(from) = rules.rate_from {
        out.push((
            "from",
            jadpo_std::money::code(from).unwrap_or("?").to_string(),
        ));
    }
    if let Some(to) = rules.rate_to {
        out.push(("to", jadpo_std::money::code(to).unwrap_or("?").to_string()));
    }
    if let Some(dimensions) = rules.dimensions {
        out.push(("dimensions", dimensions.to_string()));
    }
    if let Some(distance) = rules.distance {
        out.push(("distance", distance.name().to_string()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Kind;

    #[test]
    fn types_print_as_written() {
        let ty = TypeRef::Map(
            Box::new(TypeRef::Node(Key::new(Kind::Type, "Tag"))),
            Box::new(TypeRef::Nullable(Box::new(TypeRef::Field(Key::new(
                Kind::Field,
                "Note.id",
            ))))),
        );
        assert_eq!(ty.to_string(), "Map<Tag, Note.id?>");
        assert_eq!(
            ty.render(&|key| format!("{}!", key.name())),
            "Map<Tag!, Note.id!?>"
        );
        assert_eq!(ty.keys().len(), 2);
        assert_eq!(
            TypeRef::List(Box::new(TypeRef::Builtin(Builtin::Int))).to_string(),
            "List<Int>"
        );
    }
}
