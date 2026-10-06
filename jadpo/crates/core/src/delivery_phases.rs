//! Selected identity composition prerequisite. These facts are not live grants.
//! The complete atomic delivery finish must own any checked phase table/export.
use crate::PolicyModel;
use jadpo_diagnostics::{Diagnostic, SourceSpan};
use jadpo_semantic::{DeliveryTypeCandidate, NodeKind, SemanticGraph};

mod model;
use jadpo_syntax::*;
use model::DeliveryAuthorityCandidate;
pub use model::DeliveryPhaseCandidate;
pub(crate) use model::{
    DeliveryAuthorityLowering, DeliverySelectionLowering, DeliveryTargetCandidate,
    DeliveryTargetKind,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn check_delivery_phases(
    files: &[ParsedSyntax],
    graph: &SemanticGraph,
    policy: &PolicyModel,
    type_candidates: &[DeliveryTypeCandidate],
) -> (Vec<Diagnostic>, Vec<DeliveryPhaseCandidate>) {
    let records = files
        .iter()
        .flat_map(|file| &file.file.declarations)
        .filter_map(|declaration| match declaration {
            Declaration::Record(record) => Some((record.name.text.as_str(), record)),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let context = Context {
        files,
        graph,
        policy,
        records,
    };
    let mut diagnostics = Vec::new();
    let mut candidates = Vec::new();
    let mut bound_entities = BTreeSet::new();
    let mut bound_contracts = BTreeSet::new();
    for file in files {
        for declaration in &file.file.declarations {
            let Declaration::Job(job) = declaration else {
                continue;
            };
            let Some(delivery) = &job.delivery else {
                continue;
            };
            let unique_entity = bound_entities.insert(delivery.selection.entity.text.clone());
            let unique_contract = bound_contracts.insert(joined(&delivery.service.operation.path));
            let authority = context.resolve(delivery);
            if authority.is_none() || !unique_entity || !unique_contract {
                let mut diagnostic = Diagnostic::error("TYPE_JOB_DELIVERY_AUTHORITY_INVALID");
                diagnostic.primary = Some(SourceSpan {
                    source: file.source_name.clone(),
                    start: delivery.authority.range.start,
                    end: delivery.authority.range.end,
                });
                diagnostics.push(diagnostic);
            } else if let (Some((authority, owner_active)), Some(types)) = (
                authority,
                type_candidates.iter().find(|candidate| {
                    candidate.schedule_candidate().job == job.name.text
                        && candidate.source() == file.source_name
                }),
            ) {
                if let Some(candidate) =
                    DeliveryPhaseCandidate::resolve(types, authority, owner_active, graph)
                {
                    candidates.push(candidate);
                } else {
                    let mut diagnostic = Diagnostic::error("TYPE_JOB_DELIVERY_AUTHORITY_INVALID");
                    diagnostic.primary = Some(SourceSpan {
                        source: file.source_name.clone(),
                        start: delivery.range.start,
                        end: delivery.range.end,
                    });
                    diagnostics.push(diagnostic);
                }
            }
        }
    }
    // A competing descriptor invalidates this entire prerequisite set as well.
    if !diagnostics.is_empty() {
        candidates.clear();
    }
    (diagnostics, candidates)
}

fn joined(path: &[Name]) -> String {
    path.iter()
        .map(|name| name.text.as_str())
        .collect::<Vec<_>>()
        .join(".")
}
struct Context<'a> {
    files: &'a [ParsedSyntax],
    graph: &'a SemanticGraph,
    policy: &'a PolicyModel,
    records: BTreeMap<&'a str, &'a RecordDeclaration>,
}
impl Context<'_> {
    fn ancestors(&self, name: &str) -> Vec<String> {
        let mut current = self.graph.node(name).map(|node| node.id);
        let mut visited = BTreeSet::new();
        let mut result = Vec::new();
        while let Some(id) = current {
            if !visited.insert(id) {
                return Vec::new();
            }
            let Some(node) = self.graph.nodes.iter().find(|node| node.id == id) else {
                return Vec::new();
            };
            result.push(node.name.clone());
            current = self
                .graph
                .refinements
                .iter()
                .find(|edge| edge.refined == id)
                .map(|edge| edge.parent);
        }
        result
    }
    fn field<'a>(&self, record: &'a RecordDeclaration, name: &str) -> Option<&'a FieldDeclaration> {
        record.fields.iter().find(|field| field.name.text == name)
    }
    fn required(
        &self,
        record: &RecordDeclaration,
        field: &FieldDeclaration,
        nullable: bool,
        root: &str,
    ) -> bool {
        !field.optional
            && field.field_type.arguments.is_empty()
            && self
                .graph
                .nullable_types
                .contains(&format!("{}.{}", record.name.text, field.name.text))
                == nullable
            && self
                .ancestors(&format!("{}.{}", record.name.text, field.name.text))
                .last()
                .is_some_and(|name| name == root)
    }
    fn reference(
        &self,
        record: &RecordDeclaration,
        field: &FieldDeclaration,
        identity: &str,
    ) -> bool {
        // Actual owning reference metadata takes precedence over primitive Uuid
        // ancestry; another Uuid field is not the selected principal identity.
        self.required(record, field, false, "Uuid")
            && field.reference.as_ref().is_some_and(|reference| {
                joined(&reference.target.path) == identity
                    && !reference.target.nullable
                    && reference.target.arguments.is_empty()
            })
            && self
                .ancestors(&format!("{}.{}", record.name.text, field.name.text))
                .contains(&identity.to_owned())
    }
    fn store<'a>(&self, record: &'a RecordDeclaration) -> Option<&'a str> {
        (record.kind == RecordKind::Entity).then_some(
            record
                .dossier
                .as_ref()?
                .persistence
                .as_ref()?
                .store
                .text
                .as_str(),
        )
    }
    fn active(&self, record: &RecordDeclaration, expression: &Expression) -> bool {
        let Expression::Binary(binary) = expression else {
            return false;
        };
        if binary.operator != BinaryOperator::Equal {
            return false;
        }
        let (Expression::Name(field), Expression::Name(variant)) =
            (binary.left.as_ref(), binary.right.as_ref())
        else {
            return false;
        };
        let [field_name] = field.path.as_slice() else {
            return false;
        };
        let [enum_name, variant_name] = variant.path.as_slice() else {
            return false;
        };
        let Some(field) = self.field(record, &field_name.text) else {
            return false;
        };
        !field.optional && !self.graph.nullable_types.contains(&format!("{}.{}", record.name.text, field.name.text))
            && field.field_type.arguments.is_empty() && variant_name.text == "active"
            && self.ancestors(&format!("{}.{}", record.name.text, field.name.text)).contains(&enum_name.text)
            && self.graph.node(&joined(&variant.path)).map(|node| node.kind) == Some(NodeKind::EnumVariant)
            && self.files.iter().flat_map(|file| &file.file.declarations).any(|declaration| matches!(declaration, Declaration::Enum(enumeration) if enumeration.name.text == enum_name.text && enumeration.variants.iter().any(|variant| variant.name.text == variant_name.text && variant.fields.is_empty())))
    }
    fn resolve(
        &self,
        delivery: &JobReminderDelivery,
    ) -> Option<(DeliveryAuthorityCandidate, (String, String))> {
        let authority = &delivery.authority;
        let [strategy_name, validator_name] = authority.validator.path.as_slice() else {
            return None;
        };
        let Some(strategy) = self
            .files
            .iter()
            .flat_map(|file| &file.file.declarations)
            .find_map(|declaration| match declaration {
                Declaration::AuthenticationStrategy(strategy)
                    if strategy.name.text == strategy_name.text =>
                {
                    Some(strategy)
                }
                _ => None,
            })
        else {
            return None;
        };
        let Some(validator) = strategy
            .validators
            .iter()
            .find(|validator| validator.name.text == validator_name.text)
        else {
            return None;
        };
        let validator_identity = format!(
            "authentication.{}.validator.{}.api_key",
            strategy_name.text, validator_name.text
        );
        if validator.mode.text != "api_key"
            || validator.principal.text != "service"
            || self.graph.node(&validator_identity).map(|node| node.kind)
                != Some(NodeKind::CredentialValidation)
        {
            return None;
        }
        let Some(binding) = &validator.credentials else {
            return None;
        };
        let [credential_entity, credential_identity] = binding.identity.path.as_slice() else {
            return None;
        };
        let Some(credential) = self.records.get(credential_entity.text.as_str()) else {
            return None;
        };
        if credential
            .dossier
            .as_ref()
            .map(|dossier| &dossier.identity.text)
            != Some(&credential_identity.text)
        {
            return None;
        }
        let Some(credential_id) = self.field(credential, &credential_identity.text) else {
            return None;
        };
        if !self.required(credential, credential_id, false, "Uuid") {
            return None;
        }
        let Some(service) = self.records.get("Service") else {
            return None;
        };
        let Some(service_dossier) = &service.dossier else {
            return None;
        };
        let service_identity = format!("{}.{}", service.name.text, service_dossier.identity.text);
        let Some(service_id) = self.field(service, &service_dossier.identity.text) else {
            return None;
        };
        if !self.required(service, service_id, false, "Uuid") {
            return None;
        }
        let [principal_entity, principal_field] = binding.principal.path.as_slice() else {
            return None;
        };
        if principal_entity.text != credential.name.text {
            return None;
        }
        let Some(credential_principal) = self.field(credential, &principal_field.text) else {
            return None;
        };
        if !self.reference(credential, credential_principal, &service_identity)
            || !self.active(credential, &binding.active)
        {
            return None;
        }
        for (path, nullable, root) in [
            (&binding.verifier, false, "Text"),
            (&binding.expires, false, "Instant"),
            (&binding.revoked, true, "Instant"),
        ] {
            let [entity, field_name] = path.path.as_slice() else {
                return None;
            };
            let Some(field) = self.field(credential, &field_name.text) else {
                return None;
            };
            if entity.text != credential.name.text
                || !self.required(credential, field, nullable, root)
            {
                return None;
            }
        }
        let Some(resolution) = strategy
            .resolutions
            .iter()
            .find(|resolution| resolution.principal.text == "service")
        else {
            return None;
        };
        if resolution.authority.path.len() != 2
            || resolution.authority.path[0].text != service.name.text
            || !self.active(service, &resolution.active)
        {
            return None;
        }
        let resolution_identity =
            format!("authentication.{}.resolution.service", strategy_name.text);
        let (Some(resolution_node), Some(authority_node)) = (
            self.graph.node(&resolution_identity),
            self.graph.node(&joined(&resolution.authority.path)),
        ) else {
            return None;
        };
        if !self.graph.authentication_resolutions.iter().any(|edge| {
            edge.resolution == resolution_node.id && edge.authority == authority_node.id
        }) {
            return None;
        }
        let Some(principal) = self
            .files
            .iter()
            .flat_map(|file| &file.file.declarations)
            .find_map(|declaration| match declaration {
                Declaration::Principal(principal) => Some(principal),
                _ => None,
            })
        else {
            return None;
        };
        let Some(application) = self
            .files
            .iter()
            .flat_map(|file| &file.file.declarations)
            .find_map(|declaration| match declaration {
                Declaration::Application(application) => Some(application),
                _ => None,
            })
        else {
            return None;
        };
        if joined(&application.authentication.principal.path) != principal.name.text {
            return None;
        }
        let Some(variant) = principal
            .variants
            .iter()
            .find(|variant| variant.kind == PrincipalVariantKind::Service)
        else {
            return None;
        };
        let mapping = resolution
            .mappings
            .iter()
            .filter(|mapping| mapping.source.text == service_dossier.identity.text)
            .collect::<Vec<_>>();
        let [mapping] = mapping.as_slice() else {
            return None;
        };
        let [target_principal, target_variant, target_field] = mapping.target.path.as_slice()
        else {
            return None;
        };
        if target_principal.text != principal.name.text || target_variant.text != variant.name.text
        {
            return None;
        }
        let Some(field) = variant
            .fields
            .iter()
            .find(|field| field.name.text == target_field.text)
        else {
            return None;
        };
        if field.optional
            || field.field_type.nullable
            || !field.field_type.arguments.is_empty()
            || self
                .graph
                .nullable_types
                .contains(&joined(&mapping.target.path))
            || self
                .ancestors(&joined(&mapping.target.path))
                .last()
                .map(String::as_str)
                != Some("Uuid")
        {
            return None;
        }
        let Some(membership) = self.records.get(authority.membership.text.as_str()) else {
            return None;
        };
        let Some(membership_identity) = membership
            .dossier
            .as_ref()
            .and_then(|dossier| self.field(membership, &dossier.identity.text))
        else {
            return None;
        };
        if !self.required(membership, membership_identity, false, "Uuid") {
            return None;
        }
        let Some(selected) = &membership.membership else {
            return None;
        };
        let [role_type, role_variant] = authority.role.path.as_slice() else {
            return None;
        };
        let Some(member) = self.field(membership, &selected.member.text) else {
            return None;
        };
        let Some(role) = self.field(membership, &selected.role.text) else {
            return None;
        };
        if selected.scope.text != "application" || !self.reference(membership, member, &service_identity)
            || role.optional || self.graph.nullable_types.contains(&format!("{}.{}", membership.name.text, role.name.text))
            || joined(&role.field_type.path) != role_type.text || !role.field_type.arguments.is_empty()
            || self.graph.node(&joined(&authority.role.path)).map(|node| node.kind) != Some(NodeKind::EnumVariant)
            || !self.files.iter().flat_map(|file| &file.file.declarations).any(|declaration| matches!(declaration, Declaration::Enum(enumeration) if enumeration.name.text == role_type.text && enumeration.variants.iter().any(|variant| variant.name.text == role_variant.text && variant.fields.is_empty()))) { return None; }
        let memberships = self
            .policy
            .memberships
            .iter()
            .filter(|candidate| candidate.entity == membership.name.text)
            .collect::<Vec<_>>();
        let [selected_policy] = memberships.as_slice() else {
            return None;
        };
        if selected_policy.scope != "application"
            || selected_policy.scope_field.is_some()
            || selected_policy.principal != service.name.text
            || selected_policy.member_field != member.name.text
            || selected_policy.role_field != role.name.text
            || selected_policy.role_type != role_type.text
        {
            return None;
        }
        let Some(store) = self.store(service) else {
            return None;
        };
        if ![credential, membership]
            .iter()
            .all(|record| self.store(record) == Some(store))
            || ![
                &delivery.selection.entity.text,
                &delivery.selection.owner_visible.text,
            ]
            .iter()
            .all(|name| {
                self.records
                    .get(name.as_str())
                    .is_some_and(|record| self.store(record) == Some(store))
            })
        {
            return None;
        }
        let owner = self
            .records
            .get(delivery.selection.owner_visible.text.as_str())?;
        let owner_visible = owner
            .dossier
            .as_ref()?
            .lifecycle
            .as_ref()?
            .visible
            .as_ref()?;
        if !self.active(owner, owner_visible) {
            return None;
        }
        Some((
            DeliveryAuthorityCandidate::new(
                application.name.text.clone(),
                strategy_name.text.clone(),
                validator_identity,
                joined(&binding.identity.path),
                joined(&binding.principal.path),
                joined(&binding.verifier.path),
                active_fact(credential, &binding.active)?,
                joined(&binding.expires.path),
                joined(&binding.revoked.path),
                service_identity,
                resolution_identity,
                joined(&resolution.authority.path),
                active_fact(service, &resolution.active)?,
                joined(&mapping.target.path),
                membership.name.text.clone(),
                format!("{}.{}", membership.name.text, membership_identity.name.text),
                format!("{}.{}", membership.name.text, member.name.text),
                format!("{}.{}", membership.name.text, role.name.text),
                joined(&authority.role.path),
                store.to_owned(),
            ),
            active_fact(owner, owner_visible)?,
        ))
    }
}

fn active_fact(record: &RecordDeclaration, expression: &Expression) -> Option<(String, String)> {
    let Expression::Binary(binary) = expression else {
        return None;
    };
    let (Expression::Name(field), Expression::Name(variant)) =
        (binary.left.as_ref(), binary.right.as_ref())
    else {
        return None;
    };
    let [field] = field.path.as_slice() else {
        return None;
    };
    Some((
        format!("{}.{}", record.name.text, field.text),
        joined(&variant.path),
    ))
}
