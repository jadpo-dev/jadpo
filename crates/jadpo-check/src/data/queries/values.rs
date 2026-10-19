//! The values a query reads (area 6, "Filters"): parameters and their
//! fields, literals, constants, enum values, lists of those and
//! `clock.now`, each checked against the type it meets; a query
//! computes nothing.

use super::*;

impl Reader<'_> {
    /// A value used in a query, checked against the type it meets. Gives
    /// the value and its type.
    pub(super) fn value(
        &mut self,
        scope: &Scope<'_>,
        expr: &Expr,
        expected: &Ty,
    ) -> Option<(Value, Ty)> {
        let expr = unparen(expr);
        let constant = Const::from_expr(&expr);
        match &expr {
            Expr::Literal(_) | Expr::PrefixExpr(_)
                if !matches!(constant, Const::Other | Const::Path(_)) =>
            {
                if let Err(problem) =
                    crate::types::literal::check(self.lookup, expected.strip_nullable(), &constant)
                {
                    self.out.push(crate::types::literal::diagnostic(
                        self.lookup,
                        problem,
                        &shown(expr.syntax()),
                        expected.strip_nullable(),
                        Self::span(scope.path, &expr),
                        false,
                    ));
                    return None;
                }
                // A literal is never absent: where an optional field's
                // value goes, it is the field's type without its `?`.
                let taken = match expected.strip_nullable() {
                    field @ Ty::Field(..) if self.lookup.is_nullable(field) => {
                        self.lookup.parent(field).unwrap_or_else(|| field.clone())
                    }
                    other => other.clone(),
                };
                return Some((Value::Literal(constant.literal()?), taken));
            }
            Expr::ListExpr(list) => {
                let element = match self.lookup.structure(expected) {
                    Ty::List(inner) | Ty::Set(inner) => *inner,
                    _ => {
                        self.out.push(
                            Diagnostic::new(
                                &rule::TYPE_MISMATCH,
                                format!("A list can't be used where `{expected}` is required"),
                            )
                            .at(Self::span(scope.path, &expr), "a list")
                            .reason(format!("A list holds several values, and this compares with one `{expected}`; `in` tests a field against a list."))
                            .next("Use `in` to compare with a list"),
                        );
                        return None;
                    }
                };
                let mut items = Vec::new();
                for item in list.exprs() {
                    let (value, ty) = self.value(scope, &item, &element)?;
                    if !matches!(value, Value::Literal(_)) && !self.comparable(&ty, &element) {
                        self.mismatch(scope, &item, &ty, &element);
                        return None;
                    }
                    if self.lookup.is_nullable(&ty) {
                        self.out.push(
                            Diagnostic::new(
                                &rule::QUERY_OPTIONAL_VALUE,
                                format!("`{}` may be absent, and a list's items can't be", shown(item.syntax())),
                            )
                            .at(Self::span(scope.path, &item), "may be `none`")
                            .reason("Each item of a list a field is tested `in` is a value; an absent one would match nothing.")
                            .next("Use a value that is always there"),
                        );
                        return None;
                    }
                    items.push(value);
                }
                return Some((Value::List(items), Ty::List(Box::new(element))));
            }
            _ => {}
        }
        let Some((first, rest)) = dotted(&expr) else {
            self.value_unsupported(scope, &expr);
            return None;
        };
        let head = tree::name_ref_text(&first)?;
        let rest: Vec<String> = rest.iter().filter_map(tree::name_ref_text).collect();
        if starts_upper(&head) {
            if let ([variant], Some(Def::Enum { variants })) =
                (rest.as_slice(), self.lookup.def(&head))
            {
                if variants.iter().any(|v| v.name == *variant) {
                    return Some((
                        Value::Variant {
                            ty: head.clone(),
                            variant: variant.clone(),
                        },
                        Ty::Named(head),
                    ));
                }
                let listed: Vec<String> =
                    variants.iter().map(|v| format!("`{}`", v.name)).collect();
                let near = nearest(variant, variants.iter().map(|v| v.name.as_str()));
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_UNKNOWN_ENUM_VARIANT,
                        format!("`{head}` has no variant `{variant}`"),
                    )
                    .at(Self::span(scope.path, &expr), "not a variant")
                    .reason(format!(
                        "`{head}`'s variants are {}.{}",
                        crate::and_list(&listed),
                        near.map(|n| format!(" Did you mean `{n}`?"))
                            .unwrap_or_default()
                    ))
                    .next(match near {
                        Some(near) => format!("Use `{head}.{near}`"),
                        None => format!("Use one of `{head}`'s variants"),
                    }),
                );
                return None;
            }
            // A declaration's name is no value (area 3); an entity's field
            // is a field, which has its own error.
            if let Some(named) = crate::not_value::classify(self.lookup, self.lookup, &head)
                && (rest.is_empty() || !matches!(named, crate::not_value::Named::Entity))
            {
                self.out.push(crate::not_value::diagnostic(
                    &head,
                    &named,
                    Self::span(scope.path, &first),
                ));
                return None;
            }
            if self.entities.get(&head).is_some() {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_CONDITION_UNSUPPORTED,
                        format!("`{}` compares a field with a field", shown(expr.syntax())),
                    )
                    .at(Self::span(scope.path, &expr), "a field, not a value")
                    .reason("A condition compares a field with a value the call supplies; comparing two fields of a record can't be served by an index.")
                    .next("Compare the field with a parameter, a literal or an enum value"),
                );
                return None;
            }
            self.value_unsupported(scope, &expr);
            return None;
        }
        if head == "clock" && rest == ["now"] {
            return Some((Value::Now, Ty::Prim(Prim::Instant)));
        }
        if !scope.params.iter().any(|(name, _)| *name == head)
            && let Some(found) = self.constant(scope, &expr, &head, &rest, expected)
        {
            return found;
        }
        let Some(index) = scope.params.iter().position(|(name, _)| *name == head) else {
            if let Some(named) = crate::not_value::classify(self.lookup, self.lookup, &head) {
                self.out.push(crate::not_value::diagnostic(
                    &head,
                    &named,
                    Self::span(scope.path, &first),
                ));
                return None;
            }
            if rest.is_empty()
                && let Some(problem) = crate::foreign_value(&head, Self::span(scope.path, &first))
            {
                self.out.push(problem);
                return None;
            }
            let names: Vec<&str> = scope.params.iter().map(|(n, _)| n.as_str()).collect();
            let near = nearest(&head, names.iter().copied());
            let span = Self::span(scope.path, &first);
            let problem = Diagnostic::new(
                &rule::SEMANTIC_UNKNOWN_NAME,
                format!("`{head}` isn't declared"),
            )
            .at(span.clone(), "not a parameter")
            .reason(format!(
                "A query's values come from its parameters{}, literals, constants, enum values and `clock.now`.{}",
                if names.is_empty() {
                    String::new()
                } else {
                    format!(
                        " ({})",
                        crate::and_list(&names.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>())
                    )
                },
                near.map(|n| format!(" Did you mean `{n}`?")).unwrap_or_default()
            ));
            self.out.push(match near {
                Some(near) => problem.next(format!("Use `{near}`")).edit(span, near),
                None => problem.next(format!("Add `{head}` to the query's parameters")),
            });
            return None;
        };
        let mut ty = scope.params[index].1.clone();
        for field in &rest {
            if self.lookup.is_nullable(&ty) {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_VALUE_UNSUPPORTED,
                        format!("`{}` may be `none`, so `{field}` can't be read", shown(expr.syntax())),
                    )
                    .at(Self::span(scope.path, &expr), "inside a value that may be `none`")
                    .reason("A query reads a parameter's fields only through values that are always there.")
                    .next("Pass the inner value as a parameter of its own"),
                );
                return None;
            }
            // A caller's field (area 8): `caller.id`.
            if let Ty::Opaque(name, _) = &ty
                && let Some(kind) = [
                    jadpo_std::http::PrincipalKind::User,
                    jadpo_std::http::PrincipalKind::Service,
                ]
                .into_iter()
                .find(|kind| kind.type_name() == name)
                && let Some(found) = self.lookup.identity().field_type(kind, field)
            {
                ty = found;
                continue;
            }
            // The bounds `temporal` gives (area 10, "Zones and local times").
            if let Some(found) = range_field(&ty, field) {
                ty = found;
                continue;
            }
            let Some(host) = self.object_of(&ty) else {
                self.value_unsupported(scope, &expr);
                return None;
            };
            let Some(found) = self
                .lookup
                .field(&host, std::slice::from_ref(field))
                .cloned()
            else {
                let fields = self.lookup.fields(&host).unwrap_or(&[]);
                let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
                let near = nearest(field, names.iter().copied());
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_UNKNOWN_FIELD,
                        format!("`{host}` has no field `{field}`"),
                    )
                    .at(Self::span(scope.path, &expr), "not declared")
                    .reason(format!(
                        "`{host}`'s fields are {}.{}",
                        crate::and_list(
                            &names.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>()
                        ),
                        near.map(|n| format!(" Did you mean `{n}`?"))
                            .unwrap_or_default()
                    ))
                    .next(match near {
                        Some(near) => format!("Use `{near}` instead of `{field}`"),
                        None => format!("Name a field of `{host}`"),
                    }),
                );
                return None;
            };
            let next = self.lookup.field_type(&host, std::slice::from_ref(field));
            ty = if found.optional {
                next.nullable()
            } else {
                next
            };
        }
        Some((
            Value::Param {
                index: index as u32,
                path: rest,
            },
            ty,
        ))
    }

    /// The type of a value expression, without reporting anything.
    pub(super) fn value_type(&self, scope: &Scope<'_>, expr: &Expr) -> Option<Ty> {
        let (first, rest) = dotted(&unparen(expr))?;
        let head = tree::name_ref_text(&first)?;
        let mut ty = scope.params.iter().find(|(n, _)| *n == head)?.1.clone();
        for field in rest.iter().filter_map(tree::name_ref_text) {
            if let Some(found) = range_field(&ty, &field) {
                ty = found;
                continue;
            }
            let host = self.object_of(&ty)?;
            let found = self.lookup.field(&host, std::slice::from_ref(&field))?;
            let next = self.lookup.field_type(&host, &[field]);
            ty = if found.optional {
                next.nullable()
            } else {
                next
            };
        }
        Some(ty)
    }

    /// The object type a value's fields come from.
    pub(super) fn object_of(&self, ty: &Ty) -> Option<String> {
        let mut current = ty.strip_nullable().clone();
        for _ in 0..64 {
            match &current {
                Ty::Named(name) => return self.lookup.fields(name).map(|_| name.clone()),
                Ty::Field(..) => current = self.lookup.parent(&current)?.strip_nullable().clone(),
                _ => return None,
            }
        }
        None
    }

    /// A constant (area 3), which a query takes wherever it takes a
    /// literal: `longest_manual`, or `pricing.vat_rate` from a library.
    /// `None` when `head` and `rest` name no constant; `Some(None)` when
    /// one was found and reported.
    pub(super) fn constant(
        &mut self,
        scope: &Scope<'_>,
        expr: &Expr,
        head: &str,
        rest: &[String],
        expected: &Ty,
    ) -> Option<Option<(Value, Ty)>> {
        let (library, name) = match rest {
            [] => (None, head),
            [name] if self.lookup.library(head).is_some() => (Some(head), name.as_str()),
            _ => return None,
        };
        let ty = self.lookup.constant_type(library, name)?.clone();
        let evaluated = self.lookup.constant(library, name)?;
        // A constant whose value is wrong is reported where it is declared.
        let value = evaluated.value.as_ref()?;
        let Some(value) = query_value(value) else {
            self.value_unsupported(scope, expr);
            return Some(None);
        };
        // Compared as the value of its own type, as a parameter is: a
        // list's items are compared by the `in` that takes it.
        if !matches!(value, Value::List(_)) && !self.comparable(&ty, expected.strip_nullable()) {
            self.mismatch(scope, expr, &ty, expected);
            return Some(None);
        }
        Some(Some((value, ty)))
    }

    pub(super) fn value_unsupported(&mut self, scope: &Scope<'_>, expr: &Expr) {
        let written = shown(expr.syntax());
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_VALUE_UNSUPPORTED,
                format!("`{written}` is worked out, and a query computes nothing"),
            )
            .at(Self::span(scope.path, expr), "computed")
            .reason("A query's values are its parameters and their fields, literals, constants, enum values, lists of those and `clock.now`; anything worked out from them is computed before the call.")
            .next("Work the value out in the action, and pass it as a parameter"),
        );
    }
}
