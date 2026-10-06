use crate::{checked_source_revision, AnalyzedProject, GeneratedArtifact};
use jadpo_diagnostics::{Diagnostic, DiagnosticFact, SourceSpan};
use jadpo_syntax::{
    ApplicationDeclaration, AuthenticationStrategyDeclaration, BinaryOperator, Block,
    CallableDeclaration, ConfigDeclaration, ConfigDefaultKind, Constraint, ConstraintKind,
    Declaration, EnumDeclaration, Expression, FieldDeclaration, FieldInitialiser,
    FixtureDeclaration, HttpMethod, LiteralKind, LocalesDeclaration, MatchPattern,
    PersistenceModifier, PrincipalDeclaration, RecordDeclaration, ReferenceDeleteAction,
    RouteSuccess, Statement, TestDeclaration, TypeDeclaration, TypeReference,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

mod delivery_authority;
mod delivery_hooks;
mod delivery_invocation;
mod delivery_scheduler;
mod delivery_selection;
mod first_party;
mod liveness;

const JWT_PACKAGE: &str = include_str!("runtime/jwt/package.json");
const JWT_LOCK: &str = include_str!("runtime/jwt/bun.lock");

pub(crate) fn jwt_authentication_supported(project_path: &Path, project: &AnalyzedProject) -> bool {
    TargetGenerator::new(project_path, project)
        .is_ok_and(|generator| generator.first_party_supported() && generator.has_jwt())
}

impl TargetGenerator<'_> {
    fn has_jwt(&self) -> bool {
        self.authentication_strategies.iter().any(|strategy| {
            strategy
                .validators
                .iter()
                .any(|validator| validator.mode.text == "jwt")
        })
    }
}

pub(crate) fn first_party_authentication_supported(
    project_path: &Path,
    project: &AnalyzedProject,
) -> bool {
    TargetGenerator::new(project_path, project)
        .is_ok_and(|generator| generator.first_party_supported())
}

pub fn derive_target(
    project_path: &Path,
    project: &AnalyzedProject,
) -> Result<Vec<GeneratedArtifact>, Diagnostic> {
    let generator = TargetGenerator::new(project_path, project)?;
    let mut outputs = vec![GeneratedArtifact {
        relative_path: "target/app.ts",
        contents: generator.generate(),
    }];
    if !generator.tests.is_empty() {
        outputs.push(GeneratedArtifact {
            relative_path: "target/tests.ts",
            contents: generator.test_entrypoint(),
        });
    }
    if generator.has_authentication() {
        outputs.push(GeneratedArtifact {
            relative_path: "target/authentication.ts",
            contents: generator.authentication_target(),
        });
    }
    if generator.has_service_operations() {
        outputs.push(GeneratedArtifact {
            relative_path: "target/service-adapter.ts",
            contents: include_str!("runtime/service_adapter.ts").to_owned(),
        });
    }
    if generator.first_party_supported() {
        outputs.push(GeneratedArtifact {
            relative_path: "target/first-party-authentication.ts",
            contents: include_str!("runtime/first_party_authentication.ts").to_owned(),
        });
        if generator.has_jwt() {
            outputs.extend([
                GeneratedArtifact {
                    relative_path: "target/jwt-authentication.ts",
                    contents: include_str!("runtime/jwt_authentication.ts").to_owned(),
                },
                GeneratedArtifact {
                    relative_path: "target/package.json",
                    contents: JWT_PACKAGE.to_owned(),
                },
                GeneratedArtifact {
                    relative_path: "target/bun.lock",
                    contents: JWT_LOCK.to_owned(),
                },
                GeneratedArtifact {
                    relative_path: "audit/runtime-dependencies.json",
                    contents: include_str!("runtime/jwt/dependency.json").to_owned(),
                },
            ]);
        }
    }
    if generator.has_entities() {
        outputs.extend([
            GeneratedArtifact {
                relative_path: "target/persistence.ts",
                contents: generator.persistence_target(),
            },
            GeneratedArtifact {
                relative_path: "sql/postgres/schema.sql",
                contents: generator.schema_sql(SqlDialect::Postgres),
            },
            GeneratedArtifact {
                relative_path: "sql/sqlite/schema.sql",
                contents: generator.schema_sql(SqlDialect::Sqlite),
            },
            GeneratedArtifact {
                relative_path: "persistence/entities.json",
                contents: generator.persistence_manifest(),
            },
        ]);
    }
    if generator.has_lifecycle_purges() {
        outputs.push(GeneratedArtifact {
            relative_path: "audit/lifecycle-maintenance.json",
            contents: generator.lifecycle_maintenance_manifest(),
        });
    }
    if generator.has_lifecycles() {
        outputs.push(GeneratedArtifact {
            relative_path: "audit/lifecycles.json",
            contents: generator.lifecycle_manifest(project_path),
        });
    }
    validate_runtime_dependency_contract(
        &outputs,
        generator.first_party_supported() && generator.has_jwt(),
    )?;
    Ok(outputs)
}

fn validate_runtime_dependency_contract(
    outputs: &[GeneratedArtifact],
    jwt: bool,
) -> Result<(), Diagnostic> {
    if jwt
        && ![
            ("target/package.json", JWT_PACKAGE),
            ("target/bun.lock", JWT_LOCK),
        ]
        .iter()
        .all(|(path, expected)| {
            outputs
                .iter()
                .filter(|output| output.relative_path == *path)
                .count()
                == 1
                && outputs
                    .iter()
                    .any(|output| output.relative_path == *path && output.contents == *expected)
        })
    {
        return Err(Diagnostic::error("JADPO_TARGET_DEPENDENCY_MANIFEST"));
    }
    for output in outputs {
        let path = Path::new(output.relative_path);
        let file_name = path.file_name().and_then(|name| name.to_str());
        if matches!(file_name, Some("package.json" | "bun.lock" | "bun.lockb"))
            || path
                .components()
                .any(|component| component.as_os_str() == "node_modules")
        {
            if jwt
                && matches!(
                    (output.relative_path, output.contents.as_str()),
                    ("target/package.json", JWT_PACKAGE) | ("target/bun.lock", JWT_LOCK)
                )
            {
                continue;
            }
            return Err(Diagnostic::error("JADPO_TARGET_DEPENDENCY_MANIFEST"));
        }

        if path.extension().and_then(|extension| extension.to_str()) != Some("ts") {
            continue;
        }

        for specifier in module_specifiers(&output.contents) {
            if jwt && output.relative_path == "target/jwt-authentication.ts" && specifier == "jose"
            {
                continue;
            }
            if specifier == "bun"
                || specifier.starts_with("bun:")
                || matches!(specifier.as_str(), "node:http" | "node:net" | "node:stream")
            {
                continue;
            }
            if let Some(relative) = specifier.strip_prefix("./") {
                let dependency = path
                    .parent()
                    .unwrap_or_else(|| Path::new(""))
                    .join(relative);
                if outputs.iter().any(|candidate| {
                    Path::new(candidate.relative_path) == dependency
                        && Path::new(candidate.relative_path)
                            .extension()
                            .and_then(|extension| extension.to_str())
                            == Some("ts")
                }) {
                    continue;
                }
            }
            return Err(Diagnostic::error("JADPO_TARGET_EXTERNAL_MODULE"));
        }
    }
    Ok(())
}

fn module_specifiers(source: &str) -> Vec<String> {
    let mut specifiers = Vec::new();
    for (marker, quote) in [
        (" from \"", '"'),
        (" from '", '\''),
        ("import \"", '"'),
        ("import '", '\''),
        ("import(\"", '"'),
        ("import('", '\''),
        ("import (\"", '"'),
        ("import ('", '\''),
        ("require(\"", '"'),
        ("require('", '\''),
    ] {
        let mut remaining = source;
        while let Some(start) = remaining.find(marker) {
            let value = &remaining[start + marker.len()..];
            if let Some(end) = value.find(quote) {
                specifiers.push(value[..end].to_owned());
                remaining = &value[end + quote.len_utf8()..];
            } else {
                break;
            }
        }
    }
    specifiers
}

#[derive(Clone, Copy)]
enum SqlDialect {
    Postgres,
    Sqlite,
}

struct TargetGenerator<'project> {
    project: &'project AnalyzedProject,
    types: BTreeMap<String, &'project TypeDeclaration>,
    enums: BTreeMap<String, &'project EnumDeclaration>,
    records: BTreeMap<String, &'project RecordDeclaration>,
    failures: BTreeMap<String, &'project jadpo_syntax::FailureDeclaration>,
    callables: BTreeMap<String, &'project CallableDeclaration>,
    fixtures: BTreeMap<String, &'project FixtureDeclaration>,
    tests: Vec<&'project TestDeclaration>,
    configuration: Option<&'project ConfigDeclaration>,
    application: Option<&'project ApplicationDeclaration>,
    locales: Option<&'project LocalesDeclaration>,
    principal: Option<&'project PrincipalDeclaration>,
    authentication_strategies: Vec<&'project AuthenticationStrategyDeclaration>,
    source_revision: String,
}

impl<'project> TargetGenerator<'project> {
    fn new(project_path: &Path, project: &'project AnalyzedProject) -> Result<Self, Diagnostic> {
        let mut types = BTreeMap::new();
        let mut enums = BTreeMap::new();
        let mut records = BTreeMap::new();
        let mut failures = BTreeMap::new();
        let mut callables = BTreeMap::new();
        let mut fixtures = BTreeMap::new();
        let mut tests = Vec::new();
        let mut configuration = None;
        let mut application = None;
        let mut locales = None;
        let mut principal = None;
        let mut authentication_strategies = Vec::new();

        for source in &project.syntax.sources {
            for declaration in &source.file.declarations {
                match declaration {
                    Declaration::Application(declaration) => application = Some(declaration),
                    Declaration::Locales(declaration) => locales = Some(declaration),
                    Declaration::Principal(declaration) => principal = Some(declaration),
                    Declaration::Config(declaration) => configuration = Some(declaration),
                    Declaration::Type(declaration) => {
                        types.insert(declaration.name.text.clone(), declaration);
                    }
                    Declaration::Enum(declaration) => {
                        enums.insert(declaration.name.text.clone(), declaration);
                    }
                    Declaration::Record(declaration) => {
                        if declaration
                            .dossier
                            .as_ref()
                            .and_then(|dossier| dossier.persistence.as_ref())
                            .is_some_and(|persistence| persistence.store.text != "primary")
                        {
                            let mut diagnostic = Diagnostic::error(
                                "JADPO_TARGET_STORE_NOT_IMPLEMENTED",
                            )
                            .with_fact(DiagnosticFact::Name(
                                declaration
                                    .dossier
                                    .as_ref()
                                    .and_then(|dossier| dossier.persistence.as_ref())
                                    .expect("store was checked")
                                    .store
                                    .text
                                    .clone(),
                            ));
                            diagnostic.primary = Some(SourceSpan {
                                source: source.source_name.clone(),
                                start: declaration.name.range.start,
                                end: declaration.name.range.end,
                            });
                            return Err(diagnostic);
                        }
                        records.insert(declaration.name.text.clone(), declaration);
                    }
                    Declaration::Callable(declaration) => {
                        if declaration.consistency
                            == Some(jadpo_syntax::ConsistencyDisposition::DurableWorkflow)
                        {
                            let mut diagnostic =
                                Diagnostic::error("JADPO_TARGET_DURABLE_WORKFLOW_NOT_IMPLEMENTED")
                                    .with_fact(DiagnosticFact::Callable(
                                        declaration.name.text.clone(),
                                    ));
                            diagnostic.primary = Some(SourceSpan {
                                source: source.source_name.clone(),
                                start: declaration.name.range.start,
                                end: declaration.name.range.end,
                            });
                            return Err(diagnostic);
                        }
                        callables.insert(declaration.name.text.clone(), declaration);
                    }
                    Declaration::Fixture(declaration) => {
                        fixtures.insert(declaration.name.text.clone(), declaration);
                    }
                    Declaration::Test(declaration) => tests.push(declaration),
                    Declaration::Failure(declaration) => {
                        failures.insert(declaration.name.text.clone(), declaration);
                    }
                    Declaration::AuthenticationStrategy(declaration) => {
                        authentication_strategies.push(declaration);
                    }
                    Declaration::Job(job) => {
                        let mut diagnostic = Diagnostic::error("JADPO_TARGET_JOB_NOT_IMPLEMENTED")
                            .with_fact(DiagnosticFact::Name(job.name.text.clone()));
                        diagnostic.primary = Some(SourceSpan {
                            source: source.source_name.clone(),
                            start: job.range.start,
                            end: job.range.end,
                        });
                        return Err(diagnostic);
                    }
                    Declaration::Route(_) => {}
                }
            }
        }

        let generator = Self {
            project,
            types,
            enums,
            records,
            failures,
            callables,
            fixtures,
            tests,
            configuration,
            application,
            locales,
            principal,
            authentication_strategies,
            source_revision: checked_source_revision(project_path, project),
        };
        for source in &project.syntax.sources {
            for declaration in &source.file.declarations {
                if let Declaration::AuthenticationStrategy(strategy) = declaration {
                    if let Some(exchange) = &strategy.exchange {
                        if !generator.first_party_supported() {
                            let mut diagnostic = Diagnostic::error(
                                "JADPO_TARGET_AUTH_NOT_IMPLEMENTED",
                            )
                            .with_fact(DiagnosticFact::Route(format!(
                                "POST {}",
                                unquote(&exchange.path.text)
                            )));
                            diagnostic.primary = Some(SourceSpan {
                                source: source.source_name.clone(),
                                start: exchange.range.start,
                                end: exchange.range.end,
                            });
                            return Err(diagnostic);
                        }
                    }
                }
                if let Declaration::Route(route) = declaration {
                    if Self::is_liveness_route(route)
                        && generator.authored_liveness_response(route).is_none()
                    {
                        let mut diagnostic = Diagnostic::error("JADPO_TARGET_LIVENESS_NOT_LOCAL")
                            .with_fact(DiagnosticFact::Route("GET /health/live".to_owned()));
                        diagnostic.primary = Some(SourceSpan {
                            source: source.source_name.clone(),
                            start: route.range.start,
                            end: route.range.end,
                        });
                        return Err(diagnostic);
                    }
                    if !route.public && !generator.first_party_supported() {
                        let name = format!("{} {}", method_name(route.method), route.path);
                        let mut diagnostic = Diagnostic::error("JADPO_TARGET_AUTH_NOT_IMPLEMENTED")
                            .with_fact(DiagnosticFact::Route(name.clone()))
                            .with_impact(name);
                        diagnostic.primary = Some(SourceSpan {
                            source: source.source_name.clone(),
                            start: route.range.start,
                            end: route.path_range.end,
                        });
                        return Err(diagnostic);
                    }
                }
            }
        }
        Ok(generator)
    }

    fn has_authentication(&self) -> bool {
        self.application.is_some()
            && self.principal.is_some()
            && !self.authentication_strategies.is_empty()
    }

    fn has_service_credential_exchange(&self) -> bool {
        self.authentication_strategies
            .iter()
            .any(|strategy| strategy.exchange.is_some())
    }

    fn has_service_operations(&self) -> bool {
        !self.project.semantics.external_effects.is_empty()
    }

    fn readiness_advisories(&self) -> String {
        let services = self
            .project
            .semantics
            .external_effects
            .iter()
            .map(|effect| snake_case(&effect.service))
            .collect::<BTreeSet<_>>();
        format!(
            "{{{}}}",
            services
                .iter()
                .map(|service| format!("{}: \"unprobed\"", ts_string(service)))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }

    fn service_effect(&self, name: &str) -> Option<&jadpo_semantic::ExternalServiceEffect> {
        self.project
            .semantics
            .external_effects
            .iter()
            .find(|effect| format!("{}.{}", effect.service, effect.operation) == name)
    }

    fn service_operation_names(&self) -> BTreeSet<String> {
        self.project
            .semantics
            .external_effects
            .iter()
            .map(|effect| format!("{}.{}", effect.service, effect.operation))
            .collect()
    }

    fn authentication_target(&self) -> String {
        let application = self
            .application
            .expect("authentication target requires an application");
        let principal = self
            .principal
            .expect("authentication target requires a principal");
        let mut output = String::new();
        line(&mut output, "// Generated by Jadpo 0.0.1. Do not edit.");
        line(
            &mut output,
            "// Credential material remains inside this compiler-owned boundary.",
        );
        line(&mut output, "");
        line(
            &mut output,
            "export type PrincipalKind = \"user\" | \"service\";",
        );
        let principal_variants = principal
            .variants
            .iter()
            .map(|variant| {
                let values = variant
                    .fields
                    .iter()
                    .filter(|field| {
                        !matches!(field.name.text.as_str(), "subject" | "authentication_strength")
                    })
                    .map(|field| {
                        format!(
                            "readonly {}{}: {}",
                            field.name.text,
                            if field.optional { "?" } else { "" },
                            self.authentication_ts_type(&field.field_type)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                format!(
                    "Readonly<{{ kind: {}; subject: {}; authenticationStrength: {}; values: Readonly<{{ {values} }}> }}>",
                    ts_string(variant.kind.as_str()),
                    variant.fields.iter().find(|f| f.name.text == "subject")
                        .map_or_else(|| "string".to_owned(), |f| self.authentication_ts_type(&f.field_type)),
                    variant.fields.iter().find(|f| f.name.text == "authentication_strength")
                        .map_or_else(|| "string".to_owned(), |f| self.authentication_ts_type(&f.field_type))
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        line(
            &mut output,
            &format!("export type AuthPrincipal = {principal_variants};"),
        );
        line(&mut output, "export type ValidatedIdentity = Readonly<{ principal: AuthPrincipal; authorityRequired: boolean }>; ");
        line(&mut output, "export type ValidationResult = { kind: \"valid\"; identity: ValidatedIdentity } | { kind: \"authoritative\"; principalKind: PrincipalKind; subject: string; resolution: ResolutionResult } | { kind: \"invalid\" } | { kind: \"unavailable\" } | { kind: \"misconfigured\" }; ");
        line(&mut output, "export type ResolutionResult = { kind: \"active\"; principal: AuthPrincipal } | { kind: \"inactive\"; failureName: string } | { kind: \"missing\" } | { kind: \"duplicate\" } | { kind: \"unavailable\" }; ");
        line(&mut output, "export type AuthenticationAdapter = Readonly<{ validate(strategy: string, credential: string): Promise<ValidationResult>; resolve(identity: ValidatedIdentity): Promise<ResolutionResult> }>; ");
        line(
            &mut output,
            "export type AuthenticationRequirement = Readonly<{ freshAuthority: boolean }>; ",
        );
        line(&mut output, "type CredentialCandidate = Readonly<{ strategy: string; values: readonly string[]; malformed: boolean }>; ");
        line(&mut output, "");
        line(
            &mut output,
            "export class AuthenticationFault extends Error {",
        );
        line(&mut output, "  constructor(readonly code: \"authentication_required\" | \"invalid_credentials\" | \"ambiguous_credentials\" | \"authentication_unavailable\" | \"authentication_misconfigured\" | \"principal_inactive\" | \"authority_invariant\" | \"csrf_rejected\", readonly status: number, readonly declaredFailure: string | null = null) {");
        line(&mut output, "    super(code);");
        line(&mut output, "    this.name = \"AuthenticationFault\";");
        line(&mut output, "  }");
        line(&mut output, "}");
        line(&mut output, "");

        let revocation_mode = match application.authentication.revocation.mode {
            jadpo_syntax::RevocationMode::Immediate => "immediate",
            jadpo_syntax::RevocationMode::Bounded => "bounded",
        };
        let maximum_delay = application
            .authentication
            .revocation
            .maximum_delay
            .as_ref()
            .map(|delay| ts_string(&delay.text))
            .unwrap_or_else(|| "null".to_owned());
        line(&mut output, "export const authenticationContract = {");
        line(
            &mut output,
            &format!("  principal: {},", ts_string(&principal.name.text)),
        );
        line(
            &mut output,
            &format!(
                "  revocation: {{ mode: {}, maximumDelay: {maximum_delay} }},",
                ts_string(revocation_mode)
            ),
        );
        line(&mut output, "  strategies: [");
        for strategy in &self.authentication_strategies {
            let (transport, location) = match &strategy.transport.location {
                jadpo_syntax::CredentialLocation::Cookie(cookie) => {
                    ("cookie", unquote(&cookie.text).to_owned())
                }
                jadpo_syntax::CredentialLocation::Bearer(location) => {
                    ("bearer", location.text.clone())
                }
            };
            let validators = strategy
                .validators
                .iter()
                .map(|validator| {
                    format!(
                        "{{ name: {}, mode: {}, principal: {} }}",
                        ts_string(&validator.name.text),
                        ts_string(&validator.mode.text),
                        ts_string(&validator.principal.text)
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            line(
                &mut output,
                &format!(
                    "    {{ name: {}, transport: {}, location: {}, validators: [{validators}] }},",
                    ts_string(&strategy.name.text),
                    ts_string(transport),
                    ts_string(&location)
                ),
            );
        }
        line(&mut output, "  ],");
        line(&mut output, "} as const;");
        line(&mut output, "");
        line(
            &mut output,
            "function authInvariant(): never { throw new AuthenticationFault(\"authority_invariant\", 500); }",
        );
        line(&mut output, "function authRecord(value: unknown): Record<string, unknown> { if (typeof value !== \"object\" || value === null || Array.isArray(value)) return authInvariant(); return value as Record<string, unknown>; }");
        line(&mut output, "function authText(value: unknown, _path?: string): string { if (typeof value !== \"string\") return authInvariant(); return value; }");
        line(&mut output, "function authUuid(value: unknown, path?: string): string { const text = authText(value, path); if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(text)) return authInvariant(); return text; }");
        line(&mut output, "function authBool(value: unknown, _path?: string): boolean { if (typeof value !== \"boolean\") return authInvariant(); return value; }");
        line(&mut output, "function authInt(value: unknown, _path?: string): number { if (typeof value !== \"number\" || !Number.isSafeInteger(value)) return authInvariant(); return value; }");
        line(&mut output, "function authDecimal(value: unknown, _path?: string): number { if (typeof value !== \"number\" || !Number.isFinite(value)) return authInvariant(); return value; }");
        line(&mut output, "function authEnum<const Values extends readonly string[]>(value: unknown, _path: string, allowed: Values): Values[number] { const text = authText(value); if (!allowed.includes(text)) return authInvariant(); return text as Values[number]; }");
        line(&mut output, "function authHasOwn(value: Record<string, unknown>, key: string): boolean { return Object.prototype.hasOwnProperty.call(value, key); }");
        line(&mut output, "function authExactKeys(value: Record<string, unknown>, allowed: readonly string[]): void { if (Object.keys(value).some(key => !allowed.includes(key))) authInvariant(); }");
        line(&mut output, "");
        if self.first_party_supported() {
            line(
                &mut output,
                "// Transport/proof mode does not establish multi-factor assurance.",
            );
            line(&mut output, "export function firstPartyAuthenticationStrength(kind: PrincipalKind, proofMode: string): AuthPrincipal[\"authenticationStrength\"] {");
            line(&mut output, "  if (!(kind === \"user\" ? [\"signed\", \"opaque\", \"jwt\"] : kind === \"service\" ? [\"api_key\"] : []).includes(proofMode)) return authInvariant();");
            line(&mut output, "  switch (kind) {");
            for variant in &principal.variants {
                let configured = self.authentication_strategies.iter().any(|strategy| {
                    strategy
                        .validators
                        .iter()
                        .any(|validator| validator.principal.text == variant.kind.as_str())
                });
                let typed_strength = variant
                    .fields
                    .iter()
                    .find(|field| field.name.text == "authentication_strength")
                    .is_some_and(|field| {
                        self.enums
                            .contains_key(&self.representation_root_for(&field.field_type))
                    });
                let strength = if !configured {
                    "authInvariant()"
                } else if typed_strength {
                    "\"primary\""
                } else {
                    "proofMode"
                };
                line(
                    &mut output,
                    &format!(
                        "    case {}: return {strength};",
                        ts_string(variant.kind.as_str())
                    ),
                );
            }
            line(&mut output, "    default: return authInvariant();");
            line(&mut output, "  }");
            line(&mut output, "}");
            line(&mut output, "");
        }
        line(
            &mut output,
            "function normalizeAuthPrincipal(value: unknown): AuthPrincipal {",
        );
        line(&mut output, "  const object = authRecord(value);");
        line(&mut output, "  authExactKeys(object, [\"kind\", \"subject\", \"authenticationStrength\", \"values\"]);");
        line(&mut output, "  const kind = authText(object.kind);");
        line(&mut output, "  const values = authRecord(object.values);");
        line(&mut output, "  switch (kind) {");
        for variant in &principal.variants {
            let fields = variant
                .fields
                .iter()
                .filter(|field| {
                    !matches!(
                        field.name.text.as_str(),
                        "subject" | "authentication_strength"
                    )
                })
                .collect::<Vec<_>>();
            let allowed = fields
                .iter()
                .map(|field| ts_string(&field.name.text))
                .collect::<Vec<_>>()
                .join(", ");
            line(
                &mut output,
                &format!("    case {}: {{", ts_string(variant.kind.as_str())),
            );
            for (source, target) in [
                ("subject", "subject"),
                ("authentication_strength", "authenticationStrength"),
            ] {
                let value = format!("object.{target}");
                let validation = variant
                    .fields
                    .iter()
                    .find(|field| field.name.text == source)
                    .map_or_else(
                        || format!("authText({value})"),
                        |field| {
                            self.authentication_validation_expression(
                                &field.field_type,
                                &value,
                                &ts_string(source),
                            )
                        },
                    );
                line(
                    &mut output,
                    &format!("      const {target} = {validation};"),
                );
            }
            line(
                &mut output,
                &format!("      authExactKeys(values, [{allowed}]);"),
            );
            for field in &fields {
                if !field.optional {
                    line(
                        &mut output,
                        &format!(
                            "      if (!authHasOwn(values, {})) authInvariant();",
                            ts_string(&field.name.text)
                        ),
                    );
                }
            }
            line(
                &mut output,
                "      const normalizedValues = Object.freeze({",
            );
            for field in &fields {
                let field_name = ts_string(&field.name.text);
                let value = format!("values[{field_name}]");
                let validation = self.authentication_validation_expression(
                    &field.field_type,
                    &value,
                    &field_name,
                );
                if field.optional {
                    line(
                        &mut output,
                        &format!(
                            "        ...(authHasOwn(values, {field_name}) ? {{ {}: {validation} }} : {{}}),",
                            field.name.text
                        ),
                    );
                } else {
                    line(
                        &mut output,
                        &format!("        {}: {validation},", field.name.text),
                    );
                }
            }
            line(&mut output, "      });");
            line(
                &mut output,
                &format!(
                    "      return Object.freeze({{ kind: {} as const, subject, authenticationStrength, values: normalizedValues }});",
                    ts_string(variant.kind.as_str())
                ),
            );
            line(&mut output, "    }");
        }
        line(&mut output, "    default: return authInvariant();");
        line(&mut output, "  }");
        line(&mut output, "}");
        line(&mut output, "");
        line(&mut output, "function normalizeValidatedIdentity(value: unknown): ValidatedIdentity { const object = authRecord(value); authExactKeys(object, [\"principal\", \"authorityRequired\"]); return Object.freeze({ principal: normalizeAuthPrincipal(object.principal), authorityRequired: authBool(object.authorityRequired) }); }");
        line(&mut output, "");
        line(
            &mut output,
            "function cookieValues(header: string | null, name: string): string[] {",
        );
        line(&mut output, "  if (header === null) return [];");
        line(&mut output, "  const values: string[] = [];");
        line(&mut output, "  for (const part of header.split(\";\")) {");
        line(&mut output, "    const separator = part.indexOf(\"=\");");
        line(
            &mut output,
            "    if (separator < 0) { if (part.trim() === name) values.push(\"\"); continue; }",
        );
        line(&mut output, "    if (part.slice(0, separator).trim() === name) values.push(part.slice(separator + 1).trim());");
        line(&mut output, "  }");
        line(&mut output, "  return values;");
        line(&mut output, "}");
        line(&mut output, "");
        line(&mut output, "function bearerValues(header: string | null): { values: string[]; malformed: boolean } {");
        line(
            &mut output,
            "  if (header === null) return { values: [], malformed: false };",
        );
        line(
            &mut output,
            "  const entries = header.split(\",\").map(value => value.trim());",
        );
        line(&mut output, "  const values: string[] = [];");
        line(&mut output, "  let malformed = false;");
        line(&mut output, "  for (const entry of entries) {");
        line(
            &mut output,
            "    const match = /^Bearer[ \\t]+([^ \\t,]+)$/iu.exec(entry);",
        );
        line(&mut output, "    if (match === null) { malformed = true; values.push(\"\"); } else { values.push(match[1]); }");
        line(&mut output, "  }");
        line(&mut output, "  return { values, malformed };");
        line(&mut output, "}");
        line(&mut output, "");
        line(&mut output, "export function inventoryAuthenticationCredentials(request: Request): readonly CredentialCandidate[] {");
        line(
            &mut output,
            "  const candidates: CredentialCandidate[] = [];",
        );
        line(
            &mut output,
            "  for (const strategy of authenticationContract.strategies) {",
        );
        line(&mut output, "    if (strategy.transport === \"cookie\") {");
        line(&mut output, "      const values = cookieValues(request.headers.get(\"cookie\"), strategy.location);");
        line(&mut output, "      candidates.push({ strategy: strategy.name, values, malformed: values.some(value => value.length === 0) });");
        line(&mut output, "    } else {");
        line(
            &mut output,
            "      const bearer = bearerValues(request.headers.get(\"authorization\"));",
        );
        line(&mut output, "      candidates.push({ strategy: strategy.name, values: bearer.values, malformed: bearer.malformed });");
        line(&mut output, "    }");
        line(&mut output, "  }");
        line(&mut output, "  return candidates;");
        line(&mut output, "}");
        line(&mut output, "");
        line(&mut output, "export async function authenticateRequest(request: Request, requirement: AuthenticationRequirement, adapter: AuthenticationAdapter): Promise<AuthPrincipal> {");
        line(&mut output, "  if ((request.headers.get(\"cookie\")?.length ?? 0) > 16384 || (request.headers.get(\"authorization\")?.length ?? 0) > 8192) throw new AuthenticationFault(\"invalid_credentials\", 401);");
        line(&mut output, "  const presented = inventoryAuthenticationCredentials(request).filter(candidate => candidate.values.length > 0);");
        line(&mut output, "  if (presented.length === 0) throw new AuthenticationFault(\"authentication_required\", 401);");
        line(&mut output, "  if (presented.length !== 1 || presented[0].values.length !== 1) throw new AuthenticationFault(\"ambiguous_credentials\", 401);");
        line(&mut output, "  const candidate = presented[0];");
        line(&mut output, "  if (candidate.malformed) throw new AuthenticationFault(\"invalid_credentials\", 401);");
        line(&mut output, "  let validation: ValidationResult;");
        line(&mut output, "  try { validation = await adapter.validate(candidate.strategy, candidate.values[0]); } catch { throw new AuthenticationFault(\"authentication_unavailable\", 503); }");
        line(
            &mut output,
            "  const validationObject = authRecord(validation);",
        );
        line(
            &mut output,
            "  const validationKind = authText(validationObject.kind);",
        );
        line(&mut output, "  if (validationKind === \"invalid\") { authExactKeys(validationObject, [\"kind\"]); throw new AuthenticationFault(\"invalid_credentials\", 401); }");
        line(&mut output, "  if (validationKind === \"unavailable\") { authExactKeys(validationObject, [\"kind\"]); throw new AuthenticationFault(\"authentication_unavailable\", 503); }");
        line(&mut output, "  if (validationKind === \"misconfigured\") { authExactKeys(validationObject, [\"kind\"]); throw new AuthenticationFault(\"authentication_misconfigured\", 500); }");
        // External JWTs have no local identity fields. Their verified subject is
        // resolved once before this result, including on fresh routes. No fake
        // local UUID or authority result cached across requests is needed.
        output.push_str(r#"
  if (validationKind === "authoritative") {
    authExactKeys(validationObject, ["kind", "principalKind", "subject", "resolution"]);
    const kind = authText(validationObject.principalKind);
    const subject = authText(validationObject.subject);
    const strategy = authenticationContract.strategies.find(item => item.name === candidate.strategy);
    if (subject.length === 0 || subject.length > 512 || strategy === undefined
        || !strategy.validators.some(validator => validator.mode === "jwt" && validator.principal === kind)) {
      throw new AuthenticationFault("authority_invariant", 500);
    }
    return normalizeAuthenticationResolution(validationObject.resolution, kind, subject);
  }
"#);
        line(&mut output, "  if (validationKind !== \"valid\") throw new AuthenticationFault(\"authority_invariant\", 500);");
        line(
            &mut output,
            "  authExactKeys(validationObject, [\"kind\", \"identity\"]);",
        );
        line(
            &mut output,
            "  const identity = normalizeValidatedIdentity(validationObject.identity);",
        );
        line(&mut output, "  const strategy = authenticationContract.strategies.find(strategy => strategy.name === candidate.strategy);");
        line(&mut output, "  if (strategy === undefined) throw new AuthenticationFault(\"authority_invariant\", 500);");
        line(&mut output, "  if (!strategy.validators.some(validator => validator.principal === identity.principal.kind)) throw new AuthenticationFault(\"invalid_credentials\", 401);");
        line(&mut output, "  const authorityRequired = authenticationContract.revocation.mode === \"immediate\" || requirement.freshAuthority || identity.authorityRequired;");
        line(
            &mut output,
            "  if (!authorityRequired) return identity.principal;",
        );
        line(&mut output, "  let resolution: ResolutionResult;");
        line(&mut output, "  try { resolution = await adapter.resolve(identity); } catch { throw new AuthenticationFault(\"authentication_unavailable\", 503); }");
        line(&mut output, "  return normalizeAuthenticationResolution(resolution, identity.principal.kind, identity.principal.subject);");
        line(&mut output, "}");
        output.push_str(r#"
function normalizeAuthenticationResolution(value: unknown, kind: string, subject: string): AuthPrincipal {
  const resolution = authRecord(value);
  const result = authText(resolution.kind);
  if (result === "active") {
    authExactKeys(resolution, ["kind", "principal"]);
    const principal = normalizeAuthPrincipal(resolution.principal);
    if (principal.kind !== kind || principal.subject !== subject) authInvariant();
    return principal;
  }
  if (result === "inactive") {
    authExactKeys(resolution, ["kind", "failureName"]);
    throw new AuthenticationFault("principal_inactive", 401, authText(resolution.failureName));
  }
  if (result === "missing") {
    authExactKeys(resolution, ["kind"]);
    throw new AuthenticationFault("invalid_credentials", 401);
  }
  if (result === "unavailable") {
    authExactKeys(resolution, ["kind"]);
    throw new AuthenticationFault("authentication_unavailable", 503);
  }
  return authInvariant();
}
"#);
        output
    }

    fn generate(&self) -> String {
        let mut output = String::new();
        line(&mut output, "// Generated by Jadpo 0.0.1. Do not edit.");
        line(
            &mut output,
            "// Authored source and semantic metadata are the review surfaces.",
        );
        if self.has_service_operations() {
            line(&mut output, "import { invokeReferenceMail, ServiceAdapterFault } from \"./service-adapter.ts\";");
        }
        if self.has_route_headers() {
            line(
                &mut output,
                "import { createServer, type IncomingMessage, type ServerResponse } from \"node:http\";",
            );
            line(&mut output, "import { Readable } from \"node:stream\";");
        }
        if self
            .records
            .values()
            .any(|record| record.is_persistent_entity())
        {
            let isolated_test_import = if self.tests.is_empty() {
                ""
            } else {
                ", createIsolatedTestPersistence"
            };
            let lifecycle_import = if self.has_lifecycle_purges() {
                ", configureLifecycleRetention as bindLifecycleRetention"
            } else {
                ""
            };
            // Finished delivery bindings need a read-only app -> persistence
            // cycle for current issuer provenance. Keep both names as live
            // imports, never sample the uninitialized persistence export.
            let delivery_import = if self.project.delivery_model().bindings().is_empty() {
                ""
            } else {
                ", persistence"
            };
            line(
                &mut output,
                &format!(
                    "import {{ persistence as rootPersistence, PersistenceFault, PolicyFault, isPersistenceReady as databaseIsReady, isPersistenceInitializationFatal as persistenceInitializationFatal, refreshPersistenceReadiness as refreshDatabaseReadiness{isolated_test_import}{lifecycle_import}{delivery_import} }} from \"./persistence.ts\";"
                ),
            );
            if self.project.delivery_model().bindings().is_empty() {
                line(&mut output, "const persistence = rootPersistence;");
            }
            if self.first_party_supported() {
                line(&mut output, "import { authenticationStorage, resolveAuthenticationAuthority } from \"./persistence.ts\";");
                let exchange_import = if self.has_service_credential_exchange() {
                    ", inventoryAuthenticationCredentials"
                } else {
                    ""
                };
                line(
                    &mut output,
                    &format!("import {{ AuthenticationFault, firstPartyAuthenticationStrength{exchange_import} }} from \"./authentication.ts\";"),
                );
                let delivery_reader = if self.project.delivery_model().bindings().is_empty() {
                    ""
                } else {
                    ", readDeliveryCredentialProof"
                };
                line(&mut output, &format!("import {{ createFirstPartyAuthentication{delivery_reader} }} from \"./first-party-authentication.ts\";"));
                if self.has_jwt() {
                    line(
                        &mut output,
                        "import { createJwtAuthentication } from \"./jwt-authentication.ts\";",
                    );
                }
            }
            line(&mut output, "");
        }
        line(&mut output, "");
        self.runtime_prelude(&mut output);
        self.temporal_runtime(&mut output);
        self.type_aliases(&mut output);
        self.principal_runtime(&mut output);
        self.validators(&mut output);
        self.configuration_runtime(&mut output);
        self.first_party_application(&mut output);
        self.delivery_selection_bridges(&mut output);
        self.delivery_invocation_bridges(&mut output);
        self.delivery_scheduler_bridges(&mut output);
        self.failure_contracts(&mut output);
        self.invoke_authorization_runtime(&mut output);
        self.service_operation_implementations(&mut output);
        self.callable_implementations(&mut output);
        self.test_implementations(&mut output);
        self.lifecycle_maintenance(&mut output);
        self.http_handler(&mut output);
        output
    }

    fn lifecycle_maintenance(&self, output: &mut String) {
        if !self.has_lifecycle_purges() {
            return;
        }
        line(
            output,
            "export async function runLifecycleMaintenance(): Promise<void> {",
        );
        line(
            output,
            "  if (configuration === undefined) configuration = loadConfiguration(Bun.env);",
        );
        line(output, "  bindLifecycleRetention(configuration);");
        line(output, "  const attemptNow = new Date().toISOString();");
        for (name, record) in self.entities() {
            let Some(purge) = record
                .dossier
                .as_ref()
                .and_then(|dossier| dossier.lifecycle.as_ref())
                .and_then(|lifecycle| lifecycle.purge.as_ref())
            else {
                continue;
            };
            let Expression::Name(binding) = &purge.after else {
                continue;
            };
            if binding.path.len() != 2 || binding.path[0].text != "config" {
                continue;
            }
            let retention_field = &binding.path[1].text;
            line(output, "  {");
            line(output, "    const selected: { count: number; tokens: readonly string[] } = { count: 0, tokens: [] };");
            line(output, &format!("    const retentionMs = durationMilliseconds(configuration.{retention_field}, {});", ts_string(&format!("config.{retention_field}"))));
            line(output, "    if (retentionMs <= 0) throw new Error(\"Retention duration must be positive\");");
            line(output, "    try {");
            line(output, &format!("      const result = await rootPersistence.withOperationTime(attemptNow).retention_purge_{name}((count, tokens) => {{ selected.count = count; selected.tokens = tokens; }});"));
            line(output, &format!("      console.error(JSON.stringify({{ schemaVersion: 1, kind: \"maintenance_audit\", eventName: \"retention.purge.batch\", operation: {}, entity: {}, clause: {}, sourceRevision: {}, retentionBinding: {}, attemptNow, selectedCount: selected.count, committedCount: result.committedCount, rowIdentityTokens: result.rowIdentityTokens, bound: 500, outcome: \"committed\" }}));", ts_string(&format!("retention_purge({name})")), ts_string(name), ts_string(&format!("{name}.lifecycle.purge")), ts_string(&self.source_revision()), ts_string(&format!("config.{retention_field}"))));
            line(output, "    } catch (error) {");
            line(output, "      const rolledBack = error instanceof PersistenceFault && error.transaction?.outcome === \"no_commit\" && error.transaction.rollbackProven;");
            line(output, &format!("      console.error(JSON.stringify({{ schemaVersion: 1, kind: \"maintenance_audit\", eventName: \"retention.purge.batch\", operation: {}, entity: {}, clause: {}, sourceRevision: {}, retentionBinding: {}, attemptNow, selectedCount: selected.count, committedCount: rolledBack ? 0 : null, rowIdentityTokens: selected.tokens, bound: 500, outcome: rolledBack ? \"rolled_back\" : \"unknown\" }}));", ts_string(&format!("retention_purge({name})")), ts_string(name), ts_string(&format!("{name}.lifecycle.purge")), ts_string(&self.source_revision()), ts_string(&format!("config.{retention_field}"))));
            line(output, "      throw error;");
            line(output, "    }");
            line(output, "  }");
        }
        line(output, "}");
        line(output, "");
    }

    fn temporal_runtime(&self, output: &mut String) {
        line(
            output,
            "type Instant = string & { readonly __instant: unique symbol };",
        );
        line(
            output,
            "type CalendarDate = string & { readonly __calendarDate: unique symbol };",
        );
        line(
            output,
            "type Duration = string & { readonly __duration: unique symbol };",
        );
        line(
            output,
            "type Zone = string & { readonly __zone: unique symbol };",
        );
        line(
            output,
            "type Locale = string & { readonly __locale: unique symbol };",
        );
        line(
            output,
            "type PresentationText = string & { readonly __presentationText: unique symbol };",
        );
        line(
            output,
            "type Time = Readonly<{ instant: Instant; zone: Zone }>;",
        );
        line(
            output,
            "type InstantRange = Readonly<{ start: Instant; end: Instant }>;",
        );
        line(
            output,
            "type LocalOverlap = \"reject\" | \"earlier\" | \"later\";",
        );
        line(
            output,
            "type LocalGap = \"reject\" | \"shift_forward\" | \"shift_backward\";",
        );
        line(output, "type InvalidDay = \"reject\" | \"last_valid_day\";");
        line(output, "type Weekday = \"monday\" | \"tuesday\" | \"wednesday\" | \"thursday\" | \"friday\" | \"saturday\" | \"sunday\";");
        line(output, "type TimeFormat = \"date_full\" | \"date_long\" | \"date_medium\" | \"date_short\" | \"time_full\" | \"time_long\" | \"time_medium\" | \"time_short\" | \"date_time_full\" | \"date_time_long\" | \"date_time_medium\" | \"date_time_short\";");
        line(output, "type FriendlyTimeFormat = \"conversational\";");
        line(output, "");
        line(output, "const jadpoZones = new Set<string>([");
        for zone in include_str!("../../../data/iana-zones-2026c.txt").lines() {
            line(output, &format!("  {},", ts_string(zone)));
        }
        line(output, "]);");
        line(output, "const jadpoLocales = new Set<string>([");
        if let Some(locales) = self.locales {
            for locale in &locales.supported {
                line(output, &format!("  {},", ts_string(unquote(&locale.text))));
            }
        }
        line(output, "]);");
        let default_locale = self
            .locales
            .map(|locales| ts_string(unquote(&locales.default.text)))
            .unwrap_or_else(|| "null".to_owned());
        let unsupported_locale = self
            .locales
            .map(|locales| match locales.unsupported {
                jadpo_syntax::LocaleUnsupported::FallbackToDefault => "fallback_to_default",
                jadpo_syntax::LocaleUnsupported::Reject => "reject",
            })
            .map(ts_string)
            .unwrap_or_else(|| "null".to_owned());
        line(output, "for (const locale of jadpoLocales) { const canonical = Intl.getCanonicalLocales(locale); if (canonical.length !== 1 || canonical[0] !== locale || Intl.DateTimeFormat.supportedLocalesOf([locale], { localeMatcher: \"lookup\" }).length !== 1) throw new Error(`Unsupported generated Locale ${locale}`); }");
        line(output, "for (const zone of jadpoZones) { try { new Intl.DateTimeFormat(\"en\", { timeZone: zone }); } catch { throw new Error(`Unsupported generated Zone ${zone}`); } }");
        line(output, "export const temporalProvenance = Object.freeze({");
        line(output, "  tzdb: \"IANA 2026c\",");
        line(output, "  zoneEnumCount: jadpoZones.size,");
        line(output, &format!("  defaultLocale: {default_locale},"));
        line(
            output,
            &format!("  unsupportedLocale: {unsupported_locale},"),
        );
        line(
            output,
            "  localeData: \"compiler-owned Intl boundary; declared locales and zones verified at startup\",",
        );
        line(
            output,
            "  runtimeIcu: process.versions.icu ?? \"unreported\",",
        );
        line(output, "  runtimeEngine: `Bun ${Bun.version}`,");
        line(
            output,
            "  timezoneRules: \"runtime ICU; version recorded for release pinning\",",
        );
        line(output, "});");
        line(output, "");
        output.push_str(r#"type LocalParts = Readonly<{ year: number; month: number; day: number; hour: number; minute: number; second: number; millisecond: number }>;
type PolicyPrincipal = Readonly<{ kind: "user" | "service"; subject: string; authenticationStrength: string; values: Readonly<Record<string, unknown>> }>;
type TestServiceOutcome = Readonly<{ kind: "accepted"; value: Readonly<Record<string, unknown>> }> | Readonly<{ kind: "declared"; name: string }>;
type TestServiceFake = { readonly outcomes: readonly TestServiceOutcome[]; readonly cursor: { index: number }; elapsedMs: number };
type TestServiceFakes = Readonly<Record<string, TestServiceFake>>;
type OperationContext = Readonly<{ now: Instant; monotonicStartedAt: number; deadlineAt: number | null; serviceAttemptBudget: { attemptsUsed: number }; principal: PolicyPrincipal | null; operationId: string; retryAllowed: boolean; signal: AbortSignal | null; testServiceFakes: TestServiceFakes | null; refreshNow: () => Instant }>;

class RequestDeadlineFault extends Error {
  readonly code = "JADPO_REQUEST_DEADLINE";
  constructor() { super("declared operation deadline exceeded"); this.name = "RequestDeadlineFault"; }
}

function captureOperation(fixedNow: Instant | null = null, principal: PolicyPrincipal | null = null, refreshNow?: () => Instant, retryAllowed = false, signal: AbortSignal | null = null, testServiceFakes: TestServiceFakes | null = null): OperationContext {
  const timeSource = refreshNow ?? (fixedNow === null ? () => validateInstant(new Date().toISOString(), "clock.now") : () => fixedNow);
  const now = timeSource();
  return Object.freeze({ now, monotonicStartedAt: performance.now(), deadlineAt: null, serviceAttemptBudget: { attemptsUsed: 0 }, principal, operationId: crypto.randomUUID(), retryAllowed, signal, testServiceFakes, refreshNow: timeSource });
}

function withOperationDeadline(operation: OperationContext, durationMs: number): OperationContext {
  const deadlineAt = Math.min(operation.deadlineAt ?? Infinity, performance.now() + durationMs);
  return Object.freeze({ ...operation, deadlineAt });
}

function assertOperationDeadline(operation: OperationContext): void {
  if (operation.deadlineAt !== null && performance.now() >= operation.deadlineAt) throw new RequestDeadlineFault();
}

function refreshOperation(operation: OperationContext): OperationContext {
  return Object.freeze({ ...operation, now: operation.refreshNow() });
}

function allowOperationRetries(operation: OperationContext, allowed: boolean): OperationContext {
  return Object.freeze({ ...operation, retryAllowed: allowed });
}

async function withMonotonicDeadline<T>(duration: Duration, operation: () => Promise<T>): Promise<T> {
  const timeout = durationMilliseconds(duration, "deadline");
  if (timeout < 0) invalid("deadline", "non-negative Duration");
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      operation(),
      new Promise<T>((_resolve, reject) => { timer = setTimeout(() => reject(new Error("monotonic_deadline_exceeded")), timeout); }),
    ]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

const localFormatterCache = new Map<string, Intl.DateTimeFormat>();
function localFormatter(zone: Zone): Intl.DateTimeFormat {
  const existing = localFormatterCache.get(zone);
  if (existing !== undefined) return existing;
  const formatter = new Intl.DateTimeFormat("en-CA-u-ca-iso8601-nu-latn", {
    timeZone: zone, calendar: "iso8601", numberingSystem: "latn", hourCycle: "h23",
    year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit", fractionalSecondDigits: 3,
  });
  localFormatterCache.set(zone, formatter);
  return formatter;
}

function localParts(instant: Instant, zone: Zone): LocalParts {
  instant = validateInstant(instant, "temporal local instant");
  zone = validateZone(zone, "temporal local zone");
  const values: Record<string, string> = {};
  for (const part of localFormatter(zone).formatToParts(new Date(instant))) if (part.type !== "literal") values[part.type] = part.value;
  return Object.freeze({ year: Number(values.year), month: Number(values.month), day: Number(values.day), hour: Number(values.hour), minute: Number(values.minute), second: Number(values.second), millisecond: Number(values.fractionalSecond) });
}

function utcMilliseconds(parts: LocalParts): number {
  const value = new Date(0);
  value.setUTCFullYear(parts.year, parts.month - 1, parts.day);
  value.setUTCHours(parts.hour, parts.minute, parts.second, parts.millisecond);
  return value.getTime();
}

function sameParts(left: LocalParts, right: LocalParts): boolean {
  return left.year === right.year && left.month === right.month && left.day === right.day && left.hour === right.hour && left.minute === right.minute && left.second === right.second && left.millisecond === right.millisecond;
}

function parseClock(value: unknown): Pick<LocalParts, "hour" | "minute" | "second" | "millisecond"> {
  if (typeof value !== "string") invalid("temporal.resolve.at", "strict clock text");
  const match = /^(?:([01][0-9]|2[0-3])):([0-5][0-9])(?::([0-5][0-9])(?:\.([0-9]{1,3}))?)?$/u.exec(value);
  if (match === null) invalid("temporal.resolve.at", "HH:MM with optional seconds and milliseconds");
  return { hour: Number(match[1]), minute: Number(match[2]), second: Number(match[3] ?? 0), millisecond: Number((match[4] ?? "").padEnd(3, "0")) };
}

function calendarFromUtc(milliseconds: number): CalendarDate {
  const date = new Date(milliseconds);
  return validateCalendarDate(`${String(date.getUTCFullYear()).padStart(4, "0")}-${String(date.getUTCMonth() + 1).padStart(2, "0")}-${String(date.getUTCDate()).padStart(2, "0")}`, "calendar date");
}

function resolveLocal(parts: LocalParts, zone: Zone, overlap: LocalOverlap, gap: LocalGap): Time {
  const naive = utcMilliseconds(parts);
  const offsets = new Set<number>();
  for (let sample = naive - 36 * 3_600_000; sample <= naive + 36 * 3_600_000; sample += 6 * 3_600_000) {
    const candidate = validateInstant(new Date(sample).toISOString(), "temporal.resolve.sample");
    offsets.add(utcMilliseconds(localParts(candidate, zone)) - sample);
  }
  const candidates = [...offsets]
    .map(offset => naive - offset)
    .filter(milliseconds => Number.isFinite(milliseconds))
    .map(milliseconds => validateInstant(new Date(milliseconds).toISOString(), "temporal.resolve"))
    .filter(instant => sameParts(localParts(instant, zone), parts))
    .sort((left, right) => Date.parse(left) - Date.parse(right));
  const unique = [...new Set(candidates)];
  if (unique.length === 1) return Object.freeze({ instant: unique[0], zone });
  if (unique.length > 1) {
    if (overlap === "reject") invalid("temporal.resolve", "unambiguous local time or explicit overlap policy");
    return Object.freeze({ instant: overlap === "earlier" ? unique[0] : unique[unique.length - 1], zone });
  }
  if (gap === "reject") invalid("temporal.resolve", "existing local time or explicit gap policy");
  const orderedOffsets = [...offsets].sort((left, right) => left - right);
  const transition = Math.abs(orderedOffsets[orderedOffsets.length - 1] - orderedOffsets[0]);
  if (transition === 0) invalid("temporal.resolve", "representable local time");
  const shifted = new Date(naive + (gap === "shift_forward" ? transition : -transition));
  return resolveLocal({ year: shifted.getUTCFullYear(), month: shifted.getUTCMonth() + 1, day: shifted.getUTCDate(), hour: shifted.getUTCHours(), minute: shifted.getUTCMinutes(), second: shifted.getUTCSeconds(), millisecond: shifted.getUTCMilliseconds() }, zone, overlap, "reject");
}

function resolveOptions(options: { date: CalendarDate; at: string; zone: Zone; overlap: LocalOverlap; gap: LocalGap }): Time {
  const date = validateCalendarDate(options.date, "temporal.resolve.date");
  const zone = validateZone(options.zone, "temporal.resolve.zone");
  const overlap = temporalChoice(options.overlap, ["reject", "earlier", "later"] as const, "temporal.resolve.overlap");
  const gap = temporalChoice(options.gap, ["reject", "shift_forward", "shift_backward"] as const, "temporal.resolve.gap");
  const [year, month, day] = date.split("-").map(Number);
  return resolveLocal({ year, month, day, ...parseClock(options.at) }, zone, overlap, gap);
}

function decodeDatabaseInstant(value: unknown, path: string): Instant {
  if (value instanceof Date) return validateInstant(value.toISOString(), path);
  if (typeof value === "number" && Number.isSafeInteger(value)) return validateInstant(new Date(value).toISOString(), path);
  return validateInstant(value, path);
}

function decodeDatabaseCalendarDate(value: unknown, path: string): CalendarDate {
  if (value instanceof Date) return validateCalendarDate(value.toISOString().slice(0, 10), path);
  return validateCalendarDate(value, path);
}

function decodeDatabaseDuration(value: unknown, path: string): Duration {
  if (typeof value === "number" && Number.isSafeInteger(value)) return durationFromMilliseconds(value);
  return validateDuration(value, path);
}

function decodeDatabaseTime(value: unknown, path: string): Time {
  if (typeof value === "string") {
    try { return validateTime(JSON.parse(value), path); } catch (error) { if (error instanceof ValidationError) throw error; return invalid(path, "stored Time"); }
  }
  return validateTime(value, path);
}

function timelineInstant(value: Instant | Time): Instant {
  return typeof value === "string" ? validateInstant(value, "temporal value") : validateTime(value, "temporal value").instant;
}

function temporalChoice<const Values extends readonly string[]>(value: unknown, allowed: Values, path: string): Values[number] {
  if (typeof value !== "string" || !allowed.includes(value)) invalid(path, allowed.join(" | "));
  return value as Values[number];
}

function dateAdd(date: CalendarDate, years: number, months: number, days: number, invalidDay: InvalidDay): CalendarDate {
  const value = validateCalendarDate(date, "temporal date");
  invalidDay = temporalChoice(invalidDay, ["reject", "last_valid_day"] as const, "temporal date arithmetic.invalid_day");
  const [year, month, day] = value.split("-").map(Number);
  const targetMonth = month - 1 + months;
  const first = new Date(0); first.setUTCFullYear(year + years, targetMonth, 1); first.setUTCHours(0, 0, 0, 0);
  const lastDay = new Date(0); lastDay.setUTCFullYear(first.getUTCFullYear(), first.getUTCMonth() + 1, 0); lastDay.setUTCHours(0, 0, 0, 0);
  if (day > lastDay.getUTCDate() && invalidDay === "reject") invalid("temporal date arithmetic", "valid target calendar day");
  first.setUTCDate(Math.min(day, lastDay.getUTCDate()) + days);
  return calendarFromUtc(first.getTime());
}

function addLocal(time: Time, years: number, months: number, days: number, invalidDay: InvalidDay, overlap: LocalOverlap, gap: LocalGap): Time {
  const value = validateTime(time, "temporal local arithmetic");
  overlap = temporalChoice(overlap, ["reject", "earlier", "later"] as const, "temporal local arithmetic.overlap");
  gap = temporalChoice(gap, ["reject", "shift_forward", "shift_backward"] as const, "temporal local arithmetic.gap");
  const parts = localParts(value.instant, value.zone);
  const date = dateAdd(calendarFromUtc(utcMilliseconds({ ...parts, hour: 0, minute: 0, second: 0, millisecond: 0 })), years, months, days, invalidDay);
  const [year, month, day] = date.split("-").map(Number);
  return resolveLocal({ ...parts, year, month, day }, value.zone, overlap, gap);
}

function periodBounds(date: CalendarDate, zone: Zone, next: (date: CalendarDate) => CalendarDate): InstantRange {
  const start = resolveOptions({ date, at: "00:00", zone, overlap: "reject", gap: "reject" }).instant;
  const end = resolveOptions({ date: next(date), at: "00:00", zone, overlap: "reject", gap: "reject" }).instant;
  return Object.freeze({ start, end });
}

const weekdayNames: readonly Weekday[] = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"];

function formatOptions(style: TimeFormat): Intl.DateTimeFormatOptions {
  style = temporalChoice(style, ["date_full", "date_long", "date_medium", "date_short", "time_full", "time_long", "time_medium", "time_short", "date_time_full", "date_time_long", "date_time_medium", "date_time_short"] as const, "temporal.format.style");
  const [kind, width] = style.startsWith("date_time_") ? ["date_time", style.slice(10)] : style.startsWith("date_") ? ["date", style.slice(5)] : ["time", style.slice(5)];
  return { ...(kind === "date" || kind === "date_time" ? { dateStyle: width as "full" | "long" | "medium" | "short" } : {}), ...(kind === "time" || kind === "date_time" ? { timeStyle: width as "full" | "long" | "medium" | "short" } : {}) };
}

function componentOptions(components: Record<string, string>): Intl.DateTimeFormatOptions {
  if (typeof components !== "object" || components === null || Array.isArray(components)) invalid("temporal.format.components", "closed component object");
  const allowed: Record<string, readonly string[]> = {
    weekday: ["long", "short", "narrow"],
    day: ["numeric", "two_digit"],
    year: ["numeric", "two_digit"],
    hour: ["numeric", "two_digit"],
    minute: ["numeric", "two_digit"],
    second: ["numeric", "two_digit"],
    month: ["numeric", "two_digit", "long", "short", "narrow"],
    zone_name: ["long", "short", "long_offset", "short_offset", "long_generic", "short_generic"],
    hour_cycle: ["h11", "h12", "h23", "h24"],
  };
  const output: Record<string, string> = {};
  const mapping: Record<string, string> = { zone_name: "timeZoneName", hour_cycle: "hourCycle" };
  const entries = Object.entries(components);
  if (entries.length === 0) invalid("temporal.format.components", "at least one component");
  for (const [key, value] of entries) {
    if (allowed[key] === undefined || typeof value !== "string" || !allowed[key].includes(value)) invalid(`temporal.format.components.${key}`, allowed[key]?.join(" | ") ?? "known component");
    output[mapping[key] ?? key] = value === "two_digit" ? "2-digit" : value.replaceAll("_", "-");
  }
  return output as Intl.DateTimeFormatOptions;
}

function formatAbsolute(value: CalendarDate | Time, options: { locale: Locale; style?: TimeFormat; components?: Record<string, string> }): PresentationText {
  const locale = validateLocale(options.locale, "temporal.format.locale");
  if ((options.style === undefined) === (options.components === undefined)) invalid("temporal.format", "exactly one of style or components");
  if (typeof value === "string") {
    const date = validateCalendarDate(value, "temporal.format.value");
    return new Intl.DateTimeFormat(locale, { timeZone: "UTC", ...(options.style === undefined ? componentOptions(options.components!) : formatOptions(options.style)) }).format(new Date(`${date}T00:00:00.000Z`)) as PresentationText;
  }
  const time = validateTime(value, "temporal.format.value");
  return new Intl.DateTimeFormat(locale, { timeZone: time.zone, ...(options.style === undefined ? componentOptions(options.components!) : formatOptions(options.style)) }).format(new Date(time.instant)) as PresentationText;
}

function friendly(value: Time, options: { relative_to: Instant; locale: Locale; profile: FriendlyTimeFormat }): PresentationText {
  if (options.profile !== "conversational") invalid("temporal.format_friendly.profile", "FriendlyTimeFormat.conversational");
  const time = validateTime(value, "temporal.format_friendly.value");
  const relative = validateInstant(options.relative_to, "temporal.format_friendly.relative_to");
  const locale = validateLocale(options.locale, "temporal.format_friendly.locale");
  const difference = Date.parse(time.instant) - Date.parse(relative);
  const relativeFormatter = new Intl.RelativeTimeFormat(locale, { numeric: "always" });
  const naturalRelativeFormatter = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  if (Math.abs(difference) < 60_000) {
    const language = locale.toLowerCase().split("-")[0];
    if (language === "en") return (difference <= 0 ? "just now" : "in under a minute") as PresentationText;
    return (difference <= 0 ? naturalRelativeFormatter.format(0, "second") : relativeFormatter.format(1, "minute")) as PresentationText;
  }
  if (Math.abs(difference) < 3_600_000) return relativeFormatter.format(Math.ceil(difference / 60_000), "minute") as PresentationText;
  const target = localParts(time.instant, time.zone); const reference = localParts(relative, time.zone);
  const targetDay = Date.UTC(target.year, target.month - 1, target.day); const referenceDay = Date.UTC(reference.year, reference.month - 1, reference.day); const dayDifference = Math.round((targetDay - referenceDay) / 86_400_000);
  const weekdayTimeFormatter = new Intl.DateTimeFormat(locale, { timeZone: time.zone, weekday: "long", hour: "numeric", minute: "2-digit" });
  const targetDate = new Date(time.instant);
  if (Math.abs(dayDifference) <= 1) {
    const relativeDay = naturalRelativeFormatter.format(dayDifference, "day");
    return weekdayTimeFormatter.formatToParts(targetDate).map(part => part.type === "weekday" ? relativeDay : part.value).join("") as PresentationText;
  }
  if (Math.abs(dayDifference) <= 6) return weekdayTimeFormatter.format(targetDate) as PresentationText;
  return formatAbsolute(time, { locale, style: "date_time_long" });
}

export const temporal = Object.freeze({
  in_zone(instant: Instant, zone: Zone): Time { return Object.freeze({ instant: validateInstant(instant, "temporal.in_zone.instant"), zone: validateZone(zone, "temporal.in_zone.zone") }); },
  resolve: resolveOptions,
  add_elapsed(value: Instant | Time, duration: Duration): Instant | Time { const instant = validateInstant(new Date(Date.parse(timelineInstant(value)) + durationMilliseconds(duration, "temporal.add_elapsed.duration")).toISOString(), "temporal.add_elapsed"); return typeof value === "string" ? instant : Object.freeze({ instant, zone: value.zone }); },
  between(start: Instant | Time, end: Instant | Time): Duration { return durationFromMilliseconds(Date.parse(timelineInstant(end)) - Date.parse(timelineInstant(start))); },
  add_days(date: CalendarDate, options: { days: number }): CalendarDate { return dateAdd(date, 0, 0, validateInt(options.days, "temporal.add_days.days"), "reject"); },
  add_weeks(date: CalendarDate, options: { weeks: number }): CalendarDate { return dateAdd(date, 0, 0, validateInt(options.weeks, "temporal.add_weeks.weeks") * 7, "reject"); },
  add_months(date: CalendarDate, options: { months: number; invalid_day: InvalidDay }): CalendarDate { return dateAdd(date, 0, validateInt(options.months, "temporal.add_months.months"), 0, options.invalid_day); },
  add_years(date: CalendarDate, options: { years: number; invalid_day: InvalidDay }): CalendarDate { return dateAdd(date, validateInt(options.years, "temporal.add_years.years"), 0, 0, options.invalid_day); },
  add_local_days(time: Time, options: { days: number; overlap: LocalOverlap; gap: LocalGap }): Time { return addLocal(time, 0, 0, validateInt(options.days, "temporal.add_local_days.days"), "reject", options.overlap, options.gap); },
  add_local_weeks(time: Time, options: { weeks: number; overlap: LocalOverlap; gap: LocalGap }): Time { return addLocal(time, 0, 0, validateInt(options.weeks, "temporal.add_local_weeks.weeks") * 7, "reject", options.overlap, options.gap); },
  add_local_months(time: Time, options: { months: number; invalid_day: InvalidDay; overlap: LocalOverlap; gap: LocalGap }): Time { return addLocal(time, 0, validateInt(options.months, "temporal.add_local_months.months"), 0, options.invalid_day, options.overlap, options.gap); },
  add_local_years(time: Time, options: { years: number; invalid_day: InvalidDay; overlap: LocalOverlap; gap: LocalGap }): Time { return addLocal(time, validateInt(options.years, "temporal.add_local_years.years"), 0, 0, options.invalid_day, options.overlap, options.gap); },
  day_bounds(date: CalendarDate, zone: Zone): InstantRange { return periodBounds(date, zone, value => dateAdd(value, 0, 0, 1, "reject")); },
  week_bounds(date: CalendarDate, zone: Zone, options: { starts_on: Weekday }): InstantRange { date = validateCalendarDate(date, "temporal.week_bounds.date"); zone = validateZone(zone, "temporal.week_bounds.zone"); const startsOn = temporalChoice(options?.starts_on, weekdayNames, "temporal.week_bounds.starts_on"); const day = new Date(`${date}T00:00:00Z`).getUTCDay(); const startIndex = weekdayNames.indexOf(startsOn); const start = dateAdd(date, 0, 0, -((day - startIndex + 7) % 7), "reject"); return periodBounds(start, zone, value => dateAdd(value, 0, 0, 7, "reject")); },
  month_bounds(date: CalendarDate, zone: Zone): InstantRange { date = validateCalendarDate(date, "temporal.month_bounds.date"); zone = validateZone(zone, "temporal.month_bounds.zone"); const [year, month] = date.split("-").map(Number); const start = validateCalendarDate(`${String(year).padStart(4, "0")}-${String(month).padStart(2, "0")}-01`, "temporal.month_bounds"); return periodBounds(start, zone, value => dateAdd(value, 0, 1, 0, "reject")); },
  year_bounds(date: CalendarDate, zone: Zone): InstantRange { date = validateCalendarDate(date, "temporal.year_bounds.date"); zone = validateZone(zone, "temporal.year_bounds.zone"); const year = date.slice(0, 4); const start = validateCalendarDate(`${year}-01-01`, "temporal.year_bounds"); return periodBounds(start, zone, value => dateAdd(value, 1, 0, 0, "reject")); },
  calendar_date(time: Time): CalendarDate { const parts = localParts(time.instant, time.zone); return validateCalendarDate(`${String(parts.year).padStart(4, "0")}-${String(parts.month).padStart(2, "0")}-${String(parts.day).padStart(2, "0")}`, "temporal.calendar_date"); },
  year(time: Time): number { return localParts(time.instant, time.zone).year; }, month(time: Time): number { return localParts(time.instant, time.zone).month; }, day(time: Time): number { return localParts(time.instant, time.zone).day; },
  weekday(time: Time): Weekday { const value = validateTime(time, "temporal.weekday"); const parts = localParts(value.instant, value.zone); const date = validateCalendarDate(`${String(parts.year).padStart(4, "0")}-${String(parts.month).padStart(2, "0")}-${String(parts.day).padStart(2, "0")}`, "temporal.weekday"); return weekdayNames[new Date(`${date}T00:00:00Z`).getUTCDay()]; },
  hour(time: Time): number { return localParts(time.instant, time.zone).hour; }, minute(time: Time): number { return localParts(time.instant, time.zone).minute; }, second(time: Time): number { return localParts(time.instant, time.zone).second; }, millisecond(time: Time): number { return localParts(time.instant, time.zone).millisecond; },
  offset(time: Time): Duration { const value = validateTime(time, "temporal.offset"); return durationFromMilliseconds(utcMilliseconds(localParts(value.instant, value.zone)) - Date.parse(value.instant)); },
  zone(time: Time): Zone { return validateTime(time, "temporal.zone").zone; },
  same_zone(left: Time, right: Time): boolean { return validateTime(left, "temporal.same_zone.left").zone === validateTime(right, "temporal.same_zone.right").zone; },
  same_local(left: Time, right: Time): boolean { const a = validateTime(left, "temporal.same_local.left"); const b = validateTime(right, "temporal.same_local.right"); return sameParts(localParts(a.instant, a.zone), localParts(b.instant, b.zone)); },
  format: formatAbsolute,
  format_friendly: friendly,
});

"#);
    }

    fn configuration_runtime(&self, output: &mut String) {
        let Some(configuration) = self.configuration else {
            return;
        };
        line(output, "type ApplicationConfiguration = {");
        for field in &configuration.fields {
            line(
                output,
                &format!(
                    "  readonly {}: {};",
                    field.name.text,
                    self.ts_type(&field.field_type)
                ),
            );
        }
        line(output, "};");
        line(output, "let configuration: ApplicationConfiguration;");
        line(output, "");
        line(
            output,
            "function parseConfigurationBool(value: string, field: string): boolean {",
        );
        line(output, "  if (value === \"true\") return true;");
        line(output, "  if (value === \"false\") return false;");
        line(output, "  return invalid(field, \"Bool\");");
        line(output, "}");
        line(
            output,
            "function parseConfigurationInt(value: string, field: string): number {",
        );
        line(
            output,
            "  if (!/^-?(?:0|[1-9][0-9]*)$/u.test(value)) return invalid(field, \"Int\");",
        );
        line(output, "  return validateInt(Number(value), field);");
        line(output, "}");
        line(
            output,
            "function parseConfigurationDecimal(value: string, field: string): number {",
        );
        line(
            output,
            "  if (value.trim() === \"\") return invalid(field, \"Decimal\");",
        );
        line(output, "  return validateDecimal(Number(value), field);");
        line(output, "}");
        line(
            output,
            "function normalizeConfigurationDuration(value: string, field: string): string {",
        );
        line(output, "  if (value.startsWith(\"P\") || value.startsWith(\"-P\")) return validateDuration(value, field);");
        line(
            output,
            "  const match = /^(-?\\d+(?:\\.\\d+)?)(ms|s|m|h|d)$/u.exec(value);",
        );
        line(
            output,
            "  if (match === null) return invalid(field, \"Duration\");",
        );
        line(output, "  const [, amount, unit] = match;");
        line(output, "  const multiplier = unit === \"d\" ? 86_400_000 : unit === \"h\" ? 3_600_000 : unit === \"m\" ? 60_000 : unit === \"s\" ? 1_000 : 1;");
        line(
            output,
            "  const milliseconds = Number(amount) * multiplier;",
        );
        line(output, "  if (!Number.isSafeInteger(milliseconds)) return invalid(field, \"millisecond-precise Duration\");");
        line(output, "  return durationFromMilliseconds(milliseconds);");
        line(output, "}");
        line(output, "function requireConfigurationValue(environment: Record<string, string | undefined>, binding: string, field: string, fallback?: string): string {");
        line(output, "  const value = environment[binding] ?? fallback;");
        line(output, "  if (value === undefined || value === \"\") throw new Error(`Missing configuration field ${field}`);");
        line(output, "  return value;");
        line(output, "}");
        line(output, "function loadConfiguration(environment: Record<string, string | undefined>): ApplicationConfiguration {");
        let retention_fields = self.lifecycle_purge_config_fields();
        if retention_fields.is_empty() {
            line(output, "  return {");
        } else {
            line(output, "  const loaded: ApplicationConfiguration = {");
        }
        for field in &configuration.fields {
            let binding = field
                .binding
                .as_ref()
                .map(|binding| unquote(&binding.text))
                .unwrap_or("");
            let fallback = field.default.as_ref().map_or_else(
                || "undefined".to_owned(),
                |default| match default.kind {
                    ConfigDefaultKind::String => ts_string(unquote(&default.text)),
                    ConfigDefaultKind::Integer
                    | ConfigDefaultKind::Decimal
                    | ConfigDefaultKind::Boolean
                    | ConfigDefaultKind::Duration => ts_string(&default.text),
                },
            );
            let raw = format!(
                "requireConfigurationValue(environment, {}, {}, {fallback})",
                ts_string(binding),
                ts_string(&field.name.text),
            );
            let parsed = match self.representation_root_for(&field.field_type).as_str() {
                "Bool" => format!(
                    "parseConfigurationBool({raw}, {})",
                    ts_string(&field.name.text)
                ),
                "Int" => format!(
                    "parseConfigurationInt({raw}, {})",
                    ts_string(&field.name.text)
                ),
                "Decimal" => format!(
                    "parseConfigurationDecimal({raw}, {})",
                    ts_string(&field.name.text)
                ),
                "Duration" => format!(
                    "normalizeConfigurationDuration({raw}, {})",
                    ts_string(&field.name.text)
                ),
                _ => raw,
            };
            let validated = self.validation_expression(
                &field.field_type,
                &parsed,
                &ts_string(&format!("config.{}", field.name.text)),
            );
            line(output, &format!("    {}: {validated},", field.name.text));
        }
        line(output, "  };");
        for field in retention_fields {
            line(
                output,
                &format!(
                    "  if (durationMilliseconds(loaded.{field}, {}) <= 0) throw new Error(\"Retention must be positive: config.{field}\");",
                    ts_string(&format!("config.{field}"))
                ),
            );
        }
        if !self.lifecycle_purge_config_fields().is_empty() {
            line(output, "  return loaded;");
        }
        line(output, "}");
        line(output, "");
    }

    fn representation_root_for(&self, reference: &TypeReference) -> String {
        let mut current = type_name(reference);
        let mut visited = BTreeSet::new();
        while visited.insert(current.clone()) {
            let Some(declaration) = self.types.get(&current) else {
                break;
            };
            current = type_name(&declaration.parent);
        }
        current
    }

    fn source_revision(&self) -> String {
        self.source_revision.clone()
    }

    fn has_entities(&self) -> bool {
        self.records
            .values()
            .any(|record| record.is_persistent_entity())
    }

    fn has_generated_change_field(&self, name: &str) -> bool {
        self.records
            .get(name)
            .and_then(|entity| generated_change_field(entity))
            .is_some()
    }

    fn has_derived_representations(&self) -> bool {
        self.project
            .entity_model
            .entities
            .iter()
            .any(|entity| !entity.representations.is_empty())
    }

    fn entity_has_derived_representations(&self, name: &str) -> bool {
        self.project
            .entity_model
            .entities
            .iter()
            .find(|entity| entity.name == name)
            .is_some_and(|entity| !entity.representations.is_empty())
    }

    fn has_persistence_operations(&self) -> bool {
        self.callables
            .values()
            .any(|callable| block_contains_persistence(&callable.body))
            || self
                .tests
                .iter()
                .any(|test| block_contains_persistence(&test.body))
    }

    fn callable_is_mutative(&self, name: &str) -> bool {
        callable_is_mutative(name, &self.callables, &mut BTreeSet::new())
    }

    fn callable_is_repeatable(&self, name: &str) -> bool {
        let type_names = self.types.keys().cloned().collect::<BTreeSet<_>>();
        callable_is_repeatable(name, &self.callables, &type_names, &mut BTreeSet::new())
    }

    fn callable_may_suspend(&self, name: &str) -> bool {
        callable_may_suspend(
            name,
            &self.callables,
            &self.service_operation_names(),
            &mut BTreeSet::new(),
        )
    }

    fn block_may_suspend(&self, block: &Block) -> bool {
        block_may_suspend(
            block,
            &self.callables,
            &self.service_operation_names(),
            &mut BTreeSet::new(),
        )
    }

    fn expression_may_suspend(&self, expression: &Expression) -> bool {
        expression_may_suspend(
            expression,
            &self.callables,
            &self.service_operation_names(),
            &mut BTreeSet::new(),
        )
    }

    fn entities(&self) -> impl Iterator<Item = (&String, &&RecordDeclaration)> {
        self.records
            .iter()
            .filter(|(_, declaration)| declaration.is_persistent_entity())
    }

    fn schema_entities(&self) -> Vec<(&String, &&RecordDeclaration)> {
        let entities = self.entities().collect::<Vec<_>>();
        let mut emitted = BTreeSet::new();
        let mut ordered = Vec::new();

        while ordered.len() < entities.len() {
            let next = entities.iter().copied().find(|(name, entity)| {
                !emitted.contains(name.as_str())
                    && entity.fields.iter().all(|field| {
                        field.reference.as_ref().map_or(true, |reference| {
                            let target = &reference.target.path[0].text;
                            target == name.as_str()
                                || !self.records.contains_key(target)
                                || emitted.contains(target.as_str())
                        })
                    })
            });
            let Some((name, entity)) = next else {
                ordered.extend(
                    entities
                        .iter()
                        .copied()
                        .filter(|(name, _)| !emitted.contains(name.as_str())),
                );
                break;
            };
            emitted.insert(name.as_str());
            ordered.push((name, entity));
        }
        ordered
    }

    fn schema_sql(&self, dialect: SqlDialect) -> String {
        let mut output = String::from("-- Generated by Jadpo 0.0.1. Do not edit.\n\n");
        for (name, entity) in self.schema_entities() {
            let table = sql_identifier(&snake_case(name));
            let body = self.table_body(name, entity, dialect, "  ", ",\n");
            line(
                &mut output,
                &format!("CREATE TABLE IF NOT EXISTS {table} (\n{body}\n);"),
            );
            for statement in self.index_statements(name, entity) {
                line(&mut output, &statement);
            }
            line(&mut output, "");
        }
        if self.first_party_supported() {
            line(&mut output, "CREATE TABLE IF NOT EXISTS \"__jadpo_auth_sessions\" (\"id\" TEXT PRIMARY KEY, \"data\" TEXT NOT NULL, \"revoked\" INTEGER NOT NULL DEFAULT 0);");
            line(&mut output, "CREATE TABLE IF NOT EXISTS \"__jadpo_auth_service_credentials\" (\"id\" TEXT PRIMARY KEY, \"data\" TEXT NOT NULL, \"revoked\" INTEGER NOT NULL DEFAULT 0);");
        }
        output
    }

    fn persistence_manifest(&self) -> String {
        let entities = self
            .entities()
            .map(|(name, entity)| {
                let fields = entity
                    .fields
                    .iter()
                    .map(|field| {
                        let reference = field.reference.as_ref().map_or_else(
                            || "null".to_owned(),
                            |reference| {
                                format!(
                                    "{{\"entity\":{},\"field\":{},\"relationship\":{},\"required\":{},\"on_delete\":{}}}",
                                    ts_string(&reference.target.path[0].text),
                                    ts_string(&reference.target.path[1].text),
                                    ts_string(owning_relationship_name(field)),
                                    !self.reference_is_nullable(&field.field_type),
                                    ts_string(reference_delete_name(reference.on_delete))
                                )
                            },
                        );
                        let generated = field.generated.map_or_else(
                            || "null".to_owned(),
                            |role| ts_string(match role {
                                jadpo_syntax::GeneratedFieldRole::Identity => "identity",
                                jadpo_syntax::GeneratedFieldRole::Create => "create",
                                jadpo_syntax::GeneratedFieldRole::CreateOrChange => "create_or_change",
                            }),
                        );
                        format!(
                            "{{\"name\":{},\"type\":{},\"nullable\":{},\"identity\":{},\"unique\":{},\"indexed\":{},\"generated\":{generated},\"reference\":{reference}}}",
                            ts_string(&field.name.text),
                            ts_string(&type_name(&field.field_type)),
                            self.reference_is_nullable(&field.field_type),
                            has_modifier(field, PersistenceModifier::Identity),
                            has_modifier(field, PersistenceModifier::Unique),
                            has_modifier(field, PersistenceModifier::Index)
                                || field.reference.is_some()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let identity = entity
                    .fields
                    .iter()
                    .find(|field| has_modifier(field, PersistenceModifier::Identity))
                    .map(|field| ts_string(&field.name.text))
                    .unwrap_or_else(|| "null".to_owned());
                let unique_constraints = entity
                    .fields
                    .iter()
                    .filter(|field| has_modifier(field, PersistenceModifier::Unique))
                    .map(|field| ts_string(&field.name.text));
                let compound_unique_constraints = entity
                    .persistence_constraints
                    .iter()
                    .map(|constraint| {
                        format!(
                            "{{\"name\":{},\"fields\":[{}]}}",
                            ts_string(&constraint.name.text),
                            constraint
                                .fields
                                .iter()
                                .map(|field| ts_string(&field.text))
                                .collect::<Vec<_>>()
                                .join(",")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let indexes = entity
                    .fields
                    .iter()
                    .filter(|field| {
                        has_modifier(field, PersistenceModifier::Index)
                            || field.reference.is_some()
                    })
                    .map(|field| ts_string(&field.name.text));
                let inverses = entity
                    .inverses
                    .iter()
                    .map(|inverse| {
                        format!(
                            "{{\"name\":{},\"cardinality\":{},\"entity\":{},\"via\":{}}}",
                            ts_string(&inverse.name.text),
                            ts_string(match inverse.cardinality {
                                jadpo_syntax::InverseCardinality::Many => "many",
                                jadpo_syntax::InverseCardinality::Optional => "optional",
                            }),
                            ts_string(&inverse.target.text),
                            ts_string(&type_name(&inverse.via))
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let postgres = self.insert_sql(name, entity, SqlDialect::Postgres);
                let sqlite = self.insert_sql(name, entity, SqlDialect::Sqlite);
                let queries = entity
                    .fields
                    .iter()
                    .map(|field| {
                        let postgres = self.select_optional_sql(
                            name,
                            entity,
                            &field.name.text,
                            SqlDialect::Postgres,
                        );
                        let sqlite = self.select_optional_sql(
                            name,
                            entity,
                            &field.name.text,
                            SqlDialect::Sqlite,
                        );
                        let mut entries = vec![format!(
                            "{}:{{\"postgres\":{},\"sqlite\":{}}},{}:{{\"postgres\":{},\"sqlite\":{}}}",
                            ts_string(&format!("optional_by_{}", field.name.text)),
                            ts_string(&postgres),
                            ts_string(&sqlite),
                            ts_string(&format!("required_by_{}", field.name.text)),
                            ts_string(&postgres),
                            ts_string(&sqlite)
                        )];
                        for order in &entity.fields {
                            for (direction, sql_direction) in
                                [("asc", "ASC"), ("desc", "DESC")]
                            {
                                let postgres = self.select_many_sql(
                                    name,
                                    entity,
                                    &field.name.text,
                                    &order.name.text,
                                    sql_direction,
                                    SqlDialect::Postgres,
                                );
                                let sqlite = self.select_many_sql(
                                    name,
                                    entity,
                                    &field.name.text,
                                    &order.name.text,
                                    sql_direction,
                                    SqlDialect::Sqlite,
                                );
                                entries.push(format!(
                                    "{}:{{\"postgres\":{},\"sqlite\":{}}}",
                                    ts_string(&format!(
                                        "many_by_{}_order_by_{}_{}",
                                        field.name.text, order.name.text, direction
                                    )),
                                    ts_string(&postgres),
                                    ts_string(&sqlite)
                                ));
                                let postgres = self.select_many_paginated_sql(
                                    name,
                                    entity,
                                    &field.name.text,
                                    &order.name.text,
                                    sql_direction,
                                    SqlDialect::Postgres,
                                );
                                let sqlite = self.select_many_paginated_sql(
                                    name,
                                    entity,
                                    &field.name.text,
                                    &order.name.text,
                                    sql_direction,
                                    SqlDialect::Sqlite,
                                );
                                entries.push(format!(
                                    "{}:{{\"postgres\":{},\"sqlite\":{}}}",
                                    ts_string(&format!(
                                        "many_by_{}_order_by_{}_{}_paginated",
                                        field.name.text, order.name.text, direction
                                    )),
                                    ts_string(&postgres),
                                    ts_string(&sqlite)
                                ));
                            }
                        }
                        entries.join(",")
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let table = sql_identifier(&snake_case(name));
                let lifecycle_write_guard = self
                    .lifecycle_visibility_sql(name)
                    .map(|predicate| format!(" AND ({predicate})"))
                    .unwrap_or_default();
                let lifecycle_owned_fields = self.lifecycle_owned_field_names(name);
                let returned_fields = entity
                    .fields
                    .iter()
                    .map(|field| sql_identifier(&field.name.text))
                    .collect::<Vec<_>>()
                    .join(", ");
                let mut mutations = Vec::new();
                for predicate in &entity.fields {
                    for change in &entity.fields {
                        if change.generated.is_some()
                            || lifecycle_owned_fields.contains(&change.name.text)
                        {
                            continue;
                        }
                        let key = format!(
                            "update_required_by_{}_set_{}",
                            predicate.name.text, change.name.text
                        );
                        let postgres = format!(
                            "UPDATE {table} SET {} = $1 WHERE {} = $2{lifecycle_write_guard} RETURNING {returned_fields}",
                            sql_identifier(&change.name.text),
                            sql_identifier(&predicate.name.text)
                        );
                        let sqlite = format!(
                            "UPDATE {table} SET {} = ?1 WHERE {} = ?2{lifecycle_write_guard} RETURNING {returned_fields}",
                            sql_identifier(&change.name.text),
                            sql_identifier(&predicate.name.text)
                        );
                        mutations.push(format!(
                            "{}:{{\"postgres\":{},\"sqlite\":{}}}",
                            ts_string(&key),
                            ts_string(&postgres),
                            ts_string(&sqlite)
                        ));
                    }
                    if self.lifecycle_visibility_sql(name).is_none() {
                        let key = format!("delete_required_by_{}", predicate.name.text);
                        let postgres = format!(
                            "DELETE FROM {table} WHERE {} = $1 RETURNING {returned_fields}",
                            sql_identifier(&predicate.name.text)
                        );
                        let sqlite = format!(
                            "DELETE FROM {table} WHERE {} = ?1 RETURNING {returned_fields}",
                            sql_identifier(&predicate.name.text)
                        );
                        mutations.push(format!(
                            "{}:{{\"postgres\":{},\"sqlite\":{}}}",
                            ts_string(&key),
                            ts_string(&postgres),
                            ts_string(&sqlite)
                        ));
                    }
                }
                let mut emitted_multi_updates = BTreeSet::new();
                for callable in self.callables.values() {
                    let mut updates = Vec::new();
                    collect_update_expressions(&callable.body, &mut updates);
                    for update in updates.iter().copied().filter(|update| {
                        update.patch.is_none()
                            && update.changes.len() > 1
                            && update
                                .target
                                .path
                                .iter()
                                .map(|part| part.text.as_str())
                                .collect::<Vec<_>>()
                                .join(".")
                                == *name
                    }) {
                        let change_suffix = update
                            .changes
                            .iter()
                            .map(|change| change.name.text.as_str())
                            .collect::<Vec<_>>()
                            .join("_and_");
                        let key = format!(
                            "update_required_by_{}_set_{change_suffix}",
                            update.field.text
                        );
                        if !emitted_multi_updates.insert(key.clone()) {
                            continue;
                        }
                        let postgres_set = update
                            .changes
                            .iter()
                            .enumerate()
                            .map(|(index, change)| {
                                format!(
                                    "{} = ${}",
                                    sql_identifier(&change.name.text),
                                    index + 1
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        let sqlite_set = update
                            .changes
                            .iter()
                            .enumerate()
                            .map(|(index, change)| {
                                format!(
                                    "{} = ?{}",
                                    sql_identifier(&change.name.text),
                                    index + 1
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        let predicate_index = update.changes.len() + 1;
                        let postgres = format!(
                            "UPDATE {table} SET {postgres_set} WHERE {} = ${predicate_index}{lifecycle_write_guard} RETURNING {returned_fields}",
                            sql_identifier(&update.field.text)
                        );
                        let sqlite = format!(
                            "UPDATE {table} SET {sqlite_set} WHERE {} = ?{predicate_index}{lifecycle_write_guard} RETURNING {returned_fields}",
                            sql_identifier(&update.field.text)
                        );
                        mutations.push(format!(
                            "{}:{{\"postgres\":{},\"sqlite\":{}}}",
                            ts_string(&key),
                            ts_string(&postgres),
                            ts_string(&sqlite)
                        ));
                    }
                    for update in updates.iter().copied().filter(|update| {
                        update.patch.is_some()
                            && update
                                .target
                                .path
                                .iter()
                                .map(|part| part.text.as_str())
                                .collect::<Vec<_>>()
                                .join(".")
                                == *name
                    }) {
                        let patch_fields = self
                            .patch_fields_for_update(update)
                            .expect("checked patch input resolves to a record");
                        let suffix = patch_method_suffix(update, &patch_fields);
                        let key = format!(
                            "update_required_by_{}_{suffix}",
                            update.field.text
                        );
                        if !emitted_multi_updates.insert(key.clone()) {
                            continue;
                        }
                        let mut postgres_set = patch_fields
                            .iter()
                            .enumerate()
                            .map(|(index, field)| {
                                let supplied = index * 2 + 1;
                                let value = supplied + 1;
                                let column = sql_identifier(&field.name.text);
                                format!(
                                    "{column} = CASE WHEN ${supplied} THEN ${value} ELSE {column} END"
                                )
                            })
                            .collect::<Vec<_>>();
                        let mut sqlite_set = patch_fields
                            .iter()
                            .enumerate()
                            .map(|(index, field)| {
                                let supplied = index * 2 + 1;
                                let value = supplied + 1;
                                let column = sql_identifier(&field.name.text);
                                format!(
                                    "{column} = CASE WHEN ?{supplied} THEN ?{value} ELSE {column} END"
                                )
                            })
                            .collect::<Vec<_>>();
                        let derived = patch_derived_changes(update);
                        for (index, (change, supplied)) in derived.iter().enumerate() {
                            let value = patch_fields.len() * 2 + index + 1;
                            let column = sql_identifier(&change.name.text);
                            if let Some(supplied) = supplied {
                                let supplied_field = &supplied.path[1].text;
                                let supplied_index = patch_fields
                                    .iter()
                                    .position(|field| field.name.text == *supplied_field)
                                    .expect("checked supplied patch field exists")
                                    * 2
                                    + 1;
                                postgres_set.push(format!(
                                    "{column} = CASE WHEN ${supplied_index} THEN ${value} ELSE {column} END"
                                ));
                                sqlite_set.push(format!(
                                    "{column} = CASE WHEN ?{supplied_index} THEN ?{value} ELSE {column} END"
                                ));
                            } else {
                                postgres_set.push(format!("{column} = ${value}"));
                                sqlite_set.push(format!("{column} = ?{value}"));
                            }
                        }
                        let postgres_set = postgres_set.join(", ");
                        let sqlite_set = sqlite_set.join(", ");
                        let predicate_index = patch_fields.len() * 2 + derived.len() + 1;
                        let postgres = format!(
                            "UPDATE {table} SET {postgres_set} WHERE {} = ${predicate_index}{lifecycle_write_guard} RETURNING {returned_fields}",
                            sql_identifier(&update.field.text)
                        );
                        let sqlite = format!(
                            "UPDATE {table} SET {sqlite_set} WHERE {} = ?{predicate_index}{lifecycle_write_guard} RETURNING {returned_fields}",
                            sql_identifier(&update.field.text)
                        );
                        mutations.push(format!(
                            "{}:{{\"postgres\":{},\"sqlite\":{},\"omission\":\"supplied_flag\"}}",
                            ts_string(&key),
                            ts_string(&postgres),
                            ts_string(&sqlite)
                        ));
                    }
                }
                let mutations = mutations.join(",");
                format!(
                    "{{\"entity\":{},\"table\":{},\"identity\":{identity},\"unique_constraints\":[{}],\"compound_unique_constraints\":[{compound_unique_constraints}],\"indexes\":[{}],\"fields\":[{fields}],\"inverses\":[{inverses}],\"create\":{{\"postgres\":{},\"sqlite\":{}}},\"queries\":{{{queries}}},\"mutations\":{{{mutations}}}}}",
                    ts_string(name),
                    ts_string(&snake_case(name)),
                    unique_constraints.collect::<Vec<_>>().join(","),
                    indexes.collect::<Vec<_>>().join(","),
                    ts_string(&postgres),
                    ts_string(&sqlite)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let mut query_plans = Vec::new();
        for (child_name, child) in self.entities() {
            let Some(child_identity) = child
                .fields
                .iter()
                .find(|field| has_modifier(field, PersistenceModifier::Identity))
            else {
                continue;
            };
            for field in &child.fields {
                let Some(reference) = &field.reference else {
                    continue;
                };
                let Some(parent_name) =
                    reference.target.path.first().map(|part| part.text.as_str())
                else {
                    continue;
                };
                let Some(target_field) =
                    reference.target.path.get(1).map(|part| part.text.as_str())
                else {
                    continue;
                };
                let Some(parent) = self.records.get(parent_name) else {
                    continue;
                };
                let child_sql = self.select_optional_sql(
                    child_name,
                    child,
                    &child_identity.name.text,
                    SqlDialect::Postgres,
                );
                let parent_sql = self.select_optional_sql(
                    parent_name,
                    parent,
                    target_field,
                    SqlDialect::Postgres,
                );
                query_plans.push(format!(
                    "{{\"child\":{},\"relationship\":{},\"parent\":{},\"strategy\":\"bounded_parent_lookup\",\"query_count\":2,\"child_cardinality\":\"required\",\"parent_cardinality\":{},\"foreign_field\":{},\"target_field\":{},\"child_sql\":{},\"parent_sql\":{}}}",
                    ts_string(child_name),
                    ts_string(owning_relationship_name(field)),
                    ts_string(parent_name),
                    ts_string(if self.reference_is_nullable(&field.field_type) { "optional" } else { "required" }),
                    ts_string(&format!("{child_name}.{}", field.name.text)),
                    ts_string(&format!("{parent_name}.{target_field}")),
                    ts_string(&child_sql),
                    ts_string(&parent_sql)
                ));
                if !self.reference_is_nullable(&field.field_type) {
                    for nested in parent.inverses.iter().filter(|inverse| {
                        inverse.cardinality == jadpo_syntax::InverseCardinality::Optional
                    }) {
                        let Some(nested_child) = self.records.get(&nested.target.text) else {
                            continue;
                        };
                        let Some(nested_via_field) =
                            nested.via.path.get(1).map(|part| part.text.as_str())
                        else {
                            continue;
                        };
                        let Some(nested_target_field) = nested_child
                            .fields
                            .iter()
                            .find(|candidate| candidate.name.text == nested_via_field)
                            .and_then(|candidate| candidate.reference.as_ref())
                            .and_then(|reference| reference.target.path.get(1))
                            .map(|part| part.text.as_str())
                        else {
                            continue;
                        };
                        let leaf_sql = self.select_optional_sql(
                            &nested.target.text,
                            nested_child,
                            nested_via_field,
                            SqlDialect::Postgres,
                        );
                        query_plans.push(format!(
                            "{{\"root\":{},\"path\":[{},{}],\"leaf\":{},\"strategy\":\"bounded_nested_lookup\",\"query_count\":3,\"maximum_depth\":2,\"root_cardinality\":\"required\",\"middle_cardinality\":\"required\",\"leaf_cardinality\":\"optional\",\"leaf_target_field\":{},\"root_sql\":{},\"middle_sql\":{},\"leaf_sql\":{}}}",
                            ts_string(child_name),
                            ts_string(owning_relationship_name(field)),
                            ts_string(&nested.name.text),
                            ts_string(&nested.target.text),
                            ts_string(&format!("{parent_name}.{nested_target_field}")),
                            ts_string(&child_sql),
                            ts_string(&parent_sql),
                            ts_string(&leaf_sql)
                        ));
                    }
                }
            }
        }
        for (parent_name, parent) in self.entities() {
            for inverse in &parent.inverses {
                let Some(child) = self.records.get(&inverse.target.text) else {
                    continue;
                };
                let Some(via_field) = inverse.via.path.get(1).map(|part| part.text.as_str()) else {
                    continue;
                };
                let Some(target_field) = child
                    .fields
                    .iter()
                    .find(|field| field.name.text == via_field)
                    .and_then(|field| field.reference.as_ref())
                    .and_then(|reference| reference.target.path.get(1))
                    .map(|name| name.text.as_str())
                else {
                    continue;
                };
                if inverse.cardinality == jadpo_syntax::InverseCardinality::Optional {
                    let Some(parent_identity) = parent
                        .fields
                        .iter()
                        .find(|field| has_modifier(field, PersistenceModifier::Identity))
                    else {
                        continue;
                    };
                    let parent_sql = self.select_optional_sql(
                        parent_name,
                        parent,
                        &parent_identity.name.text,
                        SqlDialect::Postgres,
                    );
                    let child_sql = self.select_optional_sql(
                        &inverse.target.text,
                        child,
                        via_field,
                        SqlDialect::Postgres,
                    );
                    query_plans.push(format!(
                        "{{\"parent\":{},\"relationship\":{},\"child\":{},\"strategy\":\"bounded_optional_inverse\",\"query_count\":2,\"parent_cardinality\":\"required\",\"child_cardinality\":\"optional\",\"foreign_field\":{},\"target_field\":{},\"parent_sql\":{},\"child_sql\":{}}}",
                        ts_string(parent_name),
                        ts_string(&inverse.name.text),
                        ts_string(&inverse.target.text),
                        ts_string(&format!("{}.{via_field}", inverse.target.text)),
                        ts_string(&format!("{parent_name}.{target_field}")),
                        ts_string(&parent_sql),
                        ts_string(&child_sql)
                    ));
                    continue;
                }
                let Some(parent_order) = parent
                    .fields
                    .iter()
                    .find(|field| has_modifier(field, PersistenceModifier::Identity))
                else {
                    continue;
                };
                let Some(child_order) = child
                    .fields
                    .iter()
                    .find(|field| has_modifier(field, PersistenceModifier::Identity))
                else {
                    continue;
                };
                let parent_sql = self.select_optional_sql(
                    parent_name,
                    parent,
                    target_field,
                    SqlDialect::Postgres,
                );
                let child_sql = self.select_many_paginated_sql(
                    &inverse.target.text,
                    child,
                    via_field,
                    &child_order.name.text,
                    "ASC",
                    SqlDialect::Postgres,
                );
                query_plans.push(format!(
                    "{{\"parent\":{},\"relationship\":{},\"strategy\":\"bounded_batch\",\"query_count\":2,\"parent_cardinality\":\"required\",\"child_cardinality\":\"many\",\"parent_sql\":{},\"child_sql\":{},\"order_by\":{},\"direction\":\"asc\",\"pagination\":{{\"limit_parameter\":2,\"offset_parameter\":3}}}}",
                    ts_string(parent_name),
                    ts_string(&inverse.name.text),
                    ts_string(&parent_sql),
                    ts_string(&child_sql),
                    ts_string(&format!("{}.{}", inverse.target.text, child_order.name.text))
                ));

                let parent_predicate = parent
                    .fields
                    .iter()
                    .find(|field| {
                        has_modifier(field, PersistenceModifier::Index)
                            && !has_modifier(field, PersistenceModifier::Identity)
                            && !has_modifier(field, PersistenceModifier::Unique)
                    })
                    .unwrap_or(parent_order);
                let postgres = self.select_many_with_inverse_sql(
                    parent_name,
                    parent,
                    &parent_predicate.name.text,
                    &parent_order.name.text,
                    "ASC",
                    &inverse.target.text,
                    child,
                    via_field,
                    target_field,
                    &child_order.name.text,
                    "ASC",
                    SqlDialect::Postgres,
                );
                let sqlite = self.select_many_with_inverse_sql(
                    parent_name,
                    parent,
                    &parent_predicate.name.text,
                    &parent_order.name.text,
                    "ASC",
                    &inverse.target.text,
                    child,
                    via_field,
                    target_field,
                    &child_order.name.text,
                    "ASC",
                    SqlDialect::Sqlite,
                );
                query_plans.push(format!(
                    "{{\"parent\":{},\"relationship\":{},\"strategy\":\"parent_page_join\",\"query_count\":1,\"parent_cardinality\":\"many\",\"child_cardinality\":\"many\",\"parent_pagination_before_join\":true,\"postgres\":{},\"sqlite\":{},\"parameters\":{{\"predicate\":1,\"parent_limit\":2,\"parent_offset\":3,\"child_limit\":4,\"child_offset\":5}}}}",
                    ts_string(parent_name),
                    ts_string(&inverse.name.text),
                    ts_string(&postgres),
                    ts_string(&sqlite)
                ));
            }
        }
        for callable in self.callables.values() {
            let mut queries = Vec::new();
            collect_query_expressions(&callable.body, &mut queries);
            for query in &queries {
                let Some(page) = &query.page else {
                    continue;
                };
                let entity = query
                    .target
                    .path
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                let columns = query_page_index_columns(page);
                let predicate_fields = query_page_index_predicate_fields(page);
                let order = page
                    .order
                    .iter()
                    .map(|field| ts_string(&field.field.text))
                    .collect::<Vec<_>>()
                    .join(",");
                query_plans.push(format!(
                    "{{\"entity\":{},\"strategy\":\"indexed_keyset_page\",\"query_count\":1,\"offset\":false,\"fetch_limit\":\"page_size_plus_one\",\"cursor_fields\":[{order}],\"index\":{},\"postgres_first_sql\":{},\"postgres_after_sql\":{},\"sqlite_first_sql\":{},\"sqlite_after_sql\":{}}}",
                    ts_string(&entity),
                    ts_string(&query_page_index_name(&entity, &columns, &predicate_fields)),
                    ts_string(&self.query_page_sql(&entity, page, SqlDialect::Postgres, false)),
                    ts_string(&self.query_page_sql(&entity, page, SqlDialect::Postgres, true)),
                    ts_string(&self.query_page_sql(&entity, page, SqlDialect::Sqlite, false)),
                    ts_string(&self.query_page_sql(&entity, page, SqlDialect::Sqlite, true))
                ));
            }
            for query in queries.into_iter().filter(|query| query.includes.len() > 1) {
                let parent = query
                    .target
                    .path
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                let result = type_name(&query.includes[0].result);
                let relationships = query
                    .includes
                    .iter()
                    .map(|include| ts_string(&include.relationship.text))
                    .collect::<Vec<_>>()
                    .join(",");
                let (cardinality, strategy, query_count) = match query.cardinality {
                    jadpo_syntax::QueryCardinality::Required => (
                        "required",
                        "parent_then_bounded_children",
                        query.includes.len() + 1,
                    ),
                    jadpo_syntax::QueryCardinality::Many => (
                        "many",
                        "independent_parent_page_joins",
                        query.includes.len(),
                    ),
                    jadpo_syntax::QueryCardinality::Optional => continue,
                };
                query_plans.push(format!(
                    "{{\"parent\":{},\"result\":{},\"relationships\":[{relationships}],\"strategy\":{},\"query_count\":{query_count},\"parent_cardinality\":{},\"cartesian_product_avoided\":true}}",
                    ts_string(&parent),
                    ts_string(&result),
                    ts_string(strategy),
                    ts_string(cardinality)
                ));
            }
        }
        let query_plans = query_plans.join(",");
        format!("{{\"schema_version\":1,\"phase\":\"persistence\",\"transaction_policy\":\"mutative_action\",\"nested_transaction_policy\":\"reuse\",\"entities\":[{entities}],\"query_plans\":[{query_plans}]}}\n")
    }

    fn persistence_policy_runtime(&self, output: &mut String) {
        self.persistence_policy_function(output, true);
        self.persistence_policy_function(output, false);
        self.persistence_policy_sql_runtime(output);
        let field_effects = self
            .project
            .policy
            .entities
            .iter()
            .flat_map(|entity| {
                entity.fields.iter().flat_map(move |field| {
                    // A declared policy narrows every field effect, not just
                    // the explicitly granted ones. Empty means deny, not inherit.
                    ["create", "read", "update", "delete"]
                        .into_iter()
                        .map(move |effect| {
                            ts_string(&format!("{}:{effect}.{}", entity.entity, field.field))
                        })
                })
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
            .join(",");
        line(
            output,
            &format!("const policyFieldEffects = new Set<string>([{field_effects}]);"),
        );
        line(output, "");
        line(output, "async function policyAllows(entity: string, effect: string, row: Record<string, unknown>, policy: PolicyRequest, postgres: SQL | null, sqlite: Database | null): Promise<boolean> {");
        line(output, "  return postgres !== null");
        line(
            output,
            "    ? policyAllowsPostgres(entity, effect, row, policy, postgres)",
        );
        line(
            output,
            "    : policyAllowsSqlite(entity, effect, row, policy, sqlite!);",
        );
        line(output, "}");
        line(output, "");
        line(output, "async function policyFilterRows(entity: string, rows: readonly unknown[], policy: PolicyRequest, postgres: SQL | null, sqlite: Database | null): Promise<unknown[]> {");
        line(output, "  const visible: unknown[] = [];");
        line(output, "  for (const row of rows) if (await policyAllows(entity, \"read\", row as Record<string, unknown>, policy, postgres, sqlite)) visible.push(row);");
        line(output, "  return visible;");
        line(output, "}");
        line(output, "");
        line(output, "async function policyAllowsWrite(entity: string, effect: string, row: Record<string, unknown>, fields: readonly string[], policy: PolicyRequest, postgres: SQL | null, sqlite: Database | null): Promise<boolean> {");
        line(output, "  if (!(await policyAllows(entity, effect, row, policy, postgres, sqlite))) return false;");
        line(output, "  for (const field of fields) {");
        line(
            output,
            "    const fieldEffect = `${entity}:${effect}.${field}`;",
        );
        line(output, "    if (policyFieldEffects.has(fieldEffect) && !(await policyAllows(entity, `${effect}.${field}`, row, policy, postgres, sqlite))) return false;");
        line(output, "  }");
        line(output, "  return true;");
        line(output, "}");
        line(output, "");
        line(output, "function policyAllowsWriteSqlite(entity: string, effect: string, row: Record<string, unknown>, fields: readonly string[], policy: PolicyRequest, sqlite: Database): boolean {");
        line(
            output,
            "  if (!policyAllowsSqlite(entity, effect, row, policy, sqlite)) return false;",
        );
        line(output, "  for (const field of fields) {");
        line(
            output,
            "    const fieldEffect = `${entity}:${effect}.${field}`;",
        );
        line(output, "    if (policyFieldEffects.has(fieldEffect) && !policyAllowsSqlite(entity, `${effect}.${field}`, row, policy, sqlite)) return false;");
        line(output, "  }");
        line(output, "  return true;");
        line(output, "}");
        line(output, "");
    }

    fn persistence_policy_sql_runtime(&self, output: &mut String) {
        line(output, "type PolicySqlScope = Readonly<{ permitted: boolean; clause: string; values: readonly unknown[] }>; ");
        line(
            output,
            "function policyScopedSql(sql: string, clause: string): string {",
        );
        line(
            output,
            "  const markers = [\" ORDER BY \", \" LIMIT \", \" FOR UPDATE\", \" RETURNING \"]; ",
        );
        line(output, "  const positions = markers.map(marker => sql.indexOf(marker)).filter(position => position >= 0);");
        line(
            output,
            "  const insertion = positions.length === 0 ? sql.length : Math.min(...positions);",
        );
        line(
            output,
            "  return `${sql.slice(0, insertion)} AND (${clause})${sql.slice(insertion)}`;",
        );
        line(output, "}");
        line(output, "");
        line(output, "function policySqlScope(entity: string, effect: string, policy: PolicyRequest, dialect: \"postgres\" | \"sqlite\", parameterOffset: number): PolicySqlScope {");
        if !self.project.policy.active {
            line(
                output,
                "  return { permitted: true, clause: \"1 = 1\", values: [] };",
            );
            line(output, "}");
            line(output, "");
            return;
        }
        line(
            output,
            "  const operationKey = `${policy.operation}:${entity}:${effect}`;",
        );
        line(output, "  switch (operationKey) {");
        for operation in &self.project.policy.operations {
            for obligation in &operation.obligations {
                if obligation.source != "operation_exception" && obligation.source != "operation" {
                    continue;
                }
                let key = format!(
                    "{}:{}:{}",
                    operation.operation, obligation.entity, obligation.effect
                );
                line(output, &format!("    case {}: {{", ts_string(&key)));
                self.policy_sql_subjects(
                    output,
                    "      ",
                    &obligation.entity,
                    &obligation.subjects,
                );
                line(output, "    }");
            }
        }
        line(output, "  }");
        line(output, "  switch (`${entity}:${effect}`) {");
        for entity in &self.project.policy.entities {
            let effects = entity
                .rules
                .iter()
                .flat_map(|rule| rule.effects.iter().cloned())
                .collect::<BTreeSet<_>>();
            for effect in effects {
                let subjects = entity
                    .rules
                    .iter()
                    .filter(|rule| rule.effects.contains(&effect))
                    .map(|rule| rule.subject.clone())
                    .collect::<Vec<_>>();
                let key = format!("{}:{effect}", entity.entity);
                line(output, &format!("    case {}: {{", ts_string(&key)));
                self.policy_sql_subjects(output, "      ", &entity.entity, &subjects);
                line(output, "    }");
            }
            for field in &entity.fields {
                let effects = field
                    .rules
                    .iter()
                    .flat_map(|rule| rule.effects.iter().cloned())
                    .collect::<BTreeSet<_>>();
                for effect in effects {
                    let subjects = field
                        .rules
                        .iter()
                        .filter(|rule| rule.effects.contains(&effect))
                        .map(|rule| rule.subject.clone())
                        .collect::<Vec<_>>();
                    let key = format!("{}:{effect}.{}", entity.entity, field.field);
                    line(output, &format!("    case {}: {{", ts_string(&key)));
                    self.policy_sql_subjects(output, "      ", &entity.entity, &subjects);
                    line(output, "    }");
                }
            }
        }
        line(output, "  }");
        line(
            output,
            "  return { permitted: false, clause: \"0 = 1\", values: [] };",
        );
        line(output, "}");
        line(output, "");
    }

    fn policy_sql_subjects(
        &self,
        output: &mut String,
        indent: &str,
        entity_name: &str,
        subjects: &[String],
    ) {
        if subjects.iter().any(|subject| subject == "Access.public") {
            line(
                output,
                &format!("{indent}return {{ permitted: true, clause: \"1 = 1\", values: [] }};"),
            );
            return;
        }
        line(output, &format!("{indent}const predicates: string[] = [];"));
        line(output, &format!("{indent}const values: unknown[] = [];"));
        if subjects
            .iter()
            .any(|subject| subject == "Access.authenticated")
        {
            line(
                output,
                &format!("{indent}if (policy.principal !== null) predicates.push(\"1 = 1\");"),
            );
        }
        let entity_policy = self
            .project
            .policy
            .entities
            .iter()
            .find(|policy| policy.entity == entity_name);
        let scope_name = entity_policy
            .map(|policy| policy.scope.as_deref().unwrap_or(entity_name))
            .unwrap_or("application");
        let scope_field = entity_policy
            .and_then(|policy| policy.scope_field.as_deref())
            .or_else(|| {
                self.records
                    .get(scope_name)
                    .and_then(|record| identity_field_name(record))
            });
        let outer_table = sql_identifier(&snake_case(entity_name));
        let outer_scope = scope_field
            .map(|field| format!("{outer_table}.{}", sql_identifier(field)))
            .unwrap_or_else(|| "NULL".to_owned());
        for subject in subjects {
            if subject.starts_with("Access.") {
                continue;
            }
            for binding in self
                .project
                .policy
                .bindings
                .iter()
                .filter(|binding| binding.role == *subject && binding.scope == scope_name)
            {
                let principal_field = self.principal_identity_expression(&binding.principal);
                line(
                    output,
                    &format!("{indent}if ({principal_field} !== undefined) {{"),
                );
                line(
                    output,
                    &format!("{indent}  const principalValue = {principal_field};"),
                );
                line(output, &format!("{indent}  values.push(principalValue);"));
                line(output, &format!("{indent}  const parameter = dialect === \"postgres\" ? `$${{parameterOffset + values.length}}` : `?${{parameterOffset + values.length}}`;"));
                let predicate = if binding.entity == entity_name {
                    format!(
                        "{outer_table}.{} = ${{parameter}}",
                        sql_identifier(&binding.field)
                    )
                } else if let Some(scope_record) = self.records.get(scope_name) {
                    if let Some(identity) = identity_field_name(scope_record) {
                        let scope_table = sql_identifier(&snake_case(scope_name));
                        format!(
                            "EXISTS (SELECT 1 FROM {scope_table} AS \"__jadpo_scope\" WHERE \"__jadpo_scope\".{} = {outer_scope} AND \"__jadpo_scope\".{} = ${{parameter}})",
                            sql_identifier(identity),
                            sql_identifier(&binding.field)
                        )
                    } else {
                        "0 = 1".to_owned()
                    }
                } else {
                    "0 = 1".to_owned()
                };
                line(
                    output,
                    &format!("{indent}  predicates.push(`{predicate}`);"),
                );
                line(output, &format!("{indent}}}"));
            }
            let Some((role_type, role_variant)) = subject.split_once('.') else {
                continue;
            };
            for membership in self.project.policy.memberships.iter().filter(|membership| {
                membership.role_type == role_type
                    && (membership.scope == "application" || membership.scope == scope_name)
            }) {
                let principal_field = self.principal_identity_expression(&membership.principal);
                let table = sql_identifier(&snake_case(&membership.entity));
                let member = sql_identifier(&membership.member_field);
                let role = sql_identifier(&membership.role_field);
                let role_literal = sql_text_literal(role_variant);
                let predicate = membership.scope_field.as_ref().map_or_else(
                    || format!(
                        "EXISTS (SELECT 1 FROM {table} AS \"__jadpo_membership\" WHERE \"__jadpo_membership\".{member} = ${{parameter}} AND \"__jadpo_membership\".{role} = {role_literal})"
                    ),
                    |scope| {
                        let scope = sql_identifier(scope);
                        format!(
                            "EXISTS (SELECT 1 FROM {table} AS \"__jadpo_membership\" WHERE \"__jadpo_membership\".{scope} = {outer_scope} AND \"__jadpo_membership\".{member} = ${{parameter}} AND \"__jadpo_membership\".{role} = {role_literal})"
                        )
                    },
                );
                line(
                    output,
                    &format!("{indent}if ({principal_field} !== undefined) {{"),
                );
                line(
                    output,
                    &format!("{indent}  const principalValue = {principal_field};"),
                );
                line(output, &format!("{indent}  values.push(principalValue);"));
                line(output, &format!("{indent}  const parameter = dialect === \"postgres\" ? `$${{parameterOffset + values.length}}` : `?${{parameterOffset + values.length}}`;"));
                line(
                    output,
                    &format!("{indent}  predicates.push(`{predicate}`);"),
                );
                line(output, &format!("{indent}}}"));
            }
        }
        line(output, &format!("{indent}return {{ permitted: predicates.length > 0, clause: predicates.length > 0 ? predicates.map(predicate => `(${{predicate}})`).join(\" OR \") : \"0 = 1\", values }};"));
    }

    fn persistence_policy_function(&self, output: &mut String, postgres: bool) {
        if postgres {
            line(output, "async function policyAllowsPostgres(entity: string, effect: string, row: Record<string, unknown>, policy: PolicyRequest, postgres: SQL): Promise<boolean> {");
        } else {
            line(output, "function policyAllowsSqlite(entity: string, effect: string, row: Record<string, unknown>, policy: PolicyRequest, sqlite: Database): boolean {");
        }
        if !self.project.policy.active {
            line(output, "  return true;");
            line(output, "}");
            line(output, "");
            return;
        }
        line(
            output,
            "  const operationKey = `${policy.operation}:${entity}:${effect}`;",
        );
        line(output, "  switch (operationKey) {");
        for operation in &self.project.policy.operations {
            for obligation in &operation.obligations {
                if obligation.source != "operation_exception" && obligation.source != "operation" {
                    continue;
                }
                let key = format!(
                    "{}:{}:{}",
                    operation.operation, obligation.entity, obligation.effect
                );
                line(output, &format!("    case {}: {{", ts_string(&key)));
                self.policy_subject_checks(
                    output,
                    "      ",
                    &obligation.entity,
                    &obligation.subjects,
                    postgres,
                );
                line(output, "      return false;");
                line(output, "    }");
            }
        }
        line(output, "  }");
        line(output, "  switch (`${entity}:${effect}`) {");
        for entity in &self.project.policy.entities {
            let effects = entity
                .rules
                .iter()
                .flat_map(|rule| rule.effects.iter().cloned())
                .collect::<BTreeSet<_>>();
            for effect in effects {
                let subjects = entity
                    .rules
                    .iter()
                    .filter(|rule| rule.effects.contains(&effect))
                    .map(|rule| rule.subject.clone())
                    .collect::<Vec<_>>();
                let key = format!("{}:{effect}", entity.entity);
                line(output, &format!("    case {}: {{", ts_string(&key)));
                self.policy_subject_checks(output, "      ", &entity.entity, &subjects, postgres);
                line(output, "      return false;");
                line(output, "    }");
            }
            for field in &entity.fields {
                let effects = field
                    .rules
                    .iter()
                    .flat_map(|rule| rule.effects.iter().cloned())
                    .collect::<BTreeSet<_>>();
                for effect in effects {
                    let subjects = field
                        .rules
                        .iter()
                        .filter(|rule| rule.effects.contains(&effect))
                        .map(|rule| rule.subject.clone())
                        .collect::<Vec<_>>();
                    let key = format!("{}:{effect}.{}", entity.entity, field.field);
                    line(output, &format!("    case {}: {{", ts_string(&key)));
                    self.policy_subject_checks(
                        output,
                        "      ",
                        &entity.entity,
                        &subjects,
                        postgres,
                    );
                    line(output, "      return false;");
                    line(output, "    }");
                }
            }
        }
        line(output, "  }");
        line(output, "  return false;");
        line(output, "}");
        line(output, "");
    }

    fn policy_subject_checks(
        &self,
        output: &mut String,
        indent: &str,
        entity_name: &str,
        subjects: &[String],
        postgres: bool,
    ) {
        if subjects.iter().any(|subject| subject == "Access.public") {
            line(output, &format!("{indent}return true;"));
            return;
        }
        if subjects
            .iter()
            .any(|subject| subject == "Access.authenticated")
        {
            line(
                output,
                &format!("{indent}if (policy.principal !== null) return true;"),
            );
        }
        let entity_policy = self
            .project
            .policy
            .entities
            .iter()
            .find(|policy| policy.entity == entity_name);
        let scope_name = entity_policy
            .map(|policy| policy.scope.as_deref().unwrap_or(entity_name))
            .unwrap_or("application");
        let scope_field = entity_policy
            .and_then(|policy| policy.scope_field.as_deref())
            .or_else(|| {
                self.records
                    .get(scope_name)
                    .and_then(|record| identity_field_name(record))
            });
        let scope_value = scope_field
            .map(|field| format!("row[{}]", ts_string(field)))
            .unwrap_or_else(|| "undefined".to_owned());

        for subject in subjects {
            if subject.starts_with("Access.") {
                continue;
            }
            for binding in self
                .project
                .policy
                .bindings
                .iter()
                .filter(|binding| binding.role == *subject && binding.scope == scope_name)
            {
                let principal_field = self.principal_identity_expression(&binding.principal);
                line(
                    output,
                    &format!("{indent}if (policy.principal !== null) {{"),
                );
                line(
                    output,
                    &format!("{indent}  const principalValue = {principal_field};"),
                );
                if binding.entity == entity_name {
                    line(
                        output,
                        &format!(
                            "{indent}  if (principalValue !== undefined && row[{}] === principalValue) return true;",
                            ts_string(&binding.field)
                        ),
                    );
                } else if let Some(scope_record) = self.records.get(scope_name) {
                    if let Some(identity) = identity_field_name(scope_record) {
                        let table = sql_identifier(&snake_case(scope_name));
                        let identity = sql_identifier(identity);
                        let binding_field = sql_identifier(&binding.field);
                        let postgres_sql = format!(
                            "SELECT 1 FROM {table} WHERE {identity} = ${{scopeValue}} AND {binding_field} = ${{principalValue}} LIMIT 1"
                        );
                        let sqlite_sql = format!(
                            "SELECT 1 FROM {table} WHERE {identity} = ?1 AND {binding_field} = ?2 LIMIT 1"
                        );
                        line(
                            output,
                            &format!("{indent}  const scopeValue = {scope_value};"),
                        );
                        line(output, &format!("{indent}  if (principalValue !== undefined && scopeValue !== undefined) {{"));
                        if postgres {
                            line(output, &format!("{indent}    const matches = await persistenceAsync(\"policy.direct\", () => postgres`{postgres_sql}`);"));
                        } else {
                            line(output, &format!("{indent}    const matches = persistenceSync(\"policy.direct\", () => sqlite.prepare({}).all(scopeValue, principalValue));", ts_string(&sqlite_sql)));
                        }
                        line(
                            output,
                            &format!("{indent}    if (matches.length > 0) return true;"),
                        );
                        line(output, &format!("{indent}  }}"));
                    }
                }
                line(output, &format!("{indent}}}"));
            }

            let Some((role_type, role_variant)) = subject.split_once('.') else {
                continue;
            };
            for membership in self.project.policy.memberships.iter().filter(|membership| {
                membership.role_type == role_type
                    && (membership.scope == "application" || membership.scope == scope_name)
            }) {
                let principal_field = self.principal_identity_expression(&membership.principal);
                let table = sql_identifier(&snake_case(&membership.entity));
                let membership_member = sql_identifier(&membership.member_field);
                let membership_role = sql_identifier(&membership.role_field);
                let role_literal = sql_text_literal(role_variant);
                let (postgres_sql, sqlite_sql, arguments, condition) = membership
                    .scope_field
                    .as_ref()
                    .map_or_else(
                        || {
                            (
                                format!("SELECT 1 FROM {table} WHERE {membership_member} = ${{principalValue}} AND {membership_role} = {role_literal} LIMIT 1"),
                                format!("SELECT 1 FROM {table} WHERE {membership_member} = ?1 AND {membership_role} = {role_literal} LIMIT 1"),
                                "principalValue".to_owned(),
                                "principalValue !== undefined".to_owned(),
                            )
                        },
                        |scope| {
                            let membership_scope = sql_identifier(scope);
                            (
                                format!("SELECT 1 FROM {table} WHERE {membership_scope} = ${{scopeValue}} AND {membership_member} = ${{principalValue}} AND {membership_role} = {role_literal} LIMIT 1"),
                                format!("SELECT 1 FROM {table} WHERE {membership_scope} = ?1 AND {membership_member} = ?2 AND {membership_role} = {role_literal} LIMIT 1"),
                                "scopeValue, principalValue".to_owned(),
                                "principalValue !== undefined && scopeValue !== undefined".to_owned(),
                            )
                        },
                    );
                line(
                    output,
                    &format!("{indent}if (policy.principal !== null) {{"),
                );
                line(
                    output,
                    &format!("{indent}  const principalValue = {principal_field};"),
                );
                line(
                    output,
                    &format!("{indent}  const scopeValue = {scope_value};"),
                );
                line(output, &format!("{indent}  if ({condition}) {{"));
                if postgres {
                    line(output, &format!("{indent}    const matches = await persistenceAsync(\"policy.membership\", () => postgres`{postgres_sql}`);"));
                } else {
                    line(output, &format!("{indent}    const matches = persistenceSync(\"policy.membership\", () => sqlite.prepare({}).all({arguments}));", ts_string(&sqlite_sql)));
                }
                line(
                    output,
                    &format!("{indent}    if (matches.length > 0) return true;"),
                );
                line(output, &format!("{indent}  }}"));
                line(output, &format!("{indent}}}"));
            }
        }
    }

    fn principal_identity_expression(&self, entity: &str) -> String {
        let identity = self
            .records
            .get(entity)
            .and_then(|record| identity_field_name(record));
        let mut bindings = BTreeSet::new();
        for strategy in &self.authentication_strategies {
            for resolution in &strategy.resolutions {
                if resolution
                    .authority
                    .path
                    .first()
                    .is_some_and(|n| n.text == entity)
                {
                    for mapping in &resolution.mappings {
                        if identity == Some(mapping.source.text.as_str())
                            && mapping.target.path.len() == 3
                        {
                            bindings.insert((
                                resolution.principal.text.clone(),
                                mapping.target.path[2].text.clone(),
                            ));
                        }
                    }
                }
            }
        }
        if let Some(principal) = self.principal {
            for variant in &principal.variants {
                for field in &variant.fields {
                    if field.field_type.path.len() == 2
                        && field.field_type.path[0].text == entity
                        && identity == Some(field.field_type.path[1].text.as_str())
                    {
                        bindings
                            .insert((variant.kind.as_str().to_owned(), field.name.text.clone()));
                    }
                }
            }
        }
        if !bindings.is_empty() {
            // Entity identity comes from explicit authority mappings. Matching
            // UUID bytes in another principal kind cannot confer this role.
            return format!(
                "({}undefined)",
                bindings
                    .iter()
                    .map(|(kind, field)| {
                        format!(
                            "policy.principal?.kind === {} ? policy.principal.values[{}] : ",
                            ts_string(kind),
                            ts_string(field)
                        )
                    })
                    .collect::<String>()
            );
        }
        if self.has_authentication() {
            return "undefined".to_owned();
        }
        // Existing trusted fixture boundary for unauthenticated policy probes.
        let field = ts_string(&format!("{}_id", snake_case(entity)));
        format!("policy.principal?.values[{field}]")
    }

    fn persistence_target(&self) -> String {
        let mut output = String::new();
        line(&mut output, "// Generated by Jadpo 0.0.1. Do not edit.");
        line(&mut output, "import { SQL } from \"bun\";");
        line(&mut output, "import { Database } from \"bun:sqlite\";");
        if !self.project.delivery_model().bindings().is_empty() {
            line(
                &mut output,
                "import { readCurrentDeliveryCredentialProof, readDeliverySelectionSender, validateDeliveryPayload, validateDeliverySelectionCursor } from \"./app.ts\";",
            );
        }
        line(&mut output, "let persistenceReady = false;");
        line(&mut output, "let persistenceProbeNeeded = false;");
        line(&mut output, "let persistenceInvalidationGeneration = 0;");
        line(&mut output, "");
        line(
            &mut output,
            "function persistenceInstant(value: unknown, postgres: boolean): unknown {",
        );
        line(&mut output, "  if (typeof value !== \"string\" || !/(?:Z|[+-][0-9]{2}:[0-9]{2})$/u.test(value)) throw new PersistenceFault(\"encode.instant\", \"data\", new Error(\"invalid Instant\"));");
        line(&mut output, "  const milliseconds = Date.parse(value); if (!Number.isSafeInteger(milliseconds)) throw new PersistenceFault(\"encode.instant\", \"data\", new Error(\"Instant outside portable range\"));");
        line(
            &mut output,
            "  return postgres ? new Date(milliseconds).toISOString() : milliseconds;",
        );
        line(&mut output, "}");
        line(
            &mut output,
            "function persistenceDuration(value: unknown): number {",
        );
        line(&mut output, "  if (typeof value !== \"string\") throw new PersistenceFault(\"encode.duration\", \"data\", new Error(\"invalid Duration\"));");
        line(&mut output, "  const match = /^(-)?PT(?:(\\d+)H)?(?:(\\d+)M)?(?:(\\d+)(?:\\.(\\d{1,3}))?S)?$/u.exec(value); if (match === null) throw new PersistenceFault(\"encode.duration\", \"data\", new Error(\"invalid Duration\"));");
        line(&mut output, "  const result = ((Number(match[2] ?? 0) * 60 + Number(match[3] ?? 0)) * 60 + Number(match[4] ?? 0)) * 1000 + Number((match[5] ?? \"\").padEnd(3, \"0\")); return match[1] === undefined ? result : -result;");
        line(&mut output, "}");
        if self.has_lifecycle_purges() {
            line(
                &mut output,
                "function configuredPersistenceDuration(value: unknown, path: string): number {",
            );
            line(&mut output, "  if (typeof value !== \"string\") throw new PersistenceFault(\"retention.configuration\", \"data\", new Error(`invalid configured duration ${path}`));");
            line(&mut output, "  if (value.startsWith(\"P\") || value.startsWith(\"-P\")) return persistenceDuration(value);");
            line(
                &mut output,
                "  const match = /^(-?\\d+(?:\\.\\d+)?)(ms|s|m|h|d)$/u.exec(value);",
            );
            line(&mut output, "  if (match === null) throw new PersistenceFault(\"retention.configuration\", \"data\", new Error(`invalid configured duration ${path}`));");
            line(&mut output, "  const multiplier = match[2] === \"d\" ? 86_400_000 : match[2] === \"h\" ? 3_600_000 : match[2] === \"m\" ? 60_000 : match[2] === \"s\" ? 1_000 : 1;");
            line(
                &mut output,
                "  const milliseconds = Number(match[1]) * multiplier;",
            );
            line(&mut output, "  if (!Number.isSafeInteger(milliseconds)) throw new PersistenceFault(\"retention.configuration\", \"data\", new Error(`invalid configured duration ${path}`));");
            line(&mut output, "  return milliseconds;");
            line(&mut output, "}");
        }
        line(
            &mut output,
            "function persistenceValue(value: unknown, kind: string, postgres: boolean): unknown {",
        );
        line(&mut output, "  if (value === null) return null;");
        line(
            &mut output,
            "  if (kind === \"Instant\") return persistenceInstant(value, postgres);",
        );
        line(
            &mut output,
            "  if (kind === \"Duration\") return persistenceDuration(value);",
        );
        line(
            &mut output,
            "  if (kind === \"Time\") return postgres ? value : JSON.stringify(value);",
        );
        line(&mut output, "  return value;");
        line(&mut output, "}");
        line(
            &mut output,
            "const semanticChanges = new WeakMap<object, boolean>();",
        );
        line(
            &mut output,
            "function comparablePersistenceValue(value: unknown): unknown {",
        );
        line(
            &mut output,
            "  if (value instanceof Date) return value.toISOString();",
        );
        line(
            &mut output,
            "  if (typeof value === \"bigint\") return value.toString();",
        );
        line(
            &mut output,
            "  if (Array.isArray(value)) return value.map(comparablePersistenceValue);",
        );
        line(&mut output, "  if (typeof value === \"object\" && value !== null) return Object.fromEntries(Object.entries(value as Record<string, unknown>).sort(([left], [right]) => left.localeCompare(right)).map(([key, item]) => [key, comparablePersistenceValue(item)]));");
        line(&mut output, "  return value;");
        line(&mut output, "}");
        line(&mut output, "function recordSemanticChange(before: unknown, after: unknown, ignoredField: string | null): void {");
        line(&mut output, "  if (typeof before !== \"object\" || before === null || typeof after !== \"object\" || after === null) return;");
        line(&mut output, "  const previous = { ...(before as Record<string, unknown>) }; const current = { ...(after as Record<string, unknown>) };");
        line(&mut output, "  if (ignoredField !== null) { delete previous[ignoredField]; delete current[ignoredField]; }");
        line(&mut output, "  semanticChanges.set(after, JSON.stringify(comparablePersistenceValue(previous)) !== JSON.stringify(comparablePersistenceValue(current)));");
        line(&mut output, "}");
        line(&mut output, "");
        let mut postgres_constraint_ids = Vec::new();
        let mut sqlite_constraint_ids = Vec::new();
        for (name, entity) in self.entities() {
            let table = snake_case(name);
            for field in &entity.fields {
                let logical = format!("{name}.{}", field.name.text);
                if has_modifier(field, PersistenceModifier::Identity) {
                    postgres_constraint_ids.push(format!(
                        "{}:{}",
                        ts_string(&format!("{table}_identity")),
                        ts_string(&logical)
                    ));
                    sqlite_constraint_ids.push(format!(
                        "[{},{}]",
                        ts_string(&format!(
                            "UNIQUE constraint failed: {table}.{}",
                            field.name.text
                        )),
                        ts_string(&logical)
                    ));
                }
                if has_modifier(field, PersistenceModifier::Unique) {
                    postgres_constraint_ids.push(format!(
                        "{}:{}",
                        ts_string(&format!(
                            "{}_{}_unique",
                            table,
                            snake_case(&field.name.text)
                        )),
                        ts_string(&logical)
                    ));
                    sqlite_constraint_ids.push(format!(
                        "[{},{}]",
                        ts_string(&format!(
                            "UNIQUE constraint failed: {table}.{}",
                            field.name.text
                        )),
                        ts_string(&logical)
                    ));
                }
            }
            for constraint in &entity.persistence_constraints {
                let logical = format!("{name}.{}", constraint.name.text);
                postgres_constraint_ids.push(format!(
                    "{}:{}",
                    ts_string(&format!(
                        "{}_{}_unique",
                        table,
                        snake_case(&constraint.name.text)
                    )),
                    ts_string(&logical)
                ));
                sqlite_constraint_ids.push(format!(
                    "[{},{}]",
                    ts_string(&format!(
                        "UNIQUE constraint failed: {}",
                        constraint
                            .fields
                            .iter()
                            .map(|field| format!("{table}.{}", field.text))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                    ts_string(&logical)
                ));
            }
        }
        line(
            &mut output,
            &format!(
                "const postgresConstraintIds: Record<string, string> = {{{}}};",
                postgres_constraint_ids.join(",")
            ),
        );
        line(
            &mut output,
            &format!(
                "const sqliteConstraintIds: Array<[string, string]> = [{}];",
                sqlite_constraint_ids.join(",")
            ),
        );
        line(&mut output, "");
        line(
            &mut output,
            "export type PersistenceFaultKind = \"driver\" | \"constraint\" | \"cardinality\" | \"unknown\";",
        );
        line(
            &mut output,
            "export type TransactionPhase = \"begin\" | \"statement\" | \"commit\" | \"rollback\";",
        );
        line(
            &mut output,
            "export type TransactionOutcome = \"no_commit\" | \"committed\" | \"unknown\";",
        );
        line(&mut output, "export type RetryableCause = \"sqlite_busy\" | \"sqlite_locked\" | \"postgres_connection\" | \"postgres_serialization\" | \"postgres_deadlock\" | null;");
        line(&mut output, "export type TransactionEvidence = Readonly<{ phase: TransactionPhase; database: \"sqlite\" | \"postgres\"; outcome: TransactionOutcome; retryableCause: RetryableCause; rollbackProven: boolean; commitAcknowledged: boolean }>;");
        line(&mut output, "class RequestDeadlineFault extends Error { readonly code = \"JADPO_REQUEST_DEADLINE\"; constructor() { super(\"declared operation deadline exceeded\"); this.name = \"RequestDeadlineFault\"; } }");
        line(
            &mut output,
            "export class OutcomeUnknownFault extends Error {",
        );
        line(
            &mut output,
            "  constructor(operation: string, cause?: unknown) {",
        );
        line(
            &mut output,
            "    super(`persistence outcome is unknown: ${operation}`, { cause });",
        );
        line(&mut output, "    this.name = \"OutcomeUnknownFault\";");
        line(&mut output, "  }");
        line(&mut output, "}");
        line(&mut output, "");
        line(&mut output, "export class PersistenceFault extends Error {");
        line(&mut output, "  readonly operation: string;");
        line(&mut output, "  readonly kind: PersistenceFaultKind;");
        line(&mut output, "  readonly constraint: string | null;");
        line(
            &mut output,
            "  readonly transaction: TransactionEvidence | null;",
        );
        line(&mut output, "  readonly deadlineExceeded: boolean;");
        line(
            &mut output,
            "  constructor(operation: string, kind: PersistenceFaultKind, cause: unknown, constraint: string | null = null, transaction: TransactionEvidence | null = null) {",
        );
        line(
            &mut output,
            "    super(`persistence operation failed: ${operation}`, { cause });",
        );
        line(&mut output, "    this.name = \"PersistenceFault\";");
        line(&mut output, "    this.operation = operation;");
        line(&mut output, "    this.kind = cause instanceof OutcomeUnknownFault ? \"unknown\" : cause instanceof PersistenceFault && cause.kind === \"unknown\" ? \"unknown\" : kind;");
        line(&mut output, "    this.constraint = constraint;");
        line(&mut output, "    this.transaction = transaction ?? (cause instanceof PersistenceFault ? cause.transaction : null);");
        line(&mut output, "    this.deadlineExceeded = cause instanceof Error && \"code\" in cause && cause.code === \"JADPO_REQUEST_DEADLINE\" || cause instanceof PersistenceFault && cause.deadlineExceeded;");
        line(&mut output, "  }");
        line(&mut output, "  isReadUnavailable(): boolean {");
        line(&mut output, "    return this.kind === \"driver\" && this.transaction === null && persistenceErrorCodes(this).some(code => code === \"SQLITE_BUSY\" || code === \"SQLITE_LOCKED\" || code === \"ERR_POSTGRES_CONNECTION_CLOSED\");");
        line(&mut output, "  }");
        line(&mut output, "}");
        line(&mut output, "");
        line(&mut output, "export class PolicyFault extends Error {");
        line(&mut output, "  readonly operation: string;");
        line(&mut output, "  constructor(operation: string) {");
        line(
            &mut output,
            "    super(`policy rejected operation: ${operation}`);",
        );
        line(&mut output, "    this.name = \"PolicyFault\";");
        line(&mut output, "    this.operation = operation;");
        line(&mut output, "  }");
        line(&mut output, "}");
        line(&mut output, "");
        line(
            &mut output,
            "function hasOwn(value: Record<string, unknown>, key: string): boolean {",
        );
        line(
            &mut output,
            "  return Object.prototype.hasOwnProperty.call(value, key);",
        );
        line(&mut output, "}");
        line(&mut output, "");
        line(
            &mut output,
            "function identifyPersistenceConstraint(error: unknown): string | null {",
        );
        line(
            &mut output,
            "  const details = typeof error === \"object\" && error !== null ? error as { constraint?: unknown; message?: unknown } : {};",
        );
        line(
            &mut output,
            "  const postgresName = typeof details.constraint === \"string\" ? details.constraint : \"\";",
        );
        line(
            &mut output,
            "  if (postgresConstraintIds[postgresName] !== undefined) return postgresConstraintIds[postgresName];",
        );
        line(
            &mut output,
            "  const message = typeof details.message === \"string\" ? details.message : \"\";",
        );
        line(
            &mut output,
            "  for (const [signature, identity] of sqliteConstraintIds) if (message.includes(signature)) return identity;",
        );
        line(&mut output, "  return null;");
        line(&mut output, "}");
        line(&mut output, "");
        line(
            &mut output,
            "function persistenceErrorCodes(error: unknown): string[] {",
        );
        line(
            &mut output,
            "  const codes: string[] = []; const seen = new Set<object>(); let current: unknown = error;",
        );
        line(
            &mut output,
            "  for (let depth = 0; depth < 8 && typeof current === \"object\" && current !== null && !seen.has(current); depth++) { seen.add(current); const details = current as { code?: unknown; errno?: unknown; cause?: unknown }; for (const value of [details.code, details.errno]) if (value !== undefined && value !== null) codes.push(String(value)); current = details.cause; }",
        );
        line(&mut output, "  return codes;");
        line(&mut output, "}");
        line(&mut output, "");
        line(&mut output, "function isPersistenceAvailabilityFailure(error: unknown): boolean { const codes = persistenceErrorCodes(error); return codes.some(code => code.startsWith(\"ERR_POSTGRES_CONNECTION_\") || code === \"ERR_POSTGRES_IDLE_TIMEOUT\" || code === \"ERR_POSTGRES_LIFETIME_TIMEOUT\" || code.startsWith(\"08\") || code === \"57P01\" || code === \"57P02\" || code === \"57P03\" || code === \"SQLITE_CANTOPEN\" || code === \"SQLITE_IOERR\"); }");
        line(&mut output, "");
        line(
            &mut output,
            "function retryablePersistenceCause(error: unknown): RetryableCause {",
        );
        line(&mut output, "  const codes = persistenceErrorCodes(error);");
        line(
            &mut output,
            "  if (codes.some(code => code === \"SQLITE_BUSY\")) return \"sqlite_busy\";",
        );
        line(&mut output, "  if (codes.some(code => code === \"SQLITE_LOCKED\" || code === \"SQLITE_LOCKED_SHAREDCACHE\")) return \"sqlite_locked\";");
        line(&mut output, "  if (codes.some(code => code === \"ERR_POSTGRES_CONNECTION_CLOSED\")) return \"postgres_connection\";");
        line(
            &mut output,
            "  if (codes.some(code => code === \"40001\")) return \"postgres_serialization\";",
        );
        line(
            &mut output,
            "  if (codes.some(code => code === \"40P01\")) return \"postgres_deadlock\";",
        );
        line(&mut output, "  return null;");
        line(&mut output, "}");
        line(&mut output, "");
        line(
            &mut output,
            "function classifyPersistenceError(error: unknown): PersistenceFaultKind {",
        );
        line(&mut output, "  const codes = persistenceErrorCodes(error);");
        line(
            &mut output,
            "  return codes.some(code => code.startsWith(\"23\") || code.startsWith(\"SQLITE_CONSTRAINT\")) ? \"constraint\" : \"driver\";",
        );
        line(&mut output, "}");
        line(&mut output, "function transactionFault(operation: string, cause: unknown, transaction: TransactionEvidence): PersistenceFault {");
        line(&mut output, "  return new PersistenceFault(operation, transaction.outcome === \"unknown\" ? \"unknown\" : classifyPersistenceError(cause), cause, identifyPersistenceConstraint(cause), Object.freeze(transaction));");
        line(&mut output, "}");
        line(&mut output, "");
        line(
            &mut output,
            "function persistenceSyncBase<T>(operation: string, execute: () => T): T {",
        );
        line(&mut output, "  try { return execute(); }");
        line(&mut output, "  catch (error) { const fault = error instanceof PersistenceFault ? error : new PersistenceFault(operation, classifyPersistenceError(error), error, identifyPersistenceConstraint(error)); if (isPersistenceAvailabilityFailure(error)) { persistenceProbeNeeded = true; persistenceInvalidationGeneration++; } throw fault; }");
        line(&mut output, "}");
        line(&mut output, "function persistenceSync<T>(operation: string, execute: () => T): T { return persistenceSyncBase(operation, execute); }");
        line(&mut output, "");
        line(&mut output, "function persistenceDeadlineFault(operation: string, requestDeadlineAt: number | null): PersistenceFault { const cause = requestDeadlineAt !== null && performance.now() >= requestDeadlineAt ? new RequestDeadlineFault() : new Error(\"transaction retry window exceeded\"); return new PersistenceFault(operation, \"driver\", cause); }");
        line(&mut output, "async function persistenceAsyncWithSignal<T>(operation: string, execute: () => Promise<T>, signal: AbortSignal | null, deadlineAt: number | null, requestDeadlineAt: number | null): Promise<T> {");
        line(&mut output, "  if (signal?.aborted) throw new PersistenceFault(operation, \"driver\", new Error(\"operation cancelled\"));");
        line(&mut output, "  if (deadlineAt !== null && performance.now() >= deadlineAt) throw persistenceDeadlineFault(operation, requestDeadlineAt);");
        line(&mut output, "  try { const result = await execute(); if (signal?.aborted) throw new PersistenceFault(operation, \"driver\", new Error(\"operation cancelled\")); if (deadlineAt !== null && performance.now() >= deadlineAt) throw persistenceDeadlineFault(operation, requestDeadlineAt); return result; }");
        line(&mut output, "  catch (error) { if (deadlineAt !== null && performance.now() >= deadlineAt && persistenceErrorCodes(error).includes(\"57014\") && !(error instanceof PersistenceFault && error.kind === \"unknown\")) throw persistenceDeadlineFault(operation, requestDeadlineAt); const fault = error instanceof PersistenceFault ? error : new PersistenceFault(operation, classifyPersistenceError(error), error, identifyPersistenceConstraint(error)); if (isPersistenceAvailabilityFailure(error)) { persistenceProbeNeeded = true; persistenceInvalidationGeneration++; } throw fault; }");
        line(&mut output, "}");
        line(&mut output, "async function persistenceAsync<T>(operation: string, execute: () => Promise<T>): Promise<T> { return persistenceAsyncWithSignal(operation, execute, null, null, null); }");
        line(&mut output, "");
        line(
            &mut output,
            "function reportPersistenceDependencyUnavailable(error: unknown): void {",
        );
        line(&mut output, "  console.error(JSON.stringify({");
        line(&mut output, "    schemaVersion: 1,");
        line(&mut output, "    kind: \"operational_log_event\",");
        line(&mut output, "    eventName: \"dependency.unavailable\",");
        line(
            &mut output,
            "    classification: \"RUNTIME_DEPENDENCY_UNAVAILABLE\",",
        );
        line(&mut output, "    requestId: \"readiness\",");
        line(&mut output, "    traceId: null,");
        line(&mut output, "    semanticOperationId: \"health:ready\",");
        line(
            &mut output,
            &format!(
                "    sourceRevision: {},",
                ts_string(&self.source_revision())
            ),
        );
        line(&mut output, "    attributes: {},");
        line(&mut output, "  }));");
        line(
            &mut output,
            "  if (Bun.env.JADPO_DEBUG_TARGET_STACKS === \"1\") console.error(error);",
        );
        line(&mut output, "}");
        line(&mut output, "");
        line(
            &mut output,
            "function reportPersistenceStartupFault(error: unknown): void {",
        );
        line(&mut output, "  console.error(JSON.stringify({ schemaVersion: 1, kind: \"operational_log_event\", eventName: \"operation.failed\", classification: \"RUNTIME_STARTUP_FAILED\", requestId: \"startup\", traceId: null, semanticOperationId: \"runtime:start\", sourceRevision: " );
        line(
            &mut output,
            &format!("    {},", ts_string(&self.source_revision())),
        );
        line(&mut output, "    attributes: {}, }));");
        line(
            &mut output,
            "  if (Bun.env.JADPO_DEBUG_TARGET_STACKS === \"1\") console.error(error);",
        );
        line(&mut output, "}");
        line(&mut output, "");
        line(
            &mut output,
            "const localPath = decodeURIComponent(new URL(\"../local.sqlite\", import.meta.url).pathname);",
        );
        line(&mut output, "let postgres: SQL | null = null;");
        line(&mut output, "let sqlite: Database | null = null;");
        line(
            &mut output,
            "let persistenceDependencyUnavailableReported = false;",
        );
        line(&mut output, "let persistenceStartupFaultReported = false;");
        line(&mut output, "let persistenceInitializationFatal = false;");
        line(
            &mut output,
            "async function initializePersistenceOnce(publish = true): Promise<boolean> {",
        );
        line(&mut output, "try {");
        line(
            &mut output,
            "  if (Bun.env.DATABASE_URL && postgres === null) postgres = persistenceSync(\"database.open\", () => new SQL({ url: Bun.env.DATABASE_URL!, prepare: false, connectionTimeout: 2 }));",
        );
        line(
            &mut output,
            "  if (postgres === null && sqlite === null) sqlite = persistenceSync(\"database.open\", () => new Database(Bun.env.SQLITE_PATH ?? localPath, { create: true, strict: true }));",
        );
        line(
            &mut output,
            "  if (sqlite !== null) persistenceSync(\"database.foreign_keys\", () => sqlite!.exec(\"PRAGMA foreign_keys = ON\"));",
        );
        if self.has_derived_representations() {
            line(&mut output, "  if (postgres !== null) await persistenceAsync(\"schema.change_log\", () => postgres!`CREATE TABLE IF NOT EXISTS \"__jadpo_changes\" (\"change_id\" TEXT PRIMARY KEY, \"entity\" TEXT NOT NULL, \"entity_id\" TEXT NOT NULL, \"revision\" BIGINT NOT NULL, \"operation\" TEXT NOT NULL, \"payload\" TEXT NOT NULL, \"created_at\" TIMESTAMPTZ NOT NULL, UNIQUE (\"entity\", \"entity_id\", \"revision\"))`);");
            line(&mut output, "  else persistenceSync(\"schema.change_log\", () => sqlite!.exec('CREATE TABLE IF NOT EXISTS \"__jadpo_changes\" (\"change_id\" TEXT PRIMARY KEY, \"entity\" TEXT NOT NULL, \"entity_id\" TEXT NOT NULL, \"revision\" INTEGER NOT NULL, \"operation\" TEXT NOT NULL, \"payload\" TEXT NOT NULL, \"created_at\" TEXT NOT NULL, UNIQUE (\"entity\", \"entity_id\", \"revision\"))'));");
        }
        for (name, entity) in self.schema_entities() {
            let table = sql_identifier(&snake_case(name));
            let postgres_columns = self.table_body(name, entity, SqlDialect::Postgres, "", ", ");
            let sqlite_columns = self.table_body(name, entity, SqlDialect::Sqlite, "", ", ");
            line(
                &mut output,
                &format!(
                    "  if (postgres !== null) await persistenceAsync(\"schema.{name}\", () => postgres!`CREATE TABLE IF NOT EXISTS {table} ({postgres_columns})`);"
                ),
            );
            line(
                &mut output,
                &format!(
                    "  else persistenceSync(\"schema.{name}\", () => sqlite!.exec({}));",
                    ts_string(&format!(
                        "CREATE TABLE IF NOT EXISTS {table} ({sqlite_columns})"
                    ))
                ),
            );
            for statement in self.index_statements(name, entity) {
                line(
                    &mut output,
                    &format!(
                        "  if (postgres !== null) await persistenceAsync(\"schema.{name}.index\", () => postgres!`{statement}`);"
                    ),
                );
                line(
                    &mut output,
                    &format!(
                        "  else persistenceSync(\"schema.{name}.index\", () => sqlite!.exec({}));",
                        ts_string(&statement)
                    ),
                );
            }
        }
        if self.first_party_supported() {
            line(&mut output, "  await initializeAuthenticationStorage();");
        }
        line(&mut output, "  if (publish) { persistenceReady = true; persistenceProbeNeeded = false; persistenceDependencyUnavailableReported = false; persistenceStartupFaultReported = false; persistenceInitializationFatal = false; } return true;");
        line(&mut output, "} catch (error) {");
        line(&mut output, "  persistenceReady = false;");
        line(&mut output, "  const initializationContended = persistenceErrorCodes(error).some(code => code === \"SQLITE_BUSY\" || code.startsWith(\"SQLITE_BUSY_\") || code === \"SQLITE_LOCKED\" || code.startsWith(\"SQLITE_LOCKED_\"));");
        line(&mut output, "  if (isPersistenceAvailabilityFailure(error) || initializationContended) { if (!persistenceDependencyUnavailableReported) { reportPersistenceDependencyUnavailable(error); persistenceDependencyUnavailableReported = true; } }");
        line(&mut output, "  else { persistenceInitializationFatal = true; if (!persistenceStartupFaultReported) { reportPersistenceStartupFault(error); persistenceStartupFaultReported = true; } }");
        line(&mut output, "  return false;");
        line(&mut output, "}");
        line(&mut output, "}");
        line(&mut output, "");
        if self.has_derived_representations() {
            line(&mut output, "function recordAuthorityChange(entity: string, operation: string, value: Record<string, unknown>, operationTime: string | null, connection: SQL | null = postgres, sqliteConnection: Database | null = sqlite): Promise<void> | void {");
            line(&mut output, "  let identityField: string | null = null;");
            line(&mut output, "  switch (entity) {");
            for entity in &self.project.entity_model.entities {
                if !entity.representations.is_empty() {
                    line(
                        &mut output,
                        &format!(
                            "    case {}: identityField = {}; break;",
                            ts_string(&entity.name),
                            ts_string(&entity.identity)
                        ),
                    );
                }
            }
            line(&mut output, "    default: return;");
            line(&mut output, "  }");
            line(
                &mut output,
                "  const entityId = String(value[identityField!] ?? \"\");",
            );
            line(&mut output, "  const changeId = crypto.randomUUID();");
            line(&mut output, "  const payload = JSON.stringify(value);");
            line(&mut output, "  if (operationTime === null) throw new PersistenceFault(\"change.operation_time\", \"data\", new Error(\"authority changes require a captured operation instant\"));");
            line(
                &mut output,
                "  const createdAt = persistenceInstant(operationTime, true);",
            );
            line(&mut output, "  if (connection !== null) return persistenceAsync(`change.${entity}.${operation}`, async () => { await connection!`INSERT INTO \"__jadpo_changes\" (\"change_id\", \"entity\", \"entity_id\", \"revision\", \"operation\", \"payload\", \"created_at\") SELECT ${changeId}, ${entity}, ${entityId}, COALESCE(MAX(\"revision\"), 0) + 1, ${operation}, ${payload}, ${createdAt} FROM \"__jadpo_changes\" WHERE \"entity\" = ${entity} AND \"entity_id\" = ${entityId}`; });");
            line(&mut output, "  if (sqliteConnection === null) throw new PersistenceFault(\"change.connection\", \"driver\", new Error(\"SQLite authority connection is unavailable\"));");
            line(&mut output, "  return persistenceSync(`change.${entity}.${operation}`, () => sqliteConnection.prepare('INSERT INTO \"__jadpo_changes\" (\"change_id\", \"entity\", \"entity_id\", \"revision\", \"operation\", \"payload\", \"created_at\") SELECT ?1, ?2, ?3, COALESCE(MAX(\"revision\"), 0) + 1, ?4, ?5, ?6 FROM \"__jadpo_changes\" WHERE \"entity\" = ?2 AND \"entity_id\" = ?3').run(changeId, entity, entityId, operation, payload, createdAt));");
            line(&mut output, "}");
            line(&mut output, "");
        }
        line(
            &mut output,
            "let sqliteTransactionTail: Promise<void> = Promise.resolve();",
        );
        line(
            &mut output,
            "async function serializeSQLiteTransaction<T>(execute: () => Promise<T>): Promise<T> {",
        );
        line(
            &mut output,
            "  const result = sqliteTransactionTail.then(execute, execute);",
        );
        line(
            &mut output,
            "  sqliteTransactionTail = result.then(() => undefined, () => undefined);",
        );
        line(&mut output, "  return result;");
        line(&mut output, "}");
        line(&mut output, "");
        line(&mut output, "type PolicyPrincipal = Readonly<{ kind: \"user\" | \"service\"; subject: string; values: Readonly<Record<string, unknown>> }>; ");
        line(&mut output, "type PolicyRequest = Readonly<{ principal: PolicyPrincipal | null; operation: string }>; ");
        line(&mut output, "type TransactionRetryOptions = Readonly<{ replayable: boolean; operationId: string; operation: string; startedAt: number; deadlineAt?: number | null; signal: AbortSignal | null; refreshOperationTime?: () => string }>;");
        line(&mut output, "function recordTransactionAttempt(options: TransactionRetryOptions, attempt: number, evidence: TransactionEvidence, elapsedMs: number, selectedDelayMs: number | null, retryScheduled: boolean, cancelled: boolean): void {");
        line(&mut output, "  console.error(JSON.stringify({ schemaVersion: 1, kind: \"operational_log_event\", eventName: \"transaction.attempt\", operationId: options.operationId, operation: options.operation, attempt, database: evidence.database, phase: evidence.phase, outcome: evidence.outcome, retryableCause: evidence.retryableCause, rollbackProven: evidence.rollbackProven, commitAcknowledged: evidence.commitAcknowledged, elapsedMs: Math.max(0, Math.round(elapsedMs)), selectedDelayMs: selectedDelayMs === null ? null : Math.round(selectedDelayMs), retryScheduled, cancelled }));");
        line(&mut output, "}");
        line(&mut output, "const emptyPolicyRequest: PolicyRequest = Object.freeze({ principal: null, operation: \"internal\" });");
        if self.has_lifecycle_purges() {
            line(&mut output, "let lifecycleRetentionMilliseconds: Readonly<Record<string, number>> | null = null;");
            line(&mut output, "export function configureLifecycleRetention(configuration: Record<string, unknown>): void {");
            line(
                &mut output,
                "  const configuredRetention: Record<string, number> = Object.create(null);",
            );
            for (name, entity) in self.entities() {
                let Some(purge) = entity
                    .dossier
                    .as_ref()
                    .and_then(|dossier| dossier.lifecycle.as_ref())
                    .and_then(|lifecycle| lifecycle.purge.as_ref())
                else {
                    continue;
                };
                let Expression::Name(binding) = &purge.after else {
                    continue;
                };
                let Some(field) = binding.path.get(1).map(|part| part.text.as_str()) else {
                    continue;
                };
                line(&mut output, "  {");
                line(&mut output, &format!("    const milliseconds = configuredPersistenceDuration(configuration[{}], {});", ts_string(field), ts_string(&format!("config.{field}"))));
                line(&mut output, &format!("    if (milliseconds <= 0) throw new PersistenceFault(\"retention.configuration\", \"data\", new Error(\"configured retention must be positive: config.{field}\"));"));
                line(
                    &mut output,
                    &format!(
                        "    configuredRetention[{}] = milliseconds;",
                        ts_string(name)
                    ),
                );
                line(&mut output, "  }");
            }
            line(
                &mut output,
                "  lifecycleRetentionMilliseconds = Object.freeze(configuredRetention);",
            );
            line(&mut output, "}");
        }
        line(&mut output, "");
        self.first_party_storage(&mut output);
        self.persistence_policy_runtime(&mut output);
        output.push_str(include_str!("runtime/delivery_claims.ts"));
        output.push_str(include_str!("runtime/delivery_scheduler.ts"));
        line(&mut output, "function createPersistenceClient(postgres: SQL | null, sqlite: Database | null, transactional = false, transactionState: { nextSavepoint: number; active: boolean; poisoned?: boolean } = { nextSavepoint: 0, active: false }, policy: PolicyRequest = emptyPolicyRequest, operationTime: string | null = null, signal: AbortSignal | null = null, deadlineAt: number | null = null, requestDeadlineAt: number | null = deadlineAt) {");
        line(&mut output, "const assertTransactionActive = (): void => { if (!transactional) return; if (sqlite !== null && sqlite.inTransaction !== true) { transactionState.active = false; transactionState.poisoned = true; } if (!transactionState.active || transactionState.poisoned) throw new PersistenceFault(\"transaction.inactive\", \"driver\", new Error(\"transaction capability expired\")); };");
        line(&mut output, "const persistenceAsync = async <T>(operation: string, execute: () => Promise<T>): Promise<T> => {");
        line(&mut output, "  assertTransactionActive();");
        line(
            &mut output,
            "  if (postgres !== null && transactional && deadlineAt !== null) {",
        );
        line(&mut output, "    const remaining = deadlineAt - performance.now(); if (remaining <= 0) throw persistenceDeadlineFault(operation, requestDeadlineAt);");
        line(&mut output, "    // Transaction-local server enforcement works with the pinned Bun adapter even when query.cancel() does not interrupt an active query.");
        line(&mut output, "    await persistenceAsyncWithSignal(\"transaction.timeout.refresh\", () => postgres!.unsafe(\"SELECT set_config('statement_timeout', $1, true)\", [String(Math.min(Math.ceil(remaining), 2147483647))]), signal, deadlineAt, requestDeadlineAt);");
        line(&mut output, "  }");
        line(&mut output, "  return persistenceAsyncWithSignal(operation, () => { assertTransactionActive(); return execute(); }, signal, deadlineAt, requestDeadlineAt);");
        line(&mut output, "};");
        line(
            &mut output,
            "const persistenceSync = <T>(operation: string, execute: () => T): T => {",
        );
        line(&mut output, "  const cleanup = operation === \"transaction.rollback\" || operation.endsWith(\".rollback\") || operation.endsWith(\".release\");");
        line(&mut output, "  if (!cleanup) assertTransactionActive();");
        line(&mut output, "  const stopped = () => signal?.aborted ? new PersistenceFault(operation, \"driver\", new Error(\"operation cancelled\")) : deadlineAt !== null && performance.now() >= deadlineAt ? persistenceDeadlineFault(operation, requestDeadlineAt) : null;");
        line(
            &mut output,
            "  if (!cleanup) { const fault = stopped(); if (fault !== null) throw fault; }",
        );
        line(
            &mut output,
            "  const result = persistenceSyncBase(operation, execute);",
        );
        line(&mut output, "  if (!cleanup && operation !== \"transaction.commit\") { const fault = stopped(); if (fault !== null) throw fault; }");
        line(&mut output, "  return result;");
        line(&mut output, "};");
        line(&mut output, "async function policyReadRows(operation: string, entity: string, postgresSql: string, sqliteSql: string, postgresValues: unknown[], sqliteValues: unknown[]): Promise<unknown[]> {");
        line(&mut output, "  const scope = policySqlScope(entity, \"read\", policy, postgres !== null ? \"postgres\" : \"sqlite\", postgres !== null ? postgresValues.length : sqliteValues.length);");
        line(&mut output, "  if (!scope.permitted) return [];");
        line(&mut output, "  return postgres !== null");
        line(&mut output, "    ? persistenceAsync(operation, () => postgres!.unsafe(policyScopedSql(postgresSql, scope.clause), [...postgresValues, ...scope.values]))");
        line(&mut output, "    : persistenceSync(operation, () => { const statement = sqlite!.prepare(policyScopedSql(sqliteSql, scope.clause)); try { return statement.all(...sqliteValues, ...scope.values); } finally { statement.finalize(); } });");
        line(&mut output, "}");
        line(&mut output, "");
        line(&mut output, "async function policyPostgresMutationRows(connection: SQL, entity: string, effect: string, fields: readonly string[], sql: string, values: unknown[]): Promise<unknown[]> {");
        line(&mut output, "  const effects = [effect, ...fields.filter(field => policyFieldEffects.has(`${entity}:${effect}.${field}`)).map(field => `${effect}.${field}`)];");
        line(&mut output, "  let scopedSql = sql;");
        line(&mut output, "  const scopedValues = [...values];");
        line(&mut output, "  for (const requiredEffect of effects) {");
        line(&mut output, "    const scope = policySqlScope(entity, requiredEffect, policy, \"postgres\", scopedValues.length);");
        line(&mut output, "    if (!scope.permitted) return [];");
        line(
            &mut output,
            "    scopedSql = policyScopedSql(scopedSql, scope.clause);",
        );
        line(&mut output, "    scopedValues.push(...scope.values);");
        line(&mut output, "  }");
        line(&mut output, "  return persistenceAsync(`mutation.${entity}.${effect}.policy`, () => connection.unsafe(scopedSql, scopedValues));");
        line(&mut output, "}");
        line(&mut output, "");
        line(&mut output, "function policySqliteMutationRows(entity: string, effect: string, fields: readonly string[], sql: string, values: unknown[]): unknown[] {");
        line(&mut output, "  const effects = [effect, ...fields.filter(field => policyFieldEffects.has(`${entity}:${effect}.${field}`)).map(field => `${effect}.${field}`)];");
        line(&mut output, "  let scopedSql = sql;");
        line(&mut output, "  const scopedValues = [...values];");
        line(&mut output, "  for (const requiredEffect of effects) {");
        line(&mut output, "    const scope = policySqlScope(entity, requiredEffect, policy, \"sqlite\", scopedValues.length);");
        line(&mut output, "    if (!scope.permitted) return [];");
        line(
            &mut output,
            "    scopedSql = policyScopedSql(scopedSql, scope.clause);",
        );
        line(&mut output, "    scopedValues.push(...scope.values);");
        line(&mut output, "  }");
        line(&mut output, "  return persistenceSync(`mutation.${entity}.${effect}.policy`, () => sqlite!.prepare(scopedSql).all(...scopedValues));");
        line(&mut output, "}");
        line(&mut output, "");
        output.push_str(&include_str!("runtime/delivery_outbox.ts").replace(
            "// __JADPO_BOUND_DELIVERY_METHODS__",
            &(self.delivery_selection_methods() + &self.delivery_invocation_methods()),
        ));
        line(&mut output, "const client = {");
        line(&mut output, "  ...deliveryOutbox,");
        self.delivery_authority_methods(&mut output);
        line(
            &mut output,
            "  async transaction<T>(work: (client: any) => Promise<T>, retryOptions: TransactionRetryOptions | null = null): Promise<T> {",
        );
        line(&mut output, "    if (transactional) {");
        line(&mut output, "      if (!transactionState.active) throw new PersistenceFault(\"transaction.inactive\", \"driver\", new Error(\"transaction capability expired\"));");
        line(
            &mut output,
            "      const savepoint = `jadpo_sp_${++transactionState.nextSavepoint}`;",
        );
        line(&mut output, "      if (postgres !== null) {");
        line(
            &mut output,
            "        await persistenceAsyncWithSignal(\"transaction.savepoint\", () => postgres!.unsafe(`SAVEPOINT \"${savepoint}\"`), signal, deadlineAt, requestDeadlineAt);",
        );
        line(&mut output, "        try {");
        line(&mut output, "          const result = await work(client);");
        line(
            &mut output,
            "          await persistenceAsyncWithSignal(\"transaction.savepoint.release\", () => postgres!.unsafe(`RELEASE SAVEPOINT \"${savepoint}\"`), null, null, requestDeadlineAt);",
        );
        line(&mut output, "          return result;");
        line(&mut output, "        } catch (error) {");
        line(&mut output, "          try {");
        line(
            &mut output,
            "          await persistenceAsyncWithSignal(\"transaction.savepoint.rollback\", () => postgres!.unsafe(`ROLLBACK TO SAVEPOINT \"${savepoint}\"`), null, null, requestDeadlineAt);",
        );
        line(
            &mut output,
            "          await persistenceAsyncWithSignal(\"transaction.savepoint.release\", () => postgres!.unsafe(`RELEASE SAVEPOINT \"${savepoint}\"`), null, null, requestDeadlineAt);",
        );
        line(&mut output, "          } catch (cleanupError) { transactionState.active = false; transactionState.poisoned = true; throw cleanupError; }");
        line(&mut output, "          throw error;");
        line(&mut output, "        }");
        line(&mut output, "      }");
        line(&mut output, "      persistenceSync(\"transaction.savepoint\", () => sqlite!.exec(`SAVEPOINT \"${savepoint}\"`));");
        line(&mut output, "      try {");
        line(&mut output, "        const result = await work(client);");
        line(&mut output, "        persistenceSync(\"transaction.savepoint.release\", () => sqlite!.exec(`RELEASE SAVEPOINT \"${savepoint}\"`));");
        line(&mut output, "        return result;");
        line(&mut output, "      } catch (error) {");
        line(&mut output, "        try {");
        line(&mut output, "        persistenceSync(\"transaction.savepoint.rollback\", () => sqlite!.exec(`ROLLBACK TO SAVEPOINT \"${savepoint}\"`));");
        line(&mut output, "        persistenceSync(\"transaction.savepoint.release\", () => sqlite!.exec(`RELEASE SAVEPOINT \"${savepoint}\"`));");
        line(&mut output, "        } catch (cleanupError) { transactionState.active = false; transactionState.poisoned = true; throw cleanupError; }");
        line(&mut output, "        throw error;");
        line(&mut output, "      }");
        line(&mut output, "    }");
        line(&mut output, "    const executeAttempt = async (attemptOperationTime: string | null, retryIndex: number): Promise<T> => {");
        line(
            &mut output,
            "    const attemptSignal = retryOptions?.signal ?? signal;",
        );
        line(&mut output, "    const retryDeadlineAt = retryOptions !== null && retryIndex > 0 ? Math.min(retryOptions.startedAt + 1000, retryOptions.deadlineAt ?? Infinity) : null;");
        line(&mut output, "    const attemptDeadlineAt = deadlineAt === null ? retryDeadlineAt : retryDeadlineAt === null ? deadlineAt : Math.min(deadlineAt, retryDeadlineAt);");
        line(&mut output, "    const checkAttemptLimit = (operation: string): PersistenceFault | null => { if (attemptSignal?.aborted) return new PersistenceFault(operation, \"driver\", new Error(\"operation cancelled\")); if (attemptDeadlineAt !== null && performance.now() >= attemptDeadlineAt) return persistenceDeadlineFault(operation, requestDeadlineAt); return null; };");
        line(&mut output, "    if (postgres !== null) {");
        line(&mut output, "      let callbackState: \"not_started\" | \"failed\" | \"returned\" = \"not_started\";");
        line(&mut output, "      try {");
        line(&mut output, "        const beginLimit = checkAttemptLimit(\"transaction.begin\"); if (beginLimit !== null) throw beginLimit;");
        line(
            &mut output,
            "        return await postgres.begin(async tx => {",
        );
        line(&mut output, "          callbackState = \"failed\";");
        line(
            &mut output,
            "          const attemptState = { nextSavepoint: 0, active: true };",
        );
        line(&mut output, "          try {");
        line(&mut output, "          if (attemptDeadlineAt !== null) { const remaining = attemptDeadlineAt - performance.now(); if (remaining <= 0) throw persistenceDeadlineFault(\"transaction.statement\", requestDeadlineAt); await persistenceAsyncWithSignal(\"transaction.timeout.setup\", () => tx.unsafe(\"SELECT set_config('statement_timeout', $1, true)\", [String(Math.min(Math.ceil(remaining), 2147483647))]), attemptSignal, attemptDeadlineAt, requestDeadlineAt); }");
        line(
            &mut output,
            "          const result = await work(createPersistenceClient(tx as SQL, null, true, attemptState, policy, attemptOperationTime, attemptSignal, attemptDeadlineAt, requestDeadlineAt)); if ((attemptState as { poisoned?: boolean }).poisoned) throw new PersistenceFault(\"transaction.poisoned\", \"driver\", new Error(\"transaction recovery unproved\")); attemptState.active = false; const callbackLimit = checkAttemptLimit(\"transaction.statement\"); if (callbackLimit !== null) throw callbackLimit; if (attemptDeadlineAt !== null) await persistenceAsyncWithSignal(\"transaction.timeout.reset\", () => tx.unsafe(\"SET LOCAL statement_timeout = 0\"), attemptSignal, attemptDeadlineAt, requestDeadlineAt); const afterResetLimit = checkAttemptLimit(\"transaction.statement\"); if (afterResetLimit !== null) throw afterResetLimit; callbackState = \"returned\"; return result;",
        );
        line(
            &mut output,
            "          } finally { attemptState.active = false; }",
        );
        line(&mut output, "        });");
        line(&mut output, "      } catch (error) {");
        line(
            &mut output,
            "        const retryableCause = retryablePersistenceCause(error);",
        );
        line(&mut output, "        if (callbackState === \"failed\") {");
        line(&mut output, "          if (attemptDeadlineAt !== null && performance.now() >= attemptDeadlineAt && persistenceErrorCodes(error).includes(\"57014\") && !(error instanceof PersistenceFault && error.kind === \"unknown\")) error = persistenceDeadlineFault(\"transaction.statement\", requestDeadlineAt);");
        line(
            &mut output,
            "          if (!(error instanceof PersistenceFault)) throw error;",
        );
        line(
            &mut output,
            "          if (error.kind === \"unknown\") throw error;",
        );
        line(&mut output, "          const serverAbort = retryableCause === \"postgres_serialization\" || retryableCause === \"postgres_deadlock\";");
        line(&mut output, "          throw new PersistenceFault(error.operation, error.kind, error, error.constraint, Object.freeze({ phase: \"statement\", database: \"postgres\", outcome: \"no_commit\", retryableCause: serverAbort ? retryableCause : null, rollbackProven: true, commitAcknowledged: false }));");
        line(&mut output, "        }");
        line(&mut output, "        if (callbackState === \"not_started\") throw transactionFault(\"transaction.begin\", error, { phase: \"begin\", database: \"postgres\", outcome: \"no_commit\", retryableCause, rollbackProven: false, commitAcknowledged: false });");
        line(&mut output, "        const serverAbort = retryableCause === \"postgres_serialization\" || retryableCause === \"postgres_deadlock\";");
        line(&mut output, "        throw transactionFault(\"transaction.commit\", error, { phase: \"commit\", database: \"postgres\", outcome: serverAbort ? \"no_commit\" : \"unknown\", retryableCause: serverAbort ? retryableCause : null, rollbackProven: serverAbort, commitAcknowledged: false });");
        line(&mut output, "      }");
        line(&mut output, "    }");
        line(
            &mut output,
            "    return serializeSQLiteTransaction(async () => {",
        );
        line(&mut output, "      try { const beginLimit = checkAttemptLimit(\"transaction.begin\"); if (beginLimit !== null) throw beginLimit; persistenceSync(\"transaction.begin\", () => sqlite!.exec(\"BEGIN IMMEDIATE\")); const afterBeginLimit = checkAttemptLimit(\"transaction.begin\"); if (afterBeginLimit !== null) throw afterBeginLimit; }");
        line(&mut output, "      catch (error) { const opened = sqlite!.inTransaction === true; let rollbackProven = false; if (opened) { try { sqlite!.exec(\"ROLLBACK\"); rollbackProven = true; } catch { rollbackProven = false; } } const retryableCause = opened ? (rollbackProven ? retryablePersistenceCause(error) : null) : retryablePersistenceCause(error); throw transactionFault(\"transaction.begin\", error, { phase: \"begin\", database: \"sqlite\", outcome: \"no_commit\", retryableCause, rollbackProven, commitAcknowledged: false }); }");
        line(&mut output, "      let result: T;");
        line(
            &mut output,
            "      const attemptState = { nextSavepoint: 0, active: true };",
        );
        line(&mut output, "      try {");
        line(
            &mut output,
            "        try { result = await work(createPersistenceClient(null, sqlite, true, attemptState, policy, attemptOperationTime, attemptSignal, attemptDeadlineAt, requestDeadlineAt)); if ((attemptState as { poisoned?: boolean }).poisoned) throw new PersistenceFault(\"transaction.poisoned\", \"driver\", new Error(\"transaction recovery unproved\")); } finally { attemptState.active = false; } const callbackLimit = checkAttemptLimit(\"transaction.statement\"); if (callbackLimit !== null) throw callbackLimit;",
        );
        line(&mut output, "      } catch (error) {");
        line(&mut output, "        try { sqlite!.exec(\"ROLLBACK\"); }");
        line(&mut output, "        catch (rollbackError) { const cause = new AggregateError([error, rollbackError], \"transaction rollback failed\", { cause: error }); throw transactionFault(\"transaction.rollback\", cause, { phase: \"rollback\", database: \"sqlite\", outcome: \"unknown\", retryableCause: null, rollbackProven: false, commitAcknowledged: false }); }");
        line(&mut output, "        if (error instanceof PersistenceFault) throw new PersistenceFault(error.operation, error.kind, error, error.constraint, Object.freeze({ phase: \"statement\", database: \"sqlite\", outcome: \"no_commit\", retryableCause: retryablePersistenceCause(error), rollbackProven: true, commitAcknowledged: false }));");
        line(&mut output, "        throw error;");
        line(&mut output, "      }");
        line(&mut output, "      try { persistenceSync(\"transaction.commit\", () => sqlite!.exec(\"COMMIT\")); return result; }");
        line(&mut output, "      catch (error) { const remainedActive = sqlite!.inTransaction === true; let rollbackProven = false; let cleanupFault: unknown = null; if (remainedActive) { try { sqlite!.exec(\"ROLLBACK\"); rollbackProven = true; } catch (rollbackError) { cleanupFault = rollbackError; } } const cause = cleanupFault === null ? error : new AggregateError([error, cleanupFault], \"transaction commit rollback failed\", { cause: error }); const retryableCause = rollbackProven ? retryablePersistenceCause(error) : null; throw transactionFault(\"transaction.commit\", cause, { phase: \"commit\", database: \"sqlite\", outcome: rollbackProven ? \"no_commit\" : \"unknown\", retryableCause, rollbackProven, commitAcknowledged: false }); }");
        line(&mut output, "    });");
        line(&mut output, "    };");
        line(
            &mut output,
            "    if (retryOptions === null) return executeAttempt(operationTime, 0);",
        );
        line(&mut output, "    let attempt = 0;");
        line(&mut output, "    while (true) {");
        line(&mut output, "      const attemptOperationTime = attempt === 0 ? operationTime : retryOptions.refreshOperationTime?.() ?? operationTime;");
        line(
            &mut output,
            "      const attemptStartedAt = performance.now();",
        );
        line(&mut output, "      try {");
        line(
            &mut output,
            "        const result = await executeAttempt(attemptOperationTime, attempt);",
        );
        line(&mut output, "        recordTransactionAttempt(retryOptions, attempt + 1, { phase: \"commit\", database: postgres !== null ? \"postgres\" : \"sqlite\", outcome: \"committed\", retryableCause: null, rollbackProven: false, commitAcknowledged: true }, performance.now() - attemptStartedAt, null, false, retryOptions.signal?.aborted === true);");
        line(&mut output, "        return result;");
        line(&mut output, "      }");
        line(&mut output, "      catch (error) {");
        line(&mut output, "        const evidence = error instanceof PersistenceFault ? error.transaction : null;");
        line(
            &mut output,
            "        const elapsedMs = performance.now() - attemptStartedAt;",
        );
        line(&mut output, "        const remainingMs = Math.max(0, Math.min(1000 - (performance.now() - retryOptions.startedAt), (retryOptions.deadlineAt ?? Infinity) - performance.now()));");
        line(&mut output, "        const canRetry = retryOptions.replayable && error instanceof PersistenceFault && error.kind !== \"unknown\" && attempt < 2 && evidence !== null && evidence.outcome === \"no_commit\" && evidence.retryableCause !== null && (evidence.phase === \"begin\" || evidence.rollbackProven) && remainingMs > 0;");
        line(&mut output, "        const selectedDelayMs = canRetry && !retryOptions.signal?.aborted ? Math.random() * Math.min(250, 50 * (2 ** attempt), remainingMs) : null;");
        line(
            &mut output,
            "        let retryScheduled = selectedDelayMs !== null;",
        );
        line(&mut output, "        if (selectedDelayMs !== null && selectedDelayMs > 0) await new Promise<void>(resolve => setTimeout(resolve, selectedDelayMs));");
        line(
            &mut output,
            "        const cancelled = retryOptions.signal?.aborted === true;",
        );
        line(&mut output, "        if (retryScheduled && (cancelled || performance.now() - retryOptions.startedAt >= 1000 || retryOptions.deadlineAt !== null && retryOptions.deadlineAt !== undefined && performance.now() >= retryOptions.deadlineAt)) retryScheduled = false;");
        line(&mut output, "        if (evidence !== null) recordTransactionAttempt(retryOptions, attempt + 1, evidence, elapsedMs, selectedDelayMs, retryScheduled, cancelled);");
        line(&mut output, "        if (!retryScheduled) { if (error instanceof PersistenceFault && error.kind !== \"unknown\" && evidence?.outcome === \"no_commit\" && retryOptions.deadlineAt !== null && retryOptions.deadlineAt !== undefined && performance.now() >= retryOptions.deadlineAt) throw new PersistenceFault(error.operation, error.kind, new RequestDeadlineFault(), error.constraint, evidence); throw error; }");
        line(&mut output, "        attempt++;");
        line(&mut output, "      }");
        line(&mut output, "    }");
        line(&mut output, "  },");
        line(&mut output, "  did_change(value: unknown): boolean {");
        line(&mut output, "    return typeof value === \"object\" && value !== null && semanticChanges.get(value) === true;");
        line(&mut output, "  },");
        line(&mut output, "  async query_page_rows(operation: string, entity: string, postgresFirstSql: string, sqliteFirstSql: string, postgresAfterSql: string, sqliteAfterSql: string, values: ReadonlyArray<Readonly<{ value: unknown; kind: string }>>, hasCursor: boolean): Promise<unknown[]> {");
        line(&mut output, "    const postgresValues = values.map(item => persistenceValue(item.value, item.kind, true));");
        line(&mut output, "    const sqliteValues = values.map(item => persistenceValue(item.value, item.kind, false));");
        line(&mut output, "    return policyReadRows(operation, entity, hasCursor ? postgresAfterSql : postgresFirstSql, hasCursor ? sqliteAfterSql : sqliteFirstSql, postgresValues, sqliteValues);");
        line(&mut output, "  },");
        line(
            &mut output,
            "  withPolicy(principal: PolicyPrincipal | null, operation: string) {",
        );
        line(&mut output, "    return createPersistenceClient(postgres, sqlite, transactional, transactionState, Object.freeze({ principal, operation }), operationTime, signal, deadlineAt, requestDeadlineAt);");
        line(&mut output, "  },");
        line(&mut output, "  withDeadline(value: number | null) {");
        line(&mut output, "    const bounded = value === null ? deadlineAt : deadlineAt === null ? value : Math.min(deadlineAt, value);");
        line(&mut output, "    const requestBounded = value === null ? requestDeadlineAt : requestDeadlineAt === null ? value : Math.min(requestDeadlineAt, value);");
        line(&mut output, "    return createPersistenceClient(postgres, sqlite, transactional, transactionState, policy, operationTime, signal, bounded, requestBounded);");
        line(&mut output, "  },");
        line(&mut output, "  withOperationTime(value: string) {");
        line(
            &mut output,
            "    const canonical = persistenceInstant(value, true) as string;",
        );
        line(&mut output, "    return createPersistenceClient(postgres, sqlite, transactional, transactionState, policy, canonical, signal, deadlineAt, requestDeadlineAt);");
        line(&mut output, "  },");
        line(&mut output, "  withSignal(value: AbortSignal | null) {");
        line(&mut output, "    return createPersistenceClient(postgres, sqlite, transactional, transactionState, policy, operationTime, value, deadlineAt, requestDeadlineAt);");
        line(&mut output, "  },");
        line(&mut output, "  async allowsPolicy(entity: string, effect: string, row: Record<string, unknown> = {}) {");
        line(&mut output, "    assertTransactionActive();");
        line(
            &mut output,
            "    return policyAllows(entity, effect, row, policy, postgres, sqlite);",
        );
        line(&mut output, "  },");
        for (name, entity) in self.entities() {
            let Some(lifecycle) = entity
                .dossier
                .as_ref()
                .and_then(|dossier| dossier.lifecycle.as_ref())
            else {
                continue;
            };
            let Some(purge) = &lifecycle.purge else {
                continue;
            };
            let Expression::Name(retention_binding) = &purge.after else {
                continue;
            };
            let retention_field = retention_binding
                .path
                .get(1)
                .map(|part| part.text.as_str())
                .expect("checked purge retention binding has a field");
            let identity = identity_field_name(entity).expect("persistent entity identity");
            let identity_kind = entity
                .fields
                .iter()
                .find(|field| field.name.text == identity)
                .map(|field| self.representation_root_for(&field.field_type))
                .expect("identity field declaration");
            let table = sql_identifier(&snake_case(name));
            let identity_column = sql_identifier(identity);
            let timestamp_column = sql_identifier(&purge.from.text);
            let visible = self
                .lifecycle_visibility_sql(name)
                .expect("typechecked lifecycle visibility");
            let state = self
                .lifecycle_purge_state_sql(name)
                .expect("typechecked purge transition state");
            let predicate = |placeholder: &str| {
                format!(
                    "{timestamp_column} IS NOT NULL AND {timestamp_column} <= {placeholder} AND NOT ({visible}) AND ({state})"
                )
            };
            let select_postgres = format!(
                "SELECT {identity_column}, {timestamp_column} FROM {table} WHERE {} ORDER BY {timestamp_column} ASC, {identity_column} ASC LIMIT 500 FOR UPDATE",
                predicate("$1")
            );
            let select_sqlite = format!(
                "SELECT {identity_column}, {timestamp_column} FROM {table} WHERE {} ORDER BY {timestamp_column} ASC, {identity_column} ASC LIMIT 500",
                predicate("?1")
            );
            let delete_postgres = format!(
                "DELETE FROM {table} WHERE {identity_column} = $2 AND ({}) RETURNING {identity_column}",
                predicate("$1")
            );
            let delete_sqlite = format!(
                "DELETE FROM {table} WHERE {identity_column} = ?2 AND ({}) RETURNING {identity_column}",
                predicate("?1")
            );
            line(
                &mut output,
                &format!("  async retention_purge_{name}(onSelected: (count: number, rowIdentityTokens: readonly string[]) => void): Promise<{{ committedCount: number; rowIdentityTokens: readonly string[] }}> {{"),
            );
            line(
                &mut output,
                &format!("    if (!transactional) return client.transaction((transaction: any) => transaction.retention_purge_{name}(onSelected));"),
            );
            line(
                &mut output,
                "    if (operationTime === null) throw new PersistenceFault(\"retention.operation_time\", \"data\", new Error(\"retention purge requires a captured attempt instant\"));",
            );
            line(
                &mut output,
                &format!("    const retentionMilliseconds = lifecycleRetentionMilliseconds?.[{}]; if (retentionMilliseconds === undefined) throw new PersistenceFault(\"retention.configuration\", \"data\", new Error(\"lifecycle retention is not initialized: config.{retention_field}\"));", ts_string(name)),
            );
            line(
                &mut output,
                "    const attemptMilliseconds = Date.parse(operationTime); const eligibleBefore = Math.min(attemptMilliseconds, Date.now()) - retentionMilliseconds;",
            );
            line(
                &mut output,
                &format!("    const cutoffDate = new Date(eligibleBefore); if (!Number.isSafeInteger(eligibleBefore) || Number.isNaN(cutoffDate.getTime()) || cutoffDate.getUTCFullYear() < 1 || cutoffDate.getUTCFullYear() > 9999) throw new PersistenceFault(\"retention.configuration\", \"data\", new Error(\"configured retention cutoff is outside the portable Instant range: config.{retention_field}\"));"),
            );
            line(
                &mut output,
                "    const canonicalCutoff = cutoffDate.toISOString();",
            );
            line(
                &mut output,
                "    const postgresCutoff = persistenceValue(canonicalCutoff, \"Instant\", true); const sqliteCutoff = persistenceValue(canonicalCutoff, \"Instant\", false);",
            );
            line(
                &mut output,
                &format!("    const candidates = (postgres !== null ? await persistenceAsync(\"retention.purge.{name}.select\", () => postgres!.unsafe({}, [postgresCutoff])) : persistenceSync(\"retention.purge.{name}.select\", () => sqlite!.prepare({}).all(sqliteCutoff))) as Array<Record<string, unknown>>;", ts_string(&select_postgres), ts_string(&select_sqlite)),
            );
            line(
                &mut output,
                &format!("    const rowIdentityTokens = await Promise.all(candidates.map(async row => {{ const digest = await crypto.subtle.digest(\"SHA-256\", new TextEncoder().encode({} + \":\" + String(row[{}]))); return [...new Uint8Array(digest).slice(0, 8)].map(value => value.toString(16).padStart(2, \"0\")).join(\"\"); }}));", ts_string(name), ts_string(&snake_case(identity))),
            );
            line(
                &mut output,
                "    onSelected(candidates.length, rowIdentityTokens);",
            );
            line(&mut output, "    let committedCount = 0;");
            line(&mut output, "    for (const row of candidates) {");
            line(
                &mut output,
                &format!("      const deleted = postgres !== null ? await persistenceAsync(\"retention.purge.{name}.delete\", () => postgres!.unsafe({}, [postgresCutoff, persistenceValue(row[{}], {}, true)])) : persistenceSync(\"retention.purge.{name}.delete\", () => sqlite!.prepare({}).all(sqliteCutoff, persistenceValue(row[{}], {}, false)));", ts_string(&delete_postgres), ts_string(&snake_case(identity)), ts_string(&identity_kind), ts_string(&delete_sqlite), ts_string(&snake_case(identity)), ts_string(&identity_kind)),
            );
            line(
                &mut output,
                "      if (deleted.length === 1) committedCount++;",
            );
            line(&mut output, "    }");
            line(
                &mut output,
                "    return { committedCount, rowIdentityTokens };",
            );
            line(&mut output, "  },");
        }
        for (name, entity) in self.entities() {
            let lifecycle_initial = self.lifecycle_initial_values(name);
            let schedule_hook = self.delivery_create_hook(name);
            let postgres_values = entity
                .fields
                .iter()
                .map(|field| {
                    format!(
                        "${{persistenceValue(createValue[{}], {}, true)}}",
                        ts_string(&field.name.text),
                        ts_string(&self.representation_root_for(&field.field_type))
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let table = sql_identifier(&snake_case(name));
            let lifecycle_write_guard = self
                .lifecycle_visibility_sql(name)
                .map(|predicate| format!(" AND ({predicate})"))
                .unwrap_or_default();
            let lifecycle_owned_fields = self.lifecycle_owned_field_names(name);
            let fields = entity
                .fields
                .iter()
                .map(|field| sql_identifier(&field.name.text))
                .collect::<Vec<_>>()
                .join(", ");
            let sqlite_sql = self.insert_sql(name, entity, SqlDialect::Sqlite);
            let sqlite_values = entity
                .fields
                .iter()
                .map(|field| {
                    format!(
                        "persistenceValue(createValue[{}], {}, false)",
                        ts_string(&field.name.text),
                        ts_string(&self.representation_root_for(&field.field_type))
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            line(
                &mut output,
                &format!(
                    "  async create_{name}(value: Record<string, unknown>): Promise<unknown> {{"
                ),
            );
            line(
                &mut output,
                &format!(
                    "    if (!transactional) return this.transaction((transaction: any) => transaction.create_{name}(value));"
                ),
            );
            if schedule_hook.is_some() {
                line(&mut output, "    return client.transaction(async () => {");
                line(&mut output, "      const changed = await (async () => {");
            }
            line(
                &mut output,
                "    if (value === null || typeof value !== \"object\" || Array.isArray(value)) throw new PersistenceFault(\"create.input\", \"data\", new Error(\"create requires a record\"));",
            );
            line(
                &mut output,
                "    const createValue: Record<string, unknown> = { ...value };",
            );
            for (index, (field, expression)) in lifecycle_initial.iter().enumerate() {
                let local = format!("lifecycleInitial{index}");
                line(&mut output, &format!("    const {local} = {expression};"));
                line(
                    &mut output,
                    &format!("    if (Object.prototype.hasOwnProperty.call(value, {}) && !Object.is(value[{}], {local})) throw new PersistenceFault(\"create.lifecycle.initial\", \"data\", new Error(\"caller cannot set lifecycle-owned initial state\"));", ts_string(field), ts_string(field)),
                );
                line(
                    &mut output,
                    &format!("    createValue[{}] = {local};", ts_string(field)),
                );
            }
            line(
                &mut output,
                &format!(
                    "    assertTransactionActive(); if (!(await policyAllowsWrite({}, \"create\", createValue, Object.keys(createValue), policy, postgres, sqlite))) throw new PolicyFault(policy.operation);",
                    ts_string(name)
                ),
            );
            line(&mut output, "    if (postgres !== null) {");
            line(
                &mut output,
                &format!(
                    "      const rows = await persistenceAsync(\"create.{name}\", () => postgres`INSERT INTO {table} ({fields}) VALUES ({postgres_values}) RETURNING {fields}`);"
                ),
            );
            line(
                &mut output,
                "      if (rows.length !== 1) throw new PersistenceFault(\"create.cardinality\", \"cardinality\", new Error(\"database create returned an unexpected row count\"));",
            );
            if self.entity_has_derived_representations(name) {
                line(
                    &mut output,
                    &format!(
                        "      await recordAuthorityChange({}, \"create\", rows[0] as Record<string, unknown>, operationTime, postgres, sqlite);",
                        ts_string(name)
                    ),
                );
            }
            line(&mut output, "      return rows[0];");
            line(&mut output, "    }");
            line(
                &mut output,
                &format!(
                    "    const row = persistenceSync(\"create.{name}\", () => sqlite!.prepare({}).get({sqlite_values}));",
                    ts_string(&sqlite_sql)
                ),
            );
            if self.entity_has_derived_representations(name) {
                line(
                    &mut output,
                    &format!(
                        "    recordAuthorityChange({}, \"create\", row as Record<string, unknown>, operationTime, null, sqlite);",
                        ts_string(name)
                    ),
                );
            }
            line(&mut output, "    return row;");
            if let Some(hook) = schedule_hook {
                line(&mut output, "      })();");
                line(&mut output, &format!("      await client.establish_delivery_schedule_revision({}, String((changed as Record<string, unknown>)[{}]));", ts_string(name), ts_string(&snake_case(hook))));
                line(&mut output, "      return changed;");
                line(&mut output, "    });");
            }
            line(&mut output, "  },");
            for field in &entity.fields {
                let field_name = &field.name.text;
                if let Some(updated_at) = generated_change_field(entity) {
                    let updated_column = sql_identifier(&updated_at.name.text);
                    let predicate_column = sql_identifier(field_name);
                    let returned = entity
                        .fields
                        .iter()
                        .map(|field| sql_identifier(&field.name.text))
                        .collect::<Vec<_>>()
                        .join(", ");
                    line(
                        &mut output,
                        &format!("  async touch_{name}_by_{field_name}(value: unknown, operationTime: unknown): Promise<unknown | null> {{"),
                    );
                    line(&mut output, "    const rows = postgres !== null");
                    line(
                        &mut output,
                        &format!("      ? await persistenceAsync(\"touch.{name}.{field_name}\", () => postgres`UPDATE {table} SET {updated_column} = ${{persistenceValue(operationTime, \"Instant\", true)}} WHERE {predicate_column} = ${{persistenceValue(value, {}, true)}}{lifecycle_write_guard} RETURNING {returned}`)", ts_string(&self.representation_root_for(&field.field_type))),
                    );
                    line(
                        &mut output,
                        &format!("      : persistenceSync(\"touch.{name}.{field_name}\", () => sqlite!.prepare({}).all(persistenceValue(operationTime, \"Instant\", false), persistenceValue(value, {}, false)));", ts_string(&format!("UPDATE {table} SET {updated_column} = ?1 WHERE {predicate_column} = ?2{lifecycle_write_guard} RETURNING {returned}")), ts_string(&self.representation_root_for(&field.field_type))),
                    );
                    line(&mut output, "    if (rows.length > 1) throw new PersistenceFault(\"touch.cardinality\", \"cardinality\", new Error(\"generated timestamp update returned too many rows\"));");
                    if self.entity_has_derived_representations(name) {
                        line(
                            &mut output,
                            &format!(
                                "    if (rows[0]) await recordAuthorityChange({}, \"update\", rows[0] as Record<string, unknown>, operationTime, postgres, sqlite);",
                                ts_string(name)
                            ),
                        );
                    }
                    line(&mut output, "    return rows[0] ?? null;");
                    line(&mut output, "  },");
                }
                let postgres_sql =
                    self.select_optional_sql(name, entity, field_name, SqlDialect::Postgres);
                let sqlite_sql =
                    self.select_optional_sql(name, entity, field_name, SqlDialect::Sqlite);
                let value_kind = ts_string(&self.representation_root_for(&field.field_type));
                line(
                    &mut output,
                    &format!(
                        "  async query_optional_{name}_by_{field_name}(value: unknown): Promise<unknown | null> {{"
                    ),
                );
                line(
                    &mut output,
                    &format!(
                        "    const rows = await policyReadRows({}, {}, {}, {}, [persistenceValue(value, {value_kind}, true)], [persistenceValue(value, {value_kind}, false)]);",
                        ts_string(&format!("query.{name}.{field_name}")),
                        ts_string(name),
                        ts_string(&postgres_sql),
                        ts_string(&sqlite_sql)
                    ),
                );
                line(
                    &mut output,
                    &format!(
                        "    const visible = await policyFilterRows({}, rows, policy, postgres, sqlite);",
                        ts_string(name)
                    ),
                );
                line(
                    &mut output,
                    "    if (visible.length > 1) throw new PersistenceFault(\"query.cardinality\", \"cardinality\", new Error(\"optional query returned more than one authorised row\"));",
                );
                line(&mut output, "    return visible[0] ?? null;");
                line(&mut output, "  },");
                line(
                    &mut output,
                    &format!(
                        "  async query_required_{name}_by_{field_name}(value: unknown): Promise<unknown | null> {{"
                    ),
                );
                line(
                    &mut output,
                    &format!("    return this.query_optional_{name}_by_{field_name}(value);"),
                );
                line(&mut output, "  },");
                for order in &entity.fields {
                    for (direction, sql_direction) in [("asc", "ASC"), ("desc", "DESC")] {
                        let postgres_sql = self.select_many_sql(
                            name,
                            entity,
                            field_name,
                            &order.name.text,
                            sql_direction,
                            SqlDialect::Postgres,
                        );
                        let sqlite_sql = self.select_many_sql(
                            name,
                            entity,
                            field_name,
                            &order.name.text,
                            sql_direction,
                            SqlDialect::Sqlite,
                        );
                        line(
                            &mut output,
                            &format!(
                                "  async query_many_{name}_by_{field_name}_order_by_{}_{direction}(value: unknown): Promise<unknown[]> {{",
                                order.name.text
                            ),
                        );
                        line(
                            &mut output,
                            &format!(
                                "    const rows = await policyReadRows({}, {}, {}, {}, [persistenceValue(value, {value_kind}, true)], [persistenceValue(value, {value_kind}, false)]);",
                                ts_string(&format!("query.{name}.{field_name}")),
                                ts_string(name),
                                ts_string(&postgres_sql),
                                ts_string(&sqlite_sql)
                            ),
                        );
                        line(
                            &mut output,
                            &format!(
                                "    return policyFilterRows({}, rows, policy, postgres, sqlite);",
                                ts_string(name)
                            ),
                        );
                        line(&mut output, "  },");

                        let postgres_sql = self.select_many_paginated_sql(
                            name,
                            entity,
                            field_name,
                            &order.name.text,
                            sql_direction,
                            SqlDialect::Postgres,
                        );
                        let sqlite_sql = self.select_many_paginated_sql(
                            name,
                            entity,
                            field_name,
                            &order.name.text,
                            sql_direction,
                            SqlDialect::Sqlite,
                        );
                        line(
                            &mut output,
                            &format!(
                                "  async query_many_{name}_by_{field_name}_order_by_{}_{direction}_paginated(value: unknown, limit: unknown, offset: unknown): Promise<unknown[]> {{",
                                order.name.text
                            ),
                        );
                        line(
                            &mut output,
                            &format!(
                                "    const rows = await policyReadRows({}, {}, {}, {}, [persistenceValue(value, {value_kind}, true), limit, offset], [persistenceValue(value, {value_kind}, false), limit, offset]);",
                                ts_string(&format!("query.{name}.{field_name}")),
                                ts_string(name),
                                ts_string(&postgres_sql),
                                ts_string(&sqlite_sql)
                            ),
                        );
                        line(
                            &mut output,
                            &format!(
                                "    return policyFilterRows({}, rows, policy, postgres, sqlite);",
                                ts_string(name)
                            ),
                        );
                        line(&mut output, "  },");
                    }
                }
            }
            for predicate in &entity.fields {
                let predicate_name = &predicate.name.text;
                let predicate_kind =
                    ts_string(&self.representation_root_for(&predicate.field_type));
                let postgres_policy_select =
                    self.select_optional_sql(name, entity, predicate_name, SqlDialect::Postgres);
                let sqlite_select =
                    self.select_optional_sql(name, entity, predicate_name, SqlDialect::Sqlite);
                for change in &entity.fields {
                    let change_name = &change.name.text;
                    if change.generated.is_some() || lifecycle_owned_fields.contains(change_name) {
                        continue;
                    }
                    let change_kind = ts_string(&self.representation_root_for(&change.field_type));
                    let ignored_semantic_field = generated_change_field(entity)
                        .map(|field| ts_string(&field.name.text))
                        .unwrap_or_else(|| "null".to_owned());
                    let sqlite_update = format!(
                        "UPDATE {table} SET {} = ?1 WHERE {} = ?2{lifecycle_write_guard} RETURNING {fields}",
                        sql_identifier(change_name),
                        sql_identifier(predicate_name)
                    );
                    let postgres_scoped_update = format!(
                        "UPDATE {table} SET {} = $1 WHERE {} = $2{lifecycle_write_guard} RETURNING {fields}",
                        sql_identifier(change_name),
                        sql_identifier(predicate_name)
                    );
                    let operation = format!("update.{name}.{predicate_name}.{change_name}");
                    line(
                        &mut output,
                        &format!(
                            "  async update_required_{name}_by_{predicate_name}_set_{change_name}(predicateValue: unknown, replacement: unknown): Promise<unknown | null> {{"
                        ),
                    );
                    line(&mut output, "    if (postgres !== null) {");
                    line(&mut output, "      const execute = async (tx: SQL) => {");
                    line(
                        &mut output,
                        &format!(
                            "        const matches = await policyPostgresMutationRows(tx, {}, \"update\", [{}], {}, [persistenceValue(predicateValue, {predicate_kind}, true)]);",
                            ts_string(name),
                            ts_string(change_name),
                            ts_string(&format!("{postgres_policy_select} FOR UPDATE"))
                        ),
                    );
                    line(&mut output, "        if (matches.length > 1) throw new PersistenceFault(\"update.cardinality\", \"cardinality\", new Error(\"required update matched more than one row\"));");
                    line(
                        &mut output,
                        "        if (matches.length === 0) return null;",
                    );
                    line(
                        &mut output,
                        &format!(
                            "        if (!(await policyAllowsWrite({}, \"update\", matches[0] as Record<string, unknown>, [{}], policy, tx, null))) return null;",
                            ts_string(name),
                            ts_string(change_name)
                        ),
                    );
                    line(
                        &mut output,
                        &format!(
                            "        const rows = await policyPostgresMutationRows(tx, {}, \"update\", [{}], {}, [persistenceValue(replacement, {change_kind}, true), persistenceValue(predicateValue, {predicate_kind}, true)]);",
                            ts_string(name),
                            ts_string(change_name),
                            ts_string(&postgres_scoped_update)
                        ),
                    );
                    line(&mut output, "        if (rows.length !== 1) throw new PersistenceFault(\"update.cardinality\", \"cardinality\", new Error(\"required update changed an unexpected row count\"));");
                    line(
                        &mut output,
                        &format!("        recordSemanticChange(matches[0], rows[0], {ignored_semantic_field});"),
                    );
                    if self.entity_has_derived_representations(name)
                        && generated_change_field(entity).is_none()
                    {
                        line(
                            &mut output,
                            &format!(
                                "        await recordAuthorityChange({}, \"update\", rows[0] as Record<string, unknown>, operationTime, tx, sqlite);",
                                ts_string(name)
                            ),
                        );
                    }
                    line(&mut output, "        return rows[0];");
                    line(&mut output, "      };");
                    line(
                        &mut output,
                        &format!(
                            "      return persistenceAsync({}, () => transactional ? execute(postgres) : postgres.begin(tx => execute(tx as SQL)));",
                            ts_string(&operation)
                        ),
                    );
                    line(&mut output, "    }");
                    line(
                        &mut output,
                        "    const execute = (predicateValue: unknown, replacement: unknown) => {",
                    );
                    line(
                        &mut output,
                        &format!(
                            "      const matches = policySqliteMutationRows({}, \"update\", [{}], {}, [persistenceValue(predicateValue, {predicate_kind}, false)]);",
                            ts_string(name),
                            ts_string(change_name),
                            ts_string(&sqlite_select)
                        ),
                    );
                    line(&mut output, "      if (matches.length > 1) throw new PersistenceFault(\"update.cardinality\", \"cardinality\", new Error(\"required update matched more than one row\"));");
                    line(&mut output, "      if (matches.length === 0) return null;");
                    line(
                        &mut output,
                        &format!(
                            "      if (!policyAllowsWriteSqlite({}, \"update\", matches[0] as Record<string, unknown>, [{}], policy, sqlite!)) return null;",
                            ts_string(name),
                            ts_string(change_name)
                        ),
                    );
                    line(
                        &mut output,
                        &format!("      const rows = policySqliteMutationRows({}, \"update\", [{}], {}, [persistenceValue(replacement, {change_kind}, false), persistenceValue(predicateValue, {predicate_kind}, false)]);", ts_string(name), ts_string(change_name), ts_string(&sqlite_update)),
                    );
                    line(&mut output, "      if (rows.length !== 1) throw new PersistenceFault(\"update.cardinality\", \"cardinality\", new Error(\"required update changed an unexpected row count\"));");
                    line(
                        &mut output,
                        &format!("      recordSemanticChange(matches[0], rows[0], {ignored_semantic_field});"),
                    );
                    if self.entity_has_derived_representations(name)
                        && generated_change_field(entity).is_none()
                    {
                        line(
                            &mut output,
                            &format!(
                                "      recordAuthorityChange({}, \"update\", rows[0] as Record<string, unknown>, operationTime, null, sqlite);",
                                ts_string(name)
                            ),
                        );
                    }
                    line(&mut output, "      return rows[0];");
                    line(&mut output, "    };");
                    line(&mut output, "    if (transactional) return persistenceSync(\"update.transactional\", () => execute(predicateValue, replacement));");
                    line(
                        &mut output,
                        "    const mutate = sqlite!.transaction(execute);",
                    );
                    line(
                        &mut output,
                        &format!("    return persistenceSync({}, () => mutate(predicateValue, replacement));", ts_string(&operation)),
                    );
                    line(&mut output, "  },");
                }
                if !lifecycle_write_guard.is_empty() {
                    continue;
                }
                let sqlite_delete = format!(
                    "DELETE FROM {table} WHERE {} = ?1 RETURNING {fields}",
                    sql_identifier(predicate_name)
                );
                let postgres_scoped_delete = format!(
                    "DELETE FROM {table} WHERE {} = $1 RETURNING {fields}",
                    sql_identifier(predicate_name)
                );
                let operation = format!("delete.{name}.{predicate_name}");
                line(
                    &mut output,
                    &format!(
                        "  async delete_required_{name}_by_{predicate_name}(predicateValue: unknown): Promise<unknown | null> {{"
                    ),
                );
                line(&mut output, "    if (postgres !== null) {");
                line(&mut output, "      const execute = async (tx: SQL) => {");
                line(
                    &mut output,
                    &format!(
                        "        const matches = await policyPostgresMutationRows(tx, {}, \"delete\", [], {}, [persistenceValue(predicateValue, {predicate_kind}, true)]);",
                        ts_string(name),
                        ts_string(&format!("{postgres_policy_select} FOR UPDATE"))
                    ),
                );
                line(&mut output, "        if (matches.length > 1) throw new PersistenceFault(\"delete.cardinality\", \"cardinality\", new Error(\"required delete matched more than one row\"));");
                line(
                    &mut output,
                    "        if (matches.length === 0) return null;",
                );
                line(
                    &mut output,
                    &format!(
                        "        if (!(await policyAllows({}, \"delete\", matches[0] as Record<string, unknown>, policy, tx, null))) return null;",
                        ts_string(name)
                    ),
                );
                line(
                    &mut output,
                    &format!(
                        "        const rows = await policyPostgresMutationRows(tx, {}, \"delete\", [], {}, [persistenceValue(predicateValue, {predicate_kind}, true)]);",
                        ts_string(name),
                        ts_string(&postgres_scoped_delete)
                    ),
                );
                line(&mut output, "        if (rows.length !== 1) throw new PersistenceFault(\"delete.cardinality\", \"cardinality\", new Error(\"required delete changed an unexpected row count\"));");
                if self.entity_has_derived_representations(name) {
                    line(
                        &mut output,
                        &format!(
                            "        await recordAuthorityChange({}, \"delete\", rows[0] as Record<string, unknown>, operationTime, tx, sqlite);",
                            ts_string(name)
                        ),
                    );
                }
                line(&mut output, "        return rows[0];");
                line(&mut output, "      };");
                line(
                    &mut output,
                    &format!(
                        "      return persistenceAsync({}, () => transactional ? execute(postgres) : postgres.begin(tx => execute(tx as SQL)));",
                        ts_string(&operation)
                    ),
                );
                line(&mut output, "    }");
                line(
                    &mut output,
                    "    const execute = (predicateValue: unknown) => {",
                );
                line(
                    &mut output,
                    &format!(
                        "      const matches = policySqliteMutationRows({}, \"delete\", [], {}, [persistenceValue(predicateValue, {predicate_kind}, false)]);",
                        ts_string(name),
                        ts_string(&sqlite_select)
                    ),
                );
                line(&mut output, "      if (matches.length > 1) throw new PersistenceFault(\"delete.cardinality\", \"cardinality\", new Error(\"required delete matched more than one row\"));");
                line(&mut output, "      if (matches.length === 0) return null;");
                line(
                    &mut output,
                    &format!(
                        "      if (!policyAllowsSqlite({}, \"delete\", matches[0] as Record<string, unknown>, policy, sqlite!)) return null;",
                        ts_string(name)
                    ),
                );
                line(
                    &mut output,
                    &format!(
                        "      const rows = policySqliteMutationRows({}, \"delete\", [], {}, [persistenceValue(predicateValue, {predicate_kind}, false)]);",
                        ts_string(name),
                        ts_string(&sqlite_delete)
                    ),
                );
                line(&mut output, "      if (rows.length !== 1) throw new PersistenceFault(\"delete.cardinality\", \"cardinality\", new Error(\"required delete changed an unexpected row count\"));");
                if self.entity_has_derived_representations(name) {
                    line(
                        &mut output,
                        &format!(
                            "      recordAuthorityChange({}, \"delete\", rows[0] as Record<string, unknown>, operationTime, null, sqlite);",
                            ts_string(name)
                        ),
                    );
                }
                line(&mut output, "      return rows[0];");
                line(&mut output, "    };");
                line(&mut output, "    if (transactional) return persistenceSync(\"delete.transactional\", () => execute(predicateValue));");
                line(
                    &mut output,
                    "    const mutate = sqlite!.transaction(execute);",
                );
                line(
                    &mut output,
                    &format!(
                        "    return persistenceSync({}, () => mutate(predicateValue));",
                        ts_string(&operation)
                    ),
                );
                line(&mut output, "  },");
            }
        }
        let mut emitted_patch_updates = BTreeSet::new();
        for callable in self.callables.values() {
            let mut updates = Vec::new();
            collect_update_expressions(&callable.body, &mut updates);
            for update in updates.into_iter().filter(|update| update.patch.is_some()) {
                let name = update
                    .target
                    .path
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                let patch_fields = self
                    .patch_fields_for_update(update)
                    .expect("checked patch input resolves to a record");
                let suffix = patch_method_suffix(update, &patch_fields);
                let method = format!("update_required_{name}_by_{}_{suffix}", update.field.text);
                let schedule_hook = self.delivery_patch_hook(callable, &name);
                if !emitted_patch_updates.insert(method.clone()) {
                    continue;
                }
                let entity = self
                    .records
                    .get(&name)
                    .expect("checked patch target entity exists");
                let table = sql_identifier(&snake_case(&name));
                let lifecycle_write_guard = self
                    .lifecycle_visibility_sql(&name)
                    .map(|predicate| format!(" AND ({predicate})"))
                    .unwrap_or_default();
                let returned_fields = entity
                    .fields
                    .iter()
                    .map(|field| sql_identifier(&field.name.text))
                    .collect::<Vec<_>>()
                    .join(", ");
                let predicate_field = entity
                    .fields
                    .iter()
                    .find(|field| field.name.text == update.field.text)
                    .expect("checked update predicate field exists");
                let predicate_kind =
                    ts_string(&self.representation_root_for(&predicate_field.field_type));
                let ignored_semantic_field = generated_change_field(entity)
                    .map(|field| ts_string(&field.name.text))
                    .unwrap_or_else(|| "null".to_owned());
                let postgres_policy_select = self.select_optional_sql(
                    &name,
                    entity,
                    &update.field.text,
                    SqlDialect::Postgres,
                );
                let sqlite_select =
                    self.select_optional_sql(&name, entity, &update.field.text, SqlDialect::Sqlite);
                // Patch input fields may be owning references (Todo.due_at),
                // not direct primitive aliases. Encode against the checked
                // destination field so Instant values use the adapter's actual
                // representation instead of reaching SQLite as raw text.
                let patch_encoding_kind = |field: &FieldDeclaration| {
                    let destination = entity
                        .fields
                        .iter()
                        .find(|candidate| candidate.name.text == field.name.text)
                        .expect("checked patch field belongs to destination entity");
                    ts_string(&self.representation_root_for(&destination.field_type))
                };
                let mut postgres_set = patch_fields
                    .iter()
                    .map(|field| {
                        let column = sql_identifier(&field.name.text);
                        let key = ts_string(&field.name.text);
                        let kind = patch_encoding_kind(field);
                        format!(
                            "{column} = CASE WHEN ${{hasOwn(patchValue, {key})}} THEN ${{persistenceValue(hasOwn(patchValue, {key}) ? patchValue[{key}] : null, {kind}, true)}} ELSE {column} END"
                        )
                    })
                    .collect::<Vec<_>>();
                let mut sqlite_set = patch_fields
                    .iter()
                    .enumerate()
                    .map(|(index, field)| {
                        let supplied = index * 2 + 1;
                        let value = supplied + 1;
                        let column = sql_identifier(&field.name.text);
                        format!("{column} = CASE WHEN ?{supplied} THEN ?{value} ELSE {column} END")
                    })
                    .collect::<Vec<_>>();
                let derived = patch_derived_changes(update);
                let replacements = (0..derived.len())
                    .map(|index| format!("replacement{index}: unknown"))
                    .collect::<Vec<_>>();
                let replacement_names = (0..derived.len())
                    .map(|index| format!("replacement{index}"))
                    .collect::<Vec<_>>();
                for (index, (change, supplied)) in derived.iter().enumerate() {
                    let column = sql_identifier(&change.name.text);
                    let change_field = entity
                        .fields
                        .iter()
                        .find(|field| field.name.text == change.name.text)
                        .expect("checked derived update field exists");
                    let change_kind =
                        ts_string(&self.representation_root_for(&change_field.field_type));
                    if let Some(supplied) = supplied {
                        let key = ts_string(&supplied.path[1].text);
                        postgres_set.push(format!(
                            "{column} = CASE WHEN ${{hasOwn(patchValue, {key})}} THEN ${{persistenceValue(replacement{index}, {change_kind}, true)}} ELSE {column} END"
                        ));
                        let supplied_index = patch_fields
                            .iter()
                            .position(|field| field.name.text == supplied.path[1].text)
                            .expect("checked supplied patch field exists")
                            * 2
                            + 1;
                        let value_index = patch_fields.len() * 2 + index + 1;
                        sqlite_set.push(format!(
                            "{column} = CASE WHEN ?{supplied_index} THEN ?{value_index} ELSE {column} END"
                        ));
                    } else {
                        postgres_set.push(format!("{column} = ${{persistenceValue(replacement{index}, {change_kind}, true)}}"));
                        let value_index = patch_fields.len() * 2 + index + 1;
                        sqlite_set.push(format!("{column} = ?{value_index}"));
                    }
                }
                let _postgres_set = postgres_set.join(", ");
                let sqlite_set = sqlite_set.join(", ");
                let sqlite_predicate = patch_fields.len() * 2 + derived.len() + 1;
                let sqlite_update = format!(
                    "UPDATE {table} SET {sqlite_set} WHERE {} = ?{sqlite_predicate}{lifecycle_write_guard} RETURNING {returned_fields}",
                    sql_identifier(&update.field.text)
                );
                let postgres_scoped_set = sqlite_set.replace('?', "$");
                let postgres_scoped_update = format!(
                    "UPDATE {table} SET {postgres_scoped_set} WHERE {} = ${sqlite_predicate}{lifecycle_write_guard} RETURNING {returned_fields}",
                    sql_identifier(&update.field.text)
                );
                let mut patch_arguments = patch_fields
                    .iter()
                    .flat_map(|field| {
                        let key = ts_string(&field.name.text);
                        let kind = patch_encoding_kind(field);
                        [
                            format!("hasOwn(patchValue, {key})"),
                            format!("persistenceValue(hasOwn(patchValue, {key}) ? patchValue[{key}] : null, {kind}, false)"),
                        ]
                    })
                    .collect::<Vec<_>>();
                patch_arguments.extend(derived.iter().enumerate().map(|(index, (change, _))| {
                    let field = entity
                        .fields
                        .iter()
                        .find(|field| field.name.text == change.name.text)
                        .expect("checked derived update field exists");
                    let kind = ts_string(&self.representation_root_for(&field.field_type));
                    format!("persistenceValue(replacement{index}, {kind}, false)")
                }));
                let patch_arguments = patch_arguments.join(", ");
                let mut patch_postgres_arguments = patch_fields
                    .iter()
                    .flat_map(|field| {
                        let key = ts_string(&field.name.text);
                        let kind = patch_encoding_kind(field);
                        [
                            format!("hasOwn(patchValue, {key})"),
                            format!("persistenceValue(hasOwn(patchValue, {key}) ? patchValue[{key}] : null, {kind}, true)"),
                        ]
                    })
                    .collect::<Vec<_>>();
                patch_postgres_arguments.extend(derived.iter().enumerate().map(
                    |(index, (change, _))| {
                        let field = entity
                            .fields
                            .iter()
                            .find(|field| field.name.text == change.name.text)
                            .expect("checked derived update field exists");
                        let kind = ts_string(&self.representation_root_for(&field.field_type));
                        format!("persistenceValue(replacement{index}, {kind}, true)")
                    },
                ));
                let patch_postgres_arguments = patch_postgres_arguments.join(", ");
                let signature_suffix = if replacements.is_empty() {
                    String::new()
                } else {
                    format!(", {}", replacements.join(", "))
                };
                let call_suffix = if replacement_names.is_empty() {
                    String::new()
                } else {
                    format!(", {}", replacement_names.join(", "))
                };
                let policy_fields = patch_fields
                    .iter()
                    .map(|field| {
                        let field = ts_string(&field.name.text);
                        format!("...(hasOwn(patchValue, {field}) ? [{field}] : [])")
                    })
                    .chain(derived.iter().map(|(change, supplied)| {
                        let changed = ts_string(&change.name.text);
                        supplied.as_ref().map_or_else(
                            || changed.clone(),
                            |supplied| {
                                let supplied = ts_string(&supplied.path[1].text);
                                format!("...(hasOwn(patchValue, {supplied}) ? [{changed}] : [])")
                            },
                        )
                    }))
                    .collect::<Vec<_>>()
                    .join(", ");
                let operation = format!("update.{name}.{}.patch", update.field.text);
                line(
                    &mut output,
                    &format!(
                        "  async {method}(predicateValue: unknown, patchValue: Record<string, unknown>{signature_suffix}): Promise<unknown | null> {{"
                    ),
                );
                if schedule_hook.is_some() {
                    line(&mut output, &format!("    if (!transactional) return client.transaction((transaction: any) => transaction.{method}(predicateValue, patchValue{call_suffix}));"));
                    line(&mut output, "    return client.transaction(async () => {");
                    line(&mut output, "      const changed = await (async () => {");
                }
                line(&mut output, "    if (postgres !== null) {");
                line(&mut output, "      const execute = async (tx: SQL) => {");
                line(
                    &mut output,
                    &format!(
                        "        const matches = await policyPostgresMutationRows(tx, {}, \"update\", [{policy_fields}], {}, [persistenceValue(predicateValue, {predicate_kind}, true)]);",
                        ts_string(&name),
                        ts_string(&format!("{postgres_policy_select} FOR UPDATE"))
                    ),
                );
                line(&mut output, "        if (matches.length > 1) throw new PersistenceFault(\"update.cardinality\", \"cardinality\", new Error(\"required patch update matched more than one row\"));");
                line(
                    &mut output,
                    "        if (matches.length === 0) return null;",
                );
                line(
                    &mut output,
                    &format!(
                        "        if (!(await policyAllowsWrite({}, \"update\", matches[0] as Record<string, unknown>, [{policy_fields}], policy, tx, null))) return null;",
                        ts_string(&name)
                    ),
                );
                line(
                    &mut output,
                    &format!(
                        "        const rows = await policyPostgresMutationRows(tx, {}, \"update\", [{policy_fields}], {}, [{patch_postgres_arguments}, persistenceValue(predicateValue, {predicate_kind}, true)]);",
                        ts_string(&name),
                        ts_string(&postgres_scoped_update)
                    ),
                );
                line(&mut output, "        if (rows.length !== 1) throw new PersistenceFault(\"update.cardinality\", \"cardinality\", new Error(\"required patch update changed an unexpected row count\"));");
                line(
                    &mut output,
                    &format!("        recordSemanticChange(matches[0], rows[0], {ignored_semantic_field});"),
                );
                if self.entity_has_derived_representations(&name)
                    && generated_change_field(entity).is_none()
                {
                    line(
                        &mut output,
                        &format!(
                            "        await recordAuthorityChange({}, \"update\", rows[0] as Record<string, unknown>, operationTime, tx, sqlite);",
                            ts_string(&name)
                        ),
                    );
                }
                line(&mut output, "        return rows[0];");
                line(&mut output, "      };");
                line(
                    &mut output,
                    &format!(
                        "      return persistenceAsync({}, () => transactional ? execute(postgres) : postgres.begin(tx => execute(tx as SQL)));",
                        ts_string(&operation)
                    ),
                );
                line(&mut output, "    }");
                line(
                    &mut output,
                    &format!("    const execute = (predicateValue: unknown, patchValue: Record<string, unknown>{signature_suffix}) => {{"),
                );
                line(
                    &mut output,
                    &format!(
                        "      const matches = policySqliteMutationRows({}, \"update\", [{policy_fields}], {}, [persistenceValue(predicateValue, {predicate_kind}, false)]);",
                        ts_string(&name),
                        ts_string(&sqlite_select)
                    ),
                );
                line(&mut output, "      if (matches.length > 1) throw new PersistenceFault(\"update.cardinality\", \"cardinality\", new Error(\"required patch update matched more than one row\"));");
                line(&mut output, "      if (matches.length === 0) return null;");
                line(
                    &mut output,
                    &format!(
                        "      if (!policyAllowsWriteSqlite({}, \"update\", matches[0] as Record<string, unknown>, [{policy_fields}], policy, sqlite!)) return null;",
                        ts_string(&name)
                    ),
                );
                line(
                    &mut output,
                    &format!(
                        "      const rows = policySqliteMutationRows({}, \"update\", [{policy_fields}], {}, [{patch_arguments}, persistenceValue(predicateValue, {predicate_kind}, false)]);",
                        ts_string(&name),
                        ts_string(&sqlite_update)
                    ),
                );
                line(&mut output, "      if (rows.length !== 1) throw new PersistenceFault(\"update.cardinality\", \"cardinality\", new Error(\"required patch update changed an unexpected row count\"));");
                line(
                    &mut output,
                    &format!("      recordSemanticChange(matches[0], rows[0], {ignored_semantic_field});"),
                );
                if self.entity_has_derived_representations(&name)
                    && generated_change_field(entity).is_none()
                {
                    line(
                        &mut output,
                        &format!(
                            "      recordAuthorityChange({}, \"update\", rows[0] as Record<string, unknown>, operationTime, null, sqlite);",
                            ts_string(&name)
                        ),
                    );
                }
                line(&mut output, "      return rows[0];");
                line(&mut output, "    };");
                line(
                    &mut output,
                    &format!("    if (transactional) return persistenceSync(\"update.transactional\", () => execute(predicateValue, patchValue{call_suffix}));"),
                );
                line(
                    &mut output,
                    "    const mutate = sqlite!.transaction(execute);",
                );
                line(
                    &mut output,
                    &format!(
                        "    return persistenceSync({}, () => mutate(predicateValue, patchValue{call_suffix}));",
                        ts_string(&operation)
                    ),
                );
                if let Some((identity, supplied)) = schedule_hook {
                    line(&mut output, "      })();");
                    line(&mut output, &format!("      if (changed !== null && hasOwn(patchValue, {})) await client.advance_delivery_schedule_revision({}, String((changed as Record<string, unknown>)[{}]));", ts_string(supplied), ts_string(&name), ts_string(&snake_case(identity))));
                    line(&mut output, "      return changed;");
                    line(&mut output, "    });");
                }
                line(&mut output, "  },");
            }
        }

        let mut emitted_multi_updates = BTreeSet::new();
        for callable in self.callables.values() {
            let mut updates = Vec::new();
            collect_update_expressions(&callable.body, &mut updates);
            for update in updates
                .into_iter()
                .filter(|update| update.patch.is_none() && update.changes.len() > 1)
            {
                let name = update
                    .target
                    .path
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                let change_suffix = update
                    .changes
                    .iter()
                    .map(|change| change.name.text.as_str())
                    .collect::<Vec<_>>()
                    .join("_and_");
                let method = format!(
                    "update_required_{name}_by_{}_set_{change_suffix}",
                    update.field.text
                );
                if !emitted_multi_updates.insert(method.clone()) {
                    continue;
                }
                let entity = self
                    .records
                    .get(&name)
                    .expect("checked update entity exists");
                let table = sql_identifier(&snake_case(&name));
                let lifecycle_write_guard = self
                    .lifecycle_visibility_sql(&name)
                    .map(|predicate| format!(" AND ({predicate})"))
                    .unwrap_or_default();
                let fields = entity
                    .fields
                    .iter()
                    .map(|field| sql_identifier(&field.name.text))
                    .collect::<Vec<_>>()
                    .join(", ");
                let predicate_field = entity
                    .fields
                    .iter()
                    .find(|field| field.name.text == update.field.text)
                    .expect("checked update predicate field exists");
                let predicate_kind =
                    ts_string(&self.representation_root_for(&predicate_field.field_type));
                let ignored_semantic_field = generated_change_field(entity)
                    .map(|field| ts_string(&field.name.text))
                    .unwrap_or_else(|| "null".to_owned());
                let postgres_policy_select = self.select_optional_sql(
                    &name,
                    entity,
                    &update.field.text,
                    SqlDialect::Postgres,
                );
                let sqlite_select =
                    self.select_optional_sql(&name, entity, &update.field.text, SqlDialect::Sqlite);
                let replacements = (0..update.changes.len())
                    .map(|index| format!("replacement{index}: unknown"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let replacement_names = (0..update.changes.len())
                    .map(|index| format!("replacement{index}"))
                    .collect::<Vec<_>>();
                let policy_fields = update
                    .changes
                    .iter()
                    .map(|change| ts_string(&change.name.text))
                    .collect::<Vec<_>>()
                    .join(", ");
                let postgres_set = update
                    .changes
                    .iter()
                    .enumerate()
                    .map(|(index, change)| {
                        let field = entity
                            .fields
                            .iter()
                            .find(|field| field.name.text == change.name.text)
                            .expect("checked update field exists");
                        let kind = ts_string(&self.representation_root_for(&field.field_type));
                        format!(
                            "{} = ${{persistenceValue(replacement{index}, {kind}, true)}}",
                            sql_identifier(&change.name.text)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let postgres_scoped_set = update
                    .changes
                    .iter()
                    .enumerate()
                    .map(|(index, change)| {
                        format!("{} = ${}", sql_identifier(&change.name.text), index + 1)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let sqlite_set = update
                    .changes
                    .iter()
                    .enumerate()
                    .map(|(index, change)| {
                        format!("{} = ?{}", sql_identifier(&change.name.text), index + 1)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let postgres_scoped_update = format!(
                    "UPDATE {table} SET {postgres_scoped_set} WHERE {} = ${}{lifecycle_write_guard} RETURNING {fields}",
                    sql_identifier(&update.field.text),
                    update.changes.len() + 1
                );
                let _postgres_update = format!(
                    "UPDATE {table} SET {postgres_set} WHERE {} = ${{persistenceValue(predicateValue, {predicate_kind}, true)}} RETURNING {fields}",
                    sql_identifier(&update.field.text)
                );
                let sqlite_update = format!(
                    "UPDATE {table} SET {sqlite_set} WHERE {} = ?{}{lifecycle_write_guard} RETURNING {fields}",
                    sql_identifier(&update.field.text),
                    update.changes.len() + 1
                );
                let operation = format!("update.{name}.{}.multi", update.field.text);
                line(
                    &mut output,
                    &format!(
                        "  async {method}(predicateValue: unknown, {replacements}): Promise<unknown | null> {{"
                    ),
                );
                line(&mut output, "    if (postgres !== null) {");
                line(&mut output, "      const execute = async (tx: SQL) => {");
                line(
                    &mut output,
                    &format!(
                        "        const matches = await policyPostgresMutationRows(tx, {}, \"update\", [{policy_fields}], {}, [persistenceValue(predicateValue, {predicate_kind}, true)]);",
                        ts_string(&name),
                        ts_string(&format!("{postgres_policy_select} FOR UPDATE"))
                    ),
                );
                line(&mut output, "        if (matches.length > 1) throw new PersistenceFault(\"update.cardinality\", \"cardinality\", new Error(\"required update matched more than one row\"));");
                line(
                    &mut output,
                    "        if (matches.length === 0) return null;",
                );
                line(
                    &mut output,
                    &format!(
                        "        if (!(await policyAllowsWrite({}, \"update\", matches[0] as Record<string, unknown>, [{policy_fields}], policy, tx, null))) return null;",
                        ts_string(&name)
                    ),
                );
                line(
                    &mut output,
                    &format!(
                        "        const rows = await policyPostgresMutationRows(tx, {}, \"update\", [{policy_fields}], {}, [{}, persistenceValue(predicateValue, {predicate_kind}, true)]);",
                        ts_string(&name),
                        ts_string(&postgres_scoped_update),
                        update
                            .changes
                            .iter()
                            .enumerate()
                            .map(|(index, change)| {
                                let field = entity
                                    .fields
                                    .iter()
                                    .find(|field| field.name.text == change.name.text)
                                    .expect("checked update field exists");
                                let kind = ts_string(&self.representation_root_for(&field.field_type));
                                format!("persistenceValue(replacement{index}, {kind}, true)")
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
                line(&mut output, "        if (rows.length !== 1) throw new PersistenceFault(\"update.cardinality\", \"cardinality\", new Error(\"required update changed an unexpected row count\"));");
                line(
                    &mut output,
                    &format!("        recordSemanticChange(matches[0], rows[0], {ignored_semantic_field});"),
                );
                if self.entity_has_derived_representations(&name)
                    && generated_change_field(entity).is_none()
                {
                    line(
                        &mut output,
                        &format!(
                            "        await recordAuthorityChange({}, \"update\", rows[0] as Record<string, unknown>, operationTime, tx, sqlite);",
                            ts_string(&name)
                        ),
                    );
                }
                line(&mut output, "        return rows[0];");
                line(&mut output, "      };");
                line(
                    &mut output,
                    &format!(
                        "      return persistenceAsync({}, () => transactional ? execute(postgres) : postgres.begin(tx => execute(tx as SQL)));",
                        ts_string(&operation)
                    ),
                );
                line(&mut output, "    }");
                line(
                    &mut output,
                    &format!("    const execute = (predicateValue: unknown, {replacements}) => {{"),
                );
                line(
                    &mut output,
                    &format!(
                        "      const matches = policySqliteMutationRows({}, \"update\", [{policy_fields}], {}, [persistenceValue(predicateValue, {predicate_kind}, false)]);",
                        ts_string(&name),
                        ts_string(&sqlite_select)
                    ),
                );
                line(&mut output, "      if (matches.length > 1) throw new PersistenceFault(\"update.cardinality\", \"cardinality\", new Error(\"required update matched more than one row\"));");
                line(&mut output, "      if (matches.length === 0) return null;");
                line(
                    &mut output,
                    &format!(
                        "      if (!policyAllowsWriteSqlite({}, \"update\", matches[0] as Record<string, unknown>, [{policy_fields}], policy, sqlite!)) return null;",
                        ts_string(&name)
                    ),
                );
                line(
                    &mut output,
                    &format!(
                        "      const rows = policySqliteMutationRows({}, \"update\", [{policy_fields}], {}, [{}, persistenceValue(predicateValue, {predicate_kind}, false)]);",
                        ts_string(&name),
                        ts_string(&sqlite_update),
                        update
                            .changes
                            .iter()
                            .enumerate()
                            .map(|(index, change)| {
                                let field = entity
                                    .fields
                                    .iter()
                                    .find(|field| field.name.text == change.name.text)
                                    .expect("checked update field exists");
                                let kind = ts_string(&self.representation_root_for(&field.field_type));
                                format!("persistenceValue(replacement{index}, {kind}, false)")
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                );
                line(&mut output, "      if (rows.length !== 1) throw new PersistenceFault(\"update.cardinality\", \"cardinality\", new Error(\"required update changed an unexpected row count\"));");
                line(
                    &mut output,
                    &format!("      recordSemanticChange(matches[0], rows[0], {ignored_semantic_field});"),
                );
                if self.entity_has_derived_representations(&name)
                    && generated_change_field(entity).is_none()
                {
                    line(
                        &mut output,
                        &format!(
                            "      recordAuthorityChange({}, \"update\", rows[0] as Record<string, unknown>, operationTime, null, sqlite);",
                            ts_string(&name)
                        ),
                    );
                }
                line(&mut output, "      return rows[0];");
                line(&mut output, "    };");
                let arguments = format!("predicateValue, {}", replacement_names.join(", "));
                line(
                    &mut output,
                    &format!(
                        "    if (transactional) return persistenceSync(\"update.transactional\", () => execute({arguments}));"
                    ),
                );
                line(
                    &mut output,
                    "    const mutate = sqlite!.transaction(execute);",
                );
                line(
                    &mut output,
                    &format!(
                        "    return persistenceSync({}, () => mutate({arguments}));",
                        ts_string(&operation)
                    ),
                );
                line(&mut output, "  },");
            }
        }
        let mut emitted_lifecycle_transitions = BTreeSet::new();
        for callable in self.callables.values() {
            let mut updates = Vec::new();
            collect_update_expressions(&callable.body, &mut updates);
            for update in updates
                .into_iter()
                .filter(|update| update.transition.is_some())
            {
                let name = update
                    .target
                    .path
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                let transition_name = update
                    .transition
                    .as_ref()
                    .expect("filtered lifecycle transition exists")
                    .text
                    .as_str();
                let method = format!(
                    "transition_required_{name}_by_{}_{}",
                    update.field.text, transition_name
                );
                if !emitted_lifecycle_transitions.insert(method.clone()) {
                    continue;
                }
                let entity = self
                    .records
                    .get(&name)
                    .expect("checked lifecycle transition entity exists");
                let lifecycle = entity
                    .dossier
                    .as_ref()
                    .and_then(|dossier| dossier.lifecycle.as_ref())
                    .expect("checked lifecycle transition has an entity contract");
                let transition = lifecycle
                    .transitions
                    .iter()
                    .find(|candidate| candidate.name.text == transition_name)
                    .expect("checked lifecycle transition exists");
                let table = sql_identifier(&snake_case(&name));
                let returned_fields = entity
                    .fields
                    .iter()
                    .map(|field| sql_identifier(&field.name.text))
                    .collect::<Vec<_>>()
                    .join(", ");
                let predicate_field = entity
                    .fields
                    .iter()
                    .find(|field| field.name.text == update.field.text)
                    .expect("checked transition predicate field exists");
                let predicate_kind =
                    ts_string(&self.representation_root_for(&predicate_field.field_type));
                let mut assignments = transition
                    .set
                    .iter()
                    .map(|field| (&field.name, Some(&field.value)))
                    .collect::<Vec<_>>();
                if let Some(generated) = generated_change_field(entity) {
                    if !assignments
                        .iter()
                        .any(|(name, _)| name.text == generated.name.text)
                    {
                        assignments.push((&generated.name, None));
                    }
                }
                let mut postgres_set = Vec::new();
                let mut sqlite_set = Vec::new();
                let mut postgres_values = Vec::new();
                let mut sqlite_values = Vec::new();
                let mut changed_fields = Vec::new();
                for (index, assignment) in assignments.iter().enumerate() {
                    let field = entity
                        .fields
                        .iter()
                        .find(|field| field.name.text == assignment.0.text)
                        .expect("checked lifecycle assignment field exists");
                    let kind = ts_string(&self.representation_root_for(&field.field_type));
                    let value = if assignment.1.is_none()
                        || matches!(
                            assignment.1,
                            Some(Expression::Name(name))
                                if name.path.len() == 2
                                    && name.path[0].text == "clock"
                                    && name.path[1].text == "now"
                        ) {
                        "operationTime".to_owned()
                    } else {
                        self.lifecycle_value_expression(
                            assignment.1.expect("authored lifecycle value exists"),
                        )
                    };
                    let column = sql_identifier(&assignment.0.text);
                    postgres_set.push(format!("{column} = ${}", index + 1));
                    sqlite_set.push(format!("{column} = ?{}", index + 1));
                    postgres_values.push(format!("persistenceValue({value}, {kind}, true)"));
                    sqlite_values.push(format!("persistenceValue({value}, {kind}, false)"));
                    changed_fields.push(ts_string(&assignment.0.text));
                }
                let postgres_predicate_index = assignments.len() + 1;
                let sqlite_predicate_index = assignments.len() + 1;
                let visibility = self.lifecycle_visibility_sql(&name);
                let transition_from = self
                    .lifecycle_predicate_sql(&name, &transition.from)
                    .expect("checked transition guard lowers to SQL");
                let mut guards = Vec::new();
                if visibility.as_deref() != Some(transition_from.as_str()) {
                    if let Some(visibility) = visibility.as_deref() {
                        guards.push(format!("({visibility})"));
                    }
                }
                guards.push(format!("({transition_from})"));
                let guard = guards.join(" AND ");
                let postgres_update = format!(
                    "UPDATE {table} SET {} WHERE {} = ${postgres_predicate_index} AND {guard} RETURNING {returned_fields}",
                    postgres_set.join(", "),
                    sql_identifier(&update.field.text)
                );
                let sqlite_update = format!(
                    "UPDATE {table} SET {} WHERE {} = ?{sqlite_predicate_index} AND {guard} RETURNING {returned_fields}",
                    sqlite_set.join(", "),
                    sql_identifier(&update.field.text)
                );
                let select_guard = if visibility.as_deref() == Some(transition_from.as_str()) {
                    String::new()
                } else {
                    format!(" AND ({transition_from})")
                };
                let postgres_policy_select = self
                    .select_optional_sql(&name, entity, &update.field.text, SqlDialect::Postgres)
                    .replace(" LIMIT 2", &format!("{select_guard} LIMIT 2 FOR UPDATE"));
                let sqlite_select = self
                    .select_optional_sql(&name, entity, &update.field.text, SqlDialect::Sqlite)
                    .replace(" LIMIT 2", &format!("{select_guard} LIMIT 2"));
                let policy_effect =
                    crate::entity_model::lifecycle_transition_policy_effect(transition_name);
                let policy_fields = changed_fields.join(", ");
                let ignored_semantic_field = generated_change_field(entity)
                    .map(|field| ts_string(&field.name.text))
                    .unwrap_or_else(|| "null".to_owned());
                let mut postgres_arguments = postgres_values.clone();
                postgres_arguments.push(format!(
                    "persistenceValue(predicateValue, {predicate_kind}, true)"
                ));
                let mut sqlite_arguments = sqlite_values.clone();
                sqlite_arguments.push(format!(
                    "persistenceValue(predicateValue, {predicate_kind}, false)"
                ));
                let operation = format!("lifecycle.{name}.{transition_name}");
                line(
                    &mut output,
                    &format!(
                        "  async {method}(predicateValue: unknown): Promise<unknown | null> {{"
                    ),
                );
                line(&mut output, "    if (operationTime === null) throw new PersistenceFault(\"lifecycle.operation_time\", \"data\", new Error(\"lifecycle transitions require an operation instant\"));");
                line(&mut output, "    if (postgres !== null) {");
                line(&mut output, "      const execute = async (tx: SQL) => {");
                line(
                    &mut output,
                    &format!(
                        "        const matches = await policyPostgresMutationRows(tx, {}, {}, [{}], {}, [persistenceValue(predicateValue, {predicate_kind}, true)]);",
                        ts_string(&name),
                        ts_string(policy_effect),
                        policy_fields,
                        ts_string(&postgres_policy_select),
                    ),
                );
                line(&mut output, "        if (matches.length > 1) throw new PersistenceFault(\"lifecycle.cardinality\", \"cardinality\", new Error(\"identity transition matched more than one row\"));");
                line(
                    &mut output,
                    "        if (matches.length === 0) return null;",
                );
                line(
                    &mut output,
                    &format!(
                        "        if (!(await policyAllowsWrite({}, {}, matches[0] as Record<string, unknown>, [{}], policy, tx, null))) return null;",
                        ts_string(&name),
                        ts_string(policy_effect),
                        policy_fields,
                    ),
                );
                line(
                    &mut output,
                    &format!(
                        "        const rows = await policyPostgresMutationRows(tx, {}, {}, [{}], {}, [{}]);",
                        ts_string(&name),
                        ts_string(policy_effect),
                        policy_fields,
                        ts_string(&postgres_update),
                        postgres_arguments.join(", "),
                    ),
                );
                line(&mut output, "        if (rows.length > 1) throw new PersistenceFault(\"lifecycle.cardinality\", \"cardinality\", new Error(\"identity transition changed more than one row\"));");
                line(&mut output, "        if (rows.length === 0) return null;");
                line(&mut output, &format!("        recordSemanticChange(matches[0], rows[0], {ignored_semantic_field});"));
                line(&mut output, "        return rows[0];");
                line(&mut output, "      };");
                line(
                    &mut output,
                    &format!(
                        "      return persistenceAsync({}, () => transactional ? execute(postgres) : postgres.begin(tx => execute(tx as SQL)));",
                        ts_string(&operation)
                    ),
                );
                line(&mut output, "    }");
                line(
                    &mut output,
                    "    const execute = (predicateValue: unknown) => {",
                );
                line(
                    &mut output,
                    &format!(
                        "      const matches = policySqliteMutationRows({}, {}, [{}], {}, [persistenceValue(predicateValue, {predicate_kind}, false)]);",
                        ts_string(&name),
                        ts_string(policy_effect),
                        policy_fields,
                        ts_string(&sqlite_select),
                    ),
                );
                line(&mut output, "      if (matches.length > 1) throw new PersistenceFault(\"lifecycle.cardinality\", \"cardinality\", new Error(\"identity transition matched more than one row\"));");
                line(&mut output, "      if (matches.length === 0) return null;");
                line(
                    &mut output,
                    &format!(
                        "      if (!policyAllowsWriteSqlite({}, {}, matches[0] as Record<string, unknown>, [{}], policy, sqlite!)) return null;",
                        ts_string(&name),
                        ts_string(policy_effect),
                        policy_fields,
                    ),
                );
                line(
                    &mut output,
                    &format!(
                        "      const rows = policySqliteMutationRows({}, {}, [{}], {}, [{}]);",
                        ts_string(&name),
                        ts_string(policy_effect),
                        policy_fields,
                        ts_string(&sqlite_update),
                        sqlite_arguments.join(", "),
                    ),
                );
                line(&mut output, "      if (rows.length > 1) throw new PersistenceFault(\"lifecycle.cardinality\", \"cardinality\", new Error(\"identity transition changed more than one row\"));");
                line(&mut output, "      if (rows.length === 0) return null;");
                line(&mut output, &format!("      recordSemanticChange(matches[0], rows[0], {ignored_semantic_field});"));
                line(&mut output, "      return rows[0];");
                line(&mut output, "    };");
                line(&mut output, "    if (transactional) return persistenceSync(\"lifecycle.transactional\", () => execute(predicateValue));");
                line(
                    &mut output,
                    "    const mutate = sqlite!.transaction(execute);",
                );
                line(
                    &mut output,
                    &format!(
                        "    return persistenceSync({}, () => mutate(predicateValue));",
                        ts_string(&operation)
                    ),
                );
                line(&mut output, "  },");
            }
        }
        for (child_name, child) in self.entities() {
            for relationship in child
                .fields
                .iter()
                .filter(|field| field.reference.is_some())
            {
                let reference = relationship
                    .reference
                    .as_ref()
                    .expect("filtered owning reference exists");
                let relationship_name = owning_relationship_name(relationship);
                let parent_name = &reference.target.path[0].text;
                let target_field = &reference.target.path[1].text;
                for predicate in &child.fields {
                    for cardinality in ["required", "optional"] {
                        let method = format!(
                            "query_required_{child_name}_with_{}_{cardinality}_by_{}",
                            relationship_name, predicate.name.text
                        );
                        line(
                            &mut output,
                            &format!(
                                "  async {method}(value: unknown): Promise<unknown | null> {{"
                            ),
                        );
                        line(
                            &mut output,
                            &format!(
                                "    const parent = await this.query_optional_{child_name}_by_{}(value);",
                                predicate.name.text
                            ),
                        );
                        line(&mut output, "    if (parent === null) return null;");
                        line(
                            &mut output,
                            &format!(
                                "    const foreignValue = (parent as Record<string, unknown>)[{}];",
                                ts_string(&relationship.name.text)
                            ),
                        );
                        if cardinality == "optional" {
                            line(
                                &mut output,
                                &format!(
                                    "    if (foreignValue === null) return {{ parent, {}: null }};",
                                    relationship_name
                                ),
                            );
                        }
                        line(
                            &mut output,
                            &format!(
                                "    const related = await this.query_optional_{parent_name}_by_{target_field}(foreignValue);"
                            ),
                        );
                        if self.lifecycle_visibility_sql(parent_name).is_some() {
                            let hidden_result = if cardinality == "required" {
                                "    if (related === null) return null;".to_owned()
                            } else {
                                format!(
                                    "    if (related === null) return {{ parent, {}: null }};",
                                    relationship_name
                                )
                            };
                            line(&mut output, &hidden_result);
                        } else {
                            line(
                                &mut output,
                                "    if (related === null) throw new PersistenceFault(\"query.relationship\", \"cardinality\", new Error(\"owning reference target is missing\"));",
                            );
                        }
                        line(
                            &mut output,
                            &format!("    return {{ parent, {}: related }};", relationship_name),
                        );
                        line(&mut output, "  },");
                    }
                }
                if !self.reference_is_nullable(&relationship.field_type) {
                    if let Some(parent) = self.records.get(parent_name) {
                        for nested in parent.inverses.iter().filter(|inverse| {
                            inverse.cardinality == jadpo_syntax::InverseCardinality::Optional
                        }) {
                            let Some(nested_via_field) =
                                nested.via.path.get(1).map(|part| part.text.as_str())
                            else {
                                continue;
                            };
                            let Some(nested_target_field) = self
                                .records
                                .get(&nested.target.text)
                                .and_then(|record| {
                                    record
                                        .fields
                                        .iter()
                                        .find(|field| field.name.text == nested_via_field)
                                })
                                .and_then(|field| field.reference.as_ref())
                                .and_then(|reference| reference.target.path.get(1))
                                .map(|part| part.text.as_str())
                            else {
                                continue;
                            };
                            for predicate in &child.fields {
                                let method = format!(
                                    "query_required_{child_name}_with_{}_then_{}_optional_by_{}",
                                    relationship_name, nested.name.text, predicate.name.text
                                );
                                line(
                                    &mut output,
                                    &format!(
                                        "  async {method}(value: unknown): Promise<unknown | null> {{"
                                    ),
                                );
                                line(
                                    &mut output,
                                    &format!(
                                        "    const root = await this.query_optional_{child_name}_by_{}(value);",
                                        predicate.name.text
                                    ),
                                );
                                line(&mut output, "    if (root === null) return null;");
                                line(
                                    &mut output,
                                    &format!(
                                        "    const owner = await this.query_optional_{parent_name}_by_{target_field}((root as Record<string, unknown>)[{}]);",
                                        ts_string(&relationship.name.text)
                                    ),
                                );
                                if self.lifecycle_visibility_sql(parent_name).is_some() {
                                    line(&mut output, "    if (owner === null) return null;");
                                } else {
                                    line(
                                        &mut output,
                                        "    if (owner === null) throw new PersistenceFault(\"query.relationship\", \"cardinality\", new Error(\"nested owning reference target is missing\"));",
                                    );
                                }
                                line(
                                    &mut output,
                                    &format!(
                                        "    const leaf = await this.query_optional_{}_by_{nested_via_field}((owner as Record<string, unknown>)[{}]);",
                                        nested.target.text,
                                        ts_string(nested_target_field)
                                    ),
                                );
                                line(
                                    &mut output,
                                    &format!(
                                        "    return {{ parent: root, {}: {{ parent: owner, {}: leaf }} }};",
                                        relationship_name, nested.name.text
                                    ),
                                );
                                line(&mut output, "  },");
                            }
                        }
                    }
                }
            }
        }
        for (child_name, child) in self.entities() {
            for relationship in child
                .fields
                .iter()
                .filter(|field| field.reference.is_some())
            {
                let reference = relationship
                    .reference
                    .as_ref()
                    .expect("filtered owning reference exists");
                let relationship_name = owning_relationship_name(relationship);
                let parent_name = &reference.target.path[0].text;
                let target_field = &reference.target.path[1].text;
                let Some(parent) = self.records.get(parent_name) else {
                    continue;
                };
                let child_object = child
                    .fields
                    .iter()
                    .map(|field| {
                        format!(
                            "{}: row[{}]",
                            field.name.text,
                            ts_string(&format!("child__{}", field.name.text))
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let parent_object = parent
                    .fields
                    .iter()
                    .map(|field| {
                        format!(
                            "{}: row[{}]",
                            field.name.text,
                            ts_string(&format!("owner__{}", field.name.text))
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                for predicate in &child.fields {
                    let predicate_kind =
                        ts_string(&self.representation_root_for(&predicate.field_type));
                    for order in child.fields.iter().filter(|field| {
                        has_modifier(field, PersistenceModifier::Identity)
                            || has_modifier(field, PersistenceModifier::Unique)
                    }) {
                        for direction in ["asc", "desc"] {
                            let method = format!(
                                "query_many_{child_name}_with_{relationship_name}_required_by_{}_order_by_{}_{direction}_paginated",
                                predicate.name.text,
                                order.name.text
                            );
                            let postgres_sql = self.select_many_with_owning_sql(
                                child_name,
                                child,
                                &predicate.name.text,
                                &order.name.text,
                                &direction.to_ascii_uppercase(),
                                parent_name,
                                parent,
                                &relationship.name.text,
                                target_field,
                                SqlDialect::Postgres,
                            );
                            let sqlite_sql = self.select_many_with_owning_sql(
                                child_name,
                                child,
                                &predicate.name.text,
                                &order.name.text,
                                &direction.to_ascii_uppercase(),
                                parent_name,
                                parent,
                                &relationship.name.text,
                                target_field,
                                SqlDialect::Sqlite,
                            );
                            let operation =
                                format!("query_many_include.{child_name}.{relationship_name}");
                            line(
                                &mut output,
                                &format!("  async {method}(value: unknown, limit: unknown, offset: unknown): Promise<unknown[]> {{"),
                            );
                            line(&mut output, "    const dialect = postgres !== null ? \"postgres\" : \"sqlite\";");
                            line(&mut output, &format!("    const childScope = policySqlScope({}, \"read\", policy, dialect, 3);", ts_string(child_name)));
                            line(&mut output, &format!("    const ownerScope = policySqlScope({}, \"read\", policy, dialect, 3 + childScope.values.length);", ts_string(parent_name)));
                            line(&mut output, "    if (!childScope.permitted || !ownerScope.permitted) return [];");
                            line(&mut output, &format!("    const postgresSql = {}.replace(\"__JADPO_CHILD_POLICY__\", childScope.clause).replace(\"__JADPO_OWNER_POLICY__\", ownerScope.clause);", ts_string(&postgres_sql)));
                            line(&mut output, &format!("    const sqliteSql = {}.replace(\"__JADPO_CHILD_POLICY__\", childScope.clause).replace(\"__JADPO_OWNER_POLICY__\", ownerScope.clause);", ts_string(&sqlite_sql)));
                            line(&mut output, &format!("    const postgresValues = [persistenceValue(value, {predicate_kind}, true), limit, offset, ...childScope.values, ...ownerScope.values];"));
                            line(&mut output, &format!("    const sqliteValues = [persistenceValue(value, {predicate_kind}, false), limit, offset, ...childScope.values, ...ownerScope.values];"));
                            line(&mut output, "    const rows = postgres !== null");
                            line(&mut output, &format!("      ? await persistenceAsync({}, () => postgres!.unsafe(postgresSql, postgresValues))", ts_string(&operation)));
                            line(&mut output, &format!("      : persistenceSync({}, () => {{ const statement = sqlite!.prepare(sqliteSql); try {{ return statement.all(...sqliteValues); }} finally {{ statement.finalize(); }} }});", ts_string(&operation)));
                            line(
                                &mut output,
                                "    return (rows as Array<Record<string, unknown>>).map(row => ({",
                            );
                            line(&mut output, &format!("      parent: {{ {child_object} }},"));
                            line(
                                &mut output,
                                &format!("      {relationship_name}: {{ {parent_object} }},"),
                            );
                            line(&mut output, "    }));");
                            line(&mut output, "  },");
                        }
                    }
                }
            }
        }
        for (parent_name, parent) in self.entities() {
            for inverse in &parent.inverses {
                let Some(child) = self.records.get(&inverse.target.text) else {
                    continue;
                };
                let Some(via_field) = inverse.via.path.get(1).map(|part| part.text.as_str()) else {
                    continue;
                };
                let Some(target_field) = child
                    .fields
                    .iter()
                    .find(|field| field.name.text == via_field)
                    .and_then(|field| field.reference.as_ref())
                    .and_then(|reference| reference.target.path.get(1))
                    .map(|name| name.text.as_str())
                else {
                    continue;
                };
                if inverse.cardinality == jadpo_syntax::InverseCardinality::Optional {
                    for predicate in &parent.fields {
                        let method = format!(
                            "query_required_{parent_name}_with_{}_optional_by_{}",
                            inverse.name.text, predicate.name.text
                        );
                        line(
                            &mut output,
                            &format!(
                                "  async {method}(value: unknown): Promise<unknown | null> {{"
                            ),
                        );
                        line(
                            &mut output,
                            &format!(
                                "    const parent = await this.query_optional_{parent_name}_by_{}(value);",
                                predicate.name.text
                            ),
                        );
                        line(&mut output, "    if (parent === null) return null;");
                        line(
                            &mut output,
                            &format!(
                                "    const related = await this.query_optional_{}_by_{via_field}((parent as Record<string, unknown>)[{}]);",
                                inverse.target.text,
                                ts_string(target_field)
                            ),
                        );
                        line(
                            &mut output,
                            &format!("    return {{ parent, {}: related }};", inverse.name.text),
                        );
                        line(&mut output, "  },");
                    }
                    continue;
                }
                for predicate in &parent.fields {
                    for order in child.fields.iter().filter(|field| {
                        has_modifier(field, PersistenceModifier::Identity)
                            || has_modifier(field, PersistenceModifier::Unique)
                    }) {
                        for direction in ["asc", "desc"] {
                            let method = format!(
                                "query_required_{parent_name}_with_{}_by_{}_order_by_{}_{direction}",
                                inverse.name.text, predicate.name.text, order.name.text
                            );
                            let child_method = format!(
                                "query_many_{}_by_{via_field}_order_by_{}_{direction}_paginated",
                                inverse.target.text, order.name.text
                            );
                            line(
                                &mut output,
                                &format!(
                                    "  async {method}(value: unknown, limit: unknown, offset: unknown): Promise<unknown | null> {{"
                                ),
                            );
                            line(
                                &mut output,
                                &format!(
                                    "    const parent = await this.query_optional_{parent_name}_by_{}(value);",
                                    predicate.name.text
                                ),
                            );
                            line(&mut output, "    if (parent === null) return null;");
                            line(
                                &mut output,
                                &format!(
                                    "    const children = await this.{child_method}((parent as Record<string, unknown>)[{}], limit, offset);",
                                    ts_string(target_field)
                                ),
                            );
                            line(
                                &mut output,
                                &format!(
                                    "    return {{ parent, {}: children }};",
                                    inverse.name.text
                                ),
                            );
                            line(&mut output, "  },");
                        }
                    }
                }
                let Some(child_presence) = child.fields.iter().find(|field| {
                    has_modifier(field, PersistenceModifier::Identity)
                        || has_modifier(field, PersistenceModifier::Unique)
                }) else {
                    continue;
                };
                let parent_object = parent
                    .fields
                    .iter()
                    .map(|field| {
                        format!(
                            "{}: row[{}]",
                            field.name.text,
                            ts_string(&format!("parent__{}", field.name.text))
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let child_object = child
                    .fields
                    .iter()
                    .map(|field| {
                        format!(
                            "{}: row[{}]",
                            field.name.text,
                            ts_string(&format!("child__{}", field.name.text))
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                for predicate in &parent.fields {
                    for parent_order in parent.fields.iter().filter(|field| {
                        has_modifier(field, PersistenceModifier::Identity)
                            || has_modifier(field, PersistenceModifier::Unique)
                    }) {
                        for child_order in child.fields.iter().filter(|field| {
                            has_modifier(field, PersistenceModifier::Identity)
                                || has_modifier(field, PersistenceModifier::Unique)
                        }) {
                            for parent_direction in ["asc", "desc"] {
                                for child_direction in ["asc", "desc"] {
                                    let method = format!(
                                        "query_many_{parent_name}_with_{}_by_{}_order_by_{}_{parent_direction}_include_order_by_{}_{child_direction}_paginated",
                                        inverse.name.text,
                                        predicate.name.text,
                                        parent_order.name.text,
                                        child_order.name.text
                                    );
                                    let postgres_sql = self.select_many_with_inverse_sql(
                                        parent_name,
                                        parent,
                                        &predicate.name.text,
                                        &parent_order.name.text,
                                        &parent_direction.to_ascii_uppercase(),
                                        &inverse.target.text,
                                        child,
                                        via_field,
                                        target_field,
                                        &child_order.name.text,
                                        &child_direction.to_ascii_uppercase(),
                                        SqlDialect::Postgres,
                                    );
                                    let sqlite_sql = self.select_many_with_inverse_sql(
                                        parent_name,
                                        parent,
                                        &predicate.name.text,
                                        &parent_order.name.text,
                                        &parent_direction.to_ascii_uppercase(),
                                        &inverse.target.text,
                                        child,
                                        via_field,
                                        target_field,
                                        &child_order.name.text,
                                        &child_direction.to_ascii_uppercase(),
                                        SqlDialect::Sqlite,
                                    );
                                    let postgres_template = postgres_sql
                                        .replace("$1", "${value}")
                                        .replace("$2", "${parentLimit}")
                                        .replace("$3", "${parentOffset}")
                                        .replace("$4", "${childLimit}")
                                        .replace("$5", "${childOffset}");
                                    line(
                                        &mut output,
                                        &format!(
                                            "  async {method}(value: unknown, parentLimit: unknown, parentOffset: unknown, childLimit: unknown, childOffset: unknown): Promise<unknown[]> {{"
                                        ),
                                    );
                                    line(&mut output, "    const rows = postgres !== null");
                                    line(
                                        &mut output,
                                        &format!(
                                            "      ? await persistenceAsync({}, () => postgres`{postgres_template}`)",
                                            ts_string(&format!(
                                                "query_many_include.{parent_name}.{}",
                                                inverse.name.text
                                            ))
                                        ),
                                    );
                                    line(
                                        &mut output,
                                        &format!(
                                            "      : persistenceSync({}, () => sqlite!.prepare({}).all(value, parentLimit, parentOffset, childLimit, childOffset));",
                                            ts_string(&format!(
                                                "query_many_include.{parent_name}.{}",
                                                inverse.name.text
                                            )),
                                            ts_string(&sqlite_sql)
                                        ),
                                    );
                                    line(
                                        &mut output,
                                        &format!(
                                            "    const grouped = new Map<unknown, {{ parent: Record<string, unknown>; {}: Record<string, unknown>[] }}>();",
                                            inverse.name.text
                                        ),
                                    );
                                    line(&mut output, "    for (const raw of rows) {");
                                    line(
                                        &mut output,
                                        "      const row = raw as Record<string, unknown>;",
                                    );
                                    line(
                                        &mut output,
                                        &format!("      const parent = {{ {parent_object} }};"),
                                    );
                                    line(
                                        &mut output,
                                        &format!(
                                            "      if (!(await policyAllows({}, \"read\", parent, policy, postgres, sqlite))) continue;",
                                            ts_string(parent_name)
                                        ),
                                    );
                                    line(
                                        &mut output,
                                        &format!(
                                            "      const key = parent[{}];",
                                            ts_string(target_field)
                                        ),
                                    );
                                    line(&mut output, "      let entry = grouped.get(key);");
                                    line(&mut output, "      if (entry === undefined) {");
                                    line(
                                        &mut output,
                                        &format!(
                                            "        entry = {{ parent, {}: [] }};",
                                            inverse.name.text
                                        ),
                                    );
                                    line(&mut output, "        grouped.set(key, entry);");
                                    line(&mut output, "      }");
                                    let child_presence =
                                        ts_string(&format!("child__{}", child_presence.name.text));
                                    line(
                                        &mut output,
                                        &format!(
                                            "      if (row[{child_presence}] !== null && row[{child_presence}] !== undefined) {{"
                                        ),
                                    );
                                    line(
                                        &mut output,
                                        &format!("        const child = {{ {child_object} }};"),
                                    );
                                    line(
                                        &mut output,
                                        &format!(
                                            "        if (await policyAllows({}, \"read\", child, policy, postgres, sqlite)) entry.{}.push(child);",
                                            ts_string(&inverse.target.text),
                                            inverse.name.text
                                        ),
                                    );
                                    line(&mut output, "      }");
                                    line(&mut output, "    }");
                                    line(&mut output, "    return [...grouped.values()];");
                                    line(&mut output, "  },");
                                }
                            }
                        }
                    }
                }
            }
        }
        line(&mut output, "};");
        line(&mut output, "return client;");
        line(&mut output, "}");
        line(&mut output, "");
        line(&mut output, "await initializePersistenceOnce();");
        line(
            &mut output,
            "let activePersistenceClient = createPersistenceClient(postgres, sqlite);",
        );
        output.push_str(include_str!("runtime/readiness_gate.ts"));
        line(
            &mut output,
            "export function isPersistenceReady(): boolean { return persistenceReady; }",
        );
        line(&mut output, "export function isPersistenceInitializationFatal(): boolean { return persistenceInitializationFatal; }");
        line(&mut output, "async function probePersistence(setCancel: (cancel: () => void) => void): Promise<void> {");
        line(&mut output, "  if (postgres !== null) { const query = postgres`SELECT 1`.execute(); setCancel(() => { query.cancel(); }); await query; return; }");
        line(&mut output, "  if (sqlite !== null) { persistenceSync(\"health.database\", () => sqlite!.prepare(\"PRAGMA schema_version\").get()); return; }");
        line(&mut output, "  throw new Error(\"database unavailable\");");
        line(&mut output, "}");
        line(&mut output, "const readinessGate = createReadinessGate({");
        line(&mut output, "  ready: () => persistenceReady, fatal: () => persistenceInitializationFatal, needsProbe: () => persistenceProbeNeeded, invalidation: () => persistenceInvalidationGeneration,");
        line(&mut output, "  check: async (recovering, setCancel) => recovering ? initializePersistenceOnce(false) : (await probePersistence(setCancel), true),");
        line(&mut output, "  prepare: (recovering) => recovering ? createPersistenceClient(postgres, sqlite) : null,");
        line(&mut output, "  publish: (candidate) => { if (candidate !== null) activePersistenceClient = candidate; persistenceReady = true; persistenceProbeNeeded = false; persistenceDependencyUnavailableReported = false; persistenceStartupFaultReported = false; },");
        line(
            &mut output,
            "  unavailable: () => { persistenceReady = false; },",
        );
        line(&mut output, "});");
        line(&mut output, "export function refreshPersistenceReadiness(): Promise<boolean> { return readinessGate.refresh(); }");
        line(&mut output, "export const persistence = new Proxy(activePersistenceClient, { get(_target, property) { return Reflect.get(activePersistenceClient, property, activePersistenceClient); } });");
        if !self.tests.is_empty() {
            line(&mut output, "");
            line(
                &mut output,
                "export function createIsolatedTestPersistence() {",
            );
            line(&mut output, "  if (Bun.env.DATABASE_URL) throw new PersistenceFault(\"test.database\", \"driver\", new Error(\"isolated tests require the SQLite test adapter\"));");
            line(
                &mut output,
                "  const database = new Database(\":memory:\", { create: true, strict: true });",
            );
            line(
                &mut output,
                "  database.exec(\"PRAGMA foreign_keys = ON\");",
            );
            if self.has_derived_representations() {
                line(&mut output, "  database.exec('CREATE TABLE \"__jadpo_changes\" (\"change_id\" TEXT PRIMARY KEY, \"entity\" TEXT NOT NULL, \"entity_id\" TEXT NOT NULL, \"revision\" INTEGER NOT NULL, \"operation\" TEXT NOT NULL, \"payload\" TEXT NOT NULL, \"created_at\" TEXT NOT NULL, UNIQUE (\"entity\", \"entity_id\", \"revision\"))');");
            }
            for (name, entity) in self.schema_entities() {
                let table = sql_identifier(&snake_case(name));
                let sqlite_columns = self.table_body(name, entity, SqlDialect::Sqlite, "", ", ");
                line(
                    &mut output,
                    &format!(
                        "  database.exec({});",
                        ts_string(&format!("CREATE TABLE {table} ({sqlite_columns})"))
                    ),
                );
                for statement in self.index_statements(name, entity) {
                    line(
                        &mut output,
                        &format!("  database.exec({});", ts_string(&statement)),
                    );
                }
            }
            line(&mut output, "  return Object.freeze({ client: createPersistenceClient(null, database), close: () => database.close(false) });");
            line(&mut output, "}");
        }
        output
    }

    fn table_body(
        &self,
        name: &str,
        entity: &RecordDeclaration,
        dialect: SqlDialect,
        indent: &str,
        separator: &str,
    ) -> String {
        let mut parts = entity
            .fields
            .iter()
            .map(|field| {
                let nullable = if self.reference_is_nullable(&field.field_type) {
                    ""
                } else {
                    " NOT NULL"
                };
                let column = sql_identifier(&field.name.text);
                let representation = self.representation_root_for(&field.field_type);
                let checked = match (dialect, representation.as_str()) {
                    (SqlDialect::Sqlite, "Instant" | "Duration") => format!(
                        " CHECK ({column} IS NULL OR typeof({column}) = 'integer')"
                    ),
                    (SqlDialect::Sqlite, "CalendarDate") => format!(
                        " CHECK ({column} IS NULL OR (typeof({column}) = 'text' AND length({column}) = 10 AND {column} GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'))"
                    ),
                    (SqlDialect::Sqlite, "Time") => format!(
                        " CHECK ({column} IS NULL OR (typeof({column}) = 'text' AND json_valid({column})))"
                    ),
                    _ => String::new(),
                };
                format!(
                    "{indent}{column} {}{nullable}{checked}",
                    self.sql_type(&field.field_type, dialect),
                )
            })
            .collect::<Vec<_>>();
        for field in &entity.fields {
            if has_modifier(field, PersistenceModifier::Identity) {
                parts.push(format!(
                    "{indent}CONSTRAINT {} PRIMARY KEY ({})",
                    sql_identifier(&format!("{}_identity", snake_case(name))),
                    sql_identifier(&field.name.text)
                ));
            }
            if has_modifier(field, PersistenceModifier::Unique) {
                parts.push(format!(
                    "{indent}CONSTRAINT {} UNIQUE ({})",
                    sql_identifier(&format!(
                        "{}_{}_unique",
                        snake_case(name),
                        snake_case(&field.name.text)
                    )),
                    sql_identifier(&field.name.text)
                ));
            }
            if let Some(reference) = &field.reference {
                let target_entity = &reference.target.path[0].text;
                let target_field = &reference.target.path[1].text;
                parts.push(format!(
                    "{indent}CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({}) ON DELETE {}",
                    sql_identifier(&format!(
                        "{}_{}_fk",
                        snake_case(name),
                        snake_case(&field.name.text)
                    )),
                    sql_identifier(&field.name.text),
                    sql_identifier(&snake_case(target_entity)),
                    sql_identifier(target_field),
                    reference_delete_sql(reference.on_delete)
                ));
            }
        }
        for constraint in &entity.persistence_constraints {
            let columns = constraint
                .fields
                .iter()
                .map(|field| sql_identifier(&field.text))
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!(
                "{indent}CONSTRAINT {} UNIQUE ({columns})",
                sql_identifier(&format!(
                    "{}_{}_unique",
                    snake_case(name),
                    snake_case(&constraint.name.text)
                ))
            ));
        }
        parts.join(separator)
    }

    fn index_statements(&self, name: &str, entity: &RecordDeclaration) -> Vec<String> {
        let table = sql_identifier(&snake_case(name));
        let mut statements = entity
            .fields
            .iter()
            .filter(|field| {
                has_modifier(field, PersistenceModifier::Index) || field.reference.is_some()
            })
            .map(|field| {
                let index = sql_identifier(&format!(
                    "{}_{}_idx",
                    snake_case(name),
                    snake_case(&field.name.text)
                ));
                let column = sql_identifier(&field.name.text);
                format!("CREATE INDEX IF NOT EXISTS {index} ON {table} ({column});")
            })
            .collect::<Vec<_>>();
        let mut queries = Vec::new();
        for callable in self.callables.values() {
            collect_query_expressions(&callable.body, &mut queries);
        }
        let mut seen = BTreeSet::new();
        for query in queries {
            let target = query
                .target
                .path
                .iter()
                .map(|part| part.text.as_str())
                .collect::<Vec<_>>()
                .join(".");
            let Some(page) = query.page.as_ref().filter(|_| target == name) else {
                continue;
            };
            let columns = query_page_index_columns(page);
            let predicate_fields = query_page_index_predicate_fields(page);
            if columns.is_empty() || !seen.insert((columns.clone(), predicate_fields.clone())) {
                continue;
            }
            let index = sql_identifier(&query_page_index_name(name, &columns, &predicate_fields));
            let columns = columns
                .iter()
                .map(|field| {
                    let direction = page
                        .order
                        .iter()
                        .find(|order| order.field.text == *field)
                        .map(|order| match order.direction {
                            jadpo_syntax::QueryOrderDirection::Ascending => "ASC",
                            jadpo_syntax::QueryOrderDirection::Descending => "DESC",
                        })
                        .unwrap_or("ASC");
                    format!("{} {direction}", sql_identifier(field))
                })
                .collect::<Vec<_>>()
                .join(", ");
            let predicate = query_page_index_predicates(page);
            let predicate = if predicate.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", predicate.join(" AND "))
            };
            statements.push(format!(
                "CREATE INDEX IF NOT EXISTS {index} ON {table} ({columns}){predicate};"
            ));
        }
        statements
    }

    fn insert_sql(&self, name: &str, entity: &RecordDeclaration, dialect: SqlDialect) -> String {
        let table = sql_identifier(&snake_case(name));
        let fields = entity
            .fields
            .iter()
            .map(|field| sql_identifier(&field.name.text))
            .collect::<Vec<_>>();
        let placeholders = (1..=fields.len())
            .map(|index| match dialect {
                SqlDialect::Postgres => format!("${index}"),
                SqlDialect::Sqlite => format!("?{index}"),
            })
            .collect::<Vec<_>>();
        format!(
            "INSERT INTO {table} ({}) VALUES ({}) RETURNING {}",
            fields.join(", "),
            placeholders.join(", "),
            fields.join(", ")
        )
    }

    fn lifecycle_visibility_sql(&self, entity_name: &str) -> Option<String> {
        let visible = self
            .records
            .get(entity_name)?
            .dossier
            .as_ref()?
            .lifecycle
            .as_ref()?
            .visible
            .as_ref()?;
        self.lifecycle_predicate_sql(entity_name, visible)
    }

    fn lifecycle_purge_config_fields(&self) -> Vec<String> {
        self.entities()
            .filter_map(|(_, record)| {
                let purge = record
                    .dossier
                    .as_ref()?
                    .lifecycle
                    .as_ref()?
                    .purge
                    .as_ref()?;
                let Expression::Name(binding) = &purge.after else {
                    return None;
                };
                (binding.path.len() == 2 && binding.path[0].text == "config")
                    .then(|| binding.path[1].text.clone())
            })
            .collect()
    }

    fn lifecycle_initial_values(&self, entity_name: &str) -> Vec<(String, String)> {
        let Some(initial) = self
            .records
            .get(entity_name)
            .and_then(|record| record.dossier.as_ref())
            .and_then(|dossier| dossier.lifecycle.as_ref())
            .and_then(|lifecycle| lifecycle.initial.as_ref())
        else {
            return Vec::new();
        };
        initial
            .iter()
            .map(|field| {
                (
                    field.name.text.clone(),
                    self.lifecycle_initial_expression(&field.value)
                        .expect("checked lifecycle initial expressions have a persistence value"),
                )
            })
            .collect()
    }

    fn lifecycle_initial_expression(&self, expression: &Expression) -> Option<String> {
        match expression {
            Expression::Literal(literal) => Some(if literal.kind == LiteralKind::None {
                "null".to_owned()
            } else {
                literal.text.clone()
            }),
            Expression::Grouped(grouped) => self.lifecycle_initial_expression(&grouped.value),
            Expression::Name(name)
                if name.path.len() == 2
                    && name.path[0].text == "clock"
                    && name.path[1].text == "now" =>
            {
                Some("(operationTime ?? new Date().toISOString())".to_owned())
            }
            Expression::Name(name)
                if name.path.len() == 2 && self.enums.contains_key(&name.path[0].text) =>
            {
                Some(ts_string(&name.path[1].text))
            }
            Expression::Invocation(invocation)
                if invocation.arguments.len() == 1 && invocation.named_arguments.is_empty() =>
            {
                self.lifecycle_initial_expression(&invocation.arguments[0])
            }
            _ => None,
        }
    }

    fn has_lifecycle_purges(&self) -> bool {
        self.entities().any(|(_, record)| {
            record
                .dossier
                .as_ref()
                .and_then(|dossier| dossier.lifecycle.as_ref())
                .is_some_and(|lifecycle| lifecycle.purge.is_some())
        })
    }

    fn has_lifecycles(&self) -> bool {
        self.entities().any(|(_, record)| {
            record
                .dossier
                .as_ref()
                .and_then(|dossier| dossier.lifecycle.as_ref())
                .is_some()
        })
    }

    fn lifecycle_manifest(&self, project_path: &Path) -> String {
        let normalized = |source: &str| {
            crate::artifacts::normalized_source(
                crate::artifacts::project_root(project_path),
                source,
            )
        };
        let entities = self
            .entities()
            .filter_map(|(name, record)| {
                let lifecycle = record.dossier.as_ref()?.lifecycle.as_ref()?;
                let source = self.project.syntax.sources.iter().find(|source| {
                    source.file.declarations.iter().any(|declaration| {
                        matches!(declaration, Declaration::Record(candidate) if candidate.name.text == name.as_str())
                    })
                });
                let source_name = source.map(|source| source.source_name.as_str()).unwrap_or("");
                let node_facts = |prefix: String| {
                    self.project
                        .semantics
                        .nodes
                        .iter()
                        .filter(|node| {
                            node.source == source_name
                                && (node.name == prefix
                                    || node.name.starts_with(&format!("{prefix}.")))
                        })
                        .map(|node| {
                            serde_json::json!({
                                "id": node.id.0,
                                "kind": node.kind.as_str(),
                                "name": node.name,
                                "source": normalized(&node.source),
                                "start": node.range.start,
                                "end": node.range.end,
                            })
                        })
                        .collect::<Vec<_>>()
                };
                let transitions = lifecycle
                    .transitions
                    .iter()
                    .map(|transition| {
                        serde_json::json!({
                            "name": transition.name.text,
                            "source": normalized(source_name),
                            "start": transition.range.start,
                            "end": transition.range.end,
                            "assignments": transition.set.iter().map(|field| field.name.text.as_str()).collect::<Vec<_>>(),
                            "logical_policy_effect": crate::entity_model::lifecycle_transition_policy_effect(&transition.name.text),
                            "sql_verb": "UPDATE",
                            "guarded_sql": ["identity", "lifecycle_visibility", "transition_from"],
                        })
                    })
                    .collect::<Vec<_>>();
                let generated_fields = record
                    .fields
                    .iter()
                    .filter(|field| field.generated.is_some())
                    .map(|field| {
                        let node_name = format!("{name}.{}.generated", field.name.text);
                        let node = self.project.semantics.nodes.iter().find(|node| {
                            node.source == source_name && node.name == node_name
                        });
                        serde_json::json!({
                            "name": field.name.text,
                            "node_id": node.map(|node| node.id.0),
                            "source": normalized(source_name),
                            "start": field.name.range.start,
                            "end": field.name.range.end,
                        })
                    })
                    .collect::<Vec<_>>();
                Some(serde_json::json!({
                    "entity": name,
                    "source": normalized(source_name),
                    "lifecycle_nodes": node_facts(format!("{name}.lifecycle")),
                    "policy_nodes": node_facts(format!("{name}.policy")),
                    "owned_fields": lifecycle.initial.as_ref().map(|fields| fields.iter().map(|field| field.name.text.as_str()).collect::<Vec<_>>()).unwrap_or_default(),
                    "generated_fields": generated_fields,
                    "transitions": transitions,
                    "purge": lifecycle.purge.as_ref().map(|purge| serde_json::json!({
                        "source": normalized(source_name),
                        "start": purge.range.start,
                        "end": purge.range.end,
                        "timestamp_field": purge.from.text,
                    })),
                }))
            })
            .collect::<Vec<_>>();
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "kind": "entity_lifecycle_contracts",
            "entities": entities,
        }))
        .expect("lifecycle contract manifest should be serializable")
            + "\n"
    }

    fn lifecycle_purge_state_sql(&self, entity_name: &str) -> Option<String> {
        let lifecycle = self
            .records
            .get(entity_name)?
            .dossier
            .as_ref()?
            .lifecycle
            .as_ref()?;
        let purge = lifecycle.purge.as_ref()?;
        let mut transition_states = Vec::new();
        for transition in &lifecycle.transitions {
            if !transition
                .set
                .iter()
                .any(|field| field.name.text == purge.from.text)
            {
                continue;
            }
            let mut predicates = Vec::new();
            for field in transition
                .set
                .iter()
                .filter(|field| field.name.text != purge.from.text)
            {
                let column = sql_identifier(&field.name.text);
                let value = match &field.value {
                    Expression::Literal(literal) if literal.kind == LiteralKind::None => {
                        predicates.push(format!("{column} IS NULL"));
                        continue;
                    }
                    Expression::Name(name)
                        if name.path.len() == 2
                            && name.path[0].text == "clock"
                            && name.path[1].text == "now" =>
                    {
                        sql_identifier(&purge.from.text)
                    }
                    value => self.lifecycle_value_sql(entity_name, value)?,
                };
                predicates.push(format!("{column} = {value}"));
            }
            transition_states.push(if predicates.is_empty() {
                "1 = 1".to_owned()
            } else {
                predicates.join(" AND ")
            });
        }
        (!transition_states.is_empty()).then(|| {
            transition_states
                .into_iter()
                .map(|state| format!("({state})"))
                .collect::<Vec<_>>()
                .join(" OR ")
        })
    }

    fn lifecycle_maintenance_manifest(&self) -> String {
        let capabilities = self
            .entities()
            .filter_map(|(name, record)| {
                let purge = record
                    .dossier
                    .as_ref()?
                    .lifecycle
                    .as_ref()?
                    .purge
                    .as_ref()?;
                let retention = match &purge.after {
                    Expression::Name(binding)
                        if binding.path.len() == 2 && binding.path[0].text == "config" =>
                    {
                        format!("config.{}", binding.path[1].text)
                    }
                    _ => String::new(),
                };
                let identity = identity_field_name(record).unwrap_or_default();
                Some(serde_json::json!({
                    "entity": name,
                    "clause": format!("{name}.lifecycle.purge"),
                    "effect": format!("retention_purge({name})"),
                    "capability": "compiler_maintenance_plane",
                    "source_callable": false,
                    "retention_binding": retention,
                    "timestamp_field": purge.from.text,
                    "identity_field": identity,
                    "eligibility": [
                        "timestamp_present",
                        "timestamp_before_attempt_minus_positive_retention",
                        "ordinary_visibility_false",
                        "declared_soft_delete_transition_state"
                    ],
                    "order_by": [purge.from.text, identity],
                    "batch_limit": 500,
                    "transaction": "select_and_final_predicate_recheck_and_delete",
                    "audit_event": "retention.purge.batch"
                }))
            })
            .collect::<Vec<_>>();
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "kind": "compiler_maintenance_authority",
            "default": "deny",
            "ordinary_policy_effects": "separate",
            "capabilities": capabilities
        }))
        .expect("maintenance authority manifest should be serializable")
            + "\n"
    }

    fn lifecycle_owned_field_names(&self, entity_name: &str) -> BTreeSet<String> {
        let Some(lifecycle) = self
            .records
            .get(entity_name)
            .and_then(|record| record.dossier.as_ref())
            .and_then(|dossier| dossier.lifecycle.as_ref())
        else {
            return BTreeSet::new();
        };
        lifecycle
            .initial
            .iter()
            .flat_map(|initial| initial.iter().map(|field| field.name.text.clone()))
            .chain(
                lifecycle.transitions.iter().flat_map(|transition| {
                    transition.set.iter().map(|field| field.name.text.clone())
                }),
            )
            .chain(lifecycle.purge.iter().map(|purge| purge.from.text.clone()))
            .collect()
    }

    fn lifecycle_value_expression(&self, expression: &Expression) -> String {
        match expression {
            Expression::Grouped(grouped) => self.lifecycle_value_expression(&grouped.value),
            Expression::Invocation(invocation)
                if self
                    .field_validator_name(
                        &invocation
                            .callee
                            .path
                            .iter()
                            .map(|part| part.text.as_str())
                            .collect::<Vec<_>>()
                            .join("."),
                    )
                    .is_some() =>
            {
                invocation
                    .arguments
                    .first()
                    .map(|value| self.lifecycle_value_expression(value))
                    .unwrap_or_else(|| "undefined".to_owned())
            }
            _ => self.expression(expression),
        }
    }

    fn lifecycle_predicate_sql(
        &self,
        entity_name: &str,
        expression: &Expression,
    ) -> Option<String> {
        match expression {
            Expression::Grouped(grouped) => {
                self.lifecycle_predicate_sql(entity_name, &grouped.value)
            }
            Expression::Binary(binary) if binary.operator == jadpo_syntax::BinaryOperator::And => {
                Some(format!(
                    "({}) AND ({})",
                    self.lifecycle_predicate_sql(entity_name, &binary.left)?,
                    self.lifecycle_predicate_sql(entity_name, &binary.right)?
                ))
            }
            Expression::Binary(binary)
                if binary.operator == jadpo_syntax::BinaryOperator::Equal =>
            {
                let left_none = matches!(
                    binary.left.as_ref(),
                    Expression::Literal(literal) if literal.kind == LiteralKind::None
                );
                let right_none = matches!(
                    binary.right.as_ref(),
                    Expression::Literal(literal) if literal.kind == LiteralKind::None
                );
                if left_none || right_none {
                    let field = if left_none {
                        self.lifecycle_field_column(entity_name, &binary.right)?
                    } else {
                        self.lifecycle_field_column(entity_name, &binary.left)?
                    };
                    return Some(format!("{field} IS NULL"));
                }
                Some(format!(
                    "{} = {}",
                    self.lifecycle_value_sql(entity_name, &binary.left)?,
                    self.lifecycle_value_sql(entity_name, &binary.right)?
                ))
            }
            _ => None,
        }
    }

    fn lifecycle_field_column(&self, entity_name: &str, expression: &Expression) -> Option<String> {
        let Expression::Name(name) = expression else {
            return None;
        };
        if name.path.len() != 1
            || !self
                .records
                .get(entity_name)?
                .fields
                .iter()
                .any(|field| field.name.text == name.path[0].text)
        {
            return None;
        }
        Some(sql_identifier(&name.path[0].text))
    }

    fn lifecycle_value_sql(&self, entity_name: &str, expression: &Expression) -> Option<String> {
        if let Some(column) = self.lifecycle_field_column(entity_name, expression) {
            return Some(column);
        }
        match expression {
            Expression::Grouped(grouped) => self.lifecycle_value_sql(entity_name, &grouped.value),
            Expression::Invocation(invocation)
                if self
                    .field_validator_name(
                        &invocation
                            .callee
                            .path
                            .iter()
                            .map(|part| part.text.as_str())
                            .collect::<Vec<_>>()
                            .join("."),
                    )
                    .is_some() =>
            {
                invocation
                    .arguments
                    .first()
                    .and_then(|value| self.lifecycle_value_sql(entity_name, value))
            }
            Expression::Literal(literal) => match literal.kind {
                LiteralKind::None => None,
                LiteralKind::Boolean => Some(match literal.text.as_str() {
                    "true" => "TRUE".to_owned(),
                    "false" => "FALSE".to_owned(),
                    _ => return None,
                }),
                LiteralKind::Integer | LiteralKind::Decimal => Some(literal.text.clone()),
                LiteralKind::String => {
                    let decoded = decode_string_literal(&literal.text)?;
                    Some(sql_string_literal(&decoded))
                }
            },
            Expression::Name(name) if name.path.len() == 2 => {
                let enum_name = &name.path[0].text;
                let variant_name = &name.path[1].text;
                let declaration = self.enums.get(enum_name)?;
                if enum_is_tagged(declaration)
                    || !declaration.variants.iter().any(|variant| {
                        variant.name.text == *variant_name && variant.fields.is_empty()
                    })
                {
                    return None;
                }
                Some(sql_string_literal(variant_name))
            }
            _ => None,
        }
    }

    fn select_optional_sql(
        &self,
        name: &str,
        entity: &RecordDeclaration,
        predicate_field: &str,
        dialect: SqlDialect,
    ) -> String {
        let table = sql_identifier(&snake_case(name));
        let fields = entity
            .fields
            .iter()
            .map(|field| sql_identifier(&field.name.text))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholder = match dialect {
            SqlDialect::Postgres => "$1",
            SqlDialect::Sqlite => "?1",
        };
        let visibility = self
            .lifecycle_visibility_sql(name)
            .map(|predicate| format!(" AND ({predicate})"))
            .unwrap_or_default();
        format!(
            "SELECT {fields} FROM {table} WHERE {} = {placeholder}{visibility} LIMIT 2",
            sql_identifier(predicate_field)
        )
    }

    fn select_many_sql(
        &self,
        name: &str,
        entity: &RecordDeclaration,
        predicate_field: &str,
        order_field: &str,
        direction: &str,
        dialect: SqlDialect,
    ) -> String {
        let table = sql_identifier(&snake_case(name));
        let fields = entity
            .fields
            .iter()
            .map(|field| sql_identifier(&field.name.text))
            .collect::<Vec<_>>()
            .join(", ");
        let placeholder = match dialect {
            SqlDialect::Postgres => "$1",
            SqlDialect::Sqlite => "?1",
        };
        let visibility = self
            .lifecycle_visibility_sql(name)
            .map(|predicate| format!(" AND ({predicate})"))
            .unwrap_or_default();
        format!(
            "SELECT {fields} FROM {table} WHERE {} = {placeholder}{visibility} ORDER BY {} {direction}",
            sql_identifier(predicate_field),
            sql_identifier(order_field)
        )
    }

    fn select_many_paginated_sql(
        &self,
        name: &str,
        entity: &RecordDeclaration,
        predicate_field: &str,
        order_field: &str,
        direction: &str,
        dialect: SqlDialect,
    ) -> String {
        let table = sql_identifier(&snake_case(name));
        let fields = entity
            .fields
            .iter()
            .map(|field| sql_identifier(&field.name.text))
            .collect::<Vec<_>>()
            .join(", ");
        let (predicate, limit, offset) = match dialect {
            SqlDialect::Postgres => ("$1", "$2", "$3"),
            SqlDialect::Sqlite => ("?1", "?2", "?3"),
        };
        let visibility = self
            .lifecycle_visibility_sql(name)
            .map(|predicate| format!(" AND ({predicate})"))
            .unwrap_or_default();
        format!(
            "SELECT {fields} FROM {table} WHERE {} = {predicate}{visibility} ORDER BY {} {direction} LIMIT {limit} OFFSET {offset}",
            sql_identifier(predicate_field),
            sql_identifier(order_field)
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn select_many_with_owning_sql(
        &self,
        child_name: &str,
        child: &RecordDeclaration,
        predicate_field: &str,
        order_field: &str,
        direction: &str,
        parent_name: &str,
        parent: &RecordDeclaration,
        reference_field: &str,
        target_field: &str,
        dialect: SqlDialect,
    ) -> String {
        let child_table = sql_identifier(&snake_case(child_name));
        let parent_table = sql_identifier(&snake_case(parent_name));
        let child_fields = child
            .fields
            .iter()
            .map(|field| sql_identifier(&field.name.text))
            .collect::<Vec<_>>()
            .join(", ");
        let selected_child = child
            .fields
            .iter()
            .map(|field| {
                format!(
                    "\"child_page\".{} AS {}",
                    sql_identifier(&field.name.text),
                    sql_identifier(&format!("child__{}", field.name.text))
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let selected_parent = parent
            .fields
            .iter()
            .map(|field| {
                format!(
                    "\"owner_page\".{} AS {}",
                    sql_identifier(&field.name.text),
                    sql_identifier(&format!("owner__{}", field.name.text))
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let (predicate, limit, offset) = match dialect {
            SqlDialect::Postgres => ("$1", "$2", "$3"),
            SqlDialect::Sqlite => ("?1", "?2", "?3"),
        };
        let child_visibility = self
            .lifecycle_visibility_sql(child_name)
            .map(|predicate| format!(" AND ({predicate})"))
            .unwrap_or_default();
        let parent_visibility = self
            .lifecycle_visibility_sql(parent_name)
            .map(|predicate| format!(" AND ({predicate})"))
            .unwrap_or_default();
        format!(
            "WITH \"child_page\" AS (SELECT {child_fields} FROM {child_table} WHERE {} = {predicate}{child_visibility} AND (__JADPO_CHILD_POLICY__) AND EXISTS (SELECT 1 FROM {parent_table} WHERE {}.{} = {child_table}.{}{parent_visibility} AND (__JADPO_OWNER_POLICY__)) ORDER BY {} {direction} LIMIT {limit} OFFSET {offset}) SELECT {selected_child}, {selected_parent} FROM \"child_page\" INNER JOIN {parent_table} AS \"owner_page\" ON \"child_page\".{} = \"owner_page\".{} ORDER BY \"child_page\".{} {direction}",
            sql_identifier(predicate_field),
            parent_table,
            sql_identifier(target_field),
            sql_identifier(reference_field),
            sql_identifier(order_field),
            sql_identifier(reference_field),
            sql_identifier(target_field),
            sql_identifier(order_field)
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn select_many_with_inverse_sql(
        &self,
        parent_name: &str,
        parent: &RecordDeclaration,
        predicate_field: &str,
        parent_order_field: &str,
        parent_direction: &str,
        child_name: &str,
        child: &RecordDeclaration,
        via_field: &str,
        target_field: &str,
        child_order_field: &str,
        child_direction: &str,
        dialect: SqlDialect,
    ) -> String {
        let parent_table = sql_identifier(&snake_case(parent_name));
        let child_table = sql_identifier(&snake_case(child_name));
        let parent_fields = parent
            .fields
            .iter()
            .map(|field| sql_identifier(&field.name.text))
            .collect::<Vec<_>>()
            .join(", ");
        let child_fields = child
            .fields
            .iter()
            .map(|field| sql_identifier(&field.name.text))
            .collect::<Vec<_>>()
            .join(", ");
        let selected_parent = parent
            .fields
            .iter()
            .map(|field| {
                format!(
                    "\"parent_page\".{} AS {}",
                    sql_identifier(&field.name.text),
                    sql_identifier(&format!("parent__{}", field.name.text))
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let selected_child = child
            .fields
            .iter()
            .map(|field| {
                format!(
                    "\"child_page\".{} AS {}",
                    sql_identifier(&field.name.text),
                    sql_identifier(&format!("child__{}", field.name.text))
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let (predicate, parent_limit, parent_offset, child_limit, child_offset) = match dialect {
            SqlDialect::Postgres => ("$1", "$2", "$3", "$4", "$5"),
            SqlDialect::Sqlite => ("?1", "?2", "?3", "?4", "?5"),
        };
        let parent_visibility = self
            .lifecycle_visibility_sql(parent_name)
            .map(|predicate| format!(" AND ({predicate})"))
            .unwrap_or_default();
        let child_visibility = self
            .lifecycle_visibility_sql(child_name)
            .map(|predicate| format!(" AND ({predicate})"))
            .unwrap_or_default();
        format!(
            "WITH \"parent_page\" AS (SELECT {parent_fields} FROM {parent_table} WHERE {} = {predicate}{parent_visibility} ORDER BY {} {parent_direction} LIMIT {parent_limit} OFFSET {parent_offset}), \"child_page\" AS (SELECT {child_fields}, ROW_NUMBER() OVER (PARTITION BY {} ORDER BY {} {child_direction}) AS \"__row_number\" FROM {child_table} WHERE {} IN (SELECT {} FROM \"parent_page\"){child_visibility}) SELECT {selected_parent}, {selected_child} FROM \"parent_page\" LEFT JOIN \"child_page\" ON \"child_page\".{} = \"parent_page\".{} AND \"child_page\".\"__row_number\" > {child_offset} AND \"child_page\".\"__row_number\" <= ({child_offset} + {child_limit}) ORDER BY \"parent_page\".{} {parent_direction}, \"child_page\".{} {child_direction}",
            sql_identifier(predicate_field),
            sql_identifier(parent_order_field),
            sql_identifier(via_field),
            sql_identifier(child_order_field),
            sql_identifier(via_field),
            sql_identifier(target_field),
            sql_identifier(via_field),
            sql_identifier(target_field),
            sql_identifier(parent_order_field),
            sql_identifier(child_order_field)
        )
    }

    fn sql_type(&self, reference: &TypeReference, dialect: SqlDialect) -> &'static str {
        match self.representation_type(&type_name(reference)).as_str() {
            "Bool" => match dialect {
                SqlDialect::Postgres => "BOOLEAN",
                SqlDialect::Sqlite => "INTEGER",
            },
            "Int" => "BIGINT",
            "Decimal" => match dialect {
                SqlDialect::Postgres => "NUMERIC",
                SqlDialect::Sqlite => "REAL",
            },
            "Instant" => match dialect {
                SqlDialect::Postgres => "TIMESTAMPTZ(3)",
                SqlDialect::Sqlite => "BIGINT",
            },
            "CalendarDate" => match dialect {
                SqlDialect::Postgres => "DATE",
                SqlDialect::Sqlite => "TEXT",
            },
            "Duration" => "BIGINT",
            "Time" => match dialect {
                SqlDialect::Postgres => "JSONB",
                SqlDialect::Sqlite => "TEXT",
            },
            "Text" | "Uuid" | "Zone" | "Locale" => "TEXT",
            _ => "BLOB",
        }
    }

    fn representation_type(&self, raw_name: &str) -> String {
        let mut current = raw_name.trim_end_matches('?').to_owned();
        if self.enums.contains_key(&current) {
            return "Text".to_owned();
        }
        let mut visited = BTreeSet::new();
        while visited.insert(current.clone()) {
            let Some(node) = self.project.semantics.node(&current) else {
                break;
            };
            let Some(edge) = self
                .project
                .semantics
                .refinements
                .iter()
                .find(|edge| edge.refined == node.id)
            else {
                break;
            };
            current.clone_from(&self.project.semantics.nodes[edge.parent.0 as usize].name);
        }
        current
    }

    fn runtime_prelude(&self, output: &mut String) {
        line(output, "type JsonObject = Record<string, unknown>;");
        line(
            output,
            "declare const safeOperationalTextBrand: unique symbol;",
        );
        line(
            output,
            "type SafeOperationalText = string & { readonly [safeOperationalTextBrand]: true };",
        );
        line(
            output,
            "type SafeOperationalValue = SafeOperationalText | number | boolean | null;",
        );
        line(output, "export type OperationalLogEvent = {");
        line(output, "  schemaVersion: 1;");
        line(output, "  kind: \"operational_log_event\";");
        line(output, "  eventName: string;");
        line(output, "  classification: string;");
        line(output, "  requestId: string;");
        line(output, "  traceId: string | null;");
        line(output, "  semanticOperationId: string;");
        line(output, "  sourceRevision: string;");
        line(
            output,
            "  attributes: Record<string, SafeOperationalValue>;",
        );
        line(output, "};");
        line(output, "");
        line(output, "class ValidationError extends Error {}");
        line(output, "class RequestSyntaxError extends Error {}");
        line(output, "");
        if self.has_route_headers() {
            line(
                output,
                "const rawHeaderCounts = new WeakMap<Request, ReadonlyMap<string, number>>();",
            );
            line(
                output,
                "function declaredHeaderValue(request: Request, name: string): string | null {",
            );
            line(output, "  const counts = rawHeaderCounts.get(request);");
            line(output, "  if (counts === undefined) throw new RequestSyntaxError(\"raw header metadata is unavailable\");");
            line(output, "  if ((counts.get(name.toLowerCase()) ?? 0) > 1) throw new RequestSyntaxError(\"duplicate declared header\");");
            line(output, "  return request.headers.get(name);");
            line(output, "}");
            line(output, "");
            line(
                output,
                "async function requestFromNode(incoming: IncomingMessage): Promise<Request> {",
            );
            line(output, "  const headers = new Headers();");
            line(output, "  const counts = new Map<string, number>();");
            line(
                output,
                "  for (let index = 0; index < incoming.rawHeaders.length; index += 2) {",
            );
            line(output, "    const name = incoming.rawHeaders[index]; const value = incoming.rawHeaders[index + 1] ?? \"\";");
            line(output, "    headers.append(name, value); const folded = name.toLowerCase(); counts.set(folded, (counts.get(folded) ?? 0) + 1);");
            line(output, "  }");
            line(output, "  const method = incoming.method ?? \"GET\";");
            line(output, "  const body = method === \"GET\" || method === \"HEAD\" ? undefined : Readable.toWeb(incoming) as ReadableStream<Uint8Array>;");
            line(output, "  const request = new Request(`http://localhost${incoming.url ?? \"/\"}`, { method, headers, body });");
            line(
                output,
                "  rawHeaderCounts.set(request, counts); return request;",
            );
            line(output, "}");
            line(output, "");
            line(output, "async function writeNodeResponse(outgoing: ServerResponse, response: Response): Promise<void> {");
            line(output, "  outgoing.statusCode = response.status;");
            line(
                output,
                "  for (const [name, value] of response.headers) outgoing.setHeader(name, value);",
            );
            line(output, "  const setCookies = (response.headers as Headers & { getSetCookie?: () => string[] }).getSetCookie?.() ?? [];");
            line(
                output,
                "  if (setCookies.length > 0) outgoing.setHeader(\"set-cookie\", setCookies);",
            );
            line(
                output,
                "  outgoing.end(Buffer.from(await response.arrayBuffer()));",
            );
            line(output, "}");
            line(output, "");
            line(output, "export function createApplicationServer() {");
            line(
                output,
                "  return createServer(async (incoming, outgoing) => {",
            );
            line(output, "    try { await writeNodeResponse(outgoing, await handleRequest(await requestFromNode(incoming))); }");
            line(output, "    catch { outgoing.statusCode = 500; outgoing.setHeader(\"content-type\", \"application/json\"); outgoing.end(JSON.stringify({ error: { code: \"internal_fault\", message: \"An internal error occurred.\" } })); }");
            line(
                output,
                "    finally { if (!incoming.complete) incoming.resume(); }",
            );
            line(output, "  });");
            line(output, "}");
            line(output, "");
        }
        line(output, "class AuthorizationFault extends Error {");
        line(
            output,
            "  constructor() { super(\"not_permitted\"); this.name = \"AuthorizationFault\"; }",
        );
        line(output, "}");
        line(output, "");
        line(output, "class DomainFailure extends Error {");
        line(output, "  constructor(");
        line(output, "    readonly failureName: string,");
        line(output, "    readonly publicContext: JsonObject,");
        line(output, "    readonly internalContext: JsonObject,");
        line(output, "  ) {");
        line(output, "    super(failureName);");
        line(output, "  }");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function decodeQueryComponent(value: string): string {",
        );
        line(output, "  if (/%(?![0-9A-Fa-f]{2})/u.test(value)) throw new RequestSyntaxError(\"invalid percent escape\");");
        line(output, "  try { return decodeURIComponent(value.replaceAll(\"+\", \" \")); } catch { throw new RequestSyntaxError(\"invalid query encoding\"); }");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function parseRawQuery(url: string): Map<string, string[]> {",
        );
        line(output, "  const question = url.indexOf(\"?\");");
        line(output, "  const output = new Map<string, string[]>();");
        line(output, "  if (question < 0) return output;");
        line(output, "  const hash = url.indexOf(\"#\", question + 1);");
        line(
            output,
            "  const raw = url.slice(question + 1, hash < 0 ? undefined : hash);",
        );
        line(output, "  if (raw.length === 0) return output;");
        line(output, "  for (const item of raw.split(\"&\")) {");
        line(
            output,
            "    if (item.length === 0) throw new RequestSyntaxError(\"empty query item\");",
        );
        line(output, "    const equals = item.indexOf(\"=\");");
        line(
            output,
            "    const name = decodeQueryComponent(equals < 0 ? item : item.slice(0, equals));",
        );
        line(
            output,
            "    const value = decodeQueryComponent(equals < 0 ? \"\" : item.slice(equals + 1));",
        );
        line(output, "    const values = output.get(name) ?? []; values.push(value); output.set(name, values);");
        line(output, "  }");
        line(output, "  return output;");
        line(output, "}");
        line(output, "");
        line(output, "function queryInteger(value: string): number { if (!/^-?(?:0|[1-9][0-9]*)$/u.test(value)) throw new RequestSyntaxError(\"invalid integer\"); const result = Number(value); if (!Number.isSafeInteger(result)) throw new ValidationError(\"integer outside safe range\"); return result; }");
        line(output, "function queryDecimal(value: string): number { if (!/^-?(?:0|[1-9][0-9]*)(?:\\.[0-9]+)?$/u.test(value)) throw new RequestSyntaxError(\"invalid decimal\"); return Number(value); }");
        line(output, "function queryBoolean(value: string): boolean { if (value === \"true\") return true; if (value === \"false\") return false; throw new RequestSyntaxError(\"invalid boolean\"); }");
        line(output, "function queryJson(value: string): unknown { try { return JSON.parse(value); } catch { throw new RequestSyntaxError(\"invalid JSON query value\"); } }");
        line(output, "");
        line(
            output,
            "function matchRoutePath(template: string, actual: string): JsonObject | null {",
        );
        line(output, "  const expected = template.split(\"/\");");
        line(output, "  const received = actual.split(\"/\");");
        line(
            output,
            "  if (expected.length !== received.length) return null;",
        );
        line(output, "  const values: JsonObject = {};");
        line(
            output,
            "  for (let index = 0; index < expected.length; index += 1) {",
        );
        line(output, "    const segment = expected[index];");
        line(output, "    const value = received[index];");
        line(
            output,
            "    if (segment.startsWith(\"{\") && segment.endsWith(\"}\")) {",
        );
        line(output, "      try { values[segment.slice(1, -1)] = decodeURIComponent(value); } catch { return null; }");
        line(output, "    } else if (segment !== value) return null;");
        line(output, "  }");
        line(output, "  return values;");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function invalid(path: string, expectation: string): never {",
        );
        line(
            output,
            "  throw new ValidationError(`${path} must satisfy ${expectation}`);",
        );
        line(output, "}");
        line(output, "");
        line(
            output,
            "function expectObject(value: unknown, path: string): JsonObject {",
        );
        line(
            output,
            "  if (typeof value !== \"object\" || value === null || Array.isArray(value)) invalid(path, \"a closed object\");",
        );
        line(output, "  return value as JsonObject;");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function rejectUnknownFields(value: JsonObject, allowed: readonly string[], path: string): void {",
        );
        line(output, "  const allowedSet = new Set(allowed);");
        line(output, "  for (const key of Object.keys(value)) {");
        line(
            output,
            "    if (!allowedSet.has(key)) invalid(`${path}.${key}`, \"a declared field\");",
        );
        line(output, "  }");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function hasOwn(value: JsonObject, key: string): boolean {",
        );
        line(
            output,
            "  return Object.prototype.hasOwnProperty.call(value, key);",
        );
        line(output, "}");
        line(output, "");
        line(
            output,
            "function validateText(value: unknown, path: string): string {",
        );
        line(
            output,
            "  if (typeof value !== \"string\") invalid(path, \"Text\");",
        );
        line(output, "  return value;");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function validateBool(value: unknown, path: string): boolean {",
        );
        line(
            output,
            "  if (typeof value !== \"boolean\") invalid(path, \"Bool\");",
        );
        line(output, "  return value;");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function validateInt(value: unknown, path: string): number {",
        );
        line(
            output,
            "  if (typeof value !== \"number\" || !Number.isInteger(value)) invalid(path, \"Int\");",
        );
        line(output, "  return value;");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function validateDecimal(value: unknown, path: string): number {",
        );
        line(
            output,
            "  if (typeof value !== \"number\" || !Number.isFinite(value)) invalid(path, \"Decimal\");",
        );
        line(output, "  return value;");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function validateUuid(value: unknown, path: string): string {",
        );
        line(
            output,
            "  if (typeof value !== \"string\" || !/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(value)) invalid(path, \"Uuid\");",
        );
        line(output, "  return value;");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function validateInstant(value: unknown, path: string): Instant {",
        );
        line(output, "  if (typeof value !== \"string\") invalid(path, \"Instant with an explicit RFC 3339 offset\");");
        line(output, "  const match = /^([0-9]{4})-([0-9]{2})-([0-9]{2})T(?:[01][0-9]|2[0-3]):[0-5][0-9]:[0-5][0-9](?:\\.[0-9]{1,3})?(?:Z|[+-](?:[01][0-9]|2[0-3]):[0-5][0-9])$/u.exec(value);");
        line(output, "  if (match === null || value.endsWith(\"-00:00\")) invalid(path, \"Instant with an explicit RFC 3339 offset\");");
        line(output, "  const year = Number(match[1]); const month = Number(match[2]); const day = Number(match[3]);");
        line(output, "  const calendarCheck = new Date(0); calendarCheck.setUTCFullYear(year, month - 1, day); calendarCheck.setUTCHours(0, 0, 0, 0);");
        line(output, "  if (year < 1 || calendarCheck.getUTCFullYear() !== year || calendarCheck.getUTCMonth() !== month - 1 || calendarCheck.getUTCDate() !== day) invalid(path, \"real RFC 3339 calendar date\");");
        line(output, "  const milliseconds = Date.parse(value);");
        line(
            output,
            "  if (!Number.isFinite(milliseconds)) invalid(path, \"representable Instant\");",
        );
        line(
            output,
            "  const canonical = new Date(milliseconds).toISOString();",
        );
        line(
            output,
            "  if (!/^[0-9]{4}-/u.test(canonical) || canonical.startsWith(\"0000-\")) invalid(path, \"portable Instant range\");",
        );
        line(output, "  return canonical as Instant;");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function validateCalendarDate(value: unknown, path: string): CalendarDate {",
        );
        line(output, "  if (typeof value !== \"string\" || !/^[0-9]{4}-[0-9]{2}-[0-9]{2}$/u.test(value)) invalid(path, \"CalendarDate\");");
        line(
            output,
            "  const [year, month, day] = value.split(\"-\").map(Number);",
        );
        line(
            output,
            "  const check = new Date(0); check.setUTCFullYear(year, month - 1, day); check.setUTCHours(0, 0, 0, 0);",
        );
        line(output, "  if (year < 1 || check.getUTCFullYear() !== year || check.getUTCMonth() !== month - 1 || check.getUTCDate() !== day) invalid(path, \"real calendar date\");");
        line(output, "  return value as CalendarDate;");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function validateZone(value: unknown, path: string): Zone {",
        );
        line(output, "  if (typeof value !== \"string\" || !jadpoZones.has(value)) invalid(path, \"declared canonical IANA Zone\");");
        line(output, "  return value as Zone;");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function validateLocale(value: unknown, path: string): Locale {",
        );
        line(output, "  if (typeof value !== \"string\" || !jadpoLocales.has(value)) invalid(path, \"application-declared Locale\");");
        line(output, "  return value as Locale;");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function validateTime(value: unknown, path: string): Time {",
        );
        line(output, "  const object = expectObject(value, path);");
        line(
            output,
            "  rejectUnknownFields(object, [\"instant\", \"zone\"], path);",
        );
        line(output, "  return Object.freeze({ instant: validateInstant(object.instant, `${path}.instant`), zone: validateZone(object.zone, `${path}.zone`) });");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function durationMilliseconds(value: unknown, path: string): number {",
        );
        line(
            output,
            "  if (typeof value !== \"string\") invalid(path, \"Duration\");",
        );
        line(output, "  const match = /^(-)?PT(?:(\\d+)H)?(?:(\\d+)M)?(?:(\\d+)(?:\\.(\\d{1,3}))?S)?$/u.exec(value);");
        line(output, "  if (match === null || (match[2] === undefined && match[3] === undefined && match[4] === undefined)) invalid(path, \"ISO 8601 time-only Duration\");");
        line(output, "  const milliseconds = ((Number(match[2] ?? 0) * 60 + Number(match[3] ?? 0)) * 60 + Number(match[4] ?? 0)) * 1000 + Number((match[5] ?? \"\").padEnd(3, \"0\"));");
        line(
            output,
            "  const signed = match[1] === undefined ? milliseconds : -milliseconds;",
        );
        line(
            output,
            "  if (!Number.isSafeInteger(signed)) invalid(path, \"portable Duration range\");",
        );
        line(output, "  return signed;");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function durationFromMilliseconds(milliseconds: number): Duration {",
        );
        line(output, "  if (!Number.isSafeInteger(milliseconds)) invalid(\"duration\", \"portable Duration range\");");
        line(output, "  const sign = milliseconds < 0 ? \"-\" : \"\";");
        line(output, "  let remaining = Math.abs(milliseconds); const hours = Math.floor(remaining / 3_600_000); remaining %= 3_600_000; const minutes = Math.floor(remaining / 60_000); remaining %= 60_000; const seconds = Math.floor(remaining / 1000); const fraction = remaining % 1000;");
        line(output, "  const values = `${hours === 0 ? \"\" : `${hours}H`}${minutes === 0 ? \"\" : `${minutes}M`}${seconds === 0 && fraction === 0 ? \"\" : `${seconds}${fraction === 0 ? \"\" : `.${String(fraction).padStart(3, \"0\").replace(/0+$/u, \"\")}`}S`}`;");
        line(
            output,
            "  return `${sign}PT${values === \"\" ? \"0S\" : values}` as Duration;",
        );
        line(output, "}");
        line(output, "");
        line(
            output,
            "function validateDuration(value: unknown, path: string): Duration {",
        );
        line(
            output,
            "  return durationFromMilliseconds(durationMilliseconds(value, path));",
        );
        line(output, "}");
        line(output, "");
        line(output, "");
        line(output, "function isEmail(value: string): boolean {");
        line(output, "  const parts = value.split(\"@\");");
        line(output, "  if (parts.length !== 2) return false;");
        line(output, "  const [local, domain] = parts;");
        line(
            output,
            "  return local.length > 0 && domain.includes(\".\") && !domain.startsWith(\".\") && !domain.endsWith(\".\") && !/\\s/u.test(value);",
        );
        line(output, "}");
        line(output, "");
        line(output, "function isUrl(value: string): boolean {");
        line(output, "  if (/\\s/u.test(value)) return false;");
        line(output, "  try { const parsed = new URL(value); return (parsed.protocol === \"http:\" || parsed.protocol === \"https:\") && (parsed.hostname === \"localhost\" || parsed.hostname.includes(\".\")); } catch { return false; }");
        line(output, "}");
        line(output, "");
        line(output, "function isIpv4(value: string): boolean {");
        line(output, "  const parts = value.split(\".\");");
        line(output, "  return parts.length === 4 && parts.every(part => /^[0-9]{1,3}$/u.test(part) && Number(part) <= 255);");
        line(output, "}");
        line(output, "");
        line(output, "function isIpAddress(value: string): boolean {");
        line(output, "  if (isIpv4(value)) return true;");
        line(output, "  const halves = value.split(\"::\");");
        line(output, "  if (halves.length > 2) return false;");
        line(
            output,
            "  const parts = halves.flatMap(half => half === \"\" ? [] : half.split(\":\"));",
        );
        line(output, "  let groups = 0;");
        line(output, "  for (const [index, part] of parts.entries()) {");
        line(output, "    if (part.includes(\".\")) {");
        line(
            output,
            "      if (index !== parts.length - 1 || !isIpv4(part)) return false;",
        );
        line(output, "      groups += 2;");
        line(output, "    } else {");
        line(
            output,
            "      if (!/^[0-9a-f]{1,4}$/iu.test(part)) return false;",
        );
        line(output, "      groups += 1;");
        line(output, "    }");
        line(output, "  }");
        line(
            output,
            "  return halves.length === 2 ? groups < 8 : groups === 8;",
        );
        line(output, "}");
        line(output, "");
        line(
            output,
            "function json(status: number, body: unknown, requestId: string): Response {",
        );
        line(output, "  return Response.json(body, {");
        line(output, "    status,");
        line(
            output,
            "    headers: { \"x-request-id\": requestId, \"cache-control\": \"no-store\" },",
        );
        line(output, "  });");
        line(output, "}");
        line(output, "");
        line(
            output,
            "function errorEnvelope(code: string, message: string, requestId: string, details?: JsonObject): JsonObject {",
        );
        line(output, "  return {");
        line(output, "    error: {");
        line(output, "      code,");
        line(output, "      message,");
        line(output, "      request_id: requestId,");
        line(
            output,
            "      ...(details === undefined ? {} : { details }),",
        );
        line(output, "    },");
        line(output, "  };");
        line(output, "}");
        line(output, "");
        line(
            output,
            "export function operationalEventToOpenTelemetry(event: OperationalLogEvent): JsonObject {",
        );
        line(output, "  return { name: event.eventName, attributes: { classification: event.classification, request_id: event.requestId, trace_id: event.traceId, semantic_operation_id: event.semanticOperationId, source_revision: event.sourceRevision, ...event.attributes } };");
        line(output, "}");
        line(output, "");
        line(
            output,
            "export function operationalEventToProvider(event: OperationalLogEvent): JsonObject {",
        );
        line(output, "  return { fingerprint: [event.classification, event.semanticOperationId, event.sourceRevision], tags: { operation: event.semanticOperationId, release: event.sourceRevision }, extra: event.attributes };");
        line(output, "}");
        line(output, "");
        line(
            output,
            "export function reportRuntimeFault(classification: string, semanticOperationId: string, sourceRevision: string, requestId: string, error: unknown): void {",
        );
        line(output, "  const event: OperationalLogEvent = {");
        line(output, "    schemaVersion: 1,");
        line(output, "    kind: \"operational_log_event\",");
        line(output, "    eventName: \"operation.failed\",");
        line(output, "    classification,");
        line(output, "    requestId,");
        line(output, "    traceId: null,");
        line(output, "    semanticOperationId,");
        line(output, "    sourceRevision,");
        line(output, "    attributes: {},");
        line(output, "  };");
        line(output, "  console.error(JSON.stringify(event));");
        line(
            output,
            "  if (Bun.env.JADPO_DEBUG_TARGET_STACKS === \"1\") console.error(error);",
        );
        line(output, "}");
        line(output, "");
    }

    fn type_aliases(&self, output: &mut String) {
        for name in ["Email", "Url", "IpAddress"] {
            line(
                output,
                &format!("type {name} = string & {{ readonly __brand_{name}: unique symbol }};"),
            );
        }
        line(output, "");
        for (name, declaration) in &self.enums {
            let tagged = enum_is_tagged(declaration);
            let variants = declaration
                .variants
                .iter()
                .map(|variant| {
                    if tagged {
                        let mut fields =
                            vec![format!("readonly tag: {}", ts_string(&variant.name.text))];
                        fields.extend(variant.fields.iter().map(|field| {
                            format!(
                                "{}{}: {}",
                                field.name.text,
                                if field.optional { "?" } else { "" },
                                self.ts_type(&field.field_type)
                            )
                        }));
                        format!("{{ {} }}", fields.join("; "))
                    } else {
                        ts_string(&variant.name.text)
                    }
                })
                .collect::<Vec<_>>()
                .join(" | ");
            line(output, &format!("type {name} = {variants};"));
        }
        if !self.enums.is_empty() {
            line(output, "");
        }
        for (name, declaration) in &self.types {
            line(
                output,
                &format!(
                    "type {name} = {} & {{ readonly __brand_{name}: unique symbol }};",
                    self.ts_type(&declaration.parent)
                ),
            );
        }
        if !self.types.is_empty() {
            line(output, "");
        }
        if let Some(principal) = self.principal {
            let variants = principal
                .variants
                .iter()
                .map(|variant| {
                    let mut fields = vec![format!(
                        "readonly tag: {}",
                        ts_string(variant.kind.as_str())
                    )];
                    fields.extend(variant.fields.iter().map(|field| {
                        format!(
                            "readonly {}{}: {}",
                            field.name.text,
                            if field.optional { "?" } else { "" },
                            self.ts_type(&field.field_type)
                        )
                    }));
                    format!("Readonly<{{ {} }}>", fields.join("; "))
                })
                .collect::<Vec<_>>()
                .join(" | ");
            line(
                output,
                &format!("type {} = {variants};", principal.name.text),
            );
            line(output, "");
        }
        for (name, declaration) in &self.records {
            line(output, &format!("type {name} = {{"));
            for field in &declaration.fields {
                let optional = if field.optional { "?" } else { "" };
                line(
                    output,
                    &format!(
                        "  {}{optional}: {};",
                        field.name.text,
                        self.ts_type(&field.field_type)
                    ),
                );
            }
            line(output, "};");
            line(output, "");
        }
    }

    fn principal_runtime(&self, output: &mut String) {
        let Some(principal) = self.principal else {
            return;
        };
        line(
            output,
            &format!(
                "function materializePrincipal(value: PolicyPrincipal | null): {} {{",
                principal.name.text
            ),
        );
        line(
            output,
            "  if (value === null) throw new AuthorizationFault();",
        );
        line(output, "  switch (value.kind) {");
        for variant in &principal.variants {
            line(
                output,
                &format!("    case {}:", ts_string(variant.kind.as_str())),
            );
            line(output, "      return Object.freeze({");
            line(
                output,
                &format!("        tag: {},", ts_string(variant.kind.as_str())),
            );
            for field in &variant.fields {
                let value = match field.name.text.as_str() {
                    "subject" => "value.subject".to_owned(),
                    "authentication_strength" => "value.authenticationStrength".to_owned(),
                    _ => format!("value.values[{}]", ts_string(&field.name.text)),
                };
                if field.optional
                    && !matches!(
                        field.name.text.as_str(),
                        "subject" | "authentication_strength"
                    )
                {
                    line(
                        output,
                        &format!(
                            "        ...(Object.prototype.hasOwnProperty.call(value.values, {}) ? {{ {}: {value} as {} }} : {{}}),",
                            ts_string(&field.name.text),
                            field.name.text,
                            self.ts_type(&field.field_type)
                        ),
                    );
                } else {
                    line(
                        output,
                        &format!(
                            "        {}: {value} as {},",
                            field.name.text,
                            self.ts_type(&field.field_type)
                        ),
                    );
                }
            }
            line(output, "      });");
        }
        line(output, "    default:");
        line(output, "      throw new AuthorizationFault();");
        line(output, "  }");
        line(output, "}");
        line(
            output,
            &format!(
                "function requirePrincipalVariant<K extends {}[\"tag\"]>(value: {}, tag: K): Extract<{}, {{ tag: K }}> {{ if (value.tag !== tag) throw new AuthorizationFault(); return value as Extract<{}, {{ tag: K }}>; }}",
                principal.name.text,
                principal.name.text,
                principal.name.text,
                principal.name.text,
            ),
        );
        line(output, "");
    }

    fn validators(&self, output: &mut String) {
        for (name, check, expectation) in [
            (
                "Email",
                "isEmail(candidate) && candidate.length <= 254",
                "Email",
            ),
            ("Url", "isUrl(candidate)", "Url"),
            ("IpAddress", "isIpAddress(candidate)", "IpAddress"),
        ] {
            line(
                output,
                &format!("function validate_{name}(value: unknown, path: string): {name} {{"),
            );
            line(output, "  const candidate = validateText(value, path);");
            line(
                output,
                &format!(
                    "  if (!({check})) invalid(path, {});",
                    ts_string(expectation)
                ),
            );
            line(output, &format!("  return candidate as {name};"));
            line(output, "}");
            line(output, "");
        }
        for (name, declaration) in &self.enums {
            line(
                output,
                &format!("function validate_{name}(value: unknown, path: string): {name} {{"),
            );
            if enum_is_tagged(declaration) {
                line(output, "  const object = expectObject(value, path);");
                line(
                    output,
                    "  const tag = validateText(object[\"tag\"], `${path}.tag`);",
                );
                line(output, "  switch (tag) {");
                for variant in &declaration.variants {
                    line(
                        output,
                        &format!("    case {}: {{", ts_string(&variant.name.text)),
                    );
                    let allowed = std::iter::once(ts_string("tag"))
                        .chain(
                            variant
                                .fields
                                .iter()
                                .map(|field| ts_string(&field.name.text)),
                        )
                        .collect::<Vec<_>>()
                        .join(", ");
                    line(
                        output,
                        &format!("      rejectUnknownFields(object, [{allowed}], path);"),
                    );
                    for field in &variant.fields {
                        if !field.optional {
                            line(
                                output,
                                &format!(
                                    "      if (!hasOwn(object, {})) invalid(`${{path}}.{}`, \"a required field\");",
                                    ts_string(&field.name.text),
                                    field.name.text
                                ),
                            );
                        }
                    }
                    line(output, "      return {");
                    line(
                        output,
                        &format!("        tag: {},", ts_string(&variant.name.text)),
                    );
                    for field in &variant.fields {
                        let value = self.field_validation_expression(
                            field,
                            &format!("object[{}]", ts_string(&field.name.text)),
                            &format!("`${{path}}.{}`", field.name.text),
                        );
                        if field.optional {
                            line(
                                output,
                                &format!(
                                    "        ...(hasOwn(object, {}) ? {{ {}: {value} }} : {{}}),",
                                    ts_string(&field.name.text),
                                    field.name.text
                                ),
                            );
                        } else {
                            line(output, &format!("        {}: {value},", field.name.text));
                        }
                    }
                    line(output, "      };");
                    line(output, "    }");
                }
                line(
                    output,
                    &format!("    default: return invalid(path, {});", ts_string(name)),
                );
                line(output, "  }");
            } else {
                let condition = declaration
                    .variants
                    .iter()
                    .map(|variant| format!("value !== {}", ts_string(&variant.name.text)))
                    .collect::<Vec<_>>()
                    .join(" && ");
                line(
                    output,
                    &format!("  if ({condition}) invalid(path, {});", ts_string(name)),
                );
                line(output, &format!("  return value as {name};"));
            }
            line(output, "}");
            line(output, "");
        }

        for (name, declaration) in &self.types {
            line(
                output,
                &format!("function validate_{name}(value: unknown, path: string): {name} {{"),
            );
            let parent = self.validation_expression(&declaration.parent, "value", "path");
            line(output, &format!("  const candidate = {parent};"));
            if self.reference_is_nullable(&declaration.parent) {
                line(
                    output,
                    &format!("  if (candidate === null) return null as {name};"),
                );
            }
            self.constraint_checks(output, "candidate", "path", &declaration.constraints, 2);
            line(output, &format!("  return candidate as {name};"));
            line(output, "}");
            line(output, "");
        }

        for (name, declaration) in &self.records {
            for field in &declaration.fields {
                let validator = self
                    .field_validator_name(&format!("{name}.{}", field.name.text))
                    .expect("record field has a validator");
                let field_type = self.ts_type(&field.field_type);
                line(
                    output,
                    &format!("function {validator}(value: unknown, path: string): {field_type} {{"),
                );
                let validated = self.field_validation_expression(field, "value", "path");
                line(output, &format!("  return {validated};"));
                line(output, "}");
                line(output, "");
            }
        }

        for (name, declaration) in &self.records {
            if declaration.is_persistent_entity() {
                line(
                    output,
                    &format!(
                        "function decodeDatabase_{name}(value: unknown, path: string): unknown {{"
                    ),
                );
                line(output, "  try {");
                line(output, "    const object = expectObject(value, path);");
                line(output, "    return {");
                for field in &declaration.fields {
                    let raw = format!("object[{}]", ts_string(&field.name.text));
                    let path = format!("`${{path}}.{}`", field.name.text);
                    let decoded = self.database_decode_expression(&field.field_type, &raw, &path);
                    line(output, &format!("      {}: {decoded},", field.name.text));
                }
                line(output, "    };");
                line(output, "  } catch (error) {");
                line(
                    output,
                    "    if (error instanceof PersistenceFault) throw error;",
                );
                line(
                    output,
                    &format!(
                        "    throw new PersistenceFault({}, \"data\", error);",
                        ts_string(&format!("decode.{name}"))
                    ),
                );
                line(output, "  }");
                line(output, "}");
                line(output, "");
            }
            line(
                output,
                &format!("function validate_{name}(value: unknown, path: string): {name} {{"),
            );
            if declaration.is_persistent_entity() {
                line(output, &format!("  const object = expectObject(path.startsWith(\"database.\") ? decodeDatabase_{name}(value, path) : value, path);"));
            } else {
                line(output, "  const object = expectObject(value, path);");
            }
            let fields = declaration
                .fields
                .iter()
                .map(|field| ts_string(&field.name.text))
                .collect::<Vec<_>>()
                .join(", ");
            line(
                output,
                &format!("  rejectUnknownFields(object, [{fields}], path);"),
            );
            for field in &declaration.fields {
                if !field.optional && field.default.is_none() {
                    line(
                        output,
                        &format!(
                            "  if (!hasOwn(object, {})) invalid(`${{path}}.{}`, \"a required field\");",
                            ts_string(&field.name.text),
                            field.name.text
                        ),
                    );
                }
            }
            line(output, "  return {");
            for field in &declaration.fields {
                let value = self.field_validation_expression(
                    field,
                    &format!("object[{}]", ts_string(&field.name.text)),
                    &format!("`${{path}}.{}`", field.name.text),
                );
                if field.default.is_some() {
                    let default = field.default.as_ref().map_or_else(
                        || "null".to_owned(),
                        |literal| match literal.kind {
                            LiteralKind::None => "null".to_owned(),
                            _ => literal.text.clone(),
                        },
                    );
                    line(
                        output,
                        &format!(
                            "    {}: hasOwn(object, {}) ? {value} : {default},",
                            field.name.text,
                            ts_string(&field.name.text)
                        ),
                    );
                } else if field.optional {
                    line(
                        output,
                        &format!(
                            "    ...(hasOwn(object, {}) ? {{ {}: {value} }} : {{}}),",
                            ts_string(&field.name.text),
                            field.name.text
                        ),
                    );
                } else {
                    line(output, &format!("    {}: {value},", field.name.text));
                }
            }
            line(output, "  };");
            line(output, "}");
            line(output, "");
        }
    }

    fn failure_contracts(&self, output: &mut String) {
        line(output, "const failureContracts = {");
        for contract in &self.project.failures.contracts {
            line(output, &format!("  {}: {{", contract.name));
            line(output, &format!("    code: {},", ts_string(&contract.code)));
            line(
                output,
                &format!(
                    "    status: {},",
                    bun_failure_http_status(&contract.kind, contract.http_status)
                ),
            );
            line(
                output,
                &format!(
                    "    message: {},",
                    contract
                        .message
                        .as_deref()
                        .map(ts_string)
                        .unwrap_or_else(|| "null".to_owned())
                ),
            );
            let fields = contract
                .public_fields
                .iter()
                .map(|field| ts_string(field))
                .collect::<Vec<_>>()
                .join(", ");
            line(output, &format!("    publicFields: [{fields}],"));
            line(output, "  },");
        }
        line(output, "} as const;");
        line(output, "");
    }

    fn invoke_authorization_runtime(&self, output: &mut String) {
        let operations = self
            .project
            .policy
            .operations
            .iter()
            .filter_map(|operation| {
                operation
                    .obligations
                    .iter()
                    .find(|obligation| {
                        obligation.source == "operation" && obligation.effect == "invoke"
                    })
                    .map(|obligation| (operation.operation.as_str(), obligation))
            })
            .collect::<Vec<_>>();
        if operations.is_empty() {
            return;
        }
        line(output, "async function authorizeInvoke(operation: string, principal: PolicyPrincipal | null, persistenceClient: any): Promise<boolean> {");
        line(output, "  switch (operation) {");
        for (operation, obligation) in operations {
            line(output, &format!("    case {}:", ts_string(operation)));
            if obligation
                .subjects
                .iter()
                .any(|subject| subject == "Access.public")
            {
                line(output, "      return true;");
                continue;
            }
            if obligation
                .subjects
                .iter()
                .any(|subject| subject == "Access.authenticated")
            {
                line(output, "      if (principal !== null) return true;");
            }
            if obligation
                .subjects
                .iter()
                .any(|subject| !subject.starts_with("Access."))
            {
                line(
                    output,
                    "      if (persistenceClient === null) return false;",
                );
                line(output, "      return await persistenceClient.withPolicy(principal, operation).allowsPolicy(operation, \"invoke\", {});");
            } else {
                line(output, "      return false;");
            }
        }
        line(output, "    default:");
        line(output, "      return false;");
        line(output, "  }");
        line(output, "}");
        line(output, "");
    }

    fn callable_implementations(&self, output: &mut String) {
        for declaration in self.callables.values() {
            let mutative = self.callable_is_mutative(&declaration.name.text);
            let replayable = mutative && self.callable_is_repeatable(&declaration.name.text);
            let mut parameters = declaration
                .parameters
                .iter()
                .map(|parameter| {
                    format!(
                        "{}: {}",
                        parameter.name.text,
                        self.ts_type(&parameter.parameter_type)
                    )
                })
                .collect::<Vec<_>>();
            parameters.push("__operation: OperationContext = captureOperation()".to_owned());
            let suspending = self.callable_may_suspend(&declaration.name.text);
            let needs_persistence = suspending && self.has_entities();
            if needs_persistence {
                parameters
                    .push("__persistence: typeof rootPersistence = rootPersistence".to_owned());
            }
            let parameters = parameters.join(", ");
            let (async_prefix, return_type) = if suspending {
                (
                    "async ",
                    format!("Promise<{}>", self.ts_type(&declaration.return_type)),
                )
            } else {
                ("", self.ts_type(&declaration.return_type))
            };
            line(
                output,
                &format!(
                    "{async_prefix}function {}({parameters}): {return_type} {{",
                    ts_callable_name(&declaration.name.text)
                ),
            );
            if !declaration
                .parameters
                .iter()
                .any(|parameter| parameter.name.text == "principal")
                && block_uses_runtime_name(&declaration.body, "principal")
            {
                line(
                    output,
                    "  const principal = materializePrincipal(__operation.principal);",
                );
            }
            if self.project.policy.active {
                line(
                    output,
                    &format!(
                        "  const __policyOperation = {};",
                        ts_string(&declaration.name.text)
                    ),
                );
            }
            if declaration.policy.is_some() && !mutative {
                let persistence_client = if needs_persistence {
                    "__persistence"
                } else {
                    "null"
                };
                line(
                    output,
                    &format!(
                        "  if (!(await authorizeInvoke(__policyOperation, __operation.principal, {persistence_client}))) throw new AuthorizationFault();"
                    ),
                );
            }
            if mutative {
                let operation_id = if self.project.policy.active {
                    "__policyOperation".to_owned()
                } else {
                    ts_string(&declaration.name.text)
                };
                if self.project.policy.active {
                    line(output, "  return __persistence.withDeadline(__operation.deadlineAt).withSignal(__operation.signal).withOperationTime(__operation.now).withPolicy(__operation.principal, __policyOperation).transaction(async persistence => {");
                } else {
                    line(
                        output,
                        "  return __persistence.withDeadline(__operation.deadlineAt).withSignal(__operation.signal).withOperationTime(__operation.now).transaction(async persistence => {",
                    );
                }
                if declaration.policy.is_some() {
                    line(output, &format!("    if (!(await authorizeInvoke({operation_id}, __operation.principal, persistence))) throw new AuthorizationFault();"));
                }
                self.block(output, &declaration.body, 4);
                line(output, &format!("  }}, {{ replayable: {} && __operation.retryAllowed, operationId: __operation.operationId, operation: {}, startedAt: __operation.monotonicStartedAt, deadlineAt: __operation.deadlineAt, signal: __operation.signal, refreshOperationTime: () => {{ __operation = refreshOperation(__operation); return __operation.now; }} }});", replayable, operation_id));
            } else {
                if needs_persistence {
                    if self.project.policy.active {
                        line(output, "  const persistence = __persistence.withDeadline(__operation.deadlineAt).withSignal(__operation.signal).withPolicy(__operation.principal, __policyOperation);");
                    } else {
                        line(output, "  const persistence = __persistence.withDeadline(__operation.deadlineAt).withSignal(__operation.signal);");
                    }
                }
                self.block(output, &declaration.body, 2);
            }
            line(output, "}");
            line(output, "");
        }
    }

    fn service_operation_implementations(&self, output: &mut String) {
        for effect in &self.project.semantics.external_effects {
            let qualified_name = format!("{}.{}", effect.service, effect.operation);
            let function_name = format!("__service_{}", ts_callable_name(&qualified_name));
            let input = &effect.input;
            let output_type = &effect.output;
            let credential_field = effect
                .credential_slot
                .strip_prefix("config.")
                .expect("checked service credential slot is config-owned");
            let recipient_failure = effect
                .outcome_mappings
                .iter()
                .find(|(source, _)| source == "provider.invalid_recipient")
                .map(|(_, target)| target.as_str())
                .expect("checked service contract has recipient mapping");
            let unavailable_failure = effect
                .outcome_mappings
                .iter()
                .find(|(source, _)| source == "provider.rate_limited")
                .map(|(_, target)| target.as_str())
                .expect("checked service contract has rate-limit mapping");

            line(output, &format!("async function {function_name}(input: {input}, __operation: OperationContext, __deliveryCredential: string | null = null): Promise<{output_type}> {{"));
            line(
                output,
                "  if (__deliveryCredential === null && configuration === undefined) configuration = loadConfiguration(Bun.env);",
            );
            line(
                output,
                &format!(
                    "  const checkedInput = validate_{input}(input, {});",
                    ts_string(&format!("service.{qualified_name}.input"))
                ),
            );
            line(output, &format!("  if (__operation.testServiceFakes !== null && !Object.prototype.hasOwnProperty.call(__operation.testServiceFakes, {})) throw new ServiceAdapterFault(\"outcome_unknown\");", ts_string(&qualified_name)));
            line(output, &format!("  const result = await invokeReferenceMail(checkedInput as unknown as Readonly<Record<string, unknown>>, __deliveryCredential ?? configuration.{credential_field}, __operation, __operation.testServiceFakes?.[{}] ?? null);", ts_string(&qualified_name)));
            line(
                output,
                "  if (result.kind === \"recipient_rejected\") throw new DomainFailure(",
            );
            line(
                output,
                &format!("    {}, {{}}, {{}},", ts_string(recipient_failure)),
            );
            line(output, "  );");
            line(
                output,
                "  if (result.kind === \"temporarily_unavailable\") throw new DomainFailure(",
            );
            line(
                output,
                &format!("    {}, {{}}, {{}},", ts_string(unavailable_failure)),
            );
            line(output, "  );");
            line(
                output,
                &format!(
                    "  return validate_{output_type}({{ accepted_at: result.accepted_at }}, {});",
                    ts_string(&format!("service.{qualified_name}.output"))
                ),
            );
            line(output, "}");
            line(output, "");
        }
    }

    fn test_implementations(&self, output: &mut String) {
        for (index, test) in self.tests.iter().enumerate() {
            line(
                output,
                &format!("async function jadpoTest{index}(): Promise<void> {{"),
            );
            let fixed_clock = test
                .fixture
                .as_ref()
                .and_then(|fixture| self.fixtures.get(&fixture.text))
                .and_then(|fixture| fixture.clock.as_ref())
                .map(|clock| self.expression(clock))
                .unwrap_or_else(|| {
                    "validateInstant(\"2000-01-01T00:00:00.000Z\", \"test.clock\")".to_owned()
                });
            line(
                output,
                &format!("  let __testClockNow: Instant = {fixed_clock};"),
            );
            let fixture_service_fakes = test
                .fixture
                .as_ref()
                .and_then(|fixture| self.fixtures.get(&fixture.text))
                .map(|fixture| fixture.service_fakes.as_slice())
                .unwrap_or_default();
            let mut fake_outcomes = BTreeMap::<String, Vec<String>>::new();
            for fake in fixture_service_fakes {
                for outcome in &fake.outcomes {
                    let operation = format!("{}.{}", fake.service.text, outcome.operation.text);
                    let encoded = match &outcome.value {
                        jadpo_syntax::FixtureServiceFakeValue::Accepted(value) => format!(
                            "{{ kind: \"accepted\", value: {} }}",
                            self.expression(value)
                        ),
                        jadpo_syntax::FixtureServiceFakeValue::Declared(value) => format!(
                            "{{ kind: \"declared\", name: {} }}",
                            ts_string(
                                &value
                                    .path
                                    .iter()
                                    .map(|part| part.text.as_str())
                                    .collect::<Vec<_>>()
                                    .join(".")
                            )
                        ),
                    };
                    fake_outcomes.entry(operation).or_default().push(encoded);
                }
            }
            {
                line(output, "  const __testServiceFakes: TestServiceFakes = {");
                for (operation, outcomes) in &fake_outcomes {
                    line(
                        output,
                        &format!(
                            "    {}: {{ outcomes: [{}], cursor: {{ index: 0 }}, elapsedMs: 0 }},",
                            ts_string(operation),
                            outcomes.join(", ")
                        ),
                    );
                }
                line(output, "  };");
            }
            let test_service_fakes = ", null, __testServiceFakes";
            line(
                output,
                &format!("  const __operation = captureOperation(__testClockNow, null, () => __testClockNow, true{test_service_fakes});"),
            );
            let fixture_configuration = test
                .fixture
                .as_ref()
                .and_then(|fixture| self.fixtures.get(&fixture.text))
                .and_then(|fixture| fixture.configuration.as_ref());
            let body_indent = if fixture_configuration.is_some() {
                4
            } else {
                2
            };
            if let Some(values) = fixture_configuration {
                line(output, "  const __previousConfiguration = configuration;");
                line(output, "  configuration = {");
                if let Some(configuration) = self.configuration {
                    for field in &configuration.fields {
                        let supplied = values
                            .iter()
                            .find(|value| value.name.text == field.name.text);
                        let value = supplied.map_or_else(
                            || {
                                field.default.as_ref().map_or_else(
                                    || "undefined".to_owned(),
                                    |default| match default.kind {
                                        ConfigDefaultKind::String => {
                                            ts_string(unquote(&default.text))
                                        }
                                        ConfigDefaultKind::Integer
                                        | ConfigDefaultKind::Decimal
                                        | ConfigDefaultKind::Boolean => default.text.clone(),
                                        ConfigDefaultKind::Duration => format!(
                                            "normalizeConfigurationDuration({}, {})",
                                            ts_string(&default.text),
                                            ts_string(&format!(
                                                "fixture.config.{}",
                                                field.name.text
                                            ))
                                        ),
                                    },
                                )
                            },
                            |value| self.fixture_configuration_expression(&value.value),
                        );
                        let validated = self.validation_expression(
                            &field.field_type,
                            &value,
                            &ts_string(&format!("fixture.config.{}", field.name.text)),
                        );
                        line(output, &format!("    {}: {validated},", field.name.text));
                    }
                }
                line(output, "  };");
                line(output, "  try {");
            }
            if self.has_persistence_operations() {
                line(
                    output,
                    &format!(
                        "{}const __testPersistence = createIsolatedTestPersistence();",
                        " ".repeat(body_indent)
                    ),
                );
                line(
                    output,
                    &format!(
                        "{}const persistence = __testPersistence.client;",
                        " ".repeat(body_indent)
                    ),
                );
                line(output, &format!("{}try {{", " ".repeat(body_indent)));
                self.block(output, &test.body, body_indent + 2);
                line(output, &format!("{}}} finally {{", " ".repeat(body_indent)));
                line(
                    output,
                    &format!("{}__testPersistence.close();", " ".repeat(body_indent + 2)),
                );
                line(output, &format!("{}}}", " ".repeat(body_indent)));
            } else {
                self.block(output, &test.body, body_indent);
            }
            if fixture_configuration.is_some() {
                line(output, "  } finally {");
                line(output, "    configuration = __previousConfiguration;");
                line(output, "  }");
            }
            line(output, "}");
            line(output, "");
        }
        if self.tests.is_empty() {
            return;
        }
        line(output, "export async function runTests() {");
        line(output, "  const tests = [");
        for (index, test) in self.tests.iter().enumerate() {
            let surface = if block_contains_test_call(&test.body) {
                "callable"
            } else {
                "pure"
            };
            line(
                output,
                &format!(
                    "    {{ name: {}, surface: {} as const, run: jadpoTest{index} }},",
                    ts_string(unquote(&test.name.text)),
                    ts_string(surface)
                ),
            );
        }
        line(output, "  ];");
        line(output, "  const results: Array<{ name: string; surface: \"pure\" | \"callable\" | \"route\" | \"job\"; status: \"passed\" | \"failed\"; diagnostic?: { code: string; message: string } }> = [];");
        line(output, "  for (const test of tests) {");
        line(output, "    try {");
        line(output, "      await test.run();");
        line(
            output,
            "      results.push({ name: test.name, surface: test.surface, status: \"passed\" });",
        );
        line(output, "    } catch (error) {");
        line(
            output,
            "      const message = error instanceof Error ? error.message : \"test failed\";",
        );
        line(output, "      results.push({ name: test.name, surface: test.surface, status: \"failed\", diagnostic: { code: \"TEST_FAILED\", message } });");
        line(output, "    }");
        line(output, "  }");
        line(
            output,
            "  const failed = results.filter(result => result.status === \"failed\").length;",
        );
        line(output, "  const surfaceCount = (surface: \"pure\" | \"callable\" | \"route\" | \"job\") => results.filter(result => result.surface === surface).length;");
        line(output, "  return { schema_version: 2, kind: \"test_report\", status: failed === 0 ? \"passed\" : \"failed\", summary: { total: results.length, passed: results.length - failed, failed }, evidence: { semantic_checks: \"compile\", generated_runtime_cases: 0, authored_business_cases: results.length, external_evidence: 0, surfaces: { pure: surfaceCount(\"pure\"), callable: surfaceCount(\"callable\"), route: surfaceCount(\"route\"), job: surfaceCount(\"job\") } }, results }; ");
        line(output, "}");
        line(output, "");
    }

    fn test_entrypoint(&self) -> String {
        let mut output = String::new();
        line(&mut output, "// Generated by Jadpo 0.0.1. Do not edit.");
        line(&mut output, "import { runTests } from \"./app.ts\";");
        line(&mut output, "");
        line(&mut output, "const report = await runTests();");
        line(&mut output, "console.log(JSON.stringify(report));");
        line(
            &mut output,
            "if (report.status === \"failed\") process.exit(1);",
        );
        output
    }

    fn has_authored_health_route(&self) -> bool {
        self.project.syntax.sources.iter().any(|source| {
            source.file.declarations.iter().any(|declaration| {
                matches!(
                    declaration,
                    Declaration::Route(route)
                        if route.method == HttpMethod::Get && route.path == "/health"
                )
            })
        })
    }

    fn has_route_headers(&self) -> bool {
        self.project.syntax.sources.iter().any(|source| {
            source.file.declarations.iter().any(|declaration| {
                matches!(declaration, Declaration::Route(route) if !route.headers.is_empty())
            })
        })
    }

    fn http_handler(&self, output: &mut String) {
        let readiness_advisories = self.readiness_advisories();
        line(
            output,
            "export async function handleRequest(request: Request): Promise<Response> {",
        );
        line(
            output,
            "  let __operation = captureOperation(null, null, undefined, false, request.signal);",
        );
        line(output, "  const requestId = `req_${crypto.randomUUID()}`;");
        line(
            output,
            &format!(
                "  const sourceRevision = {};",
                ts_string(&self.source_revision())
            ),
        );
        line(output, "  let semanticOperationId = \"http:unmatched\";");
        line(output, "  let checkedReadOnlyOperation = false;");
        line(output, "  let requestPath = \"<unparsed>\";");
        line(output, "  try {");
        line(output, "    const url = new URL(request.url);");
        line(output, "    requestPath = url.pathname;");

        let liveness = self.project.syntax.sources.iter()
            .flat_map(|source| &source.file.declarations)
            .find_map(|declaration| match declaration {
                Declaration::Route(route) if Self::is_liveness_route(route) => {
                    self.authored_liveness_response(route)
                }
                _ => None,
            })
            .unwrap_or_else(|| "{ status: \"live\" }".to_owned());
        line(output, &format!("    if (request.method === \"GET\" && url.pathname === \"/health/live\") return json(200, {liveness}, requestId);"));
        if self.has_entities() {
            line(output, &format!("    if (request.method === \"GET\" && url.pathname === \"/health/ready\") {{ const ready = await refreshDatabaseReadiness(); return json(ready ? 200 : 503, {{ status: ready ? \"ready\" : \"not_ready\", checks: {{ database: ready ? \"available\" : \"unavailable\" }}, advisories: {readiness_advisories} }}, requestId); }}"));
            line(output, "    if (!databaseIsReady()) return json(503, errorEnvelope(\"dependency_unavailable\", \"A required dependency is unavailable.\", requestId), requestId);");
        } else {
            line(output, &format!("    if (request.method === \"GET\" && url.pathname === \"/health/ready\") return json(200, {{ status: \"ready\", checks: {{}}, advisories: {readiness_advisories} }}, requestId);"));
        }

        if !self.has_authored_health_route() {
            line(
                output,
                "    if (request.method === \"GET\" && url.pathname === \"/health\") {",
            );
            if self.has_entities() {
                line(
                    output,
                    "      const ready = await refreshDatabaseReadiness();",
                );
                line(
                    output,
                    "      return json(ready ? 200 : 503, { ready }, requestId);",
                );
            } else {
                line(
                    output,
                    "      return json(200, { ready: true }, requestId);",
                );
            }
            line(output, "    }");
        }

        if self.has_service_credential_exchange() {
            for strategy in &self.authentication_strategies {
                let Some(exchange) = &strategy.exchange else {
                    continue;
                };
                let path = unquote(&exchange.path.text);
                line(
                    output,
                    &format!(
                        "    if (request.method === \"POST\" && url.pathname === {}) {{",
                        ts_string(path)
                    ),
                );
                line(
                    output,
                    &format!(
                        "      semanticOperationId = {};",
                        ts_string(&format!("route:POST:{path}"))
                    ),
                );
                line(
                    output,
                    "      if (!firstPartyAuthentication) throw new AuthenticationFault(\"authentication_misconfigured\", 503);",
                );
                line(
                    output,
                    "      if ((request.headers.get(\"cookie\")?.length ?? 0) > 16384 || (request.headers.get(\"authorization\")?.length ?? 0) > 8192) throw new AuthenticationFault(\"invalid_credentials\", 401);",
                );
                line(
                    output,
                    "      const presented = inventoryAuthenticationCredentials(request).filter(candidate => candidate.values.length > 0);",
                );
                line(
                    output,
                    "      if (presented.length === 0) throw new AuthenticationFault(\"authentication_required\", 401);",
                );
                line(
                    output,
                    "      if (presented.length !== 1 || presented[0].values.length !== 1) throw new AuthenticationFault(\"ambiguous_credentials\", 401);",
                );
                line(output, "      const candidate = presented[0];");
                line(
                    output,
                    &format!(
                        "      if (candidate.malformed || candidate.strategy !== {}) throw new AuthenticationFault(\"invalid_credentials\", 401);",
                        ts_string(&strategy.name.text)
                    ),
                );
                line(
                    output,
                    &format!(
                        "      const exchanged = await firstPartyAuthentication.exchangeServiceCredential({}, candidate.values[0], Date.parse(__operation.now));",
                        ts_string(&strategy.name.text)
                    ),
                );
                line(
                    output,
                    "      return json(200, { access_token: exchanged.credential, token_type: \"Bearer\", expires_at: new Date(exchanged.expires).toISOString() }, requestId);",
                );
                line(output, "    }");
            }
        }

        let mut route_id = 0usize;
        for source in &self.project.syntax.sources {
            for declaration in &source.file.declarations {
                let Declaration::Route(route) = declaration else {
                    continue;
                };
                if Self::is_liveness_route(route) {
                    continue;
                }
                let current_route_id = route_id;
                route_id += 1;
                line(
                    output,
                    &format!(
                        "    const routePath{} = matchRoutePath({}, url.pathname);",
                        current_route_id,
                        ts_string(&route.path)
                    ),
                );
                line(
                    output,
                    &format!(
                        "    if (request.method === {} && routePath{} !== null) {{",
                        ts_string(method_name(route.method)),
                        current_route_id
                    ),
                );
                line(
                    output,
                    &format!(
                        "      semanticOperationId = {};",
                        ts_string(&format!(
                            "route:{}:{}",
                            method_name(route.method),
                            route.path
                        ))
                    ),
                );
                if !route.public {
                    line(output, "      if (!firstPartyAuthentication) throw new AuthenticationFault(\"authentication_misconfigured\", 503);");
                    let mutates = route
                        .run
                        .as_ref()
                        .and_then(|run| resolved_callable_name(&run.callee.path, &self.callables))
                        .is_some_and(|name| self.callable_is_mutative(&name))
                        || route.inline_action.as_ref().is_some_and(|action| {
                            block_contains_mutation(
                                &action.body,
                                &self.callables,
                                &mut BTreeSet::new(),
                            )
                        });
                    line(output, &format!("      const authenticated = await firstPartyAuthentication.authenticate(request, {}, Date.parse(__operation.now), {mutates});", route.fresh_authority));
                    line(output, "      __operation = Object.freeze({ ...__operation, principal: authenticated });");
                    line(output, "      const current_principal = materializePrincipal(__operation.principal);");
                }
                // Queries cannot mutate or call actions. The repeatability walk
                // additionally excludes opaque calls and unknown effects. Set
                // this only after authentication's separate operational boundary.
                let read_only = route
                    .run
                    .as_ref()
                    .filter(|run| {
                        run.arguments.iter().all(|argument| {
                            matches!(argument, Expression::Name(_) | Expression::Literal(_))
                        }) && run.named_arguments.iter().all(|argument| {
                            matches!(argument.value, Expression::Name(_) | Expression::Literal(_))
                        })
                    })
                    .and_then(|run| resolved_callable_name(&run.callee.path, &self.callables))
                    .is_some_and(|name| {
                        self.callables.get(&name).is_some_and(|callable| {
                            callable.kind == jadpo_syntax::CallableKind::Query
                        }) && self.callable_is_repeatable(&name)
                            && !self.callable_is_mutative(&name)
                    });
                line(
                    output,
                    &format!("      checkedReadOnlyOperation = {read_only};"),
                );
                line(
                    output,
                    &format!(
                        "      __operation = allowOperationRetries(__operation, {});",
                        route.public
                    ),
                );
                if let Some(query_reference) = &route.query {
                    let query_name = self.schema_declaration(&type_name(query_reference));
                    let query_record = self
                        .records
                        .get(&query_name)
                        .expect("checked route query resolves to a record");
                    let allowed = query_record
                        .fields
                        .iter()
                        .map(|field| ts_string(&field.name.text))
                        .collect::<Vec<_>>()
                        .join(", ");
                    line(
                        output,
                        &format!("      let query: {};", self.ts_type(query_reference)),
                    );
                    line(output, "      try {");
                    line(
                        output,
                        "        const queryParameters = parseRawQuery(request.url);",
                    );
                    line(
                        output,
                        &format!("        const allowedQueryParameters = new Set([{allowed}]);"),
                    );
                    line(output, "        for (const name of queryParameters.keys()) if (!allowedQueryParameters.has(name)) throw new RequestSyntaxError(\"unknown query parameter\");");
                    line(output, "        const queryValue: JsonObject = {};");
                    for field in &query_record.fields {
                        let field_name = ts_string(&field.name.text);
                        let decoded = self.query_decode_expression(
                            &field.field_type,
                            &format!("values_{0}[0]", field.name.text),
                        );
                        line(
                            output,
                            &format!(
                                "        const values_{} = queryParameters.get({field_name});",
                                field.name.text
                            ),
                        );
                        line(
                            output,
                            &format!(
                                "        if ((values_{}?.length ?? 0) > 1) throw new RequestSyntaxError(\"duplicate query parameter\");",
                                field.name.text
                            ),
                        );
                        line(
                            output,
                            &format!(
                                "        if (values_{} !== undefined) queryValue[{field_name}] = {decoded};",
                                field.name.text
                            ),
                        );
                    }
                    line(
                        output,
                        &format!(
                            "        query = validate_{query_name}(queryValue, \"request.query\");"
                        ),
                    );
                    line(output, "      } catch (error) {");
                    line(output, "        if (error instanceof RequestSyntaxError) return json(400, errorEnvelope(\"invalid_request\", \"Request query syntax is invalid.\", requestId), requestId);");
                    line(output, "        if (error instanceof ValidationError) return json(422, errorEnvelope(\"invalid_value\", \"Request query value is invalid.\", requestId), requestId);");
                    line(output, "        throw error;");
                    line(output, "      }");
                }
                if !route.headers.is_empty() {
                    let header_type = route
                        .headers
                        .iter()
                        .map(|header| {
                            format!(
                                "{}{}: {}",
                                header.name.text,
                                if header.optional { "?" } else { "" },
                                self.ts_type(&header.field_type)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("; ");
                    line(output, &format!("      let headers: {{ {header_type} }};"));
                    line(output, "      try {");
                    line(output, "        headers = {");
                    for header in &route.headers {
                        let raw = format!("raw_header_{}", header.name.text);
                        let wire_name = unquote(&header.wire_name.text);
                        line(output, &format!("          ...((({raw}) => {{",));
                        let decoded = self.query_decode_expression(&header.field_type, &raw);
                        let validated = self.validation_expression(
                            &header.field_type,
                            &decoded,
                            &ts_string(&format!("request.headers.{}", header.name.text)),
                        );
                        if header.optional {
                            line(
                                output,
                                &format!(
                                    "            if ({raw} === null) return {{}}; return {{ {}: {validated} }};",
                                    header.name.text
                                ),
                            );
                        } else {
                            line(output, &format!("            if ({raw} === null) throw new RequestSyntaxError(\"missing declared header\"); return {{ {}: {validated} }};", header.name.text));
                        }
                        line(
                            output,
                            &format!(
                                "          }})(declaredHeaderValue(request, {}))),",
                                ts_string(wire_name)
                            ),
                        );
                    }
                    line(output, "        };");
                    line(output, "      } catch (error) {");
                    line(output, "        if (error instanceof RequestSyntaxError) return json(400, errorEnvelope(\"invalid_request\", \"Request header syntax is invalid.\", requestId), requestId);");
                    line(output, "        if (error instanceof ValidationError) return json(422, errorEnvelope(\"invalid_value\", \"Request header value is invalid.\", requestId), requestId);");
                    line(output, "        throw error;");
                    line(output, "      }");
                }
                if !route.path_fields.is_empty() {
                    let path_type = route
                        .path_fields
                        .iter()
                        .map(|field| {
                            format!("{}: {}", field.name.text, self.ts_type(&field.field_type))
                        })
                        .collect::<Vec<_>>()
                        .join("; ");
                    line(output, &format!("      let path: {{ {path_type} }};"));
                    line(output, "      try {");
                    line(output, "        path = {");
                    for field in &route.path_fields {
                        let raw = format!(
                            "routePath{}[{}]",
                            current_route_id,
                            ts_string(&field.name.text)
                        );
                        line(
                            output,
                            &format!(
                                "          {}: {},",
                                field.name.text,
                                self.validation_expression(
                                    &field.field_type,
                                    &raw,
                                    &ts_string(&format!("path.{}", field.name.text))
                                )
                            ),
                        );
                    }
                    line(output, "        };");
                    line(output, "      } catch (error) {");
                    line(output, "        if (error instanceof ValidationError) return json(422, errorEnvelope(\"invalid_value\", \"Request path value is invalid.\", requestId), requestId);");
                    line(output, "        throw error;");
                    line(output, "      }");
                }
                if let Some(input) = &route.input {
                    line(
                        output,
                        &format!("      let input: {};", self.ts_type(input)),
                    );
                    line(output, "      let body: unknown;");
                    line(output, "      try { body = await request.json(); } catch (error) {");
                    line(output, "        if (error instanceof SyntaxError) return json(400, errorEnvelope(\"invalid_request\", \"Request JSON syntax is invalid.\", requestId), requestId);");
                    line(output, "        throw error;");
                    line(output, "      }");
                    line(output, "      try {");
                    line(
                        output,
                        &format!(
                            "        input = {};",
                            self.validation_expression(input, "body", "\"request.body\"")
                        ),
                    );
                    line(output, "      } catch (error) {");
                    line(output, "        if (error instanceof ValidationError) return json(422, errorEnvelope(\"invalid_input\", \"Request body value is invalid.\", requestId), requestId);");
                    line(output, "        throw error;");
                    line(output, "      }");
                }
                if let Some(deadline) = &route.deadline {
                    let milliseconds = deadline
                        .milliseconds()
                        .expect("typechecked route deadline is a positive portable duration");
                    line(
                        output,
                        &format!("      __operation = withOperationDeadline(__operation, {milliseconds});"),
                    );
                    line(output, "      assertOperationDeadline(__operation);");
                }
                if let Some(run) = &route.run {
                    let invocation = self.invocation(run);
                    line(output, &format!("      const output = {invocation};"));
                } else if let Some(action) = &route.inline_action {
                    let mut visiting = BTreeSet::new();
                    if block_contains_mutation(&action.body, &self.callables, &mut visiting) {
                        if self.project.policy.active {
                            line(output, "      const output = await rootPersistence.withDeadline(__operation.deadlineAt).withSignal(__operation.signal).withOperationTime(__operation.now).withPolicy(__operation.principal, semanticOperationId).transaction(async persistence => {");
                        } else {
                            line(output, "      const output = await rootPersistence.withDeadline(__operation.deadlineAt).withSignal(__operation.signal).withOperationTime(__operation.now).transaction(async persistence => {");
                        }
                        self.block(output, &action.body, 8);
                        line(output, "      });");
                    } else {
                        let suspending = self.block_may_suspend(&action.body);
                        let await_prefix = if suspending { "await " } else { "" };
                        let async_prefix = if suspending { "async " } else { "" };
                        line(
                            output,
                            &format!("      const output = {await_prefix}({async_prefix}() => {{"),
                        );
                        if suspending && self.has_entities() {
                            if self.project.policy.active {
                                line(output, "        const persistence = rootPersistence.withDeadline(__operation.deadlineAt).withSignal(__operation.signal).withPolicy(__operation.principal, semanticOperationId);");
                            } else {
                                line(output, "        const persistence = rootPersistence.withDeadline(__operation.deadlineAt).withSignal(__operation.signal).withOperationTime(__operation.now);");
                            }
                        }
                        self.block(output, &action.body, 8);
                        line(output, "      })();");
                    }
                } else {
                    line(output, "      const output = undefined;");
                }
                match route.success {
                    RouteSuccess::NoContent => line(
                        output,
                        "      return new Response(null, { status: 204, headers: { \"x-request-id\": requestId, \"cache-control\": \"no-store\" } });",
                    ),
                    RouteSuccess::Ok | RouteSuccess::Created => {
                        let validated = route.output.as_ref().map_or_else(
                            || "null".to_owned(),
                            |output_type| {
                                self.validation_expression(
                                    output_type,
                                    "output",
                                    "\"response.body\"",
                                )
                            },
                        );
                        let status = if route.success == RouteSuccess::Created {
                            201
                        } else {
                            200
                        };
                        line(
                            output,
                            &format!("      return json({status}, {validated}, requestId);"),
                        );
                    }
                }
                line(output, "    }");
            }
        }

        line(
            output,
            "    return json(404, errorEnvelope(\"route_not_found\", \"Route not found.\", requestId), requestId);",
        );
        line(output, "  } catch (error) {");
        if self.has_entities() {
            line(output, "    if (error instanceof RequestDeadlineFault || error instanceof PersistenceFault && error.deadlineExceeded && error.kind !== \"unknown\") return json(504, errorEnvelope(\"deadline_exceeded\", \"The operation exceeded its declared deadline.\", requestId), requestId);");
        } else {
            line(output, "    if (error instanceof RequestDeadlineFault) return json(504, errorEnvelope(\"deadline_exceeded\", \"The operation exceeded its declared deadline.\", requestId), requestId);");
        }
        if self.first_party_supported() {
            line(output, "    if (error instanceof AuthenticationFault) {");
            line(output, "      if (error.declaredFailure !== null) {");
            line(output, "        const contract = failureContracts[error.declaredFailure as keyof typeof failureContracts];");
            line(output, "        if (contract) return json(contract.status, errorEnvelope(contract.code, contract.message ?? \"Authentication failed.\", requestId), requestId);");
            line(output, "      }");
            line(output, "      return json(error.status, errorEnvelope(error.code, \"Authentication failed.\", requestId), requestId);");
            line(output, "    }");
        }
        line(output, "    if (error instanceof DomainFailure) {");
        line(
            output,
            "      const contract = failureContracts[error.failureName as keyof typeof failureContracts];",
        );
        line(output, "      if (contract !== undefined) {");
        line(output, "        const details: JsonObject = {};");
        line(
            output,
            "        for (const field of contract.publicFields) {",
        );
        line(
            output,
            "          if (hasOwn(error.publicContext, field)) details[field] = error.publicContext[field];",
        );
        line(output, "        }");
        line(
            output,
            "        const publicDetails = contract.publicFields.length === 0 ? undefined : details;",
        );
        line(
            output,
            "        return json(contract.status, errorEnvelope(contract.code, contract.message ?? \"Request failed.\", requestId, publicDetails), requestId);",
        );
        line(output, "      }");
        line(output, "    }");
        if self.has_entities() {
            line(output, "    if (error instanceof PolicyFault) {");
            line(output, "      return json(403, errorEnvelope(\"not_permitted\", \"This operation is not permitted.\", requestId), requestId);");
            line(output, "    }");
        }
        if self.has_service_operations() {
            line(output, "    if (error instanceof ServiceAdapterFault) {");
            line(output, "      if (error.kind === \"deadline_exceeded\") return json(504, errorEnvelope(\"deadline_exceeded\", \"The operation exceeded its declared deadline.\", requestId), requestId);");
            line(output, "      if (error.kind === \"unavailable\") return json(503, errorEnvelope(\"service_unavailable\", \"A temporary service issue prevented the operation.\", requestId), requestId);");
            line(output, "      if (error.kind === \"outcome_unknown\") {");
            line(output, "        reportRuntimeFault(\"RUNTIME_OUTCOME_UNKNOWN\", semanticOperationId, sourceRevision, requestId, error);");
            line(output, "        return json(500, errorEnvelope(\"outcome_unknown\", \"The operation may have completed.\", requestId), requestId);");
            line(output, "      }");
            line(output, "      reportRuntimeFault(\"RUNTIME_SERVICE_MISCONFIGURED\", semanticOperationId, sourceRevision, requestId, error);");
            line(output, "      return json(500, errorEnvelope(\"internal_fault\", \"An internal error occurred.\", requestId), requestId);");
            line(output, "    }");
        }
        line(output, "    if (error instanceof AuthorizationFault) {");
        line(output, "      return json(403, errorEnvelope(\"not_permitted\", \"This operation is not permitted.\", requestId), requestId);");
        line(output, "    }");
        if self.has_entities() {
            line(
                output,
                "    if (error instanceof PersistenceFault && error.kind === \"unknown\") {",
            );
            line(output, "      reportRuntimeFault(\"RUNTIME_OUTCOME_UNKNOWN\", semanticOperationId, sourceRevision, requestId, error);");
            line(output, "      return json(500, errorEnvelope(\"outcome_unknown\", \"The operation may have completed.\", requestId), requestId);");
            line(output, "    }");
            line(output, "    if (error instanceof PersistenceFault && error.kind === \"driver\" && error.transaction?.outcome === \"no_commit\" && (error.transaction.phase === \"begin\" || error.transaction.rollbackProven)) {");
            line(output, "      return json(503, errorEnvelope(\"transaction_unavailable\", \"A temporary storage issue prevented the operation.\", requestId), requestId);");
            line(output, "    }");
            line(output, "    if (checkedReadOnlyOperation && error instanceof PersistenceFault && error.isReadUnavailable()) {");
            line(output, "      reportRuntimeFault(\"RUNTIME_READ_UNAVAILABLE\", semanticOperationId, sourceRevision, requestId, error);");
            line(output, "      return json(503, errorEnvelope(\"read_unavailable\", \"A temporary storage issue prevented the read.\", requestId), requestId);");
            line(output, "    }");
        }
        line(output, "    reportRuntimeFault(");
        line(output, "      \"RUNTIME_UNHANDLED_FAULT\",");
        line(output, "      semanticOperationId,");
        line(output, "      sourceRevision,");
        line(output, "      requestId,");
        line(output, "      error,");
        line(output, "    );");
        line(
            output,
            "    return json(500, errorEnvelope(\"internal_fault\", \"An internal error occurred.\", requestId), requestId);",
        );
        line(output, "  }");
        line(output, "}");
        line(output, "");
        line(output, "if (import.meta.main) {");
        line(output, "  const port = Number(Bun.env.PORT ?? \"3000\");");
        line(output, "  try {");
        if self.configuration.is_some() {
            line(output, "    configuration = loadConfiguration(Bun.env);");
        }
        if self.has_entities() {
            line(output, "    if (persistenceInitializationFatal()) throw new Error(\"required persistence schema is unavailable\");");
        }
        if self.first_party_supported() {
            line(output, "    await initializeAuthentication();");
        }
        if self.has_route_headers() {
            line(
                output,
                "    createApplicationServer().listen(port, \"0.0.0.0\");",
            );
        } else {
            line(output, "    Bun.serve({ port, fetch: handleRequest });");
        }
        line(output, "    console.log(JSON.stringify({");
        line(output, "      schemaVersion: 1,");
        line(output, "      kind: \"operational_log_event\",");
        if self.has_entities() {
            line(
                output,
                "      eventName: databaseIsReady() ? \"runtime.ready\" : \"runtime.live\",",
            );
            line(
                output,
                "      classification: databaseIsReady() ? \"ready\" : \"live\",",
            );
        } else {
            line(output, "      eventName: \"runtime.ready\",");
            line(output, "      classification: \"ready\",");
        }
        line(output, "      requestId: \"startup\",");
        line(output, "      traceId: null,");
        line(output, "      semanticOperationId: \"runtime:start\",");
        line(
            output,
            &format!(
                "      sourceRevision: {},",
                ts_string(&self.source_revision())
            ),
        );
        if self.has_entities() {
            line(output, "      attributes: { port, readiness: databaseIsReady() ? \"ready\" : \"not_ready\" },");
        } else {
            line(output, "      attributes: { port },");
        }
        line(output, "    }));");
        line(output, "  } catch (error) {");
        line(output, "    reportRuntimeFault(");
        line(output, "      \"RUNTIME_STARTUP_FAILED\",");
        line(output, "      \"runtime:start\",");
        line(
            output,
            &format!("      {},", ts_string(&self.source_revision())),
        );
        line(output, "      \"startup\",");
        line(output, "      error,");
        line(output, "    );");
        line(output, "    process.exit(1);");
        line(output, "  }");
        line(output, "}");
    }

    fn block(&self, output: &mut String, block: &Block, indent: usize) {
        for statement in &block.statements {
            let padding = " ".repeat(indent);
            match statement {
                Statement::Binding(binding) => {
                    let keyword = if binding.mutable { "let" } else { "const" };
                    line(
                        output,
                        &format!(
                            "{padding}{keyword} {} = {};",
                            binding.name.text,
                            self.expression(&binding.value)
                        ),
                    );
                }
                Statement::Assignment(assignment) => line(
                    output,
                    &format!(
                        "{padding}{} = {};",
                        assignment.target.text,
                        self.expression(&assignment.value)
                    ),
                ),
                Statement::Return(statement) => line(
                    output,
                    &format!("{padding}return {};", self.expression(&statement.value)),
                ),
                Statement::Reject(statement) => {
                    line(
                        output,
                        &format!(
                            "{padding}throw new DomainFailure({});",
                            self.domain_failure_arguments(statement),
                        ),
                    );
                }
                Statement::If(statement) => {
                    line(
                        output,
                        &format!("{padding}if ({}) {{", self.expression(&statement.condition)),
                    );
                    self.block(output, &statement.then_block, indent + 2);
                    if let Some(else_block) = &statement.else_block {
                        line(output, &format!("{padding}}} else {{"));
                        self.block(output, else_block, indent + 2);
                    }
                    line(output, &format!("{padding}}}"));
                }
                Statement::Match(statement) => {
                    let matched_enum = statement.arms.iter().find_map(|arm| match &arm.pattern {
                        MatchPattern::Name(pattern) => pattern.path.first(),
                        MatchPattern::Variant(pattern) => pattern.target.path.first(),
                        MatchPattern::Literal(_)
                        | MatchPattern::OptionalSome(_)
                        | MatchPattern::Wildcard(_) => None,
                    });
                    let tagged = matched_enum
                        .is_some_and(|name| {
                            self.enums
                                .get(&name.text)
                                .is_some_and(|declaration| enum_is_tagged(declaration))
                                || self
                                    .principal
                                    .is_some_and(|principal| principal.name.text == name.text)
                        });
                    let subject = self.expression(&statement.subject);
                    let needs_binding = tagged
                        || statement
                            .arms
                            .iter()
                            .any(|arm| matches!(arm.pattern, MatchPattern::OptionalSome(_)));
                    let subject_value = if needs_binding {
                        let binding = format!("matchSubject{}", statement.range.start);
                        line(output, &format!("{padding}const {binding} = {subject};"));
                        binding
                    } else {
                        subject.clone()
                    };
                    let switch_value = if tagged {
                        format!("{subject_value} === null ? null : {subject_value}.tag")
                    } else {
                        subject_value.clone()
                    };
                    line(output, &format!("{padding}switch ({switch_value}) {{"));
                    for arm in &statement.arms {
                        match &arm.pattern {
                            MatchPattern::Name(pattern) => {
                                let variant = pattern
                                    .path
                                    .last()
                                    .map(|part| part.text.as_str())
                                    .unwrap_or_default();
                                line(
                                    output,
                                    &format!("{padding}  case {}: {{", ts_string(variant)),
                                );
                            }
                            MatchPattern::Variant(pattern) => {
                                let variant = pattern
                                    .target
                                    .path
                                    .last()
                                    .map(|part| part.text.as_str())
                                    .unwrap_or_default();
                                line(
                                    output,
                                    &format!("{padding}  case {}: {{", ts_string(variant)),
                                );
                                if !pattern.bindings.is_empty() {
                                    let bindings = pattern
                                        .bindings
                                        .iter()
                                        .map(|binding| binding.text.as_str())
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    line(
                                        output,
                                        &format!(
                                            "{padding}    const {{ {bindings} }} = {subject_value};"
                                        ),
                                    );
                                }
                            }
                            MatchPattern::OptionalSome(pattern) => {
                                line(output, &format!("{padding}  default: {{"));
                                line(
                                    output,
                                    &format!(
                                        "{padding}    const {} = {subject_value};",
                                        pattern.binding.text
                                    ),
                                );
                            }
                            MatchPattern::Literal(pattern) => {
                                let value = if pattern.kind == LiteralKind::None {
                                    "null".to_owned()
                                } else {
                                    pattern.text.clone()
                                };
                                line(output, &format!("{padding}  case {value}: {{"));
                            }
                            MatchPattern::Wildcard(_) => {
                                line(output, &format!("{padding}  default: {{"));
                            }
                        }
                        self.block(output, &arm.body, indent + 4);
                        line(output, &format!("{padding}    break;"));
                        line(output, &format!("{padding}  }}"));
                    }
                    line(output, &format!("{padding}}}"));
                }
                Statement::Assert(statement) => {
                    line(
                        output,
                        &format!(
                            "{padding}if (!({})) throw new Error({});",
                            self.expression(&statement.condition),
                            ts_string(&format!(
                                "assertion failed at source bytes {}..{}",
                                statement.condition.range().start,
                                statement.condition.range().end
                            ))
                        ),
                    );
                }
                Statement::AdvanceClock(statement) => line(
                    output,
                    &format!(
                        "{padding}__testClockNow = temporal.add_elapsed(__testClockNow, {}) as Instant;",
                        self.expression(&statement.duration)
                    ),
                ),
                Statement::Unsupported(statement) => line(
                    output,
                    &format!(
                        "{padding}throw new Error({});",
                        ts_string(&format!(
                            "unsupported source statement: {}",
                            statement.keyword
                        ))
                    ),
                ),
            }
        }
    }

    fn expression(&self, expression: &Expression) -> String {
        match expression {
            Expression::Literal(literal) => match literal.kind {
                LiteralKind::None => "null".to_owned(),
                _ => literal.text.clone(),
            },
            Expression::Name(name)
                if name.path.first().is_some_and(|part| part.text == "config") =>
            {
                let suffix = name
                    .path
                    .iter()
                    .skip(1)
                    .map(|part| part.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                format!("configuration.{suffix}")
            }
            Expression::Name(name)
                if name.path.len() == 2
                    && name.path[0].text == "clock"
                    && name.path[1].text == "now" =>
            {
                "__operation.now".to_owned()
            }
            Expression::Name(name)
                if name.path.len() == 2 && name.path[0].text == "current_principal" =>
            {
                format!(
                    "requirePrincipalVariant(current_principal, {})",
                    ts_string(&name.path[1].text)
                )
            }
            Expression::Name(name) => name
                .path
                .first()
                .filter(|owner| is_temporal_enum(&owner.text))
                .filter(|_| name.path.len() == 2)
                .map(|owner| {
                    let variant = &name.path[1].text;
                    if owner.text == "Zone" {
                        ts_string(
                            &zone_name_for_variant(variant)
                                .unwrap_or_else(|| variant.replace('_', "/")),
                        )
                    } else if owner.text == "Locale" {
                        ts_string(
                            &self
                                .locales
                                .and_then(|locales| {
                                    locales.supported.iter().find_map(|locale| {
                                        let value = unquote(&locale.text);
                                        (standard_variant_name(value) == *variant)
                                            .then_some(value.to_owned())
                                    })
                                })
                                .unwrap_or_else(|| variant.replace('_', "-")),
                        )
                    } else {
                        ts_string(variant)
                    }
                })
                .unwrap_or_else(|| {
                    name.path
                        .first()
                        .and_then(|first| self.enums.get(&first.text))
                        .filter(|_| name.path.len() == 2)
                        .and_then(|declaration| {
                            name.path.last().map(|variant| (declaration, variant))
                        })
                        .map_or_else(
                            || {
                                name.path
                                    .iter()
                                    .map(|part| part.text.as_str())
                                    .collect::<Vec<_>>()
                                    .join(".")
                            },
                            |(declaration, variant)| {
                                if enum_is_tagged(declaration) {
                                    format!("{{ tag: {} }}", ts_string(&variant.text))
                                } else {
                                    ts_string(&variant.text)
                                }
                            },
                        )
                }),
            Expression::Invocation(invocation) => self.invocation(invocation),
            Expression::TestCall(call) => self.test_call(&call.invocation),
            Expression::Object(object) => self.object_literal(&object.fields),
            Expression::Construction(construction) => {
                let enum_declaration = construction
                    .target
                    .path
                    .first()
                    .and_then(|name| self.enums.get(&name.text))
                    .filter(|_| construction.target.path.len() == 2);
                if let (Some(declaration), Some(variant)) =
                    (enum_declaration, construction.target.path.last())
                {
                    if enum_is_tagged(declaration) {
                        let fields = construction
                            .fields
                            .iter()
                            .map(|field| {
                                format!("{}: {}", field.name.text, self.expression(&field.value))
                            })
                            .collect::<Vec<_>>();
                        let mut entries = vec![format!("tag: {}", ts_string(&variant.text))];
                        entries.extend(fields);
                        format!("{{ {} }}", entries.join(", "))
                    } else {
                        ts_string(&variant.text)
                    }
                } else {
                    self.object_literal(&construction.fields)
                }
            }
            Expression::Create(create) => {
                let name = create
                    .target
                    .path
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                let operation = format!(
                    "validate_{name}(await persistence.create_{name}({}), {})",
                    self.object_literal_for_create(&name, &create.fields),
                    ts_string(&format!("database.{name}"))
                );
                if create.conflicts.is_empty() {
                    operation
                } else {
                    self.constraint_binding_expression(&name, &operation, &create.conflicts)
                }
            }
            Expression::Query(query) => {
                let name = query
                    .target
                    .path
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                if let Some(page) = &query.page {
                    self.query_page_expression(page, &name)
                } else if query.includes.len() > 1 {
                    self.multi_include_expression(query, &name)
                } else if let Some(include) = query.includes.first() {
                    let result = type_name(&include.result);
                    if let Some(nested) = &include.nested_relationship {
                        let cardinality = match include.cardinality {
                            jadpo_syntax::QueryIncludeCardinality::Optional => "optional",
                            jadpo_syntax::QueryIncludeCardinality::Required => "required",
                            jadpo_syntax::QueryIncludeCardinality::Many => "many",
                        };
                        let operation = format!(
                            "await persistence.query_required_{name}_with_{}_then_{}_{cardinality}_by_{}({})",
                            include.relationship.text,
                            nested.text,
                            query.field.text,
                            self.expression(&query.value)
                        );
                        let missing = query
                            .missing
                            .as_ref()
                            .expect("nested includes require a required root query");
                        return format!(
                            "((value: unknown) => {{ if (value === null) throw new DomainFailure({}); return validate_{result}(value, {}); }})({operation})",
                            self.domain_failure_arguments(missing),
                            ts_string(&format!("database.{result}"))
                        );
                    }
                    if query.cardinality == jadpo_syntax::QueryCardinality::Many
                        && include.cardinality == jadpo_syntax::QueryIncludeCardinality::Required
                    {
                        let order = query
                            .order
                            .as_ref()
                            .expect("many owning-reference includes have explicit ordering");
                        let pagination = query
                            .pagination
                            .as_ref()
                            .expect("many owning-reference includes have explicit pagination");
                        let direction = match order.direction {
                            jadpo_syntax::QueryOrderDirection::Ascending => "asc",
                            jadpo_syntax::QueryOrderDirection::Descending => "desc",
                        };
                        let operation = format!(
                            "await persistence.query_many_{name}_with_{}_required_by_{}_order_by_{}_{direction}_paginated({}, {}, {})",
                            include.relationship.text,
                            query.field.text,
                            order.field.text,
                            self.expression(&query.value),
                            self.expression(&pagination.limit),
                            self.expression(&pagination.offset)
                        );
                        return format!(
                            "({operation}).map((value: unknown, index: number) => validate_{result}(value, `database.{result}[${{index}}]`))"
                        );
                    }
                    if include.cardinality != jadpo_syntax::QueryIncludeCardinality::Many {
                        let cardinality = match include.cardinality {
                            jadpo_syntax::QueryIncludeCardinality::Required => "required",
                            jadpo_syntax::QueryIncludeCardinality::Optional => "optional",
                            jadpo_syntax::QueryIncludeCardinality::Many => unreachable!(),
                        };
                        let operation = format!(
                            "await persistence.query_required_{name}_with_{}_{cardinality}_by_{}({})",
                            include.relationship.text,
                            query.field.text,
                            self.expression(&query.value)
                        );
                        let missing = query
                            .missing
                            .as_ref()
                            .expect("owning-parent includes require a required child query");
                        return format!(
                            "((value: unknown) => {{ if (value === null) throw new DomainFailure({}); return validate_{result}(value, {}); }})({operation})",
                            self.domain_failure_arguments(missing),
                            ts_string(&format!("database.{result}"))
                        );
                    }
                    let child_direction = match include.order.direction {
                        jadpo_syntax::QueryOrderDirection::Ascending => "asc",
                        jadpo_syntax::QueryOrderDirection::Descending => "desc",
                    };
                    if query.cardinality == jadpo_syntax::QueryCardinality::Many {
                        if self.project.policy.active {
                            return self
                                .policy_many_include_expression(query, &name, include, &result);
                        }
                        let parent_order = query
                            .order
                            .as_ref()
                            .expect("many-parent includes have explicit parent ordering");
                        let parent_pagination = query
                            .pagination
                            .as_ref()
                            .expect("many-parent includes have explicit parent pagination");
                        let parent_direction = match parent_order.direction {
                            jadpo_syntax::QueryOrderDirection::Ascending => "asc",
                            jadpo_syntax::QueryOrderDirection::Descending => "desc",
                        };
                        let operation = format!(
                            "await persistence.query_many_{name}_with_{}_by_{}_order_by_{}_{parent_direction}_include_order_by_{}_{child_direction}_paginated({}, {}, {}, {}, {})",
                            include.relationship.text,
                            query.field.text,
                            parent_order.field.text,
                            include.order.field.text,
                            self.expression(&query.value),
                            self.expression(&parent_pagination.limit),
                            self.expression(&parent_pagination.offset),
                            self.expression(&include.pagination.limit),
                            self.expression(&include.pagination.offset)
                        );
                        format!(
                            "({operation}).map((value: unknown, index: number) => validate_{result}(value, `database.{result}[${{index}}]`))"
                        )
                    } else {
                        let operation = format!(
                            "await persistence.query_required_{name}_with_{}_by_{}_order_by_{}_{child_direction}({}, {}, {})",
                            include.relationship.text,
                            query.field.text,
                            include.order.field.text,
                            self.expression(&query.value),
                            self.expression(&include.pagination.limit),
                            self.expression(&include.pagination.offset)
                        );
                        let missing = query.missing.as_ref().expect(
                            "included required parent queries have a missing failure binding",
                        );
                        format!(
                            "((value: unknown) => {{ if (value === null) throw new DomainFailure({}); return validate_{result}(value, {}); }})({operation})",
                            self.domain_failure_arguments(missing),
                            ts_string(&format!("database.{result}"))
                        )
                    }
                } else {
                    match query.cardinality {
                    jadpo_syntax::QueryCardinality::Optional => format!(
                        "((value: unknown) => value === null ? null : validate_{name}(value, {}))(await persistence.query_optional_{name}_by_{}({}))",
                        ts_string(&format!("database.{name}")),
                        query.field.text,
                        self.expression(&query.value)
                    ),
                    jadpo_syntax::QueryCardinality::Required => {
                        let missing = query
                            .missing
                            .as_ref()
                            .expect("required queries have a missing failure binding");
                        format!(
                            "((value: unknown) => {{ if (value === null) throw new DomainFailure({}); return validate_{name}(value, {}); }})(await persistence.query_required_{name}_by_{}({}))",
                            self.domain_failure_arguments(missing),
                            ts_string(&format!("database.{name}")),
                            query.field.text,
                            self.expression(&query.value)
                        )
                    }
                    jadpo_syntax::QueryCardinality::Many => {
                        let order = query
                            .order
                            .as_ref()
                            .expect("many-result queries have explicit ordering");
                        let direction = match order.direction {
                            jadpo_syntax::QueryOrderDirection::Ascending => "asc",
                            jadpo_syntax::QueryOrderDirection::Descending => "desc",
                        };
                        let (suffix, arguments) = query.pagination.as_ref().map_or_else(
                            || ("", self.expression(&query.value)),
                            |pagination| {
                                (
                                    "_paginated",
                                    format!(
                                        "{}, {}, {}",
                                        self.expression(&query.value),
                                        self.expression(&pagination.limit),
                                        self.expression(&pagination.offset)
                                    ),
                                )
                            },
                        );
                        format!(
                            "(await persistence.query_many_{name}_by_{}_order_by_{}_{direction}{suffix}({arguments})).map((value: unknown, index: number) => validate_{name}(value, `database.{name}[${{index}}]`))",
                            query.field.text,
                            order.field.text,
                        )
                    }
                }
                }
            }
            Expression::Update(update) => {
                let name = update
                    .target
                    .path
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                if let Some(transition) = &update.transition {
                    let method = format!(
                        "transition_required_{name}_by_{}_{}",
                        update.field.text, transition.text
                    );
                    let operation =
                        format!("persistence.{method}({})", self.expression(&update.value));
                    return self.required_mutation_expression(
                        &name,
                        &operation,
                        &update.missing,
                        &update.conflicts,
                    );
                }
                if let Some(patch) = &update.patch {
                    let patch_fields = self
                        .patch_fields_for_update(update)
                        .expect("checked patch input resolves to a record");
                    let suffix = patch_method_suffix(update, &patch_fields);
                    let patch_value = patch
                        .path
                        .iter()
                        .map(|part| part.text.as_str())
                        .collect::<Vec<_>>()
                        .join(".");
                    let supplied = patch_fields
                        .iter()
                        .map(|field| format!("hasOwn(patchValue, {})", ts_string(&field.name.text)))
                        .collect::<Vec<_>>()
                        .join(" || ");
                    let derived_arguments = patch_derived_changes(update)
                        .iter()
                        .map(|(change, _)| self.expression(&change.value))
                        .collect::<Vec<_>>();
                    let derived_suffix = if derived_arguments.is_empty() {
                        String::new()
                    } else {
                        format!(", {}", derived_arguments.join(", "))
                    };
                    let operation = format!(
                        "persistence.update_required_{name}_by_{}_{suffix}(predicateValue, patchValue{derived_suffix})",
                        update.field.text,
                    );
                    let operation = if self.has_generated_change_field(&name) {
                        format!("(async () => {{ const changed = await {operation}; if (changed === null || !persistence.did_change(changed)) return changed; return persistence.touch_{name}_by_{}(predicateValue, __operation.now); }})()", update.field.text)
                    } else {
                        operation
                    };
                    let mutation = self.required_mutation_expression(
                        &name,
                        &operation,
                        &update.missing,
                        &update.conflicts,
                    );
                    let empty = update
                        .empty
                        .as_ref()
                        .expect("checked patch updates bind an empty failure");
                    return format!(
                        "(await (async () => {{ const patchValue = {patch_value}; const predicateValue = {}; if (!({supplied})) throw new DomainFailure({}); return {mutation}; }})())",
                        self.expression(&update.value),
                        self.domain_failure_arguments(empty),
                    );
                }
                let changes = update
                    .changes
                    .iter()
                    .map(|change| change.name.text.as_str())
                    .collect::<Vec<_>>()
                    .join("_and_");
                let replacements = update
                    .changes
                    .iter()
                    .map(|change| self.expression(&change.value))
                    .collect::<Vec<_>>()
                    .join(", ");
                let predicate = self.expression(&update.value);
                let base_operation = format!(
                    "persistence.update_required_{name}_by_{}_set_{changes}(predicateValue, {replacements})",
                    update.field.text,
                );
                let operation = if self.has_generated_change_field(&name) {
                    format!("(async () => {{ const changed = await {base_operation}; if (changed === null || !persistence.did_change(changed)) return changed; return persistence.touch_{name}_by_{}(predicateValue, __operation.now); }})()", update.field.text)
                } else {
                    base_operation
                };
                let operation =
                    format!("(async (predicateValue: unknown) => {operation})({predicate})");
                self.required_mutation_expression(
                    &name,
                    &operation,
                    &update.missing,
                    &update.conflicts,
                )
            }
            Expression::Delete(delete) => {
                let name = delete
                    .target
                    .path
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                let operation = format!(
                    "persistence.delete_required_{name}_by_{}({})",
                    delete.field.text,
                    self.expression(&delete.value)
                );
                self.required_mutation_expression(
                    &name,
                    &operation,
                    &delete.missing,
                    &delete.conflicts,
                )
            }
            Expression::Unary(unary) => format!(
                "({}{})",
                match unary.operator {
                    jadpo_syntax::UnaryOperator::Not => "!",
                    jadpo_syntax::UnaryOperator::Negate => "-",
                },
                self.expression(&unary.value)
            ),
            Expression::Binary(binary) => format!(
                "({} {} {})",
                self.expression(&binary.left),
                match binary.operator {
                    BinaryOperator::Or => "||",
                    BinaryOperator::And => "&&",
                    BinaryOperator::Equal => "===",
                    BinaryOperator::NotEqual => "!==",
                    BinaryOperator::Less => "<",
                    BinaryOperator::LessEqual => "<=",
                    BinaryOperator::Greater => ">",
                    BinaryOperator::GreaterEqual => ">=",
                    BinaryOperator::Add => "+",
                    BinaryOperator::Subtract => "-",
                    BinaryOperator::Multiply => "*",
                    BinaryOperator::Divide => "/",
                    BinaryOperator::Remainder => "%",
                },
                self.expression(&binary.right)
            ),
            Expression::Grouped(grouped) => format!("({})", self.expression(&grouped.value)),
            Expression::Attempt(attempt) => self.expression(&attempt.value),
            Expression::OutcomeMatch(outcome) => self.outcome_match_expression(outcome),
            Expression::Missing(_) => "undefined".to_owned(),
        }
    }

    fn query_page_expression(&self, page: &jadpo_syntax::QueryPage, entity_name: &str) -> String {
        let postgres_first_sql =
            self.query_page_sql(entity_name, page, SqlDialect::Postgres, false);
        let postgres_after_sql = self.query_page_sql(entity_name, page, SqlDialect::Postgres, true);
        let sqlite_first_sql = self.query_page_sql(entity_name, page, SqlDialect::Sqlite, false);
        let sqlite_after_sql = self.query_page_sql(entity_name, page, SqlDialect::Sqlite, true);
        let values = self.query_page_values(entity_name, page);
        let projection_name = type_name(&page.projection);
        let result_name = type_name(&page.result);
        let cursor_name = type_name(&page.cursor);
        let projection = self
            .records
            .get(&projection_name)
            .expect("checked page projection exists");
        let entity = self
            .records
            .get(entity_name)
            .expect("checked page entity exists");
        let projection_fields = projection
            .fields
            .iter()
            .map(|field| {
                let entity_field = entity
                    .fields
                    .iter()
                    .find(|candidate| candidate.name.text == field.name.text)
                    .expect("checked projection field exists on page entity");
                let raw = format!("row[{}]", ts_string(&field.name.text));
                let path = format!(
                    "`database.{result_name}.items[${{index}}].{}`",
                    field.name.text
                );
                let decoded =
                    self.database_decode_expression(&entity_field.field_type, &raw, &path);
                format!("{}: {decoded}", field.name.text)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let cursor_fields = page
            .cursor_fields
            .iter()
            .map(|field| {
                let entity_field = entity
                    .fields
                    .iter()
                    .find(|candidate| candidate.name.text == field.text)
                    .expect("checked cursor field exists on page entity");
                let raw = format!("last[{}]", ts_string(&field.text));
                let path = ts_string(&format!("database.{result_name}.next.{}", field.text));
                let decoded =
                    self.database_decode_expression(&entity_field.field_type, &raw, &path);
                format!("{}: {decoded}", field.text)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let result_path = ts_string(&format!("database.{result_name}"));
        let next_path = ts_string(&format!("database.{result_name}.next"));
        let operation = ts_string(&format!("query.{entity_name}.page"));
        let page_size = self.expression(&page.limit);
        let after = self.expression(&page.after);
        format!(
            "(await (async () => {{ const values = [{values}]; const pageRows = await persistence.query_page_rows({operation}, {}, {}, {}, {}, {}, values, ({after} !== undefined && {after} !== null)); const pageSize = {page_size}; const hasNext = pageRows.length > pageSize; const selected = pageRows.slice(0, pageSize); const items = selected.map((value: unknown, index: number) => {{ const row = expectObject(value, `database.{result_name}.items[${{index}}]`); return validate_{projection_name}({{ {projection_fields} }}, `database.{result_name}.items[${{index}}]`); }}); const last = selected[selected.length - 1] as Record<string, unknown> | undefined; const next = hasNext && last !== undefined ? validate_{cursor_name}({{ {cursor_fields} }}, {}) : null; return validate_{result_name}({{ items, next }}, {}); }})())",
            ts_string(entity_name),
            ts_string(&postgres_first_sql),
            ts_string(&sqlite_first_sql),
            ts_string(&postgres_after_sql),
            ts_string(&sqlite_after_sql),
            next_path,
            result_path,
        )
    }

    fn query_page_values(&self, entity_name: &str, page: &jadpo_syntax::QueryPage) -> String {
        let entity = self
            .records
            .get(entity_name)
            .expect("checked page entity exists");
        let mut values = Vec::new();
        for predicate in &page.predicates {
            if predicate.operator == jadpo_syntax::QueryPagePredicateOperator::Equal
                && matches!(
                    predicate.value.as_ref(),
                    Expression::Literal(literal) if literal.kind == LiteralKind::None
                )
            {
                continue;
            }
            let field = entity
                .fields
                .iter()
                .find(|field| field.name.text == predicate.field.text)
                .expect("checked page predicate field exists");
            let value = self.expression(&predicate.value);
            let value = if predicate.operator == jadpo_syntax::QueryPagePredicateOperator::Equal {
                value
            } else {
                format!("({value} ?? null)")
            };
            values.push(format!(
                "{{ value: {value}, kind: {} }}",
                ts_string(&self.representation_root_for(&field.field_type))
            ));
        }
        let after = self.expression(&page.after);
        for field_name in &page.cursor_fields {
            let field = entity
                .fields
                .iter()
                .find(|field| field.name.text == field_name.text)
                .expect("checked cursor field exists");
            values.push(format!(
                "{{ value: ({after}?.{} ?? null), kind: {} }}",
                field_name.text,
                ts_string(&self.representation_root_for(&field.field_type))
            ));
        }
        values.push(format!(
            "{{ value: ({} + 1), kind: \"Int\" }}",
            self.expression(&page.limit)
        ));
        values.join(", ")
    }

    fn query_page_sql(
        &self,
        entity_name: &str,
        page: &jadpo_syntax::QueryPage,
        dialect: SqlDialect,
        continuation: bool,
    ) -> String {
        let entity = self
            .records
            .get(entity_name)
            .expect("checked page entity exists");
        let projection = self
            .records
            .get(&type_name(&page.projection))
            .expect("checked page projection exists");
        let mut selected = projection
            .fields
            .iter()
            .map(|field| field.name.text.clone())
            .collect::<Vec<_>>();
        for field in &page.cursor_fields {
            if !selected.contains(&field.text) {
                selected.push(field.text.clone());
            }
        }
        let selected = selected
            .iter()
            .map(|field| {
                let column = sql_identifier(field);
                format!("{column} AS {column}")
            })
            .collect::<Vec<_>>()
            .join(", ");
        let placeholder = |index: usize| match dialect {
            SqlDialect::Postgres => format!("${index}"),
            SqlDialect::Sqlite => format!("?{index}"),
        };
        let mut parameter = 1;
        let mut conditions = Vec::new();
        for predicate in &page.predicates {
            let column = sql_identifier(&predicate.field.text);
            let none = matches!(
                predicate.value.as_ref(),
                Expression::Literal(literal) if literal.kind == LiteralKind::None
            );
            match predicate.operator {
                jadpo_syntax::QueryPagePredicateOperator::Equal if none => {
                    conditions.push(format!("{column} IS NULL"));
                }
                jadpo_syntax::QueryPagePredicateOperator::Equal => {
                    conditions.push(format!("{column} = {}", placeholder(parameter)));
                    parameter += 1;
                }
                jadpo_syntax::QueryPagePredicateOperator::OptionalEqual => {
                    let value = match dialect {
                        SqlDialect::Postgres => {
                            let field = entity
                                .fields
                                .iter()
                                .find(|field| field.name.text == predicate.field.text)
                                .expect("checked page predicate field exists");
                            format!(
                                "{}::{}",
                                placeholder(parameter),
                                self.sql_type(&field.field_type, dialect)
                            )
                        }
                        SqlDialect::Sqlite => placeholder(parameter),
                    };
                    conditions.push(format!("({value} IS NULL OR {column} = {value})"));
                    parameter += 1;
                }
                jadpo_syntax::QueryPagePredicateOperator::OptionalLessEqual => {
                    let value = match dialect {
                        SqlDialect::Postgres => {
                            let field = entity
                                .fields
                                .iter()
                                .find(|field| field.name.text == predicate.field.text)
                                .expect("checked page predicate field exists");
                            format!(
                                "{}::{}",
                                placeholder(parameter),
                                self.sql_type(&field.field_type, dialect)
                            )
                        }
                        SqlDialect::Sqlite => placeholder(parameter),
                    };
                    conditions.push(format!("({value} IS NULL OR {column} <= {value})"));
                    parameter += 1;
                }
            }
        }
        let after_start = parameter;
        parameter += page.cursor_fields.len();
        if continuation {
            let direction = page.order[0].direction;
            let comparison = match direction {
                jadpo_syntax::QueryOrderDirection::Ascending => ">",
                jadpo_syntax::QueryOrderDirection::Descending => "<",
            };
            let columns = page
                .order
                .iter()
                .map(|order| sql_identifier(&order.field.text))
                .collect::<Vec<_>>()
                .join(", ");
            let cursor = (0..page.cursor_fields.len())
                .map(|index| placeholder(after_start + index))
                .collect::<Vec<_>>()
                .join(", ");
            conditions.push(format!("(({columns}) {comparison} ({cursor}))"));
        } else {
            for index in 0..page.cursor_fields.len() {
                let cursor_parameter = placeholder(after_start + index);
                let absent = match dialect {
                    SqlDialect::Postgres => format!("{cursor_parameter}::text IS NULL"),
                    SqlDialect::Sqlite => format!("{cursor_parameter} IS NULL"),
                };
                conditions.push(absent);
            }
        }
        if let Some(visibility) = self.lifecycle_visibility_sql(entity_name) {
            conditions.push(format!("({visibility})"));
        }
        let limit = placeholder(parameter);
        let where_clause = conditions.join(" AND ");
        let order = page
            .order
            .iter()
            .map(|field| {
                let direction = match field.direction {
                    jadpo_syntax::QueryOrderDirection::Ascending => "ASC",
                    jadpo_syntax::QueryOrderDirection::Descending => "DESC",
                };
                format!("{} {direction}", sql_identifier(&field.field.text))
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "SELECT {selected} FROM {} WHERE {where_clause} ORDER BY {order} LIMIT {limit}",
            sql_identifier(&snake_case(entity_name))
        )
    }

    fn fixture_configuration_expression(&self, expression: &Expression) -> String {
        if let Expression::Invocation(invocation) = expression {
            if invocation.callee.path.len() == 1
                && invocation.callee.path[0].text == "secret"
                && invocation.arguments.len() == 1
                && invocation.named_arguments.is_empty()
            {
                return self.expression(&invocation.arguments[0]);
            }
        }
        self.expression(expression)
    }

    fn outcome_match_expression(&self, outcome: &jadpo_syntax::OutcomeMatchExpression) -> String {
        let success = outcome
            .arms
            .iter()
            .find_map(|arm| match &arm.pattern {
                jadpo_syntax::OutcomeMatchPattern::Success(binding) => Some((binding, &arm.body)),
                jadpo_syntax::OutcomeMatchPattern::Failure(_) => None,
            })
            .expect("checked outcome matches contain one success arm");
        let success_value = match success.1 {
            jadpo_syntax::OutcomeMatchArmBody::Value(value) => self.expression(value),
            _ => "undefined".to_owned(),
        };
        let failure_cases = outcome
            .arms
            .iter()
            .filter_map(|arm| {
                let jadpo_syntax::OutcomeMatchPattern::Failure(failure) = &arm.pattern else {
                    return None;
                };
                let body = match &arm.body {
                    jadpo_syntax::OutcomeMatchArmBody::Value(value) => {
                        format!("return {};", self.expression(value))
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Reject(rejection) => format!(
                        "throw new DomainFailure({});",
                        self.domain_failure_arguments(rejection)
                    ),
                    jadpo_syntax::OutcomeMatchArmBody::Propagate(_) => "throw error;".to_owned(),
                };
                Some(format!("case {}: {{ {body} }}", ts_string(&failure.text)))
            })
            .collect::<Vec<_>>()
            .join(" ");
        let suspending = self.expression_may_suspend(&Expression::OutcomeMatch(outcome.clone()));
        let (open, close) = if suspending {
            ("(await (async () => {", "})())")
        } else {
            ("(() => {", "})()")
        };
        let failure_dispatch = if self.project.semantics.external_effects.is_empty() {
            "if (!(error instanceof DomainFailure)) throw error; switch (error.failureName)"
                .to_owned()
        } else {
            "const failureName = error instanceof DomainFailure ? error.failureName : error instanceof ServiceAdapterFault ? ({ unavailable: \"Unavailable\", misconfigured: \"Misconfigured\", outcome_unknown: \"OutcomeUnknown\" } as const)[error.kind] : null; if (failureName === null) throw error; switch (failureName)".to_owned()
        };
        format!(
            "{open} try {{ const {} = {}; return {success_value}; }} catch (error) {{ {failure_dispatch} {{ {failure_cases} default: throw error; }} }} {close}",
            success.0.text,
            self.expression(&outcome.subject),
        )
    }

    fn patch_fields_for_update(
        &self,
        update: &jadpo_syntax::UpdateExpression,
    ) -> Option<Vec<&'project FieldDeclaration>> {
        let patch = update.patch.as_ref()?;
        for (name, callable) in &self.callables {
            let mut updates = Vec::new();
            collect_update_expressions(&callable.body, &mut updates);
            if !updates
                .iter()
                .any(|candidate| std::ptr::eq(*candidate, update))
            {
                continue;
            }
            // Use the checked expression at this exact lexical use, not a
            // same-named parameter: local bindings and shadowing are semantic
            // decisions already resolved by the type checker.
            let source = &self.project.semantics.node(name)?.source;
            let inferred = self.project.typing.expressions.iter().find(|expression| {
                expression.source == *source && expression.range == patch.range
            })?;
            let record = self.records.get(&inferred.type_name)?;
            return Some(record.fields.iter().collect());
        }
        None
    }

    fn multi_include_expression(
        &self,
        query: &jadpo_syntax::QueryExpression,
        parent_name: &str,
    ) -> String {
        let first = query
            .includes
            .first()
            .expect("multi-include queries contain at least two includes");
        let result = type_name(&first.result);
        let parent = self
            .records
            .get(parent_name)
            .expect("checked include parent exists");

        if query.cardinality == jadpo_syntax::QueryCardinality::Required {
            let missing = query
                .missing
                .as_ref()
                .expect("required multi-include query has a missing binding");
            let mut loads = Vec::new();
            let mut fields = Vec::new();
            for include in &query.includes {
                let inverse = parent
                    .inverses
                    .iter()
                    .find(|inverse| inverse.name.text == include.relationship.text)
                    .expect("checked inverse exists");
                let child = self
                    .records
                    .get(&inverse.target.text)
                    .expect("checked inverse child exists");
                let via_field = inverse
                    .via
                    .path
                    .get(1)
                    .expect("checked inverse has a child field")
                    .text
                    .as_str();
                let target_field = child
                    .fields
                    .iter()
                    .find(|field| field.name.text == via_field)
                    .and_then(|field| field.reference.as_ref())
                    .and_then(|reference| reference.target.path.get(1))
                    .expect("checked inverse points at a parent field")
                    .text
                    .as_str();
                let direction = match include.order.direction {
                    jadpo_syntax::QueryOrderDirection::Ascending => "asc",
                    jadpo_syntax::QueryOrderDirection::Descending => "desc",
                };
                loads.push(format!(
                    "const {} = await persistence.query_many_{}_by_{via_field}_order_by_{}_{direction}_paginated((parent as Record<string, unknown>)[{}], {}, {});",
                    include.relationship.text,
                    inverse.target.text,
                    include.order.field.text,
                    ts_string(target_field),
                    self.expression(&include.pagination.limit),
                    self.expression(&include.pagination.offset)
                ));
                fields.push(format!(
                    "{}: {}",
                    include.relationship.text, include.relationship.text
                ));
            }
            return format!(
                "(await (async () => {{ const parent = await persistence.query_optional_{parent_name}_by_{}({}); if (parent === null) throw new DomainFailure({}); {} return validate_{result}({{ parent, {} }}, {}); }})())",
                query.field.text,
                self.expression(&query.value),
                self.domain_failure_arguments(missing),
                loads.join(" "),
                fields.join(", "),
                ts_string(&format!("database.{result}"))
            );
        }

        let parent_order = query
            .order
            .as_ref()
            .expect("many-parent multi-include query has parent ordering");
        let parent_pagination = query
            .pagination
            .as_ref()
            .expect("many-parent multi-include query has parent pagination");
        let parent_direction = match parent_order.direction {
            jadpo_syntax::QueryOrderDirection::Ascending => "asc",
            jadpo_syntax::QueryOrderDirection::Descending => "desc",
        };
        if self.project.policy.active {
            let mut loads = Vec::new();
            let mut fields = Vec::new();
            for include in &query.includes {
                let inverse = parent
                    .inverses
                    .iter()
                    .find(|inverse| inverse.name.text == include.relationship.text)
                    .expect("checked inverse exists");
                let child = self
                    .records
                    .get(&inverse.target.text)
                    .expect("checked inverse child exists");
                let via_field = inverse
                    .via
                    .path
                    .get(1)
                    .expect("checked inverse has a child field")
                    .text
                    .as_str();
                let target_field = child
                    .fields
                    .iter()
                    .find(|field| field.name.text == via_field)
                    .and_then(|field| field.reference.as_ref())
                    .and_then(|reference| reference.target.path.get(1))
                    .expect("checked inverse points at a parent field")
                    .text
                    .as_str();
                let direction = match include.order.direction {
                    jadpo_syntax::QueryOrderDirection::Ascending => "asc",
                    jadpo_syntax::QueryOrderDirection::Descending => "desc",
                };
                loads.push(format!(
                    "const {} = await persistence.query_many_{}_by_{via_field}_order_by_{}_{direction}_paginated((parent as Record<string, unknown>)[{}], {}, {});",
                    include.relationship.text,
                    inverse.target.text,
                    include.order.field.text,
                    ts_string(target_field),
                    self.expression(&include.pagination.limit),
                    self.expression(&include.pagination.offset)
                ));
                fields.push(format!(
                    "{}: {}",
                    include.relationship.text, include.relationship.text
                ));
            }
            return format!(
                "(await (async () => {{ const parents = await persistence.query_many_{parent_name}_by_{}_order_by_{}_{parent_direction}_paginated({}, {}, {}); const output: unknown[] = []; for (const parent of parents) {{ {} output.push(validate_{result}({{ parent, {} }}, `database.{result}[${{output.length}}]`)); }} return output; }})())",
                query.field.text,
                parent_order.field.text,
                self.expression(&query.value),
                self.expression(&parent_pagination.limit),
                self.expression(&parent_pagination.offset),
                loads.join(" "),
                fields.join(", ")
            );
        }
        let empty_fields = query
            .includes
            .iter()
            .map(|include| format!("{}: []", include.relationship.text))
            .collect::<Vec<_>>()
            .join(", ");
        let mut batches = Vec::new();
        let mut merges = Vec::new();
        for (index, include) in query.includes.iter().enumerate() {
            let direction = match include.order.direction {
                jadpo_syntax::QueryOrderDirection::Ascending => "asc",
                jadpo_syntax::QueryOrderDirection::Descending => "desc",
            };
            batches.push(format!(
                "const batch_{index} = await persistence.query_many_{parent_name}_with_{}_by_{}_order_by_{}_{parent_direction}_include_order_by_{}_{direction}_paginated({}, {}, {}, {}, {});",
                include.relationship.text,
                query.field.text,
                parent_order.field.text,
                include.order.field.text,
                self.expression(&query.value),
                self.expression(&parent_pagination.limit),
                self.expression(&parent_pagination.offset),
                self.expression(&include.pagination.limit),
                self.expression(&include.pagination.offset)
            ));
            merges.push(format!(
                "for (const raw of batch_{index}) {{ const item = raw as {{ parent: Record<string, unknown>; {}: unknown[] }}; const key = item.parent[{}]; let entry = merged.get(key); if (entry === undefined) {{ entry = {{ parent: item.parent, {empty_fields} }}; merged.set(key, entry); }} entry[{}] = item.{}; }}",
                include.relationship.text,
                ts_string(&parent_order.field.text),
                ts_string(&include.relationship.text),
                include.relationship.text
            ));
        }
        format!(
            "(await (async () => {{ {} const merged = new Map<unknown, Record<string, unknown>>(); {} return [...merged.values()].map((value: unknown, index: number) => validate_{result}(value, `database.{result}[${{index}}]`)); }})())",
            batches.join(" "),
            merges.join(" ")
        )
    }

    fn policy_many_include_expression(
        &self,
        query: &jadpo_syntax::QueryExpression,
        parent_name: &str,
        include: &jadpo_syntax::QueryInclude,
        result: &str,
    ) -> String {
        let parent = self
            .records
            .get(parent_name)
            .expect("checked include parent exists");
        let inverse = parent
            .inverses
            .iter()
            .find(|inverse| inverse.name.text == include.relationship.text)
            .expect("checked inverse exists");
        let child = self
            .records
            .get(&inverse.target.text)
            .expect("checked inverse child exists");
        let via_field = inverse
            .via
            .path
            .get(1)
            .expect("checked inverse has a child field")
            .text
            .as_str();
        let target_field = child
            .fields
            .iter()
            .find(|field| field.name.text == via_field)
            .and_then(|field| field.reference.as_ref())
            .and_then(|reference| reference.target.path.get(1))
            .expect("checked inverse points at a parent field")
            .text
            .as_str();
        let parent_order = query
            .order
            .as_ref()
            .expect("many-parent includes have explicit parent ordering");
        let parent_pagination = query
            .pagination
            .as_ref()
            .expect("many-parent includes have explicit parent pagination");
        let parent_direction = match parent_order.direction {
            jadpo_syntax::QueryOrderDirection::Ascending => "asc",
            jadpo_syntax::QueryOrderDirection::Descending => "desc",
        };
        let child_direction = match include.order.direction {
            jadpo_syntax::QueryOrderDirection::Ascending => "asc",
            jadpo_syntax::QueryOrderDirection::Descending => "desc",
        };
        format!(
            "(await (async () => {{ const parents = await persistence.query_many_{parent_name}_by_{}_order_by_{}_{parent_direction}_paginated({}, {}, {}); const output: unknown[] = []; for (const parent of parents) {{ const children = await persistence.query_many_{}_by_{via_field}_order_by_{}_{child_direction}_paginated((parent as Record<string, unknown>)[{}], {}, {}); output.push(validate_{result}({{ parent, {}: children }}, `database.{result}[${{output.length}}]`)); }} return output; }})())",
            query.field.text,
            parent_order.field.text,
            self.expression(&query.value),
            self.expression(&parent_pagination.limit),
            self.expression(&parent_pagination.offset),
            inverse.target.text,
            include.order.field.text,
            ts_string(target_field),
            self.expression(&include.pagination.limit),
            self.expression(&include.pagination.offset),
            include.relationship.text
        )
    }

    fn required_mutation_expression(
        &self,
        entity: &str,
        operation: &str,
        missing: &jadpo_syntax::RejectStatement,
        conflicts: &[jadpo_syntax::ConflictBinding],
    ) -> String {
        let conflict_checks = self.constraint_binding_checks(entity, conflicts);
        format!(
            "(await (async () => {{ try {{ const value = await {operation}; if (value === null) throw new DomainFailure({}); return validate_{entity}(value, {}); }} catch (error) {{ {conflict_checks} throw error; }} }})())",
            self.domain_failure_arguments(missing),
            ts_string(&format!("database.{entity}")),
        )
    }

    fn constraint_binding_expression(
        &self,
        entity: &str,
        operation: &str,
        conflicts: &[jadpo_syntax::ConflictBinding],
    ) -> String {
        let conflict_checks = self.constraint_binding_checks(entity, conflicts);
        format!(
            "(await (async () => {{ try {{ return {operation}; }} catch (error) {{ {conflict_checks} throw error; }} }})())",
        )
    }

    fn constraint_binding_checks(
        &self,
        entity: &str,
        conflicts: &[jadpo_syntax::ConflictBinding],
    ) -> String {
        let mut checks = conflicts
            .iter()
            .filter_map(|binding| {
                binding.constraint.as_ref().map(|constraint| {
                    format!(
                        "if (error instanceof PersistenceFault && error.kind === \"constraint\" && error.constraint === {}) throw new DomainFailure({});",
                        ts_string(&if constraint.path.len() == 1 {
                            format!("{entity}.{}", constraint.path[0].text)
                        } else {
                            constraint
                                .path
                                .iter()
                                .map(|part| part.text.as_str())
                                .collect::<Vec<_>>()
                                .join(".")
                        }),
                        self.domain_failure_arguments(&binding.rejection),
                    )
                })
            })
            .collect::<Vec<_>>();
        if let Some(binding) = conflicts
            .iter()
            .find(|binding| binding.constraint.is_none())
        {
            checks.push(format!(
                "if (error instanceof PersistenceFault && error.kind === \"constraint\") throw new DomainFailure({});",
                self.domain_failure_arguments(&binding.rejection),
            ));
        }
        checks.join(" ")
    }

    fn invocation(&self, invocation: &jadpo_syntax::InvocationExpression) -> String {
        let authored_name = invocation
            .callee
            .path
            .iter()
            .map(|part| part.text.as_str())
            .collect::<Vec<_>>()
            .join(".");
        let mut name = authored_name.clone();
        let mut rendered_arguments = invocation
            .arguments
            .iter()
            .map(|argument| self.expression(argument))
            .collect::<Vec<_>>();
        if !invocation.named_arguments.is_empty() {
            rendered_arguments.push(if authored_name.starts_with("temporal.") {
                self.temporal_options(&invocation.named_arguments)
            } else {
                self.object_literal(&invocation.named_arguments)
            });
        }
        if let Some(validator) = self.field_validator_name(&authored_name) {
            let argument = rendered_arguments
                .first()
                .map(String::as_str)
                .unwrap_or("undefined");
            return format!("{validator}({argument}, {})", ts_string(&authored_name));
        }
        if self.service_effect(&authored_name).is_some() {
            return format!(
                "await __service_{}({}, __operation)",
                ts_callable_name(&authored_name),
                rendered_arguments.join(", ")
            );
        }
        if !self.callables.contains_key(&name) && invocation.callee.path.len() >= 2 {
            let operation = invocation
                .callee
                .path
                .last()
                .expect("callee has a final name")
                .text
                .as_str();
            let mut candidates = self
                .callables
                .keys()
                .filter(|candidate| candidate.ends_with(&format!(".{operation}")));
            if let Some(candidate) = candidates.next() {
                if candidates.next().is_none() {
                    name.clone_from(candidate);
                    rendered_arguments.insert(
                        0,
                        invocation.callee.path[..invocation.callee.path.len() - 1]
                            .iter()
                            .map(|part| part.text.as_str())
                            .collect::<Vec<_>>()
                            .join("."),
                    );
                }
            }
        }
        let arguments = rendered_arguments.join(", ");
        if matches!(name.as_str(), "Instant" | "CalendarDate" | "Duration") {
            let argument = invocation
                .arguments
                .first()
                .map(|argument| self.expression(argument))
                .unwrap_or_else(|| "undefined".to_owned());
            let validator = match name.as_str() {
                "Instant" => "validateInstant",
                "CalendarDate" => "validateCalendarDate",
                "Duration" => "validateDuration",
                _ => unreachable!(),
            };
            format!("{validator}({argument}, {})", ts_string(&name))
        } else if self.types.contains_key(&name) {
            let argument = invocation
                .arguments
                .first()
                .map(|argument| self.expression(argument))
                .unwrap_or_else(|| "undefined".to_owned());
            format!("validate_{name}({argument}, {})", ts_string(&name))
        } else if self.callables.contains_key(&name) {
            let suspending = self.callable_may_suspend(&name);
            let await_prefix = if suspending { "await " } else { "" };
            let operation_arguments = if arguments.is_empty() {
                "__operation".to_owned()
            } else {
                format!("{arguments}, __operation")
            };
            if suspending && self.has_entities() {
                format!(
                    "{await_prefix}{}({operation_arguments}, persistence)",
                    ts_callable_name(&name)
                )
            } else {
                format!(
                    "{await_prefix}{}({operation_arguments})",
                    ts_callable_name(&name)
                )
            }
        } else {
            format!("{authored_name}({arguments})")
        }
    }

    fn test_call(&self, invocation: &jadpo_syntax::InvocationExpression) -> String {
        let name = invocation
            .callee
            .path
            .iter()
            .map(|part| part.text.as_str())
            .collect::<Vec<_>>()
            .join(".");
        let mut arguments = invocation
            .arguments
            .iter()
            .map(|argument| self.expression(argument))
            .collect::<Vec<_>>();
        arguments.push("captureOperation(__testClockNow, null, () => __testClockNow, true, null, __operation.testServiceFakes)".to_owned());
        let suspending = self.callable_may_suspend(&name);
        if suspending && self.has_entities() {
            arguments.push("persistence".to_owned());
        }
        format!(
            "{}{}({})",
            if suspending { "await " } else { "" },
            ts_callable_name(&name),
            arguments.join(", ")
        )
    }

    fn temporal_options(&self, fields: &[FieldInitialiser]) -> String {
        let entries = fields
            .iter()
            .map(|field| {
                let value = if field.name.text == "components" {
                    if let Expression::Object(object) = &field.value {
                        let components = object
                            .fields
                            .iter()
                            .map(|component| {
                                let value = match &component.value {
                                    Expression::Name(name) if name.path.len() == 1 => {
                                        ts_string(&name.path[0].text)
                                    }
                                    value => self.expression(value),
                                };
                                format!("{}: {value}", component.name.text)
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!("{{ {components} }}")
                    } else {
                        self.expression(&field.value)
                    }
                } else {
                    self.expression(&field.value)
                };
                format!("{}: {value}", field.name.text)
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{ {entries} }}")
    }

    fn object_literal(&self, fields: &[FieldInitialiser]) -> String {
        self.object_literal_from(fields.iter())
    }

    fn object_literal_for_create(&self, entity: &str, fields: &[FieldInitialiser]) -> String {
        let mut entries = fields
            .iter()
            .map(|field| format!("{}: {}", field.name.text, self.expression(&field.value)))
            .collect::<Vec<_>>();
        if let Some(record) = self.records.get(entity) {
            if let Some(initial) = record
                .dossier
                .as_ref()
                .and_then(|dossier| dossier.lifecycle.as_ref())
                .and_then(|lifecycle| lifecycle.initial.as_ref())
            {
                entries.extend(initial.iter().map(|field| {
                    format!("{}: {}", field.name.text, self.expression(&field.value))
                }));
            }
            entries.extend(record.fields.iter().filter_map(|field| {
                field.generated.map(|role| {
                    let value = match role {
                        jadpo_syntax::GeneratedFieldRole::Identity => "crypto.randomUUID()",
                        jadpo_syntax::GeneratedFieldRole::Create
                        | jadpo_syntax::GeneratedFieldRole::CreateOrChange => "__operation.now",
                    };
                    format!("{}: {value}", field.name.text)
                })
            }));
        }
        format!("{{ {} }}", entries.join(", "))
    }

    fn object_literal_from<'field>(
        &self,
        fields: impl IntoIterator<Item = &'field FieldInitialiser>,
    ) -> String {
        let fields = fields
            .into_iter()
            .map(|field| format!("{}: {}", field.name.text, self.expression(&field.value)))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{ {fields} }}")
    }

    fn domain_failure_arguments(&self, rejection: &jadpo_syntax::RejectStatement) -> String {
        let Some(declaration) = self.failures.get(&rejection.failure.text) else {
            return format!("{}, {{}}, {{}}", ts_string(&rejection.failure.text),);
        };
        let public_names = declaration
            .public_fields
            .iter()
            .map(|field| field.name.text.as_str())
            .collect::<BTreeSet<_>>();
        let internal_names = declaration
            .internal_fields
            .iter()
            .map(|field| field.name.text.as_str())
            .collect::<BTreeSet<_>>();
        format!(
            "{}, {}, {}",
            ts_string(&rejection.failure.text),
            self.object_literal_from(
                rejection
                    .values
                    .iter()
                    .filter(|field| public_names.contains(field.name.text.as_str())),
            ),
            self.object_literal_from(
                rejection
                    .values
                    .iter()
                    .filter(|field| internal_names.contains(field.name.text.as_str())),
            ),
        )
    }

    fn ts_type(&self, reference: &TypeReference) -> String {
        let raw_name = type_name(reference);
        let principal_variant = self.principal.and_then(|principal| {
            raw_name
                .strip_prefix(&format!("{}.", principal.name.text))
                .filter(|variant| !variant.contains('.'))
                .map(|variant| {
                    format!(
                        "Extract<{}, {{ readonly tag: {} }}>",
                        principal.name.text,
                        ts_string(variant)
                    )
                })
        });
        let name = principal_variant.unwrap_or_else(|| self.schema_declaration(&raw_name));
        let base = if name == "List" && reference.arguments.len() == 1 {
            format!("Array<{}>", self.ts_type(&reference.arguments[0]))
        } else if name == "Set" && reference.arguments.len() == 1 {
            format!("ReadonlySet<{}>", self.ts_type(&reference.arguments[0]))
        } else if name == "Map" && reference.arguments.len() == 2 {
            format!(
                "ReadonlyMap<{}, {}>",
                self.ts_type(&reference.arguments[0]),
                self.ts_type(&reference.arguments[1])
            )
        } else {
            match name.as_str() {
                "Bool" => "boolean".to_owned(),
                "Int" | "Decimal" => "number".to_owned(),
                "Text" | "Uuid" => "string".to_owned(),
                "Instant" | "CalendarDate" | "Duration" | "Zone" | "Locale"
                | "PresentationText" => name,
                "Time" | "InstantRange" => name,
                "Bytes" => "Uint8Array".to_owned(),
                "Unit" => "void".to_owned(),
                _ => name,
            }
        };
        if self.reference_is_nullable(reference) {
            format!("{base} | null")
        } else {
            base
        }
    }

    fn authentication_ts_type(&self, reference: &TypeReference) -> String {
        let root = self.representation_root_for(reference);
        let base = if let Some(declaration) = self.enums.get(&root) {
            declaration
                .variants
                .iter()
                .map(|variant| ts_string(&variant.name.text))
                .collect::<Vec<_>>()
                .join(" | ")
        } else {
            match root.as_str() {
                "Bool" => "boolean".to_owned(),
                "Int" | "Decimal" => "number".to_owned(),
                _ => "string".to_owned(),
            }
        };
        if self.reference_is_nullable(reference) {
            format!("{base} | null")
        } else {
            base
        }
    }

    fn authentication_validation_expression(
        &self,
        reference: &TypeReference,
        value: &str,
        path: &str,
    ) -> String {
        let root = self.representation_root_for(reference);
        let validated = if let Some(declaration) = self.enums.get(&root) {
            let values = declaration
                .variants
                .iter()
                .map(|variant| ts_string(&variant.name.text))
                .collect::<Vec<_>>()
                .join(", ");
            format!("authEnum({value}, {path}, [{values}])")
        } else {
            let validator = match root.as_str() {
                "Bool" => "authBool",
                "Int" => "authInt",
                "Decimal" => "authDecimal",
                "Uuid" => "authUuid",
                _ => "authText",
            };
            format!("{validator}({value}, {path})")
        };
        if self.reference_is_nullable(reference) {
            format!("{value} === null ? null : {validated}")
        } else {
            validated
        }
    }

    fn validation_expression(&self, reference: &TypeReference, value: &str, path: &str) -> String {
        let raw_name = type_name(reference);
        if let Some(validator) = self.field_validator_name(&raw_name) {
            let validation = format!("{validator}({value}, {path})");
            return if reference.nullable {
                format!("((candidate) => candidate === null ? null : {validator}(candidate, {path}))({value})")
            } else {
                validation
            };
        }
        let name = self.schema_declaration(&raw_name);
        let validation = if name == "List" && reference.arguments.len() == 1 {
            let item = self.validation_expression(
                &reference.arguments[0],
                "item",
                &format!("{path} + \"[\" + index + \"]\""),
            );
            format!(
                "Array.isArray({value}) ? {value}.map((item, index) => {item}) : invalid({path}, \"List\")"
            )
        } else if name == "Set" && reference.arguments.len() == 1 {
            let item = self.validation_expression(
                &reference.arguments[0],
                "item",
                &format!("{path} + \"[\" + index + \"]\""),
            );
            format!("{value} instanceof Set ? new Set([...{value}].map((item, index) => {item})) : invalid({path}, \"Set\")")
        } else if name == "Map" && reference.arguments.len() == 2 {
            let key = self.validation_expression(
                &reference.arguments[0],
                "entry[0]",
                &format!("{path} + \".key[\" + index + \"]\""),
            );
            let item = self.validation_expression(
                &reference.arguments[1],
                "entry[1]",
                &format!("{path} + \".value[\" + index + \"]\""),
            );
            format!("{value} instanceof Map ? new Map([...{value}].map((entry, index) => [{key}, {item}])) : invalid({path}, \"Map\")")
        } else if self.types.contains_key(&name)
            || self.enums.contains_key(&name)
            || self.records.contains_key(&name)
            || matches!(name.as_str(), "Email" | "Url" | "IpAddress")
        {
            format!("validate_{name}({value}, {path})")
        } else {
            match name.as_str() {
                "Bool" => format!("validateBool({value}, {path})"),
                "Int" => format!("validateInt({value}, {path})"),
                "Decimal" => format!("validateDecimal({value}, {path})"),
                "Text" => format!("validateText({value}, {path})"),
                "Uuid" => format!("validateUuid({value}, {path})"),
                "Instant" => format!("validateInstant({value}, {path})"),
                "CalendarDate" => format!("validateCalendarDate({value}, {path})"),
                "Zone" => format!("validateZone({value}, {path})"),
                "Locale" => format!("validateLocale({value}, {path})"),
                "Time" => format!("validateTime({value}, {path})"),
                "Duration" => format!("validateDuration({value}, {path})"),
                "Bytes" => {
                    format!("{value} instanceof Uint8Array ? {value} : invalid({path}, \"Bytes\")")
                }
                "Object" => format!("expectObject({value}, {path})"),
                _ => format!("invalid({path}, {})", ts_string(&name)),
            }
        };
        if self.reference_is_nullable(reference) {
            format!("{value} === null ? null : {validation}")
        } else {
            validation
        }
    }

    fn query_decode_expression(&self, reference: &TypeReference, value: &str) -> String {
        let name = self.schema_declaration(&type_name(reference));
        if self.records.contains_key(&name) {
            return format!("queryJson({value})");
        }
        if self.enums.contains_key(&name) {
            return value.to_owned();
        }
        match self.representation_root_for(reference).as_str() {
            "Int" => format!("queryInteger({value})"),
            "Decimal" => format!("queryDecimal({value})"),
            "Bool" => format!("queryBoolean({value})"),
            _ => value.to_owned(),
        }
    }

    fn database_decode_expression(
        &self,
        reference: &TypeReference,
        value: &str,
        path: &str,
    ) -> String {
        let root = self.representation_root_for(reference);
        let decoded = match root.as_str() {
            "Instant" => format!("decodeDatabaseInstant({value}, {path})"),
            "CalendarDate" => format!("decodeDatabaseCalendarDate({value}, {path})"),
            "Duration" => format!("decodeDatabaseDuration({value}, {path})"),
            "Time" => format!("decodeDatabaseTime({value}, {path})"),
            _ => value.to_owned(),
        };
        if self.reference_is_nullable(reference) {
            format!("{value} === null ? null : {decoded}")
        } else {
            decoded
        }
    }

    fn reference_is_nullable(&self, reference: &TypeReference) -> bool {
        reference.nullable
            || self
                .project
                .semantics
                .nullable_types
                .contains(&type_name(reference))
    }

    fn field_validator_name(&self, name: &str) -> Option<String> {
        let (owner, field_name) = name.split_once('.')?;
        self.records
            .get(owner)?
            .fields
            .iter()
            .find(|field| field.name.text == field_name)
            .map(|_| format!("validateField_{}_{owner}_{field_name}", owner.len()))
    }

    fn field_validation_expression(
        &self,
        field: &FieldDeclaration,
        value: &str,
        path: &str,
    ) -> String {
        // Evaluate the supplied expression once, then validate the entire
        // parent chain before applying this field's own constraints.
        let parent = self.validation_expression(&field.field_type, "raw", "fieldPath");
        let mut body = format!("((raw, fieldPath) => {{ const candidate = {parent}; ");
        if self.reference_is_nullable(&field.field_type) {
            body.push_str("if (candidate === null) return null; ");
        }
        self.constraint_checks(&mut body, "candidate", "fieldPath", &field.constraints, 0);
        body.push_str(&format!("return candidate; }})({value}, {path})"));
        body
    }

    fn constraint_checks(
        &self,
        output: &mut String,
        value: &str,
        path: &str,
        constraints: &[Constraint],
        indent: usize,
    ) {
        let padding = " ".repeat(indent);
        for constraint in constraints {
            let check = match constraint.kind {
                ConstraintKind::Min => format!("{value} < {}", constraint.value.text),
                ConstraintKind::Max => format!("{value} > {}", constraint.value.text),
                ConstraintKind::MinLength => {
                    // Match Rust's chars().count() for text while retaining
                    // element counts for collection constraints.
                    format!(
                        "(typeof {value} === \"string\" ? [...{value}].length : {value}.length) < {}",
                        constraint.value.text
                    )
                }
                ConstraintKind::MaxLength => {
                    format!(
                        "(typeof {value} === \"string\" ? [...{value}].length : {value}.length) > {}",
                        constraint.value.text
                    )
                }
                ConstraintKind::Pattern if unquote(&constraint.value.text) == "[a-z0-9_]+" => {
                    format!("!/^[a-z0-9_]+$/u.test({value})")
                }
                ConstraintKind::Pattern => continue,
                ConstraintKind::Format if unquote(&constraint.value.text) == "email" => {
                    format!("!isEmail({value})")
                }
                ConstraintKind::Format => continue,
            };
            line(
                output,
                &format!(
                    "{padding}if ({check}) invalid({path}, {});",
                    ts_string(constraint_name(constraint.kind))
                ),
            );
        }
    }

    fn schema_declaration(&self, raw_name: &str) -> String {
        let name = raw_name.trim_end_matches('?');
        if let Some(owner) = name.strip_suffix(".Ref") {
            if let Some(identity) = self.records.get(owner).and_then(|record| {
                record
                    .fields
                    .iter()
                    .find(|field| field.persistence.contains(&PersistenceModifier::Identity))
            }) {
                return self.schema_declaration(&type_name(&identity.field_type));
            }
        }
        if matches!(name, "Email" | "Url" | "IpAddress") {
            return name.to_owned();
        }
        if self.types.contains_key(name)
            || self.enums.contains_key(name)
            || self.records.contains_key(name)
        {
            return name.to_owned();
        }
        let mut current = name.to_owned();
        let mut visited = BTreeSet::new();
        while visited.insert(current.clone()) {
            if matches!(current.as_str(), "Email" | "Url" | "IpAddress") {
                return current;
            }
            let Some(node) = self.project.semantics.node(&current) else {
                break;
            };
            let Some(edge) = self
                .project
                .semantics
                .refinements
                .iter()
                .find(|edge| edge.refined == node.id)
            else {
                break;
            };
            current.clone_from(&self.project.semantics.nodes[edge.parent.0 as usize].name);
            if self.types.contains_key(&current)
                || self.enums.contains_key(&current)
                || self.records.contains_key(&current)
            {
                return current;
            }
        }
        current
    }
}

fn type_name(reference: &TypeReference) -> String {
    reference
        .path
        .iter()
        .map(|part| part.text.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

fn method_name(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Delete => "DELETE",
    }
}

fn constraint_name(kind: ConstraintKind) -> &'static str {
    match kind {
        ConstraintKind::Min => "minimum",
        ConstraintKind::Max => "maximum",
        ConstraintKind::MinLength => "minimum length",
        ConstraintKind::MaxLength => "maximum length",
        ConstraintKind::Pattern => "declared pattern",
        ConstraintKind::Format => "declared format",
    }
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(value)
}

fn decode_string_literal(value: &str) -> Option<String> {
    let value = value.strip_prefix('"')?.strip_suffix('"')?;
    let mut decoded = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        decoded.push(match characters.next()? {
            '"' => '"',
            '\\' => '\\',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            _ => return None,
        });
    }
    Some(decoded)
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
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

fn zone_name_for_variant(variant: &str) -> Option<String> {
    include_str!("../../../data/iana-zones-2026c.txt")
        .lines()
        .find(|zone| standard_variant_name(zone) == variant)
        .map(str::to_owned)
}

fn is_temporal_enum(name: &str) -> bool {
    matches!(
        name,
        "Zone"
            | "Locale"
            | "LocalOverlap"
            | "LocalGap"
            | "InvalidDay"
            | "Weekday"
            | "TimeFormat"
            | "FriendlyTimeFormat"
    )
}

fn ts_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t");
    format!("\"{escaped}\"")
}

fn ts_callable_name(value: &str) -> String {
    value.replace('.', "__")
}

fn line(output: &mut String, value: &str) {
    output.push_str(value);
    output.push('\n');
}

fn collect_query_expressions<'expression>(
    block: &'expression Block,
    queries: &mut Vec<&'expression jadpo_syntax::QueryExpression>,
) {
    for statement in &block.statements {
        match statement {
            Statement::Binding(statement) => {
                collect_queries_from_expression(&statement.value, queries)
            }
            Statement::Assignment(statement) => {
                collect_queries_from_expression(&statement.value, queries)
            }
            Statement::Return(statement) => {
                collect_queries_from_expression(&statement.value, queries)
            }
            Statement::Reject(statement) => {
                for field in &statement.values {
                    collect_queries_from_expression(&field.value, queries);
                }
            }
            Statement::If(statement) => {
                collect_queries_from_expression(&statement.condition, queries);
                collect_query_expressions(&statement.then_block, queries);
                if let Some(block) = &statement.else_block {
                    collect_query_expressions(block, queries);
                }
            }
            Statement::Match(statement) => {
                collect_queries_from_expression(&statement.subject, queries);
                for arm in &statement.arms {
                    collect_query_expressions(&arm.body, queries);
                }
            }
            Statement::Assert(statement) => {
                collect_queries_from_expression(&statement.condition, queries)
            }
            Statement::AdvanceClock(statement) => {
                collect_queries_from_expression(&statement.duration, queries)
            }
            Statement::Unsupported(_) => {}
        }
    }
}

fn block_uses_runtime_name(block: &Block, requested: &str) -> bool {
    block.statements.iter().any(|statement| match statement {
        Statement::Binding(statement) => expression_uses_runtime_name(&statement.value, requested),
        Statement::Assignment(statement) => {
            expression_uses_runtime_name(&statement.value, requested)
        }
        Statement::Return(statement) => expression_uses_runtime_name(&statement.value, requested),
        Statement::Reject(statement) => statement
            .values
            .iter()
            .any(|field| expression_uses_runtime_name(&field.value, requested)),
        Statement::If(statement) => {
            expression_uses_runtime_name(&statement.condition, requested)
                || block_uses_runtime_name(&statement.then_block, requested)
                || statement
                    .else_block
                    .as_ref()
                    .is_some_and(|block| block_uses_runtime_name(block, requested))
        }
        Statement::Match(statement) => {
            expression_uses_runtime_name(&statement.subject, requested)
                || statement
                    .arms
                    .iter()
                    .any(|arm| block_uses_runtime_name(&arm.body, requested))
        }
        Statement::Assert(statement) => {
            expression_uses_runtime_name(&statement.condition, requested)
        }
        Statement::AdvanceClock(statement) => {
            expression_uses_runtime_name(&statement.duration, requested)
        }
        Statement::Unsupported(_) => false,
    })
}

fn expression_uses_runtime_name(expression: &Expression, requested: &str) -> bool {
    match expression {
        Expression::Name(name) => name.path.first().is_some_and(|name| name.text == requested),
        Expression::Invocation(invocation) => {
            invocation
                .arguments
                .iter()
                .any(|argument| expression_uses_runtime_name(argument, requested))
                || invocation
                    .named_arguments
                    .iter()
                    .any(|argument| expression_uses_runtime_name(&argument.value, requested))
        }
        Expression::TestCall(call) => {
            call.invocation
                .arguments
                .iter()
                .any(|argument| expression_uses_runtime_name(argument, requested))
                || call
                    .invocation
                    .named_arguments
                    .iter()
                    .any(|argument| expression_uses_runtime_name(&argument.value, requested))
        }
        Expression::Construction(construction) => construction
            .fields
            .iter()
            .any(|field| expression_uses_runtime_name(&field.value, requested)),
        Expression::Object(object) => object
            .fields
            .iter()
            .any(|field| expression_uses_runtime_name(&field.value, requested)),
        Expression::Create(create) => create
            .fields
            .iter()
            .any(|field| expression_uses_runtime_name(&field.value, requested)),
        Expression::Query(query) => {
            expression_uses_runtime_name(&query.value, requested)
                || query.page.as_ref().is_some_and(|page| {
                    page.predicates
                        .iter()
                        .any(|predicate| expression_uses_runtime_name(&predicate.value, requested))
                        || expression_uses_runtime_name(&page.after, requested)
                        || expression_uses_runtime_name(&page.limit, requested)
                })
                || query.pagination.as_ref().is_some_and(|pagination| {
                    expression_uses_runtime_name(&pagination.limit, requested)
                        || expression_uses_runtime_name(&pagination.offset, requested)
                })
                || query.includes.iter().any(|include| {
                    expression_uses_runtime_name(&include.pagination.limit, requested)
                        || expression_uses_runtime_name(&include.pagination.offset, requested)
                })
        }
        Expression::Update(update) => {
            expression_uses_runtime_name(&update.value, requested)
                || update
                    .changes
                    .iter()
                    .any(|field| expression_uses_runtime_name(&field.value, requested))
                || update.conditional_changes.iter().any(|conditional| {
                    expression_uses_runtime_name(&conditional.change.value, requested)
                })
                || update.empty.as_ref().is_some_and(|rejection| {
                    rejection
                        .values
                        .iter()
                        .any(|field| expression_uses_runtime_name(&field.value, requested))
                })
        }
        Expression::Delete(delete) => expression_uses_runtime_name(&delete.value, requested),
        Expression::Binary(binary) => {
            expression_uses_runtime_name(&binary.left, requested)
                || expression_uses_runtime_name(&binary.right, requested)
        }
        Expression::Unary(unary) => expression_uses_runtime_name(&unary.value, requested),
        Expression::Grouped(grouped) => expression_uses_runtime_name(&grouped.value, requested),
        Expression::Attempt(attempt) => expression_uses_runtime_name(&attempt.value, requested),
        Expression::OutcomeMatch(outcome) => {
            expression_uses_runtime_name(&outcome.subject, requested)
                || outcome.arms.iter().any(|arm| match &arm.body {
                    jadpo_syntax::OutcomeMatchArmBody::Value(value) => {
                        expression_uses_runtime_name(value, requested)
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Reject(rejection) => rejection
                        .values
                        .iter()
                        .any(|field| expression_uses_runtime_name(&field.value, requested)),
                    jadpo_syntax::OutcomeMatchArmBody::Propagate(_) => false,
                })
        }
        Expression::Literal(_) | Expression::Missing(_) => false,
    }
}

fn collect_queries_from_expression<'expression>(
    expression: &'expression Expression,
    queries: &mut Vec<&'expression jadpo_syntax::QueryExpression>,
) {
    match expression {
        Expression::Query(query) => {
            queries.push(query);
            collect_queries_from_expression(&query.value, queries);
            if let Some(page) = &query.page {
                for predicate in &page.predicates {
                    collect_queries_from_expression(&predicate.value, queries);
                }
                collect_queries_from_expression(&page.after, queries);
                collect_queries_from_expression(&page.limit, queries);
            }
            if let Some(pagination) = &query.pagination {
                collect_queries_from_expression(&pagination.limit, queries);
                collect_queries_from_expression(&pagination.offset, queries);
            }
            for include in &query.includes {
                collect_queries_from_expression(&include.pagination.limit, queries);
                collect_queries_from_expression(&include.pagination.offset, queries);
            }
        }
        Expression::Invocation(invocation) => {
            for argument in &invocation.arguments {
                collect_queries_from_expression(argument, queries);
            }
            for argument in &invocation.named_arguments {
                collect_queries_from_expression(&argument.value, queries);
            }
        }
        Expression::TestCall(call) => {
            for argument in &call.invocation.arguments {
                collect_queries_from_expression(argument, queries);
            }
            for argument in &call.invocation.named_arguments {
                collect_queries_from_expression(&argument.value, queries);
            }
        }
        Expression::Construction(construction) => {
            for field in &construction.fields {
                collect_queries_from_expression(&field.value, queries);
            }
        }
        Expression::Object(object) => {
            for field in &object.fields {
                collect_queries_from_expression(&field.value, queries);
            }
        }
        Expression::Create(create) => {
            for field in &create.fields {
                collect_queries_from_expression(&field.value, queries);
            }
        }
        Expression::Update(update) => {
            collect_queries_from_expression(&update.value, queries);
            for field in &update.changes {
                collect_queries_from_expression(&field.value, queries);
            }
            for conditional in &update.conditional_changes {
                collect_queries_from_expression(&conditional.change.value, queries);
            }
            if let Some(empty) = &update.empty {
                for field in &empty.values {
                    collect_queries_from_expression(&field.value, queries);
                }
            }
        }
        Expression::Delete(delete) => collect_queries_from_expression(&delete.value, queries),
        Expression::Binary(binary) => {
            collect_queries_from_expression(&binary.left, queries);
            collect_queries_from_expression(&binary.right, queries);
        }
        Expression::Unary(unary) => collect_queries_from_expression(&unary.value, queries),
        Expression::Grouped(grouped) => collect_queries_from_expression(&grouped.value, queries),
        Expression::Attempt(attempt) => collect_queries_from_expression(&attempt.value, queries),
        Expression::OutcomeMatch(outcome) => {
            collect_queries_from_expression(&outcome.subject, queries);
            for arm in &outcome.arms {
                match &arm.body {
                    jadpo_syntax::OutcomeMatchArmBody::Value(value) => {
                        collect_queries_from_expression(value, queries)
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Reject(rejection) => {
                        for field in &rejection.values {
                            collect_queries_from_expression(&field.value, queries);
                        }
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Propagate(_) => {}
                }
            }
        }
        Expression::Literal(_) | Expression::Name(_) | Expression::Missing(_) => {}
    }
}

fn collect_update_expressions<'expression>(
    block: &'expression Block,
    updates: &mut Vec<&'expression jadpo_syntax::UpdateExpression>,
) {
    for statement in &block.statements {
        match statement {
            Statement::Binding(statement) => {
                collect_updates_from_expression(&statement.value, updates)
            }
            Statement::Assignment(statement) => {
                collect_updates_from_expression(&statement.value, updates)
            }
            Statement::Return(statement) => {
                collect_updates_from_expression(&statement.value, updates)
            }
            Statement::Reject(statement) => {
                for field in &statement.values {
                    collect_updates_from_expression(&field.value, updates);
                }
            }
            Statement::If(statement) => {
                collect_updates_from_expression(&statement.condition, updates);
                collect_update_expressions(&statement.then_block, updates);
                if let Some(block) = &statement.else_block {
                    collect_update_expressions(block, updates);
                }
            }
            Statement::Match(statement) => {
                collect_updates_from_expression(&statement.subject, updates);
                for arm in &statement.arms {
                    collect_update_expressions(&arm.body, updates);
                }
            }
            Statement::Assert(statement) => {
                collect_updates_from_expression(&statement.condition, updates)
            }
            Statement::AdvanceClock(statement) => {
                collect_updates_from_expression(&statement.duration, updates)
            }
            Statement::Unsupported(_) => {}
        }
    }
}

fn patch_derived_changes(
    update: &jadpo_syntax::UpdateExpression,
) -> Vec<(&FieldInitialiser, Option<&jadpo_syntax::NameExpression>)> {
    update
        .changes
        .iter()
        .map(|change| (change, None))
        .chain(
            update
                .conditional_changes
                .iter()
                .map(|conditional| (&conditional.change, Some(&conditional.supplied))),
        )
        .collect()
}

fn patch_method_suffix(
    update: &jadpo_syntax::UpdateExpression,
    patch_fields: &[&FieldDeclaration],
) -> String {
    let patch = patch_fields
        .iter()
        .map(|field| field.name.text.as_str())
        .collect::<Vec<_>>()
        .join("_and_");
    let derived = patch_derived_changes(update)
        .iter()
        .map(|(change, _)| change.name.text.as_str())
        .collect::<Vec<_>>()
        .join("_and_");
    if derived.is_empty() {
        format!("patch_{patch}")
    } else {
        format!("patch_{patch}_set_{derived}")
    }
}

fn collect_updates_from_expression<'expression>(
    expression: &'expression Expression,
    updates: &mut Vec<&'expression jadpo_syntax::UpdateExpression>,
) {
    match expression {
        Expression::Update(update) => {
            updates.push(update);
            collect_updates_from_expression(&update.value, updates);
            for change in &update.changes {
                collect_updates_from_expression(&change.value, updates);
            }
            for conditional in &update.conditional_changes {
                collect_updates_from_expression(&conditional.change.value, updates);
            }
            if let Some(empty) = &update.empty {
                for field in &empty.values {
                    collect_updates_from_expression(&field.value, updates);
                }
            }
        }
        Expression::Invocation(invocation) => {
            for argument in &invocation.arguments {
                collect_updates_from_expression(argument, updates);
            }
            for argument in &invocation.named_arguments {
                collect_updates_from_expression(&argument.value, updates);
            }
        }
        Expression::TestCall(call) => {
            for argument in &call.invocation.arguments {
                collect_updates_from_expression(argument, updates);
            }
            for argument in &call.invocation.named_arguments {
                collect_updates_from_expression(&argument.value, updates);
            }
        }
        Expression::Construction(construction) => {
            for field in &construction.fields {
                collect_updates_from_expression(&field.value, updates);
            }
        }
        Expression::Object(object) => {
            for field in &object.fields {
                collect_updates_from_expression(&field.value, updates);
            }
        }
        Expression::Create(create) => {
            for field in &create.fields {
                collect_updates_from_expression(&field.value, updates);
            }
        }
        Expression::Query(query) => {
            collect_updates_from_expression(&query.value, updates);
            if let Some(page) = &query.page {
                for predicate in &page.predicates {
                    collect_updates_from_expression(&predicate.value, updates);
                }
                collect_updates_from_expression(&page.after, updates);
                collect_updates_from_expression(&page.limit, updates);
            }
            if let Some(pagination) = &query.pagination {
                collect_updates_from_expression(&pagination.limit, updates);
                collect_updates_from_expression(&pagination.offset, updates);
            }
            for include in &query.includes {
                collect_updates_from_expression(&include.pagination.limit, updates);
                collect_updates_from_expression(&include.pagination.offset, updates);
            }
        }
        Expression::Delete(delete) => collect_updates_from_expression(&delete.value, updates),
        Expression::Binary(binary) => {
            collect_updates_from_expression(&binary.left, updates);
            collect_updates_from_expression(&binary.right, updates);
        }
        Expression::Unary(unary) => collect_updates_from_expression(&unary.value, updates),
        Expression::Grouped(grouped) => collect_updates_from_expression(&grouped.value, updates),
        Expression::Attempt(attempt) => collect_updates_from_expression(&attempt.value, updates),
        Expression::OutcomeMatch(outcome) => {
            collect_updates_from_expression(&outcome.subject, updates);
            for arm in &outcome.arms {
                match &arm.body {
                    jadpo_syntax::OutcomeMatchArmBody::Value(value) => {
                        collect_updates_from_expression(value, updates)
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Reject(rejection) => {
                        for field in &rejection.values {
                            collect_updates_from_expression(&field.value, updates);
                        }
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Propagate(_) => {}
                }
            }
        }
        Expression::Literal(_) | Expression::Name(_) | Expression::Missing(_) => {}
    }
}

fn block_contains_persistence(block: &Block) -> bool {
    block.statements.iter().any(|statement| match statement {
        Statement::Binding(statement) => expression_contains_persistence(&statement.value),
        Statement::Assignment(statement) => expression_contains_persistence(&statement.value),
        Statement::Return(statement) => expression_contains_persistence(&statement.value),
        Statement::Reject(statement) => statement
            .values
            .iter()
            .any(|field| expression_contains_persistence(&field.value)),
        Statement::If(statement) => {
            expression_contains_persistence(&statement.condition)
                || block_contains_persistence(&statement.then_block)
                || statement
                    .else_block
                    .as_ref()
                    .is_some_and(block_contains_persistence)
        }
        Statement::Match(statement) => {
            expression_contains_persistence(&statement.subject)
                || statement
                    .arms
                    .iter()
                    .any(|arm| block_contains_persistence(&arm.body))
        }
        Statement::Assert(statement) => expression_contains_persistence(&statement.condition),
        Statement::AdvanceClock(statement) => expression_contains_persistence(&statement.duration),
        Statement::Unsupported(_) => false,
    })
}

fn block_contains_test_call(block: &Block) -> bool {
    block.statements.iter().any(|statement| match statement {
        Statement::Binding(statement) => expression_contains_test_call(&statement.value),
        Statement::Assignment(statement) => expression_contains_test_call(&statement.value),
        Statement::Return(statement) => expression_contains_test_call(&statement.value),
        Statement::Reject(statement) => statement
            .values
            .iter()
            .any(|field| expression_contains_test_call(&field.value)),
        Statement::If(statement) => {
            expression_contains_test_call(&statement.condition)
                || block_contains_test_call(&statement.then_block)
                || statement
                    .else_block
                    .as_ref()
                    .is_some_and(block_contains_test_call)
        }
        Statement::Match(statement) => {
            expression_contains_test_call(&statement.subject)
                || statement
                    .arms
                    .iter()
                    .any(|arm| block_contains_test_call(&arm.body))
        }
        Statement::Assert(statement) => expression_contains_test_call(&statement.condition),
        Statement::AdvanceClock(statement) => expression_contains_test_call(&statement.duration),
        Statement::Unsupported(_) => false,
    })
}

fn expression_contains_test_call(expression: &Expression) -> bool {
    match expression {
        Expression::TestCall(_) => true,
        Expression::Invocation(invocation) => {
            invocation
                .arguments
                .iter()
                .any(expression_contains_test_call)
                || invocation
                    .named_arguments
                    .iter()
                    .any(|argument| expression_contains_test_call(&argument.value))
        }
        Expression::Construction(value) => value
            .fields
            .iter()
            .any(|field| expression_contains_test_call(&field.value)),
        Expression::Object(value) => value
            .fields
            .iter()
            .any(|field| expression_contains_test_call(&field.value)),
        Expression::Create(value) => value
            .fields
            .iter()
            .any(|field| expression_contains_test_call(&field.value)),
        Expression::Query(value) => {
            expression_contains_test_call(&value.value)
                || value.page.as_ref().is_some_and(|page| {
                    page.predicates
                        .iter()
                        .any(|predicate| expression_contains_test_call(&predicate.value))
                        || expression_contains_test_call(&page.after)
                        || expression_contains_test_call(&page.limit)
                })
                || value.pagination.as_ref().is_some_and(|pagination| {
                    expression_contains_test_call(&pagination.limit)
                        || expression_contains_test_call(&pagination.offset)
                })
                || value.includes.iter().any(|include| {
                    expression_contains_test_call(&include.pagination.limit)
                        || expression_contains_test_call(&include.pagination.offset)
                })
        }
        Expression::Update(value) => {
            expression_contains_test_call(&value.value)
                || value
                    .changes
                    .iter()
                    .any(|field| expression_contains_test_call(&field.value))
                || value
                    .conditional_changes
                    .iter()
                    .any(|change| expression_contains_test_call(&change.change.value))
        }
        Expression::Delete(value) => expression_contains_test_call(&value.value),
        Expression::Attempt(value) => expression_contains_test_call(&value.value),
        Expression::OutcomeMatch(value) => {
            expression_contains_test_call(&value.subject)
                || value.arms.iter().any(|arm| match &arm.body {
                    jadpo_syntax::OutcomeMatchArmBody::Value(value) => {
                        expression_contains_test_call(value)
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Reject(value) => value
                        .values
                        .iter()
                        .any(|field| expression_contains_test_call(&field.value)),
                    jadpo_syntax::OutcomeMatchArmBody::Propagate(_) => false,
                })
        }
        Expression::Unary(value) => expression_contains_test_call(&value.value),
        Expression::Binary(value) => {
            expression_contains_test_call(&value.left)
                || expression_contains_test_call(&value.right)
        }
        Expression::Grouped(value) => expression_contains_test_call(&value.value),
        Expression::Literal(_) | Expression::Name(_) | Expression::Missing(_) => false,
    }
}

fn expression_contains_persistence(expression: &Expression) -> bool {
    match expression {
        Expression::Create(_)
        | Expression::Query(_)
        | Expression::Update(_)
        | Expression::Delete(_) => true,
        Expression::Invocation(invocation) => {
            invocation
                .arguments
                .iter()
                .any(expression_contains_persistence)
                || invocation
                    .named_arguments
                    .iter()
                    .any(|argument| expression_contains_persistence(&argument.value))
        }
        Expression::TestCall(call) => {
            call.invocation
                .arguments
                .iter()
                .any(expression_contains_persistence)
                || call
                    .invocation
                    .named_arguments
                    .iter()
                    .any(|argument| expression_contains_persistence(&argument.value))
        }
        Expression::Construction(construction) => construction
            .fields
            .iter()
            .any(|field| expression_contains_persistence(&field.value)),
        Expression::Object(object) => object
            .fields
            .iter()
            .any(|field| expression_contains_persistence(&field.value)),
        Expression::Binary(binary) => {
            expression_contains_persistence(&binary.left)
                || expression_contains_persistence(&binary.right)
        }
        Expression::Unary(unary) => expression_contains_persistence(&unary.value),
        Expression::Grouped(grouped) => expression_contains_persistence(&grouped.value),
        Expression::Attempt(attempt) => expression_contains_persistence(&attempt.value),
        Expression::OutcomeMatch(outcome) => {
            expression_contains_persistence(&outcome.subject)
                || outcome.arms.iter().any(|arm| match &arm.body {
                    jadpo_syntax::OutcomeMatchArmBody::Value(value) => {
                        expression_contains_persistence(value)
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Reject(rejection) => rejection
                        .values
                        .iter()
                        .any(|field| expression_contains_persistence(&field.value)),
                    jadpo_syntax::OutcomeMatchArmBody::Propagate(_) => false,
                })
        }
        Expression::Literal(_) | Expression::Name(_) | Expression::Missing(_) => false,
    }
}

fn callable_may_suspend(
    name: &str,
    callables: &BTreeMap<String, &CallableDeclaration>,
    service_operations: &BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> bool {
    if !visiting.insert(name.to_owned()) {
        return false;
    }
    let result = callables.get(name).is_some_and(|callable| {
        callable.policy.is_some()
            || block_may_suspend(&callable.body, callables, service_operations, visiting)
    });
    visiting.remove(name);
    result
}

fn block_may_suspend(
    block: &Block,
    callables: &BTreeMap<String, &CallableDeclaration>,
    service_operations: &BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> bool {
    block.statements.iter().any(|statement| match statement {
        Statement::Binding(statement) => {
            expression_may_suspend(&statement.value, callables, service_operations, visiting)
        }
        Statement::Assignment(statement) => {
            expression_may_suspend(&statement.value, callables, service_operations, visiting)
        }
        Statement::Return(statement) => {
            expression_may_suspend(&statement.value, callables, service_operations, visiting)
        }
        Statement::Reject(statement) => statement.values.iter().any(|field| {
            expression_may_suspend(&field.value, callables, service_operations, visiting)
        }),
        Statement::If(statement) => {
            expression_may_suspend(
                &statement.condition,
                callables,
                service_operations,
                visiting,
            ) || block_may_suspend(
                &statement.then_block,
                callables,
                service_operations,
                visiting,
            ) || statement.else_block.as_ref().is_some_and(|block| {
                block_may_suspend(block, callables, service_operations, visiting)
            })
        }
        Statement::Match(statement) => {
            expression_may_suspend(&statement.subject, callables, service_operations, visiting)
                || statement.arms.iter().any(|arm| {
                    block_may_suspend(&arm.body, callables, service_operations, visiting)
                })
        }
        Statement::Assert(statement) => expression_may_suspend(
            &statement.condition,
            callables,
            service_operations,
            visiting,
        ),
        Statement::AdvanceClock(statement) => {
            expression_may_suspend(&statement.duration, callables, service_operations, visiting)
        }
        Statement::Unsupported(_) => false,
    })
}

fn expression_may_suspend(
    expression: &Expression,
    callables: &BTreeMap<String, &CallableDeclaration>,
    service_operations: &BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> bool {
    match expression {
        Expression::Create(_)
        | Expression::Query(_)
        | Expression::Update(_)
        | Expression::Delete(_) => true,
        Expression::Invocation(invocation) => {
            invocation.arguments.iter().any(|argument| {
                expression_may_suspend(argument, callables, service_operations, visiting)
            }) || invocation.named_arguments.iter().any(|argument| {
                expression_may_suspend(&argument.value, callables, service_operations, visiting)
            }) || service_operations.contains(
                &invocation
                    .callee
                    .path
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
            ) || resolved_callable_name(&invocation.callee.path, callables).is_some_and(|callee| {
                callable_may_suspend(&callee, callables, service_operations, visiting)
            })
        }
        Expression::TestCall(call) => {
            let invocation = &call.invocation;
            invocation.arguments.iter().any(|argument| {
                expression_may_suspend(argument, callables, service_operations, visiting)
            }) || invocation.named_arguments.iter().any(|argument| {
                expression_may_suspend(&argument.value, callables, service_operations, visiting)
            }) || service_operations.contains(
                &invocation
                    .callee
                    .path
                    .iter()
                    .map(|part| part.text.as_str())
                    .collect::<Vec<_>>()
                    .join("."),
            ) || resolved_callable_name(&invocation.callee.path, callables).is_some_and(|callee| {
                callable_may_suspend(&callee, callables, service_operations, visiting)
            })
        }
        Expression::Construction(construction) => construction.fields.iter().any(|field| {
            expression_may_suspend(&field.value, callables, service_operations, visiting)
        }),
        Expression::Object(object) => object.fields.iter().any(|field| {
            expression_may_suspend(&field.value, callables, service_operations, visiting)
        }),
        Expression::Binary(binary) => {
            expression_may_suspend(&binary.left, callables, service_operations, visiting)
                || expression_may_suspend(&binary.right, callables, service_operations, visiting)
        }
        Expression::Unary(unary) => {
            expression_may_suspend(&unary.value, callables, service_operations, visiting)
        }
        Expression::Grouped(grouped) => {
            expression_may_suspend(&grouped.value, callables, service_operations, visiting)
        }
        Expression::Attempt(attempt) => {
            expression_may_suspend(&attempt.value, callables, service_operations, visiting)
        }
        Expression::OutcomeMatch(outcome) => {
            expression_may_suspend(&outcome.subject, callables, service_operations, visiting)
                || outcome.arms.iter().any(|arm| match &arm.body {
                    jadpo_syntax::OutcomeMatchArmBody::Value(value) => {
                        expression_may_suspend(value, callables, service_operations, visiting)
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Reject(rejection) => {
                        rejection.values.iter().any(|field| {
                            expression_may_suspend(
                                &field.value,
                                callables,
                                service_operations,
                                visiting,
                            )
                        })
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Propagate(_) => false,
                })
        }
        Expression::Literal(_) | Expression::Name(_) | Expression::Missing(_) => false,
    }
}

fn callable_is_mutative(
    name: &str,
    callables: &BTreeMap<String, &CallableDeclaration>,
    visiting: &mut BTreeSet<String>,
) -> bool {
    if !visiting.insert(name.to_owned()) {
        return false;
    }
    let result = callables
        .get(name)
        .is_some_and(|callable| block_contains_mutation(&callable.body, callables, visiting));
    visiting.remove(name);
    result
}

fn block_contains_mutation(
    block: &Block,
    callables: &BTreeMap<String, &CallableDeclaration>,
    visiting: &mut BTreeSet<String>,
) -> bool {
    block.statements.iter().any(|statement| match statement {
        Statement::Binding(statement) => {
            expression_contains_mutation(&statement.value, callables, visiting)
        }
        Statement::Assignment(statement) => {
            expression_contains_mutation(&statement.value, callables, visiting)
        }
        Statement::Return(statement) => {
            expression_contains_mutation(&statement.value, callables, visiting)
        }
        Statement::Reject(statement) => statement
            .values
            .iter()
            .any(|field| expression_contains_mutation(&field.value, callables, visiting)),
        Statement::If(statement) => {
            expression_contains_mutation(&statement.condition, callables, visiting)
                || block_contains_mutation(&statement.then_block, callables, visiting)
                || statement
                    .else_block
                    .as_ref()
                    .is_some_and(|block| block_contains_mutation(block, callables, visiting))
        }
        Statement::Match(statement) => {
            expression_contains_mutation(&statement.subject, callables, visiting)
                || statement
                    .arms
                    .iter()
                    .any(|arm| block_contains_mutation(&arm.body, callables, visiting))
        }
        Statement::Assert(statement) => {
            expression_contains_mutation(&statement.condition, callables, visiting)
        }
        Statement::AdvanceClock(statement) => {
            expression_contains_mutation(&statement.duration, callables, visiting)
        }
        Statement::Unsupported(_) => false,
    })
}

fn expression_contains_mutation(
    expression: &Expression,
    callables: &BTreeMap<String, &CallableDeclaration>,
    visiting: &mut BTreeSet<String>,
) -> bool {
    match expression {
        Expression::Create(_) | Expression::Update(_) | Expression::Delete(_) => true,
        Expression::Query(query) => {
            expression_contains_mutation(&query.value, callables, visiting)
                || query.page.as_ref().is_some_and(|page| {
                    page.predicates.iter().any(|predicate| {
                        expression_contains_mutation(&predicate.value, callables, visiting)
                    }) || expression_contains_mutation(&page.after, callables, visiting)
                        || expression_contains_mutation(&page.limit, callables, visiting)
                })
                || query.pagination.as_ref().is_some_and(|pagination| {
                    expression_contains_mutation(&pagination.limit, callables, visiting)
                        || expression_contains_mutation(&pagination.offset, callables, visiting)
                })
                || query.includes.iter().any(|include| {
                    expression_contains_mutation(&include.pagination.limit, callables, visiting)
                        || expression_contains_mutation(
                            &include.pagination.offset,
                            callables,
                            visiting,
                        )
                })
        }
        Expression::Invocation(invocation) => {
            invocation
                .arguments
                .iter()
                .any(|argument| expression_contains_mutation(argument, callables, visiting))
                || invocation.named_arguments.iter().any(|argument| {
                    expression_contains_mutation(&argument.value, callables, visiting)
                })
                || resolved_callable_name(&invocation.callee.path, callables)
                    .is_some_and(|callee| callable_is_mutative(&callee, callables, visiting))
        }
        Expression::TestCall(call) => {
            resolved_callable_name(&call.invocation.callee.path, callables)
                .is_some_and(|callee| callable_is_mutative(&callee, callables, visiting))
        }
        Expression::Construction(construction) => construction
            .fields
            .iter()
            .any(|field| expression_contains_mutation(&field.value, callables, visiting)),
        Expression::Object(object) => object
            .fields
            .iter()
            .any(|field| expression_contains_mutation(&field.value, callables, visiting)),
        Expression::Binary(binary) => {
            expression_contains_mutation(&binary.left, callables, visiting)
                || expression_contains_mutation(&binary.right, callables, visiting)
        }
        Expression::Unary(unary) => expression_contains_mutation(&unary.value, callables, visiting),
        Expression::Grouped(grouped) => {
            expression_contains_mutation(&grouped.value, callables, visiting)
        }
        Expression::Attempt(attempt) => {
            expression_contains_mutation(&attempt.value, callables, visiting)
        }
        Expression::OutcomeMatch(outcome) => {
            expression_contains_mutation(&outcome.subject, callables, visiting)
                || outcome.arms.iter().any(|arm| match &arm.body {
                    jadpo_syntax::OutcomeMatchArmBody::Value(value) => {
                        expression_contains_mutation(value, callables, visiting)
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Reject(rejection) => {
                        rejection.values.iter().any(|field| {
                            expression_contains_mutation(&field.value, callables, visiting)
                        })
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Propagate(_) => false,
                })
        }
        Expression::Literal(_) | Expression::Name(_) | Expression::Missing(_) => false,
    }
}

fn callable_is_repeatable(
    name: &str,
    callables: &BTreeMap<String, &CallableDeclaration>,
    type_names: &BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> bool {
    if !visiting.insert(name.to_owned()) {
        return false;
    }
    let result = callables.get(name).is_some_and(|callable| {
        callable.policy.is_none()
            && block_is_repeatable(&callable.body, callables, type_names, visiting)
    });
    visiting.remove(name);
    result
}

fn block_is_repeatable(
    block: &Block,
    callables: &BTreeMap<String, &CallableDeclaration>,
    type_names: &BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> bool {
    block.statements.iter().all(|statement| match statement {
        Statement::Binding(statement) => {
            expression_is_repeatable(&statement.value, callables, type_names, visiting)
        }
        Statement::Assignment(statement) => {
            expression_is_repeatable(&statement.value, callables, type_names, visiting)
        }
        Statement::Return(statement) => {
            expression_is_repeatable(&statement.value, callables, type_names, visiting)
        }
        Statement::Reject(statement) => {
            fields_are_repeatable(&statement.values, callables, type_names, visiting)
        }
        Statement::If(statement) => {
            expression_is_repeatable(&statement.condition, callables, type_names, visiting)
                && block_is_repeatable(&statement.then_block, callables, type_names, visiting)
                && statement.else_block.as_ref().map_or(true, |block| {
                    block_is_repeatable(block, callables, type_names, visiting)
                })
        }
        Statement::Match(statement) => {
            expression_is_repeatable(&statement.subject, callables, type_names, visiting)
                && statement
                    .arms
                    .iter()
                    .all(|arm| block_is_repeatable(&arm.body, callables, type_names, visiting))
        }
        Statement::Assert(statement) => {
            expression_is_repeatable(&statement.condition, callables, type_names, visiting)
        }
        Statement::AdvanceClock(_) | Statement::Unsupported(_) => false,
    })
}

fn fields_are_repeatable(
    fields: &[FieldInitialiser],
    callables: &BTreeMap<String, &CallableDeclaration>,
    type_names: &BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> bool {
    fields
        .iter()
        .all(|field| expression_is_repeatable(&field.value, callables, type_names, visiting))
}

fn rejection_is_repeatable(
    rejection: &jadpo_syntax::RejectStatement,
    callables: &BTreeMap<String, &CallableDeclaration>,
    type_names: &BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> bool {
    fields_are_repeatable(&rejection.values, callables, type_names, visiting)
}

fn expression_is_repeatable(
    expression: &Expression,
    callables: &BTreeMap<String, &CallableDeclaration>,
    type_names: &BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> bool {
    match expression {
        Expression::Literal(_) | Expression::Name(_) => true,
        Expression::Invocation(invocation) => {
            let arguments_are_repeatable = invocation.arguments.iter().all(|argument| {
                expression_is_repeatable(argument, callables, type_names, visiting)
            }) && fields_are_repeatable(
                &invocation.named_arguments,
                callables,
                type_names,
                visiting,
            );
            if !arguments_are_repeatable {
                return false;
            }
            if let Some(callee) = resolved_callable_name(&invocation.callee.path, callables) {
                return callable_is_repeatable(&callee, callables, type_names, visiting);
            }
            let authored = invocation
                .callee
                .path
                .iter()
                .map(|part| part.text.as_str())
                .collect::<Vec<_>>()
                .join(".");
            matches!(authored.as_str(), "Instant" | "CalendarDate" | "Duration")
                || type_names.contains(&authored)
                || authored.starts_with("temporal.")
        }
        Expression::TestCall(_) | Expression::Missing(_) => false,
        Expression::Object(object) => {
            fields_are_repeatable(&object.fields, callables, type_names, visiting)
        }
        Expression::Construction(construction) => {
            fields_are_repeatable(&construction.fields, callables, type_names, visiting)
        }
        Expression::Create(create) => {
            fields_are_repeatable(&create.fields, callables, type_names, visiting)
                && create.conflicts.iter().all(|binding| {
                    rejection_is_repeatable(&binding.rejection, callables, type_names, visiting)
                })
        }
        Expression::Query(query) => {
            expression_is_repeatable(&query.value, callables, type_names, visiting)
                && query.pagination.as_ref().map_or(true, |pagination| {
                    expression_is_repeatable(&pagination.limit, callables, type_names, visiting)
                        && expression_is_repeatable(
                            &pagination.offset,
                            callables,
                            type_names,
                            visiting,
                        )
                })
                && query.page.as_ref().map_or(true, |page| {
                    page.predicates.iter().all(|predicate| {
                        expression_is_repeatable(&predicate.value, callables, type_names, visiting)
                    }) && expression_is_repeatable(&page.after, callables, type_names, visiting)
                        && expression_is_repeatable(&page.limit, callables, type_names, visiting)
                })
                && query.includes.iter().all(|include| {
                    expression_is_repeatable(
                        &include.pagination.limit,
                        callables,
                        type_names,
                        visiting,
                    ) && expression_is_repeatable(
                        &include.pagination.offset,
                        callables,
                        type_names,
                        visiting,
                    )
                })
                && query.missing.as_ref().map_or(true, |rejection| {
                    rejection_is_repeatable(rejection, callables, type_names, visiting)
                })
        }
        Expression::Update(update) => {
            expression_is_repeatable(&update.value, callables, type_names, visiting)
                && fields_are_repeatable(&update.changes, callables, type_names, visiting)
                && update.conditional_changes.iter().all(|change| {
                    expression_is_repeatable(&change.change.value, callables, type_names, visiting)
                })
                && update.empty.as_ref().map_or(true, |rejection| {
                    rejection_is_repeatable(rejection, callables, type_names, visiting)
                })
                && rejection_is_repeatable(&update.missing, callables, type_names, visiting)
                && update.conflicts.iter().all(|binding| {
                    rejection_is_repeatable(&binding.rejection, callables, type_names, visiting)
                })
        }
        Expression::Delete(delete) => {
            expression_is_repeatable(&delete.value, callables, type_names, visiting)
                && rejection_is_repeatable(&delete.missing, callables, type_names, visiting)
                && delete.conflicts.iter().all(|binding| {
                    rejection_is_repeatable(&binding.rejection, callables, type_names, visiting)
                })
        }
        Expression::Attempt(attempt) => {
            expression_is_repeatable(&attempt.value, callables, type_names, visiting)
        }
        Expression::OutcomeMatch(outcome) => {
            expression_is_repeatable(&outcome.subject, callables, type_names, visiting)
                && outcome.arms.iter().all(|arm| match &arm.body {
                    jadpo_syntax::OutcomeMatchArmBody::Value(value) => {
                        expression_is_repeatable(value, callables, type_names, visiting)
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Reject(rejection) => {
                        rejection_is_repeatable(rejection, callables, type_names, visiting)
                    }
                    jadpo_syntax::OutcomeMatchArmBody::Propagate(_) => true,
                })
        }
        Expression::Unary(unary) => {
            expression_is_repeatable(&unary.value, callables, type_names, visiting)
        }
        Expression::Binary(binary) => {
            expression_is_repeatable(&binary.left, callables, type_names, visiting)
                && expression_is_repeatable(&binary.right, callables, type_names, visiting)
        }
        Expression::Grouped(grouped) => {
            expression_is_repeatable(&grouped.value, callables, type_names, visiting)
        }
    }
}

fn resolved_callable_name(
    path: &[jadpo_syntax::Name],
    callables: &BTreeMap<String, &CallableDeclaration>,
) -> Option<String> {
    let authored = path
        .iter()
        .map(|part| part.text.as_str())
        .collect::<Vec<_>>()
        .join(".");
    if callables.contains_key(&authored) {
        return Some(authored);
    }
    let operation = path.last()?.text.as_str();
    let mut candidates = callables
        .keys()
        .filter(|candidate| candidate.ends_with(&format!(".{operation}")));
    let candidate = candidates.next()?.clone();
    candidates.next().is_none().then_some(candidate)
}

fn snake_case(value: &str) -> String {
    let mut result = String::new();
    for (index, character) in value.chars().enumerate() {
        if character.is_ascii_uppercase() {
            if index > 0 {
                result.push('_');
            }
            result.push(character.to_ascii_lowercase());
        } else {
            result.push(character);
        }
    }
    result
}

fn enum_is_tagged(declaration: &EnumDeclaration) -> bool {
    declaration
        .variants
        .iter()
        .any(|variant| !variant.fields.is_empty())
}

fn query_page_index_columns(page: &jadpo_syntax::QueryPage) -> Vec<String> {
    let mut columns = Vec::new();
    for predicate in &page.predicates {
        if predicate.operator == jadpo_syntax::QueryPagePredicateOperator::Equal
            && !matches!(
                predicate.value.as_ref(),
                Expression::Literal(literal) if literal.kind == LiteralKind::None
            )
            && !columns.contains(&predicate.field.text)
        {
            columns.push(predicate.field.text.clone());
        }
    }
    for order in &page.order {
        if !columns.contains(&order.field.text) {
            columns.push(order.field.text.clone());
        }
    }
    columns
}

fn query_page_index_predicates(page: &jadpo_syntax::QueryPage) -> Vec<String> {
    query_page_index_predicate_fields(page)
        .iter()
        .map(|field| format!("{} IS NULL", sql_identifier(field)))
        .collect()
}

fn query_page_index_predicate_fields(page: &jadpo_syntax::QueryPage) -> Vec<String> {
    let mut fields = page
        .predicates
        .iter()
        .filter(|predicate| {
            predicate.operator == jadpo_syntax::QueryPagePredicateOperator::Equal
                && matches!(
                    predicate.value.as_ref(),
                    Expression::Literal(literal) if literal.kind == LiteralKind::None
                )
        })
        .map(|predicate| predicate.field.text.clone())
        .collect::<Vec<_>>();
    fields.sort();
    fields.dedup();
    fields
}

fn query_page_index_name(entity: &str, columns: &[String], predicate_fields: &[String]) -> String {
    let predicate_suffix = if predicate_fields.is_empty() {
        String::new()
    } else {
        format!(
            "_where_{}_is_null",
            predicate_fields
                .iter()
                .map(|field| snake_case(field))
                .collect::<Vec<_>>()
                .join("_and_")
        )
    };
    format!(
        "{}_page_{}{}_idx",
        snake_case(entity),
        columns
            .iter()
            .map(|field| snake_case(field))
            .collect::<Vec<_>>()
            .join("_"),
        predicate_suffix
    )
}

fn sql_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn sql_text_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn identity_field_name(record: &RecordDeclaration) -> Option<&str> {
    record
        .dossier
        .as_ref()
        .map(|dossier| dossier.identity.text.as_str())
        .or_else(|| {
            record
                .fields
                .iter()
                .find(|field| has_modifier(field, PersistenceModifier::Identity))
                .map(|field| field.name.text.as_str())
        })
}

fn has_modifier(field: &FieldDeclaration, modifier: PersistenceModifier) -> bool {
    field.persistence.contains(&modifier)
}

fn generated_change_field(entity: &RecordDeclaration) -> Option<&FieldDeclaration> {
    entity
        .fields
        .iter()
        .find(|field| field.generated == Some(jadpo_syntax::GeneratedFieldRole::CreateOrChange))
}

fn owning_relationship_name(field: &FieldDeclaration) -> &str {
    field
        .reference
        .as_ref()
        .and_then(|reference| reference.relationship.as_ref())
        .map_or(field.name.text.as_str(), |name| name.text.as_str())
}

const fn reference_delete_name(action: ReferenceDeleteAction) -> &'static str {
    match action {
        ReferenceDeleteAction::Restrict => "restrict",
        ReferenceDeleteAction::Cascade => "cascade",
        ReferenceDeleteAction::SetNull => "set_null",
    }
}

const fn reference_delete_sql(action: ReferenceDeleteAction) -> &'static str {
    match action {
        ReferenceDeleteAction::Restrict => "RESTRICT",
        ReferenceDeleteAction::Cascade => "CASCADE",
        ReferenceDeleteAction::SetNull => "SET NULL",
    }
}

#[cfg(test)]
mod tests {
    use super::{derive_target, validate_runtime_dependency_contract};
    use crate::{analyze_project, create_project, GeneratedArtifact};
    use std::fs;
    use std::path::Path;

    #[test]
    fn delivery_schedule_hook_components_keep_full_execution_disabled() {
        use super::{SqlDialect, TargetGenerator};
        use jadpo_syntax::{Declaration, SourceFile};
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap();
        let mut sources =
            crate::discover_sources(&root.join("examples/golden-todo-migration")).unwrap();
        let contracts = sources
            .iter_mut()
            .find(|source| source.path.ends_with("values/contracts.jadpo"))
            .unwrap();
        contracts.text = contracts.text.replacen(
            "idempotency_key: Todo.id",
            "idempotency_key: ReminderIntentId",
            1,
        );
        sources.push(SourceFile::new(
            root.join("tests/validation/fixtures/reminder-delivery-v1.jadpo"),
            include_str!("../../../../tests/validation/fixtures/reminder-delivery-v1.jadpo")
                .to_owned(),
        ));
        if std::env::var("JADPO_DELIVERY_HOOK_COMPONENT_VARIANT").as_deref()
            == Ok("completion-representation")
        {
            // Additional checked-source control, not a rewrite of emitted code
            // or the frozen golden fixture: field-role naming independence and
            // the existing durable derived-representation mutation machinery.
            for source in &mut sources {
                source.text = source.text.replace("updated_at", "modified_at");
                if source.path.ends_with("entities/todo.jadpo") {
                    source.text = source.text.replacen(
                        "    query by_id(",
                        "    cache hot {\n        store: redis\n        from: primary\n        strategy: invalidate\n        delivery: durable\n    }\n\n    query by_id(",
                        1,
                    );
                }
            }
        }
        let project = crate::analyze_sources(sources).unwrap();
        let errors = project
            .syntax
            .diagnostics()
            .chain(project.semantics.diagnostics.iter())
            .chain(project.typing.diagnostics.iter())
            .chain(project.failures.diagnostics.iter())
            .chain(project.entity_model.diagnostics.iter())
            .chain(project.policy.diagnostics.iter())
            .filter(|diagnostic| diagnostic.severity == jadpo_diagnostics::Severity::Error)
            .collect::<Vec<_>>();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(project.delivery_model().bindings().len(), 1);
        assert_eq!(
            derive_target(root, &project).unwrap_err().code,
            "JADPO_TARGET_JOB_NOT_IMPLEMENTED"
        );
        // Inspect the actual production persistence renderer on the complete
        // checked source. Do not remove declarations, clear errors, alter hooks,
        // or turn this component sink into a publicly executable job target.
        let mut generator = TargetGenerator {
            project: &project,
            types: Default::default(),
            enums: Default::default(),
            records: Default::default(),
            failures: Default::default(),
            callables: Default::default(),
            fixtures: Default::default(),
            tests: vec![],
            configuration: None,
            application: None,
            locales: None,
            principal: None,
            authentication_strategies: vec![],
            source_revision: crate::checked_source_revision(root, &project),
        };
        for source in &project.syntax.sources {
            for declaration in &source.file.declarations {
                match declaration {
                    Declaration::Type(value) => {
                        generator.types.insert(value.name.text.clone(), value);
                    }
                    Declaration::Enum(value) => {
                        generator.enums.insert(value.name.text.clone(), value);
                    }
                    Declaration::Record(value) => {
                        generator.records.insert(value.name.text.clone(), value);
                    }
                    Declaration::Callable(value) => {
                        generator.callables.insert(value.name.text.clone(), value);
                    }
                    Declaration::Failure(value) => {
                        generator.failures.insert(value.name.text.clone(), value);
                    }
                    Declaration::Fixture(value) => {
                        generator.fixtures.insert(value.name.text.clone(), value);
                    }
                    Declaration::Test(value) => generator.tests.push(value),
                    Declaration::Config(value) => generator.configuration = Some(value),
                    Declaration::Application(value) => generator.application = Some(value),
                    Declaration::Locales(value) => generator.locales = Some(value),
                    Declaration::Principal(value) => generator.principal = Some(value),
                    Declaration::AuthenticationStrategy(value) => {
                        generator.authentication_strategies.push(value)
                    }
                    _ => {}
                }
            }
        }
        let persistence = generator.persistence_target();
        assert_eq!(
            persistence
                .matches("await client.establish_delivery_schedule_revision(\"Todo\"")
                .count(),
            1
        );
        assert_eq!(
            persistence
                .matches("await client.advance_delivery_schedule_revision(\"Todo\"")
                .count(),
            1
        );
        assert!(persistence.contains("if (changed !== null && hasOwn(patchValue, \"due_at\"))"));
        assert!(persistence.contains("async update_required_Todo_by_id_patch_title_and_status_and_due_at_set_reminder_sent_at"));
        assert_eq!(
            derive_target(root, &project).unwrap_err().code,
            "JADPO_TARGET_JOB_NOT_IMPLEMENTED"
        );
        if let Some(sink) = std::env::var_os("JADPO_DELIVERY_HOOK_COMPONENT_OUTPUT") {
            let sink = std::path::PathBuf::from(sink);
            assert!(
                sink.is_absolute() && sink.is_dir(),
                "test-owned component directory required"
            );
            fs::write(sink.join("persistence.ts"), persistence).unwrap();
            // Unmodified actual app/auth renderers for native credential handoff
            // components. This still does not bypass public job derivation.
            fs::write(sink.join("app.ts"), generator.generate()).unwrap();
            fs::write(
                sink.join("authentication.ts"),
                generator.authentication_target(),
            )
            .unwrap();
            fs::write(
                sink.join("first-party-authentication.ts"),
                include_str!("runtime/first_party_authentication.ts"),
            )
            .unwrap();
            fs::write(
                sink.join("jwt-authentication.ts"),
                include_str!("runtime/jwt_authentication.ts"),
            )
            .unwrap();
            fs::write(
                sink.join("service-adapter.ts"),
                include_str!("runtime/service_adapter.ts"),
            )
            .unwrap();
            fs::write(
                sink.join("postgres.sql"),
                generator.schema_sql(SqlDialect::Postgres),
            )
            .unwrap();
            fs::write(
                sink.join("sqlite.sql"),
                generator.schema_sql(SqlDialect::Sqlite),
            )
            .unwrap();
            fs::write(
                sink.join("binding.json"),
                serde_json::to_vec_pretty(&project.delivery_model().bindings()[0].source_facts())
                    .unwrap(),
            )
            .unwrap();
            // Public ordinary-target control for destination-field encoding:
            // no delivery descriptor, private component constructor or job.
            let ordinary = crate::analyze_sources(vec![SourceFile::new(
                sink.join("ordinary.jadpo"),
                r#"
entity Alarm {
    id: Uuid
    due: Instant?
    identity: id
    persistence { store: primary role: authority }
    action patch_alarm(input: AlarmUpdate) fails AlarmMissing, AlarmConflict, AlarmEmpty -> Alarm {
        var changes: AlarmPatch = input.changes
        return attempt update required Alarm {
            where: id == input.id patch: changes
            empty: AlarmEmpty missing: AlarmMissing conflict: AlarmConflict
        }
    }
}
type AlarmPatch = Object { due: Alarm.due optional }
type AlarmUpdate = Object { id: Alarm.id changes: AlarmPatch }
failure AlarmMissing { kind: NotFound code: "alarm_missing" }
failure AlarmConflict { kind: Conflict code: "alarm_conflict" }
failure AlarmEmpty { kind: InvalidValue code: "alarm_empty" }
"#
                .to_owned(),
            )])
            .unwrap();
            let ordinary_errors = ordinary
                .syntax
                .diagnostics()
                .chain(ordinary.semantics.diagnostics.iter())
                .chain(ordinary.typing.diagnostics.iter())
                .chain(ordinary.failures.diagnostics.iter())
                .chain(ordinary.entity_model.diagnostics.iter())
                .chain(ordinary.policy.diagnostics.iter())
                .filter(|diagnostic| diagnostic.severity == jadpo_diagnostics::Severity::Error)
                .collect::<Vec<_>>();
            assert!(ordinary_errors.is_empty(), "{ordinary_errors:?}");
            let artifacts = derive_target(&sink, &ordinary).unwrap();
            fs::write(
                sink.join("ordinary-persistence.ts"),
                &artifacts
                    .iter()
                    .find(|artifact| artifact.relative_path == "target/persistence.ts")
                    .unwrap()
                    .contents,
            )
            .unwrap();
        }
    }

    #[test]
    fn typed_query_lowering_uses_raw_decoding_and_split_boundary_failures() {
        let path = Path::new("route-query.jadpo");
        let project = crate::analyze_sources(vec![jadpo_syntax::SourceFile::new(
            path.to_owned(),
            r#"
type PageSize = Int { min: 1 max: 100 }
type ListTodos = Object { page_size: PageSize default 25 }
route GET /todos { auth: none query: ListTodos output: ListTodos action: { return query } }
"#
            .to_owned(),
        )])
        .expect("query source should analyze");
        let outputs = derive_target(path, &project).expect("typed query should lower");
        let app = &outputs
            .iter()
            .find(|output| output.relative_path == "target/app.ts")
            .unwrap()
            .contents;
        for expected in [
            "parseRawQuery(request.url)",
            "duplicate query parameter",
            "queryInteger(values_page_size[0])",
            "validate_ListTodos(queryValue, \"request.query\")",
            "errorEnvelope(\"invalid_request\"",
            "errorEnvelope(\"invalid_value\"",
            "hasOwn(object, \"page_size\") ?",
            ": 25,",
        ] {
            assert!(app.contains(expected), "{expected}");
        }
    }

    #[test]
    fn route_headers_generate_a_raw_multiplicity_adapter() {
        let source_path =
            repository_root().join("tests/compile/pass/165_route_query_headers.jadpo");
        let project = analyze_project(&source_path).expect("header fixture should analyze");
        let outputs = derive_target(&source_path, &project).expect("header adapter should lower");
        let app = &outputs
            .iter()
            .find(|output| output.relative_path == "target/app.ts")
            .unwrap()
            .contents;
        for expected in [
            "from \"node:http\"",
            "from \"node:stream\"",
            "incoming.rawHeaders",
            "Readable.toWeb(incoming)",
            "duplicate declared header",
            "declaredHeaderValue(request, \"X-Trace\")",
            "createApplicationServer().listen",
        ] {
            assert!(app.contains(expected), "{expected}");
        }
    }

    #[test]
    fn declared_service_exchange_generates_a_fail_closed_http_endpoint() {
        let source_path = repository_root().join("tests/compile/pass/163_service_exchange.jadpo");
        let project = analyze_project(&source_path).expect("exchange fixture should check");
        let outputs =
            derive_target(&source_path, &project).expect("reviewed service exchange should lower");
        let app = &outputs
            .iter()
            .find(|output| output.relative_path == "target/app.ts")
            .unwrap()
            .contents;
        assert!(app.contains("POST\" && url.pathname === \"/auth/exchange\""));
        assert!(app.contains("inventoryAuthenticationCredentials(request).filter"));
        assert!(app.contains("candidate.strategy !== \"api_bearer\""));
        assert!(app.contains("exchangeServiceCredential(\"api_bearer\", candidate.values[0]"));
        assert!(app.contains("new Date(exchanged.expires).toISOString()"));
    }

    #[test]
    fn first_party_authentication_opens_only_complete_supported_routes() {
        let source_path = repository_root().join("examples/first-party-authentication/app.jadpo");
        let source = fs::read_to_string(&source_path).unwrap();
        let checked = |text: &str| {
            crate::analyze_sources(vec![jadpo_syntax::SourceFile::new(
                source_path.clone(),
                text.to_owned(),
            )])
            .unwrap()
        };
        let project = checked(&source);
        assert!(
            project.typing.diagnostics.is_empty(),
            "{:?}",
            project.typing.diagnostics
        );
        assert!(
            project.failures.diagnostics.is_empty(),
            "{:?}",
            project.failures.diagnostics
        );
        let outputs = derive_target(&source_path, &project).unwrap();
        let app = &outputs
            .iter()
            .find(|o| o.relative_path == "target/app.ts")
            .unwrap()
            .contents;
        assert!(
            app.find("firstPartyAuthentication.authenticate(request")
                .unwrap()
                < app.find("await request.json()").unwrap()
        );
        assert!(outputs
            .iter()
            .any(|o| o.relative_path == "target/first-party-authentication.ts"));
        for modified in [
            source.replace("mode: signed", "mode: jwt"),
            source.replace("secret: config.signing_key", ""),
            source.replace("origin: config.browser_origin", ""),
        ] {
            assert_eq!(
                derive_target(&source_path, &checked(&modified))
                    .unwrap_err()
                    .code,
                "JADPO_TARGET_AUTH_NOT_IMPLEMENTED"
            );
        }
        let immediate = source
            .replace(
                "mode: bounded\n            maximum_delay: 5m",
                "mode: immediate",
            )
            .replace("mode: signed", "mode: opaque");
        assert!(derive_target(&source_path, &checked(&immediate)).is_ok());
        let invalid =
            checked(&source.replace("secret: config.signing_key", "secret: \"literal-secret\""));
        assert!(invalid
            .typing
            .diagnostics
            .iter()
            .any(|d| d.code == "TYPE_AUTH_ADAPTER_SETTING"));
    }

    #[test]
    fn first_party_authentication_checks_strength_vocabulary_not_type_name() {
        let project_path = repository_root().join("examples/golden-todo-migration");
        let checked = |enum_name: &str, variants: &str| {
            let sources = crate::discover_sources(&project_path)
                .unwrap()
                .into_iter()
                .map(|source| {
                    jadpo_syntax::SourceFile::new(
                        source.path,
                        source
                            .text
                            .replace("    primary\n    multi_factor", variants)
                            .replace("AuthenticationStrength", enum_name),
                    )
                })
                .collect();
            let project = crate::analyze_sources(sources).unwrap();
            assert!(project.syntax.diagnostics().next().is_none());
            assert!(
                project.typing.diagnostics.is_empty(),
                "{:?}",
                project.typing.diagnostics
            );
            project
        };
        let renamed = checked("LoginAssurance", "    primary\n    multi_factor");
        assert!(derive_target(&project_path, &renamed).is_ok());
        for variants in [
            "    multi_factor",
            "    primary\n    multi_factor\n    trusted",
            "    signed\n    api_key",
        ] {
            let project = checked("AuthenticationStrength", variants);
            assert_eq!(
                derive_target(&project_path, &project).unwrap_err().code,
                "JADPO_TARGET_AUTH_NOT_IMPLEMENTED",
                "{variants}"
            );
        }
    }

    #[test]
    fn first_party_authentication_binds_credentials_to_separate_service_identity() {
        let project_path = repository_root().join("examples/golden-todo-migration");
        let analyzed = analyze_project(&project_path).expect("golden migration should analyze");
        assert!(
            analyzed.typing.diagnostics.is_empty(),
            "{:?}",
            analyzed.typing.diagnostics
        );
        assert!(
            analyzed.failures.diagnostics.is_empty(),
            "{:?}",
            analyzed.failures.diagnostics
        );
        let outputs = derive_target(&project_path, &analyzed)
            .expect("a credential authority may reference a separate service identity");
        assert!(outputs
            .iter()
            .any(|output| output.relative_path == "target/first-party-authentication.ts"));
        let app = &outputs
            .iter()
            .find(|output| output.relative_path == "target/app.ts")
            .expect("application target should exist")
            .contents;
        assert!(app.contains("values: { service_id: id }"));
        assert!(!app.contains("verifier: authorityRow[\"verifier\"]"));
        // Constant authored liveness is emitted before auth/readiness, leaving
        // seven ordinary routes with distinct path bindings.
        assert_eq!(app.matches("const routePath").count(), 7);
        for route_id in 0..7 {
            assert!(app.contains(&format!("const routePath{route_id} =")));
        }
        for replacement in [
            "id: Uuid\n    unsupplied: Text",
            "id: Uuid\n    unsupplied: Int",
            "id: Uuid\n    changed_at: Instant generated { on: create_or_change }",
        ] {
            let sources = crate::discover_sources(&project_path)
                .unwrap()
                .into_iter()
                .map(|source| {
                    let text = if source.text.contains("entity ServiceCredential {") {
                        source.text.replace("id: Uuid", replacement)
                    } else {
                        source.text
                    };
                    jadpo_syntax::SourceFile::new(source.path, text)
                })
                .collect();
            let project = crate::analyze_sources(sources).unwrap();
            assert!(
                project.typing.diagnostics.is_empty(),
                "{:?}",
                project.typing.diagnostics
            );
            assert_eq!(
                derive_target(&project_path, &project).unwrap_err().code,
                "JADPO_TARGET_AUTH_NOT_IMPLEMENTED"
            );
        }
    }

    #[test]
    fn lifecycle_authority_lookup_projects_only_subject_identity_and_lifecycle_state() {
        let project_path = repository_root().join("examples/golden-todo-migration");
        let lifecycle = r#"    lifecycle {
        initial: {
            status: User.status(UserStatus.active)
            disabled_at: none
        }
        visible when status == UserStatus.active
        transition disable {
            from: status == UserStatus.active
            set: {
                status: User.status(UserStatus.disabled)
                disabled_at: clock.now
            }
        }
        purge after config.soft_delete_retention from disabled_at
    }

"#;
        let analyze = |authentication_replacement: Option<(&str, &str)>| {
            let sources = crate::discover_sources(&project_path)
                .unwrap()
                .into_iter()
                .map(|source| {
                    let mut text = source.text;
                    if source
                        .path
                        .to_string_lossy()
                        .ends_with("entities/user.jadpo")
                    {
                        text = text.replace(
                            "    policy {\n        UserRole.self: [read]\n    }",
                            &format!(
                                "{lifecycle}    policy {{\n        UserRole.self: [read]\n    }}"
                            ),
                        );
                    }
                    if source
                        .path
                        .to_string_lossy()
                        .ends_with("authentication.jadpo")
                    {
                        if let Some((before, after)) = authentication_replacement {
                            text = text.replace(before, after);
                        }
                    }
                    jadpo_syntax::SourceFile::new(source.path, text)
                })
                .collect();
            crate::analyze_sources(sources).expect("lifecycle authentication should analyze")
        };

        let analyzed = analyze(None);
        assert!(
            analyzed.typing.diagnostics.is_empty(),
            "{:?}",
            analyzed.typing.diagnostics
        );
        let outputs = derive_target(&project_path, &analyzed).expect("lifecycle auth should lower");
        let persistence = &outputs
            .iter()
            .find(|output| output.relative_path == "target/persistence.ts")
            .expect("persistence target should exist")
            .contents;
        let authority_line = persistence
            .lines()
            .find(|line| line.contains("browser_session:user"))
            .expect("user authority lookup should exist");
        for field in ["id", "authentication_subject", "status", "disabled_at"] {
            assert!(authority_line.contains(field), "{authority_line}");
        }
        assert!(!authority_line.contains("email"), "{authority_line}");
        let app = &outputs
            .iter()
            .find(|output| output.relative_path == "target/app.ts")
            .expect("application target should exist")
            .contents;
        assert!(app.contains("const status = validateField_4_User_status("));
        assert!(app.contains("const disabled_at = validateField_4_User_disabled_at("));
        assert!(app.contains("bindLifecycleRetention(configuration)"));
        assert!(!app.contains("const email = validateField_4_User_email("));

        let private_active = analyze(Some((
            "active: status == UserStatus.active",
            "active: email == email",
        )));
        assert!(private_active
            .typing
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TYPE_AUTH_LIFECYCLE_ACTIVE_FIELD"));
        let private_mapping = analyze(Some((
            "authentication_subject -> Principal.user.subject",
            "email -> Principal.user.subject",
        )));
        assert!(private_mapping
            .typing
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TYPE_AUTH_LIFECYCLE_MAPPING_SCOPE"));
    }

    fn repository_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("compiler crate should be inside the repository")
    }

    #[test]
    fn creates_compiler_owned_uuid_identity_values() {
        let fixture = repository_root().join("tests/compile/pass/137_generated_identity.jadpo");
        let analyzed =
            analyze_project(&fixture).expect("generated identity fixture should analyze");
        assert!(
            analyzed.typing.diagnostics.is_empty(),
            "{:?}",
            analyzed.typing.diagnostics
        );
        let targets = derive_target(&fixture, &analyzed).expect("identity target should generate");
        let app = &targets
            .iter()
            .find(|target| target.relative_path == "target/app.ts")
            .expect("app target should exist")
            .contents;
        assert!(app.contains("id: crypto.randomUUID()"));
        let entities = &targets
            .iter()
            .find(|target| target.relative_path == "persistence/entities.json")
            .expect("persistence contract should exist")
            .contents;
        assert!(entities.contains("\"generated\":\"identity\""));
    }

    #[test]
    fn applies_entity_lifecycle_initial_values_to_created_rows() {
        let fixture = repository_root()
            .join("tests/compile/pass/lifecycle/170_lifecycle_initial_values.jadpo");
        let analyzed = analyze_project(&fixture).expect("lifecycle fixture should analyze");
        assert!(
            analyzed.typing.diagnostics.is_empty(),
            "{:?}",
            analyzed.typing.diagnostics
        );
        let targets = derive_target(&fixture, &analyzed).expect("lifecycle target should generate");
        let app = &targets
            .iter()
            .find(|target| target.relative_path == "target/app.ts")
            .expect("app target should exist")
            .contents;
        assert!(app.contains("status: validateField_4_User_status(\"active\", \"User.status\")"));
        assert!(app.contains("status: status, deleted_at: null"));
        assert!(app.contains("id: crypto.randomUUID()"));
        assert!(app.contains("transition_required_User_by_id_disable"));
        assert!(app.contains("runLifecycleMaintenance"));
        assert!(app.contains("bindLifecycleRetention(configuration)"));
        assert!(app.contains("Retention must be positive: config.soft_delete_retention"));
        assert!(app.contains("retention.purge.batch"));
        let persistence = &targets
            .iter()
            .find(|target| {
                target
                    .contents
                    .contains("async transition_required_User_by_id_disable")
            })
            .expect("persistence transition method should be generated")
            .contents;
        let transition_method = persistence
            .find("async transition_required_User_by_id_disable")
            .unwrap();
        let transition_end = persistence[transition_method..]
            .find("\n  },")
            .map(|offset| transition_method + offset)
            .expect("transition method should close");
        let transition_source = &persistence[transition_method..transition_end];
        assert!(transition_source.contains("status"), "{transition_source}");
        assert!(
            transition_source.contains("disabled_at"),
            "{transition_source}"
        );
        assert!(
            transition_source.contains("updated_at"),
            "{transition_source}"
        );
        assert!(transition_source.contains("$4"), "{transition_source}");
        assert!(transition_source.contains("?4"), "{transition_source}");
        assert!(
            transition_source.contains("'active'"),
            "{transition_source}"
        );
        assert!(!persistence.contains("async delete_required_User_by_"));
        assert!(!persistence.contains("async update_required_User_by_id_set_status"));
        assert!(persistence.contains("async update_required_User_by_id_set_name"));
        assert!(persistence.contains("caller cannot set lifecycle-owned initial state"));
        assert!(persistence.contains("WHERE \\\"id\\\" = $2 AND (\\\"status\\\" = 'active')"));
        assert!(transition_source.contains("operationTime"));
        assert!(persistence.contains("async retention_purge_Todo"));
        assert!(persistence.contains("export function configureLifecycleRetention"));
        assert!(!persistence.contains("Bun.env[\"JADPO_LIFECYCLE_RETENTION\"]"));
        assert!(persistence.contains("lifecycleRetentionMilliseconds"));
        assert!(persistence.contains("Math.min(attemptMilliseconds, Date.now())"));
        assert!(persistence.contains("configuredPersistenceDuration"));
        assert!(persistence
            .contains("ORDER BY \\\"deleted_at\\\" ASC, \\\"id\\\" ASC LIMIT 500 FOR UPDATE"));
        assert!(persistence.contains("async update_required_Todo_by_id_set_status"));
        assert!(persistence.contains("\\\"deleted_at\\\" IS NULL"));
        assert!(persistence.contains("\\\"deleted_at\\\" IS NOT NULL"));
        assert!(!persistence.contains("\\\"status\\\" = 'deleted'"));
        let entities = &targets
            .iter()
            .find(|target| target.relative_path == "persistence/entities.json")
            .expect("persistence contract should exist")
            .contents;
        assert!(entities.contains("\\\"status\\\" = 'active'"), "{entities}");
        let maintenance = &targets
            .iter()
            .find(|target| target.relative_path == "audit/lifecycle-maintenance.json")
            .expect("maintenance authority artifact should exist")
            .contents;
        assert!(maintenance.contains("retention_purge(Todo)"));
        assert!(!maintenance.contains("retention_purge(User)"));
        assert!(maintenance.contains("\"source_callable\": false"));
        assert!(maintenance.contains("\"batch_limit\": 500"));
    }

    #[test]
    fn lifecycle_audit_separates_policy_lifecycle_generated_fields_and_sql_effect() {
        let fixture = repository_root()
            .join("tests/compile/pass/lifecycle/174_lifecycle_delete_effect.jadpo");
        let analyzed = analyze_project(&fixture).expect("lifecycle effect fixture should analyze");
        assert!(
            analyzed.typing.diagnostics.is_empty(),
            "{:?}",
            analyzed.typing.diagnostics
        );
        let targets =
            derive_target(&fixture, &analyzed).expect("lifecycle effect target should generate");
        let manifest = &targets
            .iter()
            .find(|target| target.relative_path == "audit/lifecycles.json")
            .expect("lifecycle audit artifact should exist")
            .contents;
        let manifest: serde_json::Value =
            serde_json::from_str(manifest).expect("lifecycle audit artifact should be JSON");
        let entity = &manifest["entities"][0];
        let transition = &entity["transitions"][0];
        assert_eq!(transition["name"], "delete");
        assert_eq!(transition["logical_policy_effect"], "delete");
        assert_eq!(transition["sql_verb"], "UPDATE");
        assert_eq!(transition["guarded_sql"][0], "identity");
        assert_eq!(transition["guarded_sql"][1], "lifecycle_visibility");
        assert_eq!(transition["guarded_sql"][2], "transition_from");
        assert!(entity["lifecycle_nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| {
                node["kind"] == "lifecycle_transition"
                    && node["name"] == "Todo.lifecycle.transition.delete"
                    && node["start"].as_u64().unwrap_or_default()
                        < node["end"].as_u64().unwrap_or_default()
            }));
        assert!(entity["policy_nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|node| { node["kind"] == "policy" && node["name"] == "Todo.policy" }));
        assert!(entity["generated_fields"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| { field["name"] == "updated_at" && field["node_id"].is_number() }));
        let persistence = targets
            .iter()
            .find(|target| target.relative_path == "target/persistence.ts")
            .expect("persistence target should exist")
            .contents
            .as_str();
        assert!(persistence.contains("policyPostgresMutationRows(tx, \"Todo\", \"delete\""));
        assert!(persistence.contains("UPDATE \\\"todo\\\" SET"));
    }

    #[test]
    fn generates_the_seed_runtime_contract() {
        let seed = repository_root().join("examples/jadpo-seed");
        let analyzed = analyze_project(&seed).expect("seed should analyze");
        let targets = derive_target(&seed, &analyzed).expect("seed target should generate");

        assert_eq!(targets.len(), 5);
        assert_eq!(targets[0].relative_path, "target/app.ts");
        assert!(targets[0].contents.contains("function validate_Email"));
        assert!(targets[0]
            .contents
            .contains("parsed.hostname === \"localhost\" || parsed.hostname.includes(\".\")"));
        assert!(targets[0]
            .contents
            .contains("return halves.length === 2 ? groups < 8 : groups === 8"));
        assert!(targets[0]
            .contents
            .contains("request.method === \"POST\" && routePath"));
        assert!(targets[0]
            .contents
            .contains("matchRoutePath(\"/registrations\", url.pathname)"));
        assert!(targets[0].contents.contains("internalContext"));
        assert!(targets[0].contents.contains(
            "throw new DomainFailure(\"InviteCodeRejected\", {  }, { invite_code: input.invite_code });"
        ));
        assert!(!targets[0]
            .contents
            .contains("invite_code: error.internalContext"));
        assert!(targets[0].contents.contains("RUNTIME_UNHANDLED_FAULT"));
        assert!(targets[0].contents.contains("RUNTIME_OUTCOME_UNKNOWN"));
        assert!(targets[0].contents.contains("RUNTIME_STARTUP_FAILED"));
        assert!(targets[0].contents.contains("JADPO_DEBUG_TARGET_STACKS"));
        assert!(!targets[0]
            .contents
            .contains("unhandled generated-runtime fault"));
        assert_eq!(
            targets,
            derive_target(&seed, &analyzed).expect("second target should generate")
        );
    }

    #[test]
    fn nullable_input_default_materializes_none_when_request_omits_field() {
        let source_path = Path::new("input-default.jadpo");
        let source = r#"input CreateTodo { due_at: Instant? default none }
output TodoView { due_at: Instant? }
route POST /todos {
    auth: none
    input: CreateTodo
    output: TodoView
    action: { return TodoView { due_at: input.due_at } }
}
"#;
        let project = crate::analyze_sources(vec![jadpo_syntax::SourceFile::new(
            source_path.to_path_buf(),
            source.to_owned(),
        )])
        .expect("input default project should analyze");
        assert!(
            project.typing.diagnostics.is_empty(),
            "{:?}",
            project.typing.diagnostics
        );
        let outputs = derive_target(source_path, &project).expect("target should generate");
        let app = &outputs
            .iter()
            .find(|output| output.relative_path == "target/app.ts")
            .expect("application target")
            .contents;

        assert!(app.contains("due_at: hasOwn(object, \"due_at\") ?"));
        assert!(app.contains(": null,"));
        assert!(app.contains("type CreateTodo = {\n  due_at: Instant | null;"));

        let invalid = source.replace("Instant? default none", "Instant default none");
        let invalid_project = crate::analyze_sources(vec![jadpo_syntax::SourceFile::new(
            source_path.to_path_buf(),
            invalid,
        )])
        .expect("invalid default source should still be analyzed");
        assert!(invalid_project
            .typing
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "TYPE_MISMATCH"));
    }

    #[test]
    fn generates_the_public_health_scaffold() {
        let root =
            std::env::temp_dir().join(format!("jadpo-target-scaffold-{}", std::process::id()));
        let scaffold = root.join("example");
        create_project(&scaffold).expect("scaffold should be created");
        let analyzed = analyze_project(&scaffold).expect("scaffold should analyze");
        let targets = derive_target(&scaffold, &analyzed).expect("target should generate");

        assert!(targets[0]
            .contents
            .contains("request.method === \"GET\" && routePath"));
        assert!(targets[0]
            .contents
            .contains("matchRoutePath(\"/health\", url.pathname)"));
        assert!(targets[0]
            .contents
            .contains("url.pathname === \"/health/live\""));
        assert!(targets[0]
            .contents
            .contains("url.pathname === \"/health/ready\""));
        assert_eq!(
            targets[0]
                .contents
                .matches("matchRoutePath(\"/health\", url.pathname)")
                .count(),
            1,
            "an authored health route should replace the compiler default"
        );
        fs::remove_dir_all(root).expect("temporary scaffold should be removable");
    }

    #[test]
    fn generates_dependency_free_startup_configuration_loading() {
        let root =
            std::env::temp_dir().join(format!("jadpo-target-configuration-{}", std::process::id()));
        fs::create_dir_all(&root).expect("configuration fixture directory");
        fs::write(
            root.join("app.jadpo"),
            r#"type ApiKey = Text { min_length: 3 }
type Timeout = Duration {}
config ApplicationConfiguration {
    api_key: ApiKey { binding: "API_KEY" secret: true }
    timeout: Timeout { binding: "TIMEOUT" default: 5s }
}
function configured_timeout() -> Timeout { return config.timeout }
"#,
        )
        .expect("configuration fixture source");
        let analyzed = analyze_project(&root).expect("configuration should analyze");
        assert!(
            analyzed.semantics.diagnostics.is_empty(),
            "{:#?}",
            analyzed.semantics.diagnostics
        );
        assert!(
            analyzed.typing.diagnostics.is_empty(),
            "{:#?}",
            analyzed.typing.diagnostics
        );
        let targets = derive_target(&root, &analyzed).expect("configuration target");
        let application = &targets[0].contents;
        assert!(application.contains("function loadConfiguration"));
        assert!(application.contains("requireConfigurationValue(environment, \"API_KEY\""));
        assert!(application.contains("configuration.timeout"));
        assert!(application.contains("configuration = loadConfiguration(Bun.env)"));
        assert!(application.contains("value.startsWith(\"-P\")"));
        assert!(application.contains("return durationFromMilliseconds(milliseconds)"));
        assert!(!application.contains("dotenv"));
        fs::remove_dir_all(root).expect("configuration fixture cleanup");
    }

    #[test]
    fn protected_route_generation_names_and_locates_the_route() {
        let root = std::env::temp_dir().join(format!(
            "jadpo-target-protected-route-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("fixture directory should be created");
        let source_path = root.join("app.jadpo");
        fs::write(
            &source_path,
            "output Health { ready: Bool }\nroute GET /health { output: Health action: { return Health { ready: true } } }\n",
        )
        .expect("fixture should be written");
        let analyzed = analyze_project(&root).expect("protected route should analyze");

        let diagnostic =
            derive_target(&root, &analyzed).expect_err("protected route must block generation");

        assert_eq!(diagnostic.code, "JADPO_TARGET_AUTH_NOT_IMPLEMENTED");
        assert_eq!(
            diagnostic.message,
            "Protected route `GET /health` cannot be generated yet"
        );
        assert_eq!(
            diagnostic.context,
            vec![("route".to_owned(), "GET /health".to_owned())]
        );
        let primary = diagnostic
            .primary
            .expect("route should have a source location");
        assert_eq!(primary.source, source_path.to_string_lossy());
        assert_eq!(
            &fs::read_to_string(&source_path).expect("fixture should remain")
                [primary.start..primary.end],
            "route GET /health"
        );
        fs::remove_dir_all(root).expect("fixture should be removable");
    }

    #[test]
    fn rejects_an_unimplemented_entity_authority_store() {
        let root =
            std::env::temp_dir().join(format!("jadpo-target-entity-store-{}", std::process::id()));
        fs::create_dir_all(&root).expect("fixture directory should be created");
        fs::write(
            root.join("app.jadpo"),
            "entity Customer {\n    id: Uuid\n    identity: id\n    persistence {\n        store: secondary\n        role: authority\n    }\n}\n",
        )
        .expect("fixture should be written");
        let analyzed = analyze_project(&root).expect("store fixture should analyze");

        let diagnostic = derive_target(&root, &analyzed)
            .expect_err("an unavailable physical store must block generation");

        assert_eq!(diagnostic.code, "JADPO_TARGET_STORE_NOT_IMPLEMENTED");
        assert_eq!(
            diagnostic.context,
            vec![("name".to_owned(), "secondary".to_owned())]
        );
        fs::remove_dir_all(root).expect("temporary fixture should be removable");
    }

    #[test]
    fn rejects_unimplemented_durable_workflow_execution() {
        let root = std::env::temp_dir().join(format!(
            "jadpo-target-durable-workflow-{}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("fixture directory should be created");
        fs::write(
            root.join("app.jadpo"),
            "action orchestrate() consistency: durable_workflow -> Bool { return true }\n",
        )
        .expect("fixture should be written");
        let analyzed = analyze_project(&root).expect("workflow fixture should analyze");

        let diagnostic = derive_target(&root, &analyzed)
            .expect_err("an unimplemented durable workflow must block generation");

        assert_eq!(
            diagnostic.code,
            "JADPO_TARGET_DURABLE_WORKFLOW_NOT_IMPLEMENTED"
        );
        assert_eq!(
            diagnostic.context,
            vec![("callable".to_owned(), "orchestrate".to_owned())]
        );
        fs::remove_dir_all(root).expect("temporary fixture should be removable");
    }

    #[test]
    fn generates_mutable_local_reassignment_without_reference_syntax() {
        let fixture =
            repository_root().join("tests/compile/pass/51_mutable_local_reassignment.jadpo");
        let analyzed = analyze_project(&fixture).expect("fixture should analyze");
        assert!(analyzed.syntax.diagnostics().next().is_none());
        assert!(analyzed.semantics.diagnostics.is_empty());
        assert!(analyzed.typing.diagnostics.is_empty());
        assert!(analyzed.failures.diagnostics.is_empty());
        let targets = derive_target(&fixture, &analyzed).expect("target should generate");

        assert!(targets[0]
            .contents
            .contains("let selected = initial;\n  selected = replacement;\n  return selected;"));
    }

    #[test]
    fn keeps_pure_call_chains_synchronous_in_the_generated_target() {
        let fixture =
            repository_root().join("tests/compile/pass/110_callable_execution_matrix.jadpo");
        let analyzed = analyze_project(&fixture).expect("call matrix fixture should analyze");
        assert!(analyzed.syntax.diagnostics().next().is_none());
        assert!(analyzed.semantics.diagnostics.is_empty());
        assert!(analyzed.typing.diagnostics.is_empty());
        assert!(analyzed.failures.diagnostics.is_empty());
        let targets = derive_target(&fixture, &analyzed).expect("target should generate");
        let application = &targets[0].contents;

        assert!(application.contains("function normalize(input: RequestText, __operation: OperationContext = captureOperation()): ResponseText {"));
        assert!(application.contains(
            "function normalize_twice(input: RequestText, __operation: OperationContext = captureOperation()): ResponseText {\n  return normalize(input, __operation);"
        ));
        assert!(application.contains(
            "function prepare(input: RequestText, __operation: OperationContext = captureOperation()): ResponseText {\n  return normalize_twice(input, __operation);"
        ));
        assert!(application.contains(
            "function execute(input: RequestText, __operation: OperationContext = captureOperation()): ResponseText {\n  return prepare(input, __operation);"
        ));
        assert!(!application.contains("async function normalize("));
        assert!(!application.contains("async function prepare("));
        assert!(!application.contains("await prepare(input)"));
    }

    #[test]
    fn derives_internal_suspension_through_the_action_call_graph() {
        let fixture =
            repository_root().join("tests/compile/pass/111_transitive_internal_suspension.jadpo");
        let analyzed = analyze_project(&fixture).expect("suspension fixture should analyze");
        assert!(analyzed.syntax.diagnostics().next().is_none());
        assert!(analyzed.semantics.diagnostics.is_empty());
        assert!(analyzed.typing.diagnostics.is_empty());
        assert!(analyzed.failures.diagnostics.is_empty());
        let suspension = analyzed
            .failures
            .callables
            .iter()
            .map(|callable| (callable.callable.as_str(), callable.may_suspend))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(suspension.get("summarize"), Some(&false));
        assert_eq!(suspension.get("load_customer"), Some(&true));
        assert_eq!(suspension.get("load_summary"), Some(&true));

        let targets = derive_target(&fixture, &analyzed).expect("target should generate");
        let application = &targets[0].contents;
        assert!(application.contains("function summarize(customer: Customer, __operation: OperationContext = captureOperation()): Customer {"));
        assert!(application.contains(
            "async function load_customer(id: string, __operation: OperationContext = captureOperation(), __persistence: typeof rootPersistence = rootPersistence): Promise<Customer> {"
        ));
        assert!(application.contains(
            "async function load_summary(id: string, __operation: OperationContext = captureOperation(), __persistence: typeof rootPersistence = rootPersistence): Promise<Customer> {"
        ));
        assert!(application.contains("await load_customer(id, __operation, persistence)"));
        assert!(application.contains("return summarize(customer, __operation);"));
    }

    #[test]
    fn lowers_exhaustive_outcome_recovery_mapping_and_propagation() {
        let fixture = repository_root()
            .join("tests/compile/pass/113_outcome_match_recovery_mapping_propagation.jadpo");
        let analyzed = analyze_project(&fixture).expect("outcome fixture should analyze");
        let targets = derive_target(&fixture, &analyzed).expect("outcome target should generate");
        let app = &targets[0].contents;

        assert!(app.contains("switch (error.failureName)"));
        assert!(app.contains("case \"LookupMissing\""));
        assert!(app.contains("throw new DomainFailure(\"LookupRejected\", {  }, {  });"));
        assert!(app.contains("function recover_lookup"));
        assert!(app.contains("function map_lookup"));
        assert!(app.contains("function propagate_lookup"));
        assert!(!app.contains("async function recover_lookup"));
        assert!(!app.contains("async function map_lookup"));
        assert!(!app.contains("async function propagate_lookup"));
    }

    #[test]
    fn generates_enum_validation_and_exhaustive_match_dispatch() {
        let fixture =
            repository_root().join("tests/compile/pass/53_plain_enum_exhaustive_match.jadpo");
        let analyzed = analyze_project(&fixture).expect("enum fixture should analyze");
        assert!(analyzed.syntax.diagnostics().next().is_none());
        assert!(analyzed.semantics.diagnostics.is_empty());
        assert!(analyzed.typing.diagnostics.is_empty());
        let targets = derive_target(&fixture, &analyzed).expect("enum target should generate");
        let application = &targets[0].contents;

        assert!(application.contains("type DeliveryState = \"pending\" | \"sent\" | \"failed\";"));
        assert!(application.contains("function validate_DeliveryState"));
        assert!(application.contains("return \"sent\";"));
        assert!(application.contains("switch (input.state)"));
        assert!(application.contains("case \"pending\":"));
        assert!(application.contains("case \"sent\":"));
        assert!(application.contains("case \"failed\":"));
    }

    #[test]
    fn generates_tagged_sum_validation_construction_and_narrowing() {
        let fixture = repository_root().join("tests/compile/pass/56_tagged_sum_match.jadpo");
        let analyzed = analyze_project(&fixture).expect("tagged-sum fixture should analyze");
        assert!(analyzed.syntax.diagnostics().next().is_none());
        assert!(analyzed.semantics.diagnostics.is_empty());
        assert!(analyzed.typing.diagnostics.is_empty());
        let targets = derive_target(&fixture, &analyzed).expect("tagged target should generate");
        let application = &targets[0].contents;

        assert!(application.contains("readonly tag: \"paid\""));
        assert!(application.contains("const tag = validateText(object[\"tag\"]"));
        assert!(application.contains("switch (matchSubject"));
        assert!(application.contains("const { receipt_id, paid_at } = matchSubject"));
        assert!(application
            .contains("return { tag: \"paid\", receipt_id: receipt_id, paid_at: paid_at };"));
    }

    #[test]
    fn generates_operator_precedence_with_explicit_parentheses() {
        let fixture =
            repository_root().join("tests/compile/pass/57_operators_and_precedence.jadpo");
        let analyzed = analyze_project(&fixture).expect("operator fixture should analyze");
        assert!(analyzed.typing.diagnostics.is_empty());
        let targets = derive_target(&fixture, &analyzed).expect("operator target should generate");
        let application = &targets[0].contents;

        assert!(application.contains("(left + (right * multiplier))"));
        assert!(application.contains("((!disabled) &&"));
        assert!(application.contains("((left < right) || (left === right))"));
        assert!(application.contains("(left / right)"));
        assert!(application.contains("(prefix + suffix)"));
    }

    #[test]
    fn generates_a_structured_authored_test_runner() {
        let fixture = repository_root().join("tests/compile/pass/58_authored_tests.jadpo");
        let analyzed = analyze_project(&fixture).expect("test fixture should analyze");
        let targets = derive_target(&fixture, &analyzed).expect("test target should generate");
        let application = &targets[0].contents;
        let runner = targets
            .iter()
            .find(|target| target.relative_path == "target/tests.ts")
            .expect("test entrypoint should be generated");

        assert!(application.contains("export async function runTests()"));
        assert!(application.contains("assertion failed at source bytes"));
        assert!(application.contains("captureOperation(__testClockNow, null, () => __testClockNow, true, null, __testServiceFakes)"));
        assert!(application.contains("surface: \"pure\" as const"));
        assert!(application.contains("schema_version: 2, kind: \"test_report\""));
        assert!(application.contains("authored_business_cases: results.length"));
        assert!(runner
            .contents
            .contains("import { runTests } from \"./app.ts\""));
        assert!(!runner.contents.contains("console.error(error)"));

        let persistent_fixture =
            repository_root().join("tests/compile/pass/129_temporal_lifecycle.jadpo");
        let persistent = analyze_project(&persistent_fixture)
            .expect("persistent temporal test fixture should analyze");
        let persistent_targets = derive_target(&persistent_fixture, &persistent)
            .expect("persistent test target should generate");
        let persistent_application = &persistent_targets[0].contents;
        let persistence = persistent_targets
            .iter()
            .find(|target| target.relative_path == "target/persistence.ts")
            .expect("persistent test target should include persistence");
        assert!(persistent_application.contains("createIsolatedTestPersistence"));
        assert!(persistent_application.contains("__testPersistence.close()"));
        assert!(persistence
            .contents
            .contains("export function createIsolatedTestPersistence()"));
        assert!(persistence.contents.contains("new Database(\":memory:\""));

        let configuration_fixture =
            repository_root().join("tests/compile/pass/131_test_fixture_configuration.jadpo");
        let configured = analyze_project(&configuration_fixture)
            .expect("typed configuration fixture should analyze");
        let configured_targets = derive_target(&configuration_fixture, &configured)
            .expect("typed configuration fixture target should generate");
        let configured_application = &configured_targets[0].contents;
        assert!(configured_application.contains("api_key: validate_ApiKey(\"test-only-api-key\""));
        assert!(!configured_application.contains("secret(\"test-only-api-key\")"));
    }

    #[test]
    fn generates_parameterised_persistence_contracts() {
        let seed = repository_root().join("examples/persistence-seed");
        let analyzed = analyze_project(&seed).expect("persistence seed should analyze");
        let targets = derive_target(&seed, &analyzed).expect("persistence target should generate");

        let postgres = targets
            .iter()
            .find(|target| target.relative_path == "persistence/entities.json")
            .expect("persistence manifest should exist");
        let runtime = targets
            .iter()
            .find(|target| target.relative_path == "target/persistence.ts")
            .expect("SQLite target should exist");
        let postgres_schema = targets
            .iter()
            .find(|target| target.relative_path == "sql/postgres/schema.sql")
            .expect("PostgreSQL schema should exist");
        assert!(postgres.contents.contains("VALUES ($1, $2)"));
        assert!(postgres.contents.contains("VALUES (?1, ?2)"));
        assert!(postgres
            .contents
            .contains("\"transaction_policy\":\"mutative_action\""));
        assert!(postgres
            .contents
            .contains("\"nested_transaction_policy\":\"reuse\""));
        assert!(postgres.contents.contains("\"identity\":\"id\""));
        assert!(postgres
            .contents
            .contains("\"unique_constraints\":[\"handle\"]"));
        assert!(postgres.contents.contains(
            "\"compound_unique_constraints\":[{\"name\":\"tenant_owner\",\"fields\":[\"tenant\",\"owner_email\"]}]"
        ));
        assert!(postgres.contents.contains("\"indexes\":[\"owner_email\"]"));
        assert!(postgres.contents.contains(
            "\"reference\":{\"entity\":\"User\",\"field\":\"id\",\"relationship\":\"owner\",\"required\":true,\"on_delete\":\"cascade\"}"
        ));
        assert!(postgres.contents.contains(
            "\"inverses\":[{\"name\":\"todos\",\"cardinality\":\"many\",\"entity\":\"Todo\",\"via\":\"Todo.owner_id\"},{\"name\":\"notes\",\"cardinality\":\"many\",\"entity\":\"Note\",\"via\":\"Note.owner_id\"},{\"name\":\"profile\",\"cardinality\":\"optional\",\"entity\":\"UserProfile\",\"via\":\"UserProfile.user_id\"}]"
        ));
        assert!(postgres
            .contents
            .contains("\"strategy\":\"bounded_batch\",\"query_count\":2"));
        assert!(postgres.contents.contains(
            "\"child\":\"PatchItem\",\"relationship\":\"reviewer_id\",\"parent\":\"User\",\"strategy\":\"bounded_parent_lookup\",\"query_count\":2"
        ));
        assert!(postgres.contents.contains(
            "\"parent\":\"User\",\"relationship\":\"profile\",\"child\":\"UserProfile\",\"strategy\":\"bounded_optional_inverse\",\"query_count\":2"
        ));
        assert!(postgres.contents.contains(
            "\"root\":\"Todo\",\"path\":[\"owner\",\"profile\"],\"leaf\":\"UserProfile\",\"strategy\":\"bounded_nested_lookup\",\"query_count\":3,\"maximum_depth\":2"
        ));
        assert!(postgres
            .contents
            .contains("\"parent_cardinality\":\"optional\""));
        assert!(postgres
            .contents
            .contains("\"strategy\":\"parent_page_join\",\"query_count\":1"));
        assert!(postgres
            .contents
            .contains("\"parent_pagination_before_join\":true"));
        assert!(postgres
            .contents
            .contains("\"strategy\":\"parent_then_bounded_children\",\"query_count\":3"));
        assert!(postgres
            .contents
            .contains("\"strategy\":\"independent_parent_page_joins\",\"query_count\":2"));
        assert!(postgres
            .contents
            .contains("\"cartesian_product_avoided\":true"));
        assert!(postgres.contents.contains("LIMIT $2 OFFSET $3"));
        assert!(postgres.contents.contains("LIMIT ?2 OFFSET ?3"));
        assert!(postgres.contents.contains(
            "SELECT \\\"id\\\", \\\"email\\\" FROM \\\"customer\\\" WHERE \\\"id\\\" = $1 LIMIT 2"
        ));
        assert!(postgres.contents.contains(
            "SELECT \\\"id\\\", \\\"email\\\" FROM \\\"customer\\\" WHERE \\\"id\\\" = ?1 LIMIT 2"
        ));
        assert!(postgres.contents.contains(
            "UPDATE \\\"customer\\\" SET \\\"email\\\" = $1 WHERE \\\"id\\\" = $2 RETURNING"
        ));
        assert!(postgres.contents.contains(
            "UPDATE \\\"account\\\" SET \\\"handle\\\" = $1, \\\"owner_email\\\" = $2 WHERE \\\"id\\\" = $3 RETURNING"
        ));
        assert!(postgres.contents.contains(
            "UPDATE \\\"patch_item\\\" SET \\\"title\\\" = CASE WHEN $1 THEN $2 ELSE \\\"title\\\" END, \\\"note\\\" = CASE WHEN $3 THEN $4 ELSE \\\"note\\\" END, \\\"marker\\\" = CASE WHEN $1 THEN $5 ELSE \\\"marker\\\" END WHERE \\\"id\\\" = $6 RETURNING"
        ));
        assert!(postgres.contents.contains("\"omission\":\"supplied_flag\""));
        assert!(postgres
            .contents
            .contains("DELETE FROM \\\"customer\\\" WHERE \\\"id\\\" = ?1 RETURNING"));
        assert!(runtime
            .contents
            .contains("${persistenceValue(createValue[\"id\"], \"Uuid\", true)}"));
        assert!(runtime
            .contents
            .contains("query_optional_Customer_by_id(value: unknown)"));
        assert!(runtime
            .contents
            .contains("query_required_Customer_by_id(value: unknown)"));
        assert!(runtime
            .contents
            .contains("class PersistenceFault extends Error"));
        assert!(runtime
            .contents
            .contains("class OutcomeUnknownFault extends Error"));
        let application = targets
            .iter()
            .find(|target| target.relative_path == "target/app.ts")
            .expect("HTTP application target should exist");
        assert!(application.contents.contains("error.kind === \"unknown\""));
        assert!(application.contents.contains("outcome_unknown"));
        assert!(runtime.contents.contains(
            "new SQL({ url: Bun.env.DATABASE_URL!, prepare: false, connectionTimeout: 2 })"
        ));
        assert!(runtime.contents.contains("details.errno"));
        assert!(runtime
            .contents
            .contains("policyReadRows(\"query.Customer.id\", \"Customer\""));
        assert!(runtime
            .contents
            .contains("update_required_Customer_by_id_set_email"));
        assert!(runtime
            .contents
            .contains("update_required_Account_by_id_set_handle_and_owner_email"));
        assert!(runtime
            .contents
            .contains("update_required_PatchItem_by_id_patch_title_and_note_set_marker"));
        assert!(runtime
            .contents
            .contains("policyPostgresMutationRows(tx, \"PatchItem\", \"update\""));
        assert!(runtime
            .contents
            .contains("CASE WHEN ?1 THEN ?2 ELSE \\\"title\\\" END"));
        assert!(targets[0].contents.contains("note?: string | null;"));
        assert!(targets[0].contents.contains(
            "hasOwn(object, \"note\") ? { note: ((raw, fieldPath) => { const candidate = validateField_9_PatchItem_note"
        ));
        assert!(runtime
            .contents
            .contains("\"account_tenant_owner_unique\":\"Account.tenant_owner\""));
        assert!(runtime
            .contents
            .contains("UNIQUE constraint failed: account.tenant, account.owner_email"));
        assert!(runtime.contents.contains("delete_required_Customer_by_id"));
        assert!(runtime
            .contents
            .contains("query_required_User_with_todos_by_id_order_by_id_asc"));
        assert!(runtime
            .contents
            .contains("query_required_PatchItem_with_reviewer_id_optional_by_id"));
        assert!(runtime
            .contents
            .contains("query_required_User_with_profile_optional_by_id"));
        assert!(runtime
            .contents
            .contains("return { parent, profile: related };"));
        assert!(runtime
            .contents
            .contains("query_required_Todo_with_owner_then_profile_optional_by_id"));
        assert!(runtime
            .contents
            .contains("return { parent: root, owner: { parent: owner, profile: leaf } };"));
        assert!(targets[0]
            .contents
            .contains("persistence.query_required_User_with_profile_optional_by_id(input.id)"));
        assert!(targets[0].contents.contains(
            "persistence.query_required_Todo_with_owner_then_profile_optional_by_id(input.id)"
        ));
        assert!(runtime
            .contents
            .contains("return { parent, reviewer_id: related };"));
        assert!(runtime.contents.contains(
            "query_many_User_with_todos_by_group_order_by_id_asc_include_order_by_id_asc_paginated"
        ));
        assert!(runtime.contents.contains(
            "query_many_Todo_by_owner_id_order_by_id_asc_paginated(value: unknown, limit: unknown, offset: unknown)"
        ));
        assert!(runtime
            .contents
            .contains("return { parent, todos: children };"));
        assert!(runtime.contents.contains("postgres.begin(async tx"));
        assert!(runtime.contents.contains("sqlite!.transaction"));
        assert!(runtime.contents.contains("BEGIN IMMEDIATE"));
        assert!(runtime.contents.contains("SAVEPOINT"));
        assert!(runtime.contents.contains("ROLLBACK TO SAVEPOINT"));
        assert!(runtime.contents.contains("PRAGMA foreign_keys = ON"));
        assert!(runtime
            .contents
            .contains("CONSTRAINT \\\"account_identity\\\" PRIMARY KEY (\\\"id\\\")"));
        assert!(runtime
            .contents
            .contains("CONSTRAINT \\\"account_handle_unique\\\" UNIQUE (\\\"handle\\\")"));
        assert!(runtime.contents.contains(
            "CONSTRAINT \\\"account_tenant_owner_unique\\\" UNIQUE (\\\"tenant\\\", \\\"owner_email\\\")"
        ));
        assert!(runtime
            .contents
            .contains("CREATE INDEX IF NOT EXISTS \\\"account_owner_email_idx\\\""));
        assert!(
            postgres_schema
                .contents
                .find("CREATE TABLE IF NOT EXISTS \"user\"")
                < postgres_schema
                    .contents
                    .find("CREATE TABLE IF NOT EXISTS \"todo\"")
        );
        assert!(postgres_schema.contents.contains(
            "CONSTRAINT \"todo_owner_id_fk\" FOREIGN KEY (\"owner_id\") REFERENCES \"user\" (\"id\") ON DELETE CASCADE"
        ));
        assert!(postgres_schema.contents.contains(
            "CREATE INDEX IF NOT EXISTS \"todo_owner_id_idx\" ON \"todo\" (\"owner_id\")"
        ));
        assert!(targets[0]
            .contents
            .contains("validate_Customer(await persistence.create_Customer"));
        assert!(targets[0]
            .contents
            .contains("await persistence.query_optional_Customer_by_id(input.id)"));
        assert!(targets[0]
            .contents
            .contains("await persistence.query_required_Customer_by_id(input.id)"));
        assert!(targets[0]
            .contents
            .contains("new DomainFailure(\"CustomerNotFound\""));
        assert!(targets[0]
            .contents
            .contains("new DomainFailure(\"CustomerMutationConflict\""));
        assert!(targets[0].contents.contains(
            "persistence.update_required_Account_by_id_set_handle_and_owner_email(predicateValue, input.handle, input.owner_email)"
        ));
        assert!(targets[0]
            .contents
            .contains("error.constraint === \"Account.handle\""));
        assert!(targets[0]
            .contents
            .contains("error.constraint === \"Account.tenant_owner\""));
        assert!(targets[0]
            .contents
            .contains("error.kind === \"constraint\""));
        assert!(targets[0]
            .contents
            .contains("async function create_atomic_pair(input: CreateAtomicPair, __operation: OperationContext = captureOperation(), __persistence"));
        assert!(targets[0]
            .contents
            .contains("return __persistence.withDeadline(__operation.deadlineAt).withSignal(__operation.signal).withOperationTime(__operation.now).transaction(async persistence =>"));
        assert!(targets[0].contents.contains(
            "error instanceof PersistenceFault && error.deadlineExceeded && error.kind !== \"unknown\""
        ));
        assert!(targets[0]
            .contents
            .contains("async function find_customer(input: FindCustomer, __operation: OperationContext = captureOperation(), __persistence"));
        assert!(!targets[0].contents.contains(
            "async function find_customer(input: FindCustomer, __persistence: typeof rootPersistence = rootPersistence): Promise<Customer | null> {\n  return __persistence.transaction"
        ));
    }

    #[test]
    fn rejects_external_runtime_dependencies_and_manifests() {
        let external = vec![GeneratedArtifact {
            relative_path: "target/app.ts",
            contents: "import \"third-party-package\";\n".to_owned(),
        }];
        let diagnostic = validate_runtime_dependency_contract(&external, false)
            .expect_err("bare package imports must be rejected");
        assert_eq!(diagnostic.code, "JADPO_TARGET_EXTERNAL_MODULE");

        let manifest = vec![GeneratedArtifact {
            relative_path: "package.json",
            contents: "{}\n".to_owned(),
        }];
        let diagnostic = validate_runtime_dependency_contract(&manifest, false)
            .expect_err("dependency manifests must be rejected");
        assert_eq!(diagnostic.code, "JADPO_TARGET_DEPENDENCY_MANIFEST");

        let builtin = vec![GeneratedArtifact {
            relative_path: "target/service-adapter.ts",
            contents: "import { isIP } from \"node:net\";".to_owned(),
        }];
        validate_runtime_dependency_contract(&builtin, false)
            .expect("the pinned adapter may use the approved built-in network module");
        let unapproved_builtin = vec![GeneratedArtifact {
            relative_path: "target/service-adapter.ts",
            contents: "import { spawn } from \"node:child_process\";".to_owned(),
        }];
        assert_eq!(
            validate_runtime_dependency_contract(&unapproved_builtin, false)
                .unwrap_err()
                .code,
            "JADPO_TARGET_EXTERNAL_MODULE"
        );
    }

    #[test]
    fn checked_service_operation_uses_generated_adapter_and_propagates_suspension() {
        let fixture =
            repository_root().join("tests/compile/pass/170_checked_service_operation.jadpo");
        let analyzed = analyze_project(&fixture).expect("checked service fixture should analyze");
        let outputs = derive_target(&fixture, &analyzed).expect("service target should generate");
        let application = &outputs
            .iter()
            .find(|output| output.relative_path == "target/app.ts")
            .expect("generated application")
            .contents;
        let adapter = &outputs
            .iter()
            .find(|output| output.relative_path == "target/service-adapter.ts")
            .expect("compiler-owned service adapter")
            .contents;

        assert!(application.contains(
            "import { invokeReferenceMail, ServiceAdapterFault } from \"./service-adapter.ts\";"
        ));
        assert!(application.contains("async function deliver(mail: ReminderMessage"));
        assert!(application.contains(
            "return await __service_ReminderMail__send_overdue_reminder(mail, __operation);"
        ));
        assert!(application.contains("errorEnvelope(\"outcome_unknown\""));
        assert!(application.contains("error instanceof ServiceAdapterFault ? ({ unavailable: \"Unavailable\", misconfigured: \"Misconfigured\", outcome_unknown: \"OutcomeUnknown\" } as const)[error.kind] : null"));
        let checked_input = application
            .find("const checkedInput = validate_ReminderMessage(input, \"service.ReminderMail.send_overdue_reminder.input\")")
            .expect("service request must be validated before adapter dispatch");
        let adapter_dispatch = application
            .find("const result = await invokeReferenceMail(checkedInput as unknown as Readonly<Record<string, unknown>>")
            .expect("service operation should call the reviewed adapter");
        assert!(checked_input < adapter_dispatch);
        assert!(adapter.contains("Bun.connect({"));
        assert!(adapter.contains("serverName: SERVICE_HOST"));
        assert!(adapter.contains("Idempotency-Key: ${key}"));
        assert!(adapter.contains("eventName: \"service.attempt\""));
        assert!(!adapter.contains("node:https"));
        assert!(!adapter.contains("process.env.HTTP_PROXY"));
    }

    #[test]
    fn jwt_exception_is_exactly_pinned_and_confined_to_its_adapter() {
        let approved = vec![
            GeneratedArtifact {
                relative_path: "target/package.json",
                contents: super::JWT_PACKAGE.to_owned(),
            },
            GeneratedArtifact {
                relative_path: "target/bun.lock",
                contents: super::JWT_LOCK.to_owned(),
            },
            GeneratedArtifact {
                relative_path: "target/jwt-authentication.ts",
                contents: "import { jwtVerify } from \"jose\";".to_owned(),
            },
        ];
        validate_runtime_dependency_contract(&approved, true).unwrap();
        assert!(validate_runtime_dependency_contract(&approved, false).is_err());
        for index in [0, 1] {
            let mut substituted = approved.clone();
            substituted[index].contents = substituted[index].contents.replace("6.2.12", "6.2.11");
            assert_eq!(
                validate_runtime_dependency_contract(&substituted, true)
                    .unwrap_err()
                    .code,
                "JADPO_TARGET_DEPENDENCY_MANIFEST"
            );
        }
        for import in ["jose/dist/webapi/index.js", "other-package"] {
            let mut escaped = approved.clone();
            escaped[2].contents = format!("import {{ jwtVerify }} from \"{import}\";");
            assert_eq!(
                validate_runtime_dependency_contract(&escaped, true)
                    .unwrap_err()
                    .code,
                "JADPO_TARGET_EXTERNAL_MODULE"
            );
        }
        let mut escaped = approved.clone();
        escaped[2].relative_path = "target/app.ts";
        assert_eq!(
            validate_runtime_dependency_contract(&escaped, true)
                .unwrap_err()
                .code,
            "JADPO_TARGET_EXTERNAL_MODULE"
        );
        for path in [
            "package.json",
            "target/node_modules/jose/index.ts",
            "target/bun.lockb",
        ] {
            let mut unexpected = approved.clone();
            unexpected.push(GeneratedArtifact {
                relative_path: path,
                contents: "".to_owned(),
            });
            assert_eq!(
                validate_runtime_dependency_contract(&unexpected, true)
                    .unwrap_err()
                    .code,
                "JADPO_TARGET_DEPENDENCY_MANIFEST"
            );
        }
        let mut duplicate = approved.clone();
        duplicate.push(approved[0].clone());
        assert!(validate_runtime_dependency_contract(&duplicate, true).is_err());
        assert!(validate_runtime_dependency_contract(&approved[1..], true).is_err());
    }

    #[test]
    fn non_jwt_application_has_no_dependency_or_jose_footprint() {
        let seed = repository_root().join("examples/jadpo-seed");
        let analyzed = analyze_project(&seed).expect("seed should analyze");
        let targets = derive_target(&seed, &analyzed).expect("seed target should generate");

        assert!(targets.iter().all(|target| {
            !matches!(
                Path::new(target.relative_path)
                    .file_name()
                    .and_then(|name| name.to_str()),
                Some("package.json" | "bun.lock" | "bun.lockb")
            )
        }));
        assert!(targets.iter().all(|target| {
            !target.contents.contains("jose")
                && !target.contents.contains("node_modules")
                && !target.contents.contains("bun install")
                && !target.contents.contains("npm install")
        }));
    }

    #[test]
    fn generates_the_strategy_independent_authentication_selector() {
        let fixture =
            repository_root().join("tests/compile/pass/128_authentication_strategy_topology.jadpo");
        let analyzed = analyze_project(&fixture).expect("authentication fixture should analyze");
        assert!(
            analyzed
                .semantics
                .diagnostics
                .iter()
                .chain(analyzed.typing.diagnostics.iter())
                .next()
                .is_none(),
            "authentication fixture should check"
        );
        let targets = derive_target(&fixture, &analyzed)
            .expect("the selector can be generated before protected-route integration");
        let authentication = targets
            .iter()
            .find(|target| target.relative_path == "target/authentication.ts")
            .expect("authentication target");

        for expected in [
            "inventoryAuthenticationCredentials",
            "ambiguous_credentials",
            "invalid_credentials",
            "authentication_required",
            "requirement.freshAuthority",
            "adapter.resolve(identity)",
            "normalizeAuthPrincipal",
            "principal_inactive",
            "authority_invariant",
        ] {
            assert!(authentication.contents.contains(expected), "{expected}");
        }
        let application = targets
            .iter()
            .find(|target| target.relative_path == "target/app.ts")
            .expect("application target");
        for expected in [
            "type Principal = Readonly<{ readonly tag: \"user\"",
            "const principal = materializePrincipal(__operation.principal);",
            "switch (matchSubject",
        ] {
            assert!(application.contents.contains(expected), "{expected}");
        }
        assert!(!authentication.contents.contains("provider response"));
        assert!(!authentication.contents.contains("rawClaims"));
    }
}

/// Explicit Bun boundary policy; shared semantics have no OutcomeUnknown HTTP default.
pub(crate) fn bun_failure_http_status(kind: &str, default: Option<u16>) -> u16 {
    match (kind, default) {
        ("OutcomeUnknown", None) => 500,
        (_, Some(status)) => status,
        _ => panic!("checked failure kind has no Bun HTTP mapping: {kind}"),
    }
}
