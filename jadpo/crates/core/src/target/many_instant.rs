//! Ordinary noinclude/nopage Many callers opt in; shared raw helpers stay legacy.
use super::*;
impl TargetGenerator<'_> {
    pub(super) fn many_instant_entities(&self) -> BTreeSet<String> {
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
                    && matches!(q.cardinality, jadpo_syntax::QueryCardinality::Many)
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
    pub(super) fn instant_storage_entities(&self) -> BTreeSet<String> {
        self.point_instant_entities()
            .union(&self.many_instant_entities())
            .cloned()
            .collect()
    }
}
