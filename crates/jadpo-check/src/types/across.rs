//! Rules across an object type's fields (area 2, "Rules across fields"):
//! each rule as the type tables hold it, without positions, and the
//! declaration's own checks: where a rule may be written and what it may
//! be called.
//!
//! A rule's condition is kept as a [`Cond`], the part of it the proof of a
//! construction reads: names, fields, literals and the operators between
//! them, with anything else (a call, a `match`) kept as [`Cond::Other`],
//! which no condition proves. Its typed body, which both executors run, is
//! checked as a callable's is ([`super::body`]).

use jadpo_diag::{Diagnostic, rule};
use jadpo_syntax::ast::{self, AstNode, Expr};
use jadpo_syntax::{SyntaxKind, SyntaxNode};

use super::rules::{Const, Num};
use super::shape::FieldsBase;
use crate::tree;

/// The word an `attempt` keeps for every rule without an arm.
const INVALID: &str = "invalid";

/// A rule's condition, as its proof reads it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Cond {
    /// A name written bare: one of the type's fields, or anything else,
    /// which the rule's body reports.
    Name(String),
    /// `value.field`.
    Field(Box<Cond>, String),
    Number(Num),
    Text(String),
    Bool(bool),
    /// An operator as written: `and`, `<`, `+`.
    Binary(String, Box<Cond>, Box<Cond>),
    Not(Box<Cond>),
    Negate(Box<Cond>),
    /// A call, a `match`, a list: never proved.
    Other,
}

impl Cond {
    pub fn from_expr(expr: &Expr) -> Cond {
        let boxed =
            |expr: Option<Expr>| Box::new(expr.map_or(Cond::Other, |e| Cond::from_expr(&e)));
        match expr {
            Expr::ParenExpr(paren) => paren.expr().map_or(Cond::Other, |e| Cond::from_expr(&e)),
            Expr::Literal(_) => match Const::from_expr(expr) {
                Const::Number { value, .. } => Cond::Number(value),
                Const::Str(text) => Cond::Text(text),
                Const::Bool(value) => Cond::Bool(value),
                _ => Cond::Other,
            },
            Expr::PrefixExpr(prefix) => match prefix.op().map(|token| token.kind()) {
                Some(SyntaxKind::MINUS) => match Const::from_expr(expr) {
                    Const::Number { value, .. } => Cond::Number(value),
                    _ => Cond::Negate(boxed(prefix.expr())),
                },
                Some(SyntaxKind::NOT_KW) => Cond::Not(boxed(prefix.expr())),
                _ => Cond::Other,
            },
            Expr::BinExpr(binary) => match binary.op() {
                Some(op) => Cond::Binary(
                    op.text().to_string(),
                    boxed(binary.lhs()),
                    boxed(binary.rhs()),
                ),
                None => Cond::Other,
            },
            Expr::NameRef(name) => tree::name_ref_text(name).map_or(Cond::Other, Cond::Name),
            Expr::FieldExpr(field) => match field.name_ref().as_ref().and_then(tree::name_ref_text)
            {
                Some(name) => Cond::Field(boxed(field.expr()), name),
                None => Cond::Other,
            },
            _ => Cond::Other,
        }
    }
}

/// One rule as its type holds it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CrossDef {
    pub name: String,
    pub condition: Cond,
    /// The condition as written, on one line: `ends > starts`.
    pub written: String,
}

impl CrossDef {
    pub fn from_member(member: &ast::RuleMember) -> Option<CrossDef> {
        let name = member.name().as_ref().and_then(tree::name_text)?;
        let condition = member.condition();
        Some(CrossDef {
            name,
            written: condition
                .as_ref()
                .map(|c| spaced(c.syntax()))
                .unwrap_or_default(),
            condition: condition.map_or(Cond::Other, |c| Cond::from_expr(&c)),
        })
    }

    /// The rule as its type writes it: `rule ends_after_start: ends >
    /// starts`.
    pub fn shown(&self) -> String {
        format!("rule {}: {}", self.name, self.written)
    }
}

/// A node as written, on one line: its tokens, one space wherever the
/// source has spacing, a line break or a comment between them.
pub(crate) fn spaced(node: &SyntaxNode) -> String {
    let mut out = String::new();
    let mut gap = false;
    for token in node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
    {
        if token.kind().is_trivia() {
            gap = !out.is_empty();
            continue;
        }
        if gap {
            out.push(' ');
            gap = false;
        }
        out.push_str(token.text());
    }
    out
}

/// The rules written in a block, in order.
pub fn rule_members(list: Option<ast::MemberList>) -> Vec<ast::RuleMember> {
    list.into_iter()
        .flat_map(|list| list.member_items())
        .filter_map(|item| match item {
            ast::MemberItem::RuleMember(rule) => Some(rule),
            _ => None,
        })
        .collect()
}

/// Whether a declaration with fields may name rules across them: only an
/// `Object`, `Input` or `Output`.
pub fn has_rules(base: FieldsBase) -> bool {
    base.is_area_two()
}

