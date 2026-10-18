//! Rules across an object's fields (area 2, "Rules across fields"), as
//! plain data both executors and the codecs run: each rule's name and its
//! condition, lowered once by `jadpo-plan` from the rule's typed body.
//!
//! A condition reads only its object's fields, literals, variants,
//! operators, `match` and the standard operations that always give a
//! result from their arguments, so running one needs nothing but the
//! object's value: no store, clock, setting or key. Its locals are numbered
//! as its typed body numbers them: the object's fields first, in the order
//! the type declares them, then each name a `match` arm binds.
//!
//! A standard object type's rules across its parts (area 19, chunk 5.5t2)
//! are the same plain data, each a [`RuleExpr::Standard`] naming the check
//! `standard::across` holds for it, so a standard type's rule is run, named
//! and reported exactly as an application's is.

use crate::{Constant, Op, Rounding};

/// One rule: its name, the most locals its condition uses, and the
/// condition.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CrossRule {
    /// As written after `rule`: what an `attempt`'s arm and a failure's
    /// report call it.
    pub name: String,
    pub locals: u32,
    pub check: RuleExpr,
}

/// A condition, or a part of one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RuleExpr {
    /// A field, by its place among the object's fields, or a name a
    /// `match` arm binds after them.
    Local(u32),
    /// A literal or a plain variant, decoded for the type it goes into.
    Constant(Constant),
    /// `value.field`.
    Field {
        receiver: Box<RuleExpr>,
        name: String,
    },
    /// `[a, b]`, for `in`.
    List(Vec<RuleExpr>),
    /// `rounding` is how `*` and `/` round an amount of money, its type's
    /// `rounding:`.
    Binary {
        op: RuleOp,
        lhs: Box<RuleExpr>,
        rhs: Box<RuleExpr>,
        rounding: Rounding,
    },
    Not(Box<RuleExpr>),
    Negate(Box<RuleExpr>),
    /// A standard operation, with its arguments in parameter order.
    Std {
        op: Op,
        args: Vec<RuleExpr>,
    },
    Match {
        subject: Box<RuleExpr>,
        arms: Vec<RuleArm>,
    },
    /// A standard object type's rule across its parts (area 19, chunk
    /// 5.5t2), by the type's name and the rule's: its one check is
    /// `standard::across`'s, run over the object's fields, which are its
    /// parts in their declared order.
    Standard { ty: String, rule: String },
}

/// One arm of a `match` in a condition.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RuleArm {
    pub pattern: RulePattern,
    /// The local the arm binds: `some(count)`'s `count`.
    pub binding: Option<u32>,
    pub body: RuleExpr,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RulePattern {
    Some,
    None,
    True,
    False,
    /// A variant, by its place among its enum's variants.
    Variant(u32),
    Literal(Constant),
    Wildcard,
}

/// The operators a condition uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RuleOp {
    Or,
    And,
    Equal,
    NotEqual,
    In,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}
