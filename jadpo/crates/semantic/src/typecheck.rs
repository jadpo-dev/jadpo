use crate::{NodeKind, SemanticGraph};
use jadpo_diagnostics::{Diagnostic, DiagnosticFact, SourceSpan};
use jadpo_syntax::{
    Block, CallableDeclaration, ConfigDefaultKind, Constraint, ConstraintKind, Declaration,
    Expression, FieldInitialiser, InvocationExpression, JobDeclaration, Literal, LiteralKind, Name,
    ParsedSyntax, PersistenceModifier, RouteDeclaration, RouteSuccess, Statement, TextRange,
    TypeReference,
};
use std::collections::{BTreeMap, BTreeSet};

mod delivery_seal;
mod delivery_selector;
mod delivery_types;
pub use delivery_types::DeliveryTypeCandidate;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InferredExpression {
    pub source: String,
    pub range: TextRange,
    pub type_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClockRead {
    pub source: String,
    pub range: TextRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedJobBinding {
    pub job: String,
    pub callee: String,
    pub snapshot_type: String,
    pub interval_ms: u64,
    pub constructor_proof: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TypeCheckResult {
    pub diagnostics: Vec<Diagnostic>,
    pub expressions: Vec<InferredExpression>,
    pub clock_reads: Vec<ClockRead>,
    pub jobs: Vec<CheckedJobBinding>,
    /// Necessary resolved facts only; never delivery authority or a checked job.
    pub delivery_candidates: Vec<DeliveryTypeCandidate>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TypeValue {
    name: String,
    arguments: Vec<TypeValue>,
    nullable: bool,
    secret: bool,
}

impl TypeValue {
    fn display(&self) -> String {
        let arguments = if self.arguments.is_empty() || self.name.contains('.') {
            String::new()
        } else {
            format!(
                "<{}>",
                self.arguments
                    .iter()
                    .map(Self::display)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let nullable = if self.nullable { "?" } else { "" };
        format!("{}{arguments}{nullable}", self.name)
    }
}

#[derive(Clone, Debug)]
struct CallableSignature {
    parameters: Vec<TypeValue>,
    result: TypeValue,
}

#[derive(Clone, Copy)]
enum TemporalExpected {
    Exact(&'static str),
    Timeline,
    FormatValue,
    ClockText,
    Components,
}

#[derive(Clone, Copy)]
enum TemporalResult {
    Exact(&'static str),
    FirstArgument,
}

#[derive(Clone, Debug)]
struct RecordField {
    declared_type: TypeValue,
    optional: bool,
    generated: Option<jadpo_syntax::GeneratedFieldRole>,
    update_forbidden: bool,
    lifecycle_owned: bool,
    lifecycle_initial: bool,
}

#[derive(Clone, Debug)]
struct InverseInfo {
    child: String,
    cardinality: jadpo_syntax::InverseCardinality,
}

#[derive(Clone, Debug)]
struct OwningReferenceInfo {
    parent: String,
    nullable: bool,
}

#[derive(Default)]
struct Catalogue {
    callables: BTreeMap<String, CallableSignature>,
    records: BTreeMap<String, BTreeMap<String, RecordField>>,
    failure_fields: BTreeMap<String, BTreeMap<String, RecordField>>,
    constraints: BTreeMap<String, Vec<Constraint>>,
    entities: BTreeSet<String>,
    ordered_keys: BTreeMap<String, BTreeSet<String>>,
    persistence_constraints: BTreeSet<String>,
    inverses: BTreeMap<String, BTreeMap<String, InverseInfo>>,
    owning_references: BTreeMap<String, BTreeMap<String, OwningReferenceInfo>>,
    field_reference_targets: BTreeMap<(String, String), String>,
    record_kinds: BTreeMap<String, jadpo_syntax::RecordKind>,
    enums: BTreeMap<String, BTreeMap<String, BTreeMap<String, RecordField>>>,
    simple_enums: BTreeSet<String>,
    configuration: BTreeMap<String, RecordField>,
    principal: Option<String>,
    failures: BTreeMap<String, String>,
    persistent_entities: BTreeSet<String>,
    identity_fields: BTreeSet<(String, String)>,
    reference_fields: BTreeMap<(String, String), Vec<String>>,
    authority_stores: BTreeMap<String, Option<String>>,
    lifecycles: BTreeMap<String, jadpo_syntax::EntityLifecycle>,
    locales: BTreeSet<String>,
    locale_default: Option<String>,
}

pub fn check_types(files: &[ParsedSyntax], graph: &SemanticGraph) -> TypeCheckResult {
    let catalogue = Catalogue::from_files(files, graph);
    let mut checker = TypeChecker {
        graph,
        catalogue,
        result: TypeCheckResult::default(),
        delivery_seals: delivery_seal::seals_from_files(files),
        direct_range_inputs: None,
    };
    checker.check_files(files);
    checker.check_delivery_sealed_declarations(files);
    checker.result
}

impl Catalogue {
    fn from_files(files: &[ParsedSyntax], graph: &SemanticGraph) -> Self {
        let mut catalogue = Self::default();
        for (name, format) in [
            ("Email", "email"),
            ("Url", "url"),
            ("IpAddress", "ip_address"),
        ] {
            catalogue.constraints.insert(
                name.to_owned(),
                vec![Constraint {
                    kind: ConstraintKind::Format,
                    value: Literal {
                        kind: LiteralKind::String,
                        text: format.to_owned(),
                        range: TextRange::new(0, 0),
                    },
                    range: TextRange::new(0, 0),
                }],
            );
        }
        catalogue
            .constraints
            .entry("Email".to_owned())
            .or_default()
            .push(Constraint {
                kind: ConstraintKind::MaxLength,
                value: Literal {
                    kind: LiteralKind::Integer,
                    text: "254".to_owned(),
                    range: TextRange::new(0, 0),
                },
                range: TextRange::new(0, 0),
            });
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
            catalogue.enums.insert(
                owner.to_owned(),
                variants
                    .iter()
                    .map(|variant| ((*variant).to_owned(), BTreeMap::new()))
                    .collect(),
            );
        }
        catalogue.enums.insert(
            "Zone".to_owned(),
            include_str!("../../../data/iana-zones-2026c.txt")
                .lines()
                .map(|zone| (standard_variant_name(zone), BTreeMap::new()))
                .collect(),
        );
        catalogue.records.insert(
            "InstantRange".to_owned(),
            [
                (
                    "start".to_owned(),
                    RecordField {
                        declared_type: simple_type("Instant"),
                        optional: false,
                        generated: None,
                        update_forbidden: false,
                        lifecycle_owned: false,
                        lifecycle_initial: false,
                    },
                ),
                (
                    "end".to_owned(),
                    RecordField {
                        declared_type: simple_type("Instant"),
                        optional: false,
                        generated: None,
                        update_forbidden: false,
                        lifecycle_owned: false,
                        lifecycle_initial: false,
                    },
                ),
            ]
            .into_iter()
            .collect(),
        );
        catalogue.records.insert(
            "Time".to_owned(),
            [
                (
                    "instant".to_owned(),
                    RecordField {
                        declared_type: simple_type("Instant"),
                        optional: false,
                        generated: None,
                        update_forbidden: false,
                        lifecycle_owned: false,
                        lifecycle_initial: false,
                    },
                ),
                (
                    "zone".to_owned(),
                    RecordField {
                        declared_type: simple_type("Zone"),
                        optional: false,
                        generated: None,
                        update_forbidden: false,
                        lifecycle_owned: false,
                        lifecycle_initial: false,
                    },
                ),
            ]
            .into_iter()
            .collect(),
        );
        catalogue.records.insert(
            "__jadpo_clock".to_owned(),
            [(
                "now".to_owned(),
                RecordField {
                    declared_type: simple_type("Instant"),
                    optional: false,
                    generated: None,
                    update_forbidden: false,
                    lifecycle_owned: false,
                    lifecycle_initial: false,
                },
            )]
            .into_iter()
            .collect(),
        );
        for file in files {
            for declaration in &file.file.declarations {
                match declaration {
                    Declaration::Locales(declaration) => {
                        catalogue.locale_default = Some(unquote(&declaration.default.text));
                        for locale in &declaration.supported {
                            catalogue.locales.insert(unquote(&locale.text));
                        }
                    }
                    Declaration::Config(declaration) => {
                        catalogue
                            .configuration
                            .extend(declaration.fields.iter().map(|field| {
                                (
                                    field.name.text.clone(),
                                    RecordField {
                                        declared_type: TypeValue {
                                            secret: field.secret,
                                            ..resolved_type_value(&field.field_type, graph)
                                        },
                                        optional: field.default.is_some(),
                                        generated: None,
                                        update_forbidden: false,
                                        lifecycle_owned: false,
                                        lifecycle_initial: false,
                                    },
                                )
                            }));
                    }
                    Declaration::Principal(declaration) => {
                        catalogue.principal = Some(declaration.name.text.clone());
                        let mut variants = BTreeMap::new();
                        for variant in &declaration.variants {
                            let fields = variant
                                .fields
                                .iter()
                                .map(|field| {
                                    (
                                        field.name.text.clone(),
                                        RecordField {
                                            declared_type: resolved_type_value(
                                                &field.field_type,
                                                graph,
                                            ),
                                            optional: field.optional,
                                            generated: None,
                                            update_forbidden: false,
                                            lifecycle_owned: false,
                                            lifecycle_initial: false,
                                        },
                                    )
                                })
                                .collect::<BTreeMap<_, _>>();
                            catalogue.records.insert(
                                format!("{}.{}", declaration.name.text, variant.kind.as_str()),
                                fields.clone(),
                            );
                            variants.insert(variant.kind.as_str().to_owned(), fields);
                        }
                        catalogue
                            .enums
                            .insert(declaration.name.text.clone(), variants);
                    }
                    Declaration::Type(declaration) => {
                        catalogue.constraints.insert(
                            declaration.name.text.clone(),
                            declaration.constraints.clone(),
                        );
                    }
                    Declaration::Enum(declaration) => {
                        if declaration
                            .variants
                            .iter()
                            .all(|variant| variant.fields.is_empty())
                        {
                            catalogue.simple_enums.insert(declaration.name.text.clone());
                        }
                        let mut variants = BTreeMap::new();
                        for variant in &declaration.variants {
                            let mut fields = BTreeMap::new();
                            for field in &variant.fields {
                                catalogue.constraints.insert(
                                    format!(
                                        "{}.{}.{}",
                                        declaration.name.text, variant.name.text, field.name.text
                                    ),
                                    field.constraints.clone(),
                                );
                                fields.insert(
                                    field.name.text.clone(),
                                    RecordField {
                                        declared_type: resolved_type_value(
                                            &field.field_type,
                                            graph,
                                        ),
                                        optional: field.optional,
                                        generated: None,
                                        update_forbidden: false,
                                        lifecycle_owned: false,
                                        lifecycle_initial: false,
                                    },
                                );
                            }
                            variants.insert(variant.name.text.clone(), fields);
                        }
                        catalogue
                            .enums
                            .insert(declaration.name.text.clone(), variants);
                    }
                    Declaration::Record(declaration) => {
                        if declaration.is_persistent_entity() {
                            catalogue.authority_stores.insert(
                                declaration.name.text.clone(),
                                declaration
                                    .dossier
                                    .as_ref()
                                    .and_then(|d| d.persistence.as_ref())
                                    .map(|p| p.store.text.clone()),
                            );
                        }
                        for field in &declaration.fields {
                            if field.persistence.contains(&PersistenceModifier::Identity) {
                                catalogue.identity_fields.insert((
                                    declaration.name.text.clone(),
                                    field.name.text.clone(),
                                ));
                            }
                            if let Some(reference) = &field.reference {
                                catalogue.reference_fields.insert(
                                    (declaration.name.text.clone(), field.name.text.clone()),
                                    reference
                                        .target
                                        .path
                                        .iter()
                                        .map(|p| p.text.clone())
                                        .collect(),
                                );
                                catalogue.field_reference_targets.insert(
                                    (declaration.name.text.clone(), field.name.text.clone()),
                                    reference.target.path[0].text.clone(),
                                );
                            }
                        }
                        catalogue
                            .record_kinds
                            .insert(declaration.name.text.clone(), declaration.kind);
                        if declaration.kind == jadpo_syntax::RecordKind::Entity {
                            if let Some(lifecycle) = declaration
                                .dossier
                                .as_ref()
                                .and_then(|dossier| dossier.lifecycle.as_ref())
                            {
                                catalogue
                                    .lifecycles
                                    .insert(declaration.name.text.clone(), lifecycle.clone());
                            }
                            catalogue.entities.insert(declaration.name.text.clone());
                            if declaration.is_persistent_entity() {
                                catalogue
                                    .persistent_entities
                                    .insert(declaration.name.text.clone());
                            }
                            catalogue.ordered_keys.insert(
                                declaration.name.text.clone(),
                                declaration
                                    .fields
                                    .iter()
                                    .filter(|field| {
                                        field.persistence.contains(&PersistenceModifier::Identity)
                                            || field
                                                .persistence
                                                .contains(&PersistenceModifier::Unique)
                                    })
                                    .map(|field| field.name.text.clone())
                                    .collect(),
                            );
                            catalogue.persistence_constraints.extend(
                                declaration
                                    .fields
                                    .iter()
                                    .filter(|field| {
                                        field.persistence.contains(&PersistenceModifier::Identity)
                                            || field
                                                .persistence
                                                .contains(&PersistenceModifier::Unique)
                                    })
                                    .map(|field| {
                                        format!("{}.{}", declaration.name.text, field.name.text)
                                    }),
                            );
                            catalogue.persistence_constraints.extend(
                                declaration
                                    .persistence_constraints
                                    .iter()
                                    .map(|constraint| {
                                        format!(
                                            "{}.{}",
                                            declaration.name.text, constraint.name.text
                                        )
                                    }),
                            );
                        }
                        let lifecycle = declaration
                            .dossier
                            .as_ref()
                            .and_then(|dossier| dossier.lifecycle.as_ref());
                        let lifecycle_initial = lifecycle
                            .and_then(|contract| contract.initial.as_ref())
                            .into_iter()
                            .flatten()
                            .map(|field| field.name.text.clone())
                            .collect::<BTreeSet<_>>();
                        let mut lifecycle_owned = lifecycle_initial.clone();
                        if let Some(contract) = lifecycle {
                            lifecycle_owned.extend(contract.transitions.iter().flat_map(
                                |transition| {
                                    transition.set.iter().map(|field| field.name.text.clone())
                                },
                            ));
                            if let Some(purge) = &contract.purge {
                                lifecycle_owned.insert(purge.from.text.clone());
                            }
                        }
                        let fields = declaration
                            .fields
                            .iter()
                            .map(|field| {
                                let full_name =
                                    format!("{}.{}", declaration.name.text, field.name.text);
                                catalogue
                                    .constraints
                                    .insert(full_name, field.constraints.clone());
                                (
                                    field.name.text.clone(),
                                    RecordField {
                                        declared_type: resolved_type_value(
                                            &field.field_type,
                                            graph,
                                        ),
                                        optional: field.optional,
                                        generated: field.generated,
                                        update_forbidden: field.immutable
                                            || field.role.is_some()
                                            || field
                                                .persistence
                                                .contains(&PersistenceModifier::Identity)
                                            || declaration.membership.as_ref().is_some_and(
                                                |membership| {
                                                    membership.scope.text == field.name.text
                                                        || membership.member.text == field.name.text
                                                        || membership.role.text == field.name.text
                                                },
                                            )
                                            || lifecycle_owned.contains(&field.name.text),
                                        lifecycle_owned: lifecycle_owned.contains(&field.name.text),
                                        lifecycle_initial: lifecycle_initial
                                            .contains(&field.name.text),
                                    },
                                )
                            })
                            .collect();
                        catalogue
                            .records
                            .insert(declaration.name.text.clone(), fields);
                        catalogue.inverses.insert(
                            declaration.name.text.clone(),
                            declaration
                                .inverses
                                .iter()
                                .map(|inverse| {
                                    (
                                        inverse.name.text.clone(),
                                        InverseInfo {
                                            child: inverse.target.text.clone(),
                                            cardinality: inverse.cardinality,
                                        },
                                    )
                                })
                                .collect(),
                        );
                        catalogue.owning_references.insert(
                            declaration.name.text.clone(),
                            declaration
                                .fields
                                .iter()
                                .filter_map(|field| {
                                    let reference = field.reference.as_ref()?;
                                    Some((
                                        reference.relationship.as_ref().map_or_else(
                                            || field.name.text.clone(),
                                            |name| name.text.clone(),
                                        ),
                                        OwningReferenceInfo {
                                            parent: reference.target.path[0].text.clone(),
                                            nullable: field.field_type.nullable,
                                        },
                                    ))
                                })
                                .collect(),
                        );
                    }
                    Declaration::Failure(declaration) => {
                        catalogue
                            .failures
                            .insert(declaration.name.text.clone(), declaration.kind.text.clone());
                        for (scope, fields) in [
                            ("public", &declaration.public_fields),
                            ("internal", &declaration.internal_fields),
                        ] {
                            catalogue.failure_fields.insert(
                                format!("{}.{}", declaration.name.text, scope),
                                fields
                                    .iter()
                                    .map(|field| {
                                        (
                                            field.name.text.clone(),
                                            RecordField {
                                                declared_type: resolved_type_value(
                                                    &field.field_type,
                                                    graph,
                                                ),
                                                optional: field.optional,
                                                generated: None,
                                                update_forbidden: false,
                                                lifecycle_owned: false,
                                                lifecycle_initial: false,
                                            },
                                        )
                                    })
                                    .collect(),
                            );
                            for field in fields {
                                catalogue.constraints.insert(
                                    format!(
                                        "{}.{}.{}",
                                        declaration.name.text, scope, field.name.text
                                    ),
                                    field.constraints.clone(),
                                );
                            }
                        }
                    }
                    Declaration::Callable(declaration) => {
                        catalogue.callables.insert(
                            declaration.name.text.clone(),
                            CallableSignature {
                                parameters: declaration
                                    .parameters
                                    .iter()
                                    .map(|parameter| {
                                        resolved_type_value(&parameter.parameter_type, graph)
                                    })
                                    .collect(),
                                result: resolved_type_value(&declaration.return_type, graph),
                            },
                        );
                    }
                    Declaration::Application(_)
                    | Declaration::AuthenticationStrategy(_)
                    | Declaration::Fixture(_)
                    | Declaration::Test(_)
                    | Declaration::Route(_)
                    | Declaration::Job(_) => {}
                }
            }
        }
        for effect in &graph.external_effects {
            catalogue.callables.insert(
                format!("{}.{}", effect.service, effect.operation),
                CallableSignature {
                    parameters: vec![resolved_type_value(
                        &super::service_type_reference(&effect.input, effect.range),
                        graph,
                    )],
                    result: resolved_type_value(
                        &super::service_type_reference(&effect.output, effect.range),
                        graph,
                    ),
                },
            );
        }
        if !catalogue.configuration.is_empty() {
            catalogue
                .records
                .insert("__jadpo_config".to_owned(), catalogue.configuration.clone());
        }
        catalogue.enums.insert(
            "Locale".to_owned(),
            catalogue
                .locales
                .iter()
                .map(|locale| (standard_variant_name(locale), BTreeMap::new()))
                .collect(),
        );
        catalogue
    }

    fn is_projection_object(&self, name: &str) -> bool {
        matches!(
            self.record_kinds.get(name),
            Some(jadpo_syntax::RecordKind::Value | jadpo_syntax::RecordKind::Output)
        )
    }

    fn is_patch_object(&self, name: &str) -> bool {
        matches!(
            self.record_kinds.get(name),
            Some(jadpo_syntax::RecordKind::Value | jadpo_syntax::RecordKind::Input)
        )
    }
}

struct TypeChecker<'graph> {
    graph: &'graph SemanticGraph,
    catalogue: Catalogue,
    result: TypeCheckResult,
    delivery_seals: Vec<delivery_seal::DeliverySeal>,
    // Only original input parameters/route input declarations, never aliases or
    // shadowed locals, may supply the initial required page range operands.
    direct_range_inputs: Option<BTreeSet<String>>,
}

impl TypeChecker<'_> {
    fn base_environment(&self) -> BTreeMap<String, TypeValue> {
        let mut environment = BTreeMap::new();
        if !self.catalogue.configuration.is_empty() {
            environment.insert("config".to_owned(), simple_type("__jadpo_config"));
        }
        environment
    }

    fn check_route_success(
        &mut self,
        route: &RouteDeclaration,
        result: Option<TypeValue>,
        source: &str,
    ) {
        match route.success {
            RouteSuccess::Ok => {}
            RouteSuccess::Created => {
                let Some(output) = &route.output else {
                    self.push_diagnostic_with_facts(
                        "TYPE_MISMATCH",
                        source,
                        route.range,
                        [
                            DiagnosticFact::Expected(
                                "a declared response type for `success: created`".to_owned(),
                            ),
                            DiagnosticFact::Received(
                                result
                                    .map_or_else(|| "no response type".to_owned(), |t| t.display()),
                            ),
                        ],
                    );
                    return;
                };
                if let Some(result) = result {
                    let expected = resolved_type_value(output, self.graph);
                    self.require_compatible(&result, &expected, source, route.range);
                }
            }
            RouteSuccess::NoContent => {
                if let Some(output) = &route.output {
                    let received = resolved_type_value(output, self.graph);
                    self.push_diagnostic_with_facts(
                        "TYPE_MISMATCH",
                        source,
                        route.range,
                        [
                            DiagnosticFact::Expected(
                                "Unit with no declared response body".to_owned(),
                            ),
                            DiagnosticFact::Received(received.display()),
                        ],
                    );
                } else if let Some(result) = result {
                    self.require_compatible(&result, &simple_type("Unit"), source, route.range);
                }
            }
        }
    }

    fn check_files(&mut self, files: &[ParsedSyntax]) {
        let post_paths = files
            .iter()
            .flat_map(|file| file.file.declarations.iter())
            .filter_map(|declaration| match declaration {
                Declaration::Route(route) if route_method_name(route.method) == "POST" => {
                    Some(route.path.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for file in files {
            for declaration in &file.file.declarations {
                if let Declaration::AuthenticationStrategy(strategy) = declaration {
                    if let Some(exchange) = &strategy.exchange {
                        let path = exchange.path.text.trim_matches('"');
                        if post_paths
                            .iter()
                            .any(|route| route_could_match_literal(route, path))
                        {
                            self.push_diagnostic(
                                "TYPE_AUTH_EXCHANGE_ROUTE_COLLISION",
                                &file.source_name,
                                exchange.path.range,
                            );
                        }
                    }
                }
            }
        }
        self.check_authentication_contract(files);
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
                match declaration {
                    Declaration::Record(record) => {
                        self.check_record_defaults(record, &file.source_name);
                        self.check_generated_fields(record, &file.source_name);
                        self.check_entity_lifecycle(record, &file.source_name);
                    }
                    Declaration::Locales(locales) => self.check_locales(locales, &file.source_name),
                    Declaration::Config(configuration) => {
                        self.check_configuration(configuration, &file.source_name)
                    }
                    Declaration::AuthenticationStrategy(strategy) => {
                        self.check_authentication_strategy(strategy, &file.source_name)
                    }
                    Declaration::Callable(callable) => {
                        self.check_callable(callable, &file.source_name)
                    }
                    Declaration::Job(job) => {
                        let schedule = self.check_job_schedule(job, &file.source_name);
                        // Necessary facts only: Core's all-gates atomic finish
                        // owns completed bindings and unsupported diagnostics.
                        // Never publish delivery through ordinary checked jobs.
                        if let Some(delivery) = &job.delivery {
                            let before = self.result.diagnostics.len();
                            self.check_delivery_selector_types(delivery, files, &file.source_name);
                            let service =
                                self.check_delivery_service_types(delivery, &file.source_name);
                            if before == self.result.diagnostics.len() {
                                if let (Some(schedule), Some(service)) = (schedule, service) {
                                    self.result.delivery_candidates.push(
                                        DeliveryTypeCandidate::new(
                                            schedule,
                                            &file.source_name,
                                            delivery,
                                            service,
                                        ),
                                    );
                                }
                            }
                            continue;
                        }
                        if let Some(schedule) = schedule {
                            self.result.jobs.push(schedule);
                        }
                    }
                    Declaration::Route(route) => {
                        if let Some(deadline) = &route.deadline {
                            if deadline.milliseconds().is_none() {
                                self.push_diagnostic(
                                    "ROUTE_DEADLINE_INVALID",
                                    &file.source_name,
                                    deadline.range,
                                );
                            }
                        }
                        self.check_route_transport_bindings(route, &file.source_name);
                        let mut environment = self.base_environment();
                        if route.inline_action.is_some() {
                            environment.insert("clock".to_owned(), simple_type("__jadpo_clock"));
                        }
                        if !route.public {
                            if let Some(principal) = &self.catalogue.principal {
                                environment.insert("principal".to_owned(), simple_type(principal));
                                environment
                                    .insert("current_principal".to_owned(), simple_type(principal));
                            }
                        } else {
                            environment.insert("__route_public".to_owned(), simple_type("Unit"));
                        }
                        if let Some(input) = &route.input {
                            environment
                                .insert("input".to_owned(), resolved_type_value(input, self.graph));
                        }
                        if let Some(query) = &route.query {
                            environment
                                .insert("query".to_owned(), resolved_type_value(query, self.graph));
                        }
                        if !route.headers.is_empty() {
                            let headers_type = format!("__route_headers_{}", route.range.start);
                            self.catalogue.records.insert(
                                headers_type.clone(),
                                route
                                    .headers
                                    .iter()
                                    .map(|header| {
                                        (
                                            header.name.text.clone(),
                                            RecordField {
                                                declared_type: resolved_type_value(
                                                    &header.field_type,
                                                    self.graph,
                                                ),
                                                optional: header.optional,
                                                generated: None,
                                                update_forbidden: false,
                                                lifecycle_owned: false,
                                                lifecycle_initial: false,
                                            },
                                        )
                                    })
                                    .collect(),
                            );
                            environment.insert("headers".to_owned(), simple_type(&headers_type));
                        }
                        if !route.path_fields.is_empty() {
                            let path_type = format!("__route_path_{}", route.range.start);
                            self.catalogue.records.insert(
                                path_type.clone(),
                                route
                                    .path_fields
                                    .iter()
                                    .map(|field| {
                                        (
                                            field.name.text.clone(),
                                            RecordField {
                                                declared_type: resolved_type_value(
                                                    &field.field_type,
                                                    self.graph,
                                                ),
                                                optional: false,
                                                generated: None,
                                                update_forbidden: false,
                                                lifecycle_owned: false,
                                                lifecycle_initial: false,
                                            },
                                        )
                                    })
                                    .collect(),
                            );
                            environment.insert("path".to_owned(), simple_type(&path_type));
                        }
                        let run_result = route.run.as_ref().and_then(|run| {
                            self.infer_invocation(run, &environment, &file.source_name)
                        });
                        if let Some(action) = &route.inline_action {
                            let mut mutable_bindings = BTreeSet::new();
                            let output = route
                                .output
                                .as_ref()
                                .map(|reference| resolved_type_value(reference, self.graph))
                                .unwrap_or_else(|| simple_type("Unit"));
                            self.check_block(
                                &action.body,
                                &mut environment,
                                &mut mutable_bindings,
                                &output,
                                &file.source_name,
                            );
                        }
                        self.check_route_success(route, run_result, &file.source_name);
                    }
                    Declaration::Test(test) => {
                        let mut environment = self.base_environment();
                        let fixture = test
                            .fixture
                            .as_ref()
                            .and_then(|name| fixtures.get(name.text.as_str()).copied());
                        if fixture
                            .and_then(|fixture| fixture.configuration.as_ref())
                            .is_none()
                        {
                            environment.remove("config");
                        }
                        environment.insert("clock".to_owned(), simple_type("__jadpo_clock"));
                        environment.insert("__jadpo_test_clock".to_owned(), simple_type("Unit"));
                        let mut mutable_bindings = BTreeSet::new();
                        self.check_block(
                            &test.body,
                            &mut environment,
                            &mut mutable_bindings,
                            &simple_type("Unit"),
                            &file.source_name,
                        );
                    }
                    Declaration::Fixture(fixture) => {
                        let environment = self.base_environment();
                        if let Some(clock) = &fixture.clock {
                            if let Some(received) =
                                self.infer_expression(clock, &environment, &file.source_name)
                            {
                                self.require_compatible(
                                    &received,
                                    &simple_type("Instant"),
                                    &file.source_name,
                                    clock.range(),
                                );
                            }
                        }
                        if let Some(configuration) = &fixture.configuration {
                            self.check_fixture_configuration(
                                configuration,
                                &environment,
                                &file.source_name,
                                fixture.range,
                            );
                        }
                        let response_environment = BTreeMap::new();
                        for fake in &fixture.service_fakes {
                            for outcome in &fake.outcomes {
                                let jadpo_syntax::FixtureServiceFakeValue::Accepted(response) =
                                    &outcome.value
                                else {
                                    continue;
                                };
                                let effect = self.graph.external_effects.iter().find(|effect| {
                                    effect.service == fake.service.text
                                        && effect.operation == outcome.operation.text
                                });
                                let Some(effect) = effect else {
                                    continue;
                                };
                                let expected_name = effect.output.as_str();
                                let construction_name = match response {
                                    Expression::Construction(construction) => {
                                        Some(joined_name(&construction.target.path))
                                    }
                                    _ => None,
                                };
                                if construction_name.as_deref() != Some(expected_name) {
                                    self.push_diagnostic_with_facts(
                                        "TYPE_MISMATCH",
                                        &file.source_name,
                                        response.range(),
                                        [
                                            DiagnosticFact::Expected(format!(
                                                "a direct `{expected_name}` construction"
                                            )),
                                            DiagnosticFact::Received(
                                                construction_name.unwrap_or_else(|| {
                                                    "an arbitrary expression".to_owned()
                                                }),
                                            ),
                                        ],
                                    );
                                    continue;
                                }
                                if let Some(received) = self.infer_expression(
                                    response,
                                    &response_environment,
                                    &file.source_name,
                                ) {
                                    self.require_compatible(
                                        &received,
                                        &simple_type(expected_name),
                                        &file.source_name,
                                        response.range(),
                                    );
                                }
                            }
                        }
                    }
                    Declaration::Application(_)
                    | Declaration::Principal(_)
                    | Declaration::Type(_)
                    | Declaration::Enum(_)
                    | Declaration::Failure(_) => {}
                }
            }
        }
    }

    /// Common nominal schedule proof, not a delivery or authority proof. Its
    /// caller decides whether the result is publishable; delivery callers must
    /// still pass the separate complete binding boundary before export.
    fn check_job_schedule(
        &mut self,
        job: &JobDeclaration,
        source: &str,
    ) -> Option<CheckedJobBinding> {
        let before = self.result.diagnostics.len();
        if job.every.milliseconds().is_none() {
            self.push_diagnostic("TYPE_JOB_INTERVAL_INVALID", source, job.every.range);
        }
        let run = job.run.as_ref()?;
        let callee = joined_name(&run.callee.path);
        let signature = self.catalogue.callables.get(&callee).cloned();
        let parameter = signature.as_ref().and_then(|signature| {
            (signature.parameters.len() == 1).then(|| signature.parameters[0].clone())
        });
        // Direct unconstrained nominal Instant only: no primitive-signature
        // exemption, inherited refinement constraint or implicit cast.
        let exact_signature = signature
            .as_ref()
            .is_some_and(|signature| signature.result == simple_type("Unit"))
            && parameter.as_ref().is_some_and(|parameter| {
                !parameter.nullable
                    && !parameter.secret
                    && parameter.arguments.is_empty()
                    && self
                        .graph
                        .node(&parameter.name)
                        .is_some_and(|node| node.kind == NodeKind::Type)
                    && self.ancestors(&parameter.name)
                        == vec![parameter.name.clone(), "Instant".to_owned()]
                    && self
                        .catalogue
                        .constraints
                        .get(&parameter.name)
                        .map_or(true, Vec::is_empty)
            });
        let action = self.graph.node(&callee).map(|node| node.kind) == Some(NodeKind::Action);
        if !action || !exact_signature {
            self.push_diagnostic("TYPE_JOB_RUN_SIGNATURE_INVALID", source, run.callee.range);
        }
        let clock_argument = run.arguments.len() == 1
            && run.named_arguments.is_empty()
            && matches!(&run.arguments[0], Expression::Invocation(constructor)
                if parameter.as_ref().is_some_and(|parameter| joined_name(&constructor.callee.path) == parameter.name)
                && constructor.arguments.len() == 1 && constructor.named_arguments.is_empty()
                && matches!(&constructor.arguments[0], Expression::Name(name) if joined_name(&name.path) == "clock.now"));
        if !clock_argument {
            self.push_diagnostic("TYPE_JOB_RUN_ARGUMENT_INVALID", source, run.range);
        } else {
            let mut environment = self.base_environment();
            environment.insert("clock".to_owned(), simple_type("__jadpo_clock"));
            self.infer_invocation(run, &environment, source);
        }
        if !action || !exact_signature || before != self.result.diagnostics.len() {
            return None;
        }
        Some(CheckedJobBinding {
            job: job.name.text.clone(),
            callee,
            snapshot_type: parameter?.name,
            interval_ms: job.every.milliseconds()?,
            constructor_proof: "trusted_intrinsic_instant_to_unconstrained_direct_nominal_instant"
                .to_owned(),
        })
    }

    fn check_locales(&mut self, locales: &jadpo_syntax::LocalesDeclaration, source: &str) {
        let default = unquote(&locales.default.text);
        let mut seen = BTreeSet::new();
        for locale in &locales.supported {
            let value = unquote(&locale.text);
            if canonical_bcp47(&value).as_deref() != Some(value.as_str()) {
                self.push_diagnostic("TYPE_LOCALE_INVALID", source, locale.range);
            }
            if !seen.insert(value) {
                self.push_diagnostic("TYPE_LOCALE_DUPLICATE", source, locale.range);
            }
        }
        if canonical_bcp47(&default).as_deref() != Some(default.as_str()) {
            self.push_diagnostic("TYPE_LOCALE_INVALID", source, locales.default.range);
        }
        if !seen.contains(&default) {
            self.push_diagnostic(
                "TYPE_LOCALE_DEFAULT_UNSUPPORTED",
                source,
                locales.default.range,
            );
        }
    }

    fn check_generated_fields(&mut self, record: &jadpo_syntax::RecordDeclaration, source: &str) {
        for field in &record.fields {
            let declared = resolved_type_value(&field.field_type, self.graph);
            if record.kind == jadpo_syntax::RecordKind::Entity
                && record.is_persistent_entity()
                && self.representation_root(&declared.name).as_deref() == Some("PresentationText")
            {
                self.push_diagnostic("TYPE_PRESENTATION_TEXT_PERSISTENCE", source, field.range);
            }
            if field.generated.is_none() {
                continue;
            }
            if record.kind != jadpo_syntax::RecordKind::Entity || !record.is_persistent_entity() {
                self.push_diagnostic("TYPE_GENERATED_FIELD_CONTEXT", source, field.range);
            }
            if field.generated == Some(jadpo_syntax::GeneratedFieldRole::Identity) {
                let identity_field = field
                    .persistence
                    .contains(&jadpo_syntax::PersistenceModifier::Identity);
                if !identity_field
                    || declared.nullable
                    || field.optional
                    || self.representation_root(&declared.name).as_deref() != Some("Uuid")
                {
                    self.push_diagnostic("TYPE_GENERATED_IDENTITY_INVALID", source, field.range);
                }
            } else if declared.nullable
                || field.optional
                || self.representation_root(&declared.name).as_deref() != Some("Instant")
            {
                self.push_diagnostic("TYPE_GENERATED_FIELD_TYPE", source, field.range);
            }
        }
    }

    fn check_entity_lifecycle(&mut self, record: &jadpo_syntax::RecordDeclaration, source: &str) {
        let Some(lifecycle) = record
            .dossier
            .as_ref()
            .and_then(|dossier| dossier.lifecycle.as_ref())
        else {
            return;
        };
        let entity = record.name.text.as_str();
        if record.kind != jadpo_syntax::RecordKind::Entity || !record.is_persistent_entity() {
            self.push_diagnostic(
                "TYPE_LIFECYCLE_REQUIRES_PERSISTENT_ENTITY",
                source,
                lifecycle.range,
            );
            return;
        }
        let Some(initial) = &lifecycle.initial else {
            self.push_diagnostic("TYPE_LIFECYCLE_INITIAL_REQUIRED", source, lifecycle.range);
            return;
        };
        if lifecycle.visible.is_none() {
            self.push_diagnostic("TYPE_LIFECYCLE_VISIBLE_REQUIRED", source, lifecycle.range);
        }

        let mut environment = self.base_environment();
        environment.insert("clock".to_owned(), simple_type("__jadpo_clock"));
        let fields = self
            .catalogue
            .records
            .get(entity)
            .cloned()
            .unwrap_or_default();
        for (name, field) in &fields {
            environment.insert(name.clone(), field.declared_type.clone());
        }
        let source_fields = record
            .fields
            .iter()
            .map(|field| (field.name.text.as_str(), field))
            .collect::<BTreeMap<_, _>>();
        let mut initial_fields = BTreeSet::new();
        for field in initial {
            if !initial_fields.insert(field.name.text.as_str()) {
                self.push_diagnostic("TYPE_LIFECYCLE_INITIAL_DUPLICATE", source, field.range);
                continue;
            }
            let Some(declared) = fields.get(&field.name.text) else {
                self.push_diagnostic("TYPE_LIFECYCLE_FIELD_UNKNOWN", source, field.name.range);
                continue;
            };
            let Some(source_field) = source_fields.get(field.name.text.as_str()) else {
                continue;
            };
            if source_field.generated.is_some()
                || source_field.immutable
                || source_field
                    .persistence
                    .contains(&PersistenceModifier::Identity)
                || source_field.role.is_some()
            {
                self.push_diagnostic("TYPE_LIFECYCLE_FIELD_INELIGIBLE", source, field.name.range);
            }
            if !self.lifecycle_value_is_allowed(entity, &field.value) {
                self.push_diagnostic(
                    "TYPE_LIFECYCLE_VALUE_UNSUPPORTED",
                    source,
                    field.value.range(),
                );
            }
            if let Some(received) = self.infer_expression(&field.value, &environment, source) {
                self.require_lifecycle_compatible(
                    &received,
                    &declared.declared_type,
                    source,
                    field.value.range(),
                );
            }
        }

        if let Some(visible) = &lifecycle.visible {
            self.check_lifecycle_predicate(entity, visible, &environment, source);
        }
        for transition in &lifecycle.transitions {
            self.check_lifecycle_predicate(entity, &transition.from, &environment, source);
            if let Some(visible) = &lifecycle.visible {
                let visible_state = lifecycle_predicate_equalities(visible);
                let source_state = lifecycle_predicate_equalities(&transition.from);
                let assignment_state = transition
                    .set
                    .iter()
                    .filter_map(|field| {
                        lifecycle_constant_identity(&field.value)
                            .map(|value| (field.name.text.as_str(), value))
                    })
                    .collect::<BTreeMap<_, _>>();
                let source_is_visible = visible_state
                    .as_ref()
                    .zip(source_state.as_ref())
                    .is_some_and(|(visible, source)| {
                        visible
                            .iter()
                            .all(|(field, value)| source.get(field.as_str()) == Some(value))
                    });
                let transition_restores_visibility = visible_state
                    .as_ref()
                    .zip(source_state.as_ref())
                    .is_some_and(|(visible, source)| {
                        let source_is_hidden = visible.iter().any(|(field, value)| {
                            source
                                .get(field.as_str())
                                .is_some_and(|source_value| source_value != value)
                        });
                        source_is_hidden
                            && visible.iter().all(|(field, value)| {
                                assignment_state
                                    .get(field.as_str())
                                    .or_else(|| source.get(field.as_str()))
                                    .is_some_and(|after| after == value)
                            })
                    });
                if transition_restores_visibility {
                    self.push_diagnostic(
                        "TYPE_LIFECYCLE_RESTORE_FORBIDDEN",
                        source,
                        transition.range,
                    );
                } else if !source_is_visible {
                    self.push_diagnostic(
                        "TYPE_LIFECYCLE_TRANSITION_SOURCE_INVALID",
                        source,
                        transition.from.range(),
                    );
                }
            }
            let mut set_fields = BTreeSet::new();
            for field in &transition.set {
                if !set_fields.insert(field.name.text.as_str()) {
                    self.push_diagnostic(
                        "TYPE_LIFECYCLE_TRANSITION_FIELD_DUPLICATE",
                        source,
                        field.range,
                    );
                    continue;
                }
                if !initial_fields.contains(field.name.text.as_str()) {
                    self.push_diagnostic(
                        "TYPE_LIFECYCLE_INITIAL_FIELD_REQUIRED",
                        source,
                        field.name.range,
                    );
                }
                let Some(declared) = fields.get(&field.name.text) else {
                    self.push_diagnostic("TYPE_LIFECYCLE_FIELD_UNKNOWN", source, field.name.range);
                    continue;
                };
                if !self.lifecycle_value_is_allowed(entity, &field.value) {
                    self.push_diagnostic(
                        "TYPE_LIFECYCLE_VALUE_UNSUPPORTED",
                        source,
                        field.value.range(),
                    );
                }
                if let Some(received) = self.infer_expression(&field.value, &environment, source) {
                    self.require_lifecycle_compatible(
                        &received,
                        &declared.declared_type,
                        source,
                        field.value.range(),
                    );
                }
            }
        }

        if let Some(purge) = &lifecycle.purge {
            let purge_field = fields.get(&purge.from.text);
            let valid_timestamp = purge_field.is_some_and(|field| {
                field.declared_type.nullable
                    && self
                        .representation_root(&field.declared_type.name)
                        .as_deref()
                        == Some("Instant")
            });
            if !valid_timestamp {
                self.push_diagnostic(
                    "TYPE_LIFECYCLE_PURGE_FIELD_INVALID",
                    source,
                    purge.from.range,
                );
            }
            if !initial_fields.contains(purge.from.text.as_str()) {
                self.push_diagnostic(
                    "TYPE_LIFECYCLE_INITIAL_FIELD_REQUIRED",
                    source,
                    purge.from.range,
                );
            }
            if let Some(initial_value) = initial
                .iter()
                .find(|field| field.name.text == purge.from.text)
            {
                if !matches!(&initial_value.value, Expression::Literal(literal) if literal.kind == LiteralKind::None)
                {
                    self.push_diagnostic(
                        "TYPE_LIFECYCLE_PURGE_INITIAL_INVALID",
                        source,
                        initial_value.value.range(),
                    );
                }
            }
            let valid_retention = matches!(
                &purge.after,
                Expression::Name(name)
                    if name.path.len() == 2 && name.path[0].text == "config"
            );
            if !valid_retention {
                self.push_diagnostic(
                    "TYPE_LIFECYCLE_PURGE_RETENTION_INVALID",
                    source,
                    purge.after.range(),
                );
            }
            if let Some(received) = self.infer_expression(&purge.after, &environment, source) {
                self.require_compatible(
                    &received,
                    &simple_type("Duration"),
                    source,
                    purge.after.range(),
                );
            }
            let transitions = lifecycle
                .transitions
                .iter()
                .filter_map(|transition| {
                    transition
                        .set
                        .iter()
                        .find(|field| field.name.text == purge.from.text)
                        .map(|field| (transition, field))
                })
                .collect::<Vec<_>>();
            if transitions.is_empty()
                || transitions.iter().any(|(_, field)| {
                    !matches!(
                        &field.value,
                        Expression::Name(name)
                            if name.path.len() == 2
                                && name.path[0].text == "clock"
                                && name.path[1].text == "now"
                    )
                })
            {
                self.push_diagnostic(
                    "TYPE_LIFECYCLE_PURGE_TRANSITION_INVALID",
                    source,
                    purge.range,
                );
            }
        }
    }

    fn check_lifecycle_predicate(
        &mut self,
        entity: &str,
        expression: &Expression,
        environment: &BTreeMap<String, TypeValue>,
        source: &str,
    ) {
        if !self.lifecycle_predicate_is_allowed(entity, expression) {
            self.push_diagnostic(
                "TYPE_LIFECYCLE_PREDICATE_UNSUPPORTED",
                source,
                expression.range(),
            );
        }
        if let Some(received) = self.infer_expression(expression, environment, source) {
            self.require_compatible(&received, &simple_type("Bool"), source, expression.range());
        }
    }

    fn require_lifecycle_compatible(
        &mut self,
        received: &TypeValue,
        expected: &TypeValue,
        source: &str,
        range: TextRange,
    ) {
        let nullable_lift = expected.nullable
            && !received.nullable
            && self.representation_root(&received.name) == self.representation_root(&expected.name);
        if !nullable_lift {
            self.require_compatible(received, expected, source, range);
        }
    }

    fn lifecycle_predicate_is_allowed(&self, entity: &str, expression: &Expression) -> bool {
        match expression {
            Expression::Grouped(grouped) => {
                self.lifecycle_predicate_is_allowed(entity, &grouped.value)
            }
            Expression::Binary(binary) if binary.operator == jadpo_syntax::BinaryOperator::And => {
                self.lifecycle_predicate_is_allowed(entity, &binary.left)
                    && self.lifecycle_predicate_is_allowed(entity, &binary.right)
            }
            Expression::Binary(binary)
                if binary.operator == jadpo_syntax::BinaryOperator::Equal =>
            {
                let operand =
                    |value: &Expression| match value {
                        Expression::Literal(_) => true,
                        Expression::Name(name) if name.path.len() == 1 => self
                            .catalogue
                            .records
                            .get(entity)
                            .is_some_and(|fields| fields.contains_key(&name.path[0].text)),
                        Expression::Name(name) if name.path.len() == 2 => {
                            self.catalogue.simple_enums.contains(&name.path[0].text)
                                && self.catalogue.enums.get(&name.path[0].text).is_some_and(
                                    |variants| variants.contains_key(&name.path[1].text),
                                )
                        }
                        _ => false,
                    };
                let field = |value: &Expression| match value {
                    Expression::Name(name) if name.path.len() == 1 => self
                        .catalogue
                        .records
                        .get(entity)
                        .is_some_and(|fields| fields.contains_key(&name.path[0].text)),
                    _ => false,
                };
                operand(&binary.left)
                    && operand(&binary.right)
                    && (field(&binary.left) || field(&binary.right))
            }
            _ => false,
        }
    }

    fn lifecycle_value_is_allowed(&self, entity: &str, expression: &Expression) -> bool {
        match expression {
            Expression::Literal(_) => true,
            Expression::Grouped(grouped) => self.lifecycle_value_is_allowed(entity, &grouped.value),
            Expression::Name(name) if name.path.len() == 2 => {
                (name.path[0].text == "clock" && name.path[1].text == "now")
                    || (self.catalogue.simple_enums.contains(&name.path[0].text)
                        && self
                            .catalogue
                            .enums
                            .get(&name.path[0].text)
                            .is_some_and(|variants| variants.contains_key(&name.path[1].text)))
            }
            Expression::Invocation(invocation)
                if invocation.callee.path.len() == 2
                    && invocation.callee.path[0].text == entity
                    && invocation.arguments.len() == 1
                    && invocation.named_arguments.is_empty()
                    && self.catalogue.records.get(entity).is_some_and(|fields| {
                        fields.contains_key(&invocation.callee.path[1].text)
                    }) =>
            {
                self.lifecycle_value_is_allowed(entity, &invocation.arguments[0])
            }
            _ => false,
        }
    }

    fn check_record_defaults(&mut self, record: &jadpo_syntax::RecordDeclaration, source: &str) {
        for field in &record.fields {
            let Some(default) = &field.default else {
                continue;
            };
            let declared = resolved_type_value(&field.field_type, self.graph);
            if default.kind == LiteralKind::None {
                if declared.nullable || self.graph.nullable_types.contains(&declared.name) {
                    continue;
                }
                self.push_diagnostic_with_facts(
                    "TYPE_MISMATCH",
                    source,
                    default.range,
                    [
                        DiagnosticFact::Expected(format!("{}?", declared.display())),
                        DiagnosticFact::Received("none".to_owned()),
                    ],
                );
                continue;
            }
            let received = self.literal_type(default);
            let received_root = received
                .as_ref()
                .and_then(|value| self.representation_root(&value.name));
            let expected_root = self.representation_root(&declared.name);
            if received_root != expected_root {
                self.push_diagnostic_with_facts(
                    "TYPE_MISMATCH",
                    source,
                    default.range,
                    [
                        DiagnosticFact::Expected(declared.display()),
                        DiagnosticFact::Received(
                            received_root.unwrap_or_else(|| "literal".to_owned()),
                        ),
                    ],
                );
            } else if self
                .invalid_literal_reason(&declared.name, default)
                .is_some()
            {
                self.push_diagnostic("TYPE_INVALID_LITERAL", source, default.range);
            }
        }
    }

    fn check_route_transport_bindings(
        &mut self,
        route: &jadpo_syntax::RouteDeclaration,
        source: &str,
    ) {
        let query_fields = route.query.as_ref().and_then(|query| {
            let resolved = resolved_type_value(query, self.graph);
            if resolved.nullable || !self.catalogue.records.contains_key(&resolved.name) {
                self.push_diagnostic("TYPE_ROUTE_QUERY_OBJECT_REQUIRED", source, query.range);
                None
            } else {
                let fields = self.catalogue.records.get(&resolved.name).cloned();
                if let Some(fields) = &fields {
                    for field in fields.values() {
                        let root = self.representation_root(&field.declared_type.name);
                        let supported = !field.declared_type.nullable
                            && field.declared_type.arguments.is_empty()
                            && (matches!(
                                root.as_deref(),
                                Some(
                                    "Text"
                                        | "Bool"
                                        | "Int"
                                        | "Decimal"
                                        | "Uuid"
                                        | "Instant"
                                        | "CalendarDate"
                                        | "Time"
                                        | "Duration"
                                        | "Email"
                                        | "Url"
                                        | "IpAddress"
                                )
                            ) || self
                                .catalogue
                                .records
                                .contains_key(&field.declared_type.name)
                                || self.enum_shape_name(&field.declared_type.name).is_some());
                        if !supported {
                            self.push_diagnostic(
                                "TYPE_ROUTE_QUERY_FIELD_UNSUPPORTED",
                                source,
                                query.range,
                            );
                        }
                    }
                }
                fields
            }
        });

        let mut bindings = Vec::new();
        bindings.extend(
            route
                .path_fields
                .iter()
                .map(|field| (field.name.text.clone(), field.name.range)),
        );
        if let (Some(query), Some(fields)) = (&route.query, &query_fields) {
            bindings.extend(fields.keys().map(|name| (name.clone(), query.range)));
        }
        if let Some(input) = &route.input {
            let resolved = resolved_type_value(input, self.graph);
            if let Some(fields) = self.catalogue.records.get(&resolved.name) {
                bindings.extend(fields.keys().map(|name| (name.clone(), input.range)));
            }
        }

        let mut wire_names = BTreeSet::new();
        for header in &route.headers {
            bindings.push((header.name.text.clone(), header.name.range));
            let wire_name = unquote(&header.wire_name.text);
            let folded = wire_name.to_ascii_lowercase();
            if !valid_http_header_name(&wire_name) {
                self.push_diagnostic(
                    "TYPE_ROUTE_HEADER_NAME_INVALID",
                    source,
                    header.wire_name.range,
                );
            }
            if !wire_names.insert(folded.clone()) {
                self.push_diagnostic(
                    "TYPE_ROUTE_HEADER_WIRE_DUPLICATE",
                    source,
                    header.wire_name.range,
                );
            }
            if reserved_route_header(&folded) {
                self.push_diagnostic(
                    "TYPE_AUTH_RESERVED_ROUTE_INPUT",
                    source,
                    header.wire_name.range,
                );
            }
            let resolved = resolved_type_value(&header.field_type, self.graph);
            let root = self.representation_root(&resolved.name);
            let supported = !resolved.nullable
                && resolved.arguments.is_empty()
                && (matches!(
                    root.as_deref(),
                    Some(
                        "Text"
                            | "Bool"
                            | "Int"
                            | "Decimal"
                            | "Uuid"
                            | "Instant"
                            | "CalendarDate"
                            | "Time"
                            | "Duration"
                            | "Email"
                            | "Url"
                            | "IpAddress"
                    )
                ) || self.enum_shape_name(&resolved.name).is_some());
            if !supported {
                self.push_diagnostic(
                    "TYPE_ROUTE_HEADER_SCALAR_REQUIRED",
                    source,
                    header.field_type.range,
                );
            }
        }

        let mut seen = BTreeSet::new();
        for (name, range) in bindings {
            if !seen.insert(name) {
                self.push_diagnostic("TYPE_ROUTE_BINDING_COLLISION", source, range);
            }
        }
    }

    fn check_authentication_contract(&mut self, files: &[ParsedSyntax]) {
        let Some(principal_name) = self.catalogue.principal.clone() else {
            return;
        };

        let mut reserved_fields = BTreeSet::new();
        reserved_fields.extend([
            "authorization".to_owned(),
            "proxy_authorization".to_owned(),
            "cookie".to_owned(),
            "origin".to_owned(),
            "sec_fetch_site".to_owned(),
            "x_jadpo_csrf".to_owned(),
        ]);
        let mut principal_required = BTreeSet::new();
        for file in files {
            for declaration in &file.file.declarations {
                match declaration {
                    Declaration::AuthenticationStrategy(strategy) => {
                        match &strategy.transport.location {
                            jadpo_syntax::CredentialLocation::Cookie(cookie) => {
                                reserved_fields
                                    .insert(normalize_transport_field(&unquote(&cookie.text)));
                            }
                            jadpo_syntax::CredentialLocation::Bearer(_) => {
                                reserved_fields.insert("authorization".to_owned());
                            }
                        }
                    }
                    Declaration::Callable(callable) => {
                        if callable.parameters.iter().any(|parameter| {
                            type_reference_is_principal(&parameter.parameter_type, &principal_name)
                        }) || block_uses_name(&callable.body, "principal")
                        {
                            principal_required.insert(callable.name.text.clone());
                        }
                        if type_reference_contains_principal(
                            &callable.return_type,
                            &principal_name,
                            &self.catalogue.records,
                            &mut BTreeSet::new(),
                        ) {
                            self.push_diagnostic(
                                "TYPE_AUTH_PRINCIPAL_OUTPUT",
                                &file.source_name,
                                callable.return_annotation_range,
                            );
                        }
                    }
                    Declaration::Record(record)
                        if record.kind == jadpo_syntax::RecordKind::Output =>
                    {
                        for field in &record.fields {
                            if type_reference_contains_principal(
                                &field.field_type,
                                &principal_name,
                                &self.catalogue.records,
                                &mut BTreeSet::new(),
                            ) {
                                self.push_diagnostic(
                                    "TYPE_AUTH_PRINCIPAL_OUTPUT",
                                    &file.source_name,
                                    field.field_type.range,
                                );
                            }
                        }
                    }
                    Declaration::Failure(failure) => {
                        for field in failure
                            .public_fields
                            .iter()
                            .chain(failure.internal_fields.iter())
                        {
                            if type_reference_contains_principal(
                                &field.field_type,
                                &principal_name,
                                &self.catalogue.records,
                                &mut BTreeSet::new(),
                            ) {
                                self.push_diagnostic(
                                    "TYPE_AUTH_PRINCIPAL_FAILURE_CONTEXT",
                                    &file.source_name,
                                    field.field_type.range,
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        loop {
            let mut changed = false;
            for edge in &self.graph.calls {
                let caller = &self.graph.nodes[edge.caller.0 as usize].name;
                let callee = &self.graph.nodes[edge.callee.0 as usize].name;
                if principal_required.contains(callee) && principal_required.insert(caller.clone())
                {
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        for file in files {
            for declaration in &file.file.declarations {
                let Declaration::Route(route) = declaration else {
                    continue;
                };
                for field in &route.path_fields {
                    if reserved_fields.contains(&normalize_transport_field(&field.name.text)) {
                        self.push_diagnostic(
                            "TYPE_AUTH_RESERVED_ROUTE_INPUT",
                            &file.source_name,
                            field.name.range,
                        );
                    }
                }
                if let Some(query) = &route.query {
                    let resolved = resolved_type_value(query, self.graph);
                    if self
                        .catalogue
                        .records
                        .get(&resolved.name)
                        .is_some_and(|fields| {
                            fields.keys().any(|field| {
                                reserved_fields.contains(&normalize_transport_field(field))
                            })
                        })
                    {
                        self.push_diagnostic(
                            "TYPE_AUTH_RESERVED_ROUTE_INPUT",
                            &file.source_name,
                            query.range,
                        );
                    }
                }
                if let Some(input) = &route.input {
                    if type_reference_contains_principal(
                        input,
                        &principal_name,
                        &self.catalogue.records,
                        &mut BTreeSet::new(),
                    ) {
                        self.push_diagnostic(
                            "TYPE_AUTH_PRINCIPAL_INPUT",
                            &file.source_name,
                            input.range,
                        );
                    }
                    if let Some(record_name) = input.path.first().map(|part| &part.text) {
                        if let Some(fields) = self.catalogue.records.get(record_name) {
                            let forbidden = fields.keys().find(|field| {
                                reserved_fields.contains(&normalize_transport_field(field))
                            });
                            if forbidden.is_some() {
                                self.push_diagnostic(
                                    "TYPE_AUTH_RESERVED_ROUTE_INPUT",
                                    &file.source_name,
                                    input.range,
                                );
                            }
                        }
                    }
                }
                if route.public {
                    let route_name = format!("{} {}", route_method_name(route.method), route.path);
                    if principal_required.contains(&route_name)
                        || route.run.as_ref().is_some_and(|run| {
                            run.arguments
                                .iter()
                                .any(|argument| expression_uses_name(argument, "current_principal"))
                                || run.named_arguments.iter().any(|argument| {
                                    expression_uses_name(&argument.value, "current_principal")
                                })
                        })
                        || route.inline_action.as_ref().is_some_and(|action| {
                            block_uses_name(&action.body, "principal")
                                || block_uses_name(&action.body, "current_principal")
                        })
                    {
                        self.push_diagnostic(
                            "TYPE_AUTH_PUBLIC_PRINCIPAL",
                            &file.source_name,
                            route.range,
                        );
                    }
                }
            }
        }
    }

    fn check_callable(&mut self, callable: &CallableDeclaration, source: &str) {
        let mut environment = self.base_environment();
        if matches!(
            callable.kind,
            jadpo_syntax::CallableKind::Action | jadpo_syntax::CallableKind::Query
        ) {
            if let Some(principal) = &self.catalogue.principal {
                environment.insert("principal".to_owned(), simple_type(principal));
            }
        }
        if callable.kind == jadpo_syntax::CallableKind::Action {
            environment.insert("clock".to_owned(), simple_type("__jadpo_clock"));
        }
        let mut mutable_bindings = BTreeSet::new();
        for parameter in &callable.parameters {
            let parameter_type = resolved_type_value(&parameter.parameter_type, self.graph);
            if self.is_primitive_signature_type(&parameter_type) {
                self.push_diagnostic_with_facts(
                    "TYPE_PRIMITIVE_SIGNATURE",
                    source,
                    parameter.range,
                    [
                        DiagnosticFact::Received(parameter_type.display()),
                        DiagnosticFact::Usage("parameter".to_owned()),
                    ],
                );
            }
            environment.insert(parameter.name.text.clone(), parameter_type);
        }

        let return_type = resolved_type_value(&callable.return_type, self.graph);
        if self.is_primitive_signature_type(&return_type) {
            self.push_diagnostic_with_facts(
                "TYPE_PRIMITIVE_SIGNATURE",
                source,
                callable.return_annotation_range,
                [
                    DiagnosticFact::Received(return_type.display()),
                    DiagnosticFact::Usage("return value".to_owned()),
                ],
            );
        }

        self.check_block(
            &callable.body,
            &mut environment,
            &mut mutable_bindings,
            &return_type,
            source,
        );
    }

    fn check_authentication_strategy(
        &mut self,
        strategy: &jadpo_syntax::AuthenticationStrategyDeclaration,
        source: &str,
    ) {
        if let Some(exchange) = &strategy.exchange {
            let key = strategy
                .validators
                .iter()
                .find(|validator| validator.name.text == exchange.key.text);
            let signed = strategy
                .validators
                .iter()
                .find(|validator| validator.name.text == exchange.signed.text);
            if !matches!(strategy.transport.location, jadpo_syntax::CredentialLocation::Bearer(ref location) if location.text == "authorization_header")
                || !key.is_some_and(|validator| {
                    validator.mode.text == "api_key"
                        && validator.principal.text == "service"
                        && validator.credentials.is_some()
                })
                || !signed.is_some_and(|validator| {
                    validator.mode.text == "signed" && validator.principal.text == "service"
                })
            {
                self.push_diagnostic("TYPE_AUTH_EXCHANGE_BINDING", source, exchange.range);
            }
            let path = exchange
                .path
                .text
                .strip_prefix('"')
                .and_then(|text| text.strip_suffix('"'))
                .unwrap_or_default();
            if !path.starts_with('/')
                || !path.split('/').skip(1).all(|part| {
                    !part.is_empty()
                        && part
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
                })
            {
                self.push_diagnostic("TYPE_AUTH_EXCHANGE_PATH", source, exchange.path.range);
            }
        }
        for validator in &strategy.validators {
            if let Some(binding) = &validator.credentials {
                self.check_authentication_credentials(strategy, validator, binding, source);
            }
            if validator.mode.text == "jwt"
                && !matches!(
                    strategy.transport.location,
                    jadpo_syntax::CredentialLocation::Bearer(_)
                )
            {
                self.push_diagnostic("TYPE_AUTH_JWT_TRANSPORT", source, validator.mode.range);
            }
            let mut seen = BTreeSet::new();
            for setting in &validator.settings {
                let name = setting.name.text.as_str();
                if name == "owner" {
                    let valid = match &setting.value {
                        Expression::Name(reference) if reference.path.len() == 2 => {
                            let entity = &reference.path[0].text;
                            let field = &reference.path[1].text;
                            validator.mode.text == "api_key"
                                && validator.principal.text == "service"
                                && strategy.resolutions.iter().any(|resolution| {
                                    resolution.principal.text == "service"
                                        && resolution
                                            .authority
                                            .path
                                            .first()
                                            .is_some_and(|n| &n.text == entity)
                                })
                                && self
                                    .catalogue
                                    .records
                                    .get(entity)
                                    .and_then(|fields| fields.get(field))
                                    .is_some_and(|field| {
                                        !field.optional
                                            && !field.declared_type.nullable
                                            && !field.declared_type.secret
                                    })
                                && self
                                    .catalogue
                                    .field_reference_targets
                                    .get(&(entity.clone(), field.clone()))
                                    .is_some_and(|target| {
                                        self.catalogue.persistent_entities.contains(target)
                                    })
                        }
                        _ => false,
                    };
                    if !valid || !seen.insert(name) {
                        self.push_diagnostic("TYPE_AUTH_ADAPTER_SETTING", source, setting.range);
                    }
                    continue;
                }
                let secret = matches!(name, "secret" | "previous_secret");
                let declared = match &setting.value {
                    Expression::Name(reference)
                        if reference.path.len() == 2 && reference.path[0].text == "config" =>
                    {
                        self.catalogue
                            .configuration
                            .get(&reference.path[1].text)
                            .map(|field| field.declared_type.clone())
                    }
                    Expression::Literal(literal)
                        if literal.kind == LiteralKind::String && !secret =>
                    {
                        Some(simple_type("Text"))
                    }
                    _ => None,
                };
                let valid = declared.as_ref().is_some_and(|value| {
                    value.secret == secret
                        && !value.nullable
                        && matches!(
                            self.representation_root(&value.name).as_deref(),
                            Some("Text" | "Url")
                        )
                });
                let allowed = if validator.mode.text == "jwt" {
                    matches!(name, "issuer" | "audience" | "jwks_uri")
                } else {
                    matches!(name, "secret" | "previous_secret" | "audience" | "origin")
                };
                if !allowed || !seen.insert(name) || !valid {
                    self.push_diagnostic("TYPE_AUTH_ADAPTER_SETTING", source, setting.range);
                }
            }
        }
        let mut filled = BTreeMap::<String, BTreeSet<String>>::new();
        for mapping in &strategy.claims {
            if mapping.target.path.len() != 3 {
                continue;
            }
            let variant = mapping.target.path[1].text.clone();
            let field = mapping.target.path[2].text.clone();
            if !matches!(
                mapping.source.text.as_str(),
                "subject" | "authentication_strength"
            ) {
                self.push_diagnostic("TYPE_AUTH_CLAIM_UNKNOWN", source, mapping.source.range);
            }
            if !matches!(field.as_str(), "subject" | "authentication_strength") {
                self.push_diagnostic(
                    "TYPE_AUTH_CLAIM_AUTHORITATIVE_FIELD",
                    source,
                    mapping.target.range,
                );
                continue;
            }
            filled.entry(variant).or_default().insert(field);
        }

        let mut resolutions = BTreeSet::new();
        for resolution in &strategy.resolutions {
            let variant = resolution.principal.text.clone();
            resolutions.insert(variant.clone());
            let Some(entity) = resolution
                .authority
                .path
                .first()
                .map(|name| name.text.clone())
            else {
                continue;
            };
            if !self.catalogue.persistent_entities.contains(&entity) {
                self.push_diagnostic(
                    "TYPE_AUTH_AUTHORITY_NOT_PERSISTENT",
                    source,
                    resolution.authority.range,
                );
            }
            if self
                .catalogue
                .failures
                .get(&resolution.inactive.text)
                .is_some_and(|kind| !matches!(kind.as_str(), "Rejected" | "NotPermitted"))
            {
                self.push_diagnostic(
                    "TYPE_AUTH_INACTIVE_FAILURE_KIND",
                    source,
                    resolution.inactive.range,
                );
            }
            if let Some(authority_field) = resolution.authority.path.get(1) {
                if !self
                    .catalogue
                    .ordered_keys
                    .get(&entity)
                    .is_some_and(|fields| fields.contains(&authority_field.text))
                {
                    self.push_diagnostic(
                        "TYPE_AUTH_AUTHORITY_NOT_UNIQUE",
                        source,
                        resolution.authority.range,
                    );
                }
            }
            let environment = self
                .catalogue
                .records
                .get(&entity)
                .map(|fields| {
                    fields
                        .iter()
                        .map(|(name, field)| (name.clone(), field.declared_type.clone()))
                        .collect::<BTreeMap<_, _>>()
                })
                .unwrap_or_default();
            if let Some(active) = self.infer_expression(&resolution.active, &environment, source) {
                self.require_compatible(
                    &active,
                    &simple_type("Bool"),
                    source,
                    resolution.active.range(),
                );
            }
            if self.catalogue.lifecycles.contains_key(&entity) {
                let lifecycle_fields = self
                    .catalogue
                    .records
                    .get(&entity)
                    .into_iter()
                    .flat_map(|fields| fields.iter())
                    .filter(|(_, field)| field.lifecycle_owned)
                    .map(|(name, _)| name.as_str())
                    .collect::<BTreeSet<_>>();
                let active_reads_unowned =
                    self.catalogue.records.get(&entity).is_some_and(|fields| {
                        fields.keys().any(|field| {
                            !lifecycle_fields.contains(field.as_str())
                                && expression_uses_name(&resolution.active, field)
                        })
                    });
                if active_reads_unowned {
                    self.push_diagnostic(
                        "TYPE_AUTH_LIFECYCLE_ACTIVE_FIELD",
                        source,
                        resolution.active.range(),
                    );
                }
                let authority_subject = resolution
                    .authority
                    .path
                    .get(1)
                    .map(|field| field.text.as_str());
                if resolution.mappings.iter().any(|mapping| {
                    Some(mapping.source.text.as_str()) != authority_subject
                        && !self
                            .catalogue
                            .identity_fields
                            .contains(&(entity.clone(), mapping.source.text.clone()))
                }) {
                    self.push_diagnostic(
                        "TYPE_AUTH_LIFECYCLE_MAPPING_SCOPE",
                        source,
                        resolution.range,
                    );
                }
            }

            let principal_name = self
                .catalogue
                .records
                .keys()
                .find(|name| name.ends_with(&format!(".{variant}")))
                .cloned();
            let principal_fields = principal_name
                .as_ref()
                .and_then(|name| self.catalogue.records.get(name))
                .cloned()
                .unwrap_or_default();
            for mapping in &resolution.mappings {
                let Some(source_field) = self
                    .catalogue
                    .records
                    .get(&entity)
                    .and_then(|fields| fields.get(&mapping.source.text))
                else {
                    continue;
                };
                let Some(target_field) = mapping
                    .target
                    .path
                    .last()
                    .and_then(|name| principal_fields.get(&name.text))
                else {
                    continue;
                };
                if !self
                    .assignment_compatible(&source_field.declared_type, &target_field.declared_type)
                {
                    self.push_diagnostic_with_facts(
                        "TYPE_AUTH_MAPPING_MISMATCH",
                        source,
                        mapping.range,
                        [
                            DiagnosticFact::Expected(target_field.declared_type.display()),
                            DiagnosticFact::Received(source_field.declared_type.display()),
                        ],
                    );
                }
                if let Some(target) = mapping.target.path.last() {
                    filled
                        .entry(variant.clone())
                        .or_default()
                        .insert(target.text.clone());
                }
            }
        }

        for validator in &strategy.validators {
            if matches!(validator.principal.text.as_str(), "user" | "service")
                && !resolutions.contains(&validator.principal.text)
            {
                self.push_diagnostic(
                    "TYPE_AUTH_RESOLUTION_REQUIRED",
                    source,
                    validator.principal.range,
                );
            }
        }

        for variant in &resolutions {
            let Some((_name, fields)) = self
                .catalogue
                .records
                .iter()
                .find(|(name, _)| name.ends_with(&format!(".{variant}")))
            else {
                continue;
            };
            let populated = filled.get(variant);
            for (field, contract) in fields {
                if !contract.optional
                    && !populated.is_some_and(|populated| populated.contains(field))
                {
                    self.push_diagnostic("TYPE_AUTH_RESOLUTION_INCOMPLETE", source, strategy.range);
                    break;
                }
            }
        }
    }

    fn check_authentication_credentials(
        &mut self,
        strategy: &jadpo_syntax::AuthenticationStrategyDeclaration,
        validator: &jadpo_syntax::AuthenticationValidatorDeclaration,
        binding: &jadpo_syntax::AuthenticationCredentialBinding,
        source: &str,
    ) {
        let entity = binding
            .identity
            .path
            .first()
            .map(|n| n.text.as_str())
            .unwrap_or_default();
        let authority = strategy
            .resolutions
            .iter()
            .find(|r| r.principal.text == "service")
            .and_then(|r| r.authority.path.first())
            .map(|n| n.text.as_str());
        let mut valid = validator.mode.text == "api_key"
            && validator.principal.text == "service"
            && self.catalogue.persistent_entities.contains(entity)
            && authority.is_some_and(|a| {
                self.catalogue.persistent_entities.contains(a)
                    && self.catalogue.authority_stores.get(entity)
                        == self.catalogue.authority_stores.get(a)
            });
        let mut seen = BTreeSet::new();
        for (role, reference) in [
            ("identity", &binding.identity),
            ("principal", &binding.principal),
            ("verifier", &binding.verifier),
            ("expires", &binding.expires),
            ("revoked", &binding.revoked),
        ] {
            if reference.path.len() != 2 || reference.path[0].text != entity {
                valid = false;
                continue;
            }
            let name = &reference.path[1].text;
            valid &= seen.insert(name.clone());
            let Some(field) = self
                .catalogue
                .records
                .get(entity)
                .and_then(|fields| fields.get(name))
            else {
                valid = false;
                continue;
            };
            let root = self.representation_root(&field.declared_type.name);
            let key = (entity.to_owned(), name.clone());
            valid &= !field.optional
                && (role == "identity" || field.generated.is_none())
                && !field.declared_type.secret
                && field.declared_type.nullable == (role == "revoked");
            valid &= match role {
                "identity" => {
                    root.as_deref() == Some("Uuid") && self.catalogue.identity_fields.contains(&key)
                }
                "principal" => self
                    .catalogue
                    .reference_fields
                    .get(&key)
                    .is_some_and(|target| {
                        target.len() == 2
                            && Some(target[0].as_str()) == authority
                            && self
                                .catalogue
                                .identity_fields
                                .contains(&(target[0].clone(), target[1].clone()))
                    }),
                "verifier" => {
                    root.as_deref() == Some("Text")
                        && self
                            .catalogue
                            .ordered_keys
                            .get(entity)
                            .is_some_and(|keys| keys.contains(name))
                }
                "expires" | "revoked" => root.as_deref() == Some("Instant"),
                _ => false,
            };
        }
        // Restrict issuance to a state the adapter can construct without executing
        // arbitrary application code or inventing values for other fields.
        let active_valid = if let Expression::Binary(binary) = &binding.active {
            if binary.operator != jadpo_syntax::BinaryOperator::Equal {
                false
            } else if let Expression::Name(name) = binary.left.as_ref() {
                if name.path.len() != 1 || !seen.insert(name.path[0].text.clone()) {
                    false
                } else if let Some(field) = self
                    .catalogue
                    .records
                    .get(entity)
                    .and_then(|fields| fields.get(&name.path[0].text))
                {
                    !field.optional
                        && field.generated.is_none()
                        && !field.declared_type.nullable
                        && !field.declared_type.secret
                        && match binary.right.as_ref() {
                            Expression::Literal(literal) => {
                                literal.kind == LiteralKind::Boolean
                                    && self
                                        .representation_root(&field.declared_type.name)
                                        .as_deref()
                                        == Some("Bool")
                            }
                            Expression::Name(variant) => {
                                variant.path.len() == 2
                                    && variant.path[0].text == field.declared_type.name
                                    && self
                                        .catalogue
                                        .enums
                                        .get(&variant.path[0].text)
                                        .and_then(|variants| variants.get(&variant.path[1].text))
                                        .is_some_and(|fields| fields.is_empty())
                            }
                            _ => false,
                        }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        if !valid || !active_valid {
            self.push_diagnostic("TYPE_AUTH_ADAPTER_SETTING", source, binding.range);
        }
    }

    fn check_configuration(
        &mut self,
        configuration: &jadpo_syntax::ConfigDeclaration,
        source: &str,
    ) {
        for field in &configuration.fields {
            let expected = resolved_type_value(&field.field_type, self.graph);
            let expected_root = self.representation_root(&expected.name);
            if !matches!(
                expected_root.as_deref(),
                Some(
                    "Text"
                        | "Bool"
                        | "Int"
                        | "Decimal"
                        | "Uuid"
                        | "Instant"
                        | "CalendarDate"
                        | "Duration"
                        | "Email"
                        | "Url"
                        | "IpAddress"
                )
            ) {
                self.push_diagnostic("CONFIG_TYPE_UNSUPPORTED", source, field.field_type.range);
            }
            let Some(default) = &field.default else {
                continue;
            };
            let received_root = match default.kind {
                ConfigDefaultKind::String => "Text",
                ConfigDefaultKind::Integer => "Int",
                ConfigDefaultKind::Decimal => "Decimal",
                ConfigDefaultKind::Boolean => "Bool",
                ConfigDefaultKind::Duration => "Duration",
            };
            let expected_representation = match expected_root.as_deref() {
                Some(
                    "Text" | "Uuid" | "Instant" | "CalendarDate" | "Email" | "Url" | "IpAddress",
                ) => Some("Text"),
                other => other,
            };
            if expected.nullable || expected_representation != Some(received_root) {
                self.push_diagnostic_with_facts(
                    "CONFIG_DEFAULT_TYPE",
                    source,
                    default.range,
                    [
                        DiagnosticFact::Expected(expected.display()),
                        DiagnosticFact::Received(received_root.to_owned()),
                    ],
                );
                continue;
            }
            let literal_kind = match default.kind {
                ConfigDefaultKind::String | ConfigDefaultKind::Duration => LiteralKind::String,
                ConfigDefaultKind::Integer => LiteralKind::Integer,
                ConfigDefaultKind::Decimal => LiteralKind::Decimal,
                ConfigDefaultKind::Boolean => LiteralKind::Boolean,
            };
            let literal = Literal {
                kind: literal_kind,
                text: default.text.clone(),
                range: default.range,
            };
            if default.kind != ConfigDefaultKind::Duration
                && self
                    .invalid_literal_reason(&expected.name, &literal)
                    .is_some()
            {
                self.push_diagnostic("CONFIG_DEFAULT_INVALID", source, default.range);
            }
        }
    }

    fn check_fixture_configuration(
        &mut self,
        values: &[jadpo_syntax::FixtureConfigValue],
        environment: &BTreeMap<String, TypeValue>,
        source: &str,
        fixture_range: TextRange,
    ) {
        let mut seen = BTreeSet::new();
        for value in values {
            if !seen.insert(value.name.text.clone()) {
                self.push_diagnostic("CONFIG_FIXTURE_DUPLICATE", source, value.name.range);
                continue;
            }
            let Some(field) = self.catalogue.configuration.get(&value.name.text).cloned() else {
                self.push_diagnostic_with_facts(
                    "CONFIG_FIELD_UNKNOWN",
                    source,
                    value.name.range,
                    [DiagnosticFact::Field(value.name.text.clone())],
                );
                continue;
            };
            if field.declared_type.secret && !value.secret {
                self.push_diagnostic("CONFIG_FIXTURE_SECRET_REQUIRED", source, value.range);
            } else if !field.declared_type.secret && value.secret {
                self.push_diagnostic("CONFIG_FIXTURE_SECRET_UNEXPECTED", source, value.range);
            }

            if let Expression::Literal(literal) = &value.value {
                let received = self.literal_type(literal);
                let received_root = received
                    .as_ref()
                    .and_then(|received| self.representation_root(&received.name));
                let expected_root = self.representation_root(&field.declared_type.name);
                let contextual_temporal_text = received_root.as_deref() == Some("Text")
                    && matches!(
                        expected_root.as_deref(),
                        Some("Instant" | "CalendarDate" | "Duration")
                    );
                if received_root == expected_root || contextual_temporal_text {
                    if self
                        .invalid_literal_reason(&field.declared_type.name, literal)
                        .is_some()
                    {
                        self.push_diagnostic("CONFIG_FIXTURE_VALUE_INVALID", source, literal.range);
                    }
                    continue;
                }
            }

            if let Some(received) = self.infer_expression(&value.value, environment, source) {
                self.require_compatible(
                    &received,
                    &field.declared_type,
                    source,
                    value.value.range(),
                );
            }
        }

        let required = self
            .catalogue
            .configuration
            .iter()
            .filter(|(_, field)| !field.optional)
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        for name in required {
            if !seen.contains(&name) {
                self.push_diagnostic_with_facts(
                    "CONFIG_FIXTURE_VALUE_MISSING",
                    source,
                    fixture_range,
                    [DiagnosticFact::Field(name)],
                );
            }
        }
    }

    fn check_block(
        &mut self,
        block: &Block,
        environment: &mut BTreeMap<String, TypeValue>,
        mutable_bindings: &mut BTreeSet<String>,
        return_type: &TypeValue,
        source: &str,
    ) {
        let parent_range_inputs = self.direct_range_inputs.clone();
        if self.direct_range_inputs.is_none() {
            self.direct_range_inputs = Some(
                environment
                    .iter()
                    .filter_map(|(name, value)| {
                        (!value.nullable
                            && !value.secret
                            && !matches!(
                                name.as_str(),
                                "principal" | "current_principal" | "clock" | "config"
                            )
                            && self.catalogue.record_kinds.get(&value.name)
                                == Some(&jadpo_syntax::RecordKind::Input))
                        .then(|| name.clone())
                    })
                    .collect(),
            );
        }
        for statement in &block.statements {
            match statement {
                Statement::Binding(binding) => {
                    let inferred = self.infer_expression(&binding.value, environment, source);
                    if let Some(inputs) = &mut self.direct_range_inputs {
                        inputs.remove(&binding.name.text);
                    }
                    let binding_type = if let Some(annotation) = &binding.annotation {
                        let expected = resolved_type_value(annotation, self.graph);
                        if let Some(received) = &inferred {
                            self.require_compatible(
                                received,
                                &expected,
                                source,
                                binding.value.range(),
                            );
                        }
                        expected
                    } else if let Some(inferred) = inferred {
                        inferred
                    } else {
                        continue;
                    };
                    environment.insert(binding.name.text.clone(), binding_type);
                    if binding.mutable {
                        mutable_bindings.insert(binding.name.text.clone());
                    } else {
                        mutable_bindings.remove(&binding.name.text);
                    }
                }
                Statement::Assignment(assignment) => {
                    if let Some(inputs) = &mut self.direct_range_inputs {
                        inputs.remove(&assignment.target.text);
                    }
                    let expected = environment.get(&assignment.target.text).cloned();
                    if expected.is_none() {
                        self.push_diagnostic("TYPE_ASSIGN_UNKNOWN", source, assignment.range);
                    } else if !mutable_bindings.contains(&assignment.target.text) {
                        self.push_diagnostic("TYPE_ASSIGN_IMMUTABLE", source, assignment.range);
                    }
                    if let (Some(received), Some(expected)) = (
                        self.infer_expression(&assignment.value, environment, source),
                        expected,
                    ) {
                        self.require_compatible(
                            &received,
                            &expected,
                            source,
                            assignment.value.range(),
                        );
                    }
                }
                Statement::Return(statement) => {
                    if let Some(received) =
                        self.infer_expression(&statement.value, environment, source)
                    {
                        self.require_compatible(
                            &received,
                            return_type,
                            source,
                            statement.value.range(),
                        );
                    }
                }
                Statement::Reject(statement) => {
                    self.check_reject_fields(
                        &statement.failure.text,
                        &statement.values,
                        environment,
                        source,
                    );
                }
                Statement::If(statement) => {
                    if let Some(condition) =
                        self.infer_expression(&statement.condition, environment, source)
                    {
                        self.require_compatible(
                            &condition,
                            &simple_type("Bool"),
                            source,
                            statement.condition.range(),
                        );
                    }
                    let mut then_environment = environment.clone();
                    let mut then_mutable_bindings = mutable_bindings.clone();
                    self.check_block(
                        &statement.then_block,
                        &mut then_environment,
                        &mut then_mutable_bindings,
                        return_type,
                        source,
                    );
                    if let Some(else_block) = &statement.else_block {
                        let mut else_environment = environment.clone();
                        let mut else_mutable_bindings = mutable_bindings.clone();
                        self.check_block(
                            else_block,
                            &mut else_environment,
                            &mut else_mutable_bindings,
                            return_type,
                            source,
                        );
                    }
                }
                Statement::Match(statement) => {
                    self.check_match_statement(
                        statement,
                        environment,
                        mutable_bindings,
                        return_type,
                        source,
                    );
                }
                Statement::Assert(statement) => {
                    if let Some(condition) =
                        self.infer_expression(&statement.condition, environment, source)
                    {
                        self.require_compatible(
                            &condition,
                            &simple_type("Bool"),
                            source,
                            statement.condition.range(),
                        );
                    }
                }
                Statement::AdvanceClock(statement) => {
                    if !environment.contains_key("__jadpo_test_clock") {
                        self.push_diagnostic("TYPE_TEST_CLOCK_CONTEXT", source, statement.range);
                    }
                    if let Some(duration) =
                        self.infer_expression(&statement.duration, environment, source)
                    {
                        self.require_compatible(
                            &duration,
                            &simple_type("Duration"),
                            source,
                            statement.duration.range(),
                        );
                    }
                }
                Statement::Unsupported(_) => {}
            }
        }
        self.direct_range_inputs = parent_range_inputs;
    }

    fn check_match_statement(
        &mut self,
        statement: &jadpo_syntax::MatchStatement,
        environment: &BTreeMap<String, TypeValue>,
        mutable_bindings: &BTreeSet<String>,
        return_type: &TypeValue,
        source: &str,
    ) {
        let subject_type = self.infer_expression(&statement.subject, environment, source);
        if subject_type.as_ref().is_some_and(|subject| subject.secret) {
            self.push_diagnostic("CONFIG_SECRET_FLOW", source, statement.subject.range());
        }
        let enum_name = subject_type
            .as_ref()
            .and_then(|subject| self.enum_shape_name(&subject.name));
        let mut seen = BTreeSet::new();
        let mut wildcard_seen = false;
        let mut some_seen = false;

        for arm in &statement.arms {
            let mut pattern_bindings = Vec::new();
            if wildcard_seen {
                self.push_diagnostic(
                    "TYPE_MATCH_UNREACHABLE_PATTERN",
                    source,
                    arm.pattern.range(),
                );
            }
            match &arm.pattern {
                jadpo_syntax::MatchPattern::Wildcard(_) => {
                    if !seen.insert("_".to_owned()) {
                        self.push_diagnostic(
                            "TYPE_MATCH_DUPLICATE_PATTERN",
                            source,
                            arm.pattern.range(),
                        );
                    }
                    wildcard_seen = true;
                }
                jadpo_syntax::MatchPattern::Name(pattern) => {
                    let pattern_name = joined_name(&pattern.path);
                    if let Some(expected_enum) = enum_name.as_deref() {
                        let expected_prefix = format!("{expected_enum}.");
                        if let Some(variant) = pattern_name.strip_prefix(&expected_prefix) {
                            if variant.contains('.')
                                || !self
                                    .catalogue
                                    .enums
                                    .get(expected_enum)
                                    .is_some_and(|variants| variants.contains_key(variant))
                            {
                                self.push_diagnostic(
                                    "TYPE_MATCH_UNKNOWN_VARIANT",
                                    source,
                                    pattern.range,
                                );
                            } else if !seen.insert(variant.to_owned()) {
                                self.push_diagnostic(
                                    "TYPE_MATCH_DUPLICATE_PATTERN",
                                    source,
                                    pattern.range,
                                );
                            } else if self
                                .catalogue
                                .enums
                                .get(expected_enum)
                                .and_then(|variants| variants.get(variant))
                                .is_some_and(|fields| !fields.is_empty())
                            {
                                self.push_diagnostic(
                                    "TYPE_MATCH_VARIANT_BINDINGS_REQUIRED",
                                    source,
                                    pattern.range,
                                );
                            }
                        } else {
                            self.push_diagnostic("TYPE_MATCH_PATTERN_TYPE", source, pattern.range);
                        }
                    } else {
                        self.push_diagnostic("TYPE_MATCH_PATTERN_TYPE", source, pattern.range);
                    }
                }
                jadpo_syntax::MatchPattern::Variant(pattern) => {
                    let pattern_name = joined_name(&pattern.target.path);
                    if let Some(expected_enum) = enum_name.as_deref() {
                        let expected_prefix = format!("{expected_enum}.");
                        if let Some(variant) = pattern_name.strip_prefix(&expected_prefix) {
                            let fields = self
                                .catalogue
                                .enums
                                .get(expected_enum)
                                .and_then(|variants| variants.get(variant))
                                .cloned();
                            if variant.contains('.') || fields.is_none() {
                                self.push_diagnostic(
                                    "TYPE_MATCH_UNKNOWN_VARIANT",
                                    source,
                                    pattern.target.range,
                                );
                            } else {
                                if !seen.insert(variant.to_owned()) {
                                    self.push_diagnostic(
                                        "TYPE_MATCH_DUPLICATE_PATTERN",
                                        source,
                                        pattern.target.range,
                                    );
                                }
                                let fields = fields.unwrap_or_default();
                                let mut bound = BTreeSet::new();
                                for binding in &pattern.bindings {
                                    if !bound.insert(binding.text.clone()) {
                                        self.push_diagnostic(
                                            "TYPE_MATCH_DUPLICATE_BINDING",
                                            source,
                                            binding.range,
                                        );
                                    } else if let Some(field) = fields.get(&binding.text) {
                                        pattern_bindings.push((
                                            binding.text.clone(),
                                            TypeValue {
                                                name: format!(
                                                    "{expected_enum}.{variant}.{}",
                                                    binding.text
                                                ),
                                                arguments: field.declared_type.arguments.clone(),
                                                nullable: field.declared_type.nullable,
                                                secret: field.declared_type.secret,
                                            },
                                        ));
                                    } else {
                                        self.push_diagnostic(
                                            "TYPE_MATCH_UNKNOWN_BINDING",
                                            source,
                                            binding.range,
                                        );
                                    }
                                }
                            }
                        } else {
                            self.push_diagnostic(
                                "TYPE_MATCH_PATTERN_TYPE",
                                source,
                                pattern.target.range,
                            );
                        }
                    } else {
                        self.push_diagnostic(
                            "TYPE_MATCH_PATTERN_TYPE",
                            source,
                            pattern.target.range,
                        );
                    }
                }
                jadpo_syntax::MatchPattern::OptionalSome(pattern) => {
                    let Some(subject) = subject_type.as_ref() else {
                        continue;
                    };
                    if !subject.nullable {
                        self.push_diagnostic("TYPE_MATCH_SOME_NON_OPTIONAL", source, pattern.range);
                    }
                    if some_seen {
                        self.push_diagnostic("TYPE_MATCH_DUPLICATE_PATTERN", source, pattern.range);
                    }
                    some_seen = true;
                    pattern_bindings.push((
                        pattern.binding.text.clone(),
                        TypeValue {
                            nullable: false,
                            ..subject.clone()
                        },
                    ));
                }
                jadpo_syntax::MatchPattern::Literal(pattern) => {
                    if let (Some(subject), Some(pattern_type)) =
                        (subject_type.as_ref(), self.literal_type(pattern))
                    {
                        let root = self.representation_root(&subject.name);
                        let nullable_none = pattern_type.name == "none" && subject.nullable;
                        if !nullable_none && root.as_deref() != Some(pattern_type.name.as_str()) {
                            self.push_diagnostic("TYPE_MATCH_PATTERN_TYPE", source, pattern.range);
                        }
                    }
                    if !seen.insert(pattern.text.clone()) {
                        self.push_diagnostic("TYPE_MATCH_DUPLICATE_PATTERN", source, pattern.range);
                    }
                }
            }

            let parent_range_inputs = self.direct_range_inputs.clone();
            if let Some(inputs) = &mut self.direct_range_inputs {
                for (name, _) in &pattern_bindings {
                    inputs.remove(name);
                }
            }
            let mut arm_environment = environment.clone();
            arm_environment.extend(pattern_bindings);
            let mut arm_mutable_bindings = mutable_bindings.clone();
            self.check_block(
                &arm.body,
                &mut arm_environment,
                &mut arm_mutable_bindings,
                return_type,
                source,
            );
            self.direct_range_inputs = parent_range_inputs;
        }

        if wildcard_seen {
            return;
        }
        if let Some(enum_name) = enum_name {
            let mut missing = self
                .catalogue
                .enums
                .get(&enum_name)
                .into_iter()
                .flat_map(|variants| variants.keys())
                .filter(|variant| !some_seen && !seen.contains(*variant))
                .cloned()
                .collect::<Vec<_>>();
            if subject_type
                .as_ref()
                .is_some_and(|subject| subject.nullable)
                && !seen.contains("none")
            {
                missing.push("none".to_owned());
            }
            if !missing.is_empty() {
                self.push_diagnostic("TYPE_MATCH_NON_EXHAUSTIVE", source, statement.range);
            }
        } else if let Some(subject) = subject_type {
            if self.representation_root(&subject.name).as_deref() == Some("Bool") {
                let mut missing = ["true", "false"]
                    .into_iter()
                    .filter(|value| !some_seen && !seen.contains(*value))
                    .collect::<Vec<_>>();
                if subject.nullable && !seen.contains("none") {
                    missing.push("none");
                }
                if !missing.is_empty() {
                    self.push_diagnostic("TYPE_MATCH_NON_EXHAUSTIVE", source, statement.range);
                }
            } else if !(subject.nullable && some_seen && seen.contains("none")) {
                self.push_diagnostic("TYPE_MATCH_WILDCARD_REQUIRED", source, statement.range);
            }
        }
    }

    fn infer_expression(
        &mut self,
        expression: &Expression,
        environment: &BTreeMap<String, TypeValue>,
        source: &str,
    ) -> Option<TypeValue> {
        let inferred = match expression {
            Expression::Literal(literal) => self.literal_type(literal),
            Expression::Name(name) => {
                if name.path.len() == 2
                    && name.path[0].text == "clock"
                    && name.path[1].text == "now"
                {
                    self.result.clock_reads.push(ClockRead {
                        source: source.to_owned(),
                        range: name.range,
                    });
                }
                self.resolve_value_path(&name.path, environment, source)
            }
            Expression::Invocation(invocation) => {
                self.infer_invocation(invocation, environment, source)
            }
            Expression::TestCall(call) => {
                if !environment.contains_key("__jadpo_test_clock") {
                    self.push_diagnostic("TYPE_TEST_CALL_CONTEXT", source, call.range);
                }
                self.infer_invocation(&call.invocation, environment, source)
            }
            Expression::Construction(construction) => {
                let target = joined_name(&construction.target.path);
                if let Some((enum_name, _variant, fields)) = self.enum_variant(&target) {
                    self.check_variant_construction(
                        &target,
                        &fields,
                        &construction.fields,
                        construction.range,
                        environment,
                        source,
                    );
                    Some(simple_type(&enum_name))
                } else {
                    self.check_construction(
                        &target,
                        &construction.fields,
                        construction.range,
                        environment,
                        source,
                        false,
                    );
                    Some(simple_type(&target))
                }
            }
            Expression::Object(object) => {
                for field in &object.fields {
                    self.infer_expression(&field.value, environment, source);
                }
                Some(simple_type("Object"))
            }
            Expression::Create(create) => {
                let target = joined_name(&create.target.path);
                if !self.catalogue.entities.contains(&target) {
                    self.push_diagnostic("TYPE_CREATE_NOT_ENTITY", source, create.target.range);
                }
                self.check_construction(
                    &target,
                    &create.fields,
                    create.range,
                    environment,
                    source,
                    true,
                );
                self.check_conflict_bindings(&target, &create.conflicts, environment, source);
                Some(simple_type(&target))
            }
            Expression::Query(query) => {
                let target = joined_name(&query.target.path);
                if !self.catalogue.entities.contains(&target) {
                    self.push_diagnostic("TYPE_QUERY_NOT_ENTITY", source, query.target.range);
                }
                if let Some(page) = &query.page {
                    self.check_query_page(page, &target, environment, source);
                    return Some(resolved_type_value(&page.result, self.graph));
                }
                let expected = self
                    .catalogue
                    .records
                    .get(&target)
                    .and_then(|fields| fields.get(&query.field.text))
                    .cloned();
                if let Some(expected) = expected {
                    if expected.declared_type.nullable {
                        self.push_diagnostic(
                            "TYPE_QUERY_NULLABLE_FIELD_UNSUPPORTED",
                            source,
                            query.field.range,
                        );
                    }
                    if let Some(received) = self.infer_expression(&query.value, environment, source)
                    {
                        let field_type = TypeValue {
                            name: format!("{target}.{}", query.field.text),
                            arguments: expected.declared_type.arguments,
                            nullable: expected.declared_type.nullable,
                            secret: expected.declared_type.secret,
                        };
                        self.require_compatible(
                            &received,
                            &field_type,
                            source,
                            query.value.range(),
                        );
                    }
                } else {
                    if self.catalogue.records.contains_key(&target) {
                        self.push_diagnostic("TYPE_QUERY_UNKNOWN_FIELD", source, query.field.range);
                    }
                    self.infer_expression(&query.value, environment, source);
                }
                if let Some(missing) = &query.missing {
                    self.check_reject_fields(
                        &missing.failure.text,
                        &missing.values,
                        environment,
                        source,
                    );
                }
                if let Some(order) = &query.order {
                    if !self
                        .catalogue
                        .records
                        .get(&target)
                        .is_some_and(|fields| fields.contains_key(&order.field.text))
                    {
                        self.push_diagnostic(
                            "TYPE_QUERY_UNKNOWN_ORDER_FIELD",
                            source,
                            order.field.range,
                        );
                    } else if !self
                        .catalogue
                        .ordered_keys
                        .get(&target)
                        .is_some_and(|fields| fields.contains(&order.field.text))
                    {
                        self.push_diagnostic(
                            "TYPE_QUERY_ORDER_NOT_DETERMINISTIC",
                            source,
                            order.field.range,
                        );
                    }
                }
                if let Some(pagination) = &query.pagination {
                    self.check_query_pagination(pagination, environment, source);
                }
                let nested_result = query.includes.first().and_then(|first_include| {
                    let nested_name = first_include.nested_relationship.as_ref()?;
                    let result_name = joined_name(&first_include.result.path);
                    if query.includes.len() != 1 {
                        self.push_diagnostic(
                            "TYPE_NESTED_INCLUDE_SINGLE",
                            source,
                            first_include.range,
                        );
                    }
                    if query.cardinality != jadpo_syntax::QueryCardinality::Required {
                        self.push_diagnostic(
                            "TYPE_NESTED_INCLUDE_REQUIRED_QUERY",
                            source,
                            first_include.range,
                        );
                    }
                    if first_include.cardinality != jadpo_syntax::QueryIncludeCardinality::Optional
                    {
                        self.push_diagnostic(
                            "TYPE_NESTED_INCLUDE_CARDINALITY",
                            source,
                            first_include.range,
                        );
                    }
                    let reference = self
                        .catalogue
                        .owning_references
                        .get(&target)
                        .and_then(|references| references.get(&first_include.relationship.text))
                        .cloned();
                    let Some(reference) = reference else {
                        self.push_diagnostic(
                            "TYPE_NESTED_INCLUDE_FIRST_HOP",
                            source,
                            first_include.relationship.range,
                        );
                        return Some(simple_type(&result_name));
                    };
                    if reference.nullable {
                        self.push_diagnostic(
                            "TYPE_NESTED_INCLUDE_NULLABLE_FIRST_HOP",
                            source,
                            first_include.relationship.range,
                        );
                    }
                    let nested_inverse = self
                        .catalogue
                        .inverses
                        .get(&reference.parent)
                        .and_then(|relationships| relationships.get(&nested_name.text))
                        .filter(|inverse| {
                            inverse.cardinality == jadpo_syntax::InverseCardinality::Optional
                        })
                        .cloned();
                    let Some(nested_inverse) = nested_inverse else {
                        self.push_diagnostic(
                            "TYPE_NESTED_INCLUDE_SECOND_HOP",
                            source,
                            nested_name.range,
                        );
                        return Some(simple_type(&result_name));
                    };
                    if !self.catalogue.is_projection_object(&result_name) {
                        self.push_diagnostic(
                            "TYPE_INCLUDE_RESULT_NOT_OUTPUT",
                            source,
                            first_include.result.range,
                        );
                    }
                    let outer_shape = self.catalogue.records.get(&result_name);
                    let inner_type = outer_shape
                        .and_then(|fields| fields.get(&first_include.relationship.text))
                        .map(|field| field.declared_type.clone());
                    let outer_parent_ok = outer_shape
                        .and_then(|fields| fields.get("parent"))
                        .is_some_and(|field| field.declared_type == simple_type(&target));
                    let inner_name = inner_type
                        .as_ref()
                        .filter(|field_type| {
                            !field_type.nullable && field_type.arguments.is_empty()
                        })
                        .map(|field_type| field_type.name.clone());
                    let inner_output_ok = inner_name
                        .as_ref()
                        .is_some_and(|name| self.catalogue.is_projection_object(name));
                    let inner_shape = inner_name
                        .as_ref()
                        .and_then(|name| self.catalogue.records.get(name));
                    let inner_parent_ok = inner_shape
                        .and_then(|fields| fields.get("parent"))
                        .is_some_and(|field| field.declared_type == simple_type(&reference.parent));
                    let mut leaf_type = simple_type(&nested_inverse.child);
                    leaf_type.nullable = true;
                    let inner_leaf_ok = inner_shape
                        .and_then(|fields| fields.get(&nested_name.text))
                        .is_some_and(|field| field.declared_type == leaf_type);
                    if outer_shape.map_or(true, |fields| fields.len() != 2)
                        || !outer_parent_ok
                        || !inner_output_ok
                        || inner_shape.map_or(true, |fields| fields.len() != 2)
                        || !inner_parent_ok
                        || !inner_leaf_ok
                    {
                        self.push_diagnostic(
                            "TYPE_NESTED_INCLUDE_RESULT_SHAPE",
                            source,
                            first_include.result.range,
                        );
                    }
                    Some(simple_type(&result_name))
                });
                let inverse_one_result = query.includes.first().and_then(|first_include| {
                    if first_include.nested_relationship.is_some() {
                        return None;
                    }
                    let inverse = self
                        .catalogue
                        .inverses
                        .get(&target)
                        .and_then(|relationships| {
                            relationships.get(&first_include.relationship.text)
                        })
                        .filter(|inverse| {
                            inverse.cardinality == jadpo_syntax::InverseCardinality::Optional
                        })
                        .cloned()?;
                    let result_name = joined_name(&first_include.result.path);
                    if query.includes.len() != 1 {
                        self.push_diagnostic(
                            "TYPE_INVERSE_ONE_SINGLE",
                            source,
                            first_include.range,
                        );
                    }
                    if query.cardinality != jadpo_syntax::QueryCardinality::Required {
                        self.push_diagnostic(
                            "TYPE_INVERSE_ONE_REQUIRED_QUERY",
                            source,
                            first_include.range,
                        );
                    }
                    if first_include.cardinality != jadpo_syntax::QueryIncludeCardinality::Optional
                    {
                        self.push_diagnostic(
                            "TYPE_INVERSE_ONE_CARDINALITY",
                            source,
                            first_include.range,
                        );
                    }
                    if !self.catalogue.is_projection_object(&result_name) {
                        self.push_diagnostic(
                            "TYPE_INCLUDE_RESULT_NOT_OUTPUT",
                            source,
                            first_include.result.range,
                        );
                    }
                    let mut related_type = simple_type(&inverse.child);
                    related_type.nullable = true;
                    let shape = self.catalogue.records.get(&result_name);
                    let parent_ok = shape
                        .and_then(|fields| fields.get("parent"))
                        .is_some_and(|field| field.declared_type == simple_type(&target));
                    let related_ok = shape
                        .and_then(|fields| fields.get(&first_include.relationship.text))
                        .is_some_and(|field| field.declared_type == related_type);
                    if shape.map_or(true, |fields| fields.len() != 2) || !parent_ok || !related_ok {
                        self.push_diagnostic(
                            "TYPE_INVERSE_ONE_RESULT_SHAPE",
                            source,
                            first_include.result.range,
                        );
                    }
                    Some(simple_type(&result_name))
                });
                let owning_result = if nested_result.is_none() && inverse_one_result.is_none() {
                    query.includes.first().and_then(|first_include| {
                        if first_include.cardinality == jadpo_syntax::QueryIncludeCardinality::Many
                        {
                            return None;
                        }
                        let result_name = joined_name(&first_include.result.path);
                        if query.includes.len() != 1 {
                            self.push_diagnostic(
                                "TYPE_PARENT_INCLUDE_SINGLE",
                                source,
                                first_include.range,
                            );
                        }
                        let many_required_parent = query.cardinality
                            == jadpo_syntax::QueryCardinality::Many
                            && first_include.cardinality
                                == jadpo_syntax::QueryIncludeCardinality::Required;
                        if query.cardinality != jadpo_syntax::QueryCardinality::Required
                            && !many_required_parent
                        {
                            self.push_diagnostic(
                                "TYPE_PARENT_INCLUDE_REQUIRED_QUERY",
                                source,
                                first_include.range,
                            );
                        }
                        if many_required_parent && query.pagination.is_none() {
                            self.push_diagnostic(
                                "TYPE_INCLUDE_PARENT_PAGINATION_REQUIRED",
                                source,
                                first_include.range,
                            );
                        }
                        let reference = self
                            .catalogue
                            .owning_references
                            .get(&target)
                            .and_then(|references| references.get(&first_include.relationship.text))
                            .cloned();
                        let Some(reference) = reference else {
                            self.push_diagnostic(
                                "TYPE_PARENT_INCLUDE_UNKNOWN_REFERENCE",
                                source,
                                first_include.relationship.range,
                            );
                            return Some(simple_type(&result_name));
                        };
                        if first_include.cardinality
                            == jadpo_syntax::QueryIncludeCardinality::Required
                            && reference.nullable
                        {
                            self.push_diagnostic(
                                "TYPE_PARENT_INCLUDE_NULLABLE_REFERENCE",
                                source,
                                first_include.range,
                            );
                        }
                        if !self.catalogue.is_projection_object(&result_name) {
                            self.push_diagnostic(
                                "TYPE_INCLUDE_RESULT_NOT_OUTPUT",
                                source,
                                first_include.result.range,
                            );
                        }
                        let mut related_type = simple_type(&reference.parent);
                        related_type.nullable = first_include.cardinality
                            == jadpo_syntax::QueryIncludeCardinality::Optional;
                        let shape = self.catalogue.records.get(&result_name);
                        let parent_ok = shape
                            .and_then(|fields| fields.get("parent"))
                            .is_some_and(|field| field.declared_type == simple_type(&target));
                        let related_ok = shape
                            .and_then(|fields| fields.get(&first_include.relationship.text))
                            .is_some_and(|field| field.declared_type == related_type);
                        if shape.map_or(true, |fields| fields.len() != 2)
                            || !parent_ok
                            || !related_ok
                        {
                            self.push_diagnostic(
                                "TYPE_PARENT_INCLUDE_RESULT_SHAPE",
                                source,
                                first_include.result.range,
                            );
                        }
                        if many_required_parent {
                            Some(TypeValue {
                                name: "List".to_owned(),
                                arguments: vec![simple_type(&result_name)],
                                nullable: false,
                                secret: false,
                            })
                        } else {
                            Some(simple_type(&result_name))
                        }
                    })
                } else {
                    None
                };
                let included_result = nested_result
                    .or(inverse_one_result)
                    .or(owning_result)
                    .or_else(|| {
                        query.includes.first().map(|first_include| {
                            let result_name = first_include
                                .result
                                .path
                                .iter()
                                .map(|part| part.text.as_str())
                                .collect::<Vec<_>>()
                                .join(".");
                            let included_type = || {
                                if query.cardinality == jadpo_syntax::QueryCardinality::Many {
                                    TypeValue {
                                        name: "List".to_owned(),
                                        arguments: vec![simple_type(&result_name)],
                                        nullable: false,
                                        secret: false,
                                    }
                                } else {
                                    simple_type(&result_name)
                                }
                            };
                            if query.cardinality == jadpo_syntax::QueryCardinality::Optional {
                                self.push_diagnostic(
                                    "TYPE_INCLUDE_REQUIRED_PARENT",
                                    source,
                                    first_include.range,
                                );
                            }
                            if query.cardinality == jadpo_syntax::QueryCardinality::Many
                                && query.pagination.is_none()
                            {
                                self.push_diagnostic(
                                    "TYPE_INCLUDE_PARENT_PAGINATION_REQUIRED",
                                    source,
                                    first_include.range,
                                );
                            }
                            let mut seen_relationships = BTreeSet::new();
                            let mut included_children = Vec::new();
                            for include in &query.includes {
                                if include.cardinality
                                    != jadpo_syntax::QueryIncludeCardinality::Many
                                {
                                    self.push_diagnostic(
                                        "TYPE_INCLUDE_MIXED_CARDINALITY",
                                        source,
                                        include.range,
                                    );
                                    continue;
                                }
                                self.check_query_pagination(
                                    &include.pagination,
                                    environment,
                                    source,
                                );
                                let include_result_name = joined_name(&include.result.path);
                                if include_result_name != result_name {
                                    self.push_diagnostic(
                                        "TYPE_INCLUDE_RESULT_MISMATCH",
                                        source,
                                        include.result.range,
                                    );
                                }
                                if !seen_relationships.insert(include.relationship.text.clone()) {
                                    self.push_diagnostic(
                                        "TYPE_INCLUDE_DUPLICATE_RELATIONSHIP",
                                        source,
                                        include.relationship.range,
                                    );
                                }
                                let inverse = self
                                    .catalogue
                                    .inverses
                                    .get(&target)
                                    .and_then(|relationships| {
                                        relationships.get(&include.relationship.text)
                                    })
                                    .cloned();
                                let Some(inverse) = inverse else {
                                    self.push_diagnostic(
                                        "TYPE_INCLUDE_UNKNOWN_RELATIONSHIP",
                                        source,
                                        include.relationship.range,
                                    );
                                    continue;
                                };
                                if !self.catalogue.ordered_keys.get(&inverse.child).is_some_and(
                                    |fields| fields.contains(&include.order.field.text),
                                ) {
                                    self.push_diagnostic(
                                        "TYPE_INCLUDE_ORDER_NOT_DETERMINISTIC",
                                        source,
                                        include.order.field.range,
                                    );
                                }
                                included_children.push((include, inverse));
                            }
                            if !self.catalogue.is_projection_object(&result_name) {
                                self.push_diagnostic(
                                    "TYPE_INCLUDE_RESULT_NOT_OUTPUT",
                                    source,
                                    first_include.result.range,
                                );
                            }
                            let shape = self.catalogue.records.get(&result_name);
                            let parent_ok = shape
                                .and_then(|fields| fields.get("parent"))
                                .is_some_and(|field| field.declared_type == simple_type(&target));
                            let children_ok = included_children.iter().all(|(include, inverse)| {
                                shape
                                    .and_then(|fields| fields.get(&include.relationship.text))
                                    .is_some_and(|field| {
                                        field.declared_type
                                            == TypeValue {
                                                name: "List".to_owned(),
                                                arguments: vec![simple_type(&inverse.child)],
                                                nullable: false,
                                                secret: false,
                                            }
                                    })
                            });
                            if shape.map_or(true, |fields| fields.len() != query.includes.len() + 1)
                                || !parent_ok
                                || !children_ok
                            {
                                let _fields = included_children
                                    .iter()
                                    .map(|(include, inverse)| {
                                        format!(
                                            "{}: List<{}>",
                                            include.relationship.text, inverse.child
                                        )
                                    })
                                    .collect::<Vec<_>>()
                                    .join("`, `");
                                self.push_diagnostic(
                                    "TYPE_INCLUDE_RESULT_SHAPE",
                                    source,
                                    first_include.result.range,
                                );
                            }
                            included_type()
                        })
                    });
                if let Some(result) = included_result {
                    Some(result)
                } else {
                    match query.cardinality {
                        jadpo_syntax::QueryCardinality::Optional => {
                            let mut result = simple_type(&target);
                            result.nullable = true;
                            Some(result)
                        }
                        jadpo_syntax::QueryCardinality::Required => Some(simple_type(&target)),
                        jadpo_syntax::QueryCardinality::Many => Some(TypeValue {
                            name: "List".to_owned(),
                            arguments: vec![simple_type(&target)],
                            nullable: false,
                            secret: false,
                        }),
                    }
                }
            }
            Expression::Update(update) => {
                let target = joined_name(&update.target.path);
                if !self.catalogue.entities.contains(&target) {
                    self.push_diagnostic("TYPE_UPDATE_NOT_ENTITY", source, update.target.range);
                }
                self.check_mutation_predicate(
                    "update",
                    &target,
                    &update.field,
                    &update.value,
                    environment,
                    source,
                );
                if update.changes.is_empty()
                    && update.patch.is_none()
                    && update.transition.is_none()
                {
                    self.push_diagnostic("TYPE_UPDATE_FIELD_REQUIRED", source, update.range);
                }
                let expected_fields = self.catalogue.records.get(&target).cloned();
                if let Some(transition) = &update.transition {
                    let lifecycle = self.catalogue.lifecycles.get(&target);
                    let declared = lifecycle.is_some_and(|lifecycle| {
                        lifecycle
                            .transitions
                            .iter()
                            .any(|candidate| candidate.name.text == transition.text)
                    });
                    if lifecycle.is_none() || !declared {
                        self.push_diagnostic(
                            "TYPE_LIFECYCLE_TRANSITION_UNKNOWN",
                            source,
                            transition.range,
                        );
                    }
                    if !self
                        .catalogue
                        .identity_fields
                        .contains(&(target.clone(), update.field.text.clone()))
                    {
                        self.push_diagnostic(
                            "TYPE_LIFECYCLE_TRANSITION_IDENTITY_REQUIRED",
                            source,
                            update.field.range,
                        );
                    }
                    if !update.changes.is_empty()
                        || !update.conditional_changes.is_empty()
                        || update.patch.is_some()
                    {
                        self.push_diagnostic(
                            "TYPE_LIFECYCLE_TRANSITION_BODY_INVALID",
                            source,
                            update.range,
                        );
                    }
                }
                let mut changed_fields = BTreeSet::new();
                for change in &update.changes {
                    if !changed_fields.insert(change.name.text.clone()) {
                        self.push_diagnostic("TYPE_UPDATE_DUPLICATE_FIELD", source, change.range);
                    }
                    let Some(expected) = expected_fields
                        .as_ref()
                        .and_then(|fields| fields.get(&change.name.text))
                    else {
                        self.push_diagnostic(
                            "TYPE_UPDATE_UNKNOWN_FIELD",
                            source,
                            change.name.range,
                        );
                        continue;
                    };
                    if expected.generated.is_some() {
                        self.push_diagnostic(
                            "TYPE_GENERATED_FIELD_ASSIGNMENT",
                            source,
                            change.name.range,
                        );
                        continue;
                    }
                    if expected.lifecycle_owned {
                        self.push_diagnostic(
                            "TYPE_LIFECYCLE_FIELD_UPDATE_FORBIDDEN",
                            source,
                            change.name.range,
                        );
                        continue;
                    }
                    if expected.update_forbidden {
                        self.push_diagnostic(
                            "TYPE_FIELD_UPDATE_FORBIDDEN",
                            source,
                            change.name.range,
                        );
                        continue;
                    }
                    if let Some(received) =
                        self.infer_expression(&change.value, environment, source)
                    {
                        let field_type = TypeValue {
                            name: format!("{target}.{}", change.name.text),
                            arguments: expected.declared_type.arguments.clone(),
                            nullable: expected.declared_type.nullable,
                            secret: expected.declared_type.secret,
                        };
                        self.require_compatible(
                            &received,
                            &field_type,
                            source,
                            change.value.range(),
                        );
                    }
                }
                for conditional in &update.conditional_changes {
                    let change = &conditional.change;
                    if !changed_fields.insert(change.name.text.clone()) {
                        self.push_diagnostic("TYPE_UPDATE_DUPLICATE_FIELD", source, change.range);
                    }
                    let Some(expected) = expected_fields
                        .as_ref()
                        .and_then(|fields| fields.get(&change.name.text))
                    else {
                        self.push_diagnostic(
                            "TYPE_UPDATE_UNKNOWN_FIELD",
                            source,
                            change.name.range,
                        );
                        continue;
                    };
                    if expected.generated.is_some() {
                        self.push_diagnostic(
                            "TYPE_GENERATED_FIELD_ASSIGNMENT",
                            source,
                            change.name.range,
                        );
                        continue;
                    }
                    if expected.lifecycle_owned {
                        self.push_diagnostic(
                            "TYPE_LIFECYCLE_FIELD_UPDATE_FORBIDDEN",
                            source,
                            change.name.range,
                        );
                        continue;
                    }
                    if expected.update_forbidden {
                        self.push_diagnostic(
                            "TYPE_FIELD_UPDATE_FORBIDDEN",
                            source,
                            change.name.range,
                        );
                        continue;
                    }
                    if let Some(received) =
                        self.infer_expression(&change.value, environment, source)
                    {
                        let field_type = TypeValue {
                            name: format!("{target}.{}", change.name.text),
                            arguments: expected.declared_type.arguments.clone(),
                            nullable: expected.declared_type.nullable,
                            secret: expected.declared_type.secret,
                        };
                        self.require_compatible(
                            &received,
                            &field_type,
                            source,
                            change.value.range(),
                        );
                    }
                }
                if let Some(patch) = &update.patch {
                    if patch.path.len() != 1 {
                        self.push_diagnostic("TYPE_PATCH_INPUT_BINDING", source, patch.range);
                    }
                    let patch_expression = Expression::Name(patch.clone());
                    if let Some(received) =
                        self.infer_expression(&patch_expression, environment, source)
                    {
                        if !self.catalogue.is_patch_object(&received.name) {
                            self.push_diagnostic("TYPE_PATCH_NOT_INPUT", source, patch.range);
                        } else if let Some(patch_fields) =
                            self.catalogue.records.get(&received.name).cloned()
                        {
                            if patch_fields.is_empty() {
                                self.push_diagnostic(
                                    "TYPE_PATCH_FIELD_REQUIRED",
                                    source,
                                    patch.range,
                                );
                            }
                            for (field_name, patch_field) in &patch_fields {
                                if !patch_field.optional {
                                    self.push_diagnostic(
                                        "TYPE_PATCH_FIELD_NOT_OPTIONAL",
                                        source,
                                        patch.range,
                                    );
                                }
                                let Some(entity_field) = expected_fields
                                    .as_ref()
                                    .and_then(|fields| fields.get(field_name))
                                else {
                                    self.push_diagnostic(
                                        "TYPE_PATCH_UNKNOWN_FIELD",
                                        source,
                                        patch.range,
                                    );
                                    continue;
                                };
                                if entity_field.generated.is_some() {
                                    self.push_diagnostic(
                                        "TYPE_GENERATED_FIELD_INPUT",
                                        source,
                                        patch.range,
                                    );
                                    continue;
                                }
                                if entity_field.lifecycle_owned {
                                    self.push_diagnostic(
                                        "TYPE_LIFECYCLE_FIELD_UPDATE_FORBIDDEN",
                                        source,
                                        patch.range,
                                    );
                                } else if entity_field.update_forbidden {
                                    self.push_diagnostic(
                                        "TYPE_FIELD_UPDATE_FORBIDDEN",
                                        source,
                                        patch.range,
                                    );
                                    continue;
                                }
                                let expected = TypeValue {
                                    name: format!("{target}.{field_name}"),
                                    arguments: entity_field.declared_type.arguments.clone(),
                                    nullable: entity_field.declared_type.nullable,
                                    secret: entity_field.declared_type.secret,
                                };
                                self.require_compatible(
                                    &patch_field.declared_type,
                                    &expected,
                                    source,
                                    patch.range,
                                );
                            }
                            for conditional in &update.conditional_changes {
                                let supplied = &conditional.supplied;
                                let valid_binding = supplied.path.len() == 2
                                    && supplied.path[0].text == patch.path[0].text;
                                if !valid_binding {
                                    self.push_diagnostic(
                                        "TYPE_PATCH_CONDITION_BINDING",
                                        source,
                                        supplied.range,
                                    );
                                    continue;
                                }
                                let supplied_field = &supplied.path[1].text;
                                if !patch_fields.contains_key(supplied_field) {
                                    self.push_diagnostic(
                                        "TYPE_PATCH_CONDITION_UNKNOWN_FIELD",
                                        source,
                                        supplied.range,
                                    );
                                }
                            }
                            for patch_field in patch_fields.keys() {
                                if changed_fields.contains(patch_field) {
                                    self.push_diagnostic(
                                        "TYPE_PATCH_DERIVED_OVERLAP",
                                        source,
                                        patch.range,
                                    );
                                }
                            }
                        }
                    }
                }
                if let Some(empty) = &update.empty {
                    self.check_failure_binding_fields(empty, environment, source);
                }
                self.check_failure_binding_fields(&update.missing, environment, source);
                self.check_conflict_bindings(&target, &update.conflicts, environment, source);
                Some(simple_type(&target))
            }
            Expression::Delete(delete) => {
                let target = joined_name(&delete.target.path);
                if !self.catalogue.entities.contains(&target) {
                    self.push_diagnostic("TYPE_DELETE_NOT_ENTITY", source, delete.target.range);
                }
                if self.catalogue.lifecycles.contains_key(&target) {
                    self.push_diagnostic(
                        "TYPE_LIFECYCLE_HARD_DELETE_FORBIDDEN",
                        source,
                        delete.range,
                    );
                }
                self.check_mutation_predicate(
                    "delete",
                    &target,
                    &delete.field,
                    &delete.value,
                    environment,
                    source,
                );
                self.check_failure_binding_fields(&delete.missing, environment, source);
                self.check_conflict_bindings(&target, &delete.conflicts, environment, source);
                Some(simple_type(&target))
            }
            Expression::Unary(unary) => {
                let value = self.infer_expression(&unary.value, environment, source);
                if let Some(value) = value {
                    if value.secret {
                        self.push_diagnostic("CONFIG_SECRET_FLOW", source, unary.range);
                    }
                    let root = self.representation_root(&value.name);
                    match unary.operator {
                        jadpo_syntax::UnaryOperator::Not => {
                            if value.nullable || root.as_deref() != Some("Bool") {
                                self.push_diagnostic("TYPE_UNARY_OPERAND", source, unary.range);
                            }
                            Some(simple_type("Bool"))
                        }
                        jadpo_syntax::UnaryOperator::Negate => {
                            if value.nullable || !matches!(root.as_deref(), Some("Int" | "Decimal"))
                            {
                                self.push_diagnostic("TYPE_UNARY_OPERAND", source, unary.range);
                            }
                            Some(root.map_or_else(|| simple_type("Int"), |root| simple_type(&root)))
                        }
                    }
                } else {
                    None
                }
            }
            Expression::Binary(binary) => {
                let left = self.infer_expression(&binary.left, environment, source);
                let right = self.infer_expression(&binary.right, environment, source);
                if let (Some(left), Some(right)) = (left, right) {
                    self.infer_binary(binary, &left, &right, source)
                } else {
                    None
                }
            }
            Expression::Grouped(grouped) => {
                self.infer_expression(&grouped.value, environment, source)
            }
            Expression::Attempt(attempt) => {
                self.infer_expression(&attempt.value, environment, source)
            }
            Expression::OutcomeMatch(outcome) => {
                let subject = self.infer_expression(&outcome.subject, environment, source);
                for arm in &outcome.arms {
                    let mut arm_environment = environment.clone();
                    if let (jadpo_syntax::OutcomeMatchPattern::Success(binding), Some(subject)) =
                        (&arm.pattern, subject.as_ref())
                    {
                        arm_environment.insert(binding.text.clone(), subject.clone());
                    }
                    match &arm.body {
                        jadpo_syntax::OutcomeMatchArmBody::Value(value) => {
                            if let (Some(received), Some(expected)) = (
                                self.infer_expression(value, &arm_environment, source),
                                subject.as_ref(),
                            ) {
                                self.require_compatible(&received, expected, source, value.range());
                            }
                        }
                        jadpo_syntax::OutcomeMatchArmBody::Reject(rejection) => {
                            self.check_reject_fields(
                                &rejection.failure.text,
                                &rejection.values,
                                &arm_environment,
                                source,
                            );
                        }
                        jadpo_syntax::OutcomeMatchArmBody::Propagate(_) => {}
                    }
                }
                subject
            }
            Expression::Missing(_) => None,
        };

        if let Some(inferred) = &inferred {
            if self.delivery_type_is_sealed(inferred) {
                self.push_diagnostic("TYPE_JOB_DELIVERY_SEALED_USE", source, expression.range());
            }
            self.result.expressions.push(InferredExpression {
                source: source.to_owned(),
                range: expression.range(),
                type_name: inferred.display(),
            });
        }
        inferred
    }

    fn infer_binary(
        &mut self,
        binary: &jadpo_syntax::BinaryExpression,
        left: &TypeValue,
        right: &TypeValue,
        source: &str,
    ) -> Option<TypeValue> {
        use jadpo_syntax::BinaryOperator;
        if left.secret || right.secret {
            self.push_diagnostic("CONFIG_SECRET_FLOW", source, binary.range);
        }
        let comparable = self.representation_compatible(left, right)
            || self.representation_compatible(right, left);
        let left_root = self.representation_root(&left.name);
        let right_root = self.representation_root(&right.name);
        let non_nullable = !left.nullable && !right.nullable;
        let compares_nullable_with_none =
            (left.name == "none" && right.nullable) || (right.name == "none" && left.nullable);
        let timeline_pair = matches!(left_root.as_deref(), Some("Instant" | "Time"))
            && matches!(right_root.as_deref(), Some("Instant" | "Time"));
        match binary.operator {
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                if !comparable && !timeline_pair && !compares_nullable_with_none {
                    self.push_diagnostic("TYPE_INCOMPARABLE", source, binary.range);
                }
                Some(simple_type("Bool"))
            }
            BinaryOperator::And | BinaryOperator::Or => {
                if !non_nullable
                    || left_root.as_deref() != Some("Bool")
                    || right_root.as_deref() != Some("Bool")
                {
                    self.push_diagnostic("TYPE_LOGICAL_OPERAND", source, binary.range);
                }
                Some(simple_type("Bool"))
            }
            BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual => {
                let ordered = matches!(
                    left_root.as_deref(),
                    Some("Int" | "Decimal" | "Text" | "Instant")
                ) || timeline_pair;
                if !non_nullable || (!comparable && !timeline_pair) || !ordered {
                    self.push_diagnostic("TYPE_ORDERING_OPERAND", source, binary.range);
                }
                Some(simple_type("Bool"))
            }
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Remainder => {
                let temporal = match (binary.operator, left_root.as_deref(), right_root.as_deref())
                {
                    (BinaryOperator::Add, Some("Instant"), Some("Duration")) => {
                        Some(simple_type("Instant"))
                    }
                    (BinaryOperator::Add, Some("Duration"), Some("Duration"))
                    | (BinaryOperator::Subtract, Some("Duration"), Some("Duration"))
                    | (BinaryOperator::Subtract, Some("Instant"), Some("Instant")) => {
                        Some(simple_type("Duration"))
                    }
                    (BinaryOperator::Subtract, Some("Instant"), Some("Duration")) => {
                        Some(simple_type("Instant"))
                    }
                    _ => None,
                };
                if temporal.is_some() && non_nullable {
                    return temporal;
                }
                let numeric = matches!(left_root.as_deref(), Some("Int" | "Decimal"))
                    && matches!(right_root.as_deref(), Some("Int" | "Decimal"));
                let text_add = binary.operator == BinaryOperator::Add
                    && left_root.as_deref() == Some("Text")
                    && right_root.as_deref() == Some("Text");
                if !non_nullable || (!numeric && !text_add) {
                    self.push_diagnostic("TYPE_ARITHMETIC_OPERAND", source, binary.range);
                }
                if text_add {
                    Some(simple_type("Text"))
                } else if left_root.as_deref() == Some("Decimal")
                    || right_root.as_deref() == Some("Decimal")
                    || binary.operator == BinaryOperator::Divide
                {
                    Some(simple_type("Decimal"))
                } else {
                    Some(simple_type("Int"))
                }
            }
        }
    }

    fn check_mutation_predicate(
        &mut self,
        _operation: &str,
        target: &str,
        field: &Name,
        value: &Expression,
        environment: &BTreeMap<String, TypeValue>,
        source: &str,
    ) {
        let expected = self
            .catalogue
            .records
            .get(target)
            .and_then(|fields| fields.get(&field.text))
            .cloned();
        let Some(expected) = expected else {
            if self.catalogue.records.contains_key(target) {
                self.push_diagnostic("TYPE_MUTATION_UNKNOWN_PREDICATE_FIELD", source, field.range);
            }
            self.infer_expression(value, environment, source);
            return;
        };
        if expected.declared_type.nullable {
            self.push_diagnostic(
                "TYPE_MUTATION_NULLABLE_FIELD_UNSUPPORTED",
                source,
                field.range,
            );
        }
        if let Some(received) = self.infer_expression(value, environment, source) {
            let field_type = TypeValue {
                name: format!("{target}.{}", field.text),
                arguments: expected.declared_type.arguments,
                nullable: expected.declared_type.nullable,
                secret: expected.declared_type.secret,
            };
            self.require_compatible(&received, &field_type, source, value.range());
        }
    }

    fn check_failure_binding_fields(
        &mut self,
        binding: &jadpo_syntax::RejectStatement,
        environment: &BTreeMap<String, TypeValue>,
        source: &str,
    ) {
        self.check_reject_fields(&binding.failure.text, &binding.values, environment, source);
    }

    fn check_conflict_bindings(
        &mut self,
        target: &str,
        bindings: &[jadpo_syntax::ConflictBinding],
        environment: &BTreeMap<String, TypeValue>,
        source: &str,
    ) {
        let mut seen = BTreeSet::new();
        for binding in bindings {
            self.check_failure_binding_fields(&binding.rejection, environment, source);
            let (key, range) = binding.constraint.as_ref().map_or_else(
                || ("<fallback>".to_owned(), binding.range),
                |constraint| {
                    let name = joined_name(&constraint.path);
                    let name = if constraint.path.len() == 1 {
                        format!("{target}.{name}")
                    } else {
                        name
                    };
                    (name, constraint.range)
                },
            );
            if !seen.insert(key.clone()) {
                self.push_diagnostic("TYPE_CONFLICT_DUPLICATE_BINDING", source, range);
            }
            if key != "<fallback>" && !self.catalogue.persistence_constraints.contains(&key) {
                self.push_diagnostic("TYPE_CONFLICT_UNKNOWN_CONSTRAINT", source, range);
            }
        }
    }

    fn infer_invocation(
        &mut self,
        invocation: &InvocationExpression,
        environment: &BTreeMap<String, TypeValue>,
        source: &str,
    ) -> Option<TypeValue> {
        let authored_callee = joined_name(&invocation.callee.path);
        if authored_callee.starts_with("temporal.") {
            return self.infer_temporal_invocation(
                &authored_callee,
                invocation,
                environment,
                source,
            );
        }
        let mut callee = authored_callee.clone();
        let mut receiver = None;
        let is_type_constructor = self
            .graph
            .node(&authored_callee)
            .is_some_and(|node| node.kind.is_type());
        if !is_type_constructor
            && !self.catalogue.callables.contains_key(&callee)
            && invocation.callee.path.len() >= 2
        {
            let receiver_path = &invocation.callee.path[..invocation.callee.path.len() - 1];
            if let Some(receiver_type) = self.resolve_value_path(receiver_path, environment, source)
            {
                let owner = receiver_type
                    .name
                    .strip_suffix(".Ref")
                    .map(str::to_owned)
                    .or_else(|| self.record_shape_name(&receiver_type.name));
                if let Some(owner) = owner {
                    let operation = format!(
                        "{owner}.{}",
                        invocation
                            .callee
                            .path
                            .last()
                            .expect("callee has a name")
                            .text
                    );
                    if self.catalogue.callables.contains_key(&operation) {
                        callee = operation;
                        receiver = Some(receiver_type);
                    }
                }
            }
        }
        if let Some(signature) = self.catalogue.callables.get(&callee).cloned() {
            if self
                .delivery_seals
                .iter()
                .any(|seal| seal.operation == callee)
            {
                self.push_diagnostic("TYPE_JOB_DELIVERY_SEALED_USE", source, invocation.range);
            }
            for argument in &invocation.named_arguments {
                self.push_diagnostic("TYPE_NAMED_ARGUMENT_UNSUPPORTED", source, argument.range);
                self.infer_expression(&argument.value, environment, source);
            }
            let received_count = invocation.arguments.len() + usize::from(receiver.is_some());
            if signature.parameters.len() != received_count {
                self.push_diagnostic_with_facts(
                    "TYPE_ARGUMENT_COUNT",
                    source,
                    invocation.range,
                    [
                        DiagnosticFact::Name(callee.clone()),
                        DiagnosticFact::ExpectedCount(signature.parameters.len().to_string()),
                        DiagnosticFact::ReceivedCount(received_count.to_string()),
                    ],
                );
            }
            let mut expected = signature.parameters.iter();
            if let Some(receiver) = &receiver {
                if let Some(receiver_expected) = expected.next() {
                    self.require_compatible(
                        receiver,
                        receiver_expected,
                        source,
                        invocation.callee.range,
                    );
                }
            }
            for (argument, expected) in invocation.arguments.iter().zip(expected) {
                if let Some(received) = self.infer_expression(argument, environment, source) {
                    self.require_compatible(&received, expected, source, argument.range());
                }
            }
            return Some(signature.result);
        }

        let Some(target_node) = self.graph.node(&authored_callee) else {
            for argument in &invocation.arguments {
                self.infer_expression(argument, environment, source);
            }
            for argument in &invocation.named_arguments {
                self.infer_expression(&argument.value, environment, source);
            }
            return None;
        };
        if !target_node.kind.is_type() {
            return None;
        }
        if invocation.arguments.len() != 1 || !invocation.named_arguments.is_empty() {
            self.push_diagnostic_with_facts(
                "TYPE_CONSTRUCTOR_ARGUMENT_COUNT",
                source,
                invocation.range,
                [
                    DiagnosticFact::Name(authored_callee.clone()),
                    DiagnosticFact::ExpectedCount("1".to_owned()),
                    DiagnosticFact::ReceivedCount(
                        (invocation.arguments.len() + invocation.named_arguments.len()).to_string(),
                    ),
                ],
            );
            return Some(simple_type(&authored_callee));
        }

        let argument = &invocation.arguments[0];
        let argument_type = self.infer_expression(argument, environment, source);
        if let Some(argument_type) = &argument_type {
            if !self.constructor_input_compatible(argument_type, &authored_callee) {
                let expected = self
                    .ancestors(&authored_callee)
                    .get(1)
                    .cloned()
                    .unwrap_or_else(|| callee.clone());
                self.push_diagnostic_with_facts(
                    "TYPE_CONSTRUCTOR_INPUT",
                    source,
                    argument.range(),
                    [
                        DiagnosticFact::Name(authored_callee.clone()),
                        DiagnosticFact::Expected(expected),
                        DiagnosticFact::Received(argument_type.display()),
                    ],
                );
            }
        }
        if let Expression::Literal(literal) = argument {
            if let Some(reason) = self.invalid_literal_reason(&authored_callee, literal) {
                let constraint = reason
                    .strip_prefix("constraint `")
                    .and_then(|value| value.strip_suffix("` was not satisfied"))
                    .unwrap_or("declared")
                    .to_owned();
                self.push_diagnostic_with_facts(
                    "TYPE_INVALID_LITERAL",
                    source,
                    invocation.range,
                    [
                        DiagnosticFact::Subject(authored_callee.clone()),
                        DiagnosticFact::Constraint(constraint),
                    ],
                );
            }
        }
        // Construction changes the nominal contract, not CONFIG-D05's
        // information-flow classification. A cast is not a secret sink.
        let mut result = simple_type(&authored_callee);
        result.secret = argument_type.as_ref().is_some_and(|value| value.secret);
        Some(result)
    }

    fn infer_temporal_invocation(
        &mut self,
        callee: &str,
        invocation: &InvocationExpression,
        environment: &BTreeMap<String, TypeValue>,
        source: &str,
    ) -> Option<TypeValue> {
        use TemporalExpected::{ClockText, Exact, Timeline};
        use TemporalResult::{Exact as ResultType, FirstArgument};

        let operation = callee.strip_prefix("temporal.").unwrap_or(callee);
        let (positional, named, result): (&[TemporalExpected], &[(&str, TemporalExpected)], _) =
            match operation {
                "in_zone" => (&[Exact("Instant"), Exact("Zone")], &[], ResultType("Time")),
                "resolve" => (
                    &[],
                    &[
                        ("date", Exact("CalendarDate")),
                        ("at", ClockText),
                        ("zone", Exact("Zone")),
                        ("overlap", Exact("LocalOverlap")),
                        ("gap", Exact("LocalGap")),
                    ],
                    ResultType("Time"),
                ),
                "add_elapsed" => (&[Timeline, Exact("Duration")], &[], FirstArgument),
                "between" => (&[Timeline, Timeline], &[], ResultType("Duration")),
                "add_days" => (
                    &[Exact("CalendarDate")],
                    &[("days", Exact("Int"))],
                    ResultType("CalendarDate"),
                ),
                "add_weeks" => (
                    &[Exact("CalendarDate")],
                    &[("weeks", Exact("Int"))],
                    ResultType("CalendarDate"),
                ),
                "add_months" => (
                    &[Exact("CalendarDate")],
                    &[
                        ("months", Exact("Int")),
                        ("invalid_day", Exact("InvalidDay")),
                    ],
                    ResultType("CalendarDate"),
                ),
                "add_years" => (
                    &[Exact("CalendarDate")],
                    &[
                        ("years", Exact("Int")),
                        ("invalid_day", Exact("InvalidDay")),
                    ],
                    ResultType("CalendarDate"),
                ),
                "add_local_days" => (
                    &[Exact("Time")],
                    &[
                        ("days", Exact("Int")),
                        ("overlap", Exact("LocalOverlap")),
                        ("gap", Exact("LocalGap")),
                    ],
                    ResultType("Time"),
                ),
                "add_local_weeks" => (
                    &[Exact("Time")],
                    &[
                        ("weeks", Exact("Int")),
                        ("overlap", Exact("LocalOverlap")),
                        ("gap", Exact("LocalGap")),
                    ],
                    ResultType("Time"),
                ),
                "add_local_months" => (
                    &[Exact("Time")],
                    &[
                        ("months", Exact("Int")),
                        ("invalid_day", Exact("InvalidDay")),
                        ("overlap", Exact("LocalOverlap")),
                        ("gap", Exact("LocalGap")),
                    ],
                    ResultType("Time"),
                ),
                "add_local_years" => (
                    &[Exact("Time")],
                    &[
                        ("years", Exact("Int")),
                        ("invalid_day", Exact("InvalidDay")),
                        ("overlap", Exact("LocalOverlap")),
                        ("gap", Exact("LocalGap")),
                    ],
                    ResultType("Time"),
                ),
                "day_bounds" | "month_bounds" | "year_bounds" => (
                    &[Exact("CalendarDate"), Exact("Zone")],
                    &[],
                    ResultType("InstantRange"),
                ),
                "week_bounds" => (
                    &[Exact("CalendarDate"), Exact("Zone")],
                    &[("starts_on", Exact("Weekday"))],
                    ResultType("InstantRange"),
                ),
                "calendar_date" => (&[Exact("Time")], &[], ResultType("CalendarDate")),
                "year" | "month" | "day" | "hour" | "minute" | "second" | "millisecond" => {
                    (&[Exact("Time")], &[], ResultType("Int"))
                }
                "weekday" => (&[Exact("Time")], &[], ResultType("Weekday")),
                "offset" => (&[Exact("Time")], &[], ResultType("Duration")),
                "zone" => (&[Exact("Time")], &[], ResultType("Zone")),
                "same_zone" | "same_local" => {
                    (&[Exact("Time"), Exact("Time")], &[], ResultType("Bool"))
                }
                "format" => return self.infer_temporal_format(invocation, environment, source),
                "format_friendly" => (
                    &[Exact("Time")],
                    &[
                        ("relative_to", Exact("Instant")),
                        ("locale", Exact("Locale")),
                        ("profile", Exact("FriendlyTimeFormat")),
                    ],
                    ResultType("PresentationText"),
                ),
                _ => {
                    for argument in &invocation.arguments {
                        self.infer_expression(argument, environment, source);
                    }
                    for argument in &invocation.named_arguments {
                        self.infer_expression(&argument.value, environment, source);
                    }
                    return None;
                }
            };

        self.check_temporal_arguments(callee, invocation, positional, named, environment, source);
        match result {
            ResultType(name) => Some(simple_type(name)),
            FirstArgument => invocation
                .arguments
                .first()
                .and_then(|argument| self.infer_expression(argument, environment, source)),
        }
    }

    fn infer_temporal_format(
        &mut self,
        invocation: &InvocationExpression,
        environment: &BTreeMap<String, TypeValue>,
        source: &str,
    ) -> Option<TypeValue> {
        if invocation.arguments.len() != 1 {
            self.push_diagnostic_with_facts(
                "TYPE_ARGUMENT_COUNT",
                source,
                invocation.range,
                [
                    DiagnosticFact::Name("temporal.format".to_owned()),
                    DiagnosticFact::ExpectedCount("1".to_owned()),
                    DiagnosticFact::ReceivedCount(invocation.arguments.len().to_string()),
                ],
            );
        }
        if let Some(value) = invocation.arguments.first() {
            let received = self.infer_expression(value, environment, source);
            self.require_temporal_expected(
                received,
                TemporalExpected::FormatValue,
                source,
                value.range(),
            );
        }
        let locale = invocation
            .named_arguments
            .iter()
            .find(|argument| argument.name.text == "locale");
        if let Some(locale) = locale {
            let received = self.infer_expression(&locale.value, environment, source);
            self.require_temporal_expected(
                received,
                TemporalExpected::Exact("Locale"),
                source,
                locale.value.range(),
            );
        } else {
            self.push_diagnostic_with_facts(
                "TYPE_MISSING_FIELD",
                source,
                invocation.range,
                [
                    DiagnosticFact::Subject("temporal.format".to_owned()),
                    DiagnosticFact::Field("locale".to_owned()),
                ],
            );
        }
        let style = invocation
            .named_arguments
            .iter()
            .find(|argument| argument.name.text == "style");
        let components = invocation
            .named_arguments
            .iter()
            .find(|argument| argument.name.text == "components");
        match (style, components) {
            (Some(style), None) => {
                let received = self.infer_expression(&style.value, environment, source);
                self.require_temporal_expected(
                    received,
                    TemporalExpected::Exact("TimeFormat"),
                    source,
                    style.value.range(),
                );
            }
            (None, Some(components)) => {
                if let Expression::Object(object) = &components.value {
                    self.check_temporal_format_components(object, source);
                } else {
                    let received = self.infer_expression(&components.value, environment, source);
                    self.require_temporal_expected(
                        received,
                        TemporalExpected::Components,
                        source,
                        components.value.range(),
                    );
                }
            }
            _ => self.push_diagnostic("TYPE_TEMPORAL_FORMAT_CHOICE", source, invocation.range),
        }
        for argument in &invocation.named_arguments {
            if !matches!(
                argument.name.text.as_str(),
                "locale" | "style" | "components"
            ) {
                self.push_diagnostic_with_facts(
                    "TYPE_UNKNOWN_FIELD",
                    source,
                    argument.name.range,
                    [
                        DiagnosticFact::Subject("temporal.format".to_owned()),
                        DiagnosticFact::Field(argument.name.text.clone()),
                    ],
                );
            }
        }
        if self.catalogue.locales.is_empty() {
            self.push_diagnostic("TYPE_TEMPORAL_LOCALES_REQUIRED", source, invocation.range);
        }
        Some(simple_type("PresentationText"))
    }

    fn check_temporal_format_components(
        &mut self,
        object: &jadpo_syntax::ObjectExpression,
        source: &str,
    ) {
        let mut seen = BTreeSet::new();
        for field in &object.fields {
            if !seen.insert(field.name.text.clone()) {
                self.push_diagnostic(
                    "TYPE_TEMPORAL_FORMAT_COMPONENT_DUPLICATE",
                    source,
                    field.name.range,
                );
                continue;
            }
            let allowed: &[&str] = match field.name.text.as_str() {
                "weekday" => &["long", "short", "narrow"],
                "day" | "year" | "hour" | "minute" | "second" => &["numeric", "two_digit"],
                "month" => &["numeric", "two_digit", "long", "short", "narrow"],
                "zone_name" => &[
                    "long",
                    "short",
                    "long_offset",
                    "short_offset",
                    "long_generic",
                    "short_generic",
                ],
                "hour_cycle" => &["h11", "h12", "h23", "h24"],
                _ => {
                    self.push_diagnostic_with_facts(
                        "TYPE_UNKNOWN_FIELD",
                        source,
                        field.name.range,
                        [
                            DiagnosticFact::Subject("temporal.format components".to_owned()),
                            DiagnosticFact::Field(field.name.text.clone()),
                        ],
                    );
                    continue;
                }
            };
            let value = match &field.value {
                Expression::Name(value) if value.path.len() == 1 => &value.path[0].text,
                _ => {
                    self.push_diagnostic(
                        "TYPE_TEMPORAL_FORMAT_COMPONENT_INVALID",
                        source,
                        field.value.range(),
                    );
                    continue;
                }
            };
            if !allowed.contains(&value.as_str()) {
                self.push_diagnostic(
                    "TYPE_TEMPORAL_FORMAT_COMPONENT_INVALID",
                    source,
                    field.value.range(),
                );
            }
        }
        if object.fields.is_empty() {
            self.push_diagnostic(
                "TYPE_TEMPORAL_FORMAT_COMPONENTS_EMPTY",
                source,
                object.range,
            );
        }
    }

    fn check_temporal_arguments(
        &mut self,
        callee: &str,
        invocation: &InvocationExpression,
        positional: &[TemporalExpected],
        named: &[(&str, TemporalExpected)],
        environment: &BTreeMap<String, TypeValue>,
        source: &str,
    ) {
        if invocation.arguments.len() != positional.len() {
            self.push_diagnostic_with_facts(
                "TYPE_ARGUMENT_COUNT",
                source,
                invocation.range,
                [
                    DiagnosticFact::Name(callee.to_owned()),
                    DiagnosticFact::ExpectedCount(positional.len().to_string()),
                    DiagnosticFact::ReceivedCount(invocation.arguments.len().to_string()),
                ],
            );
        }
        for (argument, expected) in invocation.arguments.iter().zip(positional) {
            let received = self.infer_expression(argument, environment, source);
            self.require_temporal_expected(received, *expected, source, argument.range());
        }
        for argument in invocation.arguments.iter().skip(positional.len()) {
            self.infer_expression(argument, environment, source);
        }

        for (name, expected) in named {
            if let Some(argument) = invocation
                .named_arguments
                .iter()
                .find(|argument| argument.name.text == *name)
            {
                let received = self.infer_expression(&argument.value, environment, source);
                self.require_temporal_expected(received, *expected, source, argument.value.range());
                if matches!(expected, TemporalExpected::ClockText) {
                    if let Expression::Literal(literal) = &argument.value {
                        if literal.kind != LiteralKind::String
                            || !valid_clock_text(&unquote(&literal.text))
                        {
                            self.push_diagnostic(
                                "TYPE_TEMPORAL_CLOCK_INVALID",
                                source,
                                literal.range,
                            );
                        }
                    }
                }
            } else {
                self.push_diagnostic_with_facts(
                    "TYPE_MISSING_FIELD",
                    source,
                    invocation.range,
                    [
                        DiagnosticFact::Subject(callee.to_owned()),
                        DiagnosticFact::Field((*name).to_owned()),
                    ],
                );
            }
        }
        for argument in &invocation.named_arguments {
            if !named.iter().any(|(name, _)| *name == argument.name.text) {
                self.push_diagnostic_with_facts(
                    "TYPE_UNKNOWN_FIELD",
                    source,
                    argument.name.range,
                    [
                        DiagnosticFact::Subject(callee.to_owned()),
                        DiagnosticFact::Field(argument.name.text.clone()),
                    ],
                );
                self.infer_expression(&argument.value, environment, source);
            }
        }
    }

    fn require_temporal_expected(
        &mut self,
        received: Option<TypeValue>,
        expected: TemporalExpected,
        source: &str,
        range: TextRange,
    ) {
        let Some(received) = received else {
            return;
        };
        // Temporal helpers are ordinary computation, not declared adapter
        // sinks. Representation compatibility cannot declassify a secret.
        if received.secret {
            self.push_diagnostic("CONFIG_SECRET_FLOW", source, range);
            return;
        }
        let root = self.representation_root(&received.name);
        let (accepted, expected_name) = match expected {
            TemporalExpected::Exact(name) => (root.as_deref() == Some(name), name.to_owned()),
            TemporalExpected::Timeline => (
                matches!(root.as_deref(), Some("Instant" | "Time")),
                "Instant or Time".to_owned(),
            ),
            TemporalExpected::FormatValue => (
                matches!(root.as_deref(), Some("CalendarDate" | "Time")),
                "CalendarDate or Time".to_owned(),
            ),
            TemporalExpected::ClockText => (
                root.as_deref() == Some("Text"),
                "strict clock text".to_owned(),
            ),
            TemporalExpected::Components => (
                root.as_deref() == Some("Object"),
                "format components".to_owned(),
            ),
        };
        if !accepted {
            self.push_diagnostic_with_facts(
                if matches!(expected, TemporalExpected::FormatValue)
                    && root.as_deref() == Some("Instant")
                {
                    "TYPE_TEMPORAL_FORMAT_REQUIRES_TIME"
                } else {
                    "TYPE_MISMATCH"
                },
                source,
                range,
                [
                    DiagnosticFact::Expected(expected_name),
                    DiagnosticFact::Received(received.display()),
                ],
            );
        }
    }

    fn check_construction(
        &mut self,
        target: &str,
        fields: &[FieldInitialiser],
        range: TextRange,
        environment: &BTreeMap<String, TypeValue>,
        source: &str,
        use_field_identity: bool,
    ) {
        let Some(expected_fields) = self.catalogue.records.get(target).cloned() else {
            self.push_diagnostic("TYPE_NOT_RECORD", source, range);
            return;
        };
        let supplied = fields
            .iter()
            .map(|field| field.name.text.as_str())
            .collect::<BTreeSet<_>>();

        for (name, expected) in &expected_fields {
            if expected.generated.is_none()
                && !expected.lifecycle_initial
                && !expected.optional
                && !supplied.contains(name.as_str())
            {
                self.push_diagnostic_with_facts(
                    "TYPE_MISSING_FIELD",
                    source,
                    range,
                    [
                        DiagnosticFact::Subject(target.to_owned()),
                        DiagnosticFact::Field(name.clone()),
                    ],
                );
            }
        }
        for field in fields {
            let Some(expected) = expected_fields.get(&field.name.text) else {
                let mut facts = vec![
                    DiagnosticFact::Subject(target.to_owned()),
                    DiagnosticFact::Field(field.name.text.clone()),
                ];
                if let Some(suggestion) =
                    closest_name(&field.name.text, expected_fields.keys().map(String::as_str))
                {
                    facts.push(DiagnosticFact::SuggestedName(suggestion));
                }
                self.push_diagnostic_with_facts(
                    "TYPE_UNKNOWN_FIELD",
                    source,
                    field.name.range,
                    facts,
                );
                continue;
            };
            if expected.lifecycle_initial {
                self.push_diagnostic(
                    "TYPE_LIFECYCLE_FIELD_CREATE_FORBIDDEN",
                    source,
                    field.name.range,
                );
                continue;
            }
            if expected.generated.is_some() {
                self.push_diagnostic("TYPE_GENERATED_FIELD_ASSIGNMENT", source, field.name.range);
                continue;
            }
            if let Some(received) = self.infer_expression(&field.value, environment, source) {
                let expected_type = if use_field_identity {
                    TypeValue {
                        name: format!("{target}.{}", field.name.text),
                        arguments: expected.declared_type.arguments.clone(),
                        nullable: expected.declared_type.nullable,
                        secret: expected.declared_type.secret,
                    }
                } else {
                    expected.declared_type.clone()
                };
                self.require_compatible(&received, &expected_type, source, field.value.range());
            }
        }
    }

    fn check_variant_construction(
        &mut self,
        target: &str,
        expected_fields: &BTreeMap<String, RecordField>,
        supplied_fields: &[FieldInitialiser],
        range: TextRange,
        environment: &BTreeMap<String, TypeValue>,
        source: &str,
    ) {
        let supplied = supplied_fields
            .iter()
            .map(|field| field.name.text.as_str())
            .collect::<BTreeSet<_>>();
        for (name, expected) in expected_fields {
            if !expected.optional && !supplied.contains(name.as_str()) {
                self.push_diagnostic_with_facts(
                    "TYPE_MISSING_VARIANT_FIELD",
                    source,
                    range,
                    [
                        DiagnosticFact::Subject(target.to_owned()),
                        DiagnosticFact::Field(name.clone()),
                    ],
                );
            }
        }
        let mut seen = BTreeSet::new();
        for field in supplied_fields {
            if !seen.insert(field.name.text.clone()) {
                self.push_diagnostic("TYPE_DUPLICATE_VARIANT_FIELD", source, field.name.range);
                continue;
            }
            let Some(expected) = expected_fields.get(&field.name.text) else {
                let mut facts = vec![
                    DiagnosticFact::Subject(target.to_owned()),
                    DiagnosticFact::Field(field.name.text.clone()),
                ];
                if let Some(suggestion) =
                    closest_name(&field.name.text, expected_fields.keys().map(String::as_str))
                {
                    facts.push(DiagnosticFact::SuggestedName(suggestion));
                }
                self.push_diagnostic_with_facts(
                    "TYPE_UNKNOWN_VARIANT_FIELD",
                    source,
                    field.name.range,
                    facts,
                );
                continue;
            };
            if let Some(received) = self.infer_expression(&field.value, environment, source) {
                self.require_compatible(
                    &received,
                    &expected.declared_type,
                    source,
                    field.value.range(),
                );
            }
        }
    }

    fn enum_variant(
        &self,
        target: &str,
    ) -> Option<(String, String, BTreeMap<String, RecordField>)> {
        let (enum_name, variant) = target.split_once('.')?;
        if variant.contains('.') {
            return None;
        }
        let fields = self.catalogue.enums.get(enum_name)?.get(variant)?.clone();
        Some((enum_name.to_owned(), variant.to_owned(), fields))
    }

    fn check_reject_fields(
        &mut self,
        failure: &str,
        fields: &[FieldInitialiser],
        environment: &BTreeMap<String, TypeValue>,
        source: &str,
    ) {
        let mut expected_fields = self
            .catalogue
            .failure_fields
            .get(&format!("{failure}.public"))
            .cloned()
            .unwrap_or_default();
        expected_fields.extend(
            self.catalogue
                .failure_fields
                .get(&format!("{failure}.internal"))
                .cloned()
                .unwrap_or_default(),
        );
        for field in fields {
            let inferred = self.infer_expression(&field.value, environment, source);
            if let (Some(received), Some(expected)) =
                (inferred, expected_fields.get(&field.name.text))
            {
                self.require_compatible(
                    &received,
                    &expected.declared_type,
                    source,
                    field.value.range(),
                );
            }
        }
    }

    fn resolve_value_path(
        &mut self,
        path: &[Name],
        environment: &BTreeMap<String, TypeValue>,
        source: &str,
    ) -> Option<TypeValue> {
        let first = path.first()?;
        if first.text == "current_principal" && environment.contains_key("__route_public") {
            return None;
        }
        if path.len() == 2
            && first.text == "current_principal"
            && environment.contains_key("current_principal")
        {
            if let Some(principal) = &self.catalogue.principal {
                if self
                    .catalogue
                    .enums
                    .get(principal)
                    .is_some_and(|variants| variants.contains_key(&path[1].text))
                {
                    return Some(simple_type(&format!("{principal}.{}", path[1].text)));
                }
            }
        }
        if path.len() == 2 {
            if let Some(variants) = self.catalogue.enums.get(&first.text) {
                let variant = &path[1];
                if variants.contains_key(&variant.text) {
                    return Some(simple_type(&first.text));
                }
                let mut facts = vec![
                    DiagnosticFact::Subject(first.text.clone()),
                    DiagnosticFact::Name(variant.text.clone()),
                ];
                if let Some(suggestion) =
                    closest_name(&variant.text, variants.keys().map(String::as_str))
                {
                    facts.push(DiagnosticFact::SuggestedName(suggestion));
                }
                self.push_diagnostic_with_facts(
                    "TYPE_UNKNOWN_ENUM_VARIANT",
                    source,
                    variant.range,
                    facts,
                );
                return None;
            }
        }
        let Some(mut current) = environment.get(&first.text).cloned() else {
            if first.text == "clock" {
                self.push_diagnostic("TYPE_CLOCK_CONTEXT", source, first.range);
                return None;
            }
            let mut facts = vec![DiagnosticFact::Name(first.text.clone())];
            if let Some(suggestion) =
                closest_name(&first.text, environment.keys().map(String::as_str))
            {
                facts.push(DiagnosticFact::SuggestedName(suggestion));
            }
            self.push_diagnostic_with_facts("TYPE_UNKNOWN_VALUE", source, first.range, facts);
            return None;
        };

        for field in &path[1..] {
            if current.nullable {
                self.push_diagnostic_with_facts(
                    "TYPE_NULLABLE_SELECTION",
                    source,
                    field.range,
                    [
                        DiagnosticFact::Received(current.display()),
                        DiagnosticFact::Field(field.text.clone()),
                    ],
                );
                return None;
            }
            let Some(record_name) = self.record_shape_name(&current.name) else {
                self.push_diagnostic_with_facts(
                    "TYPE_FIELD_ON_NON_RECORD",
                    source,
                    field.range,
                    [
                        DiagnosticFact::Received(current.display()),
                        DiagnosticFact::Field(field.text.clone()),
                    ],
                );
                return None;
            };
            let record_fields = self.catalogue.records.get(&record_name);
            let declared = record_fields
                .and_then(|fields| fields.get(&field.text))
                .cloned();
            let Some(declared) = declared else {
                let mut facts = vec![
                    DiagnosticFact::Subject(record_name.clone()),
                    DiagnosticFact::Field(field.text.clone()),
                ];
                if let Some(suggestion) = record_fields
                    .and_then(|fields| closest_name(&field.text, fields.keys().map(String::as_str)))
                {
                    facts.push(DiagnosticFact::SuggestedName(suggestion));
                }
                self.push_diagnostic_with_facts("TYPE_UNKNOWN_FIELD", source, field.range, facts);
                return None;
            };
            current = if record_name.starts_with("__route_path_")
                || record_name.starts_with("__route_headers_")
                || matches!(
                    record_name.as_str(),
                    "__jadpo_config" | "__jadpo_clock" | "Time" | "InstantRange"
                ) {
                declared.declared_type
            } else {
                TypeValue {
                    name: format!("{record_name}.{}", field.text),
                    arguments: declared.declared_type.arguments,
                    nullable: declared.declared_type.nullable,
                    secret: declared.declared_type.secret,
                }
            };
        }
        Some(current)
    }

    fn require_compatible(
        &mut self,
        received: &TypeValue,
        expected: &TypeValue,
        source: &str,
        range: TextRange,
    ) {
        if received.secret && !expected.secret {
            self.push_diagnostic("CONFIG_SECRET_FLOW", source, range);
            return;
        }
        if self.assignment_compatible(received, expected) {
            return;
        }
        let shared_parent = self.shared_parent(received, expected);
        let both_fields = self.node_kind(&received.name) == Some(NodeKind::Field)
            && self.node_kind(&expected.name) == Some(NodeKind::Field);
        if both_fields && shared_parent.is_some() {
            self.push_diagnostic_with_facts(
                "TYPE_SIBLING_MISMATCH",
                source,
                range,
                [
                    DiagnosticFact::Expected(expected.display()),
                    DiagnosticFact::Received(received.display()),
                ],
            );
        } else {
            self.push_diagnostic_with_facts(
                "TYPE_MISMATCH",
                source,
                range,
                [
                    DiagnosticFact::Expected(expected.display()),
                    DiagnosticFact::Received(received.display()),
                ],
            );
        }
    }

    fn assignment_compatible(&self, received: &TypeValue, expected: &TypeValue) -> bool {
        if received.name == "none" {
            return expected.nullable;
        }
        if received == expected {
            return true;
        }
        if received.nullable && !expected.nullable {
            return false;
        }
        if received.arguments != expected.arguments {
            return false;
        }
        if let Some(owner) = expected.name.strip_suffix(".Ref") {
            return received.name == owner || received.name == expected.name;
        }
        if self.node_kind(&expected.name) == Some(NodeKind::PreludeType)
            && self.node_kind(&received.name) != Some(NodeKind::Field)
        {
            return false;
        }
        self.has_refinement_path(&received.name, &expected.name)
    }

    fn representation_compatible(&self, left: &TypeValue, right: &TypeValue) -> bool {
        left.arguments == right.arguments
            && (left.name == right.name || self.has_refinement_path(&left.name, &right.name))
    }

    fn constructor_input_compatible(&self, argument: &TypeValue, target: &str) -> bool {
        let declared_type = self.ancestors(target).get(1).cloned();
        if !argument.nullable
            && argument.arguments.is_empty()
            && declared_type.as_ref().is_some_and(|declared| {
                argument.name == *declared || self.has_refinement_path(&argument.name, declared)
            })
        {
            return true;
        }

        let argument_root = self.representation_root(&argument.name);
        let target_root = self.representation_root(target);
        if matches!(
            target_root.as_deref(),
            Some("Instant" | "CalendarDate" | "Duration")
        ) {
            return argument_root.as_deref() == Some("Text");
        }
        argument_root.is_some() && argument_root == target_root
    }

    fn has_refinement_path(&self, from: &str, to: &str) -> bool {
        let mut current = self.graph.node(from).map(|node| node.id);
        let Some(target) = self.graph.node(to).map(|node| node.id) else {
            return false;
        };
        let mut visited = BTreeSet::new();
        while let Some(node) = current {
            if node == target {
                return true;
            }
            if !visited.insert(node) {
                break;
            }
            current = self
                .graph
                .refinements
                .iter()
                .find(|edge| edge.refined == node)
                .map(|edge| edge.parent);
        }
        false
    }

    fn ancestors(&self, name: &str) -> Vec<String> {
        let mut ancestors = Vec::new();
        let mut current = self.graph.node(name).map(|node| node.id);
        let mut visited = BTreeSet::new();
        while let Some(node) = current {
            if !visited.insert(node) {
                break;
            }
            ancestors.push(self.graph.nodes[node.0 as usize].name.clone());
            current = self
                .graph
                .refinements
                .iter()
                .find(|edge| edge.refined == node)
                .map(|edge| edge.parent);
        }
        ancestors
    }

    fn shared_parent(&self, left: &TypeValue, right: &TypeValue) -> Option<String> {
        let right_ancestors = self
            .ancestors(&right.name)
            .into_iter()
            .collect::<BTreeSet<_>>();
        self.ancestors(&left.name)
            .into_iter()
            .skip(1)
            .find(|ancestor| right_ancestors.contains(ancestor))
    }

    fn representation_root(&self, name: &str) -> Option<String> {
        self.ancestors(name)
            .into_iter()
            .rev()
            .find(|ancestor| self.node_kind(ancestor) == Some(NodeKind::PreludeType))
    }

    fn record_shape_name(&self, name: &str) -> Option<String> {
        if self.catalogue.records.contains_key(name) {
            return Some(name.to_owned());
        }
        self.ancestors(name)
            .into_iter()
            .find(|candidate| self.catalogue.records.contains_key(candidate))
    }

    fn enum_shape_name(&self, name: &str) -> Option<String> {
        self.ancestors(name)
            .into_iter()
            .find(|candidate| self.catalogue.enums.contains_key(candidate))
    }

    fn node_kind(&self, name: &str) -> Option<NodeKind> {
        self.graph.node(name).map(|node| node.kind)
    }

    fn is_primitive_signature_type(&self, value: &TypeValue) -> bool {
        value.arguments.is_empty()
            && self.node_kind(&value.name) == Some(NodeKind::PreludeType)
            && value.name != "Unit"
    }

    fn literal_type(&self, literal: &Literal) -> Option<TypeValue> {
        match literal.kind {
            LiteralKind::String => Some(simple_type("Text")),
            LiteralKind::Integer => Some(simple_type("Int")),
            LiteralKind::Decimal => Some(simple_type("Decimal")),
            LiteralKind::Boolean => Some(simple_type("Bool")),
            LiteralKind::None => Some(simple_type("none")),
        }
    }

    fn invalid_literal_reason(&self, target: &str, literal: &Literal) -> Option<String> {
        let target_root = self.representation_root(target);
        let text = unquote(&literal.text);
        let temporal_invalid = match target_root.as_deref() {
            Some("Instant") => !valid_instant(&text),
            Some("CalendarDate") => !valid_calendar_date(&text),
            Some("Duration") => !valid_duration(&text),
            _ => false,
        };
        if temporal_invalid {
            return Some("constraint `canonical temporal value` was not satisfied".to_owned());
        }
        let constraints = self.constraints_for(target);
        for constraint in constraints {
            let violated = match constraint.kind {
                ConstraintKind::MinLength => constraint
                    .value
                    .text
                    .parse::<usize>()
                    .ok()
                    .is_some_and(|minimum| text.chars().count() < minimum),
                ConstraintKind::MaxLength => constraint
                    .value
                    .text
                    .parse::<usize>()
                    .ok()
                    .is_some_and(|maximum| text.chars().count() > maximum),
                ConstraintKind::Min => numeric_literal(literal)
                    .zip(numeric_constraint(constraint))
                    .is_some_and(|(value, minimum)| value < minimum),
                ConstraintKind::Max => numeric_literal(literal)
                    .zip(numeric_constraint(constraint))
                    .is_some_and(|(value, maximum)| value > maximum),
                ConstraintKind::Format => match constraint.value.text.as_str() {
                    "email" => !valid_email(&text),
                    "url" => !valid_url(&text),
                    "ip_address" => !valid_ip_address(&text),
                    _ => false,
                },
                ConstraintKind::Pattern => {
                    !matches_core_pattern(&text, &unquote(&constraint.value.text))
                }
            };
            if violated {
                return Some(format!(
                    "constraint `{}` was not satisfied",
                    constraint_name(constraint.kind)
                ));
            }
        }
        None
    }

    fn constraints_for(&self, target: &str) -> Vec<&Constraint> {
        self.ancestors(target)
            .iter()
            .flat_map(|name| self.catalogue.constraints.get(name).into_iter().flatten())
            .collect()
    }

    fn push_diagnostic(&mut self, code: &'static str, source: &str, range: TextRange) {
        self.push_diagnostic_with_facts(code, source, range, []);
    }

    fn push_diagnostic_with_facts(
        &mut self,
        code: &'static str,
        source: &str,
        range: TextRange,
        facts: impl IntoIterator<Item = DiagnosticFact>,
    ) {
        let mut diagnostic = facts
            .into_iter()
            .fold(Diagnostic::error(code), Diagnostic::with_fact);
        diagnostic.primary = Some(SourceSpan {
            source: source.to_owned(),
            start: range.start,
            end: range.end,
        });
        self.result.diagnostics.push(diagnostic);
    }

    fn check_query_pagination(
        &mut self,
        pagination: &jadpo_syntax::QueryPagination,
        environment: &BTreeMap<String, TypeValue>,
        source: &str,
    ) {
        for (_label, expression, minimum) in [
            ("limit", pagination.limit.as_ref(), 1_i64),
            ("offset", pagination.offset.as_ref(), 0_i64),
        ] {
            if let Some(received) = self.infer_expression(expression, environment, source) {
                self.require_compatible(&received, &simple_type("Int"), source, expression.range());
            }
            let Expression::Literal(literal) = expression else {
                self.push_diagnostic(
                    "TYPE_QUERY_PAGINATION_CONSTANT_REQUIRED",
                    source,
                    expression.range(),
                );
                continue;
            };
            let value = (literal.kind == LiteralKind::Integer)
                .then(|| literal.text.parse::<i64>().ok())
                .flatten();
            if !value.is_some_and(|value| value >= minimum) {
                self.push_diagnostic("TYPE_QUERY_PAGINATION_RANGE", source, literal.range);
            }
        }
    }

    fn check_query_page(
        &mut self,
        page: &jadpo_syntax::QueryPage,
        target: &str,
        environment: &BTreeMap<String, TypeValue>,
        source: &str,
    ) {
        let Some(entity_fields) = self.catalogue.records.get(target).cloned() else {
            return;
        };
        use jadpo_syntax::QueryPagePredicateOperator as Op;
        let mut range_fields = BTreeSet::new();
        for predicate in &page.predicates {
            if matches!(
                predicate.operator,
                Op::RequiredGreaterEqual | Op::RequiredLess
            ) {
                range_fields.insert(predicate.field.text.clone());
            }
        }
        if range_fields.len() > 1 {
            self.push_diagnostic("TYPE_QUERY_PAGE_RANGE_PAIR", source, page.range);
        }
        for field in &range_fields {
            let operators = page
                .predicates
                .iter()
                .filter(|p| &p.field.text == field)
                .map(|p| p.operator)
                .collect::<Vec<_>>();
            if operators.len() != 2
                || !operators.contains(&Op::RequiredGreaterEqual)
                || !operators.contains(&Op::RequiredLess)
            {
                self.push_diagnostic("TYPE_QUERY_PAGE_RANGE_PAIR", source, page.range);
            }
        }
        let mut predicate_fields = BTreeSet::new();
        for predicate in &page.predicates {
            if !range_fields.contains(&predicate.field.text)
                && !predicate_fields.insert(predicate.field.text.clone())
            {
                self.push_diagnostic("TYPE_MISMATCH", source, predicate.field.range);
            }
            let Some(expected) = entity_fields.get(&predicate.field.text) else {
                self.push_diagnostic("TYPE_QUERY_UNKNOWN_FIELD", source, predicate.field.range);
                self.infer_expression(&predicate.value, environment, source);
                continue;
            };
            let received = self.infer_expression(&predicate.value, environment, source);
            match predicate.operator {
                jadpo_syntax::QueryPagePredicateOperator::Equal => {
                    if received.as_ref().is_some_and(|value| value.name == "none") {
                        if !expected.declared_type.nullable {
                            self.push_diagnostic("TYPE_MISMATCH", source, predicate.value.range());
                        }
                    } else {
                        let field_type = TypeValue {
                            name: format!("{target}.{}", predicate.field.text),
                            arguments: expected.declared_type.arguments.clone(),
                            nullable: expected.declared_type.nullable,
                            secret: expected.declared_type.secret,
                        };
                        if let Some(received) = received {
                            self.require_compatible(
                                &received,
                                &field_type,
                                source,
                                predicate.value.range(),
                            );
                        }
                    }
                }
                Op::RequiredGreaterEqual | Op::RequiredLess => {
                    if expected.declared_type.nullable
                        || expected.declared_type.secret
                        || self
                            .representation_root(&expected.declared_type.name)
                            .as_deref()
                            != Some("Instant")
                    {
                        self.push_diagnostic(
                            "TYPE_QUERY_PAGE_RANGE_FIELD",
                            source,
                            predicate.field.range,
                        );
                    }
                    if !self.required_range_input_field(&predicate.value, environment) {
                        self.push_diagnostic(
                            "TYPE_QUERY_PAGE_RANGE_INPUT",
                            source,
                            predicate.value.range(),
                        );
                    }
                    if let Some(received) = received {
                        let field_type = TypeValue {
                            name: format!("{target}.{}", predicate.field.text),
                            arguments: expected.declared_type.arguments.clone(),
                            nullable: false,
                            secret: false,
                        };
                        self.require_compatible(
                            &received,
                            &field_type,
                            source,
                            predicate.value.range(),
                        );
                    }
                }
                jadpo_syntax::QueryPagePredicateOperator::OptionalEqual
                | jadpo_syntax::QueryPagePredicateOperator::OptionalLessEqual => {
                    if !self.optional_input_field(&predicate.value, environment) {
                        self.push_diagnostic("TYPE_MISMATCH", source, predicate.value.range());
                    }
                    let Some(received) = received else { continue };
                    if predicate.operator
                        == jadpo_syntax::QueryPagePredicateOperator::OptionalLessEqual
                    {
                        let left_root = self.representation_root(&expected.declared_type.name);
                        let right_root = self.representation_root(&received.name);
                        if expected.declared_type.name != received.name && left_root != right_root {
                            self.push_diagnostic("TYPE_MISMATCH", source, predicate.value.range());
                        }
                        if !matches!(left_root.as_deref(), Some("Int" | "Decimal" | "Instant")) {
                            self.push_diagnostic("TYPE_MISMATCH", source, predicate.field.range);
                        }
                    } else {
                        let field_type = TypeValue {
                            name: format!("{target}.{}", predicate.field.text),
                            arguments: expected.declared_type.arguments.clone(),
                            nullable: false,
                            secret: expected.declared_type.secret,
                        };
                        self.require_compatible(
                            &received,
                            &field_type,
                            source,
                            predicate.value.range(),
                        );
                    }
                }
            }
        }

        let mut order_fields = BTreeSet::new();
        for order in &page.order {
            let Some(field) = entity_fields.get(&order.field.text) else {
                self.push_diagnostic("TYPE_QUERY_UNKNOWN_ORDER_FIELD", source, order.field.range);
                continue;
            };
            if !order_fields.insert(order.field.text.clone()) {
                self.push_diagnostic("TYPE_QUERY_ORDER_NOT_DETERMINISTIC", source, order.range);
            }
            let root = self.representation_root(&field.declared_type.name);
            if field.declared_type.nullable
                || !matches!(
                    root.as_deref(),
                    Some("Uuid" | "Text" | "Int" | "Decimal" | "Instant")
                )
            {
                self.push_diagnostic("TYPE_QUERY_ORDER_NOT_DETERMINISTIC", source, order.range);
            }
        }
        if page.order.is_empty()
            || page.order.first().is_some_and(|first| {
                page.order
                    .iter()
                    .any(|order| order.direction != first.direction)
            })
            || page.order.last().is_some_and(|order| {
                !self
                    .catalogue
                    .ordered_keys
                    .get(target)
                    .is_some_and(|fields| fields.contains(&order.field.text))
            })
        {
            self.push_diagnostic("TYPE_QUERY_ORDER_NOT_DETERMINISTIC", source, page.range);
        }

        let cursor_type = resolved_type_value(&page.cursor, self.graph);
        if !page.after_optional || !self.optional_input_field(&page.after, environment) {
            self.push_diagnostic("TYPE_MISMATCH", source, page.after.range());
        }
        if let Some(received) = self.infer_expression(&page.after, environment, source) {
            let mut expected_cursor = cursor_type.clone();
            expected_cursor.nullable = false;
            self.require_compatible(&received, &expected_cursor, source, page.after.range());
        }
        let sort_names = page
            .order
            .iter()
            .map(|order| order.field.text.as_str())
            .collect::<Vec<_>>();
        let cursor_names = page
            .cursor_fields
            .iter()
            .map(|field| field.text.as_str())
            .collect::<Vec<_>>();
        if sort_names != cursor_names {
            self.push_diagnostic("TYPE_QUERY_ORDER_NOT_DETERMINISTIC", source, page.range);
        }
        if let Some(cursor_fields) = self.catalogue.records.get(&cursor_type.name).cloned() {
            for field_name in &page.cursor_fields {
                let Some(entity_field) = entity_fields.get(&field_name.text) else {
                    self.push_diagnostic(
                        "TYPE_QUERY_UNKNOWN_ORDER_FIELD",
                        source,
                        field_name.range,
                    );
                    continue;
                };
                let Some(cursor_field) = cursor_fields.get(&field_name.text) else {
                    self.push_diagnostic("TYPE_QUERY_PAGE_SHAPE", source, field_name.range);
                    continue;
                };
                if !self.page_field_types_match(
                    &cursor_field.declared_type,
                    &entity_field.declared_type,
                ) {
                    self.push_diagnostic("TYPE_QUERY_PAGE_SHAPE", source, field_name.range);
                }
            }
        }

        if let Some(received) = self.infer_expression(&page.limit, environment, source) {
            self.require_compatible(&received, &simple_type("Int"), source, page.limit.range());
        }

        let projection_type = resolved_type_value(&page.projection, self.graph);
        let projection_name = projection_type.name.clone();
        if !self.catalogue.is_projection_object(&projection_name) {
            self.push_diagnostic("TYPE_QUERY_PAGE_SHAPE", source, page.projection.range);
        }
        if let Some(projection_fields) = self.catalogue.records.get(&projection_name).cloned() {
            for (name, projected) in projection_fields {
                if let Some(entity_field) = entity_fields.get(&name) {
                    if !self.page_field_types_match(
                        &entity_field.declared_type,
                        &projected.declared_type,
                    ) {
                        self.push_diagnostic(
                            "TYPE_QUERY_PAGE_SHAPE",
                            source,
                            page.projection.range,
                        );
                    }
                } else {
                    self.push_diagnostic("TYPE_QUERY_UNKNOWN_FIELD", source, page.projection.range);
                }
            }
        }

        let result_type = resolved_type_value(&page.result, self.graph);
        let Some(result_fields) = self.catalogue.records.get(&result_type.name).cloned() else {
            self.push_diagnostic("TYPE_QUERY_PAGE_SHAPE", source, page.result.range);
            return;
        };
        let items_ok = result_fields.get("items").is_some_and(|items| {
            items.declared_type.name == "List"
                && items.declared_type.arguments == vec![projection_type.clone()]
                && !items.declared_type.nullable
        });
        let next_ok = result_fields.get("next").is_some_and(|next| {
            next.declared_type.name == cursor_type.name
                && next.declared_type.nullable
                && next.declared_type.arguments.is_empty()
        });
        if result_fields.len() != 2 || !items_ok || !next_ok {
            self.push_diagnostic("TYPE_QUERY_PAGE_SHAPE", source, page.result.range);
        }
    }

    fn page_field_types_match(&self, left: &TypeValue, right: &TypeValue) -> bool {
        left.nullable == right.nullable
            && left.arguments == right.arguments
            && (left.name == right.name
                || self.representation_root(&left.name) == self.representation_root(&right.name))
    }

    fn required_range_input_field(
        &self,
        expression: &Expression,
        environment: &BTreeMap<String, TypeValue>,
    ) -> bool {
        let Expression::Name(name) = expression else {
            return false;
        };
        if name.path.len() != 2
            || !self
                .direct_range_inputs
                .as_ref()
                .is_some_and(|inputs| inputs.contains(&name.path[0].text))
        {
            return false;
        }
        environment
            .get(&name.path[0].text)
            .filter(|record| {
                !record.nullable
                    && !record.secret
                    && self.catalogue.record_kinds.get(&record.name)
                        == Some(&jadpo_syntax::RecordKind::Input)
            })
            .and_then(|record| self.catalogue.records.get(&record.name))
            .and_then(|fields| fields.get(&name.path[1].text))
            .is_some_and(|field| {
                !field.optional && !field.declared_type.nullable && !field.declared_type.secret
            })
    }

    fn optional_input_field(
        &self,
        expression: &Expression,
        environment: &BTreeMap<String, TypeValue>,
    ) -> bool {
        let Expression::Name(name) = expression else {
            return false;
        };
        if name.path.len() != 2 {
            return false;
        }
        environment
            .get(&name.path[0].text)
            .and_then(|record| self.record_shape_name(&record.name))
            .and_then(|record| self.catalogue.records.get(&record))
            .and_then(|fields| fields.get(&name.path[1].text))
            .is_some_and(|field| field.optional)
    }
}

fn route_could_match_literal(route: &str, literal: &str) -> bool {
    let route_parts = route.split('/').collect::<Vec<_>>();
    let literal_parts = literal.split('/').collect::<Vec<_>>();
    route_parts.len() == literal_parts.len()
        && route_parts
            .iter()
            .zip(literal_parts)
            .all(|(part, literal_part)| {
                part == &literal_part || part.starts_with('{') && part.ends_with('}')
            })
}

fn resolved_type_value(reference: &TypeReference, graph: &SemanticGraph) -> TypeValue {
    let name = joined_name(&reference.path);
    TypeValue {
        nullable: reference.nullable || graph.nullable_types.contains(&name),
        name,
        arguments: reference
            .arguments
            .iter()
            .map(|argument| resolved_type_value(argument, graph))
            .collect(),
        secret: false,
    }
}

fn type_value(reference: &TypeReference) -> TypeValue {
    TypeValue {
        name: joined_name(&reference.path),
        arguments: reference.arguments.iter().map(type_value).collect(),
        nullable: reference.nullable,
        secret: false,
    }
}

fn type_reference_is_principal(reference: &TypeReference, principal: &str) -> bool {
    let name = joined_name(&reference.path);
    name == principal || name.starts_with(&format!("{principal}."))
}

fn type_reference_contains_principal(
    reference: &TypeReference,
    principal: &str,
    records: &BTreeMap<String, BTreeMap<String, RecordField>>,
    visiting: &mut BTreeSet<String>,
) -> bool {
    let value = type_value(reference);
    type_value_contains_principal(&value, principal, records, visiting)
}

fn type_value_contains_principal(
    value: &TypeValue,
    principal: &str,
    records: &BTreeMap<String, BTreeMap<String, RecordField>>,
    visiting: &mut BTreeSet<String>,
) -> bool {
    if value.name == principal || value.name.starts_with(&format!("{principal}.")) {
        return true;
    }
    if value
        .arguments
        .iter()
        .any(|argument| type_value_contains_principal(argument, principal, records, visiting))
    {
        return true;
    }
    if !visiting.insert(value.name.clone()) {
        return false;
    }
    let contains = records.get(&value.name).is_some_and(|fields| {
        fields.values().any(|field| {
            type_value_contains_principal(&field.declared_type, principal, records, visiting)
        })
    });
    visiting.remove(&value.name);
    contains
}

fn normalize_transport_field(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn valid_http_header_name(value: &str) -> bool {
    !value.is_empty()
        && value.is_ascii()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

fn reserved_route_header(name: &str) -> bool {
    name.starts_with("x-jadpo-")
        || matches!(
            name,
            "authorization"
                | "proxy-authorization"
                | "cookie"
                | "origin"
                | "sec-fetch-site"
                | "x-forwarded-user"
                | "x-auth-request-user"
                | "x-client-cert"
                | "x-forwarded-client-cert"
        )
}

fn route_method_name(method: jadpo_syntax::HttpMethod) -> &'static str {
    match method {
        jadpo_syntax::HttpMethod::Get => "GET",
        jadpo_syntax::HttpMethod::Post => "POST",
        jadpo_syntax::HttpMethod::Put => "PUT",
        jadpo_syntax::HttpMethod::Patch => "PATCH",
        jadpo_syntax::HttpMethod::Delete => "DELETE",
    }
}

fn block_uses_name(block: &Block, requested: &str) -> bool {
    block.statements.iter().any(|statement| match statement {
        Statement::Binding(statement) => expression_uses_name(&statement.value, requested),
        Statement::Assignment(statement) => expression_uses_name(&statement.value, requested),
        Statement::Return(statement) => expression_uses_name(&statement.value, requested),
        Statement::Reject(statement) => statement
            .values
            .iter()
            .any(|field| expression_uses_name(&field.value, requested)),
        Statement::If(statement) => {
            expression_uses_name(&statement.condition, requested)
                || block_uses_name(&statement.then_block, requested)
                || statement
                    .else_block
                    .as_ref()
                    .is_some_and(|block| block_uses_name(block, requested))
        }
        Statement::Match(statement) => {
            expression_uses_name(&statement.subject, requested)
                || statement
                    .arms
                    .iter()
                    .any(|arm| block_uses_name(&arm.body, requested))
        }
        Statement::Assert(statement) => expression_uses_name(&statement.condition, requested),
        Statement::AdvanceClock(statement) => expression_uses_name(&statement.duration, requested),
        Statement::Unsupported(_) => false,
    })
}

fn lifecycle_constant_identity(expression: &Expression) -> Option<String> {
    match expression {
        Expression::Literal(literal) => {
            Some(format!("literal:{:?}:{}", literal.kind, literal.text))
        }
        Expression::Name(name) => Some(format!(
            "name:{}",
            name.path
                .iter()
                .map(|part| part.text.as_str())
                .collect::<Vec<_>>()
                .join(".")
        )),
        Expression::Invocation(invocation)
            if invocation.arguments.len() == 1 && invocation.named_arguments.is_empty() =>
        {
            lifecycle_constant_identity(&invocation.arguments[0])
        }
        Expression::Grouped(grouped) => lifecycle_constant_identity(&grouped.value),
        _ => None,
    }
}

fn lifecycle_predicate_equalities(expression: &Expression) -> Option<BTreeMap<String, String>> {
    fn collect(expression: &Expression, values: &mut BTreeMap<String, String>) -> Option<()> {
        match expression {
            Expression::Grouped(grouped) => collect(&grouped.value, values),
            Expression::Binary(binary) if binary.operator == jadpo_syntax::BinaryOperator::And => {
                collect(&binary.left, values)?;
                collect(&binary.right, values)
            }
            Expression::Binary(binary)
                if binary.operator == jadpo_syntax::BinaryOperator::Equal =>
            {
                let pair = match (&*binary.left, &*binary.right) {
                    (Expression::Name(field), value) if field.path.len() == 1 => Some((
                        field.path[0].text.clone(),
                        lifecycle_constant_identity(value)?,
                    )),
                    (value, Expression::Name(field)) if field.path.len() == 1 => Some((
                        field.path[0].text.clone(),
                        lifecycle_constant_identity(value)?,
                    )),
                    _ => None,
                }?;
                if values
                    .insert(pair.0, pair.1.clone())
                    .is_some_and(|previous| previous != pair.1)
                {
                    return None;
                }
                Some(())
            }
            _ => None,
        }
    }

    let mut values = BTreeMap::new();
    collect(expression, &mut values)?;
    Some(values)
}

fn expression_uses_name(expression: &Expression, requested: &str) -> bool {
    match expression {
        Expression::Name(name) => name.path.first().is_some_and(|name| name.text == requested),
        Expression::Invocation(invocation) => {
            invocation
                .arguments
                .iter()
                .any(|argument| expression_uses_name(argument, requested))
                || invocation
                    .named_arguments
                    .iter()
                    .any(|argument| expression_uses_name(&argument.value, requested))
        }
        Expression::TestCall(call) => {
            call.invocation
                .arguments
                .iter()
                .any(|argument| expression_uses_name(argument, requested))
                || call
                    .invocation
                    .named_arguments
                    .iter()
                    .any(|argument| expression_uses_name(&argument.value, requested))
        }
        Expression::Construction(construction) => construction
            .fields
            .iter()
            .any(|field| expression_uses_name(&field.value, requested)),
        Expression::Object(object) => object
            .fields
            .iter()
            .any(|field| expression_uses_name(&field.value, requested)),
        Expression::Create(create) => create
            .fields
            .iter()
            .any(|field| expression_uses_name(&field.value, requested)),
        Expression::Query(query) => {
            expression_uses_name(&query.value, requested)
                || query.pagination.as_ref().is_some_and(|pagination| {
                    expression_uses_name(&pagination.limit, requested)
                        || expression_uses_name(&pagination.offset, requested)
                })
                || query.includes.iter().any(|include| {
                    expression_uses_name(&include.pagination.limit, requested)
                        || expression_uses_name(&include.pagination.offset, requested)
                })
        }
        Expression::Update(update) => {
            expression_uses_name(&update.value, requested)
                || update.patch.as_ref().is_some_and(|patch| {
                    patch
                        .path
                        .first()
                        .is_some_and(|name| name.text == requested)
                })
                || update
                    .changes
                    .iter()
                    .any(|field| expression_uses_name(&field.value, requested))
                || update.conditional_changes.iter().any(|conditional| {
                    expression_uses_name(&conditional.change.value, requested)
                        || conditional
                            .supplied
                            .path
                            .first()
                            .is_some_and(|name| name.text == requested)
                })
        }
        Expression::Delete(delete) => expression_uses_name(&delete.value, requested),
        Expression::Attempt(attempt) => expression_uses_name(&attempt.value, requested),
        Expression::OutcomeMatch(outcome) => {
            expression_uses_name(&outcome.subject, requested)
                || outcome.arms.iter().any(|arm| match &arm.body {
                    jadpo_syntax::OutcomeMatchArmBody::Value(value) => {
                        expression_uses_name(value, requested)
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Reject(rejection) => rejection
                        .values
                        .iter()
                        .any(|field| expression_uses_name(&field.value, requested)),
                    jadpo_syntax::OutcomeMatchArmBody::Propagate(_) => false,
                })
        }
        Expression::Unary(unary) => expression_uses_name(&unary.value, requested),
        Expression::Binary(binary) => {
            expression_uses_name(&binary.left, requested)
                || expression_uses_name(&binary.right, requested)
        }
        Expression::Grouped(grouped) => expression_uses_name(&grouped.value, requested),
        Expression::Literal(_) | Expression::Missing(_) => false,
    }
}

fn simple_type(name: &str) -> TypeValue {
    TypeValue {
        name: name.to_owned(),
        arguments: Vec::new(),
        nullable: false,
        secret: false,
    }
}

fn joined_name(path: &[Name]) -> String {
    path.iter()
        .map(|name| name.text.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

fn closest_name<'a>(requested: &str, candidates: impl Iterator<Item = &'a str>) -> Option<String> {
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

fn unquote(value: &str) -> String {
    let value = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value);
    let mut result = String::new();
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            result.push(character);
            continue;
        }
        match chars.next() {
            Some('n') => result.push('\n'),
            Some('r') => result.push('\r'),
            Some('t') => result.push('\t'),
            Some('"') => result.push('"'),
            Some('\\') => result.push('\\'),
            Some(other) => result.push(other),
            None => result.push('\\'),
        }
    }
    result
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

fn numeric_literal(literal: &Literal) -> Option<f64> {
    literal.text.parse().ok()
}

fn numeric_constraint(constraint: &Constraint) -> Option<f64> {
    constraint.value.text.parse().ok()
}

fn valid_email(value: &str) -> bool {
    let mut pieces = value.split('@');
    let local = pieces.next().unwrap_or_default();
    let domain = pieces.next().unwrap_or_default();
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && pieces.next().is_none()
        && !value.chars().any(char::is_whitespace)
}

fn valid_bcp47(value: &str) -> bool {
    let mut parts = value.split('-');
    let Some(language) = parts.next() else {
        return false;
    };
    if !(2..=8).contains(&language.len())
        || !language.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        return false;
    }
    parts.all(|part| {
        !part.is_empty() && part.len() <= 8 && part.bytes().all(|byte| byte.is_ascii_alphanumeric())
    })
}

fn canonical_bcp47(value: &str) -> Option<String> {
    if !valid_bcp47(value) {
        return None;
    }
    let mut output = Vec::new();
    let mut extension = false;
    let mut script_seen = false;
    let mut region_seen = false;
    for (index, part) in value.split('-').enumerate() {
        let canonical = if index == 0 || extension {
            part.to_ascii_lowercase()
        } else if part.len() == 1 {
            extension = true;
            part.to_ascii_lowercase()
        } else if !script_seen
            && part.len() == 4
            && part.bytes().all(|byte| byte.is_ascii_alphabetic())
        {
            script_seen = true;
            let mut characters = part.chars();
            let first = characters.next()?.to_ascii_uppercase();
            format!("{first}{}", characters.as_str().to_ascii_lowercase())
        } else if !region_seen
            && ((part.len() == 2 && part.bytes().all(|byte| byte.is_ascii_alphabetic()))
                || (part.len() == 3 && part.bytes().all(|byte| byte.is_ascii_digit())))
        {
            region_seen = true;
            part.to_ascii_uppercase()
        } else {
            part.to_ascii_lowercase()
        };
        output.push(canonical);
    }
    Some(output.join("-"))
}

fn valid_calendar_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let number = |range: std::ops::Range<usize>| value.get(range)?.parse::<u32>().ok();
    let (Some(year), Some(month), Some(day)) = (number(0..4), number(5..7), number(8..10)) else {
        return false;
    };
    if year == 0 || !(1..=12).contains(&month) {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let maximum = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=maximum).contains(&day)
}

fn valid_clock_text(value: &str) -> bool {
    let (clock, fraction) = value
        .split_once('.')
        .map_or((value, None), |(clock, fraction)| (clock, Some(fraction)));
    if fraction.is_some_and(|fraction| {
        fraction.is_empty()
            || fraction.len() > 3
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    }) {
        return false;
    }
    let fields = clock.split(':').collect::<Vec<_>>();
    if !matches!(fields.len(), 2 | 3)
        || fields
            .iter()
            .any(|field| field.len() != 2 || !field.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return false;
    }
    let hour = fields[0].parse::<u8>().ok();
    let minute = fields[1].parse::<u8>().ok();
    let second = fields.get(2).and_then(|value| value.parse::<u8>().ok());
    hour.is_some_and(|value| value <= 23)
        && minute.is_some_and(|value| value <= 59)
        && second.map_or(true, |value| value <= 59)
        && (fraction.is_none() || fields.len() == 3)
}

fn valid_instant(value: &str) -> bool {
    let Some(separator) = value.find('T') else {
        return false;
    };
    if !valid_calendar_date(&value[..separator]) {
        return false;
    }
    let time_and_offset = &value[separator + 1..];
    let (clock, offset) = if let Some(clock) = time_and_offset.strip_suffix('Z') {
        (clock, "Z")
    } else if time_and_offset.len() >= 6 {
        let split = time_and_offset.len() - 6;
        let (clock, offset) = time_and_offset.split_at(split);
        (clock, offset)
    } else {
        return false;
    };
    if !valid_clock_text(clock) || clock.matches(':').count() != 2 {
        return false;
    }
    if offset == "Z" {
        return true;
    }
    let bytes = offset.as_bytes();
    if bytes.len() != 6
        || !matches!(bytes[0], b'+' | b'-')
        || bytes[3] != b':'
        || !bytes[1..3].iter().all(u8::is_ascii_digit)
        || !bytes[4..6].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    let hour = offset[1..3].parse::<u8>().unwrap_or(99);
    let minute = offset[4..6].parse::<u8>().unwrap_or(99);
    // RFC 3339's unknown local offset does not identify a resolved Instant.
    if offset == "-00:00" || hour > 23 || minute > 59 {
        return false;
    }
    // The explicit offset can move a valid local endpoint outside the
    // portable UTC range (years 0001..9999). Offsets are less than a day,
    // so only these two boundary dates can cross that range. Fractional
    // milliseconds cannot change comparison with a whole-minute offset.
    let wall_seconds = clock[0..2].parse::<u32>().unwrap_or(0) * 3_600
        + clock[3..5].parse::<u32>().unwrap_or(0) * 60
        + clock[6..8].parse::<u32>().unwrap_or(0);
    let offset_seconds = u32::from(hour) * 3_600 + u32::from(minute) * 60;
    if &value[..separator] == "0001-01-01" && bytes[0] == b'+' {
        return wall_seconds >= offset_seconds;
    }
    if &value[..separator] == "9999-12-31" && bytes[0] == b'-' {
        return wall_seconds + offset_seconds < 86_400;
    }
    true
}

fn valid_duration(value: &str) -> bool {
    let canonical = value.strip_prefix('-').unwrap_or(value);
    let Some(mut rest) = canonical.strip_prefix("PT") else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    let mut found = false;
    let mut milliseconds = 0.0;
    for (suffix, multiplier) in [('H', 3_600_000.0), ('M', 60_000.0), ('S', 1_000.0)] {
        let Some(position) = rest.find(suffix) else {
            continue;
        };
        let number = &rest[..position];
        let valid = if suffix == 'S' {
            let mut pieces = number.split('.');
            let whole = pieces.next().unwrap_or_default();
            let fraction = pieces.next();
            !whole.is_empty()
                && whole.bytes().all(|byte| byte.is_ascii_digit())
                && fraction.map_or(true, |fraction| {
                    !fraction.is_empty()
                        && fraction.len() <= 3
                        && fraction.bytes().all(|byte| byte.is_ascii_digit())
                })
                && pieces.next().is_none()
        } else {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        };
        if !valid {
            return false;
        }
        let Ok(component) = number.parse::<f64>() else {
            return false;
        };
        milliseconds += component * multiplier;
        found = true;
        rest = &rest[position + 1..];
    }
    found
        && rest.is_empty()
        && milliseconds.is_finite()
        && milliseconds.fract() == 0.0
        && milliseconds <= 9_007_199_254_740_991.0
}

fn valid_url(value: &str) -> bool {
    let Some((scheme, remainder)) = value.split_once("://") else {
        return false;
    };
    matches!(scheme, "http" | "https")
        && !remainder.is_empty()
        && !remainder.starts_with('.')
        && !remainder.chars().any(char::is_whitespace)
        && remainder
            .split(['/', '?', '#'])
            .next()
            .is_some_and(|host| host.contains('.') || host == "localhost")
}

fn valid_ip_address(value: &str) -> bool {
    value.parse::<std::net::IpAddr>().is_ok()
}

fn matches_core_pattern(value: &str, pattern: &str) -> bool {
    match pattern {
        "[a-z0-9_]+" => {
            !value.is_empty()
                && value.chars().all(|character| {
                    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
                })
        }
        _ => true,
    }
}

fn constraint_name(kind: ConstraintKind) -> &'static str {
    match kind {
        ConstraintKind::Min => "min",
        ConstraintKind::Max => "max",
        ConstraintKind::MinLength => "min_length",
        ConstraintKind::MaxLength => "max_length",
        ConstraintKind::Pattern => "pattern",
        ConstraintKind::Format => "format",
    }
}

#[cfg(test)]
mod tests {
    use super::check_types;
    use crate::build_semantic_graph;
    use jadpo_syntax::parse;
    use std::path::Path;

    fn check(source: &str) -> super::TypeCheckResult {
        let parsed = parse(Path::new("test.jadpo"), source);
        assert!(
            parsed.diagnostics.is_empty(),
            "{source}\n{:#?}",
            parsed.diagnostics
        );
        let graph = build_semantic_graph(std::slice::from_ref(&parsed));
        assert!(graph.diagnostics.is_empty(), "{:#?}", graph.diagnostics);
        check_types(&[parsed], &graph)
    }

    #[test]
    fn reports_keyset_page_projection_and_result_shape_mismatch() {
        let result = check(
            r#"
entity Todo { id: Uuid identity }
input ListTodos { id: Uuid optional after: TodoCursor optional }
value TodoCursor { id: Todo.id }
output TodoView { id: Todo.id }
output TodoPage { items: List<Text> next: TodoCursor? }
action list(input: ListTodos) -> TodoPage {
    return attempt query page Todo -> TodoPage {
        where: id == input.id
        order_by: id desc
        after: optional input.after
        limit: 10
        project: TodoView
        cursor: TodoCursor(id)
    }
}

"#,
        );

        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TYPE_QUERY_PAGE_SHAPE"),
            "{:#?}",
            result.diagnostics
        );
    }

    #[test]
    fn typechecks_closed_entity_lifecycle_state_and_guards() {
        let result = check(
            r#"
enum UserStatus { active disabled }
entity User {
    id: Uuid identity
    status: UserStatus
    disabled_at: Instant?
    identity: id
    persistence { store: primary role: authority }
    lifecycle {
        initial: { status: User.status(UserStatus.active) disabled_at: none }
        visible when status == UserStatus.active
        transition disable {
            from: status == UserStatus.active
            set: { status: User.status(UserStatus.disabled) disabled_at: clock.now }
        }
    }
}
"#,
        );
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    }

    #[test]
    fn lifecycle_fields_cannot_be_written_by_an_ordinary_update() {
        let result = check(
            r#"
enum UserStatus { active disabled }
failure UserMissing { kind: NotFound code: "user_missing" message: "Missing" }
failure MutationConflict { kind: Conflict code: "mutation_conflict" message: "Conflict" }
input DisableUser { id: Uuid }
entity User {
    id: Uuid identity
    status: UserStatus
    disabled_at: Instant?
    identity: id
    persistence { store: primary role: authority }
    lifecycle {
        initial: { status: User.status(UserStatus.active) disabled_at: none }
        visible when status == UserStatus.active
        transition disable {
            from: status == UserStatus.active
            set: { status: User.status(UserStatus.disabled) disabled_at: clock.now }
        }
    }
}
action disable(input: DisableUser) -> Bool {
    var changed = attempt update required User {
        where: id == input.id
        set: { status: UserStatus.disabled disabled_at: clock.now }
        missing: UserMissing
        conflict: MutationConflict
    }
    return true
}
"#,
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TYPE_LIFECYCLE_FIELD_UPDATE_FORBIDDEN"),
            "{:#?}",
            result.diagnostics
        );
    }

    #[test]
    fn lifecycle_contract_rejections_have_specific_diagnostics() {
        fn entity(name: &str, body: &str, persisted: bool) -> String {
            let persistence = if persisted {
                "persistence { store: primary role: authority }"
            } else {
                ""
            };
            format!(
                "entity {name} {{ id: Uuid deleted_at: Instant? status: Text identity: id {persistence} lifecycle {{ {body} }} }}"
            )
        }

        let cases = [
            (
                "TYPE_LIFECYCLE_REQUIRES_PERSISTENT_ENTITY",
                entity("NoStore", "initial: { deleted_at: none } visible when deleted_at == none", false),
            ),
            (
                "TYPE_LIFECYCLE_INITIAL_REQUIRED",
                entity("NoInitial", "visible when deleted_at == none", true),
            ),
            (
                "TYPE_LIFECYCLE_VISIBLE_REQUIRED",
                entity("NoVisibility", "initial: { deleted_at: none }", true),
            ),
            (
                "TYPE_LIFECYCLE_INITIAL_DUPLICATE",
                entity("DuplicateInitial", "initial: { deleted_at: none deleted_at: none } visible when deleted_at == none", true),
            ),
            (
                "TYPE_LIFECYCLE_FIELD_UNKNOWN",
                entity("UnknownInitial", "initial: { ghost: none } visible when deleted_at == none", true),
            ),
            (
                "TYPE_LIFECYCLE_FIELD_INELIGIBLE",
                entity("IdentityOwned", "initial: { id: id deleted_at: none } visible when deleted_at == none", true),
            ),
            (
                "TYPE_LIFECYCLE_VALUE_UNSUPPORTED",
                entity("OpenValue", "initial: { deleted_at: supplied } visible when deleted_at == none", true),
            ),
            (
                "TYPE_LIFECYCLE_TRANSITION_FIELD_DUPLICATE",
                entity("DuplicateTransitionField", "initial: { deleted_at: none } visible when deleted_at == none transition delete { from: deleted_at == none set: { deleted_at: clock.now deleted_at: clock.now } }", true),
            ),
            (
                "TYPE_LIFECYCLE_INITIAL_FIELD_REQUIRED",
                entity("MissingInitialField", "initial: { deleted_at: none } visible when deleted_at == none transition delete { from: deleted_at == none set: { status: \"deleted\" } }", true),
            ),
            (
                "TYPE_LIFECYCLE_PURGE_FIELD_INVALID",
                entity("InvalidPurgeField", "initial: { deleted_at: none status: \"active\" } visible when deleted_at == none transition delete { from: deleted_at == none set: { status: \"deleted\" } } purge after config.retention from status", true),
            ),
            (
                "TYPE_LIFECYCLE_PURGE_INITIAL_INVALID",
                entity("InvalidPurgeInitial", "initial: { deleted_at: clock.now } visible when deleted_at == none transition delete { from: deleted_at == none set: { deleted_at: clock.now } } purge after config.retention from deleted_at", true),
            ),
            (
                "TYPE_LIFECYCLE_PURGE_RETENTION_INVALID",
                entity("InvalidRetention", "initial: { deleted_at: none } visible when deleted_at == none transition delete { from: deleted_at == none set: { deleted_at: clock.now } } purge after 30 from deleted_at", true),
            ),
            (
                "TYPE_LIFECYCLE_PURGE_TRANSITION_INVALID",
                entity("InvalidPurgeTransition", "initial: { deleted_at: none } visible when deleted_at == none transition delete { from: deleted_at == none set: { deleted_at: none } } purge after config.retention from deleted_at", true),
            ),
            (
                "TYPE_LIFECYCLE_PREDICATE_UNSUPPORTED",
                entity("UnsupportedVisibility", "initial: { deleted_at: none } visible when principal.id == id", true),
            ),
        ];

        for (expected, source) in cases {
            let result = check(&source);
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.code == expected),
                "expected {expected} for source {source:?}, got {:#?}",
                result.diagnostics
            );
        }

        let unknown_transition = format!(
            "{}\n{}",
            entity(
                "UnknownTransition",
                "initial: { deleted_at: none } visible when deleted_at == none transition delete { from: deleted_at == none set: { deleted_at: clock.now } }",
                true,
            ),
            r#"
failure TodoMissing { kind: NotFound code: "todo_missing" message: "Missing" }
failure TodoConflict { kind: Conflict code: "todo_conflict" message: "Conflict" }
action update_todo(key: UnknownTransition.id) -> Bool {
    var changed = attempt update required UnknownTransition {
        where: id == key
        transition: absent
        missing: TodoMissing
        conflict: TodoConflict
    }
    return true
}
"#,
        );
        let result = check(&unknown_transition);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "TYPE_LIFECYCLE_TRANSITION_UNKNOWN"),
            "{:#?}",
            result.diagnostics
        );
    }

    #[test]
    fn widens_fields_but_rejects_siblings() {
        let result = check(
            r#"
entity Customer { email: Email }
entity Supplier { email: Email }
function receipt(email: Customer.email) -> Customer.email { return email }
function misuse(supplier: Supplier) -> Customer.email { return receipt(supplier.email) }
"#,
        );
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, "TYPE_SIBLING_MISMATCH");
    }

    #[test]
    fn checks_route_query_and_header_transport_contracts() {
        let result = check(
            r#"
type Nested = Object { id: Uuid }
type QueryInput = Object { id: Uuid nested: Nested optional values: List<Text> optional }
type BodyInput = Object { id: Text }
route GET /items/{id} {
    auth: none
    path: { id: Uuid }
    query: QueryInput
    headers: {
        first: Text from "X-Trace" optional
        second: Text from "x-trace" optional
        auth: Text from "Authorization" optional
        nested_header: Nested from "X-Nested" optional
        invalid_name: Text from "Bad Header" optional
    }
    input: BodyInput
    action: { return true }
}
route GET /scalar { auth: none query: Text action: { return true } }
"#,
        );
        let codes = result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();
        for expected in [
            "TYPE_ROUTE_BINDING_COLLISION",
            "TYPE_ROUTE_HEADER_WIRE_DUPLICATE",
            "TYPE_AUTH_RESERVED_ROUTE_INPUT",
            "TYPE_ROUTE_HEADER_SCALAR_REQUIRED",
            "TYPE_ROUTE_HEADER_NAME_INVALID",
            "TYPE_ROUTE_QUERY_FIELD_UNSUPPORTED",
            "TYPE_ROUTE_QUERY_OBJECT_REQUIRED",
        ] {
            assert!(codes.contains(&expected), "{expected}: {codes:#?}");
        }
    }

    #[test]
    fn validates_constant_constructors() {
        let result = check(
            r#"
function email() -> Email { return Email("not-an-email") }
"#,
        );
        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].code, "TYPE_INVALID_LITERAL");
    }

    #[test]
    fn types_an_included_inverse_as_its_named_output_shape() {
        let result = check(
            r#"
entity User { id: Uuid identity inverse todos: many Todo via Todo.owner_id }
entity Todo { id: Uuid identity owner_id: User.id references User.id on_delete cascade }
output UserTodos { parent: User todos: List<Todo> }
failure UserNotFound { kind: NotFound code: "user_not_found" }
action load(user_id: User.id) -> UserTodos fails UserNotFound {
    return attempt query required User {
        where: id == user_id
        include: todos into: UserTodos order_by: id asc limit: 100 offset: 0
        missing: UserNotFound
    }
}
"#,
        );
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert!(result
            .expressions
            .iter()
            .any(|expression| expression.type_name == "UserTodos"));
    }

    #[test]
    fn rejects_unbounded_or_negative_relationship_pagination() {
        let result = check(
            r#"
entity User { id: Uuid identity inverse todos: many Todo via Todo.owner_id }
entity Todo { id: Uuid identity owner_id: User.id references User.id on_delete cascade }
output UserTodos { parent: User todos: List<Todo> }
failure UserNotFound { kind: NotFound code: "user_not_found" }
action load(user_id: User.id) -> UserTodos fails UserNotFound {
    return attempt query required User {
        where: id == user_id
        include: todos into: UserTodos order_by: id asc limit: 0 offset: -1
        missing: UserNotFound
    }
}
"#,
        );
        assert_eq!(
            result
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "TYPE_QUERY_PAGINATION_RANGE")
                .count(),
            2
        );
    }

    #[test]
    fn types_a_paginated_many_parent_include_as_a_list_of_output_shapes() {
        let result = check(
            r#"
entity User {
    id: Uuid identity
    group: Text index
    inverse todos: many Todo via Todo.owner_id
}
entity Todo { id: Uuid identity owner_id: User.id references User.id on_delete cascade }
output UserTodos { parent: User todos: List<Todo> }
action load(group: User.group) -> List<UserTodos> {
    return attempt query many User {
        where: group == group
        order_by: id asc
        limit: 20 offset: 0
        include: todos into: UserTodos order_by: id asc limit: 10 offset: 0
    }
}
"#,
        );
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert!(result
            .expressions
            .iter()
            .any(|expression| expression.type_name == "List<UserTodos>"));
    }

    #[test]
    fn permits_only_explicit_mutable_local_reassignment() {
        let result = check(
            r#"
type Choice = Text {}
function choose(initial: Choice, replacement: Choice) -> Choice {
    var mut selected: Choice = initial
    selected = replacement
    return selected
}
"#,
        );
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    }

    #[test]
    fn rejects_parameter_and_immutable_local_reassignment() {
        let result = check(
            r#"
type Choice = Text {}
type OtherChoice = Text {}
function replace_parameter(original: Choice, replacement: Choice) -> Choice {
    original = replacement
    return original
}
function replace_fixed(initial: Choice, replacement: Choice) -> Choice {
    var fixed: Choice = initial
    fixed = replacement
    return fixed
}
function replace_with_wrong_type(initial: Choice, replacement: OtherChoice) -> Choice {
    var mut selected: Choice = initial
    selected = (replacement)
    return selected
}
"#,
        );
        assert_eq!(
            result
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            vec![
                "TYPE_ASSIGN_IMMUTABLE",
                "TYPE_ASSIGN_IMMUTABLE",
                "TYPE_MISMATCH"
            ]
        );
    }

    #[test]
    fn reports_specific_assignment_construction_match_and_query_failures() {
        let cases = [
            (
                r#"type Choice = Text {}
function choose(value: Choice) -> Choice {
    unknown_local = value
    return value
}"#,
                "TYPE_ASSIGN_UNKNOWN",
            ),
            (
                r#"type Name = Text {}
function name() -> Name { return Name("one", "two") }"#,
                "TYPE_CONSTRUCTOR_ARGUMENT_COUNT",
            ),
            (
                r#"input Draft { name: Text }
action create_draft(name: Text) -> Draft {
    return create Draft { name: name }
}"#,
                "TYPE_CREATE_NOT_ENTITY",
            ),
            (
                r#"input Draft { id: Uuid }
failure DraftAbsent { kind: NotFound code: "draft_absent" }
failure Clash { kind: Conflict code: "clash" }
action delete_draft(id: Uuid) -> Draft fails DraftAbsent, Clash {
    return delete required Draft {
        where: id == id
        missing: DraftAbsent
        conflict: Clash
    }
}"#,
                "TYPE_DELETE_NOT_ENTITY",
            ),
            (
                r#"enum Outcome { accepted { note: Text } }
function outcome() -> Outcome {
    return Outcome.accepted { note: "one" note: "two" }
}"#,
                "TYPE_DUPLICATE_VARIANT_FIELD",
            ),
            (
                r#"type CustomerId = Uuid {}
type OrderId = Uuid {}
function same(customer: CustomerId, order: OrderId) -> Bool {
    return customer == order
}"#,
                "TYPE_INCOMPARABLE",
            ),
            (
                r#"type Name = Text {}
function bad(candidate: Name) -> Name {
    return candidate { field: "value" }
}"#,
                "TYPE_NOT_RECORD",
            ),
            (
                r#"input Filter { value: Text }
action find(value: Text) -> Filter? {
    return query optional Filter { where: value == value }
}"#,
                "TYPE_QUERY_NOT_ENTITY",
            ),
            (
                r#"entity Item { id: Uuid identity name: Text }
action find(value: Text) -> Item? {
    return query optional Item { where: unknown_field == value }
}"#,
                "TYPE_QUERY_UNKNOWN_FIELD",
            ),
            (
                r#"entity Item { id: Uuid identity name: Text }
action list(value: Text) -> List<Item> {
    return query many Item { where: name == value order_by: unknown_order asc }
}"#,
                "TYPE_QUERY_UNKNOWN_ORDER_FIELD",
            ),
            (
                r#"entity Item { id: Uuid identity name: Text }
action list(value: Text, page_size: Int) -> List<Item> {
    return query many Item { where: name == value order_by: id asc limit: page_size offset: 0 }
}"#,
                "TYPE_QUERY_PAGINATION_CONSTANT_REQUIRED",
            ),
            (
                r#"entity Item { id: Uuid identity group: Text }
action list(group: Text) -> List<Item> {
    return query many Item { where: group == group order_by: group asc }
}"#,
                "TYPE_QUERY_ORDER_NOT_DETERMINISTIC",
            ),
            (
                r#"enum Outcome { accepted { note: Text detail: Text } }
function describe(value: Outcome) -> Bool {
    match value { Outcome.accepted { item, item } => {} }
    return true
}"#,
                "TYPE_MATCH_DUPLICATE_BINDING",
            ),
            (
                r#"function describe(value: Bool) -> Bool {
    match value { "yes" => {} _ => {} }
    return true
}"#,
                "TYPE_MATCH_PATTERN_TYPE",
            ),
            (
                r#"type Name = Text {}
function describe(value: Name) -> Bool {
    match value { some(item) => {} _ => {} }
    return true
}"#,
                "TYPE_MATCH_SOME_NON_OPTIONAL",
            ),
            (
                r#"enum Outcome { accepted { note: Text } rejected }
function describe(value: Outcome) -> Bool {
    match value { Outcome.accepted => {} Outcome.rejected => {} }
    return true
}"#,
                "TYPE_MATCH_VARIANT_BINDINGS_REQUIRED",
            ),
            (
                r#"type Name = Text {}
function describe(value: Name) -> Bool {
    match value { "known" => {} }
    return true
}"#,
                "TYPE_MATCH_WILDCARD_REQUIRED",
            ),
            (
                r#"entity Item { id: Uuid identity }
failure ItemAbsent { kind: NotFound code: "item_absent" }
failure ItemConflict { kind: Conflict code: "item_conflict" }
action change(id: Item.id) -> Item fails ItemAbsent, ItemConflict {
    return update required Item {
        where: id == id
        set: {}
        missing: ItemAbsent
        conflict: ItemConflict
    }
}"#,
                "TYPE_UPDATE_FIELD_REQUIRED",
            ),
            (
                r#"input Draft { id: Uuid }
failure DraftAbsent { kind: NotFound code: "draft_absent" }
failure DraftConflict { kind: Conflict code: "draft_conflict" }
action change(id: Uuid) -> Draft fails DraftAbsent, DraftConflict {
    return update required Draft {
        where: id == id
        set: { id: id }
        missing: DraftAbsent
        conflict: DraftConflict
    }
}"#,
                "TYPE_UPDATE_NOT_ENTITY",
            ),
            (
                r#"entity Item { id: Uuid identity }
failure ItemAbsent { kind: NotFound code: "item_absent" }
failure ItemConflict { kind: Conflict code: "item_conflict" }
action change(id: Item.id) -> Item fails ItemAbsent, ItemConflict {
    return update required Item {
        where: id == id
        set: { unknown_field: id }
        missing: ItemAbsent
        conflict: ItemConflict
    }
}"#,
                "TYPE_UPDATE_UNKNOWN_FIELD",
            ),
            (
                r#"entity Item { id: Uuid identity nickname: Text? }
failure ItemAbsent { kind: NotFound code: "item_absent" }
failure ItemConflict { kind: Conflict code: "item_conflict" }
action change(nickname: Text) -> Item fails ItemAbsent, ItemConflict {
    return update required Item {
        where: nickname == nickname
        set: { nickname: nickname }
        missing: ItemAbsent
        conflict: ItemConflict
    }
}"#,
                "TYPE_MUTATION_NULLABLE_FIELD_UNSUPPORTED",
            ),
            (
                r#"entity Item { id: Uuid identity }
failure ItemAbsent { kind: NotFound code: "item_absent" }
failure ItemConflict { kind: Conflict code: "item_conflict" }
action change(id: Item.id) -> Item fails ItemAbsent, ItemConflict {
    return update required Item {
        where: unknown_field == id
        set: { id: id }
        missing: ItemAbsent
        conflict: ItemConflict
    }
}"#,
                "TYPE_MUTATION_UNKNOWN_PREDICATE_FIELD",
            ),
            (
                r#"entity Item { id: Uuid identity }
failure FirstConflict { kind: Conflict code: "first_conflict" }
failure SecondConflict { kind: Conflict code: "second_conflict" }
action create_item(id: Item.id) -> Item fails FirstConflict, SecondConflict {
    return create Item { id: id }
        conflict: FirstConflict
        conflict: SecondConflict
}"#,
                "TYPE_CONFLICT_DUPLICATE_BINDING",
            ),
            (
                r#"entity Item { id: Uuid identity name: Text }
input PatchItem { name: Text optional }
failure EmptyPatch { kind: InvalidValue code: "empty_patch" }
failure ItemAbsent { kind: NotFound code: "item_absent" }
failure ItemConflict { kind: Conflict code: "item_conflict" }
action patch_item(id: Item.id, input: PatchItem) -> Item fails EmptyPatch, ItemAbsent, ItemConflict {
    return update required Item {
        where: id == id
        patch: input.name
        empty: EmptyPatch
        missing: ItemAbsent
        conflict: ItemConflict
    }
}"#,
                "TYPE_PATCH_INPUT_BINDING",
            ),
            (
                r#"entity Item { id: Uuid identity name: Text }
failure EmptyPatch { kind: InvalidValue code: "empty_patch" }
failure ItemAbsent { kind: NotFound code: "item_absent" }
failure ItemConflict { kind: Conflict code: "item_conflict" }
action patch_item(id: Item.id, item: Item) -> Item fails EmptyPatch, ItemAbsent, ItemConflict {
    return update required Item {
        where: id == id
        patch: item
        empty: EmptyPatch
        missing: ItemAbsent
        conflict: ItemConflict
    }
}"#,
                "TYPE_PATCH_NOT_INPUT",
            ),
            (
                r#"entity Item { id: Uuid identity name: Text }
input EmptyPatch {}
failure EmptyChange { kind: InvalidValue code: "empty_change" }
failure ItemAbsent { kind: NotFound code: "item_absent" }
failure ItemConflict { kind: Conflict code: "item_conflict" }
action patch_item(id: Item.id, input: EmptyPatch) -> Item fails EmptyChange, ItemAbsent, ItemConflict {
    return update required Item {
        where: id == id
        patch: input
        empty: EmptyChange
        missing: ItemAbsent
        conflict: ItemConflict
    }
}"#,
                "TYPE_PATCH_FIELD_REQUIRED",
            ),
            (
                r#"entity Item { id: Uuid identity name: Text }
input PatchItem { name: Text optional }
failure EmptyChange { kind: InvalidValue code: "empty_change" }
failure ItemAbsent { kind: NotFound code: "item_absent" }
failure ItemConflict { kind: Conflict code: "item_conflict" }
action patch_item(id: Item.id, input: PatchItem, other: PatchItem) -> Item fails EmptyChange, ItemAbsent, ItemConflict {
    return update required Item {
        where: id == id
        patch: input
        empty: EmptyChange
        set: { name: input.name when other.name supplied }
        missing: ItemAbsent
        conflict: ItemConflict
    }
}"#,
                "TYPE_PATCH_CONDITION_BINDING",
            ),
            (
                r#"entity Item { id: Uuid identity name: Text }
input PatchItem { name: Text optional }
failure EmptyChange { kind: InvalidValue code: "empty_change" }
failure ItemAbsent { kind: NotFound code: "item_absent" }
failure ItemConflict { kind: Conflict code: "item_conflict" }
action patch_item(id: Item.id, input: PatchItem) -> Item fails EmptyChange, ItemAbsent, ItemConflict {
    return update required Item {
        where: id == id
        patch: input
        empty: EmptyChange
        set: { name: input.name when input.unknown_field supplied }
        missing: ItemAbsent
        conflict: ItemConflict
    }
}"#,
                "TYPE_PATCH_CONDITION_UNKNOWN_FIELD",
            ),
        ];

        for (source, expected) in cases {
            let result = check(source);
            let codes = result
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>();
            assert!(
                codes.contains(&expected),
                "{expected}: {codes:#?}\n{source}"
            );
        }
    }

    #[test]
    fn reports_each_invalid_relationship_include_contract() {
        let result = check(
            r#"
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.owner_id
    inverse profile: optional Profile via Profile.user_id
}
entity Todo {
    id: Uuid identity
    sort_key: Text
    owner_id: User.id? references User.id as owner on_delete set_null
}
entity Profile {
    id: Uuid identity
    user_id: User.id unique references User.id on_delete cascade
}
input BadResult { parent: User todos: List<Todo> }
output DifferentResult { parent: User todos: List<Todo> }
input BadProfileResult { parent: User profile: Profile }
input BadOwnerResult { parent: Todo owner: User? }
input BadNestedResult { parent: Todo owner: BadProfileResult }
failure UserAbsent { kind: NotFound code: "user_absent" }
failure TodoAbsent { kind: NotFound code: "todo_absent" }

action bad_many() -> BadResult {
    return query optional User {
        where: id == User.id("00000000-0000-0000-0000-000000000000")
        include: todos into: BadResult order_by: sort_key asc limit: 10 offset: 0
        include: todos into: DifferentResult order_by: id asc limit: 10 offset: 0
        include: profile optional into: BadResult
        include: ghosts into: BadResult order_by: id asc limit: 10 offset: 0
    }
}

action bad_many_parent_page() -> List<BadResult> {
    return query many User {
        where: id == User.id("00000000-0000-0000-0000-000000000000")
        order_by: id asc
        include: todos into: BadResult order_by: id asc limit: 10 offset: 0
    }
}

action bad_inverse_one() -> BadProfileResult {
    return query optional User {
        where: id == User.id("00000000-0000-0000-0000-000000000000")
        include: profile into: BadProfileResult order_by: id asc limit: 1 offset: 0
        include: todos into: BadProfileResult order_by: id asc limit: 1 offset: 0
    }
}

action bad_parent() -> BadOwnerResult {
    return query optional Todo {
        where: id == Todo.id("00000000-0000-0000-0000-000000000000")
        include: owner required into: BadOwnerResult
        include: owner optional into: BadOwnerResult
    }
}

action unknown_parent() -> BadOwnerResult fails TodoAbsent {
    return query required Todo {
        where: id == Todo.id("00000000-0000-0000-0000-000000000000")
        include: unknown_parent required into: BadOwnerResult
        missing: TodoAbsent
    }
}

action bad_nested() -> BadNestedResult {
    return query optional Todo {
        where: id == Todo.id("00000000-0000-0000-0000-000000000000")
        include: owner.profile required into: BadNestedResult
        include: owner.profile optional into: BadNestedResult
    }
}

action unknown_nested_parent() -> BadNestedResult fails TodoAbsent {
    return query required Todo {
        where: id == Todo.id("00000000-0000-0000-0000-000000000000")
        include: unknown_parent.profile optional into: BadNestedResult
        missing: TodoAbsent
    }
}

action unknown_nested_child() -> BadNestedResult fails TodoAbsent {
    return query required Todo {
        where: id == Todo.id("00000000-0000-0000-0000-000000000000")
        include: owner.unknown_child optional into: BadNestedResult
        missing: TodoAbsent
    }
}
"#,
        );
        let codes = result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();

        for expected in [
            "TYPE_INCLUDE_DUPLICATE_RELATIONSHIP",
            "TYPE_INCLUDE_MIXED_CARDINALITY",
            "TYPE_INCLUDE_ORDER_NOT_DETERMINISTIC",
            "TYPE_INCLUDE_PARENT_PAGINATION_REQUIRED",
            "TYPE_INCLUDE_REQUIRED_PARENT",
            "TYPE_INCLUDE_RESULT_MISMATCH",
            "TYPE_INCLUDE_RESULT_NOT_OUTPUT",
            "TYPE_INCLUDE_RESULT_SHAPE",
            "TYPE_INCLUDE_UNKNOWN_RELATIONSHIP",
            "TYPE_INVERSE_ONE_CARDINALITY",
            "TYPE_INVERSE_ONE_REQUIRED_QUERY",
            "TYPE_INVERSE_ONE_RESULT_SHAPE",
            "TYPE_INVERSE_ONE_SINGLE",
            "TYPE_NESTED_INCLUDE_CARDINALITY",
            "TYPE_NESTED_INCLUDE_FIRST_HOP",
            "TYPE_NESTED_INCLUDE_NULLABLE_FIRST_HOP",
            "TYPE_NESTED_INCLUDE_REQUIRED_QUERY",
            "TYPE_NESTED_INCLUDE_RESULT_SHAPE",
            "TYPE_NESTED_INCLUDE_SECOND_HOP",
            "TYPE_NESTED_INCLUDE_SINGLE",
            "TYPE_PARENT_INCLUDE_REQUIRED_QUERY",
            "TYPE_PARENT_INCLUDE_RESULT_SHAPE",
            "TYPE_PARENT_INCLUDE_SINGLE",
            "TYPE_PARENT_INCLUDE_UNKNOWN_REFERENCE",
        ] {
            assert!(codes.contains(&expected), "{expected}: {codes:#?}");
        }
    }

    #[test]
    fn types_multiple_inverses_as_one_exact_output_shape() {
        let result = check(
            r#"
entity User {
    id: Uuid identity
    inverse todos: many Todo via Todo.owner_id
    inverse notes: many Note via Note.owner_id
}
entity Todo { id: Uuid identity owner_id: User.id references User.id on_delete cascade }
entity Note { id: Uuid identity owner_id: User.id references User.id on_delete cascade }
output UserActivity { parent: User todos: List<Todo> notes: List<Note> }
failure UserNotFound { kind: NotFound code: "user_not_found" }
action load(user_id: User.id) -> UserActivity fails UserNotFound {
    return attempt query required User {
        where: id == user_id
        include: todos into: UserActivity order_by: id asc limit: 10 offset: 0
        include: notes into: UserActivity order_by: id desc limit: 5 offset: 0
        missing: UserNotFound
    }
}
"#,
        );
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        assert!(result
            .expressions
            .iter()
            .any(|expression| expression.type_name == "UserActivity"));
    }

    #[test]
    fn types_configuration_access_and_rejects_bad_defaults() {
        let result = check(
            r#"
type Timeout = Duration {}
type DisplayName = Text { min_length: 2 }
value ComplexValue { name: Text }
config ApplicationConfiguration {
    timeout: Timeout { binding: "TIMEOUT" default: 5s }
    count: Int { binding: "COUNT" default: "many" }
    display_name: DisplayName { binding: "DISPLAY_NAME" default: "" }
    complex: ComplexValue { binding: "COMPLEX" }
}
function configured_timeout() -> Timeout {
    return config.timeout
}
"#,
        );
        let codes = result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();
        assert!(codes.contains(&"CONFIG_DEFAULT_TYPE"), "{codes:#?}");
        assert!(codes.contains(&"CONFIG_DEFAULT_INVALID"), "{codes:#?}");
        assert!(codes.contains(&"CONFIG_TYPE_UNSUPPORTED"), "{codes:#?}");
        assert!(!codes.contains(&"TYPE_UNKNOWN_VALUE"), "{codes:#?}");
    }

    #[test]
    fn rejects_secret_configuration_at_an_ordinary_return_boundary() {
        let result = check(
            r#"
type ApiKey = Text { min_length: 3 }
config ApplicationConfiguration {
    api_key: ApiKey { binding: "API_KEY" secret: true }
}
function leak_key() -> ApiKey {
    return config.api_key
}
test "secret match" {
    match config.api_key {
        _ => {}
    }
}
"#,
        );
        let codes = result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();
        assert_eq!(
            codes
                .iter()
                .filter(|code| **code == "CONFIG_SECRET_FLOW")
                .count(),
            1,
            "{codes:#?}"
        );
        assert!(codes.contains(&"TYPE_UNKNOWN_VALUE"), "{codes:#?}");
    }

    #[test]
    fn accepts_textual_prelude_configuration_defaults() {
        let result = check(
            r#"
config ApplicationConfiguration {
    endpoint: Url { binding: "ENDPOINT" default: "https://example.com" }
    address: IpAddress { binding: "ADDRESS" default: "127.0.0.1" }
    identifier: Uuid { binding: "IDENTIFIER" default: "550e8400-e29b-41d4-a716-446655440000" }
}
"#,
        );
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    }
}
