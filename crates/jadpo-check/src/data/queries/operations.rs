//! Standard operations in conditions (area 6, "Operations in conditions";
//! decided 2026-10-19, built in chunk 6.3b): an operation area 19 gives an
//! exact SQL form stands where a field would, `text.length(Book.title) <=
//! 24`. The field is its first argument, given as it is, and every other
//! argument is written out, so the compiler can derive an index on the
//! operation's result for each record; its result is compared with a value
//! as a field's is, or, a `Bool`, is the condition itself. The database
//! works it out with its form ([`jadpo_sql::condition`]), which gives what
//! the code gives, so the query keeps the records `list.filter` would.
//!
//! What it refuses, each the most conservative reading: an operation with
//! no exact form, or one PostgreSQL can't index (`jadpo_sql::form::
//! indexable`); a value given when the query runs as another argument; an
//! operation of another operation's result, or with the field anywhere but
//! first (`query.condition_unsupported`); a field kept in several columns
//! or whole as its JSON (chunk 5.9b), which no condition compares yet
//! (`toolchain.not_built_yet`);
//! and arguments code would refuse (`type.mismatch`, a zero divisor, a
//! negative count, empty text to find).

use super::*;
use jadpo_sql::form;
use jadpo_std::{Kind, Op, Rounding};

/// The standard operation a call names, when it names one: `text.length`.
pub(super) fn standard_operation(call: &ast::CallExpr) -> Option<&'static jadpo_std::Operation> {
    let (head, rest) = dotted(&call.expr()?)?;
    let [name] = rest.as_slice() else {
        return None;
    };
    jadpo_std::operation(&tree::name_ref_text(&head)?, &tree::name_ref_text(name)?)
}

