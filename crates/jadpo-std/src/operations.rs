//! Every standard-library operation, typed (docs/compiler-runtime.md §2.1):
//! its parameters with their kinds, its result, and the one Rust function in
//! `jadpo-value` that runs it for both executors. The Jadpo stubs in
//! `jadpo-check` declare the same operations for programs to call, and a
//! test holds the two to the same arity and kinds; `jadpo-value` matches on
//! [`Op`] exhaustively, so an operation listed here without an
//! implementation does not compile.

use crate::{Builtin, Rounding};

/// What a parameter or a result is, as far as an operation cares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A type variable (`T`): any type, fixed by the arguments at each call.
    Any,
    /// `List<T>`.
    List,
    /// An inline function, `item -> expression` (area 3).
    Function,
    /// A value built on `Int` or `Decimal`.
    Number,
    /// A value built on this built-in type.
    Builtin(Builtin),
    /// A value of a standard-library enum, by its name.
    Enum(&'static str),
}

/// One parameter: its name, which a call may give it by (area 3), and its
/// kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Param {
    pub name: &'static str,
    pub kind: Kind,
}

/// Which operation, for an exhaustive match in each executor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Op {
    CollectionContains,
    CollectionCount,
    CollectionFilter,
    CollectionMap,
    DecimalDivide,
    IntegerDivide,
}

/// One standard-library operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operation {
    pub op: Op,
    pub namespace: &'static str,
    pub name: &'static str,
    pub params: &'static [Param],
    pub result: Kind,
    /// The smallest result it can give, when its result type can't say:
    /// `collection.count` never gives less than 0, though its result is a
    /// plain `Int`. Proving a construction safe reads it (area 4,
    /// question 6).
    pub at_least: Option<i64>,
    /// The Rust function that runs it, by its path from `jadpo-value`'s
    /// root: what generated code calls.
    pub rust: &'static str,
}

impl Operation {
    /// `namespace.name`.
    pub fn qualified(&self) -> String {
        format!("{}.{}", self.namespace, self.name)
    }
}

const fn param(name: &'static str, kind: Kind) -> Param {
    Param { name, kind }
}

const ROUNDING: Kind = Kind::Enum(Rounding::NAME);

/// Every operation, sorted by namespace and name.
pub const OPERATIONS: &[Operation] = &[
    Operation {
        op: Op::CollectionContains,
        namespace: "collection",
        name: "contains",
        params: &[param("items", Kind::List), param("item", Kind::Any)],
        result: Kind::Builtin(Builtin::Bool),
        at_least: None,
        rust: "collection::contains",
    },
    Operation {
        op: Op::CollectionCount,
        namespace: "collection",
        name: "count",
        params: &[param("items", Kind::List)],
        result: Kind::Builtin(Builtin::Int),
        at_least: Some(0),
        rust: "collection::count",
    },
    Operation {
        op: Op::CollectionFilter,
        namespace: "collection",
        name: "filter",
        params: &[param("items", Kind::List), param("keep", Kind::Function)],
        result: Kind::List,
        at_least: None,
        rust: "collection::filter",
    },
    Operation {
        op: Op::CollectionMap,
        namespace: "collection",
        name: "map",
        params: &[param("items", Kind::List), param("each", Kind::Function)],
        result: Kind::List,
        at_least: None,
        rust: "collection::map",
    },
    Operation {
        op: Op::DecimalDivide,
        namespace: "decimal",
        name: "divide",
        params: &[
            param("dividend", Kind::Number),
            param("divisor", Kind::Number),
            param("rounding", ROUNDING),
        ],
        result: Kind::Builtin(Builtin::Decimal),
        at_least: None,
        rust: "decimal::divide",
    },
    Operation {
        op: Op::IntegerDivide,
        namespace: "integer",
        name: "divide",
        params: &[
            param("dividend", Kind::Builtin(Builtin::Int)),
            param("divisor", Kind::Builtin(Builtin::Int)),
            param("rounding", ROUNDING),
        ],
        result: Kind::Builtin(Builtin::Int),
        at_least: None,
        rust: "integer::divide",
    },
];

/// The operation `namespace.name`, if the standard library has one.
pub fn operation(namespace: &str, name: &str) -> Option<&'static Operation> {
    OPERATIONS
        .iter()
        .find(|operation| operation.namespace == namespace && operation.name == name)
}

impl Op {
    /// The table's entry for this operation.
    pub fn operation(self) -> &'static Operation {
        OPERATIONS
            .iter()
            .find(|operation| operation.op == self)
            .unwrap_or(&OPERATIONS[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_and_each_op_listed_once() {
        let names: Vec<String> = OPERATIONS.iter().map(Operation::qualified).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names, sorted);
        for operation in OPERATIONS {
            assert_eq!(operation.op.operation(), operation);
        }
        assert_eq!(
            operation("collection", "count").map(|op| op.at_least),
            Some(Some(0))
        );
        assert_eq!(operation("temporal", "in_zone"), None);
    }
}
