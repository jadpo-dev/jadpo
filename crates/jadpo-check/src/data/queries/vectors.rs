//! Vectors in queries (area 21, "The nearest-neighbour query"; chunks
//! 5.5v1b and 5.5v1c): `order: [Entity.field nearest value]` and its
//! `Nearest<T>` result, and `ai.distance` compared in `where:`.

use super::*;

/// The `nearest` entry of an `order:` list, if it has one.
pub(super) fn nearest_entry(member: &ast::Member) -> Option<ast::OrderedExpr> {
    let Some(Expr::ListExpr(list)) = member.value() else {
        return None;
    };
    list.exprs().find_map(|item| match item {
        Expr::OrderedExpr(ordered) if is_nearest(&ordered) => Some(ordered),
        _ => None,
    })
}

/// Whether a call is `ai.distance(...)` (area 21).
pub(super) fn is_distance(call: &ast::CallExpr) -> bool {
    call.expr()
        .and_then(|callee| dotted(&callee))
        .is_some_and(|(head, rest)| {
            tree::name_ref_text(&head).as_deref() == Some("ai")
                && rest.len() == 1
                && tree::name_ref_text(&rest[0]).as_deref() == Some("distance")
        })
}

pub(super) fn is_nearest(ordered: &ast::OrderedExpr) -> bool {
    ordered
        .direction()
        .is_some_and(|direction| direction.text() == "nearest")
}

/// The vector type a value is, by its own name: the type a field or a
/// parameter declares, or `Vector` itself; `None` for a value that isn't
/// built on `Vector` (as `ai.distance`'s check reads it, area 21).
pub(crate) fn vector_type(lookup: &dyn TypeView, given: &Ty) -> Option<Ty> {
    let mut current = given.strip_nullable().clone();
    for _ in 0..64 {
        match &current {
            Ty::Field(host, path) => {
                current = lookup.field(host, path)?.declared.strip_nullable().clone();
            }
            Ty::Named(_) => {
                return lookup
                    .rule_chain(&current)
                    .iter()
                    .any(|(_, rules)| rules.standard == Some(jadpo_std::standard::ai::VECTOR))
                    .then_some(current);
            }
            _ => return None,
        }
    }
    None
}

/// How a vector type measures: the `distance:` nearest it in its chain,
/// cosine when none says (area 21, decision 14).
pub(crate) fn metric(lookup: &dyn TypeView, vector: &Ty) -> jadpo_std::VectorDistance {
    lookup
        .rule_chain(vector)
        .iter()
        .find_map(|(_, rules)| rules.distance)
        .unwrap_or(jadpo_std::VectorDistance::Cosine)
}

/// The length a vector column fixes, from its type: `vector(1536)`.
pub(super) fn fixed_length(field: &FieldModel) -> Option<u64> {
    field
        .sql_type
        .as_deref()?
        .strip_prefix("vector(")?
        .strip_suffix(')')?
        .parse()
        .ok()
}

