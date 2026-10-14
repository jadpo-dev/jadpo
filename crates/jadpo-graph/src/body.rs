//! Typed bodies (docs/compiler-runtime.md §2.3 and §2.5): what each
//! function, action, fixture, test and seed does, checked and resolved, for
//! both executors to walk and for the graph's edges, the language server
//! and the review to read.
//!
//! A body is an arena of statements and expressions, its sites, numbered in
//! source order: a site's number is its index, the number the executors pass
//! to `Ctx::enter` and `Ctx::fault`, and the number its `site` node carries.
//! Every site has its source range, its line and its type: a literal's type
//! is the type it goes into. Calls name what they call by identity key, a
//! standard-library operation by the table's [`Op`], and constructions say
//! which rules they can break and the failure each rule maps to. Locals
//! are numbered too, parameters first. Branches (an `if`'s two ways, each
//! arm of a `match`) are numbered in source order for path numbers and
//! coverage (area 16). A construction an `if` condition proves records the
//! comparisons that prove it.

use jadpo_std::{Num, Op};

use crate::{Key, TypeRef};

/// A statement or expression: its number in its body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SiteId(pub u32);

/// A parameter, variable, loop item, binding or inline parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocalId(pub u32);

/// One way through an `if` or a `match`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BranchId(pub u32);

/// A callable's typed body.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Body {
    /// Parameters first, in order, then every other local in source order.
    pub locals: Vec<Local>,
    pub params: Vec<LocalId>,
    /// Every statement and expression, in source order.
    pub sites: Vec<Site>,
    /// The body's statements, in order.
    pub block: Vec<SiteId>,
    pub branches: Vec<Branch>,
    /// The failures that can leave it.
    pub failures: Vec<Key>,
    /// Whether something else may leave it that the checker can't name
    /// yet: an entity operation's failure (area 6), or a mistake already
    /// reported.
    pub open: bool,
    /// What each name written in it refers to, by byte range, in source
    /// order.
    pub names: Vec<Name>,
    /// Each construction whose rules an `if` condition proves, in source
    /// order (area 2, "Conditions as proof").
    pub proofs: Vec<Proof>,
}

impl Body {
    pub fn site(&self, id: SiteId) -> &Site {
        &self.sites[id.0 as usize]
    }

    pub fn local(&self, id: LocalId) -> &Local {
        &self.locals[id.0 as usize]
    }

    /// What the name covering byte `offset` refers to.
    pub fn name_at(&self, offset: u32) -> Option<&Name> {
        self.names
            .iter()
            .find(|name| name.start <= offset && offset <= name.end)
    }

