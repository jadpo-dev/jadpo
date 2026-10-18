//! Rules across an object's fields inside bodies (area 2, "Rules across
//! fields"): a rule's condition, checked with its type's fields in scope
//! and held to reading nothing else, and the object literal that may break
//! one, proved rule by rule or mapped by the rules' names.
//!
//! A rule is proved at a construction by reading it with each field
//! replaced by the value given for it: a comparison holds when the values
//! given are all literals and it holds for them, when the declared ranges
//! of numbers prove it, or when a condition around the construction
//! compares the same two values the same way ([`super::known`]'s
//! relations). `not` is pushed inside first, so it turns a comparison
//! round, and `and` and `or` combine what their sides prove. A rule that
//! fails for the literals written is `type.invalid_literal`.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_graph::body::{self as typed, Callee};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::across::{Cond, CrossDef};
use super::super::rules::{Const, Num};
use super::super::table::Def;
use super::super::ty::Prim;
use super::facts::LocalFact;
use super::known::{Origin, Term};
use super::ranges::{End, Interval, Reading, arithmetic};
use super::{Body, Fallible, Local, LocalKind, node_key};
use crate::tree;

/// What a rule comes to at one construction.
enum Truth {
    /// It always holds, by these comparisons (none for literals and the
    /// declared ranges alone).
    Holds(BTreeSet<Origin>),
    /// It never holds for the literals written.
    Breaks,
    /// The running program decides.
    Unknown,
}

/// A value written out, as a comparison reads it.
enum Lit {
    Number(Num),
    Instant(i64),
    Date(i32),
    Text(String),
    Bool(bool),
}

/// `not` pushed inside: `not (a < b)` is `a >= b`, and `not (a and b)` is
/// `not a or not b`.
fn negated(cond: &Cond) -> Cond {
    match cond {
        Cond::Not(inner) => (**inner).clone(),
        Cond::Bool(value) => Cond::Bool(!value),
        Cond::Binary(op, a, b) => match op.as_str() {
            "and" => Cond::Binary("or".into(), Box::new(negated(a)), Box::new(negated(b))),
            "or" => Cond::Binary("and".into(), Box::new(negated(a)), Box::new(negated(b))),
            "<" | "<=" | ">" | ">=" | "==" | "!=" => Cond::Binary(
                match op.as_str() {
                    "<" => ">=",
                    "<=" => ">",
                    ">" => "<=",
                    ">=" => "<",
                    "==" => "!=",
                    _ => "==",
                }
                .into(),
                a.clone(),
                b.clone(),
            ),
            _ => Cond::Other,
        },
        _ => Cond::Other,
    }
}

/// Whether every value of `a` and `b` meets `op`.
fn always(op: &str, a: Interval, b: Interval) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    match op {
        "<" => a.high.cmp(b.low).is_lt(),
        "<=" => a.high.cmp(b.low).is_le(),
        ">" => a.low.cmp(b.high).is_gt(),
        ">=" => a.low.cmp(b.high).is_ge(),
        "==" => {
            matches!((a.low, a.high, b.low, b.high), (End::At(w), End::At(x), End::At(y), End::At(z))
                if w.compare(x).is_eq() && x.compare(y).is_eq() && y.compare(z).is_eq())
        }
        "!=" => a.high.cmp(b.low).is_lt() || a.low.cmp(b.high).is_gt(),
        _ => false,
    }
}