impl Reader<'_> {
    /// A query ordered by `nearest` returning `Page<T>`.
    pub(super) fn nearest_page(&mut self, name: &str, span: &Span, element: &str) {
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_NEAREST_PAGE,
                format!("`{name}` returns its nearest records a page at a time"),
            )
            .at(span.clone(), "a page of nearest records")
            .reason(format!(
                "A page continues after a cursor, and a cursor over distances isn't stable: as records are added and change, the next page could skip a record or show one again. A nearest-neighbour query returns one list, of at most {MOST_NEAREST}."
            ))
            .next(format!(
                "Return `List<Nearest<{element}>>` with `limit:` in place of `page:`"
            )),
        );
    }

    /// The problem when a query's order and its result disagree about
    /// being a nearest-neighbour query.
    pub(super) fn nearest_result(
        &self,
        name: &str,
        result: &Ty,
        span: &Span,
        (written, returned, cardinality): (bool, bool, Cardinality),
        element: &str,
    ) -> Option<Diagnostic> {
        if written && !(returned && cardinality == Cardinality::List) {
            return Some(
                Diagnostic::new(
                    &rule::QUERY_NEAREST_TYPE,
                    format!("`{name}` orders by `nearest` but returns `{result}`"),
                )
                .at(span.clone(), "not `List<Nearest<T>>`")
                .reason("A nearest-neighbour query returns one bounded list of its records, each with the distance it measured: `List<Nearest<T>>`, whose items hold `item` and `distance`.")
                .next(format!("Return `List<Nearest<{element}>>`")),
            );
        }
        if returned && !written {
            return Some(
                Diagnostic::new(
                    &rule::QUERY_NEAREST_TYPE,
                    format!("`{name}` returns `{result}` but doesn't order by `nearest`"),
                )
                .at(span.clone(), "records with a distance")
                .reason("A `Nearest<T>` holds the distance a nearest-neighbour query measured, so only a query ordered by `nearest` returns one: `order: [<Entity>.<vector field> nearest <value>]`.")
                .next(format!(
                    "Order by `nearest`, or return `{}`",
                    match cardinality {
                        Cardinality::List => format!("List<{element}>"),
                        Cardinality::Optional => format!("{element}?"),
                        _ => element.to_string(),
                    }
                )),
            );
        }
        None
    }

    /// `order: [Entity.field nearest value]`, checked: the only order, on a
    /// vector field of fixed length, from a parameter of its type, with a
    /// `limit:` of at most 100.
    pub(super) fn nearest(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        member: &ast::Member,
        limit: Option<&ast::Member>,
    ) -> Option<NearestOrder> {
        let path = scope.path;
        let Some(Expr::ListExpr(list)) = member.value() else {
            return None;
        };
        let items: Vec<Expr> = list.exprs().collect();
        let at = items
            .iter()
            .position(|item| matches!(item, Expr::OrderedExpr(o) if is_nearest(o)))?;
        let Expr::OrderedExpr(ordered) = &items[at] else {
            return None;
        };
        if let Some(other) = items
            .iter()
            .enumerate()
            .find(|(i, _)| *i != at)
            .map(|(_, e)| e)
        {
            let written = shown(other.syntax());
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_NEAREST_TYPE,
                    "`nearest` is the only order a query can have".to_string(),
                )
                .at(Self::span(path, other), "another order beside `nearest`")
                .reason("A nearest-neighbour query comes back nearest first, in the order its index finds the records; sorting by another field as well would read and sort every match, which no index serves.")
                .next(format!("Remove `{written}` from `order:`, and sort the results in code if they need it")),
            );
            return None;
        }
        let field_expr = ordered.expr()?;
        let field = self.field_ref(path, entity, &field_expr)?;
        let written = format!("{}.{}", entity.name, field.name);
        let field_span = Self::span(path, &field_expr);
        if !crate::data::holds_vector(field) {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_NEAREST_TYPE,
                    format!("`{written}` isn't a vector, so records can't be ordered by nearness to it"),
                )
                .at(field_span, format!("`{}`", field_type_name(&field.ty)))
                .reason(format!(
                    "`nearest` orders records by how near a vector field is to a vector, by the distance its type measures; `{written}` is `{}`.",
                    field.ty
                ))
                .next("Order by a vector field, or by this one with `asc` or `desc`"),
            );
            return None;
        }
        let Some(dimensions) = fixed_length(field) else {
            let what = match field.compound {
                Some(compound) => format!("an `{compound}`, whose vector is kept without one"),
                None => format!("a `{}`, kept without one", field.ty.strip_nullable()),
            };
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_NEAREST_UNINDEXED,
                    format!("`{written}` has no fixed length, so no index can order it by nearness"),
                )
                .at(
                    field_span,
                    match field.compound {
                        Some(compound) => format!("an `{compound}`"),
                        None => format!("a `{}` without `dimensions:`", field.ty.strip_nullable()),
                    },
                )
                .reason(format!(
                    "A nearest-neighbour query reads an HNSW index, which pgvector builds only on a vector column of fixed length, and `{written}` is {what}."
                ))
                .next(format!(
                    "Give `{written}` a type built on `Vector` with the model's `dimensions:`"
                )),
            );
            return None;
        };
        let field_ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
        let vector = vector_type(self.lookup, &field_ty)?;
        let metric = metric(self.lookup, &vector);
        // The value it measures from: a parameter of the field's own type.
        let target = ordered.target()?;
        let Some(given) = self.value_type(scope, &target) else {
            let shown_target = shown(target.syntax());
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_NEAREST_TYPE,
                    format!("`nearest` measures from `{shown_target}`, which isn't a parameter"),
                )
                .at(Self::span(path, &target), "not a parameter")
                .reason(format!(
                    "`nearest` measures from a vector the call supplies, a parameter of `{vector}`, the type of `{written}`."
                ))
                .next(format!("Add a `{vector}` parameter and measure from it")),
            );
            return None;
        };
        if given.has_error() {
            return None;
        }
        if self.lookup.is_nullable(&given) {
            self.optional_in_where(scope, &target);
            return None;
        }
        let theirs = vector_type(self.lookup, &given);
        if theirs.as_ref() != Some(&vector) {
            let shown_given = theirs.unwrap_or_else(|| given.clone());
            let label = format!("a `{shown_given}`, not {}", a_or_an(&vector.to_string()));
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_NEAREST_TYPE,
                    format!("`nearest` compares `{written}` with a `{shown_given}`"),
                )
                .at(Self::span(path, &target), label)
                .reason(format!(
                    "A vector type stands for the model that made its vectors and how they are compared, so `nearest` measures from a vector of the field's own type; a `{shown_given}` may come from another model, and its distance from {} would mean nothing.",
                    a_or_an(&vector.to_string())
                ))
                .next(format!("Pass {}, made by the same model", a_or_an(&vector.to_string()))),
            );
            return None;
        }
        let (value, _) = self.value(scope, &target, &field_ty)?;
        // The most records it returns: at most 100.
        let limit_value = limit.and_then(ast::Member::value);
        let most = match limit_value.as_ref().map(Const::from_expr) {
            Some(Const::Number {
                value: n,
                decimal: false,
            }) => Some(u64::try_from(n.mantissa).unwrap_or(u64::MAX)),
            _ => limit_value
                .as_ref()
                .and_then(|value| self.value_type(scope, value))
                .and_then(|ty| {
                    self.lookup
                        .rule_chain(&ty)
                        .iter()
                        .find_map(|(_, rules)| rules.max)
                })
                .and_then(|max| u64::try_from(max.mantissa).ok()),
        };
        // A missing or unbounded `limit:` is reported by the list's own
        // checks.
        let most = most?;
        if most > MOST_NEAREST {
            let value = limit_value?;
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_LIMIT_RANGE,
                    format!("`limit:` allows {most} nearest records, more than {MOST_NEAREST}"),
                )
                .at(Self::span(path, &value), format!("up to {most}"))
                .reason(format!(
                    "A nearest-neighbour query returns at most {MOST_NEAREST} records: its index searches a few candidates beyond its limit, and the further it looks the slower and less exact the search."
                ))
                .next(format!("Give `limit:` at most {MOST_NEAREST}")),
            );
            return None;
        }
        // Over 4,000 dimensions no index serves it: each call reads every
        // record, a cost the author accepts with a decline (area 1).
        let search = VectorSearch::of(dimensions);
        if search == VectorSearch::Exact {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_NEAREST_EXACT,
                    format!(
                        "`{written}` is too long to index, so `nearest` reads every `{}`",
                        entity.name
                    ),
                )
                .at(field_span, format!("{} dimensions", grouped(dimensions)))
                .reason(format!(
                    "pgvector's HNSW index takes a vector of up to {} dimensions, at half precision, and `{vector}` has {}; with no index, each call measures the distance to every `{}` whose `{}` is set and sorts them all, which slows as the table grows.",
                    grouped(MOST_HALF_INDEXED_DIMENSIONS),
                    grouped(dimensions),
                    entity.name,
                    field.name
                ))
                .next(format!(
                    "Give `{vector}` at most {} `dimensions:`, asking the model for fewer with `embedding_dimensions:`",
                    grouped(MOST_HALF_INDEXED_DIMENSIONS)
                )),
            );
        }
        Some(NearestOrder {
            field: field.name.clone(),
            value,
            metric,
            most,
            dimensions,
            search,
        })
    }
}

