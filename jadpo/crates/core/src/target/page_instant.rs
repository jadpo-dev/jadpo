//! Ordinary checked noinclude pages preserve raw SQL snapshots and decode only returned fields.
use super::*;
impl TargetGenerator<'_> {
    pub(super) fn page_instant_entities(&self) -> BTreeSet<String> {
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
            .filter(|q| q.page.is_some() && q.includes.is_empty())
            .filter_map(|q| {
                let name = q
                    .target
                    .path
                    .iter()
                    .map(|p| p.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                (!self
                    .page_instant_descriptors(q.page.as_ref().unwrap(), &name)
                    .is_empty())
                .then_some(name)
            })
            .collect()
    }
    pub(super) fn page_instant_descriptors(
        &self,
        page: &jadpo_syntax::QueryPage,
        name: &str,
    ) -> Vec<(String, String, String, bool)> {
        let Some(entity) = self.records.get(name).filter(|e| e.is_persistent_entity()) else {
            return Vec::new();
        };
        let projection = self
            .records
            .get(&type_name(&page.projection))
            .expect("checked page projection exists");
        let fields: BTreeSet<&str> = projection
            .fields
            .iter()
            .map(|f| f.name.text.as_str())
            .chain(page.cursor_fields.iter().map(|f| f.text.as_str()))
            .collect();
        // Allocate over the full entity first to preserve collision reservations and stable ordinal aliases.
        self.point_instant_descriptors(entity)
            .into_iter()
            .filter(|(field, _, _, _)| fields.contains(field.as_str()))
            .collect()
    }
    pub(super) fn page_instant_decode(
        &self,
        field: &FieldDeclaration,
        entity: &str,
        row: &str,
        path: &str,
        operation: &str,
    ) -> String {
        let sqlite_decode = self.database_decode_expression(&field.field_type, "raw", "fieldPath");
        format!(
            "decodePageInstant({row}, {}, {}, {}, {path}, {}, (raw, fieldPath) => {sqlite_decode})",
            ts_string(entity),
            ts_string(&field.name.text),
            self.reference_is_nullable(&field.field_type),
            ts_string(operation)
        )
    }
}
