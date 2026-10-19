//! A query's `order:`, `limit:` and `page:` (area 6, "Order and
//! pages").

use super::*;

impl Reader<'_> {
    /// `order: [Entity.field asc, ...]`, with `id` added as the tie-break
    /// when the last field could tie (area 6, question 3).
    pub(super) fn order(
        &mut self,
        path: &str,
        entity: &EntityModel,
        member: &ast::Member,
        page: bool,
    ) -> Vec<OrderField> {
        let Some(value) = member.value() else {
            return Vec::new();
        };
        let Expr::ListExpr(list) = &value else {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_ORDER_FIELD,
                    "`order:` is a list of fields".to_string(),
                )
                .at(Self::span(path, &value), "not a list")
                .reason("`order:` lists the fields to sort by in brackets, each with `asc` or `desc`: `order: [<Entity>.<field> desc]`.")
                .next("Write `order: [<Entity>.<field> asc]`"),
            );
            return Vec::new();
        };
        let mut out: Vec<OrderField> = Vec::new();
        let mut spans: Vec<Span> = Vec::new();
        for item in list.exprs() {
            let item_span = Self::span(path, &item);
            let Expr::OrderedExpr(ordered) = &item else {
                let written = shown(item.syntax());
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_ORDER_DIRECTION,
                        format!("`{written}` says no direction"),
                    )
                    .at(item_span.clone(), "`asc` or `desc`?")
                    .reason("Each field of `order:` says which way it sorts: `asc` from the smallest, `desc` from the largest.")
                    .next(format!("Write `{written} asc` or `{written} desc`"))
                    .edit(Span::new(path, item_span.end, item_span.end), " asc"),
                );
                continue;
            };
            let Some(inner) = ordered.expr() else {
                continue;
            };
            let descending = ordered.direction().is_some_and(|d| d.text() == "desc");
            // A function of a field, which no index sorts by; `geo.distance`
            // is read as an order of its own (area 20).
            if let Expr::CallExpr(_) = unparen(&inner) {
                let written = shown(inner.syntax());
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_ORDER_FIELD,
                        format!("Records can't be sorted by `{written}`"),
                    )
                    .at(item_span.clone(), "worked out from a field")
                    .reason("`order:` sorts by the entity's fields, which an index keeps in order; a value worked out from a field isn't one. The one exception is `geo.distance(<Entity>.<point field>, <point>) asc`, nearest first inside a radius (area 20, \"Queries\").")
                    .next(format!("Sort by a field of `{}`", entity.name)),
                );
                continue;
            }
            let Some(field) = self.field_ref(path, entity, &inner) else {
                continue;
            };
            let written = format!("{}.{}", entity.name, field.name);
            if let Some(at) = out.iter().position(|o| o.field == field.name) {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_ORDER_FIELD,
                        format!("`{written}` is in `order:` twice"),
                    )
                    .at(item_span.clone(), "again")
                    .also(spans[at].clone(), "first here")
                    .reason("Records equal on a field are sorted by the fields after it, so a field listed again never changes the order.")
                    .next(format!("Remove the second `{written}`")),
                );
                continue;
            }
            let ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
            let sortable = field.sql_type.as_deref() != Some("jsonb")
                && matches!(
                    self.lookup.representation(&ty),
                    Some(
                        Prim::Int
                            | Prim::Decimal
                            | Prim::Instant
                            | Prim::CalendarDate
                            | Prim::Duration
                            | Prim::Text
                            | Prim::Uuid
                            | Prim::Quantity(_)
                            | Prim::Percentage
                    )
                );
            if !sortable {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_ORDER_FIELD,
                        format!("Records can't be sorted by `{written}`"),
                    )
                    .at(item_span.clone(), format!("`{}`", field_type_name(&field.ty)))
                    .reason(format!(
                        "A query sorts by numbers, times, dates, durations, quantities, percentages, text and identities, which the database and Jadpo order the same way; `{written}` is `{}`.",
                        field.ty
                    ))
                    .next("Sort by another field"),
                );
                continue;
            }
            if page && field.nullable {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_ORDER_FIELD,
                        format!("A page can't be ordered by `{written}`, which may be `none`"),
                    )
                    .at(item_span.clone(), "may be `none`")
                    .reason("The next page starts after the last record's values of the order fields, and a record without a value has no place to start after.")
                    .next("Order the page by a required field"),
                );
                continue;
            }
            spans.push(item_span);
            out.push(OrderField {
                field: field.name.clone(),
                descending,
                added: false,
            });
        }
        if out.is_empty() && list.exprs().next().is_none() {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_ORDER_FIELD,
                    "`order:` lists no fields".to_string(),
                )
                .at(Self::span(path, &value), "empty")
                .reason("`order:` names at least one field to sort by; the compiler adds `id` when records could tie.")
                .next("Write `order: [<Entity>.<field> asc]`"),
            );
        }
        tie_break(entity, &mut out);
        out
    }

    /// `limit:` or a page's `size:`: a whole number from 1, or a value
    /// whose type has a `max:`.
    pub(super) fn bound(
        &mut self,
        scope: &Scope<'_>,
        value: Option<Expr>,
        member: &ast::Member,
        what: &str,
    ) -> Option<Bound> {
        let Some(value) = value else {
            let at = member.member_key().map_or_else(
                || Span::new(scope.path, 0, 0),
                |k| Self::span(scope.path, &k),
            );
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_LIMIT_UNBOUNDED,
                    format!("`{what}:` has no value"),
                )
                .at(at, "no value")
                .reason("Every list is bounded: a number, or a value whose type has a `max:`.")
                .next(format!("Give `{what}:` a number")),
            );
            return None;
        };
        let span = Self::span(scope.path, &value);
        match Const::from_expr(&value) {
            Const::Number {
                value: n,
                decimal: false,
            } => {
                if n.compare(jadpo_std::Num::whole(1)).is_lt() {
                    self.out.push(
                        Diagnostic::new(
                            &rule::QUERY_LIMIT_RANGE,
                            format!("`{what}: {n}` would read nothing"),
                        )
                        .at(span, "below 1")
                        .reason(format!(
                            "`{what}:` is how many records come back at most, so it is at least 1."
                        ))
                        .next(format!("Give `{what}:` a number from 1")),
                    );
                    return None;
                }
                let whole = u64::try_from(n.mantissa).unwrap_or(u64::MAX);
                return Some(Bound::Literal(whole));
            }
            Const::Number { .. }
            | Const::Str(_)
            | Const::Bool(_)
            | Const::None
            | Const::List(_) => {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_MISMATCH,
                        format!(
                            "`{}` can't be used where a whole number is required",
                            shown(value.syntax())
                        ),
                    )
                    .at(span, "not a whole number")
                    .reason(format!(
                        "`{what}:` is how many records come back at most: a whole number."
                    ))
                    .next(format!("Give `{what}:` a whole number")),
                );
                return None;
            }
            _ => {}
        }
        let (bound, ty) = self.value(scope, &value, &Ty::Prim(Prim::Int))?;
        if self.lookup.is_nullable(&ty) {
            self.optional_in_where(scope, &value);
            return None;
        }
        if self.lookup.representation(&ty) != Some(Prim::Int) {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!(
                        "`{}` can't be used where a whole number is required",
                        shown(value.syntax())
                    ),
                )
                .at(span, format!("a `{ty}`"))
                .reason(format!(
                    "`{what}:` is how many records come back at most: a whole number."
                ))
                .next(format!("Give `{what}:` a whole number")),
            );
            return None;
        }
        let max = self
            .lookup
            .rule_chain(&ty)
            .iter()
            .find_map(|(_, rules)| rules.max);
        if max.is_none() {
            let written = shown(value.syntax());
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_LIMIT_UNBOUNDED,
                    format!("`{written}` has no maximum, so the {what} has no bound"),
                )
                .at(span, format!("`{ty}` has no `max:`"))
                .reason(format!(
                    "Every list is bounded: `{what}:` is a number, or a value whose type has a `max:`, such as the built-in `PageSize` (1 to 100)."
                ))
                .next("Type the value as `PageSize`, or as an `Int` type of your own with a `max:`"),
            );
            return None;
        }
        // The least it can be must still read something: PostgreSQL
        // refuses a negative `LIMIT`, and 0 reads nothing.
        let min = self
            .lookup
            .rule_chain(&ty)
            .iter()
            .find_map(|(_, rules)| rules.min);
        if min.is_none_or(|min| min.compare(jadpo_std::Num::whole(1)).is_lt()) {
            let written = shown(value.syntax());
            let least = min.map_or_else(|| "no `min:`".to_string(), |n| format!("`min: {n}`"));
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_LIMIT_RANGE,
                    format!("`{written}` can be below 1, so the {what} could read nothing"),
                )
                .at(span, format!("`{ty}` has {least}"))
                .reason(format!(
                    "`{what}:` is how many records come back at most, so it is at least 1, and a value's type must say so with `min: 1` or more, as the built-in `PageSize` (1 to 100) does."
                ))
                .next(format!("Give `{ty}` `min: 1`, or type the value as `PageSize`")),
            );
            return None;
        }
        Some(Bound::Value(bound))
    }

    /// `page: { size: ... after: ... }`.
    pub(super) fn page(
        &mut self,
        scope: &Scope<'_>,
        member: &ast::Member,
    ) -> (Option<Bound>, Option<Value>) {
        let key_span = member.member_key().map_or_else(
            || Span::new(scope.path, 0, 0),
            |k| Self::span(scope.path, &k),
        );
        let Some(block) = member.block() else {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_PAGE_SETTING,
                    "`page:` is a block".to_string(),
                )
                .at(key_span, "not a block")
                .reason("`page:` gives the page's `size:` and the cursor to continue `after:` in a block.")
                .next("Write `page: { size: ... after: ... }`"),
            );
            return (None, None);
        };
        let mut size = None;
        let mut after = None;
        let mut seen = BTreeSet::new();
        for (name, setting) in tree::members(Some(block.clone())) {
            let Some(word) = tree::name_text(&name) else {
                continue;
            };
            let span = Self::span(scope.path, &name);
            seen.insert(word.clone());
            match word.as_str() {
                "size" => size = self.bound(scope, setting.value(), &setting, "size"),
                "after" => {
                    let Some(value) = setting.value() else {
                        continue;
                    };
                    let wanted = Ty::Prim(Prim::PageCursor).nullable();
                    let Some((value_ref, ty)) = self.value(scope, &value, &wanted) else {
                        continue;
                    };
                    if self.lookup.representation(&ty) != Some(Prim::Text)
                        || !matches!(ty.strip_nullable(), Ty::Prim(Prim::PageCursor))
                            && self.lookup.base_prim(&ty) != Some(Prim::PageCursor)
                    {
                        self.out.push(
                            Diagnostic::new(
                                &rule::QUERY_PAGE_SETTING,
                                format!("`after:` takes a `PageCursor`, and `{}` is `{ty}`", shown(value.syntax())),
                            )
                            .at(Self::span(scope.path, &value), format!("a `{ty}`"))
                            .reason("`after:` is the cursor a previous page gave in its `next`, which a client sends back: a `PageCursor`, usually an optional field of the request's `Input`.")
                            .next("Pass a `PageCursor` here"),
                        );
                        continue;
                    }
                    after = Some(value_ref);
                }
                _ => {
                    let near = nearest(&word, ["size", "after"]);
                    let problem = Diagnostic::new(
                        &rule::QUERY_PAGE_SETTING,
                        format!("`{word}` isn't a page setting"),
                    )
                    .at(span.clone(), "not a setting")
                    .reason(format!(
                        "A page has a `size:` and an `after:` cursor; there is no offset, because a large offset is slow and skips or repeats records when rows are added.{}",
                        near.map(|n| format!(" Did you mean `{n}`?")).unwrap_or_default()
                    ));
                    self.out.push(match near {
                        Some(near) => problem.next(format!("Use `{near}:`")).edit(span, near),
                        None => problem.next(format!("Remove `{word}:`")),
                    });
                }
            }
        }
        for wanted in ["size", "after"] {
            if !seen.contains(wanted) {
                self.out.push(
                    Diagnostic::new(
                        &rule::QUERY_PAGE_SETTING,
                        format!("`page:` has no `{wanted}:`"),
                    )
                    .at(key_span.clone(), format!("no `{wanted}:`"))
                    .reason(if wanted == "size" {
                        "A page's `size:` is how many records it holds: a number, or a value whose type has a `max:`, such as `PageSize`."
                    } else {
                        "A page continues `after:` the cursor the previous page gave in its `next`; on the first call that cursor is absent."
                    })
                    .next(format!("Add `{wanted}: ...` to the block")),
                );
            }
        }
        (size, after)
    }
}