impl Reader<'_> {
    /// `ai.distance(Entity.field, value) <op> bound` in a `where:` line
    /// (area 21): the field's vector type's distance from a parameter of
    /// the same type, compared with a number by `<`, `<=`, `>` or `>=`.
    pub(super) fn distance_condition(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        call: &ast::CallExpr,
        compare: Compare,
        bound: &Expr,
        whole: &Expr,
    ) -> Option<Condition> {
        let path = scope.path;
        let args: Vec<ast::Arg> = call
            .arg_list()
            .into_iter()
            .flat_map(|list| list.args())
            .collect();
        let positional: Vec<Expr> = args
            .iter()
            .filter(|arg| arg.name().is_none())
            .filter_map(ast::Arg::expr)
            .collect();
        let [first, second] = positional.as_slice() else {
            self.unsupported(
                scope,
                whole,
                "`ai.distance` takes two vectors: a field of the entity, then a parameter of its type.".to_string(),
                format!("Write `ai.distance({}.<field>, <parameter>)`", entity.name),
            );
            return None;
        };
        if args.len() != 2 {
            self.unsupported(
                scope,
                whole,
                "`ai.distance` takes two vectors and no named arguments: a field of the entity, then a parameter of its type.".to_string(),
                format!("Write `ai.distance({}.<field>, <parameter>)`", entity.name),
            );
            return None;
        }
        let Some(field) = self.field_named(entity, first) else {
            self.unsupported(
                scope,
                whole,
                format!(
                    "A distance in a query measures from one of `{}`'s vector fields, named first, to a value the call supplies, named second.",
                    entity.name
                ),
                format!("Write `ai.distance({}.<field>, <parameter>)`", entity.name),
            );
            return None;
        };
        let written = format!("{}.{}", entity.name, field.name);
        if matches!(compare, Compare::Equal | Compare::NotEqual) {
            self.unsupported(
                scope,
                whole,
                "A distance is a float worked out by pgvector, which two computations may round a last place apart, so it is compared with a bound by `<`, `<=`, `>` or `>=`, never tested for equality.".to_string(),
                "Compare the distance with `<=` or `>=`".to_string(),
            );
            return None;
        }
        let field_ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
        let Some(vector) = vector_type(self.lookup, &field_ty) else {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{written}` can't be used where a vector is required"),
                )
                .at(Self::span(path, first), format!("a `{}`", field.ty.strip_nullable()))
                .reason(format!(
                    "`ai.distance` measures between two vectors of one type, values of a type built on `Vector`; `{written}` is `{}`.",
                    field.ty.strip_nullable()
                ))
                .next("Measure from a vector field"),
            );
            return None;
        };
        let Some(given) = self.value_type(scope, second) else {
            let shown_second = shown(second.syntax());
            self.unsupported(
                scope,
                whole,
                format!(
                    "A distance in a query measures to a vector the call supplies, a parameter of `{vector}`; `{shown_second}` isn't one."
                ),
                format!("Add a `{vector}` parameter and measure to it"),
            );
            return None;
        };
        if given.has_error() {
            return None;
        }
        if self.lookup.is_nullable(&given) {
            self.optional_in_where(scope, second);
            return None;
        }
        let theirs = vector_type(self.lookup, &given);
        if theirs.as_ref() != Some(&vector) {
            let shown_given = theirs.unwrap_or_else(|| given.clone());
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{shown_given}` can't be used where `{vector}` is required"),
                )
                .at(Self::span(path, second), format!("this is `{shown_given}`"))
                .reason(format!(
                    "`ai.distance` measures between two vectors of one type, which fixes their dimensions and how they are compared; its first is `{vector}`, and a `{shown_given}` may come from another model or be compared another way."
                ))
                .next(format!("Pass a `{vector}` here")),
            );
            return None;
        }
        let (value, _) = self.value(scope, second, &field_ty)?;
        // The bound: a number, as a literal or a parameter.
        let (number, ty) = self.value(scope, bound, &Ty::Prim(Prim::Decimal))?;
        if self.lookup.is_nullable(&ty) {
            self.optional_in_where(scope, bound);
            return None;
        }
        if !matches!(number, Value::Literal(_))
            && !matches!(
                self.lookup.representation(&ty),
                Some(Prim::Decimal | Prim::Int)
            )
        {
            self.mismatch(scope, bound, &ty, &Ty::Prim(Prim::Decimal));
            return None;
        }
        // `ai.distance`'s form, pgvector's operator by the type's metric,
        // compared with the bound (chunk 6.3b).
        Some(Condition::Call {
            call: Call {
                op: jadpo_std::Op::AiDistance,
                field: field.name.clone(),
                args: vec![
                    CallArg::Value(value, Held::Vector),
                    CallArg::Metric(metric(self.lookup, &vector)),
                ],
                result: Held::Double,
            },
            test: Test::Compare {
                op: compare,
                value: number,
            },
        })
    }
}
