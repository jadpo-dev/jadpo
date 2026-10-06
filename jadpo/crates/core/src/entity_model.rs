use jadpo_diagnostics::{Diagnostic, DiagnosticFact, SourceSpan};
use jadpo_syntax::{
    Block, CallableDeclaration, CallableKind, ConsistencyDisposition, Declaration,
    DerivedRepresentationKind, Expression, MutationGuard, ParsedSyntax, QueryFreshness, RecordKind,
    Statement, TextRange,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The v0.1 lifecycle contract keeps logical policy effects separate from SQL
/// verbs: the canonical `delete` transition has a logical delete effect even
/// though its authoritative write is an UPDATE. Other named transitions remain
/// ordinary update effects.
pub(crate) fn lifecycle_transition_policy_effect(transition: &str) -> &'static str {
    if transition == "delete" {
        "delete"
    } else {
        "update"
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EntityModel {
    pub active: bool,
    pub entities: Vec<EntityContract>,
    pub queries: Vec<QueryContract>,
    pub transactions: Vec<TransactionContract>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityContract {
    pub name: String,
    pub identity: String,
    pub persistent: bool,
    pub authority_store: Option<String>,
    pub representations: Vec<RepresentationContract>,
    pub operations: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepresentationContract {
    pub kind: &'static str,
    pub name: String,
    pub store: String,
    pub authority: String,
    pub strategy: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryContract {
    pub name: String,
    pub owner: Option<String>,
    pub freshness: &'static str,
    pub plan: &'static str,
    pub reads: Vec<String>,
    pub predicate_fields: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionContract {
    pub action: String,
    pub owners: Vec<String>,
    pub disposition: &'static str,
    pub domain: Option<String>,
    pub nested: &'static str,
    pub handled_failure: &'static str,
    pub postgres_isolation: &'static str,
    pub postgres_concurrency: &'static str,
    pub sqlite_isolation: &'static str,
    pub retry: &'static str,
}

#[derive(Clone, Debug, Default)]
struct Effects {
    mutations: BTreeSet<String>,
    reads: BTreeSet<String>,
    predicate_fields: BTreeSet<String>,
    calls: BTreeSet<String>,
}

pub fn analyze_entity_model(files: &[ParsedSyntax]) -> EntityModel {
    let active = files.iter().any(|file| {
        file.file.declarations.iter().any(|declaration| {
            matches!(declaration, Declaration::Record(record) if record.dossier.is_some())
        })
    });
    if !active {
        return EntityModel::default();
    }

    let mut model = EntityModel {
        active: true,
        ..EntityModel::default()
    };
    let mut authority_domains = BTreeMap::new();
    let mut persistent_entities = BTreeSet::new();
    let mut callables = BTreeMap::<String, (&CallableDeclaration, &str)>::new();
    let mut effects = BTreeMap::<String, Effects>::new();

    for file in files {
        validate_project_role(file, &mut model.diagnostics);
        for declaration in &file.file.declarations {
            match declaration {
                Declaration::Record(record) if record.kind == RecordKind::Entity => {
                    let Some(dossier) = &record.dossier else {
                        continue;
                    };
                    let persistent = dossier.persistence.is_some();
                    let authority_store = dossier
                        .persistence
                        .as_ref()
                        .map(|persistence| persistence.store.text.clone());
                    if let Some(store) = &authority_store {
                        persistent_entities.insert(record.name.text.clone());
                        authority_domains.insert(record.name.text.clone(), store.clone());
                    }
                    if !record
                        .fields
                        .iter()
                        .any(|field| field.name.text == dossier.identity.text)
                    {
                        push(
                            &mut model.diagnostics,
                            Diagnostic::error("DATA_IDENTITY_FIELD_UNKNOWN")
                                .with_fact(DiagnosticFact::Name(dossier.identity.text.clone())),
                            &file.source_name,
                            dossier.identity.range,
                        );
                    }
                    let mut representation_names = BTreeSet::new();
                    let mut representations = Vec::new();
                    for representation in &dossier.representations {
                        if !representation_names.insert(representation.name.text.clone()) {
                            push(
                                &mut model.diagnostics,
                                Diagnostic::error("DATA_REPRESENTATION_DUPLICATE").with_fact(
                                    DiagnosticFact::Name(representation.name.text.clone()),
                                ),
                                &file.source_name,
                                representation.name.range,
                            );
                        }
                        if authority_store.is_none()
                            || authority_store.as_deref() != Some(representation.from.text.as_str())
                            || authority_store.as_deref()
                                == Some(representation.store.text.as_str())
                        {
                            push(
                                &mut model.diagnostics,
                                Diagnostic::error("DATA_REPRESENTATION_AUTHORITY_INVALID")
                                    .with_fact(DiagnosticFact::Name(
                                        representation.name.text.clone(),
                                    )),
                                &file.source_name,
                                representation.range,
                            );
                        }
                        representations.push(RepresentationContract {
                            kind: match representation.kind {
                                DerivedRepresentationKind::Cache => "cache",
                                DerivedRepresentationKind::Projection => "projection",
                            },
                            name: representation.name.text.clone(),
                            store: representation.store.text.clone(),
                            authority: representation.from.text.clone(),
                            strategy: representation
                                .strategy
                                .as_ref()
                                .map(|strategy| strategy.text.clone()),
                        });
                    }
                    model.entities.push(EntityContract {
                        name: record.name.text.clone(),
                        identity: dossier.identity.text.clone(),
                        persistent,
                        authority_store,
                        representations,
                        operations: Vec::new(),
                    });
                }
                Declaration::Callable(callable) => {
                    callables.insert(
                        callable.name.text.clone(),
                        (callable, file.source_name.as_str()),
                    );
                    let mut callable_effects = Effects::default();
                    collect_block(&callable.body, &mut callable_effects);
                    effects.insert(callable.name.text.clone(), callable_effects);
                }
                _ => {}
            }
        }
    }

    let callable_names = callables.keys().cloned().collect::<BTreeSet<_>>();
    for callable_effects in effects.values_mut() {
        callable_effects.calls = callable_effects
            .calls
            .iter()
            .map(|call| resolve_call(call, &callable_names))
            .collect();
    }
    loop {
        let snapshot = effects.clone();
        let mut changed = false;
        for callable_effects in effects.values_mut() {
            for call in callable_effects.calls.clone() {
                let Some(callee) = snapshot.get(&call) else {
                    continue;
                };
                let before = (
                    callable_effects.mutations.len(),
                    callable_effects.reads.len(),
                    callable_effects.predicate_fields.len(),
                );
                callable_effects.mutations.extend(callee.mutations.clone());
                callable_effects.reads.extend(callee.reads.clone());
                callable_effects
                    .predicate_fields
                    .extend(callee.predicate_fields.clone());
                changed |= before
                    != (
                        callable_effects.mutations.len(),
                        callable_effects.reads.len(),
                        callable_effects.predicate_fields.len(),
                    );
            }
        }
        if !changed {
            break;
        }
    }

    for (name, (callable, source)) in &callables {
        let callable_effects = effects.get(name).cloned().unwrap_or_default();
        if let Some(owner) = &callable.owner {
            if let Some(entity) = model
                .entities
                .iter_mut()
                .find(|entity| entity.name == owner.text)
            {
                entity.operations.push(name.clone());
            }
        }
        let direct = direct_effects(&callable.body);
        for target in &direct.mutations {
            let owner_matches = callable
                .owner
                .as_ref()
                .is_some_and(|owner| owner.text == *target);
            if callable.kind == CallableKind::Action && !owner_matches {
                push(
                    &mut model.diagnostics,
                    Diagnostic::error("DATA_MUTATION_OWNER_REQUIRED")
                        .with_fact(DiagnosticFact::Callable(name.clone()))
                        .with_fact(DiagnosticFact::Name(target.clone())),
                    source,
                    callable.range,
                );
            }
            if !persistent_entities.contains(target) {
                push(
                    &mut model.diagnostics,
                    Diagnostic::error("DATA_ENTITY_NOT_PERSISTENT")
                        .with_fact(DiagnosticFact::Name(target.clone())),
                    source,
                    callable.range,
                );
            }
        }
        if callable.kind != CallableKind::Query && !direct.reads.is_empty() {
            push(
                &mut model.diagnostics,
                Diagnostic::error("DATA_RAW_QUERY_OUTSIDE_NAMED_QUERY")
                    .with_fact(DiagnosticFact::Callable(name.clone())),
                source,
                callable.name.range,
            );
        }
        if callable.kind == CallableKind::Query {
            if callable.freshness.is_none() {
                push(
                    &mut model.diagnostics,
                    Diagnostic::error("DATA_QUERY_FRESHNESS_REQUIRED")
                        .with_fact(DiagnosticFact::Callable(name.clone())),
                    source,
                    callable.name.range,
                );
            }
            model.queries.push(QueryContract {
                name: name.clone(),
                owner: callable.owner.as_ref().map(|owner| owner.text.clone()),
                freshness: freshness_name(callable.freshness),
                plan: "authoritative",
                reads: callable_effects.reads.iter().cloned().collect(),
                predicate_fields: callable_effects.predicate_fields.iter().cloned().collect(),
            });
        }
        if callable.kind == CallableKind::Action {
            let owners = callable_effects
                .mutations
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            if owners.len() > 1 && callable.consistency.is_none() {
                push(
                    &mut model.diagnostics,
                    Diagnostic::error("DATA_CONSISTENCY_REQUIRED")
                        .with_fact(DiagnosticFact::Callable(name.clone())),
                    source,
                    callable.name.range,
                );
            }
            let domains = owners
                .iter()
                .filter_map(|owner| authority_domains.get(owner).cloned())
                .collect::<BTreeSet<_>>();
            if callable.consistency == Some(ConsistencyDisposition::Atomic) && domains.len() > 1 {
                push(
                    &mut model.diagnostics,
                    Diagnostic::error("DATA_ATOMIC_DOMAIN_MISMATCH")
                        .with_fact(DiagnosticFact::Callable(name.clone())),
                    source,
                    callable.name.range,
                );
            }
            if callable.receiver == Some(jadpo_syntax::ReceiverKind::Value)
                && !direct.mutations.is_empty()
                && !matches!(
                    callable.mutation_guard,
                    Some(MutationGuard::Reload | MutationGuard::Revision)
                )
            {
                push(
                    &mut model.diagnostics,
                    Diagnostic::error("DATA_VALUE_RECEIVER_GUARD_REQUIRED")
                        .with_fact(DiagnosticFact::Callable(name.clone())),
                    source,
                    callable.name.range,
                );
            }
            if !owners.is_empty() {
                model.transactions.push(TransactionContract {
                    action: name.clone(),
                    owners,
                    disposition: match callable.consistency {
                        Some(ConsistencyDisposition::Atomic) => "atomic",
                        Some(ConsistencyDisposition::DurableWorkflow) => "durable_workflow",
                        None => "entity_action",
                    },
                    domain: (domains.len() == 1).then(|| domains.iter().next().unwrap().clone()),
                    nested: "join_declared_boundary",
                    handled_failure: "savepoint_rollback",
                    postgres_isolation: "read_committed",
                    postgres_concurrency: "row_lock_or_checked_revision",
                    sqlite_isolation: "immediate_serialized",
                    retry: "compiler_guarded_bounded_default",
                });
            }
        }
    }

    model
        .entities
        .sort_by(|left, right| left.name.cmp(&right.name));
    for entity in &mut model.entities {
        entity.operations.sort();
        entity
            .representations
            .sort_by(|left, right| left.name.cmp(&right.name));
    }
    model
        .queries
        .sort_by(|left, right| left.name.cmp(&right.name));
    model
        .transactions
        .sort_by(|left, right| left.action.cmp(&right.action));
    model
}

fn validate_project_role(file: &ParsedSyntax, diagnostics: &mut Vec<Diagnostic>) {
    let role = Path::new(&file.source_name)
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .find(|component| {
            matches!(
                *component,
                "entities" | "values" | "queries" | "workflows" | "routes" | "jobs"
            )
        });
    let Some(role) = role else { return };
    let aggregate_invalid = role == "entities"
        && file.file.declarations.iter().filter(|declaration| {
            matches!(declaration, Declaration::Record(record) if record.kind == RecordKind::Entity)
        }).count() != 1;
    let invalid_range = if aggregate_invalid {
        Some(file.file.range)
    } else {
        file.file.declarations.iter().find(|declaration| {
            let valid = match role {
                "entities" => match declaration {
                    Declaration::Record(record) => record.kind != RecordKind::Entity || record.dossier.is_some(),
                    Declaration::Callable(callable) => callable.owner.is_some(),
                    _ => true,
                },
                "values" => matches!(declaration, Declaration::Type(_) | Declaration::Enum(_) | Declaration::Record(_))
                    && !matches!(declaration, Declaration::Record(record) if record.kind == RecordKind::Entity),
                "queries" => match declaration {
                    Declaration::Callable(callable) => callable.kind == CallableKind::Query && callable.owner.is_none(),
                    Declaration::Type(_) | Declaration::Enum(_) => true,
                    Declaration::Record(record) => record.kind != RecordKind::Entity,
                    _ => false,
                },
                "workflows" => matches!(declaration, Declaration::Callable(callable) if callable.kind == CallableKind::Action && callable.owner.is_none()),
                "routes" => matches!(declaration, Declaration::Route(_)),
                "jobs" => true,
                _ => true,
            };
            !valid
        }).map(|declaration| declaration.range())
    };
    if let Some(range) = invalid_range {
        push(
            diagnostics,
            Diagnostic::error("DATA_PROJECT_ROLE_INVALID")
                .with_fact(DiagnosticFact::Usage(role.to_owned())),
            &file.source_name,
            range,
        );
    }
}

fn direct_effects(block: &Block) -> Effects {
    let mut effects = Effects::default();
    collect_block(block, &mut effects);
    effects
}

fn collect_block(block: &Block, effects: &mut Effects) {
    for statement in &block.statements {
        match statement {
            Statement::Binding(statement) => collect_expression(&statement.value, effects),
            Statement::Assignment(statement) => collect_expression(&statement.value, effects),
            Statement::Return(statement) => collect_expression(&statement.value, effects),
            Statement::Reject(statement) => {
                for field in &statement.values {
                    collect_expression(&field.value, effects);
                }
            }
            Statement::If(statement) => {
                collect_expression(&statement.condition, effects);
                collect_block(&statement.then_block, effects);
                if let Some(block) = &statement.else_block {
                    collect_block(block, effects);
                }
            }
            Statement::Match(statement) => {
                collect_expression(&statement.subject, effects);
                for arm in &statement.arms {
                    collect_block(&arm.body, effects);
                }
            }
            Statement::Assert(statement) => collect_expression(&statement.condition, effects),
            Statement::AdvanceClock(statement) => collect_expression(&statement.duration, effects),
            Statement::Unsupported(_) => {}
        }
    }
}

fn collect_expression(expression: &Expression, effects: &mut Effects) {
    match expression {
        Expression::Create(operation) => {
            effects.mutations.insert(joined(&operation.target.path));
            for field in &operation.fields {
                collect_expression(&field.value, effects);
            }
        }
        Expression::Query(operation) => {
            let target = joined(&operation.target.path);
            effects.reads.insert(target.clone());
            if let Some(page) = &operation.page {
                for predicate in &page.predicates {
                    effects
                        .predicate_fields
                        .insert(format!("{target}.{}", predicate.field.text));
                    collect_expression(&predicate.value, effects);
                }
                collect_expression(&page.after, effects);
                collect_expression(&page.limit, effects);
            } else {
                effects
                    .predicate_fields
                    .insert(format!("{target}.{}", operation.field.text));
                collect_expression(&operation.value, effects);
            }
        }
        Expression::Update(operation) => {
            effects.mutations.insert(joined(&operation.target.path));
            collect_expression(&operation.value, effects);
            for field in &operation.changes {
                collect_expression(&field.value, effects);
            }
        }
        Expression::Delete(operation) => {
            effects.mutations.insert(joined(&operation.target.path));
            collect_expression(&operation.value, effects);
        }
        Expression::Invocation(invocation) => {
            effects.calls.insert(joined(&invocation.callee.path));
            for argument in &invocation.arguments {
                collect_expression(argument, effects);
            }
            for argument in &invocation.named_arguments {
                collect_expression(&argument.value, effects);
            }
        }
        Expression::TestCall(call) => {
            effects.calls.insert(joined(&call.invocation.callee.path));
            for argument in &call.invocation.arguments {
                collect_expression(argument, effects);
            }
            for argument in &call.invocation.named_arguments {
                collect_expression(&argument.value, effects);
            }
        }
        Expression::Construction(construction) => {
            for field in &construction.fields {
                collect_expression(&field.value, effects);
            }
        }
        Expression::Object(object) => {
            for field in &object.fields {
                collect_expression(&field.value, effects);
            }
        }
        Expression::Attempt(attempt) => collect_expression(&attempt.value, effects),
        Expression::OutcomeMatch(outcome) => {
            collect_expression(&outcome.subject, effects);
            for arm in &outcome.arms {
                match &arm.body {
                    jadpo_syntax::OutcomeMatchArmBody::Value(value) => {
                        collect_expression(value, effects)
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Reject(rejection) => {
                        for field in &rejection.values {
                            collect_expression(&field.value, effects);
                        }
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Propagate(_) => {}
                }
            }
        }
        Expression::Unary(unary) => collect_expression(&unary.value, effects),
        Expression::Binary(binary) => {
            collect_expression(&binary.left, effects);
            collect_expression(&binary.right, effects);
        }
        Expression::Grouped(grouped) => collect_expression(&grouped.value, effects),
        Expression::Literal(_) | Expression::Name(_) | Expression::Missing(_) => {}
    }
}

fn resolve_call(call: &str, names: &BTreeSet<String>) -> String {
    if names.contains(call) {
        return call.to_owned();
    }
    let operation = call.rsplit('.').next().unwrap_or(call);
    let mut candidates = names
        .iter()
        .filter(|candidate| candidate.ends_with(&format!(".{operation}")));
    match (candidates.next(), candidates.next()) {
        (Some(candidate), None) => candidate.clone(),
        _ => call.to_owned(),
    }
}

fn freshness_name(freshness: Option<QueryFreshness>) -> &'static str {
    match freshness {
        Some(QueryFreshness::Authoritative) => "authoritative",
        Some(QueryFreshness::ReadYourWrites) => "read_your_writes",
        Some(QueryFreshness::BoundedStaleness) => "bounded_staleness",
        Some(QueryFreshness::Eventual) => "eventual",
        None => "missing",
    }
}

fn joined(path: &[jadpo_syntax::Name]) -> String {
    path.iter()
        .map(|part| part.text.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

fn push(
    diagnostics: &mut Vec<Diagnostic>,
    mut diagnostic: Diagnostic,
    source: &str,
    range: TextRange,
) {
    diagnostic.primary = Some(SourceSpan {
        source: source.to_owned(),
        start: range.start,
        end: range.end,
    });
    diagnostics.push(diagnostic);
}

#[cfg(test)]
mod tests {
    use jadpo_diagnostics::Diagnostic;

    #[test]
    fn entity_query_and_transaction_diagnostics_have_public_contracts() {
        for code in [
            "DATA_ATOMIC_DOMAIN_MISMATCH",
            "DATA_CONSISTENCY_INVALID",
            "DATA_CONSISTENCY_REQUIRED",
            "DATA_DELIVERY_MODE_INVALID",
            "DATA_ENTITY_NOT_PERSISTENT",
            "DATA_FRESHNESS_INVALID",
            "DATA_IDENTITY_FIELD_UNKNOWN",
            "DATA_MUTATION_GUARD_INVALID",
            "DATA_MUTATION_OWNER_REQUIRED",
            "DATA_PERSISTENCE_ROLE_INVALID",
            "DATA_PERSISTENCE_ROLE_REQUIRED",
            "DATA_PERSISTENCE_STORE_REQUIRED",
            "DATA_PROJECT_ROLE_INVALID",
            "DATA_QUERY_FRESHNESS_REQUIRED",
            "DATA_RAW_QUERY_OUTSIDE_NAMED_QUERY",
            "DATA_RECEIVER_KIND_INVALID",
            "DATA_REPRESENTATION_AUTHORITY_INVALID",
            "DATA_REPRESENTATION_DUPLICATE",
            "DATA_REPRESENTATION_SETTING_INVALID",
            "DATA_REPRESENTATION_SETTING_REQUIRED",
            "DATA_VALUE_RECEIVER_GUARD_REQUIRED",
            "EFFECT_QUERY_CALLS_ACTION",
            "EFFECT_QUERY_MUTATION",
            "JADPO_TARGET_DURABLE_WORKFLOW_NOT_IMPLEMENTED",
            "JADPO_TARGET_STORE_NOT_IMPLEMENTED",
        ] {
            let diagnostic = Diagnostic::error(code);
            assert!(!diagnostic.message.is_empty());
            assert!(!diagnostic.reason.is_empty());
            assert!(!diagnostic.recommended_next_step.title.is_empty());
        }
    }
}