/// What a condition asks of an operation's result, as written.
pub(super) enum Asked<'e> {
    /// Nothing: a `Bool` standing as the condition itself.
    Holds,
    /// `<op> value`.
    Compare(Compare, &'e Expr),
    /// `in list`.
    In(&'e Expr),
}

impl Reader<'_> {
    /// A standard operation of one of `entity`'s fields in a condition, and
    /// what the condition asks of its result. `optional` allows the
    /// compared value to be absent (`where_present:`).
    pub(super) fn operation_condition(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        call: &ast::CallExpr,
        asked: Asked<'_>,
        whole: &Expr,
        optional: bool,
    ) -> Option<Condition> {
        let operation = standard_operation(call)?;
        let written = format!("{}.{}", operation.namespace, operation.name);
        let args: Vec<ast::Arg> = call
            .arg_list()
            .into_iter()
            .flat_map(|list| list.args())
            .collect();
        // Each argument in its parameter's place, as a call in code takes
        // them: positional first, then by name.
        let mut slots: Vec<Option<Expr>> = vec![None; operation.params.len()];
        for (index, arg) in args.iter().enumerate() {
            let named = arg.name().and_then(|name| tree::name_text(&name));
            let slot = match &named {
                Some(name) => operation.params.iter().position(|p| p.name == name),
                None => (index < operation.params.len()).then_some(index),
            };
            let Some(slot) = slot else {
                let (rule, message) = match &named {
                    Some(name) => (
                        &rule::TYPE_NAMED_ARGUMENT_UNKNOWN,
                        format!("`{written}` has no parameter `{name}`"),
                    ),
                    None => (
                        &rule::TYPE_ARGUMENT_COUNT,
                        format!(
                            "`{written}` takes {} values, not {}",
                            operation.params.len(),
                            args.len()
                        ),
                    ),
                };
                let names: Vec<String> = operation
                    .params
                    .iter()
                    .map(|p| format!("`{}`", p.name))
                    .collect();
                self.out.push(
                    Diagnostic::new(rule, message)
                        .at(Self::span(scope.path, arg), "not one of its parameters")
                        .reason(format!(
                            "`{written}`'s parameters are {}, as in code.",
                            crate::and_list(&names)
                        ))
                        .next("Give each of its parameters once"),
                );
                return None;
            };
            slots[slot] = arg.expr();
        }
        // The field: the first argument, given as it is.
        let first = slots.first().cloned().flatten();
        let field = match &first {
            Some(first) => match unparen(first) {
                Expr::CallExpr(inner) if self.reads_field(entity, &inner) => {
                    self.unsupported(
                        scope,
                        whole,
                        format!("A condition's operation is called on one of `{}`'s fields itself, so an index can hold its result for each record; `{}` is called on another operation's result.", entity.name, shown(first.syntax())),
                        "Call the operation on the field, and keep anything more in a field of its own".to_string(),
                    );
                    return None;
                }
                expr if self.names_entity(&expr) => self.field_ref(scope.path, entity, &expr)?,
                _ => {
                    let elsewhere = slots
                        .iter()
                        .skip(1)
                        .flatten()
                        .any(|arg| self.field_named(entity, arg).is_some());
                    let reason = if elsewhere {
                        format!(
                            "An index holds `{written}`'s result for each record when the field is its first argument and every other is written out: `{written}({}.<field>, ...)`.",
                            entity.name
                        )
                    } else {
                        format!(
                            "A condition tests one of `{}`'s fields, given to `{written}` as its first argument.",
                            entity.name
                        )
                    };
                    self.unsupported(
                        scope,
                        whole,
                        reason,
                        format!("Write `{}.<field>` first", entity.name),
                    );
                    return None;
                }
            },
            None => {
                self.unsupported(
                    scope,
                    whole,
                    format!(
                        "A condition tests one of `{}`'s fields, given to `{written}` as its first argument.",
                        entity.name
                    ),
                    format!("Write `{written}({}.<field>, ...)`", entity.name),
                );
                return None;
            }
        };
        let field_written = format!("{}.{}", entity.name, field.name);
        // Only the application can open a sealed field (area 15; chunk P9).
        if field.sealed.is_some() {
            self.out.push(crate::data::sealed::used(
                &entity.name,
                &field.name,
                "A query's condition",
                Self::span(scope.path, whole),
                "Find the records by another field, and work this one out in the action".to_string(),
            ));
            return None;
        }
        // A field kept in several columns, or whole as its JSON (chunks
        // 5.5w2b and 5.9b), isn't compared in a query yet, through an
        // operation or not: PostgreSQL compares `jsonb` by its own rules,
        // not by Jadpo's `==`, and an operation's form reads one column.
        let not_built = match (field.compound, &field.json) {
            (Some(compound), _) => Some((
                compound.to_string(),
                "stored in several columns (an amount of money in its minor units and its currency, area 19)",
            )),
            (None, Some(held)) => Some((
                held.clone(),
                "stored whole as its JSON in one `jsonb` column (area 5)",
            )),
            (None, None) => None,
        };
        if let Some((stored, how)) = not_built {
            self.out.push(
                Diagnostic::new(
                    &rule::TOOLCHAIN_NOT_BUILT_YET,
                    format!("Comparing `{field_written}` in a query isn't built yet"),
                )
                .at(Self::span(scope.path, whole), format!("a `{stored}`"))
                .reason(format!(
                    "`{field_written}` is a `{stored}`, {how}, and this compiler's queries don't compare such a field yet, through an operation or not."
                ))
                .next("Filter the records after the query for now"),
            );
            return None;
        }
        // An exact SQL form, which the database works out as the code does.
        let exact = jadpo_sql::lowering(operation.op) == Some(Lowering::Exact)
            && operation.op != Op::AiDistance;
        if !exact {
            self.unsupported(
                scope,
                whole,
                format!("`{written}` has no SQL form, an expression PostgreSQL works out exactly as the code does (area 19, \"SQL forms\"), so the database can't decide this condition."),
                format!("Compare `{field_written}` itself, or keep what the condition needs in a field of its own"),
            );
            return None;
        }
        // The field's type, as the operation's first parameter takes it.
        let field_ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
        let held = match self.held(&field_ty) {
            Some(held) if self.takes(operation.params[0].kind, &field_ty) => held,
            _ => {
                if !field.ty.has_error()
                    && let Some(first) = &first
                {
                    let wanted = kind_shown(operation.params[0].kind);
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_MISMATCH,
                            format!("`{field_written}` can't be used where {wanted} is required"),
                        )
                        .at(Self::span(scope.path, first), format!("a `{}`", field.ty.strip_nullable()))
                        .reason(format!(
                            "`{written}`'s `{}` is {wanted}, as in code; `{field_written}` is `{}`.",
                            operation.params[0].name,
                            field.ty.strip_nullable()
                        ))
                        .next("Call it on a field of that type"),
                    );
                }
                return None;
            }
        };
        if !form::indexable(operation.op, held) {
            self.unsupported(
                scope,
                whole,
                format!("PostgreSQL can't index `{written}`'s SQL form on `{field_written}`: it reads a subquery, converts text through the database's encoding, or takes a range written out (area 6, \"Operations in conditions\"), so no index can serve the condition."),
                format!("Compare `{field_written}` itself, or keep what the condition needs in a field of its own"),
            );
            return None;
        }
        // Every other argument written out.
        let mut call_args = Vec::new();
        for (param, slot) in operation.params.iter().zip(&slots).skip(1) {
            call_args.push(match slot {
                Some(expr) => {
                    self.written_out(scope, entity, &written, param, expr, &field_ty, whole)?
                }
                None if param.optional => CallArg::Absent,
                None => {
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_ARGUMENT_COUNT,
                            format!("`{written}` needs its `{}`", param.name),
                        )
                        .at(Self::span(scope.path, call), "left out")
                        .reason(format!(
                            "`{written}` takes its `{}`, as in code.",
                            param.name
                        ))
                        .next(format!("Pass `{}` as well", param.name)),
                    );
                    return None;
                }
            });
        }
        // What it gives: a built-in type, or the field's own type.
        let (result_ty, result) = match operation.result {
            Kind::Builtin(builtin) => (Ty::Prim(builtin), form::Held::Builtin(builtin)),
            Kind::OneOf(_) | Kind::Number | Kind::Any
                if operation.result == operation.params[0].kind =>
            {
                (field_ty.clone(), held)
            }
            _ => {
                self.unsupported(
                    scope,
                    whole,
                    format!("`{written}` gives a value a condition doesn't compare: numbers, text, times, dates, durations, `Bool`, `Uuid` and `Bytes` are."),
                    format!("Compare `{field_written}` itself"),
                );
                return None;
            }
        };
        let call_model = Call {
            op: operation.op,
            field: field.name.clone(),
            args: call_args,
            result,
        };
        // A form for these arguments, which an index can hold.
        if jadpo_sql::condition::index_expression(entity, &call_model).is_none() {
            self.unsupported(
                scope,
                whole,
                format!("`{written}` has no SQL form for these arguments (area 19, \"SQL forms\"), so the database can't decide this condition."),
                format!("Compare `{field_written}` itself, or keep what the condition needs in a field of its own"),
            );
            return None;
        }
        let shown_call = shown(call.syntax());
        let test = match asked {
            Asked::Holds => {
                if !matches!(result_ty, Ty::Prim(Prim::Bool)) {
                    self.unsupported(
                        scope,
                        whole,
                        format!("A condition compares `{shown_call}` with a value; it gives `{result_ty}`, which alone says nothing to compare it with."),
                        format!("Compare `{shown_call}` with a value"),
                    );
                    return None;
                }
                Test::Holds
            }
            Asked::Compare(compare, rhs) => {
                if matches!(Const::from_expr(rhs), Const::None) {
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_INCOMPARABLE,
                            format!("`{shown_call}` is never `none`"),
                        )
                        .at(Self::span(scope.path, whole), format!("`{result_ty}` compared with `none`"))
                        .reason(format!(
                            "`{written}` gives a value whenever it is called; a record whose field is `none` meets no condition on an operation of it."
                        ))
                        .next(format!("Compare `{field_written}` with `none` instead")),
                    );
                    return None;
                }
                if compare.is_ordering() && !self.ordered(&result_ty) {
                    self.unsupported(
                        scope,
                        whole,
                        format!("`{}` orders numbers, times, dates, durations and quantities, as it does in a body, and `{shown_call}` gives `{result_ty}`.", compare_written(compare)),
                        format!("Compare `{shown_call}` with `==`, `!=` or `in`"),
                    );
                    return None;
                }
                let (value, ty) = self.value(scope, rhs, &result_ty)?;
                if !optional && self.lookup.is_nullable(&ty) {
                    self.optional_in_where(scope, rhs);
                    return None;
                }
                if !matches!(value, Value::Literal(_)) && !self.comparable(&ty, &result_ty) {
                    self.mismatch(scope, rhs, &ty, &result_ty);
                    return None;
                }
                Test::Compare { op: compare, value }
            }
            Asked::In(rhs) => {
                let wanted = Ty::List(Box::new(result_ty.clone()));
                let (value, ty) = self.value(scope, rhs, &wanted)?;
                if !optional && self.lookup.is_nullable(&ty) {
                    self.optional_in_where(scope, rhs);
                    return None;
                }
                let element = match self.lookup.structure(&ty) {
                    Ty::List(inner) | Ty::Set(inner) => Some(*inner),
                    _ => None,
                };
                match element {
                    Some(element) if self.comparable(&element, &result_ty) => {}
                    _ => {
                        self.mismatch(scope, rhs, &ty, &wanted);
                        return None;
                    }
                }
                Test::In { value }
            }
        };
        Some(Condition::Call {
            call: call_model,
            test,
        })
    }

    /// Whether an expression names an entity's field, `Entity.field`, an
    /// entity's name first.
    fn names_entity(&self, expr: &Expr) -> bool {
        dotted(expr).is_some_and(|(first, rest)| {
            rest.len() == 1 && starts_upper(&tree::name_ref_text(&first).unwrap_or_default())
        })
    }

    /// Whether a call reads one of `entity`'s fields, in any argument.
    fn reads_field(&self, entity: &EntityModel, call: &ast::CallExpr) -> bool {
        call.arg_list()
            .into_iter()
            .flat_map(|list| list.args())
            .filter_map(|arg| arg.expr())
            .any(|arg| match unparen(&arg) {
                Expr::CallExpr(inner) => self.reads_field(entity, &inner),
                other => self.field_named(entity, &other).is_some(),
            })
    }

    /// How an operation's form is given a value of `ty`: the built-in type
    /// it is built on, text for a plain enum, `jsonb` for a list, a set or
    /// a map.
    fn held(&self, ty: &Ty) -> Option<form::Held> {
        if let Some(prim) = self.lookup.representation(ty) {
            return form::reads(prim).then_some(form::Held::Builtin(prim));
        }
        match self.lookup.structure(ty) {
            Ty::List(_) | Ty::Set(_) | Ty::Map(..) => Some(form::Held::Json),
            Ty::Named(name) => match self.lookup.def(&name) {
                Some(Def::Enum { variants }) if variants.iter().all(|v| v.fields.is_none()) => {
                    Some(form::Held::Builtin(Prim::Text))
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether a parameter of `kind` takes a value of `ty`, as a call in
    /// code does.
    fn takes(&self, kind: Kind, ty: &Ty) -> bool {
        let representation = self.lookup.representation(ty);
        match kind {
            Kind::Builtin(builtin) => {
                representation.map(Prim::representation) == Some(builtin.representation())
            }
            Kind::OneOf(accepts) => representation.is_some_and(|prim| {
                accepts
                    .iter()
                    .any(|accepted| accepted.representation() == prim.representation())
            }),
            Kind::Number => representation.is_some_and(Prim::is_number),
            Kind::List => matches!(self.lookup.structure(ty), Ty::List(_)),
            Kind::Set => matches!(self.lookup.structure(ty), Ty::Set(_)),
            Kind::Map => matches!(self.lookup.structure(ty), Ty::Map(..)),
            // The form decides which types it reads (`text.from`).
            Kind::Contract | Kind::Any => true,
            _ => false,
        }
    }

    /// One argument after the field, written out: a rounding mode, a whole
    /// number or one character the operation takes written out, or a value
    /// fixed while compiling (a literal, a constant, an enum value), held
    /// to what code holds it to.
    #[allow(clippy::too_many_arguments)]
    fn written_out(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        written: &str,
        param: &jadpo_std::Param,
        expr: &Expr,
        field_ty: &Ty,
        whole: &Expr,
    ) -> Option<CallArg> {
        let expr = unparen(expr);
        let not_written = |this: &mut Self| {
            this.unsupported(
                scope,
                whole,
                format!("An index holds `{written}`'s result for each record, so every argument but the field is written out, fixed while compiling; `{}` is given when the query runs.", shown(expr.syntax())),
                format!("Write `{}` out, or compare `{}.<field>` itself", param.name, entity.name),
            );
        };
        if param.kind == Kind::Enum(Rounding::NAME) {
            let rounding = dotted(&expr).and_then(|(head, rest)| {
                let [variant] = rest.as_slice() else {
                    return None;
                };
                (tree::name_ref_text(&head)? == Rounding::NAME)
                    .then(|| Rounding::from_name(&tree::name_ref_text(variant)?))
                    .flatten()
            });
            return match rounding {
                Some(rounding) => Some(CallArg::Rounding(rounding)),
                None => {
                    not_written(self);
                    None
                }
            };
        }
        let constant = Const::from_expr(&expr);
        if param.written_out {
            return match constant {
                Const::Number { value, .. } if value.scale == 0 && value.mantissa >= 0 => {
                    Some(CallArg::Whole(i64::try_from(value.mantissa).ok()?))
                }
                _ => {
                    not_written(self);
                    None
                }
            };
        }
        if param.character {
            return match &constant {
                Const::Str(text) if text.chars().count() == 1 => {
                    Some(CallArg::Character(text.chars().next()?))
                }
                _ => {
                    not_written(self);
                    None
                }
            };
        }
        let (expected, held) = match param.kind {
            Kind::Builtin(builtin) => (Ty::Prim(builtin), form::Held::Builtin(builtin)),
            Kind::Number => (Ty::Prim(Prim::Decimal), form::Held::Builtin(Prim::Decimal)),
            Kind::OneOf(_) | Kind::Any => (field_ty.clone(), self.held(field_ty)?),
            _ => {
                not_written(self);
                return None;
            }
        };
        let (value, ty) = self.value(scope, &expr, &expected)?;
        if !value.written_out() {
            not_written(self);
            return None;
        }
        if !matches!(value, Value::Literal(_)) && !self.comparable(&ty, &expected) {
            self.mismatch(scope, &expr, &ty, &expected);
            return None;
        }
        // What code rules out while compiling: a zero divisor, a negative
        // count, empty text to find.
        let span = Self::span(scope.path, &expr);
        let shown_value = shown(expr.syntax());
        let number = match &value {
            Value::Literal(jadpo_std::Literal::Number { value, .. }) => Some(*value),
            _ => None,
        };
        if param.never_zero && number.is_some_and(|n| n.mantissa == 0) {
            self.out.push(
                Diagnostic::new(&rule::TYPE_DIVISOR_ZERO, "This divides by zero")
                    .at(span, "zero")
                    .reason(format!(
                        "`{written}`'s `{}` is never zero, as in code.",
                        param.name
                    ))
                    .next("Divide by a number other than zero"),
            );
            return None;
        }
        if param.never_negative && number.is_some_and(|n| n.mantissa < 0) {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_NEGATIVE,
                    format!("`{written}`'s `{}` is never negative", param.name),
                )
                .at(span, "below `0`")
                .reason(format!("`{shown_value}` is below `0`, and a count or a position never is, as in code (area 19, \"Cutting from either end\")."))
                .next("Pass a number of at least `0`"),
            );
            return None;
        }
        if param.never_empty
            && matches!(&value, Value::Literal(jadpo_std::Literal::Text(text)) if text.is_empty())
        {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_EMPTY,
                    format!("`{written}`'s `{}` is never empty", param.name),
                )
                .at(span, "empty text")
                .reason(format!("`{written}` never looks for `\"\"`, as in code (area 19, \"What every operation follows\")."))
                .next("Pass text of at least one character"),
            );
            return None;
        }
        Some(CallArg::Value(value, held))
    }
}

/// A parameter's kind as a sentence names what it takes.
fn kind_shown(kind: Kind) -> String {
    match kind {
        Kind::Builtin(builtin) => format!("`{}`", builtin.name()),
        Kind::OneOf(accepts) => {
            let names: Vec<String> = accepts.iter().map(|b| format!("`{}`", b.name())).collect();
            format!("one of {}", crate::and_list(&names))
        }
        Kind::Number => "a number".to_string(),
        Kind::List => "a list".to_string(),
        Kind::Set => "a set".to_string(),
        Kind::Map => "a map".to_string(),
        _ => "another type".to_string(),
    }
}

/// A comparison's operator as written.
fn compare_written(compare: Compare) -> &'static str {
    match compare {
        Compare::Equal => "==",
        Compare::NotEqual => "!=",
        Compare::Less => "<",
        Compare::LessEqual => "<=",
        Compare::Greater => ">",
        Compare::GreaterEqual => ">=",
    }
}
