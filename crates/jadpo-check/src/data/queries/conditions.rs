//! A query's conditions (area 6, "Filters"): each `where:` and
//! `where_present:` line read into a [`Condition`] on the entity's
//! fields, the field each comparison tests, and the rule that a query
//! for one record finds it by a unique field.

use super::*;

impl Reader<'_> {
    /// One `where:` or `where_present:` line.
    pub(super) fn filter_line(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        member: &ast::Member,
        present: bool,
    ) -> Option<Condition> {
        let value = member.value()?;
        if !present {
            return self.condition(scope, entity, &value, false);
        }
        // `where_present:` holds one comparison with a value that may be
        // absent.
        let expr = unparen(&value);
        if let Expr::CallExpr(call) = &expr
            && geo_condition_name(call).is_some()
        {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_CONDITION_UNSUPPORTED,
                    "`where_present:` can't hold a `geo.` condition".to_string(),
                )
                .at(Self::span(scope.path, &value), "a `geo.` condition")
                .reason("`where_present:` applies a field's comparison only when its value is there; a `geo.` condition's value is always given, in a `where:` line (area 20, \"Queries\").")
                .next("Write the condition in `where:`")
                .edit(
                    member.member_key().map_or_else(|| Self::span(scope.path, &value), |k| Self::span(scope.path, &k)),
                    "where",
                ),
            );
            return None;
        }
        let comparison = match &expr {
            Expr::BinExpr(binary) => binary
                .op()
                .is_some_and(|op| compare_token(op.text()).is_some() || op.text() == "in"),
            _ => false,
        };
        if !comparison {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_CONDITION_UNSUPPORTED,
                    "`where_present:` holds one comparison".to_string(),
                )
                .at(Self::span(scope.path, &value), "not one comparison")
                .reason("`where_present:` applies its condition only when the value on the right is there, which is how a request's optional filters are written: one field compared with one optional value, `<Entity>.<field> == filter.<field>`.")
                .next("Write one comparison, or move the condition to `where:`"),
            );
            return None;
        }
        let distance = matches!(&expr, Expr::BinExpr(binary)
            if matches!(binary.lhs().map(|lhs| unparen(&lhs)), Some(Expr::CallExpr(call)) if is_distance(&call)));
        if distance {
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_CONDITION_UNSUPPORTED,
                    "`where_present:` can't hold a distance".to_string(),
                )
                .at(Self::span(scope.path, &value), "a distance")
                .reason("`where_present:` applies a field's comparison only when its value is there; a distance's vector and its bound are both always given, in a `where:` line.")
                .next("Write the distance in `where:`")
                .edit(
                    member.member_key().map_or_else(|| Self::span(scope.path, &value), |k| Self::span(scope.path, &k)),
                    "where",
                ),
            );
            return None;
        }
        let condition = self.condition(scope, entity, &expr, true)?;
        let (value_ref, value_expr) = match (&condition, &expr) {
            (
                Condition::Compare { value, .. }
                | Condition::In { value, .. }
                | Condition::Call {
                    test: Test::Compare { value, .. } | Test::In { value },
                    ..
                },
                Expr::BinExpr(b),
            ) => (value.clone(), b.rhs()),
            _ => return None,
        };
        let optional = value_expr
            .as_ref()
            .and_then(|rhs| self.value_type(scope, rhs))
            .is_some_and(|ty| ty.has_error() || self.lookup.is_nullable(&ty));
        if !optional {
            let written = value_expr
                .as_ref()
                .map(|e| tree::written(e.syntax()))
                .unwrap_or_default();
            let span = value_expr.as_ref().map_or_else(
                || Self::span(scope.path, &value),
                |e| Self::span(scope.path, e),
            );
            let key = member
                .member_key()
                .map_or_else(|| span.clone(), |k| Self::span(scope.path, &k));
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_OPTIONAL_VALUE,
                    format!("`{written}` is always there, so `where_present:` would always apply"),
                )
                .at(span, "never absent")
                .reason("`where_present:` is for a value that may be absent, such as an optional field of a request's `Input`; a condition that always applies is a `where:` line.")
                .next("Write `where:` instead")
                .edit(key, "where"),
            );
            return None;
        }
        Some(Condition::Present {
            value: value_ref,
            condition: Box::new(condition),
        })
    }

    /// A condition on `entity`'s fields. `optional` allows the compared
    /// value to be absent (`where_present:`).
    pub(super) fn condition(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        expr: &Expr,
        optional: bool,
    ) -> Option<Condition> {
        let expr = unparen(expr);
        match &expr {
            Expr::BinExpr(binary) => {
                let op = binary.op()?.text().to_string();
                let lhs = binary.lhs()?;
                let rhs = binary.rhs()?;
                match op.as_str() {
                    "and" | "or" => {
                        if op == "or" {
                            self.under.push("or");
                        }
                        let left = self.condition(scope, entity, &lhs, optional);
                        let right = self.condition(scope, entity, &rhs, optional);
                        if op == "or" {
                            self.under.pop();
                        }
                        let (left, right) = (left?, right?);
                        let mut parts = Vec::new();
                        for part in [left, right] {
                            match (op.as_str(), part) {
                                ("and", Condition::And(inner)) | ("or", Condition::Or(inner)) => {
                                    parts.extend(inner)
                                }
                                (_, other) => parts.push(other),
                            }
                        }
                        Some(if op == "and" {
                            Condition::And(parts)
                        } else {
                            Condition::Or(parts)
                        })
                    }
                    "in" => {
                        // An operation's result among a list's values
                        // (chunk 6.3b).
                        if let Expr::CallExpr(call) = unparen(&lhs)
                            && standard_operation(&call).is_some()
                            && !is_distance(&call)
                            && geo_condition_name(&call).is_none()
                        {
                            return self.operation_condition(
                                scope,
                                entity,
                                &call,
                                Asked::In(&rhs),
                                &expr,
                                optional,
                            );
                        }
                        let field = self.condition_field(scope, entity, &lhs, &expr)?;
                        let field_ty = self.field_ty(entity, field);
                        let wanted = Ty::List(Box::new(field_ty.strip_nullable().clone()));
                        let (value, ty) = self.value(scope, &rhs, &wanted)?;
                        if !optional && self.lookup.is_nullable(&ty) {
                            self.optional_in_where(scope, &rhs);
                            return None;
                        }
                        let element = match self.lookup.structure(&ty) {
                            Ty::List(inner) | Ty::Set(inner) => Some(*inner),
                            _ => None,
                        };
                        match element {
                            Some(element) if self.comparable(&element, &field_ty) => {}
                            _ => {
                                self.mismatch(scope, &rhs, &ty, &wanted);
                                return None;
                            }
                        }
                        Some(Condition::In {
                            field: field.name.clone(),
                            value,
                        })
                    }
                    // `Entity matches phrase` (area 16, "Search"; chunk
                    // B12).
                    "matches" => self.matches_condition(scope, entity, &lhs, &rhs, &expr, optional),
                    token => {
                        let Some(compare) = compare_token(token) else {
                            self.unsupported(
                                scope,
                                &expr,
                                format!("`{token}` isn't one of a query's operators: conditions compare a field with `==`, `!=`, `<`, `<=`, `>`, `>=` or `in`, and join with `and`, `or` and `not`."),
                                "Compare the field with `==`, `!=`, `<`, `<=`, `>`, `>=` or `in`".to_string(),
                            );
                            return None;
                        };
                        // `ai.distance(Entity.field, value) <= bound` (area 21).
                        if let Expr::CallExpr(call) = unparen(&lhs)
                            && is_distance(&call)
                        {
                            return self
                                .distance_condition(scope, entity, &call, compare, &rhs, &expr);
                        }
                        // A standard operation where a field would stand
                        // (area 6, "Operations in conditions", chunk 6.3b).
                        if let Expr::CallExpr(call) = unparen(&lhs)
                            && standard_operation(&call).is_some()
                            && geo_condition_name(&call).is_none()
                        {
                            return self.operation_condition(
                                scope,
                                entity,
                                &call,
                                Asked::Compare(compare, &rhs),
                                &expr,
                                optional,
                            );
                        }
                        let field = self.condition_field(scope, entity, &lhs, &expr)?;
                        let field_ty = self.field_ty(entity, field);
                        if matches!(Const::from_expr(&rhs), Const::None)
                            && matches!(compare, Compare::Equal | Compare::NotEqual)
                        {
                            if !field.nullable {
                                let written = format!("{}.{}", entity.name, field.name);
                                self.out.push(
                                    Diagnostic::new(
                                        &rule::TYPE_INCOMPARABLE,
                                        format!("`{written}` is never `none`"),
                                    )
                                    .at(Self::span(scope.path, &expr), format!("`{field_ty}` compared with `none`"))
                                    .reason(format!(
                                        "Only a nullable field, declared `T?`, can be `none`; `{written}` is `{field_ty}`."
                                    ))
                                    .next("Remove the condition, since it is always the same"),
                                );
                                return None;
                            }
                            return Some(Condition::IsNone {
                                field: field.name.clone(),
                                negated: compare == Compare::NotEqual,
                            });
                        }
                        if compare.is_ordering() && !self.ordered(&field_ty) {
                            let written = format!("{}.{}", entity.name, field.name);
                            self.unsupported(
                                scope,
                                &expr,
                                format!("`{token}` orders numbers, times, dates, durations and quantities, as it does in a body, and `{written}` is `{}`.", field.ty.strip_nullable()),
                                format!("Compare `{written}` with `==`, `!=` or `in`"),
                            );
                            return None;
                        }
                        let wanted = field_ty.strip_nullable().clone();
                        let (value, ty) = self.value(scope, &rhs, &wanted)?;
                        if !optional && self.lookup.is_nullable(&ty) {
                            self.optional_in_where(scope, &rhs);
                            return None;
                        }
                        if !matches!(value, Value::Literal(_)) && !self.comparable(&ty, &field_ty) {
                            self.mismatch(scope, &rhs, &ty, &wanted);
                            return None;
                        }
                        Some(Condition::Compare {
                            field: field.name.clone(),
                            op: compare,
                            value,
                        })
                    }
                }
            }
            Expr::PrefixExpr(prefix) if prefix.op().is_some_and(|op| op.text() == "not") => {
                self.under.push("not");
                let inner = prefix
                    .expr()
                    .and_then(|inner| self.condition(scope, entity, &inner, optional));
                self.under.pop();
                Some(Condition::Not(Box::new(inner?)))
            }
            // `geo.within`, `geo.contains` and `geo.intersects` (area 20,
            // "Queries").
            Expr::CallExpr(call) if geo_condition_name(call).is_some() => {
                self.geo_condition(scope, entity, call, &expr)
            }
            // A standard operation that gives a `Bool`, as the condition
            // itself (chunk 6.3b).
            Expr::CallExpr(call) if standard_operation(call).is_some() && !is_distance(call) => {
                self.operation_condition(scope, entity, call, Asked::Holds, &expr, optional)
            }
            _ => {
                let (reason, next) = match self.field_named(entity, &expr) {
                    Some(field) => (
                        format!(
                            "A condition compares a field with a value; `{}.{}` alone says nothing to compare it with.",
                            entity.name, field.name
                        ),
                        format!("Write `{}.{} == true`", entity.name, field.name),
                    ),
                    None => (
                        "A condition compares one of the entity's fields with a value, so the database can use an index for it.".to_string(),
                        format!("Compare a field of `{}` with a value", entity.name),
                    ),
                };
                self.unsupported(scope, &expr, reason, next);
                None
            }
        }
    }

    /// The field a comparison tests: `Todo.status`, on the left.
    pub(super) fn condition_field<'e>(
        &mut self,
        scope: &Scope<'_>,
        entity: &'e EntityModel,
        lhs: &Expr,
        whole: &Expr,
    ) -> Option<&'e FieldModel> {
        let lhs = unparen(lhs);
        if let Expr::CallExpr(call) = &lhs
            && call
                .arg_list()
                .into_iter()
                .flat_map(|list| list.args())
                .filter_map(|arg| arg.expr())
                .any(|arg| self.field_named(entity, &arg).is_some())
        {
            self.unsupported(
                scope,
                whole,
                "A function applied to a field changes it before it is compared, so no index on the field can serve the condition.".to_string(),
                "Compare the field itself, and work the value out before the query".to_string(),
            );
            return None;
        }
        if let Some((first, rest)) = dotted(&lhs)
            && rest.len() == 1
            && starts_upper(&tree::name_ref_text(&first).unwrap_or_default())
        {
            let field = self.field_ref(scope.path, entity, &lhs)?;
            // A compound type is stored in several columns, which a
            // query's condition doesn't compare yet (area 19).
            if field.compound.is_some() {
                let written = shown(whole.syntax());
                self.out.push(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_NOT_BUILT_YET,
                        format!("Comparing `{}.{}` in a query isn't built yet", entity.name, field.name),
                    )
                    .at(
                        Self::span(scope.path, whole),
                        format!("a `{}`", field.compound.unwrap_or_default()),
                    )
                    .reason(format!(
                        "`{}.{}` is a `{}`, stored in several columns (an amount of money in its minor units and its currency, area 19), and `{written}` would compare them all, which this compiler's queries don't do yet.",
                        entity.name,
                        field.name,
                        field.compound.unwrap_or_default()
                    ))
                    .next("Filter the records after the query for now"),
                );
                return None;
            }
            // A structured value is stored whole as its JSON, which a
            // query's condition doesn't compare yet (chunks 5.5w2b and
            // 5.9b): PostgreSQL compares `jsonb` by its own rules, not by
            // Jadpo's `==`.
            if let Some(held) = &field.json {
                self.out.push(
                    Diagnostic::new(
                        &rule::TOOLCHAIN_NOT_BUILT_YET,
                        format!("Comparing `{}.{}` in a query isn't built yet", entity.name, field.name),
                    )
                    .at(Self::span(scope.path, whole), format!("a `{held}`"))
                    .reason(format!(
                        "`{}.{}` is a `{held}`, stored whole as its JSON in one `jsonb` column (area 5), and this compiler's queries don't compare such a column yet.",
                        entity.name, field.name
                    ))
                    .next("Filter the records after the query for now"),
                );
                return None;
            }
            return Some(field);
        }
        let reason = match self.field_in(scope, entity, whole) {
            true => {
                "Write the field on the left and the value on the right: `<Entity>.<field> > value`."
                    .to_string()
            }
            false => format!(
                "A condition compares one of `{}`'s fields, named with its entity (`{}.field`), with a value.",
                entity.name, entity.name
            ),
        };
        self.unsupported(
            scope,
            whole,
            reason,
            format!("Write `{}.<field>` on the left", entity.name),
        );
        None
    }

    /// Whether a comparison names a field of `entity` on its right.
    pub(super) fn field_in(&self, _scope: &Scope<'_>, entity: &EntityModel, expr: &Expr) -> bool {
        match unparen(expr) {
            Expr::BinExpr(binary) => binary
                .rhs()
                .is_some_and(|rhs| self.field_named(entity, &rhs).is_some()),
            _ => false,
        }
    }

    /// `expr` as one of `entity`'s fields, without reporting anything.
    pub(super) fn field_named<'e>(
        &self,
        entity: &'e EntityModel,
        expr: &Expr,
    ) -> Option<&'e FieldModel> {
        let (first, rest) = dotted(&unparen(expr))?;
        let [field] = rest.as_slice() else {
            return None;
        };
        if tree::name_ref_text(&first)? != entity.name {
            return None;
        }
        let field = tree::name_ref_text(field)?;
        entity.fields.iter().find(|f| f.name == field)
    }

    /// `Entity.field`, checked against the entity the query or include
    /// reads.
    pub(super) fn field_ref<'e>(
        &mut self,
        path: &str,
        entity: &'e EntityModel,
        expr: &Expr,
    ) -> Option<&'e FieldModel> {
        let (first, rest) = dotted(expr)?;
        let [field_node] = rest.as_slice() else {
            return None;
        };
        let qualifier = tree::name_ref_text(&first)?;
        let field = tree::name_ref_text(field_node)?;
        let span = Self::span(path, expr);
        if qualifier != entity.name {
            let other = self.entities.get(&qualifier).is_some();
            let problem = Diagnostic::new(
                &rule::QUERY_UNKNOWN_FIELD,
                format!("`{qualifier}.{field}` isn't a field of `{}`", entity.name),
            )
            .at(Self::span(path, &first), format!("not `{}`", entity.name))
            .reason(if other {
                format!(
                    "This part of the query reads `{}` records, and `{qualifier}` is another entity; a related entity's records come in through `include:`.",
                    entity.name
                )
            } else {
                format!(
                    "Fields are named with their entity, and this part of the query reads `{}`.",
                    entity.name
                )
            });
            self.out
                .push(if entity.fields.iter().any(|f| f.name == field) {
                    problem
                        .next(format!("Write `{}.{field}`", entity.name))
                        .edit(Self::span(path, &first), entity.name.clone())
                } else {
                    problem.next(format!("Name a field of `{}`", entity.name))
                });
            return None;
        }
        match entity.fields.iter().find(|f| f.name == field) {
            Some(found) => Some(found),
            None => {
                let names: Vec<&str> = entity.fields.iter().map(|f| f.name.as_str()).collect();
                let near = nearest(&field, names.iter().copied());
                let listed: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
                let field_span = Self::span(path, field_node);
                let problem = Diagnostic::new(
                    &rule::QUERY_UNKNOWN_FIELD,
                    format!("`{}` has no field `{field}`", entity.name),
                )
                .at(field_span.clone(), "not declared")
                .reason(format!(
                    "`{}`'s fields are {}.{}",
                    entity.name,
                    crate::and_list(&listed),
                    near.map(|n| format!(" Did you mean `{n}`?"))
                        .unwrap_or_default()
                ));
                let _ = span;
                self.out.push(match near {
                    Some(near) => problem
                        .next(format!("Use `{near}` instead of `{field}`"))
                        .edit(field_span, near),
                    None => problem.next(format!("Name a field of `{}`", entity.name)),
                });
                None
            }
        }
    }

    /// A field's type as a condition compares it: `Todo.status`, or
    /// `Todo.status?` for a nullable field.
    pub(super) fn field_ty(&self, entity: &EntityModel, field: &FieldModel) -> Ty {
        // A field whose type names nothing absorbs what is compared with it.
        if field.ty.has_error() {
            return Ty::Error;
        }
        let ty = Ty::Field(entity.name.clone(), vec![field.name.clone()]);
        if field.nullable { ty.nullable() } else { ty }
    }

    /// Whether `<` and the rest order values of `ty`, as they do in a body
    /// (area 3): numbers, times, dates, durations and quantities.
    pub(super) fn ordered(&self, ty: &Ty) -> bool {
        ordered(self.lookup, ty)
    }

    pub(super) fn comparable(&self, a: &Ty, b: &Ty) -> bool {
        comparable(self.lookup, a, b)
    }

    pub(super) fn unsupported(
        &mut self,
        scope: &Scope<'_>,
        expr: &Expr,
        reason: String,
        next: String,
    ) {
        let written = shown(expr.syntax());
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_CONDITION_UNSUPPORTED,
                format!("`{written}` isn't a condition a query can use"),
            )
            .at(Self::span(scope.path, expr), "not an indexable condition")
            .reason(reason)
            .next(next),
        );
    }

    /// `Entity matches phrase` (area 16, "Search"; chunk B12): the
    /// records whose searchable words meet what a `SearchText` parameter
    /// asks for. It stands on a `where:` line, alone or joined by `and`,
    /// since a record ranks by how well it matches and no index serves a
    /// search under `not` or `or`.
    fn matches_condition(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        lhs: &Expr,
        rhs: &Expr,
        whole: &Expr,
        optional: bool,
    ) -> Option<Condition> {
        let name = &entity.name;
        let named = match unparen(lhs) {
            Expr::NameRef(written) => tree::name_ref_text(&written),
            _ => None,
        };
        if named.as_deref() != Some(name.as_str()) {
            let reason = match self.field_named(entity, lhs) {
                Some(field) => format!(
                    "`matches` searches a record's words, in every field its entity's `search:` lists, so it is written of the entity, `{name} matches <phrase>`, not of one field such as `{name}.{}`.",
                    field.name
                ),
                None => format!(
                    "`matches` searches the records of the entity the query reads by words: `{name} matches <phrase>`."
                ),
            };
            self.unsupported(
                scope,
                whole,
                reason,
                format!("Write `{name} matches <phrase>`"),
            );
            return None;
        }
        if let Some(place) = self.search_refused {
            self.unsupported(
                scope,
                whole,
                format!("A search ranks the records a query reads by how well they match, so `matches` stands in a query's own `where:` lines, not in {place}."),
                "Search in a query of its own".to_string(),
            );
            return None;
        }
        if optional || !self.under.is_empty() {
            let (reason, next) = if optional {
                (
                    "A search's phrase is always given: `where_present:` is for a comparison whose value may be absent. A request whose search box may be empty calls a query without `matches` when it is.".to_string(),
                    "Write the search in `where:`".to_string(),
                )
            } else {
                (
                    format!(
                        "A search ranks the records it finds by how well they match, and its index finds the records that match, never those that don't, so `matches` stands on a `where:` line, alone or joined by `and`, never under `{}`.",
                        self.under.last().copied().unwrap_or("not")
                    ),
                    format!("Write `where: {name} matches <phrase>` on a line of its own"),
                )
            };
            self.unsupported(scope, whole, reason, next);
            return None;
        }
        let Some(search) = &entity.search else {
            if !entity.search_written {
                self.out.push(
                    Diagnostic::new(
                        &rule::SEARCH_UNDECLARED,
                        format!("`{name}` can't be searched by words"),
                    )
                    .at(Self::span(scope.path, whole), "no `search:`")
                    .reason(format!(
                        "A query finds records by words only in the fields its entity's `search:` lists, which the compiler indexes; `{name}` has no `search:` section."
                    ))
                    .next(format!(
                        "Add `search: {{ fields: [...] language: SearchLanguage.english }}` to `{name}`, listing the text fields people search by"
                    )),
                );
            }
            return None;
        };
        let wanted = Ty::Prim(Prim::SearchText);
        let (value, ty) = self.value(scope, rhs, &wanted)?;
        if self.lookup.is_nullable(&ty) {
            self.optional_in_where(scope, rhs);
            return None;
        }
        let searched = matches!(ty.strip_nullable(), Ty::Prim(Prim::SearchText))
            || self.lookup.base_prim(&ty) == Some(Prim::SearchText);
        if !matches!(value, Value::Param { .. }) || (!searched && !ty.has_error()) {
            let written = shown(rhs.syntax());
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{written}` isn't a `SearchText`"),
                )
                .at(Self::span(scope.path, rhs), format!("a `{ty}`"))
                .reason("`matches` searches for what someone typed into a search box, a `SearchText` parameter (or a field of one), read as words, quoted phrases and `-word`.")
                .next("Pass a `SearchText` here, such as a parameter `search_phrase: SearchText`"),
            );
            return None;
        }
        Some(Condition::Matches(jadpo_sql::Search {
            fields: search.fields.clone(),
            language: search.language,
            value,
        }))
    }

    pub(super) fn optional_in_where(&mut self, scope: &Scope<'_>, value: &Expr) {
        let written = shown(value.syntax());
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_OPTIONAL_VALUE,
                format!("`{written}` may be absent, so this `where:` line can't always be met"),
            )
            .at(Self::span(scope.path, value), "may be `none`")
            .reason("A `where:` line applies to every call; a condition that should apply only when its value is there, as a request's optional filter, is written with `where_present:`.")
            .next("Write `where_present:` for this condition"),
        );
    }

    pub(super) fn mismatch(&mut self, scope: &Scope<'_>, value: &Expr, actual: &Ty, expected: &Ty) {
        let written = shown(value.syntax());
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{written}` can't be compared with `{expected}`"),
            )
            .at(Self::span(scope.path, value), format!("a `{actual}`"))
            .reason(format!(
                "A condition compares a field with a value of the same type, after widening; `{written}` is `{actual}`."
            ))
            .next(format!("Compare with a `{expected}` value")),
        );
    }

    /// A query for at most one record finds it by a unique field.
    pub(super) fn unique(
        &mut self,
        path: &str,
        entity: &EntityModel,
        sections: &Sections,
        filter: &[Condition],
        span: &Span,
        result: &Ty,
    ) {
        let mut pinned: BTreeSet<&str> = BTreeSet::new();
        let mut lines = filter.iter();
        for (present, _) in &sections.filters {
            let Some(condition) = lines.next() else { break };
            if *present {
                continue;
            }
            let mut equal = |c: &Condition| {
                if let Condition::Compare {
                    field,
                    op: Compare::Equal,
                    ..
                } = c
                {
                    entity
                        .fields
                        .iter()
                        .find(|f| f.name == *field)
                        .map(|f| f.name.as_str())
                } else {
                    None
                }
            };
            match condition {
                Condition::And(parts) => pinned.extend(parts.iter().filter_map(&mut equal)),
                other => pinned.extend(equal(other)),
            }
        }
        if filter.len() < sections.filters.len() {
            // A line was wrong, and has been reported.
            return;
        }
        // On a tenant's records each key holds the tenancy reference too
        // (area 9, "A unique rule holds within its tenant").
        let tested = |field: &str| pinned.contains(field);
        let unique = pinned.contains("id")
            || entity.fields.iter().any(|f| {
                f.unique
                    && self
                        .entities
                        .key_decides(entity, &[f.name.as_str()], &tested)
            })
            || entity.unique.iter().any(|rule| {
                let fields: Vec<&str> = rule.fields.iter().map(String::as_str).collect();
                self.entities.key_decides(entity, &fields, &tested)
            });
        if unique {
            return;
        }
        let within = self.entities.unique_within(&entity.name);
        let with_tenancy = |keys: String, fields: &[String]| match &within {
            Some(via) if !fields.contains(via) => format!("{keys} with `{}.{via}`", entity.name),
            _ => keys,
        };
        let at = sections
            .filters
            .first()
            .and_then(|(_, m)| m.member_key())
            .map_or_else(|| span.clone(), |k| Self::span(path, &k));
        let mut keys: Vec<String> = vec![format!("`{}.id`", entity.name)];
        keys.extend(
            entity
                .fields
                .iter()
                .filter(|f| f.unique && f.name != "id")
                .map(|f| {
                    with_tenancy(
                        format!("`{}.{}`", entity.name, f.name),
                        std::slice::from_ref(&f.name),
                    )
                }),
        );
        keys.extend(
            entity
                .unique
                .iter()
                .map(|rule| with_tenancy(format!("`{}`'s fields", rule.name), &rule.fields)),
        );
        self.out.push(
            Diagnostic::new(
                &rule::QUERY_NOT_UNIQUE,
                format!("A query returning `{result}` could match several records"),
            )
            .at(at, "not a unique field")
            .reason(format!(
                "`{result}` is at most one record, so a `where:` line must test a unique field with `==`: {}.",
                crate::and_list(&keys)
            ))
            .next("Test a unique field with `==`, or return `List<T>`"),
        );
    }
}
