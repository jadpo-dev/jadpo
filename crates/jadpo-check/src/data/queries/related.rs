//! A condition on a related record (area 6, "Filtering by a related
//! record"; chunk 10.11c): `exists(TicketTag.ticket_id, TicketTag.tag_id ==
//! tag_id)` keeps the records some record of another entity refers to
//! through a reference and meets a condition on its own fields, and
//! `exists(Book.author_id, Author.name == name)` those whose own reference
//! points at a record meeting one. The plan writes it as an `EXISTS`
//! subquery, under the related entity's `visible:` and policy.

use super::*;

/// The word a related record's condition is called by.
pub(super) const EXISTS: &str = "exists";

/// Whether a call is `exists(...)`.
pub(super) fn is_exists(call: &ast::CallExpr) -> bool {
    matches!(call.expr(), Some(Expr::NameRef(name)) if tree::name_ref_text(&name).as_deref() == Some(EXISTS))
}

/// Where a related record's own condition stands, which holds no other.
pub(super) const INSIDE: &str = "a related record's condition";

/// How a related record's condition is written, for a reason.
const SHAPE: &str = "A related record is tested with `exists(<Entity>.<reference>, <condition>)`: the reference that links the two entities, then a condition on the related entity's own fields, which may be left out when any related record will do (`jadpo help topic filtering-by-a-related-record`).";

/// The references that link `entity` with `other`, either way, as written.
pub(super) fn links(entity: &EntityModel, other: &EntityModel) -> Vec<String> {
    let mut out = Vec::new();
    for field in &other.fields {
        if field
            .reference
            .as_ref()
            .is_some_and(|r| r.target == entity.name)
        {
            out.push(format!("{}.{}", other.name, field.name));
        }
    }
    for field in &entity.fields {
        if field
            .reference
            .as_ref()
            .is_some_and(|r| r.target == other.name)
        {
            out.push(format!("{}.{}", entity.name, field.name));
        }
    }
    out
}