    /// Every callee, in source order, with the site that calls it.
    pub fn calls(&self) -> impl Iterator<Item = (SiteId, &Callee)> + '_ {
        self.sites
            .iter()
            .enumerate()
            .filter_map(|(i, site)| match &site.node {
                Node::Expr(Expr::Call { callee, .. } | Expr::Build { callee, .. }) => {
                    Some((SiteId(i as u32), callee))
                }
                _ => None,
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Local {
    pub name: String,
    pub ty: TypeRef,
    pub kind: LocalKind,
    /// Where its name is written.
    pub start: u32,
    pub end: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LocalKind {
    Param,
    Var,
    VarMut,
    /// A `for` loop's item.
    Item,
    /// A name a `match` arm binds.
    Binding,
    /// An inline function's parameter.
    Inline,
}

impl LocalKind {
    /// What it is, in the reader's words.
    pub fn word(self) -> &'static str {
        match self {
            LocalKind::Param => "parameter",
            LocalKind::Var | LocalKind::VarMut => "variable",
            LocalKind::Item => "loop item",
            LocalKind::Binding => "binding",
            LocalKind::Inline => "inline parameter",
        }
    }
}

/// One statement or expression.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Site {
    pub start: u32,
    pub end: u32,
    /// The 1-based line it starts on.
    pub line: u32,
    /// An expression's type; `Unit` for a statement.
    pub ty: TypeRef,
    pub node: Node,
}

impl Site {
    /// Whether the runtime can name it in a frame, a failure or a fault:
    /// a call, a construction that can fail, `reject`, `propagate`, or
    /// arithmetic, which can overflow. These are the graph's `site` nodes,
    /// whose lines `jadpo incident` reads.
    pub fn is_fault_site(&self) -> bool {
        match &self.node {
            Node::Expr(Expr::Call { callee, .. }) => *callee != Callee::Unknown,
            Node::Expr(Expr::Build { fallible, .. }) => {
                matches!(fallible, Fallible::Construction { .. })
            }
            Node::Expr(Expr::Reject { .. } | Expr::Propagate { .. }) => true,
            Node::Expr(Expr::Binary { op, .. }) => op.can_fault(),
            Node::Expr(Expr::Unary { op, .. }) => *op == UnaryOp::Negate,
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Node {
    Stmt(Stmt),
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Stmt {
    Var {
        local: Option<LocalId>,
        value: Option<SiteId>,
    },
    /// `name = value`; `local` is `None` when the target isn't a `var mut`,
    /// which is reported.
    Assign {
        local: Option<LocalId>,
        value: Option<SiteId>,
    },
    Return {
        value: Option<SiteId>,
    },
    /// An `if`, its `else if`s and its `else`, as one flat list however
    /// long the chain.
    If {
        /// The `if` and each `else if`, in order.
        arms: Vec<IfArm>,
        /// The `else` block, empty without one.
        otherwise: Vec<SiteId>,
        /// The `else`, or the way past the chain when no condition holds.
        otherwise_branch: BranchId,
    },
    For {
        item: Option<LocalId>,
        list: Option<SiteId>,
        body: Vec<SiteId>,
    },
    Emit {
        value: Option<SiteId>,
    },
    Assert {
        value: Option<SiteId>,
        /// `assert call(...) fails Failure`: the failure it expects.
        fails: Option<TypeRef>,
    },
    Expr {
        value: Option<SiteId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Expr {
    /// A literal; its site's type is the type it goes into.
    Literal(Literal),
    Local(LocalId),
    /// `value.field`, or a variant's data's field.
    Field {
        receiver: SiteId,
        field: Key,
    },
    /// A call, with its arguments in parameter order: a standard-library
    /// operation's named arguments in their parameters' places.
    Call {
        callee: Callee,
        args: Vec<Option<SiteId>>,
        fallible: Fallible,
    },
    /// An object or a variant built with its fields: `Note { ... }`,
    /// `Shade.tinted { ... }`, or a plain `Shade.plain`.
    Build {
        callee: Callee,
        fields: Vec<(Key, SiteId)>,
        fallible: Fallible,
    },
    List {
        items: Vec<SiteId>,
    },
    Binary {
        op: BinaryOp,
        lhs: SiteId,
        rhs: SiteId,
    },
    Unary {
        op: UnaryOp,
        operand: SiteId,
    },
    Match {
        subject: Option<SiteId>,
        /// A `match` on a call's outcome: `success(...)` and failure arms.
        outcome: bool,
        arms: Vec<Arm>,
    },
    /// `attempt call(...)`, passing its failures on, or a construction
    /// with the failure each rule it can break maps to.
    Attempt {
        inner: Option<SiteId>,
        /// A construction's arms, as written; empty for a call.
        arms: Vec<RuleArm>,
    },
    Reject {
        failure: Option<Key>,
        fields: Vec<(Key, SiteId)>,
    },
    /// Passes the failure its arm handles on.
    Propagate {
        failure: Option<Key>,
    },
    /// `item -> expression`, given to a collection operation.
    Inline {
        param: Option<LocalId>,
        body: Option<SiteId>,
    },
    /// Nothing to run: a mistake already reported, or something a later
    /// area builds.
    Unchecked,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Literal {
    Number {
        value: Num,
        decimal: bool,
    },
    /// A duration as written: `15m`.
    Duration(String),
    Text(String),
    Bool(bool),
    None,
}

impl Literal {
    /// As Jadpo writes it: `25`, `1.50`, `"text"`, `15m`, `none`.
    pub fn written(&self) -> String {
        match self {
            Literal::Number { value, .. } => value.to_string(),
            Literal::Duration(written) => written.clone(),
            Literal::Text(text) => format!("{text:?}"),
            Literal::Bool(value) => value.to_string(),
            Literal::None => "none".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Or,
    And,
    Equal,
    NotEqual,
    In,
    Matches,
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

impl BinaryOp {
    pub fn from_token(token: &str) -> Option<BinaryOp> {
        Some(match token {
            "or" => BinaryOp::Or,
            "and" => BinaryOp::And,
            "==" => BinaryOp::Equal,
            "!=" => BinaryOp::NotEqual,
            "in" => BinaryOp::In,
            "matches" => BinaryOp::Matches,
            "<" => BinaryOp::Less,
            "<=" => BinaryOp::LessEqual,
            ">" => BinaryOp::Greater,
            ">=" => BinaryOp::GreaterEqual,
            "+" => BinaryOp::Add,
            "-" => BinaryOp::Subtract,
            "*" => BinaryOp::Multiply,
            "/" => BinaryOp::Divide,
            "%" => BinaryOp::Remainder,
            _ => return None,
        })
    }

    /// Whether it can fault at runtime: arithmetic can overflow.
    pub fn can_fault(self) -> bool {
        matches!(
            self,
            BinaryOp::Add
                | BinaryOp::Subtract
                | BinaryOp::Multiply
                | BinaryOp::Divide
                | BinaryOp::Remainder
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Not,
    Negate,
}

/// What a call or a build reaches.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Callee {
    Function(Key),
    Action(Key),
    Query(Key),
    Fixture(Key),
    LibraryFunction(Key),
    Std(Op),
    /// `Type(value)`, `Owner.field(value)`: re-checks a value against the
    /// type's rules.
    Construct(TypeRef),
    /// `Type { ... }`.
    Object(Key),
    /// `Enum.variant`, with or without its data.
    Variant(Key),
    /// An entity's operation (areas 5 and 6).
    Entity {
        entity: Key,
        operation: String,
    },
    /// Nothing that can be called: already reported.
    Unknown,
}

impl Callee {
    /// The project node it reaches, if one.
    pub fn key(&self) -> Option<&Key> {
        match self {
            Callee::Function(key)
            | Callee::Action(key)
            | Callee::Query(key)
            | Callee::Fixture(key)
            | Callee::LibraryFunction(key)
            | Callee::Object(key)
            | Callee::Variant(key) => Some(key),
            Callee::Entity { entity, .. } => Some(entity),
            Callee::Std(_) | Callee::Construct(_) | Callee::Unknown => None,
        }
    }

    /// Whether it is another callable of the project: a `calls` edge.
    pub fn is_callable(&self) -> bool {
        matches!(
            self,
            Callee::Function(_)
                | Callee::Action(_)
                | Callee::Query(_)
                | Callee::Fixture(_)
                | Callee::LibraryFunction(_)
        )
    }
}

/// What a call or a construction can do besides give its result.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Fallible {
    Never,
    /// It fails with these declared failures, as its `fails` list names
    /// them.
    Fails(Vec<Key>),
    /// A construction whose value is only known at runtime: it fails when
    /// the value breaks one of these rules, by key (`max`), with the
    /// failure its `attempt` maps that rule to.
    Construction {
        rules: Vec<String>,
    },
    /// Not known yet (an entity's operations, area 6), or already reported.
    Unknown,
}

/// One arm of an `attempt` on a construction (area 2, "Construction
/// failures by rule"): the rule it maps, or every rule without an arm of
/// its own (`invalid:`), and the failure raised when that rule breaks.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RuleArm {
    /// The rule's key, such as `max`; `None` for `invalid:`.
    pub rule: Option<String>,
    pub failure: Key,
    /// The fields given to the failure, as `reject` gives them.
    pub fields: Vec<(Key, SiteId)>,
}

impl RuleArm {
    /// The arm that maps the rule with key `rule`: its own, or `invalid:`.
    pub fn for_rule<'a>(arms: &'a [RuleArm], rule: &str) -> Option<&'a RuleArm> {
        arms.iter()
            .find(|arm| arm.rule.as_deref() == Some(rule))
            .or_else(|| arms.iter().find(|arm| arm.rule.is_none()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Arm {
    pub pattern: Pattern,
    pub binding: Option<LocalId>,
    pub body: ArmBody,
    pub branch: BranchId,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Pattern {
    Variant(Key),
    True,
    False,
    Some,
    None,
    Literal(Literal),
    Wildcard,
    /// `success(value)` on a call's outcome.
    Success,
    /// A failure the call can give.
    Failure(Key),
    /// Already reported.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ArmBody {
    Value(SiteId),
    Block(Vec<SiteId>),
    Empty,
}

/// A construction an `if` condition proves at least one rule of (area 2,
/// "Conditions as proof"): the construction, and the comparisons whose
/// bounds prove it, in source order. A rule they prove takes no arm, and
/// a construction they prove entirely takes no `attempt` and runs no
/// check.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Proof {
    pub site: SiteId,
    pub by: Vec<SiteId>,
}

/// One condition of an `if` chain and the block it runs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IfArm {
    pub condition: Option<SiteId>,
    pub then: Vec<SiteId>,
    pub branch: BranchId,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Branch {
    /// The `if` or `match` it is a way through.
    pub of: SiteId,
    pub start: u32,
    pub end: u32,
    pub line: u32,
}

/// What a name written in a body refers to.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Name {
    pub start: u32,
    pub end: u32,
    pub refers: Refers,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Refers {
    Local(LocalId),
    /// A node: a type, field, variant, callable, library or domain. `uses`
    /// is whether the body names it as a type it uses, which the graph's
    /// `uses` edges list.
    Node {
        key: Key,
        uses: bool,
    },
}
