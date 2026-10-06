//! Nonexecuting native relationship-failure facts. These use the same normalized
//! persistence fields as target DDL and the checker's resolved call graph. They
//! describe a possible boundary outcome, never permission or runtime execution.
use crate::AnalyzedProject;
use jadpo_syntax::{
    Block, Declaration, Expression, FieldDeclaration, FieldInitialiser, OutcomeMatchArmBody,
    RecordDeclaration, ReferenceDeleteAction, Statement, TextRange,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const RELATIONSHIP_FAILURE: &str = r#"{"code":"relationship_conflict","http_status":409,"message":"The operation conflicts with a relationship.","condition":"checked_reachable_native_foreign_key_constraint","automatic_retry":false}"#;

/// Exact lexical occurrence mapping shared with target patch lowering. A
/// same-named parameter or nominal prefix is not a checked patch record.
pub(crate) fn checked_patch_fields<'a>(
    project: &'a AnalyzedProject,
    source: &str,
    range: TextRange,
) -> Option<Vec<&'a FieldDeclaration>> {
    let names = project
        .typing
        .expressions
        .iter()
        .filter(|expression| expression.source == source && expression.range == range)
        .map(|expression| expression.type_name.as_str())
        .collect::<BTreeSet<_>>();
    if names.len() != 1 {
        return None;
    }
    let name = *names.first()?;
    let records = project
        .syntax
        .sources
        .iter()
        .flat_map(|source| &source.file.declarations)
        .filter_map(|declaration| match declaration {
            Declaration::Record(record) if record.name.text == name => Some(record),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [record] = records.as_slice() else {
        return None;
    };
    Some(record.fields.iter().collect())
}

#[derive(Clone, Default)]
pub(crate) struct RelationshipFacts {
    pub may_fail: bool,
    pub unresolved: BTreeSet<String>,
}

impl RelationshipFacts {
    fn merge(&mut self, other: &Self) {
        self.may_fail |= other.may_fail;
        self.unresolved.extend(other.unresolved.iter().cloned());
    }
    pub fn json(&self) -> String {
        serde_json::json!({
            "status": if !self.unresolved.is_empty() { "unresolved_checked_mapping" }
                else if self.may_fail { "checked_static_may_failure" } else { "checked_not_reachable" },
            "qualified": self.unresolved.is_empty(),
            "may_fail": if self.unresolved.is_empty() { Some(self.may_fail) } else { None },
            "conservative_possible": self.may_fail,
            "evidence": "normalized_persistence_references_and_checked_calls",
            "scope": "static_may_outcome_not_execution_or_authority",
            "unresolved": self.unresolved,
        }).to_string()
    }
}

pub(crate) struct RelationshipAnalysis {
    routes: BTreeMap<String, RelationshipFacts>,
}

impl RelationshipAnalysis {
    pub fn new(project: &AnalyzedProject) -> Self {
        let records = project
            .syntax
            .sources
            .iter()
            .flat_map(|source| &source.file.declarations)
            .filter_map(|declaration| match declaration {
                Declaration::Record(record) if record.is_persistent_entity() => {
                    Some((record.name.text.as_str(), record))
                }
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let schema = Schema { records };
        let mut direct = BTreeMap::new();
        for source in &project.syntax.sources {
            for declaration in &source.file.declarations {
                let (name, block) = match declaration {
                    Declaration::Callable(callable) => {
                        (callable.name.text.clone(), Some(&callable.body))
                    }
                    Declaration::Route(route) => (
                        format!("{} {}", super::method_name(route.method), route.path),
                        route.inline_action.as_ref().map(|action| &action.body),
                    ),
                    _ => continue,
                };
                let mut collector = Collector {
                    project,
                    schema: &schema,
                    source: &source.source_name,
                    facts: RelationshipFacts::default(),
                };
                if let Some(block) = block {
                    collector.block(block);
                }
                if let Declaration::Route(route) = declaration {
                    if let Some(run) = &route.run {
                        collector.expression(&Expression::Invocation(run.clone()));
                    }
                }
                direct.insert(name, collector.facts);
            }
        }
        // Fixed-point union over checked NodeId edges, rather than a second
        // receiver/suffix resolver. Recursive call cycles settle monotonically.
        loop {
            let previous = direct.clone();
            let mut changed = false;
            for edge in &project.semantics.calls {
                let caller = &project.semantics.nodes[edge.caller.0 as usize].name;
                let callee = &project.semantics.nodes[edge.callee.0 as usize].name;
                if let (Some(caller), Some(callee)) = (direct.get_mut(caller), previous.get(callee))
                {
                    let before = (caller.may_fail, caller.unresolved.len());
                    caller.merge(callee);
                    changed |= before != (caller.may_fail, caller.unresolved.len());
                }
            }
            if !changed {
                break;
            }
        }
        Self { routes: direct }
    }
    pub fn route(&self, name: &str) -> RelationshipFacts {
        self.routes.get(name).cloned().unwrap_or_default()
    }
}

struct Schema<'a> {
    records: BTreeMap<&'a str, &'a RecordDeclaration>,
}

impl Schema<'_> {
    fn written_field(&self, entity: &str, field: &str) -> bool {
        self.records.get(entity).is_some_and(|record| {
            record
                .fields
                .iter()
                .any(|candidate| candidate.name.text == field && candidate.reference.is_some())
        }) || self.records.values().any(|record| {
            record.fields.iter().any(|candidate| {
                candidate.reference.as_ref().is_some_and(|reference| {
                    reference
                        .target
                        .path
                        .first()
                        .is_some_and(|name| name.text == entity)
                        && reference
                            .target
                            .path
                            .get(1)
                            .is_some_and(|name| name.text == field)
                })
            })
        })
    }
    fn any_written_field(&self, entity: &str) -> bool {
        self.records.get(entity).is_some_and(|record| {
            record
                .fields
                .iter()
                .any(|field| self.written_field(entity, &field.name.text))
        })
    }
    fn physical_delete(&self, entity: &str, seen: &mut BTreeSet<String>) -> bool {
        if !seen.insert(entity.to_owned()) {
            return false;
        }
        self.records.iter().any(|(child, record)| {
            record.fields.iter().any(|field| {
                let Some(reference) = &field.reference else {
                    return false;
                };
                if !reference
                    .target
                    .path
                    .first()
                    .is_some_and(|target| target.text == entity)
                {
                    return false;
                }
                match reference.on_delete {
                    ReferenceDeleteAction::Restrict => true,
                    ReferenceDeleteAction::Cascade => self.physical_delete(child, seen),
                    // Checked SET NULL requires a nullable source column. It does
                    // not itself create the FK orphan that this outcome describes.
                    ReferenceDeleteAction::SetNull => false,
                }
            })
        })
    }
}

struct Collector<'a, 's> {
    project: &'a AnalyzedProject,
    schema: &'s Schema<'a>,
    source: &'a str,
    facts: RelationshipFacts,
}

impl Collector<'_, '_> {
    fn fields(&mut self, fields: &[FieldInitialiser]) {
        for field in fields {
            self.expression(&field.value);
        }
    }
    fn written(&mut self, entity: &str, fields: impl IntoIterator<Item = String>) {
        self.facts.may_fail |= fields
            .into_iter()
            .any(|field| self.schema.written_field(entity, &field));
    }
    fn block(&mut self, block: &Block) {
        for statement in &block.statements {
            match statement {
                Statement::Binding(value) => self.expression(&value.value),
                Statement::Assignment(value) => self.expression(&value.value),
                Statement::Return(value) => self.expression(&value.value),
                Statement::Reject(value) => self.fields(&value.values),
                Statement::If(value) => {
                    self.expression(&value.condition);
                    self.block(&value.then_block);
                    if let Some(block) = &value.else_block {
                        self.block(block);
                    }
                }
                Statement::Match(value) => {
                    self.expression(&value.subject);
                    for arm in &value.arms {
                        self.block(&arm.body);
                    }
                }
                Statement::Assert(value) => self.expression(&value.condition),
                Statement::AdvanceClock(value) => self.expression(&value.duration),
                Statement::Unsupported(_) => {}
            }
        }
    }
    fn expression(&mut self, expression: &Expression) {
        match expression {
            Expression::Create(value) => {
                let entity = path(&value.target.path);
                // CREATE writes outgoing FK values; incoming keys only affect
                // UPDATE/DELETE, so unrelated parent creation remains precise.
                self.facts.may_fail |=
                    self.schema
                        .records
                        .get(entity.as_str())
                        .is_some_and(|record| {
                            record.fields.iter().any(|field| field.reference.is_some())
                        });
                self.fields(&value.fields);
                for conflict in &value.conflicts {
                    self.fields(&conflict.rejection.values);
                }
            }
            Expression::Update(value) => {
                let entity = path(&value.target.path);
                let mut fields = value
                    .changes
                    .iter()
                    .map(|field| field.name.text.clone())
                    .chain(
                        value
                            .conditional_changes
                            .iter()
                            .map(|field| field.change.name.text.clone()),
                    )
                    .collect::<BTreeSet<_>>();
                if let Some(transition) = &value.transition {
                    if let Some(transition) = self
                        .schema
                        .records
                        .get(entity.as_str())
                        .and_then(|record| record.dossier.as_ref())
                        .and_then(|dossier| dossier.lifecycle.as_ref())
                        .and_then(|lifecycle| {
                            lifecycle
                                .transitions
                                .iter()
                                .find(|entry| entry.name.text == transition.text)
                        })
                    {
                        fields.extend(transition.set.iter().map(|field| field.name.text.clone()));
                    }
                }
                if let Some(patch) = &value.patch {
                    if let Some(patch_fields) =
                        checked_patch_fields(self.project, self.source, patch.range)
                    {
                        fields.extend(patch_fields.iter().map(|field| field.name.text.clone()));
                    } else if self.schema.any_written_field(&entity) {
                        self.facts.unresolved.insert(format!(
                            "checked_patch_record:{}:{}",
                            patch.range.start, patch.range.end
                        ));
                        self.facts.may_fail = true;
                    }
                }
                // The target automatically touches a generated change field
                // after a real UPDATE. It may itself be a referenced parent key.
                if let Some(record) = self.schema.records.get(entity.as_str()) {
                    fields.extend(
                        record
                            .fields
                            .iter()
                            .filter(|field| {
                                field.generated
                                    == Some(jadpo_syntax::GeneratedFieldRole::CreateOrChange)
                            })
                            .map(|field| field.name.text.clone()),
                    );
                }
                self.written(&entity, fields);
                self.expression(&value.value);
                self.fields(&value.changes);
                for field in &value.conditional_changes {
                    self.expression(&field.change.value);
                }
                for binding in value
                    .empty
                    .iter()
                    .chain(std::iter::once(&value.missing))
                    .chain(value.conflicts.iter().map(|conflict| &conflict.rejection))
                {
                    self.fields(&binding.values);
                }
            }
            Expression::Delete(value) => {
                let entity = path(&value.target.path);
                self.facts.may_fail |= self.schema.physical_delete(&entity, &mut BTreeSet::new());
                self.expression(&value.value);
                self.fields(&value.missing.values);
                for conflict in &value.conflicts {
                    self.fields(&conflict.rejection.values);
                }
            }
            Expression::Query(value) => {
                self.expression(&value.value);
                if let Some(page) = &value.page {
                    for predicate in &page.predicates {
                        self.expression(&predicate.value);
                    }
                    self.expression(&page.after);
                    self.expression(&page.limit);
                }
                if let Some(pagination) = &value.pagination {
                    self.expression(&pagination.limit);
                    self.expression(&pagination.offset);
                }
                for include in &value.includes {
                    self.expression(&include.pagination.limit);
                    self.expression(&include.pagination.offset);
                }
                if let Some(missing) = &value.missing {
                    self.fields(&missing.values);
                }
            }
            Expression::Invocation(value) => {
                for argument in &value.arguments {
                    self.expression(argument);
                }
                self.fields(&value.named_arguments);
            }
            Expression::TestCall(value) => {
                for argument in &value.invocation.arguments {
                    self.expression(argument);
                }
                self.fields(&value.invocation.named_arguments);
            }
            Expression::Construction(value) => self.fields(&value.fields),
            Expression::Object(value) => self.fields(&value.fields),
            Expression::Attempt(value) => self.expression(&value.value),
            Expression::OutcomeMatch(value) => {
                self.expression(&value.subject);
                for arm in &value.arms {
                    match &arm.body {
                        OutcomeMatchArmBody::Value(value) => self.expression(value),
                        OutcomeMatchArmBody::Reject(value) => self.fields(&value.values),
                        OutcomeMatchArmBody::Propagate(_) => {}
                    }
                }
            }
            Expression::Unary(value) => self.expression(&value.value),
            Expression::Binary(value) => {
                self.expression(&value.left);
                self.expression(&value.right);
            }
            Expression::Grouped(value) => self.expression(&value.value),
            Expression::Literal(_) | Expression::Name(_) | Expression::Missing(_) => {}
        }
    }
}

fn path(names: &[jadpo_syntax::Name]) -> String {
    names
        .iter()
        .map(|name| name.text.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unresolved_checked_occurrence_is_unqualified_at_the_metadata_boundary() {
        let source = r#"
entity Parent { id: Uuid identity }
entity Child { id: Uuid identity parent_id: Parent.id references Parent.id on_delete restrict }
input PatchChild { parent_id: Child.parent_id optional }
input Request { id: Child.id patch: PatchChild }
failure EmptyPatch {kind:InvalidValue code:"empty"}
failure MissingChild {kind:NotFound code:"missing"}
failure ConflictChild {kind:Conflict code:"conflict"}
action edit(id:Child.id,input:PatchChild) ->Child fails EmptyPatch,MissingChild,ConflictChild {
 return attempt update required Child {where:id==id patch:input empty:EmptyPatch missing:MissingChild conflict:ConflictChild}
}
route POST /edit {auth:none input:Request output:Child run:edit(input.id,input.patch)}
"#;
        let mut project = crate::analyze_sources(vec![jadpo_syntax::SourceFile::new(
            "unknown-map.jadpo".into(),
            source.into(),
        )])
        .unwrap();
        assert!(project
            .syntax
            .diagnostics()
            .chain(project.semantics.diagnostics.iter())
            .chain(project.typing.diagnostics.iter())
            .chain(project.failures.diagnostics.iter())
            .next()
            .is_none());
        // A missing checker observation must not become a precise absence.
        // This probes the metadata boundary only: corrupted typed projects do
        // not satisfy the target's separate checked lowering invariant.
        project.typing.expressions.clear();
        let routes: serde_json::Value =
            serde_json::from_str(&super::super::ArtifactModel::new(&project).routes_json())
                .unwrap();
        let fact = &routes["routes"][0]["relationship_failure_analysis"];
        assert_eq!(fact["qualified"], false);
        assert_eq!(fact["status"], "unresolved_checked_mapping");
        assert!(fact["may_fail"].is_null());
        assert_eq!(fact["conservative_possible"], true);
        assert!(!fact["unresolved"].as_array().unwrap().is_empty());
        let source = source.replace(" references Parent.id on_delete restrict", "");
        let mut unrelated = crate::analyze_sources(vec![jadpo_syntax::SourceFile::new(
            "unknown-unrelated.jadpo".into(),
            source,
        )])
        .unwrap();
        unrelated.typing.expressions.clear();
        let routes: serde_json::Value =
            serde_json::from_str(&super::super::ArtifactModel::new(&unrelated).routes_json())
                .unwrap();
        let fact = &routes["routes"][0]["relationship_failure_analysis"];
        assert_eq!(fact["qualified"], true);
        assert_eq!(fact["may_fail"], false);
        assert_eq!(fact["status"], "checked_not_reachable");
    }
}