/// Where a rule is written: the type declaration whose own block holds
/// it, when that is an `Object`, `Input` or `Output`.
fn owner(rule: &ast::RuleMember) -> Option<ast::TypeDecl> {
    let list = rule.syntax().parent().and_then(ast::MemberList::cast)?;
    let decl = list.syntax().parent().and_then(ast::TypeDecl::cast)?;
    let base = decl.base().map(|base| tree::written(base.syntax()))?;
    FieldsBase::from_base(&base)
        .is_some_and(has_rules)
        .then_some(decl)
}

/// `type.rule_outside_object` for every rule written anywhere but an
/// `Object`'s, `Input`'s or `Output`'s own block, and
/// `type.rule_name_taken` for a rule named like a field, an earlier rule
/// or `invalid`. `fields` gives a type's fields by its name.
pub fn rule_problems(
    path: &str,
    source: &SyntaxNode,
    fields: &dyn Fn(&str) -> Vec<String>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut seen: Vec<(ast::TypeDecl, Vec<(String, jadpo_diag::Span)>)> = Vec::new();
    for rule in source.descendants().filter_map(ast::RuleMember::cast) {
        let head = head_span(path, &rule);
        let Some(decl) = owner(&rule) else {
            out.push(outside_object(&rule, head));
            continue;
        };
        let Some(name) = rule.name().as_ref().and_then(tree::name_text) else {
            continue;
        };
        let host = decl
            .name()
            .as_ref()
            .and_then(tree::name_text)
            .unwrap_or_default();
        let name_span = rule
            .name()
            .map_or(head.clone(), |n| tree::node_span(path, n.syntax()));
        let earlier = match seen.iter_mut().find(|(d, _)| *d == decl) {
            Some((_, earlier)) => earlier,
            None => {
                seen.push((decl.clone(), Vec::new()));
                &mut seen.last_mut().expect("just pushed").1
            }
        };
        let taken = if name == INVALID {
            Some((
                "`invalid` is what an `attempt` calls every rule without an arm of its own, so no rule can be called that.".to_string(),
                None,
            ))
        } else if let Some((_, at)) = earlier.iter().find(|(n, _)| *n == name) {
            Some((
                format!(
                    "`{host}` already has a rule `{name}`; an `attempt`'s arm and a failure's report name a rule by its name, so each is its own."
                ),
                Some(at.clone()),
            ))
        } else if fields(&host).contains(&name) {
            Some((
                format!(
                    "`{host}` has a field `{name}`; a rule's name is its own, so `{name}:` in an `attempt` or a report can only mean one thing."
                ),
                None,
            ))
        } else {
            None
        };
        earlier.push((name.clone(), name_span.clone()));
        if let Some((reason, at)) = taken {
            let mut problem = Diagnostic::new(
                &rule::TYPE_RULE_NAME_TAKEN,
                format!("A rule can't be called `{name}` here"),
            )
            .at(name_span, "name taken");
            if let Some(at) = at {
                problem = problem.also(at, "first named here");
            }
            out.push(
                problem.reason(reason).next(
                    "Give the rule a name that says what it checks, such as `ends_after_start`",
                ),
            );
        }
    }
    out
}

/// `rule name`: where an error about the rule itself points.
fn head_span(path: &str, rule: &ast::RuleMember) -> jadpo_diag::Span {
    let start = u32::from(rule.syntax().text_range().start());
    let end = rule.colon_token().map_or_else(
        || u32::from(rule.syntax().text_range().end()),
        |colon| u32::from(colon.text_range().start()),
    );
    jadpo_diag::Span::new(path, start, end)
}

fn outside_object(rule: &ast::RuleMember, head: jadpo_diag::Span) -> Diagnostic {
    let parent = rule.syntax().parent().and_then(|list| list.parent());
    let (what, label) = match parent.as_ref().map(SyntaxNode::kind) {
        Some(SyntaxKind::TYPE_DECL) => {
            let decl = parent.clone().and_then(ast::TypeDecl::cast);
            let name = decl
                .as_ref()
                .and_then(|d| d.name())
                .as_ref()
                .and_then(tree::name_text)
                .unwrap_or_default();
            let base = decl
                .and_then(|d| d.base())
                .map(|b| tree::written(b.syntax()))
                .unwrap_or_default();
            (
                format!("`{name}`, built on `{base}`,"),
                format!("in a type built on `{base}`"),
            )
        }
        Some(SyntaxKind::ENTITY_DECL) => {
            let name = parent
                .clone()
                .and_then(ast::EntityDecl::cast)
                .and_then(|d| d.name())
                .as_ref()
                .and_then(tree::name_text)
                .unwrap_or_default();
            (format!("The entity `{name}`"), "in an entity".to_string())
        }
        _ => (
            "This block".to_string(),
            "not an object's block".to_string(),
        ),
    };
    Diagnostic::new(
        &rule::TYPE_RULE_OUTSIDE_OBJECT,
        "Only an `Object`, `Input` or `Output` names rules across its fields",
    )
    .at(head, label)
    .reason(format!(
        "{what} can't hold a rule: a rule relates the fields of one object, so it is written in the block of an `Object`, `Input` or `Output`, and checked whenever a value of it is made."
    ))
    .next("Move the rule into the object type whose fields it reads, or remove it")
}