impl Body<'_> {
    // --- a rule's condition ------------------------------------------------

    /// Checks a rule's condition with its type's fields in scope, as
    /// parameters in the order the type declares them.
    pub(super) fn rule(
        &mut self,
        host: &str,
        condition: Option<&Expr>,
        members: Option<ast::MemberList>,
    ) {
        let Some(Def::Fields { fields, .. }) = self.table.def(host).cloned() else {
            return;
        };
        let written = tree::members(members);
        for field in &fields {
            let mut ty = self
                .table
                .field_type(host, std::slice::from_ref(&field.name));
            if field.optional && !self.table.is_nullable(&ty) {
                ty = ty.nullable();
            }
            let span = written
                .iter()
                .find(|(name, _)| tree::name_text(name).as_deref() == Some(field.name.as_str()))
                .map_or_else(|| Span::new(self.path, 0, 0), |(name, _)| self.span(name));
            let id = self.facts.locals.len();
            self.facts.locals.push(LocalFact {
                name: field.name.clone(),
                ty: ty.clone(),
                kind: LocalKind::Param,
                span: span.clone(),
            });
            if let Some(scope) = self.scopes.last_mut() {
                scope.push(Local {
                    id,
                    name: field.name.clone(),
                    ty,
                    kind: LocalKind::Param,
                    span,
                    binding: None,
                    used: true,
                });
            }
        }
        let Some(condition) = condition else { return };
        let before = self.out.len();
        let ty = self.infer(condition);
        if self.out.len() > before || ty.has_error() {
            return;
        }
        let condition_like =
            !self.table.is_nullable(&ty) && self.table.representation(&ty) == Some(Prim::Bool);
        if condition_like {
            return;
        }
        let name = self
            .flow
            .name
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_string();
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_RULE_NOT_CONDITION,
                format!("The rule `{name}` isn't a condition"),
            )
            .at(self.span(condition), format!("this is `{ty}`"))
            .reason("A rule holds for a value or it doesn't, so its condition is a `Bool`: a comparison such as `high > low`, or several joined with `and`, `or` and `not`.")
            .next(format!("Write `{name}` as a comparison of `{host}`'s fields")),
        );
    }

    /// What a rule's typed body reads besides its type's fields, literals,
    /// variants, operators, `match` and the standard operations that give
    /// a result from their arguments alone: `type.rule_outside_fields` at
    /// each.
    pub(super) fn outside_fields(&self, body: &typed::Body, text: &str) -> Vec<Diagnostic> {
        let host = self.flow.name.rsplit_once('.').map_or("", |(host, _)| host);
        let mut out = Vec::new();
        for site in &body.sites {
            let typed::Node::Expr(expr) = &site.node else {
                continue;
            };
            let allowed = match expr {
                typed::Expr::Literal(_)
                | typed::Expr::Local(_)
                | typed::Expr::Field { .. }
                | typed::Expr::List { .. }
                | typed::Expr::Unary { .. }
                | typed::Expr::Unchecked => true,
                typed::Expr::Binary { op, .. } => *op != typed::BinaryOp::Matches,
                typed::Expr::Match { outcome, .. } => !outcome,
                typed::Expr::Call {
                    callee: Callee::Std(op),
                    args,
                    ..
                } => {
                    pure(*op)
                        && !args.iter().flatten().any(|arg| {
                            matches!(
                                body.site(*arg).node,
                                typed::Node::Expr(typed::Expr::Inline { .. })
                            )
                        })
                }
                typed::Expr::Build {
                    callee: Callee::Variant(_),
                    fields,
                    ..
                } => fields.is_empty(),
                _ => false,
            };
            if allowed {
                continue;
            }
            let span = Span::new(self.path, site.start, site.end);
            let written = text
                .get(site.start as usize..site.end as usize)
                .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
                .unwrap_or_default();
            out.push(
                Diagnostic::new(
                    &rule::TYPE_RULE_OUTSIDE_FIELDS,
                    format!("A rule can't read `{written}`"),
                )
                .at(span, "not one of the type's fields")
                .reason(format!(
                    "A rule reads only `{host}`'s own fields, with literals, variants, operators, `match` and the standard operations that give a result from their arguments alone, so every value of `{host}` follows it wherever it is made, the same in every run."
                ))
                .next(format!(
                    "Write the rule over `{host}`'s fields, or check this where the value is built, before building it"
                )),
            );
            // The first is the mistake; what is built on it is the same one.
            break;
        }
        out
    }

    // --- object literals ---------------------------------------------------

    /// The rules across `host`'s fields, by name and as its type writes
    /// them.
    pub(super) fn across_rules(&self, host: &str) -> Vec<(String, String)> {
        match self.table.def(host) {
            Some(Def::Fields { rules, .. }) => rules
                .iter()
                .map(|rule| (rule.name.clone(), rule.shown()))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Records whether building `host` with `record` can fail: each rule
    /// across its fields proved, broken by the literals written
    /// (`type.invalid_literal`), or left to the running program, which an
    /// `attempt` maps.
    pub(super) fn object_rules(&mut self, record: &ast::RecordExpr, host: &str) {
        let key = node_key(record.syntax());
        let rules: Vec<CrossDef> = match self.table.def(host) {
            Some(Def::Fields { rules, .. }) => rules.clone(),
            _ => Vec::new(),
        };
        let given: BTreeMap<String, Expr> = tree::members(record.member_list())
            .into_iter()
            .filter_map(|(name, member)| Some((tree::name_text(&name)?, member.value()?)))
            .collect();
        let mut breakable = Vec::new();
        let mut proved = Vec::new();
        for rule in &rules {
            match self.truth(&rule.condition, &given, host) {
                Truth::Holds(by) => {
                    if !by.is_empty() {
                        proved.push((rule.name.clone(), by.into_iter().collect()));
                    }
                }
                Truth::Breaks => {
                    self.rule_broken(record, host, rule);
                    self.facts.constructions.insert(key, Fallible::Never);
                    return;
                }
                Truth::Unknown => breakable.push(rule.name.clone()),
            }
        }
        if !proved.is_empty() {
            self.facts.proofs.insert(key, proved);
        }
        let verdict = if breakable.is_empty() {
            Fallible::Never
        } else {
            Fallible::Construction(breakable.clone())
        };
        self.facts.constructions.insert(key, verdict);
        if !breakable.is_empty() && !self.handled.contains(&key) {
            self.unhandled_record(record, host, &breakable);
        }
    }

    fn rule_broken(&mut self, record: &ast::RecordExpr, host: &str, rule: &CrossDef) {
        let span = record
            .expr()
            .map_or_else(|| self.span(record), |head| self.span(&head));
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_INVALID_LITERAL,
                format!("This `{host}` breaks its rule `{}`", rule.name),
            )
            .at(span, format!("breaks `{}`", rule.shown()))
            .reason(format!(
                "Every value the rule reads is written out, so it is worked out while compiling, and `{}` doesn't hold for them.",
                rule.written
            ))
            .next(format!("Change the values so `{}` holds", rule.written)),
        );
    }

    fn unhandled_record(&mut self, record: &ast::RecordExpr, host: &str, breakable: &[String]) {
        let rules = self.across_rules(host);
        let listed: Vec<String> = breakable
            .iter()
            .map(|name| {
                rules
                    .iter()
                    .find(|(n, _)| n == name)
                    .map_or_else(|| format!("`{name}`"), |(_, shown)| format!("`{shown}`"))
            })
            .collect();
        let first = &breakable[0];
        let span = record
            .expr()
            .map_or_else(|| self.span(record), |head| self.span(&head));
        self.flow.open = true;
        self.out.push(
            Diagnostic::new(
                &rule::FAILURE_ATTEMPT_REQUIRED,
                format!("`{host} {{ ... }}` can fail at runtime, and nothing handles it"),
            )
            .at(span, "checked when it runs")
            .reason(format!(
                "Its values are only known when the program runs, and neither the declared ranges nor a condition before it prove they follow {}, so building it can fail.",
                crate::and_list(&listed)
            ))
            .next(format!(
                "Write `attempt {host} {{ ... }} {{ {first}: SomeFailure }}`, mapping each rule that can break to an `invalid_value` failure, or `invalid:` for all of them"
            )),
        );
    }

    // --- proof ---------------------------------------------------------------

    fn truth(&self, cond: &Cond, given: &BTreeMap<String, Expr>, host: &str) -> Truth {
        match cond {
            Cond::Bool(true) => Truth::Holds(BTreeSet::new()),
            Cond::Bool(false) => Truth::Breaks,
            Cond::Not(inner) => match negated(inner) {
                Cond::Other => Truth::Unknown,
                pushed => self.truth(&pushed, given, host),
            },
            Cond::Binary(op, a, b) if op == "and" => {
                match (self.truth(a, given, host), self.truth(b, given, host)) {
                    (Truth::Breaks, _) | (_, Truth::Breaks) => Truth::Breaks,
                    (Truth::Holds(x), Truth::Holds(y)) => Truth::Holds(&x | &y),
                    _ => Truth::Unknown,
                }
            }
            Cond::Binary(op, a, b) if op == "or" => {
                match (self.truth(a, given, host), self.truth(b, given, host)) {
                    (Truth::Holds(x), _) | (_, Truth::Holds(x)) => Truth::Holds(x),
                    (Truth::Breaks, Truth::Breaks) => Truth::Breaks,
                    _ => Truth::Unknown,
                }
            }
            Cond::Binary(op, a, b)
                if matches!(op.as_str(), "<" | "<=" | ">" | ">=" | "==" | "!=") =>
            {
                self.rule_compared(op, a, b, given, host)
            }
            _ => Truth::Unknown,
        }
    }

    fn rule_compared(
        &self,
        op: &str,
        a: &Cond,
        b: &Cond,
        given: &BTreeMap<String, Expr>,
        host: &str,
    ) -> Truth {
        // Values written out are compared while compiling.
        if let (Some(x), Some(y)) = (self.lit(a, given, host), self.lit(b, given, host))
            && let Some(holds) = lit_holds(op, &x, &y)
        {
            return if holds {
                Truth::Holds(BTreeSet::new())
            } else {
                Truth::Breaks
            };
        }
        // Numbers, by their declared ranges and what the conditions say.
        let mut reading = Reading::with_conditions();
        if let (Some(x), Some(y)) = (
            self.side_interval(a, given, &mut reading),
            self.side_interval(b, given, &mut reading),
        ) && always(op, x, y)
        {
            return Truth::Holds(reading.used);
        }
        // The same comparison, known to hold around the construction.
        if let (Some(x), Some(y)) = (self.side_term(a, given), self.side_term(b, given))
            && !(x.is_constant() && y.is_constant())
            && let Some(by) = self.known.relation(&x, op, &y)
        {
            return Truth::Holds(by.clone());
        }
        Truth::Unknown
    }

    /// A side of a comparison as a value written out, when it is one.
    fn lit(&self, cond: &Cond, given: &BTreeMap<String, Expr>, host: &str) -> Option<Lit> {
        match cond {
            Cond::Number(n) => Some(Lit::Number(*n)),
            Cond::Text(text) => Some(Lit::Text(text.clone())),
            Cond::Bool(value) => Some(Lit::Bool(*value)),
            Cond::Name(name) => {
                let value = given.get(name)?;
                if !super::is_literal(value) {
                    return None;
                }
                match Const::from_expr(value) {
                    Const::Number { value, .. } => Some(Lit::Number(value)),
                    Const::Bool(flag) => Some(Lit::Bool(flag)),
                    Const::Str(text) => {
                        let ty = self.table.field_type(host, std::slice::from_ref(name));
                        match self.table.base_prim(&ty) {
                            Some(Prim::Instant) => {
                                jadpo_std::instant_millis(&text).map(Lit::Instant)
                            }
                            Some(Prim::CalendarDate) => {
                                jadpo_std::date_number(&text).map(Lit::Date)
                            }
                            Some(Prim::Text) => Some(Lit::Text(text)),
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// A numeric side's values, from the values given and the literals.
    fn side_interval(
        &self,
        cond: &Cond,
        given: &BTreeMap<String, Expr>,
        reading: &mut Reading,
    ) -> Option<Interval> {
        match cond {
            Cond::Number(n) => Some(Interval::exact(*n)),
            Cond::Name(name) => {
                let value = given.get(name)?;
                self.numeric(value)?;
                Some(self.interval(value, reading))
            }
            Cond::Negate(inner) => {
                let inner = self.side_interval(inner, given, reading)?;
                Some(Interval {
                    low: inner.high.neg(),
                    high: inner.low.neg(),
                })
            }
            Cond::Binary(op, a, b) if matches!(op.as_str(), "+" | "-" | "*" | "/" | "%") => {
                let a = self.side_interval(a, given, reading)?;
                let b = self.side_interval(b, given, reading)?;
                Some(arithmetic(op, a, b))
            }
            _ => None,
        }
    }

    /// A side as the subject a condition around the construction would
    /// have written: each field read as the value given for it.
    fn side_term(&self, cond: &Cond, given: &BTreeMap<String, Expr>) -> Option<Term> {
        match cond {
            Cond::Number(n) => Some(Term::number(*n)),
            Cond::Name(name) => self.term(given.get(name)?),
            Cond::Field(inner, name) => Some(Term::Field(
                Box::new(self.side_term(inner, given)?),
                name.clone(),
            )),
            Cond::Negate(inner) => Some(Term::Negate(Box::new(self.side_term(inner, given)?))),
            Cond::Binary(op, a, b) if matches!(op.as_str(), "+" | "-" | "*" | "/" | "%") => {
                Some(Term::Arithmetic(
                    op.chars().next()?,
                    Box::new(self.side_term(a, given)?),
                    Box::new(self.side_term(b, given)?),
                ))
            }
            _ => None,
        }
    }
}

/// Whether `x op y` holds for two values written out; `None` when they
/// don't compare.
fn lit_holds(op: &str, x: &Lit, y: &Lit) -> Option<bool> {
    let instant = |text: &str| jadpo_std::instant_millis(text);
    let date = |text: &str| jadpo_std::date_number(text);
    let order = match (x, y) {
        (Lit::Number(a), Lit::Number(b)) => a.compare(*b),
        (Lit::Instant(a), Lit::Instant(b)) => a.cmp(b),
        (Lit::Instant(a), Lit::Text(b)) => a.cmp(&instant(b)?),
        (Lit::Text(a), Lit::Instant(b)) => instant(a)?.cmp(b),
        (Lit::Date(a), Lit::Date(b)) => a.cmp(b),
        (Lit::Date(a), Lit::Text(b)) => a.cmp(&date(b)?),
        (Lit::Text(a), Lit::Date(b)) => date(a)?.cmp(b),
        (Lit::Text(a), Lit::Text(b)) if matches!(op, "==" | "!=") => a.cmp(b),
        (Lit::Bool(a), Lit::Bool(b)) if matches!(op, "==" | "!=") => a.cmp(b),
        _ => return None,
    };
    Some(match op {
        "<" => order.is_lt(),
        "<=" => order.is_le(),
        ">" => order.is_gt(),
        ">=" => order.is_ge(),
        "==" => order.is_eq(),
        _ => order.is_ne(),
    })
}

/// Whether a standard operation may be part of a rule: it gives a result
/// from its arguments alone, reading no clock, generator, key or locale
/// list, and can't fail.
fn pure(op: jadpo_std::Op) -> bool {
    let operation = op.operation();
    !op.reads_generator()
        && !op.reads_clock()
        && operation.conditions.is_empty()
        && !matches!(operation.namespace, "crypto" | "json" | "random")
        && op != jadpo_std::Op::LocaleFromAcceptLanguage
}
