//! A query's `missing:` failure and its `fails` list (area 6, "Named
//! queries").

use super::*;

impl Reader<'_> {
    /// `missing: Failure` or `missing: Failure { field: value }`.
    pub(super) fn missing(&mut self, scope: &Scope<'_>, member: &ast::Member) -> Option<Missing> {
        let value = member.value()?;
        let (head, record) = match &value {
            Expr::RecordExpr(record) => (record.expr()?, Some(record.clone())),
            other => (other.clone(), None),
        };
        let span = Self::span(scope.path, &head);
        let Expr::NameRef(name_ref) = &head else {
            self.not_failure(span, &tree::written(head.syntax()));
            return None;
        };
        let failure = tree::name_ref_text(name_ref)?;
        let Some(info) = self.lookup.failure_info(&failure).cloned() else {
            self.not_failure(span, &failure);
            return None;
        };
        if let Some(kind) = &info.kind
            && kind != "not_found"
        {
            self.out.push(
                Diagnostic::new(
                    &rule::FAILURE_MAPPING_KIND,
                    format!("`missing:` needs a `not_found` failure, and `{failure}` is `{kind}`"),
                )
                .at(span, format!("kind `{kind}`"))
                .reason("Each mapping's condition fixes the kind of failure it becomes: a read that finds nothing is `not_found`, so the client gets the status that means.")
                .next(format!(
                    "Name a `not_found` failure here, or make `{failure}`'s kind `FailureKind.not_found`"
                )),
            );
            return None;
        }
        let declared = self
            .lookup
            .fields(&failure)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        let mut fields = Vec::new();
        let mut given = BTreeSet::new();
        let mut ok = true;
        for (name, field_member) in tree::members(record.as_ref().and_then(|r| r.member_list())) {
            let Some(field) = tree::name_text(&name) else {
                continue;
            };
            let field_span = Self::span(scope.path, &name);
            let Some(found) = declared.iter().find(|f| f.name == field) else {
                ok = false;
                let names: Vec<String> = declared.iter().map(|f| format!("`{}`", f.name)).collect();
                self.out.push(
                    Diagnostic::new(
                        &rule::FAILURE_UNKNOWN_CONTEXT_FIELD,
                        format!("`{failure}` has no field `{field}`"),
                    )
                    .at(field_span, "not declared")
                    .reason(if names.is_empty() {
                        format!("`{failure}` has no fields. A failure carries only the fields its `client` and `internal` blocks declare.")
                    } else {
                        format!("`{failure}`'s fields are {}. A failure carries only the fields its `client` and `internal` blocks declare.", crate::and_list(&names))
                    })
                    .next(format!("Remove `{field}`")),
                );
                continue;
            };
            given.insert(field.clone());
            let Some(value) = field_member.value() else {
                continue;
            };
            let expected = self
                .lookup
                .field_type(&failure, std::slice::from_ref(&field));
            let Some((value_ref, ty)) = self.value(scope, &value, &expected) else {
                ok = false;
                continue;
            };
            if !matches!(value_ref, Value::Literal(_))
                && !self.lookup.assignable(&ty, &found.declared)
            {
                ok = false;
                let written = shown(value.syntax());
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_MISMATCH,
                        format!(
                            "`{written}` can't be used where `{}` is required",
                            found.declared
                        ),
                    )
                    .at(Self::span(scope.path, &value), format!("a `{ty}`"))
                    .reason(format!(
                        "`{failure}.{field}` is `{}`, and `{written}` is `{ty}`.",
                        found.declared
                    ))
                    .next(format!("Give `{field}` a `{}` value", found.declared)),
                );
                continue;
            }
            fields.push((field, value_ref));
        }
        let missing: Vec<String> = declared
            .iter()
            .filter(|f| !given.contains(&f.name))
            .map(|f| format!("`{}`", f.name))
            .collect();
        if !missing.is_empty() {
            ok = false;
            let first = missing[0].trim_matches('`').to_string();
            self.out.push(
                Diagnostic::new(
                    &rule::FAILURE_MISSING_CONTEXT_FIELD,
                    format!("`{failure}` is missing {}", crate::and_list(&missing)),
                )
                .at(span, "fields missing")
                .reason("A failure is raised with every field its `client` and `internal` blocks declare, so the client and the log always get them.")
                .next(match record {
                    Some(_) => format!("Add `{first}: ...` to the block"),
                    None => format!("Give its fields in a block: `missing: {failure} {{ {first}: ... }}`"),
                }),
            );
        }
        // Fields in the failure's declared order.
        fields.sort_by_key(|(name, _)| declared.iter().position(|f| f.name == *name));
        ok.then_some(Missing { failure, fields })
    }

    pub(super) fn not_failure(&mut self, span: Span, written: &str) {
        self.out.push(
            Diagnostic::new(
                &rule::SEMANTIC_UNKNOWN_NAME,
                format!("`{written}` isn't a declared failure"),
            )
            .at(span, "not a failure")
            .reason("`missing:` names the `not_found` failure a caller gets when no record matches, declared in the domain's `failures/` folder.")
            .next(format!(
                "Declare `type {written} = Failure {{ kind: FailureKind.not_found }}`, or name another failure"
            )),
        );
    }

    /// The `fails` list says exactly what can leave the query: its
    /// `missing:` failure, when it has one.
    pub(super) fn fails(
        &mut self,
        path: &str,
        decl: &ast::QueryDecl,
        name: &str,
        missing: Option<&Missing>,
        member: Option<&ast::Member>,
    ) {
        let listed: Vec<(String, Span)> = decl
            .fails_clause()
            .into_iter()
            .flat_map(|clause| clause.types())
            .map(|ty| (tree::written(ty.syntax()), Self::span(path, &ty)))
            .collect();
        if let Some(missing) = missing
            && !listed
                .iter()
                .any(|(written, _)| *written == missing.failure)
        {
            let at = member
                .and_then(|m| m.value())
                .map_or_else(|| Span::new(path, 0, 0), |v| Self::span(path, &v));
            let failure = &missing.failure;
            self.out.push(
                Diagnostic::new(
                    &rule::FAILURE_UNDECLARED_PROPAGATION,
                    format!("`{failure}` can leave `{name}`, but its `fails` list doesn't say so"),
                )
                .at(at, "raised when nothing matches")
                .reason("`fails` lists exactly the failures that can leave a query, so every caller knows what it must handle.")
                .next(format!("Add `fails {failure}` before the result type")),
            );
        }
        for (written, span) in listed {
            if missing.is_some_and(|m| m.failure == written) {
                continue;
            }
            // A `missing:` that was reported keeps its failure listed.
            if missing.is_none() && member.is_some() {
                continue;
            }
            self.out.push(
                Diagnostic::new(
                    &rule::FAILURE_STALE_DECLARATION,
                    format!("`{written}` can never leave `{name}`"),
                )
                .at(span.clone(), "never raised")
                .reason("A query raises only its `missing:` failure, when it returns exactly one record, and `fails` lists exactly the failures that can leave, so callers aren't made to handle one that never happens.")
                .next(format!("Remove `{written}` from `fails`")),
            );
        }
    }
}