impl Reader<'_> {
    /// `exists(Related.reference, condition)` in a condition on `entity`.
    pub(super) fn related_condition(
        &mut self,
        scope: &Scope<'_>,
        entity: &EntityModel,
        call: &ast::CallExpr,
        whole: &Expr,
    ) -> Option<Condition> {
        if let Some(place) = self.related_refused {
            self.unsupported(
                scope,
                whole,
                format!("A related record is tested in a query's own `where:` lines, as one condition on the records it reads, not in {place}."),
                "Test the related record in the query's own `where:`".to_string(),
            );
            return None;
        }
        let args: Vec<ast::Arg> = call
            .arg_list()
            .into_iter()
            .flat_map(|list| list.args())
            .collect();
        let positional: Vec<Expr> = args.iter().filter_map(ast::Arg::expr).collect();
        if args.iter().any(|arg| arg.name().is_some())
            || positional.is_empty()
            || positional.len() > 2
            || positional.len() != args.len()
        {
            self.unsupported(
                scope,
                whole,
                SHAPE.to_string(),
                "Write `exists(<Entity>.<reference>, <condition>)`".to_string(),
            );
            return None;
        }
        let via_expr = unparen(&positional[0]);
        let (qualifier, field) = match dotted(&via_expr) {
            Some((first, rest)) if rest.len() == 1 => (
                tree::name_ref_text(&first).unwrap_or_default(),
                tree::name_ref_text(&rest[0]).unwrap_or_default(),
            ),
            _ => (String::new(), String::new()),
        };
        let entities = self.entities;
        // The related record refers here, or this one refers to it.
        let found: Option<(&EntityModel, &FieldModel, bool)> = if qualifier == entity.name {
            entity
                .fields
                .iter()
                .find(|f| f.name == field)
                .and_then(|f| {
                    let target = entities.get(&f.reference.as_ref()?.target)?;
                    Some((target, f, true))
                })
        } else {
            entities.get(&qualifier).and_then(|other| {
                let f = other.fields.iter().find(|f| {
                    f.name == field
                        && f.reference
                            .as_ref()
                            .is_some_and(|r| r.target == entity.name)
                })?;
                Some((other, f, false))
            })
        };
        let via_written = shown(via_expr.syntax());
        let Some((target, via, parent)) = found else {
            // A reference whose type names nothing is resolution's error.
            if entities
                .get(&qualifier)
                .and_then(|e| e.fields.iter().find(|f| f.name == field))
                .is_some_and(|f| f.ty.has_error())
            {
                return None;
            }
            let mut options: Vec<String> = Vec::new();
            for other in &entities.all().entities {
                if other.name != entity.name {
                    options.extend(links(entity, other).into_iter().map(|l| format!("`{l}`")));
                }
            }
            options.sort();
            options.dedup();
            let named = if options.is_empty() {
                format!(" No reference leads to or from `{}`.", entity.name)
            } else {
                format!(" The references here are {}.", crate::and_list(&options))
            };
            self.out.push(
                Diagnostic::new(
                    &rule::QUERY_CONDITION_UNSUPPORTED,
                    format!(
                        "`{via_written}` isn't a reference to or from `{}`",
                        entity.name
                    ),
                )
                .at(Self::span(scope.path, &via_expr), "not a reference")
                .reason(format!(
                    "`exists` follows a reference: another entity's reference to `{e}` asks for a record that refers here, and one of `{e}`'s own asks for the record it points at.{named}",
                    e = entity.name
                ))
                .next(if options.is_empty() {
                    "Add a reference field, or read the records with a query of their own".to_string()
                } else {
                    "Name one of those references first".to_string()
                }),
            );
            return None;
        };
        // The subquery names both tables, so they must differ, and live in
        // one database and one domain.
        let refused = if target.name == entity.name {
            Some((
                format!(
                    "`{via_written}` links `{}` with itself, and a condition on a related record reads another entity's records.",
                    entity.name
                ),
                "Read the related records with a query of their own, by the `id` this one gives"
                    .to_string(),
            ))
        } else if target.store != entity.store || target.redis {
            Some((
                format!(
                    "A query is one statement, which reads one database, and `{}` and `{}` are kept apart.",
                    entity.name, target.name
                ),
                format!("Read the `{}` with a query of its own", target.name),
            ))
        } else if self.domain_of(&target.name) != self.domain_of(&entity.name) {
            Some((
                format!(
                    "`{}` belongs to another domain, whose data comes from its public queries, so it decides what it shares.",
                    target.name
                ),
                format!(
                    "Call a public query of `{}`'s domain, and pass what it gives",
                    target.name
                ),
            ))
        } else {
            None
        };
        if let Some((reason, next)) = refused {
            self.unsupported(scope, whole, reason, next);
            return None;
        }
        let condition = match positional.get(1) {
            Some(inner) => {
                // The related record's condition is on its own fields, and
                // holds no other related record, search or `geo.` test.
                let under = std::mem::take(&mut self.under);
                let refused = self.related_refused.replace(INSIDE);
                let search_place = self.search_refused.replace(INSIDE);
                let read = self.condition(scope, target, inner, false);
                self.under = under;
                self.related_refused = refused;
                self.search_refused = search_place;
                let read = read?;
                if read.searches() {
                    self.unsupported(
                        scope,
                        whole,
                        "A related record's condition is decided by the database alone, inside one `EXISTS`, and a `geo.` test or a search inside text is worked out again in Jadpo's code on the rows a query reads.".to_string(),
                        format!("Test that field of `{}` in a query of its own", target.name),
                    );
                    return None;
                }
                Some(read)
            }
            None => None,
        };
        Some(Condition::Related(Box::new(Related {
            entity: target.name.clone(),
            via: via.name.clone(),
            parent,
            condition,
        })))
    }

    /// The domain an entity is declared in, when it is in one.
    fn domain_of(&self, entity: &str) -> Option<String> {
        self.lookup
            .declared(entity)
            .iter()
            .find(|s| s.kind == ItemKind::Entity)
            .and_then(|s| match &s.ownership {
                Ownership::Domain(domain) => Some(domain.clone()),
                _ => None,
            })
    }
}
