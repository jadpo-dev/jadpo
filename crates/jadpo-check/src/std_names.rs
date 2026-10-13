//! The names the language itself declares, read from `std/names.txt`:
//! built-in types, standard-library namespaces and their operations, the
//! namespaces the compiler fills in, and the names too generic for a domain
//! or library.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

const NAMES: &str = include_str!("../std/names.txt");

#[derive(Debug, Default)]
pub struct StdNames {
    types: BTreeSet<&'static str>,
    namespaces: BTreeMap<&'static str, BTreeSet<&'static str>>,
    generated: BTreeSet<&'static str>,
    generic: BTreeSet<&'static str>,
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
        NAMES_TABLE.get_or_init(|| StdNames::parse(NAMES))
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
                "types" => table.types.extend(line.split_whitespace()),
                "namespaces" => {
                    if let Some((name, operations)) = line.split_once(':') {
                        table
                            .namespaces
                            .entry(name.trim())
                            .or_default()
                            .extend(operations.split_whitespace());
                    }
                }
                "generated" => table.generated.extend(line.split_whitespace()),
                "generic" => table.generic.extend(line.split_whitespace()),
                _ => {}
            }
        }
        table
    }

    pub fn is_type(&self, name: &str) -> bool {
        self.types.contains(name)
    }

    pub fn types(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.types.iter().copied()
    }

    pub fn namespace(&self, name: &str) -> Option<Namespace> {
        if self.namespaces.contains_key(name) {
            Some(Namespace::Standard)
        } else if self.generated.contains(name) {
            Some(Namespace::Generated)
        } else {
            None
        }
    }

    /// The operations of a standard namespace.
    pub fn operations(&self, namespace: &str) -> impl Iterator<Item = &'static str> + '_ {
        self.namespaces
            .get(namespace)
            .into_iter()
            .flat_map(|operations| operations.iter().copied())
    }

    pub fn has_operation(&self, namespace: &str, operation: &str) -> bool {
        self.namespaces
            .get(namespace)
            .is_some_and(|operations| operations.contains(operation))
    }

    /// The first namespace, in name order, with an operation called `name`.
    pub fn namespace_of(&self, operation: &str) -> Option<&'static str> {
        self.namespaces
            .iter()
            .find(|(_, operations)| operations.contains(operation))
            .map(|(name, _)| *name)
    }

    /// Every reserved namespace name: standard and generated.
    pub fn reserved(&self, name: &str) -> bool {
        self.namespace(name).is_some()
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
    fn the_names_file_reads() {
        let names = StdNames::get();
        assert!(names.is_type("Int") && names.is_type("FailureKind"));
        assert_eq!(names.namespace("temporal"), Some(Namespace::Standard));
        assert_eq!(names.namespace("clock"), Some(Namespace::Generated));
        assert_eq!(names.namespace_of("in_zone"), Some("temporal"));
        assert!(names.has_operation("collection", "count"));
        assert!(names.is_generic("utils"));
    }
}
