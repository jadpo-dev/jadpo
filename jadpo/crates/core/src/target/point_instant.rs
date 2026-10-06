//! Only ordinary authored point callers opt in; shared raw consumers stay legacy.
use super::*;
impl TargetGenerator<'_> {
    pub(super) fn point_instant_entities(&self) -> BTreeSet<String> {
        let mut queries = Vec::new();
        for source in &self.project.syntax.sources {
            for declaration in &source.file.declarations {
                match declaration {
                    Declaration::Callable(c) => collect_query_expressions(&c.body, &mut queries),
                    Declaration::Test(t) => collect_query_expressions(&t.body, &mut queries),
                    Declaration::Route(r) => {
                        if let Some(action) = &r.inline_action {
                            collect_query_expressions(&action.body, &mut queries);
                        }
                        if let Some(run) = &r.run {
                            for argument in &run.arguments {
                                collect_queries_from_expression(argument, &mut queries);
                            }
                            for argument in &run.named_arguments {
                                collect_queries_from_expression(&argument.value, &mut queries);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        queries
            .into_iter()
            .filter(|q| {
                q.page.is_none()
                    && q.includes.is_empty()
                    && matches!(
                        q.cardinality,
                        jadpo_syntax::QueryCardinality::Optional
                            | jadpo_syntax::QueryCardinality::Required
                    )
            })
            .map(|q| {
                q.target
                    .path
                    .iter()
                    .map(|p| p.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".")
            })
            .filter(|name| {
                self.records.get(name).is_some_and(|r| {
                    r.is_persistent_entity()
                        && r.fields
                            .iter()
                            .any(|f| self.representation_root_for(&f.field_type) == "Instant")
                })
            })
            .collect()
    }
    pub(super) fn point_instant_descriptors(
        &self,
        entity: &RecordDeclaration,
    ) -> Vec<(String, String, String, bool)> {
        let mut used: BTreeSet<String> =
            entity.fields.iter().map(|f| f.name.text.clone()).collect();
        fn allocate(used: &mut BTreeSet<String>, base: String) -> String {
            let mut candidate = base.clone();
            let mut index = 0_usize;
            while !used.insert(candidate.clone()) {
                index = index
                    .checked_add(1)
                    .expect("a reserved alias cannot exhaust the source field namespace");
                candidate = format!("{base}_{index}");
            }
            candidate
        }
        entity
            .fields
            .iter()
            .enumerate()
            .filter(|(_, f)| self.representation_root_for(&f.field_type) == "Instant")
            .map(|(ordinal, f)| {
                let field = f.name.text.clone();
                let value = allocate(&mut used, format!("jadpo_point_epoch_{ordinal}"));
                let kind = allocate(&mut used, format!("jadpo_point_type_{ordinal}"));
                (
                    field,
                    value,
                    kind,
                    self.reference_is_nullable(&f.field_type),
                )
            })
            .collect()
    }
    pub(super) fn point_instant_projection_sql(
        &self,
        original: &str,
        descriptors: &[(String, String, String, bool)],
    ) -> String {
        let extras = descriptors.iter().map(|(field, value, kind, _)| {
            let field = sql_identifier(field);
            format!("CASE WHEN pg_typeof({field})::text = 'timestamp with time zone' THEN EXTRACT(EPOCH FROM {field}::text::timestamptz)::text ELSE NULL END AS {}, pg_typeof({field})::text AS {}", sql_identifier(value), sql_identifier(kind))
        }).collect::<Vec<_>>().join(", ");
        original.replacen(" FROM ", &format!(", {extras} FROM "), 1)
    }
}
