use jadpo_diagnostics::{Diagnostic, DiagnosticFact, SourceSpan};
use jadpo_syntax::{
    Block, CallableKind, ConsistencyDisposition, Constraint, ConstraintKind, Declaration,
    Expression, HttpMethod, Name, ParsedSyntax, PersistenceModifier, RecordKind,
    ReferenceDeleteAction, ServiceDeclaration, Statement, TextRange, TypeReference,
};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

mod failurecheck;
mod typecheck;

pub use failurecheck::{
    check_failures, CallableFailureSet, FailureCheckResult, FailureContract, JobFailureSet,
    RouteFailure,
};
pub use typecheck::{
    check_types, CheckedJobBinding, ClockRead, DeliveryTypeCandidate, InferredExpression,
    TypeCheckResult,
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NodeId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NodeKind {
    PreludeType,
    StandardNamespace,
    StandardFunction,
    StandardValue,
    StandardFailure,
    Application,
    Locales,
    AuthenticationStrategy,
    CredentialSlot,
    CredentialValidation,
    CredentialClaimMapping,
    PrincipalResolution,
    PrincipalMapping,
    Principal,
    PrincipalVariant,
    Configuration,
    ConfigurationField,
    Type,
    Enum,
    EnumVariant,
    Entity,
    EntityReference,
    EntityLifecycle,
    LifecycleInitial,
    LifecycleVisibility,
    LifecycleTransition,
    LifecyclePurge,
    Policy,
    PolicyOperation,
    PolicyRule,
    Value,
    Input,
    Output,
    Field,
    GeneratedField,
    Relationship,
    PersistenceConstraint,
    Failure,
    Function,
    Action,
    Query,
    Fixture,
    Test,
    Route,
    Job,
    ServiceOperation,
    // Core inserts these only after ordinary resolution and atomic delivery
    // finish. They are not authored callables, types or generic policy grants.
    DeliverySelection,
    DeliveryCompletion,
    DeliveryAcceptanceProof,
    DeliveryReconciliation,
}

impl NodeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PreludeType => "prelude_type",
            Self::StandardNamespace => "standard_namespace",
            Self::StandardFunction => "standard_function",
            Self::StandardValue => "standard_value",
            Self::StandardFailure => "standard_failure",
            Self::Application => "application",
            Self::Locales => "locales",
            Self::AuthenticationStrategy => "authentication_strategy",
            Self::CredentialSlot => "credential_slot",
            Self::CredentialValidation => "credential_validation",
            Self::CredentialClaimMapping => "credential_claim_mapping",
            Self::PrincipalResolution => "principal_resolution",
            Self::PrincipalMapping => "principal_mapping",
            Self::Principal => "principal",
            Self::PrincipalVariant => "principal_variant",
            Self::Configuration => "configuration",
            Self::ConfigurationField => "configuration_field",
            Self::Type => "type",
            Self::Enum => "enum",
            Self::EnumVariant => "enum_variant",
            Self::Entity => "entity",
            Self::EntityReference => "entity_reference",
            Self::EntityLifecycle => "entity_lifecycle",
            Self::LifecycleInitial => "lifecycle_initial",
            Self::LifecycleVisibility => "lifecycle_visibility",
            Self::LifecycleTransition => "lifecycle_transition",
            Self::LifecyclePurge => "lifecycle_purge",
            Self::Policy => "policy",
            Self::PolicyOperation => "policy_operation",
            Self::PolicyRule => "policy_rule",
            Self::Value => "object",
            Self::Input => "input",
            Self::Output => "output",
            Self::Field => "field",
            Self::GeneratedField => "generated_field",
            Self::Relationship => "relationship",
            Self::PersistenceConstraint => "persistence_constraint",
            Self::Failure => "failure",
            Self::Function => "function",
            Self::Action => "action",
            Self::Query => "query",
            Self::Fixture => "fixture",
            Self::Test => "test",
            Self::Route => "route",
            Self::Job => "job",
            Self::ServiceOperation => "service_operation",
            Self::DeliverySelection => "delivery_selection",
            Self::DeliveryCompletion => "delivery_completion",
            Self::DeliveryAcceptanceProof => "delivery_acceptance_proof",
            Self::DeliveryReconciliation => "delivery_reconciliation",
        }
    }

    const fn is_type(self) -> bool {
        matches!(
            self,
            Self::PreludeType
                | Self::Type
                | Self::Enum
                | Self::Principal
                | Self::PrincipalVariant
                | Self::Entity
                | Self::EntityReference
                | Self::Value
                | Self::Input
                | Self::Output
                | Self::Field
        )
    }

    const fn user_name(self) -> &'static str {
        match self {
            Self::PreludeType | Self::Type => "type",
            Self::StandardNamespace => "standard-library namespace",
            Self::StandardFunction => "standard-library function",
            Self::StandardValue => "standard-library value",
            Self::StandardFailure => "predefined category",
            Self::Application => "application",
            Self::Locales => "locale declaration",
            Self::AuthenticationStrategy => "authentication strategy",
            Self::CredentialSlot => "credential slot",
            Self::CredentialValidation => "credential validation",
            Self::CredentialClaimMapping => "credential claim mapping",
            Self::PrincipalResolution => "principal resolution",
            Self::PrincipalMapping => "principal mapping",
            Self::Principal => "principal declaration",
            Self::PrincipalVariant => "principal variant",
            Self::Configuration => "configuration",
            Self::ConfigurationField => "configuration field",
            Self::Enum => "enum",
            Self::EnumVariant => "enum variant",
            Self::Entity => "entity",
            Self::EntityReference => "entity reference",
            Self::EntityLifecycle => "entity lifecycle",
            Self::LifecycleInitial => "lifecycle initial state",
            Self::LifecycleVisibility => "lifecycle visibility predicate",
            Self::LifecycleTransition => "lifecycle transition",
            Self::LifecyclePurge => "lifecycle purge rule",
            Self::Policy => "policy declaration",
            Self::PolicyOperation => "policy operation",
            Self::PolicyRule => "policy rule",
            Self::Value => "object type",
            Self::Input => "input",
            Self::Output => "output",
            Self::Field => "field",
            Self::GeneratedField => "generated field",
            Self::Relationship => "relationship",
            Self::PersistenceConstraint => "persistence constraint",
            Self::Failure => "failure",
            Self::Function => "function",
            Self::Action => "action",
            Self::Query => "query",
            Self::Fixture => "test fixture",
            Self::Test => "test",
            Self::Route => "route",
            Self::Job => "scheduled job",
            Self::ServiceOperation => "external service operation",
            Self::DeliverySelection => "compiler-private delivery selection",
            Self::DeliveryCompletion => "compiler-private delivery completion",
            Self::DeliveryAcceptanceProof => "compiler-private delivery acceptance proof",
            Self::DeliveryReconciliation => "compiler-private delivery receipt reconciliation",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticNode {
    pub id: NodeId,
    pub kind: NodeKind,
    pub name: String,
    pub source: String,
    pub range: TextRange,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RefinementEdge {
    pub refined: NodeId,
    pub parent: NodeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CallEdge {
    pub caller: NodeId,
    pub callee: NodeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticationResolutionEdge {
    pub resolution: NodeId,
    pub authority: NodeId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticGraph {
    pub modules: Vec<SemanticModule>,
    pub nodes: Vec<SemanticNode>,
    pub refinements: Vec<RefinementEdge>,
    /// Named contracts that include absence, including inherited field references.
    pub nullable_types: BTreeSet<String>,
    pub calls: Vec<CallEdge>,
    /// Compiler-checked external service operations and their closed effect contract.
    pub external_effects: Vec<ExternalServiceEffect>,
    pub authentication_resolutions: Vec<AuthenticationResolutionEdge>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalServiceEffect {
    pub service: String,
    pub operation: String,
    pub method: String,
    pub path: String,
    pub input: String,
    pub output: String,
    pub idempotency_type: String,
    pub egress: String,
    pub credential_slot: String,
    pub credential_header: String,
    pub imported_contract: String,
    pub import_version: String,
    pub import_sha256: String,
    pub timeout_ms: u32,
    pub max_attempts: u8,
    pub max_elapsed_ms: u32,
    pub jitter: String,
    pub redirects_allowed: bool,
    pub proxy_allowed: bool,
    pub outcomes: Vec<String>,
    pub outcome_mappings: Vec<(String, String)>,
    /// Exact source-to-import field parity accepted for the closed operation schema.
    pub input_schema: Vec<ServiceFieldParity>,
    pub output_schema: Vec<ServiceFieldParity>,
    pub source: String,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceFieldParity {
    pub field: String,
    pub source_type: String,
    pub source_shape: String,
    pub imported_shape: String,
}

impl ExternalServiceEffect {
    pub fn schema_parity_json(&self) -> String {
        let fields_json = |fields: &[ServiceFieldParity]| {
            fields
                .iter()
                .map(|field| {
                    format!(
                        "{{\"field\":\"{}\",\"source_type\":\"{}\",\"source_shape\":\"{}\",\"imported_shape\":\"{}\"}}",
                        escape_json(&field.field),
                        escape_json(&field.source_type),
                        escape_json(&field.source_shape),
                        escape_json(&field.imported_shape)
                    )
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        format!(
            "{{\"input\":[{}],\"output\":[{}]}}",
            fields_json(&self.input_schema),
            fields_json(&self.output_schema)
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticModule {
    pub name: String,
    pub source: String,
    pub imports: Vec<String>,
    pub exports: Vec<String>,
}

impl SemanticGraph {
    pub fn node(&self, name: &str) -> Option<&SemanticNode> {
        self.nodes.iter().find(|node| node.name == name)
    }

    pub fn manifest(&self) -> SemanticManifest<'_> {
        SemanticManifest { graph: self }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SemanticManifest<'graph> {
    graph: &'graph SemanticGraph,
}

impl SemanticManifest<'_> {
    pub fn to_json(self) -> String {
        let modules = self
            .graph
            .modules
            .iter()
            .map(|module| {
                format!(
                    "{{\"name\":\"{}\",\"source\":\"{}\",\"imports\":{},\"exports\":{}}}",
                    escape_json(&module.name),
                    escape_json(&module.source),
                    json_strings(&module.imports),
                    json_strings(&module.exports)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let nodes = self
            .graph
            .nodes
            .iter()
            .map(|node| {
                format!(
                    "{{\"id\":{},\"kind\":\"{}\",\"name\":\"{}\",\"source\":\"{}\",\"start\":{},\"end\":{}}}",
                    node.id.0,
                    node.kind.as_str(),
                    escape_json(&node.name),
                    escape_json(&node.source),
                    node.range.start,
                    node.range.end
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let refinements = self
            .graph
            .refinements
            .iter()
            .map(|edge| {
                format!(
                    "{{\"refined\":\"{}\",\"parent\":\"{}\"}}",
                    escape_json(&self.graph.nodes[edge.refined.0 as usize].name),
                    escape_json(&self.graph.nodes[edge.parent.0 as usize].name)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let calls = self
            .graph
            .calls
            .iter()
            .map(|edge| {
                format!(
                    "{{\"caller\":\"{}\",\"callee\":\"{}\"}}",
                    escape_json(&self.graph.nodes[edge.caller.0 as usize].name),
                    escape_json(&self.graph.nodes[edge.callee.0 as usize].name)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let authentication_resolutions = self
            .graph
            .authentication_resolutions
            .iter()
            .map(|edge| {
                format!(
                    "{{\"resolution\":\"{}\",\"authority\":\"{}\"}}",
                    escape_json(&self.graph.nodes[edge.resolution.0 as usize].name),
                    escape_json(&self.graph.nodes[edge.authority.0 as usize].name)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let external_effects = self
            .graph
            .external_effects
            .iter()
            .map(|effect| {
                format!(
                    "{{\"service\":\"{}\",\"operation\":\"{}\",\"method\":\"{}\",\"path\":\"{}\",\"input\":\"{}\",\"output\":\"{}\",\"idempotency_type\":\"{}\",\"egress\":\"{}\",\"credential_slot\":\"{}\",\"credential_header\":\"{}\",\"imported_contract\":\"{}\",\"import_version\":\"{}\",\"import_sha256\":\"{}\",\"timeout_ms\":{},\"max_attempts\":{},\"max_elapsed_ms\":{},\"jitter\":\"{}\",\"redirects_allowed\":{},\"proxy_allowed\":{},\"outcomes\":[{}],\"outcome_mappings\":[{}],\"schema_parity\":{},\"source\":\"{}\",\"start\":{},\"end\":{}}}",
                    escape_json(&effect.service),
                    escape_json(&effect.operation),
                    escape_json(&effect.method),
                    escape_json(&effect.path),
                    escape_json(&effect.input),
                    escape_json(&effect.output),
                    escape_json(&effect.idempotency_type),
                    escape_json(&effect.egress),
                    escape_json(&effect.credential_slot),
                    escape_json(&effect.credential_header),
                    escape_json(&effect.imported_contract),
                    escape_json(&effect.import_version),
                    escape_json(&effect.import_sha256),
                    effect.timeout_ms,
                    effect.max_attempts,
                    effect.max_elapsed_ms,
                    escape_json(&effect.jitter),
                    effect.redirects_allowed,
                    effect.proxy_allowed,
                    json_strings(&effect.outcomes),
                    effect
                        .outcome_mappings
                        .iter()
                        .map(|(provider, failure)| format!("[\"{}\",\"{}\"]", escape_json(provider), escape_json(failure)))
                        .collect::<Vec<_>>()
                        .join(","),
                    effect.schema_parity_json(),
                    escape_json(&effect.source),
                    effect.range.start,
                    effect.range.end
                )
            })
            .collect::<Vec<_>>()
            .join(",");

        format!(
            "{{\"schema_version\":1,\"phase\":\"semantic\",\"modules\":[{modules}],\"nodes\":[{nodes}],\"refinements\":[{refinements}],\"calls\":[{calls}],\"external_effects\":[{external_effects}],\"authentication_resolutions\":[{authentication_resolutions}]}}"
        )
    }
}

pub fn checked_manifest_json(
    graph: &SemanticGraph,
    typing: &TypeCheckResult,
    failures: &FailureCheckResult,
) -> String {
    let mut manifest = graph
        .manifest()
        .to_json()
        .replacen("\"phase\":\"semantic\"", "\"phase\":\"checked\"", 1)
        .replacen("\"schema_version\":1", "\"schema_version\":2", 1);
    manifest.pop();

    let mut expressions = typing.expressions.clone();
    expressions.sort_by(|left, right| {
        (
            &left.source,
            left.range.start,
            left.range.end,
            &left.type_name,
        )
            .cmp(&(
                &right.source,
                right.range.start,
                right.range.end,
                &right.type_name,
            ))
    });
    let expressions = expressions
        .iter()
        .map(|expression| {
            format!(
                "{{\"source\":\"{}\",\"start\":{},\"end\":{},\"type\":\"{}\"}}",
                escape_json(&expression.source),
                expression.range.start,
                expression.range.end,
                escape_json(&expression.type_name)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let mut clock_reads = typing.clock_reads.clone();
    clock_reads.sort_by_key(|read| (read.source.clone(), read.range.start, read.range.end));
    clock_reads.dedup();
    let clock_reads = clock_reads
        .iter()
        .map(|read| {
            format!(
                "{{\"source\":\"{}\",\"start\":{},\"end\":{}}}",
                escape_json(&read.source),
                read.range.start,
                read.range.end
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let contracts = failures
        .contracts
        .iter()
        .map(|contract| {
            format!(
                "{{\"name\":\"{}\",\"kind\":\"{}\",\"http_status\":{},\"code\":\"{}\",\"message\":{},\"public_fields\":{},\"internal_fields\":{}}}",
                escape_json(&contract.name),
                escape_json(&contract.kind),
                contract.http_status.map_or_else(|| "null".to_owned(), |status| status.to_string()),
                escape_json(&contract.code),
                contract
                    .message
                    .as_ref()
                    .map(|message| format!("\"{}\"", escape_json(message)))
                    .unwrap_or_else(|| "null".to_owned()),
                json_strings(&contract.public_fields),
                json_strings(&contract.internal_fields)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let route_failures = failures
        .routes
        .iter()
        .map(|route| {
            format!(
                "{{\"route\":\"{}\",\"failure\":\"{}\",\"kind\":\"{}\",\"http_status\":{},\"derived\":{}}}",
                escape_json(&route.route),
                escape_json(&route.failure),
                escape_json(&route.kind),
                route.http_status.map_or_else(|| "null".to_owned(), |status| status.to_string()),
                route.derived
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let callable_outcomes = failures
        .callables
        .iter()
        .map(|callable| {
            format!(
                "{{\"callable\":\"{}\",\"failures\":{},\"may_suspend\":{},\"completion\":\"before_caller_continues\"}}",
                escape_json(&callable.callable),
                json_strings(&callable.failures),
                callable.may_suspend
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    manifest.push_str(&format!(
        ",\"expression_types\":[{expressions}],\"clock_reads\":[{clock_reads}],\"failure_contracts\":[{contracts}],\"callable_outcomes\":[{callable_outcomes}],\"route_failures\":[{route_failures}]}}"
    ));
    manifest
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScaffoldManifest {
    pub schema_version: u32,
    pub phase: &'static str,
    pub source_files: Vec<String>,
}

impl ScaffoldManifest {
    pub fn to_json(&self) -> String {
        let sources = self
            .source_files
            .iter()
            .map(|source| format!("\"{}\"", escape_json(source)))
            .collect::<Vec<_>>()
            .join(",");

        format!(
            "{{\"schema_version\":{},\"phase\":\"{}\",\"source_files\":[{}]}}",
            self.schema_version, self.phase, sources
        )
    }
}

#[derive(Clone, Debug)]
struct PendingNode {
    kind: NodeKind,
    name: String,
    source: String,
    range: TextRange,
}

#[derive(Clone, Copy, Debug)]
enum ReferenceKind {
    Type,
    Principal,
    PrincipalVariant,
    AuthenticationField,
    ConfigurationField,
    Failure,
    StandardFailure,
    Fixture,
}

impl ReferenceKind {
    const fn user_name(self) -> &'static str {
        match self {
            Self::Type => "type",
            Self::Principal => "principal declaration",
            Self::PrincipalVariant => "principal variant",
            Self::AuthenticationField => "authentication authority or principal field",
            Self::ConfigurationField => "configuration field",
            Self::Failure => "declared failure",
            Self::StandardFailure => "predefined category",
            Self::Fixture => "test fixture",
        }
    }

    const fn examples(self) -> Option<&'static str> {
        match self {
            Self::StandardFailure => {
                Some("`InvalidValue`, `NotFound`, `Conflict`, or `Unavailable`")
            }
            Self::Type
            | Self::Principal
            | Self::PrincipalVariant
            | Self::AuthenticationField
            | Self::ConfigurationField
            | Self::Failure => None,
            Self::Fixture => None,
        }
    }

    const fn accepts(self, kind: NodeKind) -> bool {
        match self {
            Self::Type => kind.is_type(),
            Self::Principal => matches!(kind, NodeKind::Principal),
            Self::PrincipalVariant => matches!(kind, NodeKind::PrincipalVariant),
            Self::AuthenticationField => matches!(kind, NodeKind::Field),
            Self::ConfigurationField => matches!(kind, NodeKind::ConfigurationField),
            Self::Failure => matches!(kind, NodeKind::Failure | NodeKind::StandardFailure),
            Self::StandardFailure => matches!(kind, NodeKind::StandardFailure),
            Self::Fixture => matches!(kind, NodeKind::Fixture),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum ReferenceUsage {
    ApplicationPrincipal,
    AuthenticationPrincipal,
    AuthenticationMapping,
    AuthenticationAuthority,
    TypeDefinition,
    LocalAnnotation,
    Field,
    Relationship,
    InverseRelationship,
    FailureKind,
    FailsEntry,
    Parameter,
    ReturnValue,
    RouteInput,
    RouteOutput,
    ConfigurationField,
    TestFixture,
    ServiceInput,
    ServiceOutput,
    ServiceIdentity,
    ServiceOutcome,
    ServiceCredentialSlot,
}

impl ReferenceUsage {
    const fn user_name(self) -> &'static str {
        match self {
            Self::ApplicationPrincipal => "application authentication principal",
            Self::AuthenticationPrincipal => "authentication strategy principal",
            Self::AuthenticationMapping => "authentication mapping",
            Self::AuthenticationAuthority => "authentication authority lookup",
            Self::TypeDefinition => "type definition",
            Self::LocalAnnotation => "local type annotation",
            Self::Field => "field",
            Self::Relationship => "relationship",
            Self::InverseRelationship => "inverse relationship",
            Self::FailureKind => "failure's `kind`",
            Self::FailsEntry => "`fails` entry",
            Self::Parameter => "parameter",
            Self::ReturnValue => "return value",
            Self::RouteInput => "route input",
            Self::RouteOutput => "route output",
            Self::ConfigurationField => "configuration field",
            Self::TestFixture => "test fixture",
            Self::ServiceInput => "external service input",
            Self::ServiceOutput => "external service output",
            Self::ServiceIdentity => "external service idempotency identity",
            Self::ServiceOutcome => "external service outcome",
            Self::ServiceCredentialSlot => "external service credential slot",
        }
    }
}

#[derive(Clone, Debug)]
struct PendingReference {
    refined: Option<String>,
    target: String,
    expected: ReferenceKind,
    usage: ReferenceUsage,
    source: String,
    range: TextRange,
}

#[derive(Clone, Debug)]
struct PendingCall {
    caller: String,
    callee: String,
    source: String,
    range: TextRange,
}

#[derive(Clone, Debug)]
struct PendingAuthenticationResolution {
    resolution: String,
    authority: String,
}

pub fn build_semantic_graph(files: &[ParsedSyntax]) -> SemanticGraph {
    let mut builder = GraphBuilder::default();
    builder.add_prelude();
    builder.validate_naming(files);
    builder.configure_modules(files);
    for file in files {
        builder.collect_file(file);
    }
    for file in files {
        builder.collect_services(file, files);
    }
    builder.validate_fixture_service_fakes(files);
    builder.resolve_nullability();
    builder.validate_storage_nullability(files);
    builder.validate_relationships(files);
    builder.finish(files)
}

#[derive(Default)]
struct GraphBuilder {
    nodes: BTreeMap<String, PendingNode>,
    modules: Vec<SemanticModule>,
    module_scopes: BTreeMap<String, ModuleScope>,
    references: Vec<PendingReference>,
    nullable_types: BTreeSet<String>,
    nullable_references: Vec<(String, String, TextRange)>,
    calls: Vec<PendingCall>,
    authentication_resolutions: Vec<PendingAuthenticationResolution>,
    external_effects: Vec<ExternalServiceEffect>,
    atomic_callers: BTreeSet<String>,
    persistence_write_callers: BTreeSet<String>,
    diagnostics: Vec<Diagnostic>,
    application: Option<String>,
    locales: bool,
    principal: Option<String>,
    configuration: Option<String>,
    configuration_bindings: BTreeMap<String, (String, TextRange)>,
    authentication_strategies: BTreeSet<String>,
    credential_slots: BTreeMap<String, (String, TextRange)>,
    authentication_principals: Vec<(String, String, TextRange)>,
}

#[derive(Clone, Debug, Default)]
struct ModuleScope {
    imported_names: BTreeSet<String>,
}

impl GraphBuilder {
    fn validate_naming(&mut self, files: &[ParsedSyntax]) {
        for file in files {
            if let Some(module) = &file.file.module {
                for segment in &module.path {
                    self.require_name_case(segment, NameCase::LowerSnake, &file.source_name);
                }
            }
            for declaration in &file.file.declarations {
                match declaration {
                    Declaration::Application(value) => {
                        self.require_name_case(
                            &value.name,
                            NameCase::UpperCamel,
                            &file.source_name,
                        );
                    }
                    Declaration::AuthenticationStrategy(value) => {
                        self.require_name_case(
                            &value.name,
                            NameCase::LowerSnake,
                            &file.source_name,
                        );
                        for validator in &value.validators {
                            self.require_name_case(
                                &validator.name,
                                NameCase::LowerSnake,
                                &file.source_name,
                            );
                        }
                        if let jadpo_syntax::CredentialLocation::Bearer(slot) =
                            &value.transport.location
                        {
                            self.require_name_case(slot, NameCase::LowerSnake, &file.source_name);
                        }
                        for mapping in value.claims.iter().chain(
                            value
                                .resolutions
                                .iter()
                                .flat_map(|resolution| &resolution.mappings),
                        ) {
                            self.require_name_case(
                                &mapping.source,
                                NameCase::LowerSnake,
                                &file.source_name,
                            );
                        }
                    }
                    Declaration::Principal(value) => {
                        self.require_name_case(
                            &value.name,
                            NameCase::UpperCamel,
                            &file.source_name,
                        );
                        for variant in &value.variants {
                            self.require_name_case(
                                &variant.name,
                                NameCase::LowerSnake,
                                &file.source_name,
                            );
                            self.require_field_names(&variant.fields, &file.source_name);
                        }
                    }
                    Declaration::Config(value) => {
                        self.require_name_case(
                            &value.name,
                            NameCase::UpperCamel,
                            &file.source_name,
                        );
                        for field in &value.fields {
                            self.require_name_case(
                                &field.name,
                                NameCase::LowerSnake,
                                &file.source_name,
                            );
                        }
                    }
                    Declaration::Type(value) => {
                        if !value.name.text.contains('.')
                            && !value.name.text.starts_with("__jadpo_")
                        {
                            self.require_name_case(
                                &value.name,
                                NameCase::UpperCamel,
                                &file.source_name,
                            );
                        }
                    }
                    Declaration::Enum(value) => {
                        self.require_name_case(
                            &value.name,
                            NameCase::UpperCamel,
                            &file.source_name,
                        );
                        for variant in &value.variants {
                            self.require_name_case(
                                &variant.name,
                                NameCase::LowerSnake,
                                &file.source_name,
                            );
                            self.require_field_names(&variant.fields, &file.source_name);
                        }
                    }
                    Declaration::Record(value) => {
                        if !value.name.text.contains('.')
                            && !value.name.text.starts_with("__jadpo_")
                        {
                            self.require_name_case(
                                &value.name,
                                NameCase::UpperCamel,
                                &file.source_name,
                            );
                        }
                        self.require_field_names(&value.fields, &file.source_name);
                        for inverse in &value.inverses {
                            self.require_name_case(
                                &inverse.name,
                                NameCase::LowerSnake,
                                &file.source_name,
                            );
                        }
                        for constraint in &value.persistence_constraints {
                            self.require_name_case(
                                &constraint.name,
                                NameCase::LowerSnake,
                                &file.source_name,
                            );
                        }
                        if let Some(dossier) = &value.dossier {
                            if let Some(persistence) = &dossier.persistence {
                                self.require_name_case(
                                    &persistence.store,
                                    NameCase::LowerSnake,
                                    &file.source_name,
                                );
                            }
                            for representation in &dossier.representations {
                                self.require_name_case(
                                    &representation.name,
                                    NameCase::LowerSnake,
                                    &file.source_name,
                                );
                                self.require_name_case(
                                    &representation.store,
                                    NameCase::LowerSnake,
                                    &file.source_name,
                                );
                                self.require_name_case(
                                    &representation.from,
                                    NameCase::LowerSnake,
                                    &file.source_name,
                                );
                                if let Some(strategy) = &representation.strategy {
                                    self.require_name_case(
                                        strategy,
                                        NameCase::LowerSnake,
                                        &file.source_name,
                                    );
                                }
                            }
                            if let Some(lifecycle) = &dossier.lifecycle {
                                if let Some(initial) = &lifecycle.initial {
                                    for field in initial {
                                        self.require_name_case(
                                            &field.name,
                                            NameCase::LowerSnake,
                                            &file.source_name,
                                        );
                                    }
                                }
                                for transition in &lifecycle.transitions {
                                    self.require_name_case(
                                        &transition.name,
                                        NameCase::LowerSnake,
                                        &file.source_name,
                                    );
                                    for field in &transition.set {
                                        self.require_name_case(
                                            &field.name,
                                            NameCase::LowerSnake,
                                            &file.source_name,
                                        );
                                    }
                                }
                                if let Some(purge) = &lifecycle.purge {
                                    self.require_name_case(
                                        &purge.from,
                                        NameCase::LowerSnake,
                                        &file.source_name,
                                    );
                                }
                            }
                        }
                    }
                    Declaration::Failure(value) => {
                        self.require_name_case(
                            &value.name,
                            NameCase::UpperCamel,
                            &file.source_name,
                        );
                        self.require_field_names(&value.public_fields, &file.source_name);
                        self.require_field_names(&value.internal_fields, &file.source_name);
                    }
                    Declaration::Callable(value) => {
                        let operation = value
                            .name
                            .text
                            .rsplit('.')
                            .next()
                            .unwrap_or(&value.name.text);
                        let operation_name = jadpo_syntax::Name {
                            text: operation.to_owned(),
                            range: value.name.range,
                        };
                        self.require_name_case(
                            &operation_name,
                            NameCase::LowerSnake,
                            &file.source_name,
                        );
                        if standard_operation_namespace(operation).is_some() {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("SEM_STANDARD_OPERATION_RESERVED")
                                    .with_fact(DiagnosticFact::Name(operation.to_owned())),
                                &file.source_name,
                                value.name.range,
                            ));
                        }
                        for parameter in &value.parameters {
                            self.require_name_case(
                                &parameter.name,
                                NameCase::LowerSnake,
                                &file.source_name,
                            );
                        }
                        self.require_block_names(&value.body, &file.source_name);
                    }
                    Declaration::Fixture(value) => {
                        self.require_name_case(
                            &value.name,
                            NameCase::LowerSnake,
                            &file.source_name,
                        );
                        if let Some(clock) = &value.clock {
                            self.require_expression_names(clock, &file.source_name);
                        }
                        if let Some(configuration) = &value.configuration {
                            for item in configuration {
                                self.require_name_case(
                                    &item.name,
                                    NameCase::LowerSnake,
                                    &file.source_name,
                                );
                                self.require_expression_names(&item.value, &file.source_name);
                            }
                        }
                        for fake in &value.service_fakes {
                            self.require_name_case(
                                &fake.service,
                                NameCase::UpperCamel,
                                &file.source_name,
                            );
                            for outcome in &fake.outcomes {
                                self.require_name_case(
                                    &outcome.operation,
                                    NameCase::LowerSnake,
                                    &file.source_name,
                                );
                                match &outcome.value {
                                    jadpo_syntax::FixtureServiceFakeValue::Accepted(value) => {
                                        self.require_expression_names(value, &file.source_name)
                                    }
                                    jadpo_syntax::FixtureServiceFakeValue::Declared(value) => {
                                        for segment in &value.path {
                                            self.require_name_case(
                                                segment,
                                                NameCase::LowerSnake,
                                                &file.source_name,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Declaration::Test(value) => {
                        self.require_block_names(&value.body, &file.source_name);
                    }
                    Declaration::Route(value) => {
                        self.require_field_names(&value.path_fields, &file.source_name);
                        for header in &value.headers {
                            self.require_name_case(
                                &header.name,
                                NameCase::LowerSnake,
                                &file.source_name,
                            );
                        }
                        if let Some(run) = &value.run {
                            self.require_expression_names(
                                &Expression::Invocation(run.clone()),
                                &file.source_name,
                            );
                        }
                        if let Some(action) = &value.inline_action {
                            self.require_block_names(&action.body, &file.source_name);
                        }
                    }
                    Declaration::Job(value) => {
                        self.require_name_case(
                            &value.name,
                            NameCase::LowerSnake,
                            &file.source_name,
                        );
                        if let Some(run) = &value.run {
                            self.require_expression_names(
                                &Expression::Invocation(run.clone()),
                                &file.source_name,
                            );
                        }
                    }
                    Declaration::Locales(_) => {}
                }
                if let Some(name) = declaration_name(declaration) {
                    if is_standard_namespace(&name.text) {
                        self.diagnostics.push(with_span(
                            Diagnostic::error("SEM_STANDARD_NAMESPACE_RESERVED")
                                .with_fact(DiagnosticFact::Name(name.text.clone())),
                            &file.source_name,
                            name.range,
                        ));
                    }
                }
            }
        }
    }

    fn require_field_names(&mut self, fields: &[jadpo_syntax::FieldDeclaration], source: &str) {
        for field in fields {
            self.require_name_case(&field.name, NameCase::LowerSnake, source);
            if let Some(reference) = &field.reference {
                if let Some(relationship) = &reference.relationship {
                    self.require_name_case(relationship, NameCase::LowerSnake, source);
                }
            }
        }
    }

    fn require_block_names(&mut self, block: &Block, source: &str) {
        for statement in &block.statements {
            match statement {
                Statement::Binding(statement) => {
                    self.require_name_case(&statement.name, NameCase::LowerSnake, source);
                    self.require_expression_names(&statement.value, source);
                }
                Statement::Assignment(statement) => {
                    self.require_expression_names(&statement.value, source);
                }
                Statement::Return(statement) => {
                    self.require_expression_names(&statement.value, source);
                }
                Statement::Reject(statement) => {
                    for field in &statement.values {
                        self.require_expression_names(&field.value, source);
                    }
                }
                Statement::If(statement) => {
                    self.require_expression_names(&statement.condition, source);
                    self.require_block_names(&statement.then_block, source);
                    if let Some(block) = &statement.else_block {
                        self.require_block_names(block, source);
                    }
                }
                Statement::Match(statement) => {
                    self.require_expression_names(&statement.subject, source);
                    for arm in &statement.arms {
                        match &arm.pattern {
                            jadpo_syntax::MatchPattern::Variant(pattern) => {
                                for binding in &pattern.bindings {
                                    self.require_name_case(binding, NameCase::LowerSnake, source);
                                }
                            }
                            jadpo_syntax::MatchPattern::OptionalSome(pattern) => {
                                self.require_name_case(
                                    &pattern.binding,
                                    NameCase::LowerSnake,
                                    source,
                                );
                            }
                            _ => {}
                        }
                        self.require_block_names(&arm.body, source);
                    }
                }
                Statement::Assert(statement) => {
                    self.require_expression_names(&statement.condition, source);
                }
                Statement::AdvanceClock(statement) => {
                    self.require_expression_names(&statement.duration, source);
                }
                Statement::Unsupported(_) => {}
            }
        }
    }

    fn require_expression_names(&mut self, expression: &Expression, source: &str) {
        match expression {
            Expression::Literal(_) | Expression::Name(_) | Expression::Missing(_) => {}
            Expression::Invocation(value) => {
                for argument in &value.arguments {
                    self.require_expression_names(argument, source);
                }
                for argument in &value.named_arguments {
                    self.require_expression_names(&argument.value, source);
                }
            }
            Expression::TestCall(value) => {
                for argument in &value.invocation.arguments {
                    self.require_expression_names(argument, source);
                }
                for argument in &value.invocation.named_arguments {
                    self.require_expression_names(&argument.value, source);
                }
            }
            Expression::Object(value) => {
                for field in &value.fields {
                    self.require_expression_names(&field.value, source);
                }
            }
            Expression::Construction(value) => {
                for field in &value.fields {
                    self.require_expression_names(&field.value, source);
                }
            }
            Expression::Create(value) => {
                for field in &value.fields {
                    self.require_expression_names(&field.value, source);
                }
                for conflict in &value.conflicts {
                    for field in &conflict.rejection.values {
                        self.require_expression_names(&field.value, source);
                    }
                }
            }
            Expression::Query(value) => {
                self.require_expression_names(&value.value, source);
                if let Some(page) = &value.page {
                    for predicate in &page.predicates {
                        self.require_expression_names(&predicate.value, source);
                    }
                    self.require_expression_names(&page.after, source);
                    self.require_expression_names(&page.limit, source);
                }
                if let Some(pagination) = &value.pagination {
                    self.require_expression_names(&pagination.limit, source);
                    self.require_expression_names(&pagination.offset, source);
                }
                for include in &value.includes {
                    self.require_expression_names(&include.pagination.limit, source);
                    self.require_expression_names(&include.pagination.offset, source);
                }
                if let Some(missing) = &value.missing {
                    for field in &missing.values {
                        self.require_expression_names(&field.value, source);
                    }
                }
            }
            Expression::Update(value) => {
                self.require_expression_names(&value.value, source);
                for field in &value.changes {
                    self.require_expression_names(&field.value, source);
                }
                for change in &value.conditional_changes {
                    self.require_expression_names(&change.change.value, source);
                }
                for rejection in value
                    .empty
                    .iter()
                    .chain(std::iter::once(&value.missing))
                    .chain(value.conflicts.iter().map(|conflict| &conflict.rejection))
                {
                    for field in &rejection.values {
                        self.require_expression_names(&field.value, source);
                    }
                }
            }
            Expression::Delete(value) => {
                self.require_expression_names(&value.value, source);
                for rejection in std::iter::once(&value.missing)
                    .chain(value.conflicts.iter().map(|conflict| &conflict.rejection))
                {
                    for field in &rejection.values {
                        self.require_expression_names(&field.value, source);
                    }
                }
            }
            Expression::Attempt(value) => self.require_expression_names(&value.value, source),
            Expression::OutcomeMatch(value) => {
                self.require_expression_names(&value.subject, source);
                for arm in &value.arms {
                    if let jadpo_syntax::OutcomeMatchPattern::Success(binding) = &arm.pattern {
                        self.require_name_case(binding, NameCase::LowerSnake, source);
                    }
                    match &arm.body {
                        jadpo_syntax::OutcomeMatchArmBody::Value(value) => {
                            self.require_expression_names(value, source)
                        }
                        jadpo_syntax::OutcomeMatchArmBody::Reject(value) => {
                            for field in &value.values {
                                self.require_expression_names(&field.value, source);
                            }
                        }
                        jadpo_syntax::OutcomeMatchArmBody::Propagate(_) => {}
                    }
                }
            }
            Expression::Unary(value) => self.require_expression_names(&value.value, source),
            Expression::Binary(value) => {
                self.require_expression_names(&value.left, source);
                self.require_expression_names(&value.right, source);
            }
            Expression::Grouped(value) => self.require_expression_names(&value.value, source),
        }
    }

    fn require_name_case(&mut self, name: &jadpo_syntax::Name, expected: NameCase, source: &str) {
        if expected.accepts(&name.text) {
            return;
        }
        let diagnostic = Diagnostic::error("SEM_NAME_CASE")
            .with_fact(DiagnosticFact::Name(name.text.clone()))
            .with_fact(DiagnosticFact::Expected(expected.description().to_owned()))
            .with_fact(DiagnosticFact::SuggestedName(expected.convert(&name.text)));
        self.diagnostics
            .push(with_span(diagnostic, source, name.range));
    }

    fn add_prelude(&mut self) {
        for name in [
            "Bool",
            "Bytes",
            "Decimal",
            "Duration",
            "Int",
            "List",
            "Map",
            "Object",
            "Set",
            "Text",
            "Unit",
            "Uuid",
            "Instant",
            "CalendarDate",
            "Time",
            "Zone",
            "Locale",
            "PresentationText",
            "InstantRange",
            "LocalOverlap",
            "LocalGap",
            "InvalidDay",
            "Weekday",
            "TimeFormat",
            "FriendlyTimeFormat",
        ] {
            self.add_node(PendingNode {
                kind: NodeKind::PreludeType,
                name: name.to_owned(),
                source: "<prelude>".to_owned(),
                range: TextRange::new(0, 0),
            });
        }
        for (owner, variants) in [
            ("LocalOverlap", &["reject", "earlier", "later"][..]),
            (
                "LocalGap",
                &["reject", "shift_forward", "shift_backward"][..],
            ),
            ("InvalidDay", &["reject", "last_valid_day"][..]),
            (
                "Weekday",
                &[
                    "monday",
                    "tuesday",
                    "wednesday",
                    "thursday",
                    "friday",
                    "saturday",
                    "sunday",
                ][..],
            ),
            (
                "TimeFormat",
                &[
                    "date_full",
                    "date_long",
                    "date_medium",
                    "date_short",
                    "time_full",
                    "time_long",
                    "time_medium",
                    "time_short",
                    "date_time_full",
                    "date_time_long",
                    "date_time_medium",
                    "date_time_short",
                ][..],
            ),
            ("FriendlyTimeFormat", &["conversational"][..]),
        ] {
            for variant in variants {
                self.add_node(PendingNode {
                    kind: NodeKind::StandardValue,
                    name: format!("{owner}.{variant}"),
                    source: "<prelude>".to_owned(),
                    range: TextRange::new(0, 0),
                });
            }
        }
        for zone in include_str!("../../../data/iana-zones-2026c.txt").lines() {
            self.add_node(PendingNode {
                kind: NodeKind::StandardValue,
                name: format!("Zone.{}", standard_variant_name(zone)),
                source: "<prelude>".to_owned(),
                range: TextRange::new(0, 0),
            });
        }
        self.add_node(PendingNode {
            kind: NodeKind::StandardNamespace,
            name: "temporal".to_owned(),
            source: "<prelude>".to_owned(),
            range: TextRange::new(0, 0),
        });
        self.add_node(PendingNode {
            kind: NodeKind::StandardNamespace,
            name: "collection".to_owned(),
            source: "<prelude>".to_owned(),
            range: TextRange::new(0, 0),
        });
        for name in [
            "in_zone",
            "resolve",
            "add_elapsed",
            "between",
            "add_days",
            "add_weeks",
            "add_months",
            "add_years",
            "add_local_days",
            "add_local_weeks",
            "add_local_months",
            "add_local_years",
            "day_bounds",
            "week_bounds",
            "month_bounds",
            "year_bounds",
            "calendar_date",
            "year",
            "month",
            "day",
            "weekday",
            "hour",
            "minute",
            "second",
            "millisecond",
            "offset",
            "zone",
            "same_zone",
            "same_local",
            "format",
            "format_friendly",
        ] {
            self.add_node(PendingNode {
                kind: NodeKind::StandardFunction,
                name: format!("temporal.{name}"),
                source: "<prelude>".to_owned(),
                range: TextRange::new(0, 0),
            });
        }
        for name in ["Email", "Url", "IpAddress"] {
            self.add_node(PendingNode {
                kind: NodeKind::Type,
                name: name.to_owned(),
                source: "<prelude>".to_owned(),
                range: TextRange::new(0, 0),
            });
            self.references.push(PendingReference {
                refined: Some(name.to_owned()),
                target: "Text".to_owned(),
                expected: ReferenceKind::Type,
                usage: ReferenceUsage::TypeDefinition,
                source: "<prelude>".to_owned(),
                range: TextRange::new(0, 0),
            });
        }
        for name in [
            "Conflict",
            "InternalFault",
            "InvalidRequest",
            "InvalidValue",
            "Misconfigured",
            "NotFound",
            "NotPermitted",
            "NotVisible",
            "OutcomeUnknown",
            "PreconditionFailed",
            "RateLimited",
            "Rejected",
            "TimedOut",
            "Unauthenticated",
            "Unavailable",
        ] {
            self.add_node(PendingNode {
                kind: NodeKind::StandardFailure,
                name: name.to_owned(),
                source: "<prelude>".to_owned(),
                range: TextRange::new(0, 0),
            });
        }
    }

    fn configure_modules(&mut self, files: &[ParsedSyntax]) {
        let explicit_modules = files.iter().any(|file| file.file.module.is_some());
        if !explicit_modules {
            for file in files {
                if let Some(export) = file.file.exports.first() {
                    self.diagnostics.push(with_span(
                        Diagnostic::error("MOD_PUBLIC_REQUIRES_MODULE"),
                        &file.source_name,
                        export.range,
                    ));
                }
            }
            return;
        }

        let mut files_by_module = BTreeMap::<String, &ParsedSyntax>::new();
        for file in files {
            let Some(module) = &file.file.module else {
                self.diagnostics.push(with_span(
                    Diagnostic::error("MOD_MODULE_REQUIRED"),
                    &file.source_name,
                    file.file.range,
                ));
                continue;
            };
            let name = name_expression(&module.path);
            if module
                .path
                .first()
                .is_some_and(|segment| is_standard_namespace(&segment.text))
            {
                self.diagnostics.push(with_span(
                    Diagnostic::error("MOD_STANDARD_NAMESPACE_RESERVED")
                        .with_fact(DiagnosticFact::Name(name.clone())),
                    &file.source_name,
                    module.range,
                ));
            }
            if let Some(previous) = files_by_module.get(&name) {
                self.diagnostics.push(with_span(
                    Diagnostic::error("MOD_DUPLICATE_MODULE")
                        .with_note(format!("first module file is {}", previous.source_name)),
                    &file.source_name,
                    module.range,
                ));
            } else {
                files_by_module.insert(name, file);
            }
        }

        let mut dependencies = files_by_module
            .keys()
            .map(|name| (name.clone(), BTreeSet::new()))
            .collect::<BTreeMap<_, _>>();

        for file in files {
            let Some(module) = &file.file.module else {
                continue;
            };
            let module_name = name_expression(&module.path);
            let local_names = file
                .file
                .declarations
                .iter()
                .filter_map(declaration_name)
                .map(|name| name.text.as_str())
                .collect::<BTreeSet<_>>();
            let mut imported_names = BTreeSet::new();
            let mut imports = Vec::new();
            for import in &file.file.imports {
                let target_name = name_expression(&import.module);
                if import
                    .module
                    .first()
                    .is_some_and(|segment| is_standard_namespace(&segment.text))
                {
                    self.diagnostics.push(with_span(
                        Diagnostic::error("MOD_STANDARD_NAMESPACE_IMPORT")
                            .with_fact(DiagnosticFact::Name(target_name)),
                        &file.source_name,
                        import.range,
                    ));
                    continue;
                }
                if target_name == module_name {
                    self.diagnostics.push(with_span(
                        Diagnostic::error("MOD_SELF_IMPORT"),
                        &file.source_name,
                        import.range,
                    ));
                    continue;
                }
                let Some(target) = files_by_module.get(&target_name).copied() else {
                    self.diagnostics.push(with_span(
                        Diagnostic::error("MOD_UNKNOWN_MODULE"),
                        &file.source_name,
                        import.range,
                    ));
                    continue;
                };
                dependencies
                    .entry(module_name.clone())
                    .or_default()
                    .insert(target_name.clone());
                let target_exports = target
                    .file
                    .exports
                    .iter()
                    .map(|name| name.text.as_str())
                    .collect::<BTreeSet<_>>();
                let target_declarations = target
                    .file
                    .declarations
                    .iter()
                    .filter_map(declaration_name)
                    .map(|name| name.text.as_str())
                    .collect::<BTreeSet<_>>();
                for name in &import.names {
                    if local_names.contains(name.text.as_str()) {
                        self.diagnostics.push(with_span(
                            Diagnostic::error("MOD_IMPORT_CONFLICT"),
                            &file.source_name,
                            name.range,
                        ));
                        continue;
                    }
                    if !imported_names.insert(name.text.clone()) {
                        self.diagnostics.push(with_span(
                            Diagnostic::error("MOD_DUPLICATE_IMPORT"),
                            &file.source_name,
                            name.range,
                        ));
                        continue;
                    }
                    if !target_exports.contains(name.text.as_str()) {
                        let (code, _message) = if target_declarations.contains(name.text.as_str()) {
                            (
                                "MOD_PRIVATE_IMPORT",
                                format!(
                                    "declaration `{}` is private to module `{target_name}`",
                                    name.text
                                ),
                            )
                        } else {
                            (
                                "MOD_UNKNOWN_EXPORT",
                                format!(
                                    "module `{target_name}` has no exported declaration `{}`",
                                    name.text
                                ),
                            )
                        };
                        self.diagnostics.push(with_span(
                            Diagnostic::error(code),
                            &file.source_name,
                            name.range,
                        ));
                        imported_names.remove(&name.text);
                        continue;
                    }
                    imports.push(format!("{target_name}.{}", name.text));
                }
            }

            let mut exports = file
                .file
                .exports
                .iter()
                .map(|name| name.text.clone())
                .collect::<Vec<_>>();
            exports.sort();
            imports.sort();
            self.modules.push(SemanticModule {
                name: module_name.clone(),
                source: file.source_name.clone(),
                imports,
                exports,
            });
            self.module_scopes
                .insert(file.source_name.clone(), ModuleScope { imported_names });
        }

        let mut emitted = BTreeSet::new();
        loop {
            let ready = dependencies
                .iter()
                .filter(|(name, required)| {
                    !emitted.contains(*name)
                        && required.iter().all(|module| emitted.contains(module))
                })
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>();
            if ready.is_empty() {
                break;
            }
            emitted.extend(ready);
        }
        if emitted.len() != dependencies.len() {
            let cyclic = dependencies
                .keys()
                .filter(|name| !emitted.contains(*name))
                .cloned()
                .collect::<BTreeSet<_>>();
            for file in files {
                let Some(module) = &file.file.module else {
                    continue;
                };
                let module_name = name_expression(&module.path);
                if !cyclic.contains(&module_name) {
                    continue;
                }
                if let Some(import) = file
                    .file
                    .imports
                    .iter()
                    .find(|import| cyclic.contains(&name_expression(&import.module)))
                {
                    self.diagnostics.push(with_span(
                        Diagnostic::error("MOD_IMPORT_CYCLE"),
                        &file.source_name,
                        import.range,
                    ));
                }
            }
        }
        self.modules
            .sort_by(|left, right| left.name.cmp(&right.name));
    }

    fn collect_file(&mut self, file: &ParsedSyntax) {
        for declaration in &file.file.declarations {
            match declaration {
                Declaration::Locales(declaration) => {
                    if self.locales {
                        self.diagnostics.push(with_span(
                            Diagnostic::error("SEM_MULTIPLE_LOCALE_DECLARATIONS"),
                            &file.source_name,
                            declaration.range,
                        ));
                        self.add_node(PendingNode {
                            kind: NodeKind::Locales,
                            name: format!("locales#{}", declaration.range.start),
                            source: file.source_name.clone(),
                            range: declaration.range,
                        });
                    } else {
                        self.locales = true;
                        self.add_node(PendingNode {
                            kind: NodeKind::Locales,
                            name: "locales".to_owned(),
                            source: file.source_name.clone(),
                            range: declaration.range,
                        });
                        let mut emitted = BTreeSet::new();
                        for locale in &declaration.supported {
                            let tag = locale.text.trim_matches('"');
                            let name = format!("Locale.{}", standard_variant_name(tag));
                            if emitted.insert(name.clone()) {
                                self.add_node(PendingNode {
                                    kind: NodeKind::StandardValue,
                                    name,
                                    source: file.source_name.clone(),
                                    range: locale.range,
                                });
                            }
                        }
                    }
                }
                Declaration::Application(declaration) => {
                    if self.application.is_some() {
                        self.diagnostics.push(with_span(
                            Diagnostic::error("SEM_MULTIPLE_APPLICATIONS"),
                            &file.source_name,
                            declaration.name.range,
                        ));
                    } else {
                        self.application = Some(declaration.name.text.clone());
                    }
                    self.add_authored_node(
                        NodeKind::Application,
                        &declaration.name.text,
                        &file.source_name,
                        declaration.name.range,
                    );
                    let principal_reference = &declaration.authentication.principal;
                    if principal_reference.nullable
                        || !principal_reference.arguments.is_empty()
                        || principal_reference.path.len() != 1
                    {
                        self.diagnostics.push(with_span(
                            Diagnostic::error("SEM_APPLICATION_PRINCIPAL_SHAPE"),
                            &file.source_name,
                            principal_reference.range,
                        ));
                    } else {
                        self.references.push(PendingReference {
                            refined: None,
                            target: name_expression(&principal_reference.path),
                            expected: ReferenceKind::Principal,
                            usage: ReferenceUsage::ApplicationPrincipal,
                            source: file.source_name.clone(),
                            range: principal_reference.range,
                        });
                    }
                    let revocation = &declaration.authentication.revocation;
                    match (revocation.mode, revocation.maximum_delay.as_ref()) {
                        (jadpo_syntax::RevocationMode::Bounded, None) => {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("SEM_REVOCATION_DELAY_REQUIRED"),
                                &file.source_name,
                                revocation.range,
                            ));
                        }
                        (jadpo_syntax::RevocationMode::Immediate, Some(delay)) => {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("SEM_REVOCATION_DELAY_FORBIDDEN"),
                                &file.source_name,
                                delay.range,
                            ));
                        }
                        (_, Some(delay)) if !positive_duration(&delay.text) => {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("SEM_REVOCATION_DELAY_INVALID"),
                                &file.source_name,
                                delay.range,
                            ));
                        }
                        _ => {}
                    }
                }
                Declaration::AuthenticationStrategy(declaration) => {
                    if !self
                        .authentication_strategies
                        .insert(declaration.name.text.clone())
                    {
                        self.diagnostics.push(with_span(
                            Diagnostic::error("SEM_AUTH_STRATEGY_DUPLICATE"),
                            &file.source_name,
                            declaration.name.range,
                        ));
                        continue;
                    }
                    let strategy_name = format!("authentication.{}", declaration.name.text);
                    self.add_authored_node(
                        NodeKind::AuthenticationStrategy,
                        &strategy_name,
                        &file.source_name,
                        declaration.name.range,
                    );

                    if declaration.validators.is_empty() {
                        self.diagnostics.push(with_span(
                            Diagnostic::error("SEM_AUTH_VALIDATOR_REQUIRED"),
                            &file.source_name,
                            declaration.range,
                        ));
                    }
                    let mut validators = BTreeSet::new();
                    for validator in &declaration.validators {
                        if !validators.insert(validator.name.text.as_str()) {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("SEM_AUTH_VALIDATOR_DUPLICATE"),
                                &file.source_name,
                                validator.name.range,
                            ));
                            continue;
                        }
                        if matches!(
                            validator.mode.text.as_str(),
                            "signed" | "opaque" | "api_key" | "jwt"
                        ) {
                            self.add_authored_node(
                                NodeKind::CredentialValidation,
                                &format!(
                                    "{strategy_name}.validator.{}.{}",
                                    validator.name.text, validator.mode.text
                                ),
                                &file.source_name,
                                validator.range,
                            );
                        } else {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("SEM_AUTH_VALIDATION_MODE_UNKNOWN"),
                                &file.source_name,
                                validator.mode.range,
                            ));
                        }

                        if let Some(binding) = &validator.credentials {
                            for (role, reference) in [
                                ("identity", &binding.identity),
                                ("principal", &binding.principal),
                                ("verifier", &binding.verifier),
                                ("expires", &binding.expires),
                                ("revoked", &binding.revoked),
                            ] {
                                let target = name_expression(&reference.path);
                                self.add_authored_node(
                                    NodeKind::CredentialValidation,
                                    &format!(
                                        "{strategy_name}.validator.{}.credentials.{role}.{target}",
                                        validator.name.text
                                    ),
                                    &file.source_name,
                                    reference.range,
                                );
                                self.references.push(PendingReference {
                                    refined: None,
                                    target,
                                    expected: ReferenceKind::AuthenticationField,
                                    usage: ReferenceUsage::AuthenticationAuthority,
                                    source: file.source_name.clone(),
                                    range: reference.range,
                                });
                            }
                        }

                        if matches!(validator.principal.text.as_str(), "user" | "service") {
                            self.authentication_principals.push((
                                validator.principal.text.clone(),
                                file.source_name.clone(),
                                validator.principal.range,
                            ));
                        } else {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("SEM_AUTH_PRINCIPAL_VARIANT_UNKNOWN"),
                                &file.source_name,
                                validator.principal.range,
                            ));
                        }
                    }

                    let mut claim_targets = BTreeSet::new();
                    for mapping in &declaration.claims {
                        let target = name_expression(&mapping.target.path);
                        if !claim_targets.insert(target.clone()) {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("SEM_AUTH_MAPPING_DUPLICATE"),
                                &file.source_name,
                                mapping.target.range,
                            ));
                            continue;
                        }
                        if mapping.target.path.len() != 3
                            || !matches!(mapping.target.path[1].text.as_str(), "user" | "service")
                        {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("SEM_AUTH_MAPPING_TARGET_INVALID"),
                                &file.source_name,
                                mapping.target.range,
                            ));
                            continue;
                        }
                        self.add_authored_node(
                            NodeKind::CredentialClaimMapping,
                            &format!("{strategy_name}.claim.{}.{}", mapping.source.text, target),
                            &file.source_name,
                            mapping.range,
                        );
                        self.references.push(PendingReference {
                            refined: None,
                            target,
                            expected: ReferenceKind::AuthenticationField,
                            usage: ReferenceUsage::AuthenticationMapping,
                            source: file.source_name.clone(),
                            range: mapping.target.range,
                        });
                    }

                    let mut resolution_variants = BTreeSet::new();
                    for resolution in &declaration.resolutions {
                        if !matches!(resolution.principal.text.as_str(), "user" | "service") {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("SEM_AUTH_PRINCIPAL_VARIANT_UNKNOWN"),
                                &file.source_name,
                                resolution.principal.range,
                            ));
                            continue;
                        }
                        if !resolution_variants.insert(resolution.principal.text.as_str()) {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("SEM_AUTH_RESOLUTION_DUPLICATE"),
                                &file.source_name,
                                resolution.principal.range,
                            ));
                            continue;
                        }
                        let resolution_name =
                            format!("{strategy_name}.resolution.{}", resolution.principal.text);
                        self.add_authored_node(
                            NodeKind::PrincipalResolution,
                            &resolution_name,
                            &file.source_name,
                            resolution.range,
                        );
                        let authority = name_expression(&resolution.authority.path);
                        if resolution.authority.path.len() != 2 {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("SEM_AUTH_RESOLUTION_AUTHORITY_INVALID"),
                                &file.source_name,
                                resolution.authority.range,
                            ));
                        } else {
                            self.authentication_resolutions
                                .push(PendingAuthenticationResolution {
                                    resolution: resolution_name.clone(),
                                    authority: authority.clone(),
                                });
                            self.references.push(PendingReference {
                                refined: None,
                                target: authority,
                                expected: ReferenceKind::AuthenticationField,
                                usage: ReferenceUsage::AuthenticationAuthority,
                                source: file.source_name.clone(),
                                range: resolution.authority.range,
                            });
                        }
                        if resolution.mappings.is_empty() {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("SEM_AUTH_MAPPING_REQUIRED"),
                                &file.source_name,
                                resolution.range,
                            ));
                        }
                        let authority_root = resolution
                            .authority
                            .path
                            .first()
                            .map(|name| name.text.as_str())
                            .unwrap_or_default();
                        let mut targets = BTreeSet::new();
                        for mapping in &resolution.mappings {
                            let target = name_expression(&mapping.target.path);
                            if !targets.insert(target.clone()) {
                                self.diagnostics.push(with_span(
                                    Diagnostic::error("SEM_AUTH_MAPPING_DUPLICATE"),
                                    &file.source_name,
                                    mapping.target.range,
                                ));
                                continue;
                            }
                            if mapping.target.path.len() != 3
                                || mapping.target.path[1].text != resolution.principal.text
                            {
                                self.diagnostics.push(with_span(
                                    Diagnostic::error("SEM_AUTH_MAPPING_TARGET_INVALID"),
                                    &file.source_name,
                                    mapping.target.range,
                                ));
                                continue;
                            }
                            self.add_authored_node(
                                NodeKind::PrincipalMapping,
                                &format!("{resolution_name}.{}.{}", mapping.source.text, target),
                                &file.source_name,
                                mapping.range,
                            );
                            for (reference_target, range) in [
                                (
                                    format!("{authority_root}.{}", mapping.source.text),
                                    mapping.source.range,
                                ),
                                (target, mapping.target.range),
                            ] {
                                self.references.push(PendingReference {
                                    refined: None,
                                    target: reference_target,
                                    expected: ReferenceKind::AuthenticationField,
                                    usage: ReferenceUsage::AuthenticationMapping,
                                    source: file.source_name.clone(),
                                    range,
                                });
                            }
                        }
                        self.references.push(PendingReference {
                            refined: None,
                            target: resolution.inactive.text.clone(),
                            expected: ReferenceKind::Failure,
                            usage: ReferenceUsage::AuthenticationMapping,
                            source: file.source_name.clone(),
                            range: resolution.inactive.range,
                        });
                    }

                    let (slot_key, slot_name, slot_range) = match &declaration.transport.location {
                        jadpo_syntax::CredentialLocation::Cookie(location) => {
                            let cookie = unquote_string(&location.text);
                            (
                                format!("cookie:{cookie}"),
                                format!("{strategy_name}.cookie.{cookie}"),
                                location.range,
                            )
                        }
                        jadpo_syntax::CredentialLocation::Bearer(location) => {
                            if location.text != "authorization_header" {
                                self.diagnostics.push(with_span(
                                    Diagnostic::error("SEM_AUTH_BEARER_LOCATION_FORBIDDEN"),
                                    &file.source_name,
                                    location.range,
                                ));
                            }
                            (
                                format!("bearer:{}", location.text),
                                format!("{strategy_name}.bearer.{}", location.text),
                                location.range,
                            )
                        }
                    };
                    if self
                        .credential_slots
                        .insert(slot_key, (declaration.name.text.clone(), slot_range))
                        .is_some()
                    {
                        self.diagnostics.push(with_span(
                            Diagnostic::error("SEM_AUTH_CREDENTIAL_SLOT_DUPLICATE"),
                            &file.source_name,
                            slot_range,
                        ));
                    } else {
                        self.add_authored_node(
                            NodeKind::CredentialSlot,
                            &slot_name,
                            &file.source_name,
                            slot_range,
                        );
                    }
                }
                Declaration::Principal(declaration) => {
                    if self.principal.is_some() {
                        self.diagnostics.push(with_span(
                            Diagnostic::error("SEM_MULTIPLE_PRINCIPALS"),
                            &file.source_name,
                            declaration.name.range,
                        ));
                    } else {
                        self.principal = Some(declaration.name.text.clone());
                    }
                    self.add_authored_node(
                        NodeKind::Principal,
                        &declaration.name.text,
                        &file.source_name,
                        declaration.name.range,
                    );
                    let mut variants = BTreeSet::new();
                    for variant in &declaration.variants {
                        let variant_name =
                            format!("{}.{}", declaration.name.text, variant.kind.as_str());
                        if !variants.insert(variant.kind.as_str()) {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("SEM_PRINCIPAL_VARIANT_DUPLICATE"),
                                &file.source_name,
                                variant.name.range,
                            ));
                            continue;
                        }
                        self.add_authored_node(
                            NodeKind::PrincipalVariant,
                            &variant_name,
                            &file.source_name,
                            variant.name.range,
                        );
                        for field in &variant.fields {
                            if !field.persistence.is_empty() || field.reference.is_some() {
                                self.diagnostics.push(with_span(
                                    Diagnostic::error("DATA_MODIFIER_NON_ENTITY"),
                                    &file.source_name,
                                    field.range,
                                ));
                            }
                            let field_name = format!("{variant_name}.{}", field.name.text);
                            self.add_authored_node(
                                NodeKind::Field,
                                &field_name,
                                &file.source_name,
                                field.name.range,
                            );
                            self.add_type_reference(
                                Some(field_name),
                                &field.field_type,
                                &file.source_name,
                                ReferenceUsage::Field,
                            );
                        }
                    }
                    if !variants.contains("user") || !variants.contains("service") {
                        self.diagnostics.push(with_span(
                            Diagnostic::error("SEM_PRINCIPAL_VARIANT_REQUIRED"),
                            &file.source_name,
                            declaration.range,
                        ));
                    }
                }
                Declaration::Config(declaration) => {
                    if self.configuration.is_some() {
                        self.diagnostics.push(with_span(
                            Diagnostic::error("CONFIG_MULTIPLE_DECLARATIONS"),
                            &file.source_name,
                            declaration.name.range,
                        ));
                    } else {
                        self.configuration = Some(declaration.name.text.clone());
                    }
                    self.add_authored_node(
                        NodeKind::Configuration,
                        &declaration.name.text,
                        &file.source_name,
                        declaration.name.range,
                    );
                    for field in &declaration.fields {
                        let field_name = format!("{}.{}", declaration.name.text, field.name.text);
                        self.add_authored_node(
                            NodeKind::ConfigurationField,
                            &field_name,
                            &file.source_name,
                            field.name.range,
                        );
                        self.add_type_reference(
                            Some(field_name),
                            &field.field_type,
                            &file.source_name,
                            ReferenceUsage::ConfigurationField,
                        );
                        let Some(binding) = &field.binding else {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("CONFIG_BINDING_REQUIRED"),
                                &file.source_name,
                                field.range,
                            ));
                            continue;
                        };
                        let binding_name = unquote_string(&binding.text);
                        if binding_name.is_empty() {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("CONFIG_BINDING_EMPTY"),
                                &file.source_name,
                                binding.range,
                            ));
                        } else if !valid_configuration_binding(&binding_name) {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("CONFIG_BINDING_INVALID"),
                                &file.source_name,
                                binding.range,
                            ));
                        } else if self
                            .configuration_bindings
                            .insert(binding_name, (file.source_name.clone(), binding.range))
                            .is_some()
                        {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("CONFIG_DUPLICATE_BINDING"),
                                &file.source_name,
                                binding.range,
                            ));
                        }
                        if field.secret && field.default.is_some() {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("CONFIG_SECRET_DEFAULT"),
                                &file.source_name,
                                field.default.as_ref().expect("default exists").range,
                            ));
                        }
                    }
                }
                Declaration::Type(declaration) => {
                    self.add_authored_node(
                        NodeKind::Type,
                        &declaration.name.text,
                        &file.source_name,
                        declaration.name.range,
                    );
                    self.add_type_reference(
                        Some(declaration.name.text.clone()),
                        &declaration.parent,
                        &file.source_name,
                        ReferenceUsage::TypeDefinition,
                    );
                }
                Declaration::Enum(declaration) => {
                    self.add_authored_node(
                        NodeKind::Enum,
                        &declaration.name.text,
                        &file.source_name,
                        declaration.name.range,
                    );
                    for variant in &declaration.variants {
                        let variant_name =
                            format!("{}.{}", declaration.name.text, variant.name.text);
                        self.add_authored_node(
                            NodeKind::EnumVariant,
                            &variant_name,
                            &file.source_name,
                            variant.name.range,
                        );
                        for field in &variant.fields {
                            if !field.persistence.is_empty() || field.reference.is_some() {
                                let diagnostic = Diagnostic::error("DATA_MODIFIER_NON_ENTITY");
                                self.diagnostics.push(with_span(
                                    diagnostic,
                                    &file.source_name,
                                    field.range,
                                ));
                            }
                            let field_name = format!("{variant_name}.{}", field.name.text);
                            self.add_authored_node(
                                NodeKind::Field,
                                &field_name,
                                &file.source_name,
                                field.name.range,
                            );
                            self.add_type_reference(
                                Some(field_name),
                                &field.field_type,
                                &file.source_name,
                                ReferenceUsage::Field,
                            );
                        }
                    }
                }
                Declaration::Record(declaration) => {
                    let kind = match declaration.kind {
                        RecordKind::Entity => NodeKind::Entity,
                        RecordKind::Value => NodeKind::Value,
                        RecordKind::Input => NodeKind::Input,
                        RecordKind::Output => NodeKind::Output,
                    };
                    self.add_authored_node(
                        kind,
                        &declaration.name.text,
                        &file.source_name,
                        declaration.name.range,
                    );
                    if let Some(policy) = &declaration.policy {
                        self.add_policy_nodes(
                            &format!("{}.policy", declaration.name.text),
                            policy,
                            &file.source_name,
                        );
                    }
                    if declaration.kind == RecordKind::Entity {
                        self.add_authored_node(
                            NodeKind::EntityReference,
                            &format!("{}.Ref", declaration.name.text),
                            &file.source_name,
                            declaration.name.range,
                        );
                        if let Some(lifecycle) = declaration
                            .dossier
                            .as_ref()
                            .and_then(|dossier| dossier.lifecycle.as_ref())
                        {
                            self.add_authored_node(
                                NodeKind::EntityLifecycle,
                                &format!("{}.lifecycle", declaration.name.text),
                                &file.source_name,
                                lifecycle.range,
                            );
                            if let Some(initial) = &lifecycle.initial {
                                self.add_authored_node(
                                    NodeKind::LifecycleInitial,
                                    &format!("{}.lifecycle.initial", declaration.name.text),
                                    &file.source_name,
                                    initial.first().map_or(lifecycle.range, |field| field.range),
                                );
                            }
                            if let Some(visible) = &lifecycle.visible {
                                self.add_authored_node(
                                    NodeKind::LifecycleVisibility,
                                    &format!("{}.lifecycle.visibility", declaration.name.text),
                                    &file.source_name,
                                    visible.range(),
                                );
                            }
                            for transition in &lifecycle.transitions {
                                self.add_authored_node(
                                    NodeKind::LifecycleTransition,
                                    &format!(
                                        "{}.lifecycle.transition.{}",
                                        declaration.name.text, transition.name.text
                                    ),
                                    &file.source_name,
                                    transition.range,
                                );
                            }
                            if let Some(purge) = &lifecycle.purge {
                                self.add_authored_node(
                                    NodeKind::LifecyclePurge,
                                    &format!("{}.lifecycle.purge", declaration.name.text),
                                    &file.source_name,
                                    purge.range,
                                );
                            }
                        }
                    }
                    let mut identity_field = None;
                    for field in &declaration.fields {
                        if declaration.kind != RecordKind::Entity
                            && (!field.persistence.is_empty() || field.reference.is_some())
                        {
                            let diagnostic = Diagnostic::error("DATA_MODIFIER_NON_ENTITY");
                            self.diagnostics.push(with_span(
                                diagnostic,
                                &file.source_name,
                                field.range,
                            ));
                        }
                        if field.persistence.contains(&PersistenceModifier::Identity) {
                            if let Some(_previous) = identity_field {
                                let diagnostic = Diagnostic::error("DATA_MULTIPLE_IDENTITIES");
                                self.diagnostics.push(with_span(
                                    diagnostic,
                                    &file.source_name,
                                    field.range,
                                ));
                            } else {
                                identity_field = Some(field.name.text.as_str());
                            }
                        }
                        let field_name = format!("{}.{}", declaration.name.text, field.name.text);
                        self.add_authored_node(
                            NodeKind::Field,
                            &field_name,
                            &file.source_name,
                            field.name.range,
                        );
                        if field.generated.is_some() {
                            self.add_authored_node(
                                NodeKind::GeneratedField,
                                &format!("{field_name}.generated"),
                                &file.source_name,
                                field.name.range,
                            );
                        }
                        if let Some(policy) = &field.policy {
                            self.add_policy_nodes(
                                &format!("{field_name}.policy"),
                                policy,
                                &file.source_name,
                            );
                        }
                        self.add_type_reference(
                            Some(field_name),
                            &field.field_type,
                            &file.source_name,
                            ReferenceUsage::Field,
                        );
                        if let Some(reference) = &field.reference {
                            self.add_type_reference(
                                None,
                                &reference.target,
                                &file.source_name,
                                ReferenceUsage::Relationship,
                            );
                            if let Some(relationship) = &reference.relationship {
                                self.add_authored_node(
                                    NodeKind::Relationship,
                                    &format!("{}.{}", declaration.name.text, relationship.text),
                                    &file.source_name,
                                    relationship.range,
                                );
                            }
                        }
                    }
                    for inverse in &declaration.inverses {
                        let inverse_name =
                            format!("{}.{}", declaration.name.text, inverse.name.text);
                        self.add_authored_node(
                            NodeKind::Relationship,
                            &inverse_name,
                            &file.source_name,
                            inverse.name.range,
                        );
                        self.references.push(PendingReference {
                            refined: None,
                            target: inverse.target.text.clone(),
                            expected: ReferenceKind::Type,
                            usage: ReferenceUsage::InverseRelationship,
                            source: file.source_name.clone(),
                            range: inverse.target.range,
                        });
                        self.add_type_reference(
                            None,
                            &inverse.via,
                            &file.source_name,
                            ReferenceUsage::InverseRelationship,
                        );
                    }
                    let fields = declaration
                        .fields
                        .iter()
                        .map(|field| (field.name.text.as_str(), field))
                        .collect::<BTreeMap<_, _>>();
                    let mut constraint_shapes = BTreeSet::new();
                    for constraint in &declaration.persistence_constraints {
                        self.add_authored_node(
                            NodeKind::PersistenceConstraint,
                            &format!("{}.{}", declaration.name.text, constraint.name.text),
                            &file.source_name,
                            constraint.name.range,
                        );
                        if constraint.fields.len() < 2 {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("DATA_COMPOUND_CONSTRAINT_FIELDS"),
                                &file.source_name,
                                constraint.range,
                            ));
                        }
                        let mut seen_fields = BTreeSet::new();
                        for field_name in &constraint.fields {
                            if !seen_fields.insert(field_name.text.as_str()) {
                                self.diagnostics.push(with_span(
                                    Diagnostic::error("DATA_CONSTRAINT_DUPLICATE_FIELD"),
                                    &file.source_name,
                                    field_name.range,
                                ));
                                continue;
                            }
                            let Some(_field) = fields.get(field_name.text.as_str()) else {
                                self.diagnostics.push(with_span(
                                    Diagnostic::error("DATA_CONSTRAINT_UNKNOWN_FIELD"),
                                    &file.source_name,
                                    field_name.range,
                                ));
                                continue;
                            };
                        }
                        let mut shape = constraint
                            .fields
                            .iter()
                            .map(|field| field.text.as_str())
                            .collect::<Vec<_>>();
                        shape.sort_unstable();
                        if !constraint_shapes.insert(shape) {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("DATA_DUPLICATE_CONSTRAINT_SHAPE"),
                                &file.source_name,
                                constraint.range,
                            ));
                        }
                    }
                }
                Declaration::Failure(declaration) => {
                    self.add_authored_node(
                        NodeKind::Failure,
                        &declaration.name.text,
                        &file.source_name,
                        declaration.name.range,
                    );
                    self.references.push(PendingReference {
                        refined: None,
                        target: declaration.kind.text.clone(),
                        expected: ReferenceKind::StandardFailure,
                        usage: ReferenceUsage::FailureKind,
                        source: file.source_name.clone(),
                        range: declaration.kind.range,
                    });
                    for (scope, fields) in [
                        ("public", &declaration.public_fields),
                        ("internal", &declaration.internal_fields),
                    ] {
                        for field in fields {
                            let field_name =
                                format!("{}.{}.{}", declaration.name.text, scope, field.name.text);
                            self.add_authored_node(
                                NodeKind::Field,
                                &field_name,
                                &file.source_name,
                                field.name.range,
                            );
                            self.add_type_reference(
                                Some(field_name),
                                &field.field_type,
                                &file.source_name,
                                ReferenceUsage::Field,
                            );
                        }
                    }
                }
                Declaration::Callable(declaration) => {
                    let kind = match declaration.kind {
                        CallableKind::Function => NodeKind::Function,
                        CallableKind::Action => NodeKind::Action,
                        CallableKind::Query => NodeKind::Query,
                    };
                    let callable_name = declaration.name.text.clone();
                    if declaration.kind == CallableKind::Action
                        && declaration.consistency == Some(ConsistencyDisposition::Atomic)
                    {
                        self.atomic_callers.insert(callable_name.clone());
                    }
                    self.add_authored_node(
                        kind,
                        &callable_name,
                        &file.source_name,
                        declaration.name.range,
                    );
                    for parameter in &declaration.parameters {
                        self.add_type_reference(
                            None,
                            &parameter.parameter_type,
                            &file.source_name,
                            ReferenceUsage::Parameter,
                        );
                    }
                    self.add_type_reference(
                        None,
                        &declaration.return_type,
                        &file.source_name,
                        ReferenceUsage::ReturnValue,
                    );
                    for failure in &declaration.failures {
                        self.references.push(PendingReference {
                            refined: None,
                            target: failure.text.clone(),
                            expected: ReferenceKind::Failure,
                            usage: ReferenceUsage::FailsEntry,
                            source: file.source_name.clone(),
                            range: failure.range,
                        });
                    }
                    self.collect_block_calls(&callable_name, &declaration.body, &file.source_name);
                }
                Declaration::Fixture(declaration) => {
                    self.add_authored_node(
                        NodeKind::Fixture,
                        &declaration.name.text,
                        &file.source_name,
                        declaration.name.range,
                    );
                    if let Some(clock) = &declaration.clock {
                        self.collect_expression_calls(
                            &format!("fixture:{}", declaration.name.text),
                            clock,
                            &file.source_name,
                        );
                    }
                    if let Some(configuration) = &declaration.configuration {
                        for item in configuration {
                            self.collect_expression_calls(
                                &format!("fixture:{}", declaration.name.text),
                                &item.value,
                                &file.source_name,
                            );
                        }
                    }
                    for fake in &declaration.service_fakes {
                        for outcome in &fake.outcomes {
                            if let jadpo_syntax::FixtureServiceFakeValue::Accepted(value) =
                                &outcome.value
                            {
                                self.collect_expression_calls(
                                    &format!("fixture:{}", declaration.name.text),
                                    value,
                                    &file.source_name,
                                );
                            }
                        }
                    }
                }
                Declaration::Test(declaration) => {
                    let name = format!("test:{}", declaration.name.text);
                    self.add_authored_node(
                        NodeKind::Test,
                        &name,
                        &file.source_name,
                        declaration.name.range,
                    );
                    if let Some(fixture) = &declaration.fixture {
                        self.references.push(PendingReference {
                            refined: None,
                            target: fixture.text.clone(),
                            expected: ReferenceKind::Fixture,
                            usage: ReferenceUsage::TestFixture,
                            source: file.source_name.clone(),
                            range: fixture.range,
                        });
                    }
                    self.collect_block_calls(&name, &declaration.body, &file.source_name);
                }
                Declaration::Job(declaration) => {
                    self.add_authored_node(
                        NodeKind::Job,
                        &declaration.name.text,
                        &file.source_name,
                        declaration.name.range,
                    );
                    if let Some(run) = &declaration.run {
                        self.collect_expression_calls(
                            &declaration.name.text,
                            &Expression::Invocation(run.clone()),
                            &file.source_name,
                        );
                    }
                }
                Declaration::Route(declaration) => {
                    let route_name = format!(
                        "{} {}",
                        http_method_name(declaration.method),
                        declaration.path
                    );
                    self.add_authored_node(
                        NodeKind::Route,
                        &route_name,
                        &file.source_name,
                        declaration.range,
                    );
                    if let Some(input) = &declaration.input {
                        self.add_type_reference(
                            None,
                            input,
                            &file.source_name,
                            ReferenceUsage::RouteInput,
                        );
                    }
                    if let Some(query) = &declaration.query {
                        self.add_type_reference(
                            None,
                            query,
                            &file.source_name,
                            ReferenceUsage::RouteInput,
                        );
                    }
                    for header in &declaration.headers {
                        self.add_type_reference(
                            None,
                            &header.field_type,
                            &file.source_name,
                            ReferenceUsage::RouteInput,
                        );
                    }
                    if let Some(output) = &declaration.output {
                        self.add_type_reference(
                            None,
                            output,
                            &file.source_name,
                            ReferenceUsage::RouteOutput,
                        );
                    }
                    if let Some(run) = &declaration.run {
                        self.calls.push(PendingCall {
                            caller: route_name.clone(),
                            callee: name_expression(&run.callee.path),
                            source: file.source_name.clone(),
                            range: run.callee.range,
                        });
                    }
                    if let Some(action) = &declaration.inline_action {
                        self.collect_block_calls(&route_name, &action.body, &file.source_name);
                    }
                }
            }
        }
    }

    fn collect_services(&mut self, file: &ParsedSyntax, files: &[ParsedSyntax]) {
        for service in &file.file.services {
            let configuration = self.configuration.clone();
            match checked_service_effect(
                service,
                &file.source_name,
                files,
                configuration.as_deref(),
            ) {
                Ok(effect) => {
                    self.add_node(PendingNode {
                        kind: NodeKind::ServiceOperation,
                        name: format!("{}.{}", effect.service, effect.operation),
                        source: file.source_name.clone(),
                        range: service.range,
                    });
                    for type_name in [
                        effect.idempotency_type.as_str(),
                        effect.input.as_str(),
                        effect.output.as_str(),
                    ] {
                        self.add_type_reference(
                            None,
                            &service_type_reference(type_name, service.range),
                            &file.source_name,
                            if type_name == effect.idempotency_type {
                                ReferenceUsage::ServiceIdentity
                            } else if type_name == effect.input {
                                ReferenceUsage::ServiceInput
                            } else {
                                ReferenceUsage::ServiceOutput
                            },
                        );
                    }
                    for outcome in &effect.outcomes {
                        self.references.push(PendingReference {
                            refined: None,
                            target: outcome.clone(),
                            expected: ReferenceKind::Failure,
                            usage: ReferenceUsage::ServiceOutcome,
                            source: file.source_name.clone(),
                            range: service.range,
                        });
                    }
                    if let Some(configuration) = &self.configuration {
                        self.references.push(PendingReference {
                            refined: None,
                            target: format!("{configuration}.mail_api_key"),
                            expected: ReferenceKind::ConfigurationField,
                            usage: ReferenceUsage::ServiceCredentialSlot,
                            source: file.source_name.clone(),
                            range: service.range,
                        });
                    } else {
                        self.diagnostics.push(with_span(
                            Diagnostic::error("SEM_SERVICE_CONFIGURATION_MISSING")
                                .with_fact(DiagnosticFact::Name(service.name.text.clone())),
                            &file.source_name,
                            service.range,
                        ));
                    }
                    self.external_effects.push(effect);
                }
                Err(code) => {
                    self.diagnostics.push(with_span(
                        Diagnostic::error(code)
                            .with_fact(DiagnosticFact::Name(service.name.text.clone())),
                        &file.source_name,
                        service.range,
                    ));
                }
            }
        }
    }

    fn validate_fixture_service_fakes(&mut self, files: &[ParsedSyntax]) {
        for file in files {
            for declaration in &file.file.declarations {
                let Declaration::Fixture(fixture) = declaration else {
                    continue;
                };
                let mut seen_services = BTreeSet::new();
                for fake in &fixture.service_fakes {
                    if !seen_services.insert(fake.service.text.clone()) {
                        self.diagnostics.push(with_span(
                            Diagnostic::error("SEM_SERVICE_CONTRACT_INVALID")
                                .with_fact(DiagnosticFact::Name(fake.service.text.clone())),
                            &file.source_name,
                            fake.service.range,
                        ));
                        continue;
                    }
                    let Some(effect) = self
                        .external_effects
                        .iter()
                        .find(|effect| effect.service == fake.service.text)
                    else {
                        self.diagnostics.push(with_span(
                            Diagnostic::error("SEM_UNKNOWN_NAME")
                                .with_fact(DiagnosticFact::Name(fake.service.text.clone()))
                                .with_fact(DiagnosticFact::Expected(
                                    "declared external service".to_owned(),
                                ))
                                .with_fact(DiagnosticFact::Usage("test fixture fake".to_owned())),
                            &file.source_name,
                            fake.service.range,
                        ));
                        continue;
                    };
                    for outcome in &fake.outcomes {
                        if outcome.operation.text != effect.operation {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("SEM_UNKNOWN_NAME")
                                    .with_fact(DiagnosticFact::Name(format!(
                                        "{}.{}",
                                        fake.service.text, outcome.operation.text
                                    )))
                                    .with_fact(DiagnosticFact::Expected(
                                        "operation declared by this external service".to_owned(),
                                    ))
                                    .with_fact(DiagnosticFact::Usage(
                                        "test fixture fake".to_owned(),
                                    )),
                                &file.source_name,
                                outcome.operation.range,
                            ));
                            continue;
                        }
                        if let jadpo_syntax::FixtureServiceFakeValue::Declared(name) =
                            &outcome.value
                        {
                            let declared = name_expression(&name.path);
                            if !effect
                                .outcome_mappings
                                .iter()
                                .any(|(name, _)| name == &declared)
                            {
                                self.diagnostics.push(with_span(
                                    Diagnostic::error("SEM_UNKNOWN_NAME")
                                        .with_fact(DiagnosticFact::Name(declared))
                                        .with_fact(DiagnosticFact::Expected(
                                            "outcome declared by the pinned service contract"
                                                .to_owned(),
                                        ))
                                        .with_fact(DiagnosticFact::Usage(
                                            "test fixture fake".to_owned(),
                                        )),
                                    &file.source_name,
                                    name.range,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    fn validate_relationships(&mut self, files: &[ParsedSyntax]) {
        let mut entities = BTreeMap::new();
        for file in files {
            for declaration in &file.file.declarations {
                if let Declaration::Record(record) = declaration {
                    if record.kind == RecordKind::Entity {
                        entities.insert(
                            record.name.text.clone(),
                            (record, file.source_name.as_str()),
                        );
                    }
                }
            }
        }

        let mut dependencies = entities
            .keys()
            .map(|name| (name.clone(), BTreeSet::new()))
            .collect::<BTreeMap<_, _>>();

        for (entity_name, (entity, source)) in &entities {
            for field in &entity.fields {
                let Some(reference) = &field.reference else {
                    continue;
                };
                let target_parts = reference
                    .target
                    .path
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<Vec<_>>();
                if target_parts.len() != 2
                    || !reference.target.arguments.is_empty()
                    || reference.target.nullable
                {
                    let diagnostic = Diagnostic::error("DATA_RELATIONSHIP_TARGET_FIELD");
                    self.diagnostics
                        .push(with_span(diagnostic, source, reference.target.range));
                    continue;
                }

                let target_entity_name = target_parts[0];
                let target_field_name = target_parts[1];
                let Some((target_entity, _)) = entities.get(target_entity_name) else {
                    continue;
                };
                let Some(target_field) = target_entity
                    .fields
                    .iter()
                    .find(|candidate| candidate.name.text == target_field_name)
                else {
                    let diagnostic = Diagnostic::error("DATA_RELATIONSHIP_TARGET_FIELD");
                    self.diagnostics
                        .push(with_span(diagnostic, source, reference.target.range));
                    continue;
                };

                if self.reference_is_nullable(&target_field.field_type)
                    || (!target_field
                        .persistence
                        .contains(&PersistenceModifier::Identity)
                        && !target_field
                            .persistence
                            .contains(&PersistenceModifier::Unique))
                {
                    let diagnostic = Diagnostic::error("DATA_RELATIONSHIP_TARGET_NOT_KEY");
                    self.diagnostics
                        .push(with_span(diagnostic, source, reference.target.range));
                }

                let source_type = field
                    .field_type
                    .path
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<Vec<_>>();
                if source_type != target_parts || !field.field_type.arguments.is_empty() {
                    let diagnostic = Diagnostic::error("DATA_RELATIONSHIP_TYPE_MISMATCH");
                    self.diagnostics
                        .push(with_span(diagnostic, source, field.field_type.range));
                }

                if reference.on_delete == ReferenceDeleteAction::SetNull
                    && !self.reference_is_nullable(&field.field_type)
                {
                    let diagnostic = Diagnostic::error("DATA_RELATIONSHIP_SET_NULL_REQUIRED");
                    self.diagnostics
                        .push(with_span(diagnostic, source, reference.range));
                }

                if target_entity_name != entity_name {
                    dependencies
                        .entry(entity_name.clone())
                        .or_default()
                        .insert(target_entity_name.to_owned());
                }
            }

            let mut inverse_names = BTreeSet::new();
            for inverse in &entity.inverses {
                if !inverse_names.insert(inverse.name.text.as_str())
                    || entity
                        .fields
                        .iter()
                        .any(|field| field.name.text == inverse.name.text)
                {
                    let diagnostic = Diagnostic::error("DATA_INVERSE_DUPLICATE_NAME");
                    self.diagnostics
                        .push(with_span(diagnostic, source, inverse.name.range));
                }

                let via_parts = inverse
                    .via
                    .path
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<Vec<_>>();
                if via_parts.len() != 2
                    || !inverse.via.arguments.is_empty()
                    || inverse.via.nullable
                    || via_parts.first().copied() != Some(inverse.target.text.as_str())
                {
                    let diagnostic = Diagnostic::error("DATA_INVERSE_VIA_FIELD");
                    self.diagnostics
                        .push(with_span(diagnostic, source, inverse.via.range));
                    continue;
                }
                let child_name = via_parts[0];
                let child_field_name = via_parts[1];
                let Some((child, _)) = entities.get(child_name) else {
                    continue;
                };
                let Some(child_field) = child
                    .fields
                    .iter()
                    .find(|field| field.name.text == child_field_name)
                else {
                    let diagnostic = Diagnostic::error("DATA_INVERSE_VIA_FIELD");
                    self.diagnostics
                        .push(with_span(diagnostic, source, inverse.via.range));
                    continue;
                };
                let points_back = child_field.reference.as_ref().is_some_and(|reference| {
                    reference.target.path.len() == 2
                        && reference.target.path[0].text == *entity_name
                });
                if !points_back {
                    let diagnostic = Diagnostic::error("DATA_INVERSE_NOT_OWNING_REFERENCE");
                    self.diagnostics
                        .push(with_span(diagnostic, source, inverse.via.range));
                }
                if inverse.cardinality == jadpo_syntax::InverseCardinality::Optional
                    && !child_field.persistence.iter().any(|modifier| {
                        matches!(
                            modifier,
                            PersistenceModifier::Identity | PersistenceModifier::Unique
                        )
                    })
                {
                    let diagnostic = Diagnostic::error("DATA_INVERSE_OPTIONAL_NOT_UNIQUE");
                    self.diagnostics
                        .push(with_span(diagnostic, source, inverse.via.range));
                }
            }
        }

        let mut emitted = BTreeSet::new();
        loop {
            let ready = dependencies
                .iter()
                .filter(|(name, required)| {
                    !emitted.contains(*name)
                        && required
                            .iter()
                            .all(|dependency| emitted.contains(dependency))
                })
                .map(|(name, _)| name.clone())
                .collect::<Vec<_>>();
            if ready.is_empty() {
                break;
            }
            emitted.extend(ready);
        }

        if emitted.len() != dependencies.len() {
            for (entity_name, (entity, source)) in &entities {
                if emitted.contains(entity_name) {
                    continue;
                }
                if let Some(reference) = entity.fields.iter().find_map(|field| {
                    field.reference.as_ref().filter(|reference| {
                        reference
                            .target
                            .path
                            .first()
                            .is_some_and(|target| !emitted.contains(&target.text))
                    })
                }) {
                    let diagnostic = Diagnostic::error("DATA_RELATIONSHIP_CYCLE");
                    self.diagnostics
                        .push(with_span(diagnostic, source, reference.range));
                }
            }
        }
    }

    fn add_node(&mut self, node: PendingNode) {
        if let Some(previous) = self.nodes.get(&node.name) {
            let diagnostic = Diagnostic::error("SEM_DUPLICATE_DECLARATION")
                .with_fact(DiagnosticFact::Name(node.name.clone()))
                .with_note(format!(
                    "first declaration is at {}:{}..{}",
                    previous.source, previous.range.start, previous.range.end
                ));
            self.diagnostics
                .push(with_span(diagnostic, &node.source, node.range));
            return;
        }
        self.nodes.insert(node.name.clone(), node);
    }

    fn add_authored_node(&mut self, kind: NodeKind, name: &str, source: &str, range: TextRange) {
        if is_standard_namespace(name) {
            return;
        }
        self.add_node(PendingNode {
            kind,
            name: name.to_owned(),
            source: source.to_owned(),
            range,
        });
    }

    fn add_policy_nodes(
        &mut self,
        prefix: &str,
        policy: &jadpo_syntax::PolicyDeclaration,
        source: &str,
    ) {
        self.add_authored_node(NodeKind::Policy, prefix, source, policy.range);
        for (index, rule) in policy.rules.iter().enumerate() {
            self.add_authored_node(
                NodeKind::PolicyRule,
                &format!("{prefix}.rule.{index}"),
                source,
                rule.range,
            );
        }
        for operation in &policy.operations {
            let operation_prefix = format!("{prefix}.operation.{}", operation.name.text);
            self.add_authored_node(
                NodeKind::PolicyOperation,
                &operation_prefix,
                source,
                operation.range,
            );
            for (index, rule) in operation.rules.iter().enumerate() {
                self.add_authored_node(
                    NodeKind::PolicyRule,
                    &format!("{operation_prefix}.rule.{index}"),
                    source,
                    rule.range,
                );
            }
        }
    }

    fn add_type_reference(
        &mut self,
        refined: Option<String>,
        reference: &TypeReference,
        source: &str,
        usage: ReferenceUsage,
    ) {
        if reference.nullable {
            if let Some(name) = &refined {
                self.nullable_types.insert(name.clone());
            }
            self.nullable_references.push((
                name_expression(&reference.path),
                source.to_owned(),
                reference.range,
            ));
        }
        self.references.push(PendingReference {
            refined,
            target: name_expression(&reference.path),
            expected: ReferenceKind::Type,
            usage,
            source: source.to_owned(),
            range: reference.range,
        });
        for argument in &reference.arguments {
            self.add_type_reference(None, argument, source, usage);
        }
    }

    fn collect_block_calls(&mut self, caller: &str, block: &Block, source: &str) {
        for statement in &block.statements {
            match statement {
                Statement::Binding(statement) => {
                    if let Some(annotation) = &statement.annotation {
                        self.add_type_reference(
                            None,
                            annotation,
                            source,
                            ReferenceUsage::LocalAnnotation,
                        );
                    }
                    self.collect_expression_calls(caller, &statement.value, source)
                }
                Statement::Assignment(statement) => {
                    self.collect_expression_calls(caller, &statement.value, source)
                }
                Statement::Return(statement) => {
                    self.collect_expression_calls(caller, &statement.value, source)
                }
                Statement::Reject(statement) => {
                    for field in &statement.values {
                        self.collect_expression_calls(caller, &field.value, source);
                    }
                }
                Statement::If(statement) => {
                    self.collect_expression_calls(caller, &statement.condition, source);
                    self.collect_block_calls(caller, &statement.then_block, source);
                    if let Some(else_block) = &statement.else_block {
                        self.collect_block_calls(caller, else_block, source);
                    }
                }
                Statement::Match(statement) => {
                    self.collect_expression_calls(caller, &statement.subject, source);
                    for arm in &statement.arms {
                        self.collect_block_calls(caller, &arm.body, source);
                    }
                }
                Statement::Assert(statement) => {
                    self.collect_expression_calls(caller, &statement.condition, source)
                }
                Statement::AdvanceClock(statement) => {
                    self.collect_expression_calls(caller, &statement.duration, source)
                }
                Statement::Unsupported(_) => {}
            }
        }
    }

    fn collect_expression_calls(&mut self, caller: &str, expression: &Expression, source: &str) {
        match expression {
            Expression::Invocation(invocation) => {
                self.calls.push(PendingCall {
                    caller: caller.to_owned(),
                    callee: name_expression(&invocation.callee.path),
                    source: source.to_owned(),
                    range: invocation.callee.range,
                });
                for argument in &invocation.arguments {
                    self.collect_expression_calls(caller, argument, source);
                }
                for argument in &invocation.named_arguments {
                    self.collect_expression_calls(caller, &argument.value, source);
                }
            }
            Expression::TestCall(call) => {
                self.calls.push(PendingCall {
                    caller: caller.to_owned(),
                    callee: name_expression(&call.invocation.callee.path),
                    source: source.to_owned(),
                    range: call.invocation.callee.range,
                });
                for argument in &call.invocation.arguments {
                    self.collect_expression_calls(caller, argument, source);
                }
                for argument in &call.invocation.named_arguments {
                    self.collect_expression_calls(caller, &argument.value, source);
                }
            }
            Expression::Construction(construction) => {
                for field in &construction.fields {
                    self.collect_expression_calls(caller, &field.value, source);
                }
            }
            Expression::Object(object) => {
                for field in &object.fields {
                    self.collect_expression_calls(caller, &field.value, source);
                }
            }
            Expression::Create(create) => {
                self.persistence_write_callers.insert(caller.to_owned());
                for field in &create.fields {
                    self.collect_expression_calls(caller, &field.value, source);
                }
                for conflict in &create.conflicts {
                    for field in &conflict.rejection.values {
                        self.collect_expression_calls(caller, &field.value, source);
                    }
                }
            }
            Expression::Query(query) => {
                self.collect_expression_calls(caller, &query.value, source);
                if let Some(page) = &query.page {
                    for predicate in &page.predicates {
                        self.collect_expression_calls(caller, &predicate.value, source);
                    }
                    self.collect_expression_calls(caller, &page.after, source);
                    self.collect_expression_calls(caller, &page.limit, source);
                }
                if let Some(pagination) = &query.pagination {
                    self.collect_expression_calls(caller, &pagination.limit, source);
                    self.collect_expression_calls(caller, &pagination.offset, source);
                }
                for include in &query.includes {
                    self.collect_expression_calls(caller, &include.pagination.limit, source);
                    self.collect_expression_calls(caller, &include.pagination.offset, source);
                }
                if let Some(missing) = &query.missing {
                    for field in &missing.values {
                        self.collect_expression_calls(caller, &field.value, source);
                    }
                }
            }
            Expression::Update(update) => {
                self.persistence_write_callers.insert(caller.to_owned());
                self.collect_expression_calls(caller, &update.value, source);
                for change in &update.changes {
                    self.collect_expression_calls(caller, &change.value, source);
                }
                for conditional in &update.conditional_changes {
                    self.collect_expression_calls(caller, &conditional.change.value, source);
                }
                for binding in update
                    .empty
                    .iter()
                    .chain(std::iter::once(&update.missing))
                    .chain(update.conflicts.iter().map(|conflict| &conflict.rejection))
                {
                    for field in &binding.values {
                        self.collect_expression_calls(caller, &field.value, source);
                    }
                }
            }
            Expression::Delete(delete) => {
                self.persistence_write_callers.insert(caller.to_owned());
                self.collect_expression_calls(caller, &delete.value, source);
                for binding in std::iter::once(&delete.missing)
                    .chain(delete.conflicts.iter().map(|conflict| &conflict.rejection))
                {
                    for field in &binding.values {
                        self.collect_expression_calls(caller, &field.value, source);
                    }
                }
            }
            Expression::Binary(binary) => {
                self.collect_expression_calls(caller, &binary.left, source);
                self.collect_expression_calls(caller, &binary.right, source);
            }
            Expression::Unary(unary) => self.collect_expression_calls(caller, &unary.value, source),
            Expression::Grouped(grouped) => {
                self.collect_expression_calls(caller, &grouped.value, source)
            }
            Expression::Attempt(attempt) => {
                self.collect_expression_calls(caller, &attempt.value, source)
            }
            Expression::OutcomeMatch(outcome) => {
                self.collect_expression_calls(caller, &outcome.subject, source);
                for arm in &outcome.arms {
                    match &arm.body {
                        jadpo_syntax::OutcomeMatchArmBody::Value(value) => {
                            self.collect_expression_calls(caller, value, source)
                        }
                        jadpo_syntax::OutcomeMatchArmBody::Reject(rejection) => {
                            for field in &rejection.values {
                                self.collect_expression_calls(caller, &field.value, source);
                            }
                        }
                        jadpo_syntax::OutcomeMatchArmBody::Propagate(_) => {}
                    }
                }
            }
            Expression::Literal(_) | Expression::Name(_) | Expression::Missing(_) => {}
        }
    }

    fn reference_is_nullable(&self, reference: &TypeReference) -> bool {
        reference.nullable
            || self
                .nullable_types
                .contains(&name_expression(&reference.path))
    }

    fn resolve_nullability(&mut self) {
        // Resolve nullability to a fixed point so declaration order and chains of
        // field refinements do not erase absence. The finite named graph bounds
        // this walk even when another diagnostic rejects a refinement cycle.
        loop {
            let before = self.nullable_types.len();
            for reference in &self.references {
                if self.nullable_types.contains(&reference.target) {
                    if let Some(refined) = &reference.refined {
                        self.nullable_types.insert(refined.clone());
                    }
                }
            }
            if self.nullable_types.len() == before {
                break;
            }
        }
        for (target, source, range) in &self.nullable_references {
            if self.nullable_types.contains(target) {
                self.diagnostics.push(with_span(
                    Diagnostic::error("TYPE_REDUNDANT_NULLABILITY")
                        .with_fact(DiagnosticFact::Name(target.clone())),
                    source,
                    *range,
                ));
            }
        }
    }

    fn validate_storage_nullability(&mut self, files: &[ParsedSyntax]) {
        for file in files {
            for declaration in &file.file.declarations {
                let Declaration::Record(record) = declaration else {
                    continue;
                };
                for field in &record.fields {
                    if field.persistence.contains(&PersistenceModifier::Identity)
                        && self.reference_is_nullable(&field.field_type)
                    {
                        self.diagnostics.push(with_span(
                            Diagnostic::error("DATA_IDENTITY_NULLABLE"),
                            &file.source_name,
                            field.range,
                        ));
                    }
                }
                for constraint in &record.persistence_constraints {
                    let mut seen = BTreeSet::new();
                    for name in &constraint.fields {
                        if !seen.insert(&name.text) {
                            continue;
                        }
                        let Some(field) = record
                            .fields
                            .iter()
                            .find(|field| field.name.text == name.text)
                        else {
                            continue;
                        };
                        if self.reference_is_nullable(&field.field_type) {
                            self.diagnostics.push(with_span(
                                Diagnostic::error("DATA_CONSTRAINT_NULLABLE_FIELD"),
                                &file.source_name,
                                name.range,
                            ));
                        }
                    }
                }
            }
        }
    }

    fn finish(mut self, files: &[ParsedSyntax]) -> SemanticGraph {
        if let Some(principal) = &self.principal {
            for (variant, source, range) in &self.authentication_principals {
                self.references.push(PendingReference {
                    refined: None,
                    target: format!("{principal}.{variant}"),
                    expected: ReferenceKind::PrincipalVariant,
                    usage: ReferenceUsage::AuthenticationPrincipal,
                    source: source.clone(),
                    range: *range,
                });
            }
        }
        let nodes = self
            .nodes
            .values()
            .enumerate()
            .map(|(index, node)| SemanticNode {
                id: NodeId(index as u32),
                kind: node.kind,
                name: node.name.clone(),
                source: node.source.clone(),
                range: node.range,
            })
            .collect::<Vec<_>>();
        let ids = nodes
            .iter()
            .map(|node| (node.name.clone(), node.id))
            .collect::<BTreeMap<_, _>>();
        let kinds = nodes
            .iter()
            .map(|node| (node.name.clone(), node.kind))
            .collect::<BTreeMap<_, _>>();
        let mut refinements = Vec::new();

        for reference in self.references {
            let Some(target) = ids.get(&reference.target).copied() else {
                let mut diagnostic = Diagnostic::error("SEM_UNKNOWN_NAME")
                    .with_fact(DiagnosticFact::Name(reference.target.clone()))
                    .with_fact(DiagnosticFact::Expected(
                        reference.expected.user_name().to_owned(),
                    ))
                    .with_fact(DiagnosticFact::Usage(
                        reference.usage.user_name().to_owned(),
                    ));
                if let Some(examples) = reference.expected.examples() {
                    diagnostic =
                        diagnostic.with_fact(DiagnosticFact::Examples(examples.to_owned()));
                }
                if let Some(suggestion) = closest_semantic_name(
                    &reference.target,
                    nodes
                        .iter()
                        .filter(|node| reference.expected.accepts(node.kind))
                        .map(|node| node.name.as_str()),
                ) {
                    diagnostic = diagnostic.with_fact(DiagnosticFact::SuggestedName(suggestion));
                }
                self.diagnostics
                    .push(with_span(diagnostic, &reference.source, reference.range));
                continue;
            };
            if let Some(diagnostic) = module_visibility_diagnostic(
                &self.module_scopes,
                &nodes[target.0 as usize],
                &reference.target,
                &reference.source,
                reference.range,
            ) {
                self.diagnostics.push(diagnostic);
                continue;
            }
            let target_kind = kinds[&reference.target];
            let kind_matches = match reference.expected {
                ReferenceKind::Type => target_kind.is_type(),
                ReferenceKind::Principal => matches!(target_kind, NodeKind::Principal),
                ReferenceKind::PrincipalVariant => {
                    matches!(target_kind, NodeKind::PrincipalVariant)
                }
                ReferenceKind::AuthenticationField => matches!(target_kind, NodeKind::Field),
                ReferenceKind::ConfigurationField => target_kind == NodeKind::ConfigurationField,
                ReferenceKind::Failure => {
                    matches!(target_kind, NodeKind::Failure | NodeKind::StandardFailure)
                }
                ReferenceKind::StandardFailure => target_kind == NodeKind::StandardFailure,
                ReferenceKind::Fixture => target_kind == NodeKind::Fixture,
            };
            if !kind_matches {
                let mut diagnostic = Diagnostic::error("SEM_WRONG_NAME_KIND")
                    .with_fact(DiagnosticFact::Name(reference.target.clone()))
                    .with_fact(DiagnosticFact::Expected(
                        reference.expected.user_name().to_owned(),
                    ))
                    .with_fact(DiagnosticFact::ActualKind(
                        target_kind.user_name().to_owned(),
                    ))
                    .with_fact(DiagnosticFact::Usage(
                        reference.usage.user_name().to_owned(),
                    ));
                if let Some(examples) = reference.expected.examples() {
                    diagnostic =
                        diagnostic.with_fact(DiagnosticFact::Examples(examples.to_owned()));
                }
                self.diagnostics
                    .push(with_span(diagnostic, &reference.source, reference.range));
                continue;
            }

            if let Some(refined_name) = reference.refined {
                if let Some(refined) = ids.get(&refined_name).copied() {
                    refinements.push(RefinementEdge {
                        refined,
                        parent: target,
                    });
                }
            }
        }

        // Propagate persistence-write effects through action calls before
        // deciding whether an external operation can share their transaction.
        // Also carry an atomic/write context down to nested actions that may
        // contain the actual service call.
        let mut write_effect_callers = self.persistence_write_callers.clone();
        let mut unsafe_effect_contexts = self.atomic_callers.clone();
        unsafe_effect_contexts.extend(write_effect_callers.iter().cloned());
        loop {
            let mut changed = false;
            for call in &self.calls {
                // Authored tests invoke each callable as a separate operation.
                // A write performed by one test call does not make the next
                // service call part of that write's transaction.
                if kinds.get(&call.caller) == Some(&NodeKind::Test) {
                    continue;
                }
                let Some(callee_name) = resolve_callable_name(&call.callee, &nodes, &ids) else {
                    continue;
                };
                if write_effect_callers.contains(&callee_name)
                    && write_effect_callers.insert(call.caller.clone())
                {
                    changed = true;
                }
            }
            let previous_context_count = unsafe_effect_contexts.len();
            unsafe_effect_contexts.extend(write_effect_callers.iter().cloned());
            for call in &self.calls {
                if !unsafe_effect_contexts.contains(&call.caller) {
                    continue;
                }
                let Some(callee_name) = resolve_callable_name(&call.callee, &nodes, &ids) else {
                    continue;
                };
                if kinds.get(&callee_name) == Some(&NodeKind::Action)
                    && unsafe_effect_contexts.insert(callee_name)
                {
                    changed = true;
                }
            }
            changed |= unsafe_effect_contexts.len() != previous_context_count;
            if !changed {
                break;
            }
        }

        let mut calls = Vec::new();
        for call in self.calls {
            let Some(caller) = ids.get(&call.caller).copied() else {
                continue;
            };
            // Scheduled entries resolve an exact static name. A misspelled
            // owner must never bind via ordinary receiver/suffix fallback.
            let resolved_callee = if kinds.get(&call.caller) == Some(&NodeKind::Job) {
                call.callee.clone()
            } else {
                resolve_callable_name(&call.callee, &nodes, &ids)
                    .unwrap_or_else(|| call.callee.clone())
            };
            let Some(callee) = ids.get(&resolved_callee).copied() else {
                if let Some(namespace) = standard_operation_namespace(
                    call.callee.rsplit('.').next().unwrap_or(&call.callee),
                ) {
                    let operation = call.callee.rsplit('.').next().unwrap_or(&call.callee);
                    let diagnostic = Diagnostic::error("SEM_STANDARD_OPERATION_QUALIFICATION")
                        .with_fact(DiagnosticFact::Name(call.callee.clone()))
                        .with_fact(DiagnosticFact::SuggestedName(format!(
                            "{namespace}.{operation}"
                        )));
                    self.diagnostics
                        .push(with_span(diagnostic, &call.source, call.range));
                    continue;
                }
                let mut diagnostic = Diagnostic::error("SEM_UNKNOWN_CALLEE")
                    .with_fact(DiagnosticFact::Name(call.callee.clone()))
                    .with_fact(DiagnosticFact::Expected("function or action".to_owned()))
                    .with_fact(DiagnosticFact::Usage("call".to_owned()));
                if let Some(suggestion) = closest_semantic_name(
                    &call.callee,
                    nodes
                        .iter()
                        .filter(|node| {
                            matches!(
                                node.kind,
                                NodeKind::Function
                                    | NodeKind::Action
                                    | NodeKind::Query
                                    | NodeKind::StandardFunction
                                    | NodeKind::ServiceOperation
                            )
                        })
                        .map(|node| node.name.as_str()),
                ) {
                    diagnostic = diagnostic.with_fact(DiagnosticFact::SuggestedName(suggestion));
                }
                self.diagnostics
                    .push(with_span(diagnostic, &call.source, call.range));
                continue;
            };
            if let Some(diagnostic) = module_visibility_diagnostic(
                &self.module_scopes,
                &nodes[callee.0 as usize],
                &resolved_callee,
                &call.source,
                call.range,
            ) {
                self.diagnostics.push(diagnostic);
                continue;
            }
            match kinds[&resolved_callee] {
                NodeKind::Function
                | NodeKind::Action
                | NodeKind::Query
                | NodeKind::StandardFunction => calls.push(CallEdge { caller, callee }),
                NodeKind::ServiceOperation => {
                    if kinds.get(&call.caller) != Some(&NodeKind::Action) {
                        let diagnostic = Diagnostic::error("SEM_SERVICE_CALL_CONTEXT")
                            .with_fact(DiagnosticFact::Name(call.callee.clone()))
                            .with_fact(DiagnosticFact::Callable(call.caller.clone()));
                        self.diagnostics
                            .push(with_span(diagnostic, &call.source, call.range));
                    } else if unsafe_effect_contexts.contains(&call.caller) {
                        let diagnostic = Diagnostic::error("SEM_SERVICE_ATOMIC_EFFECT")
                            .with_fact(DiagnosticFact::Name(call.callee.clone()))
                            .with_fact(DiagnosticFact::Callable(call.caller.clone()));
                        self.diagnostics
                            .push(with_span(diagnostic, &call.source, call.range));
                    } else {
                        calls.push(CallEdge { caller, callee });
                    }
                }
                kind if kind.is_type() => {}
                actual => {
                    let diagnostic = Diagnostic::error("SEM_NOT_CALLABLE")
                        .with_fact(DiagnosticFact::Name(call.callee.clone()))
                        .with_fact(DiagnosticFact::Expected("function or action".to_owned()))
                        .with_fact(DiagnosticFact::ActualKind(actual.user_name().to_owned()));
                    self.diagnostics
                        .push(with_span(diagnostic, &call.source, call.range));
                }
            }
        }

        refinements.sort_by_key(|edge| (edge.refined, edge.parent));
        refinements.dedup();
        calls.sort_by_key(|edge| (edge.caller, edge.callee));
        calls.dedup();
        Self::validate_test_service_fakes(
            files,
            &nodes,
            &calls,
            &self.external_effects,
            &mut self.diagnostics,
        );
        let mut authentication_resolutions = self
            .authentication_resolutions
            .into_iter()
            .filter_map(|edge| {
                Some(AuthenticationResolutionEdge {
                    resolution: *ids.get(&edge.resolution)?,
                    authority: *ids.get(&edge.authority)?,
                })
            })
            .collect::<Vec<_>>();
        authentication_resolutions.sort_by_key(|edge| (edge.resolution, edge.authority));
        authentication_resolutions.dedup();
        self.external_effects.sort_by(|left, right| {
            (&left.service, &left.operation).cmp(&(&right.service, &right.operation))
        });

        SemanticGraph {
            modules: self.modules,
            nodes,
            refinements,
            nullable_types: self.nullable_types,
            calls,
            external_effects: self.external_effects,
            authentication_resolutions,
            diagnostics: self.diagnostics,
        }
    }

    fn validate_test_service_fakes(
        files: &[ParsedSyntax],
        nodes: &[SemanticNode],
        calls: &[CallEdge],
        external_effects: &[ExternalServiceEffect],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let ids = nodes
            .iter()
            .map(|node| (node.name.as_str(), node.id))
            .collect::<BTreeMap<_, _>>();
        let fixtures = files
            .iter()
            .flat_map(|file| file.file.declarations.iter())
            .filter_map(|declaration| match declaration {
                Declaration::Fixture(fixture) => Some((fixture.name.text.as_str(), fixture)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        for file in files {
            for declaration in &file.file.declarations {
                let Declaration::Test(test) = declaration else {
                    continue;
                };
                let Some(test_id) = ids.get(format!("test:{}", test.name.text).as_str()) else {
                    continue;
                };
                let fake_services = test
                    .fixture
                    .as_ref()
                    .and_then(|name| fixtures.get(name.text.as_str()).copied())
                    .map(|fixture| {
                        fixture
                            .service_fakes
                            .iter()
                            .map(|fake| fake.service.text.as_str())
                            .collect::<BTreeSet<_>>()
                    })
                    .unwrap_or_default();
                let mut visited = BTreeSet::new();
                let mut pending = vec![*test_id];
                let mut missing = BTreeSet::new();
                while let Some(caller) = pending.pop() {
                    if !visited.insert(caller.0) {
                        continue;
                    }
                    for edge in calls.iter().filter(|edge| edge.caller == caller) {
                        let callee = &nodes[edge.callee.0 as usize];
                        if callee.kind == NodeKind::ServiceOperation {
                            if let Some(effect) = external_effects.iter().find(|effect| {
                                format!("{}.{}", effect.service, effect.operation) == callee.name
                            }) {
                                if !fake_services.contains(effect.service.as_str())
                                    && missing.insert(effect.service.clone())
                                {
                                    diagnostics.push(with_span(
                                        Diagnostic::error("SEM_TEST_SERVICE_FAKE_REQUIRED")
                                            .with_fact(DiagnosticFact::Name(
                                                effect.service.clone(),
                                            )),
                                        &file.source_name,
                                        test.range,
                                    ));
                                }
                            }
                        } else {
                            pending.push(edge.callee);
                        }
                    }
                }
            }
        }
    }
}

fn service_type_reference(name: &str, range: TextRange) -> TypeReference {
    TypeReference {
        path: vec![Name {
            text: name.to_owned(),
            range,
        }],
        arguments: Vec::new(),
        nullable: false,
        range,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WireShape {
    value_type: String,
    format: Option<String>,
    nullable: bool,
    min_length: Option<u64>,
    max_length: Option<u64>,
}

impl WireShape {
    fn display(&self) -> String {
        let mut parts = vec![self.value_type.clone()];
        if let Some(format) = &self.format {
            parts.push(format!("format={format}"));
        }
        if self.nullable {
            parts.push("nullable".to_owned());
        }
        if let Some(minimum) = self.min_length {
            parts.push(format!("minLength={minimum}"));
        }
        if let Some(maximum) = self.max_length {
            parts.push(format!("maxLength={maximum}"));
        }
        parts.join(",")
    }
}

#[derive(Clone, Debug)]
struct ImportedMailSchemas {
    input: BTreeMap<String, WireShape>,
    output: BTreeMap<String, WireShape>,
}

fn parse_supported_mail_snapshot(bytes: &[u8]) -> Option<ImportedMailSchemas> {
    let document: JsonValue = serde_json::from_slice(bytes).ok()?;
    if document.get("openapi")?.as_str()? != "3.1.0"
        || document.get("info")?.get("version")?.as_str()? != "0.1.0"
    {
        return None;
    }
    let servers = document.get("servers")?.as_array()?;
    if servers.len() != 1 || servers[0].get("url")?.as_str()? != "https://mail.example.invalid" {
        return None;
    }
    let paths = document.get("paths")?.as_object()?;
    if paths.len() != 1 {
        return None;
    }
    let path = paths.get("/v1/messages")?.as_object()?;
    if path.len() != 1 {
        return None;
    }
    let operation = path.get("post")?;
    if operation.get("operationId")?.as_str()? != "send_overdue_reminder" {
        return None;
    }
    let parameters = operation.get("parameters")?.as_array()?;
    if parameters.len() != 1
        || parameters[0].get("name")?.as_str()? != "Idempotency-Key"
        || parameters[0].get("in")?.as_str()? != "header"
        || parameters[0].get("required")?.as_bool()? != true
        || parse_wire_shape(parameters[0].get("schema")?)?
            != (WireShape {
                value_type: "string".to_owned(),
                format: Some("uuid".to_owned()),
                nullable: false,
                min_length: None,
                max_length: None,
            })
    {
        return None;
    }
    let security = operation.get("security")?.as_array()?;
    if security.len() != 1
        || security[0].as_object()?.len() != 1
        || !security[0]
            .get("providerCredential")?
            .as_array()?
            .is_empty()
    {
        return None;
    }
    let scheme = document
        .get("components")?
        .get("securitySchemes")?
        .get("providerCredential")?;
    if scheme.get("type")?.as_str()? != "http" || scheme.get("scheme")?.as_str()? != "bearer" {
        return None;
    }

    let request = operation.get("requestBody")?;
    if request.get("required")?.as_bool()? != true
        || request.get("content")?.as_object()?.len() != 1
    {
        return None;
    }
    let input = parse_object_schema(
        request
            .get("content")?
            .get("application/json")?
            .get("schema")?,
    )?;

    let responses = operation.get("responses")?.as_object()?;
    let expected_statuses = ["202", "400", "401", "409", "429"]
        .into_iter()
        .collect::<BTreeSet<_>>();
    if responses
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>()
        != expected_statuses
    {
        return None;
    }
    let accepted = responses.get("202")?;
    let response_key = accepted.get("headers")?.get("Idempotency-Key")?;
    if response_key.get("required")?.as_bool()? != true
        || parse_wire_shape(response_key.get("schema")?)?
            != (WireShape {
                value_type: "string".to_owned(),
                format: Some("uuid".to_owned()),
                nullable: false,
                min_length: None,
                max_length: None,
            })
        || accepted.get("content")?.as_object()?.len() != 1
    {
        return None;
    }
    let output = parse_object_schema(
        accepted
            .get("content")?
            .get("application/json")?
            .get("schema")?,
    )?;

    let expected_errors = [
        ("400", "InvalidRecipient"),
        ("401", "Authentication"),
        ("409", "IdempotencyConflict"),
        ("429", "RateLimited"),
    ];
    let component_responses = document.get("components")?.get("responses")?.as_object()?;
    let component_schemas = document.get("components")?.get("schemas")?.as_object()?;
    for (status, name) in expected_errors {
        let reference = format!("#/components/responses/{name}");
        if responses.get(status)?.get("$ref")?.as_str()? != reference {
            return None;
        }
        let response = component_responses.get(name)?;
        let schema_reference = format!("#/components/schemas/{name}");
        if response
            .get("content")?
            .get("application/json")?
            .get("schema")?
            .get("$ref")?
            .as_str()?
            != schema_reference
        {
            return None;
        }
        let schema = component_schemas.get(name)?;
        if schema.get("type")?.as_str()? != "object"
            || schema.get("additionalProperties")?.as_bool()? != false
            || schema.get("required")?.as_array()?.len() != 1
            || schema.get("required")?.as_array()?[0].as_str()? != "code"
            || schema
                .get("properties")?
                .get("code")?
                .get("const")?
                .as_str()?
                != match name {
                    "InvalidRecipient" => "invalid_recipient",
                    "Authentication" => "authentication",
                    "IdempotencyConflict" => "idempotency_conflict",
                    "RateLimited" => "rate_limited",
                    _ => return None,
                }
        {
            return None;
        }
    }
    Some(ImportedMailSchemas { input, output })
}

fn parse_object_schema(schema: &JsonValue) -> Option<BTreeMap<String, WireShape>> {
    let object = schema.as_object()?;
    if object.len() != 4
        || schema.get("type")?.as_str()? != "object"
        || schema.get("additionalProperties")?.as_bool()? != false
    {
        return None;
    }
    let properties = schema.get("properties")?.as_object()?;
    let required = schema
        .get("required")?
        .as_array()?
        .iter()
        .map(JsonValue::as_str)
        .collect::<Option<BTreeSet<_>>>()?;
    if required.len() != schema.get("required")?.as_array()?.len()
        || required != properties.keys().map(String::as_str).collect()
    {
        return None;
    }
    properties
        .iter()
        .map(|(name, shape)| Some((name.clone(), parse_wire_shape(shape)?)))
        .collect()
}

fn parse_wire_shape(schema: &JsonValue) -> Option<WireShape> {
    let object = schema.as_object()?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "type" | "format" | "minLength" | "maxLength"))
    {
        return None;
    }
    let (value_type, nullable) = match schema.get("type")? {
        JsonValue::String(value_type) => (value_type.clone(), false),
        JsonValue::Array(values) if values.len() == 2 => {
            let types = values
                .iter()
                .map(JsonValue::as_str)
                .collect::<Option<Vec<_>>>()?;
            if types.contains(&"null") {
                (
                    types.iter().find(|value| **value != "null")?.to_string(),
                    true,
                )
            } else {
                return None;
            }
        }
        _ => return None,
    };
    let min_length = match schema.get("minLength") {
        Some(value) => Some(value.as_u64()?),
        None => None,
    };
    let max_length = match schema.get("maxLength") {
        Some(value) => Some(value.as_u64()?),
        None => None,
    };
    if (min_length.is_some() || max_length.is_some()) && value_type != "string" {
        return None;
    }
    Some(WireShape {
        value_type,
        format: match schema.get("format") {
            Some(value) => Some(value.as_str()?.to_owned()),
            None => None,
        },
        nullable,
        min_length,
        max_length,
    })
}

fn source_record_parity(
    files: &[ParsedSyntax],
    type_name: &str,
    imported: &BTreeMap<String, WireShape>,
) -> Option<Vec<ServiceFieldParity>> {
    let record = files
        .iter()
        .flat_map(|file| &file.file.declarations)
        .find_map(|declaration| match declaration {
            Declaration::Record(record)
                if record.name.text == type_name && record.kind == RecordKind::Value =>
            {
                Some(record)
            }
            _ => None,
        })?;
    if record.fields.len() != imported.len() {
        return None;
    }
    let mut source_fields = BTreeMap::new();
    for field in &record.fields {
        if source_fields
            .insert(field.name.text.as_str(), field)
            .is_some()
        {
            return None;
        }
    }
    if source_fields.keys().copied().collect::<BTreeSet<_>>()
        != imported.keys().map(String::as_str).collect()
    {
        return None;
    }
    imported
        .iter()
        .map(|(name, imported_shape)| {
            let field = source_fields.get(name.as_str())?;
            if field.optional {
                return None;
            }
            let mut visiting = BTreeSet::new();
            let mut source_shape = resolve_wire_type(&field.field_type, files, &mut visiting, 0)?;
            apply_wire_constraints(&mut source_shape, &field.constraints)?;
            if &source_shape != imported_shape {
                return None;
            }
            Some(ServiceFieldParity {
                field: name.clone(),
                source_type: display_type_reference(&field.field_type),
                source_shape: source_shape.display(),
                imported_shape: imported_shape.display(),
            })
        })
        .collect()
}

fn source_record_field_has_exact_type(
    files: &[ParsedSyntax],
    record_name: &str,
    field_name: &str,
    type_name: &str,
) -> bool {
    let field = files
        .iter()
        .flat_map(|file| &file.file.declarations)
        .find_map(|declaration| match declaration {
            Declaration::Record(record)
                if record.name.text == record_name && record.kind == RecordKind::Value =>
            {
                record
                    .fields
                    .iter()
                    .find(|field| field.name.text == field_name)
            }
            _ => None,
        });
    field.is_some_and(|field| {
        !field.optional
            && !field.field_type.nullable
            && field.field_type.arguments.is_empty()
            && field.field_type.path.len() == 1
            && field.field_type.path[0].text == type_name
    })
}

fn display_type_reference(reference: &TypeReference) -> String {
    let mut name = reference
        .path
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join(".");
    if !reference.arguments.is_empty() {
        name.push('<');
        name.push_str(
            &reference
                .arguments
                .iter()
                .map(display_type_reference)
                .collect::<Vec<_>>()
                .join(", "),
        );
        name.push('>');
    }
    if reference.nullable {
        name.push('?');
    }
    name
}

fn resolve_wire_type(
    reference: &TypeReference,
    files: &[ParsedSyntax],
    visiting: &mut BTreeSet<String>,
    depth: usize,
) -> Option<WireShape> {
    if depth > 16 || !reference.arguments.is_empty() || reference.path.is_empty() {
        return None;
    }
    let mut shape = if reference.path.len() == 1 {
        resolve_named_wire_type(&reference.path[0].text, files, visiting, depth + 1)?
    } else if reference.path.len() == 2 {
        let entity = &reference.path[0].text;
        let field_name = &reference.path[1].text;
        let field = files
            .iter()
            .flat_map(|file| &file.file.declarations)
            .find_map(|declaration| match declaration {
                Declaration::Record(record) if record.name.text == *entity => record
                    .fields
                    .iter()
                    .find(|field| field.name.text == *field_name),
                _ => None,
            })?;
        let mut field_shape = resolve_wire_type(&field.field_type, files, visiting, depth + 1)?;
        apply_wire_constraints(&mut field_shape, &field.constraints)?;
        field_shape
    } else {
        return None;
    };
    shape.nullable |= reference.nullable;
    Some(shape)
}

fn resolve_named_wire_type(
    name: &str,
    files: &[ParsedSyntax],
    visiting: &mut BTreeSet<String>,
    depth: usize,
) -> Option<WireShape> {
    let builtin = match name {
        "Text" => Some(("string", None)),
        "Email" => Some(("string", Some("email"))),
        "Uuid" => Some(("string", Some("uuid"))),
        "Instant" => Some(("string", Some("date-time"))),
        "Int" => Some(("integer", None)),
        "Bool" => Some(("boolean", None)),
        _ => None,
    };
    if let Some((value_type, format)) = builtin {
        return Some(WireShape {
            value_type: value_type.to_owned(),
            format: format.map(str::to_owned),
            nullable: false,
            min_length: None,
            max_length: None,
        });
    }
    if !visiting.insert(name.to_owned()) {
        return None;
    }
    let alias = files
        .iter()
        .flat_map(|file| &file.file.declarations)
        .find_map(|declaration| match declaration {
            Declaration::Type(alias) if alias.name.text == name => Some(alias),
            _ => None,
        });
    let result = alias.and_then(|alias| {
        let mut shape = resolve_wire_type(&alias.parent, files, visiting, depth + 1)?;
        apply_wire_constraints(&mut shape, &alias.constraints)?;
        Some(shape)
    });
    visiting.remove(name);
    result
}

fn apply_wire_constraints(shape: &mut WireShape, constraints: &[Constraint]) -> Option<()> {
    for constraint in constraints {
        let value = unquote_string(&constraint.value.text);
        match constraint.kind {
            ConstraintKind::MinLength => shape.min_length = Some(value.parse().ok()?),
            ConstraintKind::MaxLength => shape.max_length = Some(value.parse().ok()?),
            ConstraintKind::Format => shape.format = Some(value),
            ConstraintKind::Min | ConstraintKind::Max | ConstraintKind::Pattern => return None,
        }
    }
    Some(())
}

fn validate_secret_service_configuration(
    files: &[ParsedSyntax],
    configuration_name: Option<&str>,
) -> Result<(), &'static str> {
    let configuration_name = configuration_name.ok_or("SEM_SERVICE_CONFIGURATION_MISSING")?;
    let configuration = files
        .iter()
        .flat_map(|file| &file.file.declarations)
        .find_map(|declaration| match declaration {
            Declaration::Config(configuration) if configuration.name.text == configuration_name => {
                Some(configuration)
            }
            _ => None,
        })
        .ok_or("SEM_SERVICE_CONFIGURATION_MISSING")?;
    let field = configuration
        .fields
        .iter()
        .find(|field| field.name.text == "mail_api_key")
        .ok_or("SEM_SERVICE_CONFIGURATION_MISSING")?;
    if !field.secret {
        return Err("SEM_SERVICE_SECRET_SINK_INVALID");
    }
    let mut visiting = BTreeSet::new();
    let shape = resolve_wire_type(&field.field_type, files, &mut visiting, 0)
        .ok_or("SEM_SERVICE_SECRET_SINK_INVALID")?;
    if shape.value_type != "string" || shape.nullable {
        return Err("SEM_SERVICE_SECRET_SINK_INVALID");
    }
    Ok(())
}

fn locate_service_import(files: &[ParsedSyntax], source: &str, imported: &str) -> Option<PathBuf> {
    let import = Path::new(imported);
    if import.is_absolute()
        || import
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    let source_path = absolute_source_path(source)?;
    let source_directory = fs::canonicalize(source_path.parent()?).ok()?;
    let root = source_directory
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .map(Path::to_path_buf)
        .or_else(|| common_source_directory(files, &source_directory))?;
    let canonical_root = fs::canonicalize(root).ok()?;
    let candidate = fs::canonicalize(canonical_root.join(import)).ok()?;
    if !candidate.starts_with(&canonical_root) || !candidate.is_file() {
        return None;
    }
    Some(candidate)
}

fn absolute_source_path(source: &str) -> Option<PathBuf> {
    let path = Path::new(source);
    if path.is_absolute() {
        Some(path.to_path_buf())
    } else {
        Some(std::env::current_dir().ok()?.join(path))
    }
}

fn common_source_directory(files: &[ParsedSyntax], initial: &Path) -> Option<PathBuf> {
    let directories = files
        .iter()
        .filter_map(|file| absolute_source_path(&file.source_name))
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .map(|path| fs::canonicalize(path).ok())
        .collect::<Option<Vec<_>>>()?;
    let mut common = initial.to_path_buf();
    while !directories
        .iter()
        .all(|directory| directory.starts_with(&common))
    {
        common = common.parent()?.to_path_buf();
    }
    Some(common)
}

fn checked_service_effect(
    service: &ServiceDeclaration,
    source: &str,
    files: &[ParsedSyntax],
    configuration_name: Option<&str>,
) -> Result<ExternalServiceEffect, &'static str> {
    const IMPORT: &str = "tests/assurance/service-reference-mail-v0.1.json";
    const VERSION: &str = "0.1.0";
    const SHA256: &str = "c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d";
    const EXPECTED: &[&str] = &[
        "import.file: \"tests/assurance/service-reference-mail-v0.1.json\"",
        "import.version: \"0.1.0\"",
        "import.sha256: \"c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d\"",
        "credential.scheme: bearer",
        "credential.header: \"Authorization\"",
        "credential.secret: config.mail_api_key",
        "base_url: \"https://mail.example.invalid\"",
        "egress: mail.example.invalid:443",
        "proxy: prohibited",
        "redirects: prohibited",
        "timeout: 5s",
        "retry: exponential(max_attempts: 3, max_elapsed: 30s, jitter: full)",
        "operation: send_overdue_reminder",
        "POST: /v1/messages",
        "idempotency_key: ReminderIntentId",
        "input: ReminderMessage",
        "output: ReminderReceipt",
        "request_key: input.idempotency_key equals header \"Idempotency-Key\"",
        "receipt_key: response header \"Idempotency-Key\" equals request_key",
        "maps: provider.invalid_recipient -> ReminderRecipientRejected",
        "maps: provider.rate_limited -> ReminderTemporarilyUnavailable",
        "maps: provider.authentication -> fault Misconfigured",
        "maps: provider.idempotency_conflict -> fault Misconfigured",
        "maps: transport.pre_dispatch_refusal -> fault Unavailable",
        "maps: transport.pre_dispatch_timeout -> fault Unavailable",
        "maps: transport.possible_dispatch_timeout -> fault OutcomeUnknown",
        "maps: transport.possible_dispatch_loss -> fault OutcomeUnknown",
        "maps: transport.invalid_acknowledgement -> fault OutcomeUnknown",
        "maps: transport.unexpected_response -> fault OutcomeUnknown",
    ];

    let lines = service
        .items
        .iter()
        .map(|item| {
            let prefix = if item.path.is_empty() {
                String::new()
            } else {
                format!("{}.", item.path.join("."))
            };
            format!("{prefix}{}: {}", item.key, item.value)
        })
        .collect::<Vec<_>>();
    let contains = |expected: &str| lines.iter().any(|line| line == expected);
    if service.name.text != "ReminderMail"
        || !lines
            .iter()
            .map(String::as_str)
            .eq(EXPECTED.iter().copied())
    {
        let code = if EXPECTED[..3].iter().any(|line| !contains(line)) {
            "SEM_SERVICE_IMPORT_PIN_INVALID"
        } else if lines
            .iter()
            .any(|line| line.starts_with("base_url:") || line.starts_with("egress:"))
            && (!contains(EXPECTED[6]) || !contains(EXPECTED[7]))
        {
            "SEM_SERVICE_EGRESS_INVALID"
        } else if EXPECTED[3..6].iter().any(|line| !contains(line)) {
            "SEM_SERVICE_SECRET_SINK_INVALID"
        } else if !contains(EXPECTED[14]) {
            "SEM_SERVICE_IDEMPOTENCY_INVALID"
        } else {
            "SEM_SERVICE_CONTRACT_INVALID"
        };
        return Err(code);
    }

    let imported_path =
        locate_service_import(files, source, IMPORT).ok_or("SEM_SERVICE_IMPORT_NOT_FOUND")?;
    let bytes = fs::read(imported_path).map_err(|_| "SEM_SERVICE_IMPORT_NOT_FOUND")?;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != SHA256 {
        return Err("SEM_SERVICE_IMPORT_DIGEST_MISMATCH");
    }
    validate_secret_service_configuration(files, configuration_name)?;
    let imported = parse_supported_mail_snapshot(&bytes).ok_or("SEM_SERVICE_CONTRACT_INVALID")?;
    if !source_record_field_has_exact_type(
        files,
        "ReminderMessage",
        "idempotency_key",
        "ReminderIntentId",
    ) {
        return Err("SEM_SERVICE_CONTRACT_INVALID");
    }
    let input_schema = source_record_parity(files, "ReminderMessage", &imported.input)
        .ok_or("SEM_SERVICE_CONTRACT_INVALID")?;
    let output_schema = source_record_parity(files, "ReminderReceipt", &imported.output)
        .ok_or("SEM_SERVICE_CONTRACT_INVALID")?;

    Ok(ExternalServiceEffect {
        service: service.name.text.clone(),
        operation: "send_overdue_reminder".to_owned(),
        method: "POST".to_owned(),
        path: "/v1/messages".to_owned(),
        input: "ReminderMessage".to_owned(),
        output: "ReminderReceipt".to_owned(),
        idempotency_type: "ReminderIntentId".to_owned(),
        egress: "mail.example.invalid:443".to_owned(),
        credential_slot: "config.mail_api_key".to_owned(),
        credential_header: "Authorization".to_owned(),
        imported_contract: IMPORT.to_owned(),
        import_version: VERSION.to_owned(),
        import_sha256: SHA256.to_owned(),
        timeout_ms: 5_000,
        max_attempts: 3,
        max_elapsed_ms: 30_000,
        jitter: "full".to_owned(),
        redirects_allowed: false,
        proxy_allowed: false,
        outcomes: vec![
            "ReminderRecipientRejected".to_owned(),
            "ReminderTemporarilyUnavailable".to_owned(),
            "Misconfigured".to_owned(),
            "Unavailable".to_owned(),
            "OutcomeUnknown".to_owned(),
        ],
        input_schema,
        output_schema,
        outcome_mappings: vec![
            (
                "provider.invalid_recipient".to_owned(),
                "ReminderRecipientRejected".to_owned(),
            ),
            (
                "provider.rate_limited".to_owned(),
                "ReminderTemporarilyUnavailable".to_owned(),
            ),
            (
                "provider.authentication".to_owned(),
                "Misconfigured".to_owned(),
            ),
            (
                "provider.idempotency_conflict".to_owned(),
                "Misconfigured".to_owned(),
            ),
            (
                "transport.pre_dispatch_refusal".to_owned(),
                "Unavailable".to_owned(),
            ),
            (
                "transport.pre_dispatch_timeout".to_owned(),
                "Unavailable".to_owned(),
            ),
            (
                "transport.possible_dispatch_timeout".to_owned(),
                "OutcomeUnknown".to_owned(),
            ),
            (
                "transport.possible_dispatch_loss".to_owned(),
                "OutcomeUnknown".to_owned(),
            ),
            (
                "transport.invalid_acknowledgement".to_owned(),
                "OutcomeUnknown".to_owned(),
            ),
            (
                "transport.unexpected_response".to_owned(),
                "OutcomeUnknown".to_owned(),
            ),
        ],
        source: source.to_owned(),
        range: service.range,
    })
}

fn resolve_callable_name(
    callee: &str,
    nodes: &[SemanticNode],
    ids: &BTreeMap<String, NodeId>,
) -> Option<String> {
    if ids.contains_key(callee) {
        return Some(callee.to_owned());
    }
    let operation = callee.rsplit('.').next()?;
    let mut candidates = nodes
        .iter()
        .filter(|node| {
            matches!(
                node.kind,
                NodeKind::Function | NodeKind::Action | NodeKind::Query
            ) && node.name.ends_with(&format!(".{operation}"))
        })
        .map(|node| node.name.clone());
    match (candidates.next(), candidates.next()) {
        (Some(candidate), None) => Some(candidate),
        _ => None,
    }
}

fn module_visibility_diagnostic(
    scopes: &BTreeMap<String, ModuleScope>,
    target_node: &SemanticNode,
    target_name: &str,
    source: &str,
    range: TextRange,
) -> Option<Diagnostic> {
    let scope = scopes.get(source)?;
    if target_node.source == source || target_node.source == "<prelude>" {
        return None;
    }
    let root_name = target_name.split('.').next().unwrap_or(target_name);
    if scope.imported_names.contains(root_name) {
        return None;
    }
    Some(with_span(
        Diagnostic::error("MOD_IMPORT_REQUIRED"),
        source,
        range,
    ))
}

#[derive(Clone, Copy)]
enum NameCase {
    UpperCamel,
    LowerSnake,
}

impl NameCase {
    fn accepts(self, value: &str) -> bool {
        match self {
            Self::UpperCamel => {
                value
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_uppercase())
                    && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
            }
            Self::LowerSnake => {
                value
                    .bytes()
                    .next()
                    .is_some_and(|byte| byte.is_ascii_lowercase())
                    && value.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'
                    })
                    && !value.ends_with('_')
                    && !value.contains("__")
            }
        }
    }

    const fn description(self) -> &'static str {
        match self {
            Self::UpperCamel => "UpperCamelCase",
            Self::LowerSnake => "lower_snake_case",
        }
    }

    fn convert(self, value: &str) -> String {
        let snake = to_lower_snake(value);
        match self {
            Self::LowerSnake => snake,
            Self::UpperCamel => snake
                .split('_')
                .filter(|part| !part.is_empty())
                .map(|part| {
                    let mut characters = part.chars();
                    characters.next().map_or_else(String::new, |first| {
                        first.to_uppercase().chain(characters).collect()
                    })
                })
                .collect(),
        }
    }
}

fn to_lower_snake(value: &str) -> String {
    let mut output = String::new();
    let mut previous_was_separator = true;
    for character in value.chars() {
        if character == '_' || character == '-' {
            if !previous_was_separator && !output.is_empty() {
                output.push('_');
            }
            previous_was_separator = true;
            continue;
        }
        if character.is_ascii_uppercase() {
            if !previous_was_separator && !output.is_empty() {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        } else if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
        }
        previous_was_separator = false;
    }
    while output.ends_with('_') {
        output.pop();
    }
    output
}

fn is_standard_namespace(value: &str) -> bool {
    matches!(value, "temporal" | "collection")
}

fn standard_operation_namespace(value: &str) -> Option<&'static str> {
    matches!(
        value,
        "in_zone"
            | "resolve"
            | "add_elapsed"
            | "between"
            | "add_days"
            | "add_weeks"
            | "add_months"
            | "add_years"
            | "add_local_days"
            | "add_local_weeks"
            | "add_local_months"
            | "add_local_years"
            | "day_bounds"
            | "week_bounds"
            | "month_bounds"
            | "year_bounds"
            | "calendar_date"
            | "year"
            | "month"
            | "day"
            | "weekday"
            | "hour"
            | "minute"
            | "second"
            | "millisecond"
            | "offset"
            | "zone"
            | "same_zone"
            | "same_local"
            | "format"
            | "format_friendly"
    )
    .then_some("temporal")
}

fn declaration_name(declaration: &Declaration) -> Option<&jadpo_syntax::Name> {
    match declaration {
        Declaration::Application(declaration) => Some(&declaration.name),
        Declaration::Locales(_) => None,
        Declaration::AuthenticationStrategy(declaration) => Some(&declaration.name),
        Declaration::Principal(declaration) => Some(&declaration.name),
        Declaration::Config(declaration) => Some(&declaration.name),
        Declaration::Type(declaration) => Some(&declaration.name),
        Declaration::Enum(declaration) => Some(&declaration.name),
        Declaration::Record(declaration) => Some(&declaration.name),
        Declaration::Failure(declaration) => Some(&declaration.name),
        Declaration::Callable(declaration) => Some(&declaration.name),
        Declaration::Fixture(declaration) => Some(&declaration.name),
        Declaration::Test(_) => None,
        Declaration::Route(_) => None,
        Declaration::Job(declaration) => Some(&declaration.name),
    }
}

fn positive_duration(value: &str) -> bool {
    value
        .trim_end_matches(|character: char| character.is_ascii_alphabetic())
        .parse::<f64>()
        .is_ok_and(|value| value.is_finite() && value > 0.0)
}

fn unquote_string(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
        .to_owned()
}

fn valid_configuration_binding(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn name_expression(path: &[jadpo_syntax::Name]) -> String {
    path.iter()
        .map(|name| name.text.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

fn standard_variant_name(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !result.is_empty() {
                result.push('_');
            }
            result.push(character.to_ascii_lowercase());
            separator = false;
        } else {
            separator = true;
        }
    }
    result
}

fn closest_semantic_name<'a>(
    requested: &str,
    candidates: impl Iterator<Item = &'a str>,
) -> Option<String> {
    let requested_lower = requested.to_ascii_lowercase();
    candidates
        .filter(|candidate| *candidate != requested)
        .filter_map(|candidate| {
            let distance = edit_distance(&requested_lower, &candidate.to_ascii_lowercase());
            (distance <= 3).then_some((distance, candidate))
        })
        .min_by(|left, right| left.cmp(right))
        .map(|(_, candidate)| candidate.to_owned())
}

fn edit_distance(left: &str, right: &str) -> usize {
    let mut previous = (0..=right.chars().count()).collect::<Vec<_>>();
    for (left_index, left_character) in left.chars().enumerate() {
        let mut current = vec![left_index + 1];
        for (right_index, right_character) in right.chars().enumerate() {
            current.push(
                (previous[right_index + 1] + 1).min(
                    (current[right_index] + 1).min(
                        previous[right_index] + usize::from(left_character != right_character),
                    ),
                ),
            );
        }
        previous = current;
    }
    previous.last().copied().unwrap_or_default()
}

fn http_method_name(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Delete => "DELETE",
    }
}

fn with_span(mut diagnostic: Diagnostic, source: &str, range: TextRange) -> Diagnostic {
    diagnostic.primary = Some(SourceSpan {
        source: source.to_owned(),
        start: range.start,
        end: range.end,
    });
    diagnostic
}

fn escape_json(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '\"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect()
}

fn json_strings(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| format!("\"{}\"", escape_json(value)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_semantic_graph, check_failures, check_types, parse_supported_mail_snapshot, NameCase,
        NodeKind, ScaffoldManifest,
    };
    use jadpo_syntax::parse;
    use serde_json::Value as JsonValue;
    use std::fs;
    use std::path::Path;

    fn repository_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("semantic crate should be inside the repository")
    }

    fn service_source(replacement: Option<(&str, &str)>) -> (String, String) {
        let candidate_path = repository_root().join("tests/assurance/service-successor-v0.1.jadpo");
        let candidate = fs::read_to_string(&candidate_path)
            .expect("successor service candidate should be readable");
        let parsed_candidate = parse(&candidate_path, &candidate);
        let candidate_service = parsed_candidate
            .file
            .services
            .first()
            .expect("successor should declare a service");
        let mut service =
            candidate[candidate_service.range.start..candidate_service.range.end].to_owned();
        if let Some((from, to)) = replacement {
            service = service.replace(from, to);
        }
        let source = format!(
            r#"config TestConfiguration {{
    mail_api_key: Text {{ binding: "MAIL_API_KEY" secret: true }}
}}

type ReminderIntentId = Uuid
type TodoTitle = Text {{
    min_length: 1
    max_length: 200
}}
value ReminderMessage {{
    idempotency_key: ReminderIntentId
    from: Email
    to: Email
    todo_title: TodoTitle
    due_at: Instant?
}}
value ReminderReceipt {{
    accepted_at: Instant
}}

failure ReminderRecipientRejected {{
    kind: Rejected
    code: "recipient_rejected"
    message: "Recipient rejected."
}}

failure ReminderTemporarilyUnavailable {{
    kind: Unavailable
    code: "temporarily_unavailable"
    message: "Temporarily unavailable."
}}

{service}

action deliver(mail: ReminderMessage)
    fails ReminderRecipientRejected, ReminderTemporarilyUnavailable, Misconfigured, Unavailable, OutcomeUnknown
    -> ReminderReceipt
{{
    return attempt ReminderMail.send_overdue_reminder(mail)
}}
"#
        );
        (source, candidate_path.to_string_lossy().into_owned())
    }

    #[test]
    fn manifest_json_escapes_source_paths() {
        let manifest = ScaffoldManifest {
            schema_version: 1,
            phase: "scaffold",
            source_files: vec!["a/quoted\"file.jadpo".to_owned()],
        };
        assert_eq!(
            manifest.to_json(),
            "{\"schema_version\":1,\"phase\":\"scaffold\",\"source_files\":[\"a/quoted\\\"file.jadpo\"]}"
        );
    }

    #[test]
    fn builds_deterministic_field_and_refinement_nodes() {
        let source = r#"
entity Customer { email: Email }
"#;
        let parsed = parse(Path::new("app.jadpo"), source);
        let graph = build_semantic_graph(std::slice::from_ref(&parsed));

        assert!(graph.diagnostics.is_empty(), "{:#?}", graph.diagnostics);
        assert_eq!(graph.node("Email").unwrap().kind, NodeKind::Type);
        assert_eq!(graph.node("Customer.email").unwrap().kind, NodeKind::Field);
        let edges = graph
            .refinements
            .iter()
            .map(|edge| {
                (
                    graph.nodes[edge.refined.0 as usize].name.as_str(),
                    graph.nodes[edge.parent.0 as usize].name.as_str(),
                )
            })
            .collect::<Vec<_>>();
        assert!(edges.contains(&("Email", "Text")));
        assert!(edges.contains(&("Customer.email", "Email")));
    }

    #[test]
    fn adds_checked_service_operation_to_effect_graph() {
        let (source, name) = service_source(None);
        let parsed = parse(Path::new(&name), &source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

        let graph = build_semantic_graph(std::slice::from_ref(&parsed));
        assert!(graph.diagnostics.is_empty(), "{:#?}", graph.diagnostics);
        assert_eq!(graph.external_effects.len(), 1);
        let effect = &graph.external_effects[0];
        assert_eq!(effect.service, "ReminderMail");
        assert_eq!(effect.operation, "send_overdue_reminder");
        assert_eq!(effect.method, "POST");
        assert_eq!(effect.path, "/v1/messages");
        assert_eq!(effect.egress, "mail.example.invalid:443");
        assert_eq!(effect.credential_slot, "config.mail_api_key");
        assert_eq!(effect.outcomes.len(), 5);
        assert_eq!(effect.input_schema.len(), 5);
        assert_eq!(effect.output_schema.len(), 1);
        let title = effect
            .input_schema
            .iter()
            .find(|field| field.field == "todo_title")
            .expect("title field should be in the pinned request schema");
        assert_eq!(title.source_type, "TodoTitle");
        assert_eq!(title.source_shape, "string,minLength=1,maxLength=200");
        assert_eq!(title.imported_shape, title.source_shape);
        assert!(effect.schema_parity_json().contains("\"todo_title\""));
        assert!(graph.calls.iter().any(|edge| {
            graph.nodes[edge.caller.0 as usize].name == "deliver"
                && graph.nodes[edge.callee.0 as usize].name == "ReminderMail.send_overdue_reminder"
        }));
        let typing = check_types(std::slice::from_ref(&parsed), &graph);
        assert!(typing.diagnostics.is_empty(), "{:#?}", typing.diagnostics);
        let failures = check_failures(std::slice::from_ref(&parsed), &graph);
        assert!(
            failures.diagnostics.is_empty(),
            "{:#?}",
            failures.diagnostics
        );
    }

    #[test]
    fn checks_authored_service_fake_operation_outcome_and_response_contracts() {
        fn diagnostics(fake: &str) -> Vec<String> {
            let (mut source, name) = service_source(None);
            source.push_str(fake);
            let parsed = parse(Path::new(&name), &source);
            assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
            let files = [parsed];
            let graph = build_semantic_graph(&files);
            let mut codes = graph
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code.to_owned())
                .collect::<Vec<_>>();
            codes.extend(
                check_types(&files, &graph)
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.code.to_owned()),
            );
            codes
        }

        let valid = diagnostics(
            r#"
fixture accepted {
    config { mail_api_key: secret("fixture-key") }
    service ReminderMail: fake {
        send_overdue_reminder => accept ReminderReceipt {
            accepted_at: Instant("2026-01-15T12:00:00Z")
        }
    }
}
"#,
        );
        assert!(valid.is_empty(), "{valid:?}");

        let unknown_operation = diagnostics(
            r#"
fixture bad_operation {
    service ReminderMail: fake { send_message => transport.pre_dispatch_refusal }
}
"#,
        );
        assert!(unknown_operation.contains(&"SEM_UNKNOWN_NAME".to_owned()));

        let undeclared_outcome = diagnostics(
            r#"
fixture bad_outcome {
    service ReminderMail: fake { send_overdue_reminder => provider.unlisted }
}
"#,
        );
        assert!(undeclared_outcome.contains(&"SEM_UNKNOWN_NAME".to_owned()));

        let wrong_response = diagnostics(
            r#"
fixture bad_response {
    service ReminderMail: fake { send_overdue_reminder => accept ReminderMessage {} }
}
"#,
        );
        assert!(wrong_response.contains(&"TYPE_MISMATCH".to_owned()));

        let secret_response = diagnostics(
            r#"
fixture secret_response {
    config { mail_api_key: secret("fixture-key") }
    service ReminderMail: fake {
        send_overdue_reminder => accept ReminderReceipt { accepted_at: config.mail_api_key }
    }
}
"#,
        );
        assert!(
            secret_response.contains(&"SEM_UNKNOWN_NAME".to_owned())
                || secret_response.contains(&"TYPE_MISMATCH".to_owned())
                || secret_response.contains(&"TYPE_UNKNOWN_VALUE".to_owned()),
            "{secret_response:?}"
        );
    }

    #[test]
    fn requires_service_fake_for_service_operation_reachable_from_authored_test() {
        let (mut source, name) = service_source(None);
        source.push_str(
            r#"
test "service calls require a fake" {
    var result = call deliver(none)
}
"#,
        );
        let parsed = parse(Path::new(&name), &source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let graph = build_semantic_graph(&[parsed]);
        assert!(
            graph
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "SEM_TEST_SERVICE_FAKE_REQUIRED" }),
            "{:#?}",
            graph.diagnostics
        );
    }

    #[test]
    fn rejects_dynamic_service_egress_with_a_stable_diagnostic() {
        let (source, name) = service_source(Some((
            "egress: mail.example.invalid:443",
            "egress: runtime.mail_host",
        )));
        let parsed = parse(Path::new(&name), &source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

        let graph = build_semantic_graph(&[parsed]);
        assert!(graph
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "SEM_SERVICE_EGRESS_INVALID"));
        assert!(graph.external_effects.is_empty());
    }

    #[test]
    fn rejects_service_contract_mutations_by_security_invariant() {
        for (from, to, expected) in [
            (
                "sha256: \"c6a8b26a4b7ec82414df2e6608cb5eb6ca65f1767f7047bfbfedfc8f9441f83d\"",
                "sha256: \"unreviewed\"",
                "SEM_SERVICE_IMPORT_PIN_INVALID",
            ),
            (
                "secret: config.mail_api_key",
                "secret: config.untrusted_value",
                "SEM_SERVICE_SECRET_SINK_INVALID",
            ),
            (
                "idempotency_key: ReminderIntentId",
                "idempotency_key: Uuid",
                "SEM_SERVICE_IDEMPOTENCY_INVALID",
            ),
            (
                "POST /v1/messages",
                "POST /v1/other",
                "SEM_SERVICE_CONTRACT_INVALID",
            ),
        ] {
            let (source, name) = service_source(Some((from, to)));
            let parsed = parse(Path::new(&name), &source);
            assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
            let graph = build_semantic_graph(&[parsed]);
            assert!(
                graph
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == expected),
                "{expected}: {:#?}",
                graph.diagnostics
            );
            assert!(graph.external_effects.is_empty());
        }
    }

    #[test]
    fn rejects_source_request_and_receipt_schema_drift() {
        for (from, to) in [
            ("todo_title: TodoTitle", "todo_title: Text"),
            ("    due_at: Instant?\n", ""),
            ("accepted_at: Instant", "accepted_at: Text"),
        ] {
            let (source, name) = service_source(None);
            let source = source.replace(from, to);
            let parsed = parse(Path::new(&name), &source);
            assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
            let graph = build_semantic_graph(&[parsed]);
            assert!(
                graph
                    .diagnostics
                    .iter()
                    .any(|diagnostic| { diagnostic.code == "SEM_SERVICE_CONTRACT_INVALID" }),
                "{:#?}",
                graph.diagnostics
            );
            assert!(graph.external_effects.is_empty());
        }
    }

    #[test]
    fn rejects_wire_compatible_but_non_nominal_idempotency_identity() {
        let (source, name) = service_source(None);
        let source = source.replacen(
            "idempotency_key: ReminderIntentId",
            "idempotency_key: Uuid",
            1,
        );
        let parsed = parse(Path::new(&name), &source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let graph = build_semantic_graph(&[parsed]);
        assert!(
            graph
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "SEM_SERVICE_CONTRACT_INVALID"),
            "{:#?}",
            graph.diagnostics
        );
        assert!(graph.external_effects.is_empty());
    }

    #[test]
    fn rejects_a_non_secret_mail_credential_configuration_field() {
        let (source, name) = service_source(None);
        let source = source.replace("secret: true", "secret: false");
        let parsed = parse(Path::new(&name), &source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let graph = build_semantic_graph(&[parsed]);
        assert!(
            graph
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "SEM_SERVICE_SECRET_SINK_INVALID" }),
            "{:#?}",
            graph.diagnostics
        );
        assert!(graph.external_effects.is_empty());
    }

    #[test]
    fn parses_the_pinned_mail_snapshot_and_rejects_unsupported_schema_keywords() {
        let path = repository_root().join("tests/assurance/service-reference-mail-v0.1.json");
        let bytes = fs::read(path).expect("pinned mail snapshot should be readable");
        assert!(parse_supported_mail_snapshot(&bytes).is_some());

        let mut document: JsonValue = serde_json::from_slice(&bytes).unwrap();
        document["paths"]["/v1/messages"]["post"]["requestBody"]["content"]["application/json"]
            ["schema"]["properties"]["todo_title"]["pattern"] = JsonValue::String(".*".to_owned());
        let changed = serde_json::to_vec(&document).unwrap();
        assert!(parse_supported_mail_snapshot(&changed).is_none());
    }

    #[test]
    fn rejects_changed_imported_snapshot_bytes() {
        let (source, _) = service_source(None);
        let root = std::env::temp_dir().join(format!(
            "jadpo-service-import-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be positive")
                .as_nanos()
        ));
        let import = root.join("tests/assurance/service-reference-mail-v0.1.json");
        fs::create_dir_all(import.parent().expect("import should have a parent"))
            .expect("temporary service tree should be created");
        fs::write(&import, b"changed contract bytes").expect("changed snapshot should be written");
        let source_path = root.join("app.jadpo");
        let parsed = parse(&source_path, &source);
        let graph = build_semantic_graph(&[parsed]);
        assert!(graph
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "SEM_SERVICE_IMPORT_DIGEST_MISMATCH" }));
        assert!(graph.external_effects.is_empty());
        fs::remove_dir_all(root).expect("temporary service tree should be removed");
    }

    #[test]
    fn reports_missing_service_snapshot_without_falling_back_to_the_compiler_checkout() {
        let (source, _) = service_source(None);
        let root = std::env::temp_dir().join(format!(
            "jadpo-service-missing-import-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be positive")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("temporary service project should be created");
        let parsed = parse(&root.join("app.jadpo"), &source);
        let graph = build_semantic_graph(&[parsed]);
        assert!(graph
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "SEM_SERVICE_IMPORT_NOT_FOUND" }));
        assert!(graph.external_effects.is_empty());
        fs::remove_dir_all(root).expect("temporary service project should be removed");
    }

    #[test]
    fn does_not_resolve_service_imports_above_the_project_root() {
        let (source, _) = service_source(None);
        let root = std::env::temp_dir().join(format!(
            "jadpo-service-import-boundary-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be positive")
                .as_nanos()
        ));
        let project = root.join("project");
        let outside_import = root.join("tests/assurance/service-reference-mail-v0.1.json");
        fs::create_dir_all(project.join(".git")).expect("project marker should be created");
        fs::create_dir_all(outside_import.parent().unwrap())
            .expect("outside import directory should be created");
        let repository_snapshot =
            repository_root().join("tests/assurance/service-reference-mail-v0.1.json");
        fs::copy(repository_snapshot, &outside_import).expect("pinned snapshot should be copied");

        let parsed = parse(&project.join("app.jadpo"), &source);
        let graph = build_semantic_graph(&[parsed]);
        assert!(
            graph
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "SEM_SERVICE_IMPORT_NOT_FOUND" }),
            "{:#?}",
            graph.diagnostics
        );
        assert!(graph.external_effects.is_empty());
        fs::remove_dir_all(root).expect("temporary project tree should be removed");
    }

    #[test]
    fn reports_a_missing_configuration_for_a_checked_service_credential() {
        let (source, name) = service_source(None);
        let source = source.replace(
            "config TestConfiguration {\n    mail_api_key: Text { binding: \"MAIL_API_KEY\" secret: true }\n}\n\n",
            "",
        );
        let parsed = parse(Path::new(&name), &source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let graph = build_semantic_graph(&[parsed]);
        assert!(graph
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.code == "SEM_SERVICE_CONFIGURATION_MISSING" }));
    }

    #[test]
    fn rejects_service_call_from_a_pure_function() {
        let (source, name) = service_source(None);
        let source = source
            .replace("action deliver(mail: ReminderMessage)", "function deliver(mail: ReminderMessage)")
            .replace(
                "    fails ReminderRecipientRejected, ReminderTemporarilyUnavailable, Misconfigured, Unavailable, OutcomeUnknown\n",
                "",
            );
        let parsed = parse(Path::new(&name), &source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

        let graph = build_semantic_graph(&[parsed]);
        assert!(graph
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "SEM_SERVICE_CALL_CONTEXT"));
        assert!(!graph.calls.iter().any(|edge| {
            graph.nodes[edge.callee.0 as usize].kind == NodeKind::ServiceOperation
        }));
    }

    #[test]
    fn rejects_service_call_from_an_atomic_action() {
        let (source, name) = service_source(None);
        let source = source.replace(
            "action deliver(mail: ReminderMessage)\n",
            "action deliver(mail: ReminderMessage)\n    consistency: atomic\n",
        );
        let parsed = parse(Path::new(&name), &source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

        let graph = build_semantic_graph(&[parsed]);
        assert!(graph
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "SEM_SERVICE_ATOMIC_EFFECT"));
        assert!(!graph.calls.iter().any(|edge| {
            graph.nodes[edge.callee.0 as usize].kind == NodeKind::ServiceOperation
        }));
    }

    #[test]
    fn rejects_service_dispatch_sharing_an_action_with_persistence_writes() {
        let (source, name) = service_source(None);
        let source = source
            .replace(
                "type ReminderIntentId = Uuid",
                "entity Payment { amount: Int }\n\ntype ReminderIntentId = Uuid",
            )
            .replace(
                "    return attempt ReminderMail.send_overdue_reminder(mail)",
                "    var receipt = attempt ReminderMail.send_overdue_reminder(mail)\n    var payment = attempt create Payment { amount: 1 }\n    return receipt",
            );
        let parsed = parse(Path::new(&name), &source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

        let graph = build_semantic_graph(&[parsed]);
        assert!(graph
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "SEM_SERVICE_ATOMIC_EFFECT"));
        assert!(!graph.calls.iter().any(|edge| {
            graph.nodes[edge.callee.0 as usize].kind == NodeKind::ServiceOperation
        }));
    }

    #[test]
    fn rejects_service_effects_reached_through_atomic_or_writing_actions() {
        let (base, name) = service_source(None);
        let delegated = base.replace(
            "    return attempt ReminderMail.send_overdue_reminder(mail)",
            "    return attempt send_mail(mail)",
        );
        let with_service_helper = delegated.replace(
            "action deliver(mail: ReminderMessage)",
            "action send_mail(mail: ReminderMessage)\n    fails ReminderRecipientRejected, ReminderTemporarilyUnavailable, Misconfigured, Unavailable, OutcomeUnknown\n    -> ReminderReceipt\n{\n    return attempt ReminderMail.send_overdue_reminder(mail)\n}\n\naction deliver(mail: ReminderMessage)",
        );
        let atomic_source = with_service_helper.replace(
            "action deliver(mail: ReminderMessage)\n",
            "action deliver(mail: ReminderMessage)\n    consistency: atomic\n",
        );
        let writing_source = with_service_helper
            .replace(
                "type ReminderIntentId = Uuid",
                "entity Payment { amount: Int }\n\ntype ReminderIntentId = Uuid",
            )
            .replace(
                "    return attempt send_mail(mail)",
                "    var payment = attempt create Payment { amount: 1 }\n    return attempt send_mail(mail)",
            );

        for source in [atomic_source, writing_source] {
            let parsed = parse(Path::new(&name), &source);
            assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
            let graph = build_semantic_graph(&[parsed]);
            assert!(graph
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "SEM_SERVICE_ATOMIC_EFFECT"));
            assert!(!graph.calls.iter().any(|edge| {
                graph.nodes[edge.callee.0 as usize].kind == NodeKind::ServiceOperation
            }));
        }
    }

    #[test]
    fn indexes_entity_lifecycle_contract_components() {
        let source = r#"
enum UserStatus { active disabled }
enum UserRole { self }
entity User {
    id: Uuid identity
    status: UserStatus
    deleted_at: Instant?
    updated_at: Instant generated { on: create_or_change }
    identity: id
    persistence { store: primary role: authority }
    policy {
        UserRole.self: [read]
        operations {
            disable_user { UserRole.self: [update] }
        }
    }
    lifecycle {
        initial: { status: User.status(UserStatus.active) deleted_at: none }
        visible when status == UserStatus.active
        transition disable {
            from: status == UserStatus.active
            set: { status: User.status(UserStatus.disabled) }
        }
        purge after config.soft_delete_retention from deleted_at
    }
}
"#;
        let parsed = parse(Path::new("lifecycle.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let graph = build_semantic_graph(&[parsed.clone()]);
        assert!(graph.diagnostics.is_empty(), "{:#?}", graph.diagnostics);
        assert_eq!(
            graph.node("User.lifecycle").unwrap().kind,
            NodeKind::EntityLifecycle
        );
        assert_eq!(
            graph.node("User.lifecycle.initial").unwrap().kind,
            NodeKind::LifecycleInitial
        );
        assert_eq!(
            graph.node("User.lifecycle.visibility").unwrap().kind,
            NodeKind::LifecycleVisibility
        );
        assert_eq!(
            graph
                .node("User.lifecycle.transition.disable")
                .unwrap()
                .kind,
            NodeKind::LifecycleTransition
        );
        assert_eq!(
            graph.node("User.lifecycle.purge").unwrap().kind,
            NodeKind::LifecyclePurge
        );
        assert_eq!(graph.node("User.policy").unwrap().kind, NodeKind::Policy);
        assert_eq!(
            graph.node("User.policy.rule.0").unwrap().kind,
            NodeKind::PolicyRule
        );
        assert_eq!(
            graph
                .node("User.policy.operation.disable_user")
                .unwrap()
                .kind,
            NodeKind::PolicyOperation
        );
        assert_eq!(
            graph
                .node("User.policy.operation.disable_user.rule.0")
                .unwrap()
                .kind,
            NodeKind::PolicyRule
        );
        assert_eq!(
            graph.node("User.updated_at.generated").unwrap().kind,
            NodeKind::GeneratedField
        );
        for node in [
            "User.lifecycle.transition.disable",
            "User.policy.operation.disable_user",
            "User.updated_at.generated",
        ] {
            let node = graph.node(node).unwrap();
            assert_eq!(node.source, "lifecycle.jadpo");
            assert!(node.range.end > node.range.start);
        }
        assert_eq!(graph, build_semantic_graph(&[parsed]));
    }

    #[test]
    fn type_name_resolution_is_case_sensitive() {
        let source = r#"
type UserName = Text {}
type ExactReference = UserName {}
type CaseMismatch = Username {}
"#;
        let parsed = parse(Path::new("case-sensitive.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);

        let graph = build_semantic_graph(&[parsed]);
        assert_eq!(graph.diagnostics.len(), 1, "{:#?}", graph.diagnostics);
        assert_eq!(graph.diagnostics[0].code, "SEM_UNKNOWN_NAME");
    }

    #[test]
    fn semantic_name_shapes_separate_types_from_runtime_names() {
        for name in ["User", "User2", "URL2"] {
            assert!(NameCase::UpperCamel.accepts(name), "{name}");
        }
        for name in ["user", "user2", "user_name"] {
            assert!(NameCase::LowerSnake.accepts(name), "{name}");
        }

        for name in ["user", "User_Name", "UserName_", "_User"] {
            assert!(!NameCase::UpperCamel.accepts(name), "{name}");
        }
        for name in ["User", "_name", "user__name", "user_name_", "userName"] {
            assert!(!NameCase::LowerSnake.accepts(name), "{name}");
        }
    }

    #[test]
    fn enforces_selective_module_imports_visibility_and_cycles() {
        let shared = parse(
            Path::new("shared.jadpo"),
            "module todo.shared\npublic type ContactEmail = Email {}\n",
        );
        let api = parse(
            Path::new("api.jadpo"),
            "module todo.api\nimport todo.shared { ContactEmail }\npublic input Signup { email: ContactEmail }\n",
        );
        assert!(shared.diagnostics.is_empty(), "{:#?}", shared.diagnostics);
        assert!(api.diagnostics.is_empty(), "{:#?}", api.diagnostics);
        let graph = build_semantic_graph(&[shared.clone(), api]);
        assert!(graph.diagnostics.is_empty(), "{:#?}", graph.diagnostics);
        assert_eq!(graph.modules.len(), 2);
        assert_eq!(graph.modules[0].name, "todo.api");
        assert_eq!(graph.modules[0].imports, vec!["todo.shared.ContactEmail"]);
        assert!(graph
            .manifest()
            .to_json()
            .contains("\"modules\":[{\"name\":\"todo.api\""));

        let missing_import = parse(
            Path::new("missing.jadpo"),
            "module todo.consumer\ninput SignupWithoutImport { email: ContactEmail }\n",
        );
        let graph = build_semantic_graph(&[shared, missing_import]);
        assert!(
            graph
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "MOD_IMPORT_REQUIRED"),
            "{:#?}",
            graph.diagnostics
        );

        let private_source = parse(
            Path::new("private.jadpo"),
            "module private.jadpo\ntype Hidden = Text {}\n",
        );
        let private_user = parse(
            Path::new("private-user.jadpo"),
            "module private.user\nimport private.jadpo { Hidden }\ninput UsesHidden { value: Hidden }\n",
        );
        let graph = build_semantic_graph(&[private_source, private_user]);
        assert!(graph
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MOD_PRIVATE_IMPORT"));

        let cycle_a = parse(
            Path::new("cycle-a.jadpo"),
            "module cycle.a\nimport cycle.b { B }\npublic type A = B {}\n",
        );
        let cycle_b = parse(
            Path::new("cycle-b.jadpo"),
            "module cycle.b\nimport cycle.a { A }\npublic type B = A {}\n",
        );
        let graph = build_semantic_graph(&[cycle_a, cycle_b]);
        assert!(graph
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "MOD_IMPORT_CYCLE"));
    }

    #[test]
    fn reports_each_invalid_module_relationship() {
        let parse_file = |name: &str, source: &str| {
            let parsed = parse(Path::new(name), source);
            assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
            parsed
        };
        let codes = |files: Vec<_>| {
            build_semantic_graph(&files)
                .diagnostics
                .into_iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>()
        };

        let without_module = codes(vec![parse_file(
            "public-without-module.jadpo",
            "public type PublicName = Text {}\ntype LocalName = Text {}\n",
        )]);
        assert!(
            without_module.contains(&"MOD_PUBLIC_REQUIRES_MODULE"),
            "{without_module:#?}"
        );

        let duplicate_modules = codes(vec![
            parse_file(
                "first.jadpo",
                "module duplicate.name\ntype First = Text {}\n",
            ),
            parse_file(
                "second.jadpo",
                "module duplicate.name\ntype Second = Text {}\n",
            ),
        ]);
        assert!(
            duplicate_modules.contains(&"MOD_DUPLICATE_MODULE"),
            "{duplicate_modules:#?}"
        );

        let mixed_modules = codes(vec![
            parse_file(
                "explicit.jadpo",
                "module explicit.name\ntype Named = Text {}\n",
            ),
            parse_file("implicit.jadpo", "type Unnamed = Text {}\n"),
        ]);
        assert!(
            mixed_modules.contains(&"MOD_MODULE_REQUIRED"),
            "{mixed_modules:#?}"
        );

        let invalid_imports = codes(vec![
            parse_file(
                "shared.jadpo",
                "module shared.names\npublic type Exported = Text {}\npublic type Extra = Text {}\ntype Private = Text {}\n",
            ),
            parse_file(
                "consumer.jadpo",
                r#"module consumer.names
import consumer.names { Local }
import absent.package { Missing }
import shared.names { Exported, Extra, Extra, Unknown }
type Exported = Text {}
type Local = Text {}
"#,
            ),
        ]);
        for expected in [
            "MOD_SELF_IMPORT",
            "MOD_UNKNOWN_MODULE",
            "MOD_IMPORT_CONFLICT",
            "MOD_DUPLICATE_IMPORT",
            "MOD_UNKNOWN_EXPORT",
        ] {
            assert!(
                invalid_imports.contains(&expected),
                "{expected}: {invalid_imports:#?}"
            );
        }
    }

    #[test]
    fn reports_duplicate_and_unknown_semantic_names() {
        let source = r#"
entity Customer { id: Missing }
entity Customer { id: Uuid }
"#;
        let parsed = parse(Path::new("bad.jadpo"), source);
        let graph = build_semantic_graph(&[parsed]);
        let codes = graph
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();

        assert!(codes.contains(&"SEM_DUPLICATE_DECLARATION"));
        assert!(codes.contains(&"SEM_UNKNOWN_NAME"));
    }

    #[test]
    fn validates_entity_persistence_modifiers() {
        let source = r#"
value Token { value: Text unique }
entity Account {
    first_id: Uuid identity
    second_id: Uuid? identity
}
"#;
        let parsed = parse(Path::new("bad-persistence.jadpo"), source);
        let graph = build_semantic_graph(&[parsed]);
        let codes = graph
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();

        assert!(codes.contains(&"DATA_MODIFIER_NON_ENTITY"));
        assert!(codes.contains(&"DATA_IDENTITY_NULLABLE"));
        assert!(codes.contains(&"DATA_MULTIPLE_IDENTITIES"));
    }

    #[test]
    fn validates_compound_persistence_constraints() {
        let source = r#"
entity Account {
    id: Uuid identity
    tenant: Text
    email: Text?
    constraint too_short: unique(id)
    constraint invalid: unique(email, email, absent)
    constraint first_shape: unique(id, tenant)
    constraint repeated_shape: unique(tenant, id)
}
"#;
        let parsed = parse(Path::new("bad-compound-constraints.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let graph = build_semantic_graph(&[parsed]);
        let codes = graph
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();

        assert!(codes.contains(&"DATA_COMPOUND_CONSTRAINT_FIELDS"));
        assert!(codes.contains(&"DATA_CONSTRAINT_NULLABLE_FIELD"));
        assert!(codes.contains(&"DATA_CONSTRAINT_DUPLICATE_FIELD"));
        assert!(codes.contains(&"DATA_CONSTRAINT_UNKNOWN_FIELD"));
        assert!(codes.contains(&"DATA_DUPLICATE_CONSTRAINT_SHAPE"));
    }

    #[test]
    fn validates_relationship_targets_lifecycle_and_cycles() {
        let source = r#"
entity User {
    id: Uuid identity
    email: Text
    todo_id: Todo.id references Todo.id on_delete restrict
}
entity Todo {
    id: Uuid identity
    user_id: Text references User.email on_delete set_null
}
"#;
        let parsed = parse(Path::new("bad-relationships.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let graph = build_semantic_graph(&[parsed]);
        let codes = graph
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();

        assert!(codes.contains(&"DATA_RELATIONSHIP_TARGET_NOT_KEY"));
        assert!(codes.contains(&"DATA_RELATIONSHIP_TYPE_MISMATCH"));
        assert!(codes.contains(&"DATA_RELATIONSHIP_SET_NULL_REQUIRED"));
        assert!(codes.contains(&"DATA_RELATIONSHIP_CYCLE"));
    }

    #[test]
    fn validates_inverse_relationships_against_owning_references() {
        let source = r#"
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.unrelated
}
entity Todo {
    id: Uuid identity
    owner_id: User.id references User.id on_delete cascade
    unrelated: Uuid
}
"#;
        let parsed = parse(Path::new("bad-inverse.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let graph = build_semantic_graph(&[parsed]);
        assert!(graph
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "DATA_INVERSE_NOT_OWNING_REFERENCE"));
    }

    #[test]
    fn reports_each_invalid_relationship_name_and_target_shape() {
        let source = r#"
entity User {
    id: Uuid identity
    todos: Text
    inverse todos: many Todo via Todo.absent_field
    inverse malformed: many Todo via User.id
}
entity Todo {
    id: Uuid identity
    owner_id: User.id references User on_delete cascade
}
"#;
        let parsed = parse(Path::new("invalid-relationship-shapes.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let graph = build_semantic_graph(&[parsed]);
        let codes = graph
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();

        assert!(codes.contains(&"DATA_INVERSE_DUPLICATE_NAME"), "{codes:#?}");
        assert!(codes.contains(&"DATA_INVERSE_VIA_FIELD"), "{codes:#?}");
        assert!(
            codes.contains(&"DATA_RELATIONSHIP_TARGET_FIELD"),
            "{codes:#?}"
        );
    }

    #[test]
    fn indexes_an_explicit_owning_relationship_name() {
        let source = r#"
entity User { id: Uuid identity }
entity Todo {
    id: Uuid identity
    owner_id: User.id references User.id as owner on_delete cascade
}
"#;
        let parsed = parse(Path::new("named-relationship.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let graph = build_semantic_graph(&[parsed]);

        assert!(graph.diagnostics.is_empty(), "{:#?}", graph.diagnostics);
        assert_eq!(
            graph.node("Todo.owner").unwrap().kind,
            NodeKind::Relationship
        );
        assert_eq!(graph.node("Todo.owner_id").unwrap().kind, NodeKind::Field);
    }

    #[test]
    fn requires_optional_inverse_references_to_be_unique() {
        let source = r#"
entity User {
    id: Uuid identity
    inverse profile: optional Profile via Profile.user_id
}
entity Profile {
    id: Uuid identity
    user_id: User.id references User.id on_delete cascade
}
"#;
        let parsed = parse(Path::new("non-unique-optional-inverse.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let graph = build_semantic_graph(&[parsed]);
        assert!(graph
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "DATA_INVERSE_OPTIONAL_NOT_UNIQUE"));
    }

    #[test]
    fn validates_configuration_bindings_and_secret_defaults() {
        let source = r#"
config First {
    first: Text { binding: "SHARED" }
    without_binding: Text { secret: false }
    empty_binding: Text { binding: "" }
    invalid_binding: Text { binding: "BAD-NAME" }
}
config Second {
    second: Text { binding: "SHARED" secret: true default: "unsafe" }
}
"#;
        let parsed = parse(Path::new("bad-config.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let graph = build_semantic_graph(&[parsed]);
        let codes = graph
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();
        assert!(codes.contains(&"CONFIG_MULTIPLE_DECLARATIONS"));
        assert!(codes.contains(&"CONFIG_BINDING_REQUIRED"));
        assert!(codes.contains(&"CONFIG_DUPLICATE_BINDING"));
        assert!(codes.contains(&"CONFIG_BINDING_EMPTY"));
        assert!(codes.contains(&"CONFIG_BINDING_INVALID"));
        assert!(codes.contains(&"CONFIG_SECRET_DEFAULT"));
    }

    #[test]
    fn indexes_the_application_and_closed_principal_contract() {
        let source = r#"
application TodoApplication {
    authentication {
        principal: Principal
        revocation { mode: immediate }
    }
}
principal Principal {
    user { subject: Text }
    service { subject: Text }
}
"#;
        let parsed = parse(Path::new("authentication.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let graph = build_semantic_graph(&[parsed]);

        assert!(graph.diagnostics.is_empty(), "{:#?}", graph.diagnostics);
        assert_eq!(
            graph
                .node("TodoApplication")
                .expect("application node")
                .kind,
            NodeKind::Application
        );
        assert_eq!(
            graph.node("Principal").expect("principal node").kind,
            NodeKind::Principal
        );
        assert_eq!(
            graph.node("Principal.user").expect("user variant").kind,
            NodeKind::PrincipalVariant
        );
        assert_eq!(
            graph
                .node("Principal.service")
                .expect("service variant")
                .kind,
            NodeKind::PrincipalVariant
        );
    }

    #[test]
    fn indexes_authentication_strategies_slots_and_validation_modes() {
        let source = r#"
application TodoApplication {
    authentication {
        principal: Principal
        revocation { mode: immediate }
    }
}
principal Principal {
    user { subject: Text }
    service { subject: Text }
}
authentication api_bearer {
    transport { bearer: authorization_header }
    validators {
        opaque_user { mode: opaque principal: user }
        service_key { mode: api_key principal: service }
    }
}
"#;
        let parsed = parse(Path::new("authentication-strategy.jadpo"), source);
        assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
        let graph = build_semantic_graph(&[parsed]);
        assert!(graph.diagnostics.is_empty(), "{:#?}", graph.diagnostics);
        for (name, kind) in [
            (
                "authentication.api_bearer",
                NodeKind::AuthenticationStrategy,
            ),
            (
                "authentication.api_bearer.bearer.authorization_header",
                NodeKind::CredentialSlot,
            ),
            (
                "authentication.api_bearer.validator.opaque_user.opaque",
                NodeKind::CredentialValidation,
            ),
            (
                "authentication.api_bearer.validator.service_key.api_key",
                NodeKind::CredentialValidation,
            ),
        ] {
            assert_eq!(graph.node(name).expect(name).kind, kind);
        }
    }
}
