// Rich compiler diagnostics are the intentional public error contract. Boxing
// every `Result` error would spread transport concerns through the core API.
#![allow(clippy::result_large_err)]

use jadpo_diagnostics::{Diagnostic, DiagnosticFact, SourceSpan};
use jadpo_semantic::{
    build_semantic_graph, check_failures, check_types, FailureCheckResult, ScaffoldManifest,
    SemanticGraph, TypeCheckResult,
};
use jadpo_syntax::{
    parse, ConstraintKind, Declaration, ParsedSyntax, PersistenceModifier, RecordKind, SourceFile,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

mod artifacts;
mod configuration;
mod delivery;
mod delivery_hooks;
mod delivery_phases;
mod entity_model;
mod formatter;
mod index_advisor;
mod language_service;
mod migration_identity;
mod policy;
mod scaffold;
mod target;

pub use artifacts::{
    approval_text, derive_approval_subject, derive_approval_subject_with_state_pin,
    derive_artifacts, derive_protected_attempt, validate_approval_export,
    validate_protected_attempt_export, write_artifacts, GeneratedArtifact,
};
pub use configuration::{
    check_local_configuration, configuration_fields, local_configuration_environment,
    set_local_configuration, ConfigurationField, LocalConfigurationState, LocalConfigurationStatus,
};
pub use entity_model::{
    EntityContract, EntityModel, QueryContract, RepresentationContract, TransactionContract,
};
pub use delivery_phases::DeliveryPhaseCandidate;
pub use delivery::{CheckedReminderDeliveryBinding, DeliveryModel, DeliveryPrivateEdge, DeliveryPrivateGraph, DeliveryPrivateNode};
pub use delivery_hooks::DeliveryHookCandidate;
pub use formatter::format_source;
pub use index_advisor::{
    accept_index_recommendation, index_recommendation_count, index_recommendations_json,
};
pub use language_service::{LanguageIndex, LanguageOccurrence, LanguageSymbol};
pub use migration_identity::{
    diff_schema_identities, initialize_schema_identities, register_schema_additions,
    rename_schema_identity, snapshot_schema_identities, validate_schema_decisions,
    validate_schema_identities, write_schema_decision_template, write_schema_migration_plan,
    write_schema_migration_sql_review,
};
pub use policy::{
    EntityPolicyContract, FieldPolicyContract, MembershipContract, OperationPolicyContract,
    PolicyFieldRead, PolicyModel, PolicyObligation, PolicyRuleContract, RoleBindingContract,
};
pub use scaffold::{create_project, ScaffoldFile};
pub use target::derive_target;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedProject {
    pub sources: Vec<ParsedSyntax>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalyzedProject {
    pub syntax: ParsedProject,
    pub semantics: SemanticGraph,
    pub typing: TypeCheckResult,
    pub failures: FailureCheckResult,
    pub entity_model: EntityModel,
    pub policy: PolicyModel,
    delivery_phase_candidates: Vec<DeliveryPhaseCandidate>,
    delivery_model: DeliveryModel,
    delivery_hook_candidates: Vec<DeliveryHookCandidate>,
}

impl AnalyzedProject {
    /// Incomplete nonexecuting obligations, not checked delivery authority.
    pub fn delivery_phase_candidates(&self) -> &[DeliveryPhaseCandidate] {
        &self.delivery_phase_candidates
    }
    pub fn delivery_model(&self) -> &DeliveryModel {
        &self.delivery_model
    }
    pub fn delivery_hook_candidates(&self) -> &[DeliveryHookCandidate] {
        &self.delivery_hook_candidates
    }
}

impl ParsedProject {
    pub fn diagnostics(&self) -> impl Iterator<Item = &Diagnostic> {
        self.sources
            .iter()
            .flat_map(|source| source.diagnostics.iter())
    }

    pub fn declaration_count(&self) -> usize {
        self.sources
            .iter()
            .map(|source| {
                source.file.declarations.len()
                    + source.file.persistence.len()
                    + source.file.services.len()
            })
            .sum()
    }
}

pub fn checked_source_revision(project_path: &Path, project: &AnalyzedProject) -> String {
    let project_root = if project_path.is_dir() {
        project_path
    } else {
        project_path.parent().unwrap_or_else(|| Path::new("."))
    };
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for source in &project.syntax.sources {
        let normalized = Path::new(&source.source_name)
            .strip_prefix(project_root)
            .unwrap_or_else(|_| Path::new(&source.source_name))
            .to_string_lossy()
            .replace('\\', "/");
        for byte in normalized
            .as_bytes()
            .iter()
            .chain(std::iter::once(&0))
            .chain(source.source_text.as_bytes())
            .chain(std::iter::once(&0xff))
        {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("src_{hash:016x}")
}

pub fn discover_sources(project: &Path) -> Result<Vec<SourceFile>, Diagnostic> {
    if !project.exists() {
        return Err(Diagnostic::error("JADPO_PROJECT_NOT_FOUND"));
    }

    let mut paths = Vec::new();
    collect_source_paths(project, &mut paths)?;
    paths.sort();

    if paths.is_empty() {
        return Err(Diagnostic::error("JADPO_NO_SOURCES"));
    }

    paths
        .into_iter()
        .map(|path| {
            fs::read_to_string(&path)
                .map(|text| SourceFile::new(path.clone(), text))
                .map_err(|_error| Diagnostic::error("JADPO_SOURCE_READ_FAILED"))
        })
        .collect()
}

pub fn scaffold_manifest(project: &Path) -> Result<ScaffoldManifest, Diagnostic> {
    let sources = discover_sources(project)?;
    let project_root = if project.is_dir() {
        project
    } else {
        project.parent().unwrap_or_else(|| Path::new("."))
    };

    let mut source_files = sources
        .iter()
        .map(|source| {
            source
                .path
                .strip_prefix(project_root)
                .unwrap_or(&source.path)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect::<Vec<_>>();
    source_files.sort();

    Ok(ScaffoldManifest {
        schema_version: 1,
        phase: "scaffold",
        source_files,
    })
}

pub fn parse_project(project: &Path) -> Result<ParsedProject, Diagnostic> {
    let sources = discover_sources(project)?;
    let mut sources = sources
        .into_iter()
        .map(|source| parse(&source.path, &source.text))
        .collect::<Vec<_>>();
    apply_builtin_compatibility(&mut sources);
    apply_persistence_declarations(&mut sources);

    Ok(ParsedProject { sources })
}

pub fn analyze_project(project: &Path) -> Result<AnalyzedProject, Diagnostic> {
    analyze_sources(discover_sources(project)?)
}

pub fn analyze_sources(mut sources: Vec<SourceFile>) -> Result<AnalyzedProject, Diagnostic> {
    sources.sort_by(|left, right| left.path.cmp(&right.path));
    let mut parsed_sources = sources
        .into_iter()
        .map(|source| parse(&source.path, &source.text))
        .collect::<Vec<_>>();
    apply_builtin_compatibility(&mut parsed_sources);
    apply_persistence_declarations(&mut parsed_sources);
    let syntax = ParsedProject {
        sources: parsed_sources,
    };
    if syntax.declaration_count() == 0 && syntax.diagnostics().next().is_none() {
        return Err(Diagnostic::error("JADPO_EMPTY_PROJECT")
            .with_note("add a type, enum, record, failure, callable, or route declaration"));
    }
    let mut semantics = build_semantic_graph(&syntax.sources);
    let typing = check_types(&syntax.sources, &semantics);
    let failures = check_failures(&syntax.sources, &semantics);
    let entity_model = entity_model::analyze_entity_model(&syntax.sources);
    let policy = policy::analyze_policy(&syntax.sources, &typing);
    let (hook_diagnostics, delivery_hook_candidates) = delivery_hooks::check_delivery_hooks(&syntax.sources, &semantics);
    semantics.diagnostics.extend(hook_diagnostics);
    let (phase_diagnostics, delivery_phase_candidates) = delivery_phases::check_delivery_phases(
        &syntax.sources,
        &semantics,
        &policy,
        &typing.delivery_candidates,
    );
    semantics.diagnostics.extend(phase_diagnostics);
    semantics
        .diagnostics
        .extend(entity_model.diagnostics.iter().cloned());
    semantics
        .diagnostics
        .extend(policy.diagnostics.iter().cloned());
    let (delivery_model, delivery_finish_diagnostics) = delivery::finish_delivery_candidates(
        &syntax, &mut semantics, &typing, &failures, &delivery_phase_candidates, &delivery_hook_candidates,
    );
    semantics.diagnostics.extend(delivery_finish_diagnostics);
    Ok(AnalyzedProject {
        syntax,
        semantics,
        typing,
        failures,
        entity_model,
        policy,
        delivery_phase_candidates,
        delivery_model,
        delivery_hook_candidates,
    })
}

fn apply_builtin_compatibility(sources: &mut [ParsedSyntax]) {
    for source in sources {
        source.file.declarations.retain(|declaration| {
            let Declaration::Type(declaration) = declaration else {
                return true;
            };
            if declaration.name.text != "Email"
                || declaration.parent.path.len() != 1
                || declaration.parent.path[0].text != "Text"
            {
                return true;
            }
            !(declaration.constraints.iter().any(|constraint| {
                constraint.kind == ConstraintKind::Format && constraint.value.text == "email"
            }) && declaration
                .constraints
                .iter()
                .all(|constraint| match constraint.kind {
                    ConstraintKind::Format => constraint.value.text == "email",
                    ConstraintKind::MaxLength => constraint.value.text == "254",
                    _ => false,
                }))
        });
    }
}

fn apply_persistence_declarations(sources: &mut [ParsedSyntax]) {
    let records = sources
        .iter()
        .enumerate()
        .flat_map(|(source_index, source)| {
            source.file.declarations.iter().enumerate().filter_map(
                move |(declaration_index, declaration)| match declaration {
                    Declaration::Record(record) => {
                        Some((record.name.text.clone(), (source_index, declaration_index)))
                    }
                    _ => None,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let declaration_kinds = sources
        .iter()
        .flat_map(|source| source.file.declarations.iter())
        .filter_map(|declaration| match declaration {
            Declaration::Type(value) => Some((value.name.text.clone(), "scalar type")),
            Declaration::Enum(value) => Some((value.name.text.clone(), "enum type")),
            Declaration::Failure(value) => Some((value.name.text.clone(), "failure")),
            Declaration::Callable(value) => Some((value.name.text.clone(), "callable")),
            Declaration::Application(_)
            | Declaration::Locales(_)
            | Declaration::AuthenticationStrategy(_)
            | Declaration::Principal(_)
            | Declaration::Config(_)
            | Declaration::Record(_)
            | Declaration::Fixture(_)
            | Declaration::Test(_)
            | Declaration::Route(_)
            | Declaration::Job(_) => None,
        })
        .collect::<BTreeMap<_, _>>();
    let persistence = sources
        .iter()
        .enumerate()
        .flat_map(|(source_index, source)| {
            source
                .file
                .persistence
                .iter()
                .cloned()
                .map(move |declaration| (source_index, declaration))
        })
        .collect::<Vec<_>>();

    for (persistence_source, specification) in persistence {
        let Some(&(record_source, record_declaration)) = records.get(&specification.target.text)
        else {
            let diagnostic =
                if let Some(actual_kind) = declaration_kinds.get(&specification.target.text) {
                    Diagnostic::error("SEM_WRONG_NAME_KIND")
                        .with_fact(DiagnosticFact::Name(specification.target.text.clone()))
                        .with_fact(DiagnosticFact::Expected("object type".to_owned()))
                        .with_fact(DiagnosticFact::ActualKind((*actual_kind).to_owned()))
                        .with_fact(DiagnosticFact::Usage("persistence declaration".to_owned()))
                } else {
                    Diagnostic::error("SEM_UNKNOWN_NAME")
                        .with_fact(DiagnosticFact::Name(specification.target.text.clone()))
                        .with_fact(DiagnosticFact::Expected("object type".to_owned()))
                        .with_fact(DiagnosticFact::Usage("persistence declaration".to_owned()))
                };
            push_persistence_diagnostic(
                sources,
                persistence_source,
                diagnostic,
                specification.target.range,
            );
            continue;
        };

        let mut missing_fields = Vec::new();
        {
            let Declaration::Record(record) =
                &mut sources[record_source].file.declarations[record_declaration]
            else {
                unreachable!("record index was collected from a record declaration")
            };
            record.kind = RecordKind::Entity;

            for (names, modifier) in [
                (&specification.identities, PersistenceModifier::Identity),
                (&specification.uniques, PersistenceModifier::Unique),
                (&specification.indexes, PersistenceModifier::Index),
            ] {
                for name in names {
                    if let Some(field) = record
                        .fields
                        .iter_mut()
                        .find(|field| field.name.text == name.text)
                    {
                        if !field.persistence.contains(&modifier) {
                            field.persistence.push(modifier);
                        }
                    } else {
                        missing_fields.push(name.clone());
                    }
                }
            }
            for reference in &specification.references {
                if let Some(field) = record
                    .fields
                    .iter_mut()
                    .find(|field| field.name.text == reference.field.text)
                {
                    field.reference = Some(reference.reference.clone());
                } else {
                    missing_fields.push(reference.field.clone());
                }
            }
            record
                .persistence_constraints
                .extend(specification.constraints.clone());
            record.inverses.extend(specification.inverses.clone());
        }
        for field in &missing_fields {
            push_unknown_persistence_field(
                sources,
                persistence_source,
                &specification.target.text,
                field,
            );
        }
    }
}

fn push_unknown_persistence_field(
    sources: &mut [ParsedSyntax],
    source_index: usize,
    target: &str,
    field: &jadpo_syntax::Name,
) {
    let diagnostic = Diagnostic::error("SEM_UNKNOWN_NAME")
        .with_fact(DiagnosticFact::Name(field.text.clone()))
        .with_fact(DiagnosticFact::Expected("field".to_owned()))
        .with_fact(DiagnosticFact::Usage(format!("`persist {target}` setting")));
    push_persistence_diagnostic(sources, source_index, diagnostic, field.range);
}

fn push_persistence_diagnostic(
    sources: &mut [ParsedSyntax],
    source_index: usize,
    mut diagnostic: Diagnostic,
    range: jadpo_syntax::TextRange,
) {
    diagnostic.primary = Some(SourceSpan {
        source: sources[source_index].source_name.clone(),
        start: range.start,
        end: range.end,
    });
    sources[source_index].diagnostics.push(diagnostic);
}

fn collect_source_paths(path: &Path, output: &mut Vec<PathBuf>) -> Result<(), Diagnostic> {
    if path.is_file() {
        if path.extension().and_then(|extension| extension.to_str()) == Some("jadpo") {
            output.push(path.to_owned());
        }
        return Ok(());
    }

    let entries =
        fs::read_dir(path).map_err(|_error| Diagnostic::error("JADPO_PROJECT_READ_FAILED"))?;

    for entry in entries {
        let entry = entry.map_err(|_error| Diagnostic::error("JADPO_PROJECT_READ_FAILED"))?;
        let entry_path = entry.path();

        if entry_path.is_dir()
            && entry_path.file_name().and_then(|name| name.to_str()) != Some("build")
        {
            collect_source_paths(&entry_path, output)?;
        } else if entry_path
            .extension()
            .and_then(|extension| extension.to_str())
            == Some("jadpo")
        {
            output.push(entry_path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{analyze_project, discover_sources, parse_project, scaffold_manifest};
    use std::fs;
    use std::path::Path;

    fn repository_root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("compiler crate should be inside the repository")
    }

    #[test]
    fn discovers_the_jadpo_seed() {
        let seed = repository_root().join("examples/jadpo-seed");
        let sources = discover_sources(&seed).expect("seed should be discoverable");

        assert_eq!(sources.len(), 1);
        assert!(sources[0].path.ends_with("app.jadpo"));
    }

    #[test]
    fn discovers_all_compile_fixtures() {
        let fixtures = repository_root().join("tests/compile");
        let sources = discover_sources(&fixtures).expect("fixtures should be discoverable");

        assert_eq!(sources.len(), 216);
    }

    #[test]
    fn never_discovers_generated_sources_beneath_build() {
        let root =
            std::env::temp_dir().join(format!("jadpo-source-boundary-{}", std::process::id()));
        fs::create_dir_all(root.join("build/generated"))
            .expect("temporary build directory should be created");
        fs::write(root.join("app.jadpo"), "type Name = Text {}\n")
            .expect("authored source should be written");
        fs::write(
            root.join("build/generated/accidental.jadpo"),
            "type Generated = Text {}\n",
        )
        .expect("generated source should be written");

        let sources = discover_sources(&root).expect("project should be discoverable");
        assert_eq!(sources.len(), 1);
        assert!(sources[0].path.ends_with("app.jadpo"));
        fs::remove_dir_all(root).expect("temporary project should be removable");
    }

    #[test]
    fn emits_a_deterministic_scaffold_manifest() {
        let seed = repository_root().join("examples/jadpo-seed");
        let manifest = scaffold_manifest(&seed).expect("manifest should build");

        assert_eq!(manifest.schema_version, 1);
        assert_eq!(manifest.phase, "scaffold");
        assert_eq!(manifest.source_files, vec!["app.jadpo"]);
    }

    #[test]
    fn parses_the_jadpo_seed_without_diagnostics() {
        let seed = repository_root().join("examples/jadpo-seed");
        let project = parse_project(&seed).expect("seed should be readable");

        assert_eq!(project.sources.len(), 1);
        assert_eq!(project.declaration_count(), 9);
        assert_eq!(project.diagnostics().count(), 0);
    }

    #[test]
    fn preserves_specific_parser_diagnostics() {
        let fixture = repository_root().join("tests/compile/fail/10_arbitrary_exception.jadpo");
        let project = parse_project(&fixture).expect("fixture should be readable");
        let diagnostics = project.diagnostics().collect::<Vec<_>>();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "SYN_UNSUPPORTED_THROW");
    }

    #[test]
    fn rejects_a_project_with_no_declarations() {
        let root = std::env::temp_dir().join(format!("jadpo-empty-project-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("stale fixture should be removable");
        }
        fs::create_dir_all(&root).expect("fixture should be created");
        fs::write(root.join("empty.jadpo"), "\n// still empty\n")
            .expect("empty source should be written");

        let diagnostic = analyze_project(&root).expect_err("empty project should fail");

        assert_eq!(diagnostic.code, "JADPO_EMPTY_PROJECT");
        fs::remove_dir_all(root).expect("fixture should be removable");
    }

    #[test]
    fn permits_an_empty_file_beside_project_declarations() {
        let root =
            std::env::temp_dir().join(format!("jadpo-empty-companion-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).expect("stale fixture should be removable");
        }
        fs::create_dir_all(&root).expect("fixture should be created");
        fs::write(root.join("empty.jadpo"), "").expect("empty source should be written");
        fs::write(root.join("app.jadpo"), "type Name = Text {}\n")
            .expect("declaration should be written");

        let project = analyze_project(&root).expect("non-empty project should analyze");

        assert_eq!(project.syntax.sources.len(), 2);
        assert_eq!(project.syntax.declaration_count(), 1);
        fs::remove_dir_all(root).expect("fixture should be removable");
    }

    #[test]
    fn builds_the_seed_semantic_contract() {
        let seed = repository_root().join("examples/jadpo-seed");
        let project = analyze_project(&seed).expect("seed should be analyzable");

        assert_eq!(project.syntax.diagnostics().count(), 0);
        assert!(
            project.semantics.diagnostics.is_empty(),
            "{:#?}",
            project.semantics.diagnostics
        );
        assert!(
            project.typing.diagnostics.is_empty(),
            "{:#?}",
            project.typing.diagnostics
        );
        assert!(
            project.failures.diagnostics.is_empty(),
            "{:#?}",
            project.failures.diagnostics
        );
        for expected in [
            "Email",
            "InviteCode",
            "Customer",
            "Customer.id",
            "Customer.email",
            "RegisterCustomer",
            "RegisterCustomer.email",
            "RegisterCustomer.invite_code",
            "RegistrationAccepted",
            "RegistrationAccepted.email",
            "InviteCodeRejected",
            "InviteCodeRejected.internal.invite_code",
            "register_customer",
            "POST /registrations",
            "GET /registrations/{email}",
        ] {
            assert!(
                project.semantics.node(expected).is_some(),
                "missing {expected}"
            );
        }

        let refinements = project
            .semantics
            .refinements
            .iter()
            .map(|edge| {
                (
                    project.semantics.nodes[edge.refined.0 as usize]
                        .name
                        .as_str(),
                    project.semantics.nodes[edge.parent.0 as usize]
                        .name
                        .as_str(),
                )
            })
            .collect::<Vec<_>>();
        for expected in [
            ("Email", "Text"),
            ("InviteCode", "Text"),
            ("Customer.id", "Uuid"),
            ("Customer.email", "Email"),
            ("RegisterCustomer.email", "Customer.email"),
            ("RegisterCustomer.invite_code", "InviteCode"),
            ("RegistrationAccepted.email", "Customer.email"),
            ("InviteCodeRejected.internal.invite_code", "InviteCode"),
        ] {
            assert!(refinements.contains(&expected), "missing {expected:?}");
        }
    }

    #[test]
    fn indexes_each_syntax_valid_fixture_without_name_errors() {
        let root = repository_root().join("tests/compile");
        for source in discover_sources(&root).expect("fixtures should be discoverable") {
            let project = analyze_project(&source.path).expect("fixture should be analyzable");
            if source
                .path
                .components()
                .any(|component| component.as_os_str() == "pass")
                && project.syntax.diagnostics().count() == 0
            {
                assert!(
                    project.semantics.diagnostics.is_empty(),
                    "{}: {:#?}",
                    source.path.display(),
                    project.semantics.diagnostics
                );
            }
        }
    }

    #[test]
    fn type_fixtures_emit_their_contract_diagnostics_and_spans() {
        let fixtures = [
            ("pass/01_valid_email_literal", Vec::<&str>::new()),
            (
                "fail/02_invalid_email_literal",
                vec!["TYPE_INVALID_LITERAL"],
            ),
            (
                "fail/03_wrong_identifier_domain",
                vec!["TYPE_SIBLING_MISMATCH"],
            ),
            ("pass/04_field_widening", Vec::<&str>::new()),
            (
                "fail/05_sibling_field_rejection",
                vec!["TYPE_SIBLING_MISMATCH"],
            ),
            (
                "fail/06_primitive_callable_signature",
                vec!["TYPE_PRIMITIVE_SIGNATURE", "TYPE_PRIMITIVE_SIGNATURE"],
            ),
            ("pass/11_optional_and_nullable", Vec::<&str>::new()),
            ("pass/12_nullable_field_widening", Vec::<&str>::new()),
            ("fail/13_implicit_narrowing", vec!["TYPE_MISMATCH"]),
            ("fail/14_semantic_to_primitive", vec!["TYPE_MISMATCH"]),
            ("fail/15_missing_record_field", vec!["TYPE_MISSING_FIELD"]),
            ("fail/16_invariant_collection", vec!["TYPE_MISMATCH"]),
            (
                "fail/21_failure_context_type",
                vec!["TYPE_SIBLING_MISMATCH"],
            ),
            (
                "pass/22_nested_structured_field_selection",
                Vec::<&str>::new(),
            ),
            (
                "fail/23_nullable_structured_selection",
                vec!["TYPE_NULLABLE_SELECTION"],
            ),
            ("pass/24_create_entity", Vec::<&str>::new()),
            ("pass/26_query_optional_entity", Vec::<&str>::new()),
            ("pass/29_query_required_entity", Vec::<&str>::new()),
            ("pass/32_update_required_entity", Vec::<&str>::new()),
            ("pass/33_delete_required_entity", Vec::<&str>::new()),
            ("pass/34_update_multiple_fields", Vec::<&str>::new()),
            (
                "fail/37_update_duplicate_field",
                vec!["TYPE_UPDATE_DUPLICATE_FIELD"],
            ),
            ("pass/38_compound_constraint_mapping", Vec::<&str>::new()),
            (
                "fail/39_unknown_constraint_mapping",
                vec!["TYPE_CONFLICT_UNKNOWN_CONSTRAINT"],
            ),
            (
                "fail/28_query_nullable_field",
                vec!["TYPE_QUERY_NULLABLE_FIELD_UNSUPPORTED"],
            ),
            ("pass/46_optional_parent_include", Vec::<&str>::new()),
            ("pass/48_optional_inverse_include", Vec::<&str>::new()),
            ("pass/49_bounded_nested_include", Vec::<&str>::new()),
            ("pass/50_named_owning_reference", Vec::<&str>::new()),
            ("pass/51_mutable_local_reassignment", Vec::<&str>::new()),
            ("pass/53_plain_enum_exhaustive_match", Vec::<&str>::new()),
            ("pass/54_scalar_match", Vec::<&str>::new()),
            ("pass/56_tagged_sum_match", Vec::<&str>::new()),
            ("pass/57_operators_and_precedence", Vec::<&str>::new()),
            ("pass/58_authored_tests", Vec::<&str>::new()),
            (
                "fail/52_immutable_reassignment",
                vec![
                    "TYPE_ASSIGN_IMMUTABLE",
                    "TYPE_ASSIGN_IMMUTABLE",
                    "TYPE_MISMATCH",
                ],
            ),
            (
                "fail/53_non_exhaustive_enum_match",
                vec!["TYPE_MATCH_NON_EXHAUSTIVE"],
            ),
            (
                "fail/54_non_exhaustive_bool_match",
                vec!["TYPE_MATCH_NON_EXHAUSTIVE"],
            ),
            (
                "fail/55_invalid_enum_match_patterns",
                vec![
                    "TYPE_MATCH_DUPLICATE_PATTERN",
                    "TYPE_MATCH_UNKNOWN_VARIANT",
                    "TYPE_MATCH_UNREACHABLE_PATTERN",
                ],
            ),
            (
                "fail/56_invalid_tagged_sum",
                vec!["TYPE_MISSING_VARIANT_FIELD", "TYPE_MATCH_UNKNOWN_BINDING"],
            ),
            (
                "fail/47_required_nullable_parent_include",
                vec!["TYPE_PARENT_INCLUDE_NULLABLE_REFERENCE"],
            ),
        ];
        let expected_spans = [
            (
                "fail/02_invalid_email_literal",
                vec!["Email(\"not-an-email\")"],
            ),
            ("fail/03_wrong_identifier_domain", vec!["order.id"]),
            ("fail/05_sibling_field_rejection", vec!["supplier.email"]),
            (
                "fail/06_primitive_callable_signature",
                vec!["value: Text", "-> Text"],
            ),
            (
                "fail/13_implicit_narrowing",
                vec!["Email(\"person@example.com\")"],
            ),
            (
                "fail/14_semantic_to_primitive",
                vec!["Email(\"person@example.com\")"],
            ),
            ("fail/15_missing_record_field", vec!["Receipt {}"]),
            ("fail/16_invariant_collection", vec!["groups.items"]),
            ("fail/21_failure_context_type", vec!["supplier.email"]),
            ("fail/23_nullable_structured_selection", vec!["postal_code"]),
            ("fail/28_query_nullable_field", vec!["email"]),
            (
                "fail/47_required_nullable_parent_include",
                vec!["include: reviewer_id required into: PatchItemReviewer"],
            ),
            (
                "fail/52_immutable_reassignment",
                vec![
                    "original = replacement",
                    "fixed = replacement",
                    "(replacement)",
                ],
            ),
            (
                "fail/57_invalid_operator_operands",
                vec![
                    "TYPE_ARITHMETIC_OPERAND",
                    "TYPE_LOGICAL_OPERAND",
                    "TYPE_ORDERING_OPERAND",
                    "TYPE_UNARY_OPERAND",
                ],
            ),
            ("fail/58_assert_requires_bool", vec!["TYPE_MISMATCH"]),
            ("fail/37_update_duplicate_field", vec!["email: (email)"]),
            (
                "fail/39_unknown_constraint_mapping",
                vec!["Membership.no_such_constraint"],
            ),
        ];

        for (fixture, expected_codes) in fixtures {
            let path = repository_root()
                .join("tests/compile")
                .join(format!("{fixture}.jadpo"));
            let project = analyze_project(&path).expect("fixture should be analyzable");
            let codes = project
                .typing
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>();
            assert_eq!(codes, expected_codes, "{fixture}");

            if let Some((_, matches)) = expected_spans
                .iter()
                .find(|(expected_fixture, _)| *expected_fixture == fixture)
            {
                let source = fs::read_to_string(&path).expect("fixture should be readable");
                let actual = project
                    .typing
                    .diagnostics
                    .iter()
                    .map(|diagnostic| {
                        let span = diagnostic
                            .primary
                            .as_ref()
                            .expect("diagnostic needs a span");
                        &source[span.start..span.end]
                    })
                    .collect::<Vec<_>>();
                assert_eq!(actual, *matches, "{fixture}");
            }

            if fixture == "pass/22_nested_structured_field_selection" {
                let source = fs::read_to_string(&path).expect("fixture should be readable");
                let expression = "customer.billing_address.postal_code";
                let start = source.find(expression).expect("expression should exist");
                let end = start + expression.len();
                assert!(project.typing.expressions.iter().any(|inferred| {
                    inferred.range.start == start
                        && inferred.range.end == end
                        && inferred.type_name == "Address.postal_code"
                }));
            }
            if fixture == "pass/24_create_entity" {
                let source = fs::read_to_string(&path).expect("fixture should be readable");
                let expression =
                    "create Customer {\n        id: input.id\n        email: input.email\n    }";
                let start = source
                    .find(expression)
                    .expect("create expression should exist");
                let end = start + expression.len();
                assert!(project.typing.expressions.iter().any(|inferred| {
                    inferred.range.start == start
                        && inferred.range.end == end
                        && inferred.type_name == "Customer"
                }));
            }
            if fixture == "pass/26_query_optional_entity" {
                let source = fs::read_to_string(&path).expect("fixture should be readable");
                let expression = "query optional Customer {\n        where: id == input.id\n    }";
                let start = source
                    .find(expression)
                    .expect("query expression should exist");
                let end = start + expression.len();
                assert!(project.typing.expressions.iter().any(|inferred| {
                    inferred.range.start == start
                        && inferred.range.end == end
                        && inferred.type_name == "Customer?"
                }));
            }
            if fixture == "pass/29_query_required_entity" {
                let source = fs::read_to_string(&path).expect("fixture should be readable");
                let expression = "query required Customer {\n        where: id == input.id\n        missing: CustomerNotFound {\n                customer_id: input.id\n        }\n    }";
                let start = source
                    .find(expression)
                    .expect("required query expression should exist");
                let end = start + expression.len();
                assert!(project.typing.expressions.iter().any(|inferred| {
                    inferred.range.start == start
                        && inferred.range.end == end
                        && inferred.type_name == "Customer"
                }));
            }
            if fixture == "pass/32_update_required_entity" {
                let source = fs::read_to_string(&path).expect("fixture should be readable");
                let expression = "update required Customer {\n        where: id == input.id\n        set: {\n            email: input.email\n        }\n        missing: CustomerNotFound\n        conflict: CustomerConflict\n    }";
                let start = source
                    .find(expression)
                    .expect("update expression should exist");
                let end = start + expression.len();
                assert!(project.typing.expressions.iter().any(|inferred| {
                    inferred.range.start == start
                        && inferred.range.end == end
                        && inferred.type_name == "Customer"
                }));
            }
            if fixture == "pass/33_delete_required_entity" {
                let source = fs::read_to_string(&path).expect("fixture should be readable");
                let expression = "delete required Customer {\n        where: id == id\n        missing: CustomerNotFound\n        conflict: CustomerConflict\n    }";
                let start = source
                    .find(expression)
                    .expect("delete expression should exist");
                let end = start + expression.len();
                assert!(project.typing.expressions.iter().any(|inferred| {
                    inferred.range.start == start
                        && inferred.range.end == end
                        && inferred.type_name == "Customer"
                }));
            }
        }
    }

    #[test]
    fn failure_fixtures_emit_contracts_routes_diagnostics_and_spans() {
        let fixtures = [
            (
                "fail/07_undeclared_failure",
                vec!["FAIL_UNDECLARED_PROPAGATION"],
                vec!["reject RegistrationClosed"],
            ),
            (
                "pass/08_automatic_not_found_mapping",
                Vec::new(),
                Vec::new(),
            ),
            (
                "pass/09_internal_context_not_public",
                Vec::new(),
                Vec::new(),
            ),
            (
                "fail/17_transitive_failure_propagation",
                vec!["FAIL_UNDECLARED_PROPAGATION"],
                vec!["child()"],
            ),
            (
                "fail/18_function_calls_action",
                vec!["EFFECT_FUNCTION_CALLS_ACTION"],
                vec!["perform()"],
            ),
            (
                "fail/19_missing_failure_context",
                vec!["FAIL_MISSING_CONTEXT_FIELD"],
                vec!["reject Missing"],
            ),
            (
                "fail/20_duplicate_failure_code",
                vec!["FAIL_DUPLICATE_CODE"],
                vec!["Second {\n    kind: Conflict\n    code: \"same\""],
            ),
            (
                "fail/25_function_create_effect",
                vec!["EFFECT_FUNCTION_PERSISTENCE"],
                vec!["create Customer {\n        id: id\n    }"],
            ),
            (
                "fail/27_function_query_effect",
                vec!["EFFECT_FUNCTION_PERSISTENCE"],
                vec!["query optional Customer {\n        where: id == id\n    }"],
            ),
            ("pass/29_query_required_entity", Vec::new(), Vec::new()),
            (
                "fail/30_query_required_wrong_failure_kind",
                vec!["FAIL_REQUIRED_QUERY_NOT_NOT_FOUND"],
                vec!["LookupRejected"],
            ),
            (
                "fail/31_query_required_undeclared_failure",
                vec!["FAIL_UNDECLARED_PROPAGATION"],
                vec!["CustomerNotFound"],
            ),
            ("pass/32_update_required_entity", Vec::new(), Vec::new()),
            ("pass/33_delete_required_entity", Vec::new(), Vec::new()),
            (
                "fail/35_mutation_missing_wrong_kind",
                vec!["FAIL_REQUIRED_MUTATION_NOT_NOT_FOUND"],
                vec!["WrongMissing"],
            ),
            (
                "fail/36_mutation_conflict_wrong_kind",
                vec!["FAIL_MUTATION_CONFLICT_NOT_CONFLICT"],
                vec!["WrongConflict"],
            ),
        ];

        for (fixture, expected_codes, expected_spans) in fixtures {
            let path = repository_root()
                .join("tests/compile")
                .join(format!("{fixture}.jadpo"));
            let project = analyze_project(&path).expect("fixture should be analyzable");
            let codes = project
                .failures
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>();
            assert_eq!(codes, expected_codes, "{fixture}");

            let source = fs::read_to_string(&path).expect("fixture should be readable");
            let actual_spans = project
                .failures
                .diagnostics
                .iter()
                .map(|diagnostic| {
                    let span = diagnostic
                        .primary
                        .as_ref()
                        .expect("diagnostic needs a span");
                    &source[span.start..span.end]
                })
                .collect::<Vec<_>>();
            assert_eq!(actual_spans, expected_spans, "{fixture}");

            if fixture == "pass/08_automatic_not_found_mapping" {
                assert_eq!(project.failures.routes.len(), 1);
                assert_eq!(project.failures.routes[0].failure, "CustomerNotFound");
                assert_eq!(project.failures.routes[0].http_status, Some(404));
                assert!(project.failures.routes[0].derived);
            }
            if fixture == "pass/09_internal_context_not_public" {
                let contract = project
                    .failures
                    .contracts
                    .iter()
                    .find(|contract| contract.name == "CustomerNotFound")
                    .expect("failure contract should exist");
                assert!(contract.public_fields.is_empty());
                assert_eq!(contract.internal_fields, vec!["customer_id"]);
            }
        }
    }

    #[test]
    fn p106_fixtures_cover_failure_flow_paths_and_inline_routes() {
        for fixture in [
            "pass/59_p106_failure_route",
            "pass/60_p106_inline_action",
            "pass/61_distinct_failure_context",
            "pass/62_attempted_persistence",
            "pass/63_function_failure_propagation",
            "pass/64_multi_path_inline_route",
            "pass/65_authenticated_route_default",
            "pass/66_inline_action_failure_surface",
        ] {
            let path = repository_root()
                .join("tests/compile")
                .join(format!("{fixture}.jadpo"));
            let project = analyze_project(&path).expect("fixture should be analyzable");
            assert!(project.syntax.diagnostics().next().is_none(), "{fixture}");
            assert!(project.semantics.diagnostics.is_empty(), "{fixture}");
            assert!(project.typing.diagnostics.is_empty(), "{fixture}");
            assert!(project.failures.diagnostics.is_empty(), "{fixture}");
        }

        let cases = [
            (
                "fail/59_route_path_binding_mismatch",
                vec!["ROUTE_PATH_BINDING_MISSING", "ROUTE_PATH_BINDING_EXTRA"],
                "syntax",
            ),
            (
                "fail/60_fallible_call_requires_attempt",
                vec!["FAIL_ATTEMPT_REQUIRED"],
                "failure",
            ),
            (
                "fail/61_stale_fails_entry",
                vec!["FAIL_STALE_DECLARATION"],
                "failure",
            ),
            (
                "fail/62_route_behaviour_conflict",
                vec!["ROUTE_BEHAVIOUR_CONFLICT"],
                "syntax",
            ),
            (
                "fail/63_failure_context_overlap",
                vec!["FAIL_CONTEXT_FIELD_OVERLAP"],
                "failure",
            ),
            (
                "fail/64_persistence_requires_attempt",
                vec!["FAIL_ATTEMPT_REQUIRED"],
                "failure",
            ),
            (
                "fail/65_function_fallible_call_requires_attempt",
                vec!["FAIL_ATTEMPT_REQUIRED"],
                "failure",
            ),
            (
                "fail/66_inline_action_stale_fails",
                vec!["FAIL_STALE_DECLARATION"],
                "failure",
            ),
            (
                "fail/67_route_path_modifier_invalid",
                vec!["ROUTE_PATH_FIELD_MODIFIER_INVALID"],
                "syntax",
            ),
            (
                "fail/68_duplicate_fails_entry",
                vec!["FAIL_DUPLICATE_DECLARATION"],
                "failure",
            ),
            (
                "fail/69_duplicate_route_item",
                vec!["ROUTE_ITEM_DUPLICATE"],
                "syntax",
            ),
        ];
        for (fixture, expected, stage) in cases {
            let path = repository_root()
                .join("tests/compile")
                .join(format!("{fixture}.jadpo"));
            let project = analyze_project(&path).expect("fixture should be analyzable");
            let actual = if stage == "syntax" {
                project
                    .syntax
                    .diagnostics()
                    .map(|diagnostic| diagnostic.code)
                    .collect::<Vec<_>>()
            } else {
                project
                    .failures
                    .diagnostics
                    .iter()
                    .map(|diagnostic| diagnostic.code)
                    .collect::<Vec<_>>()
            };
            assert_eq!(actual, expected, "{fixture}");
        }

        let authenticated = analyze_project(
            &repository_root().join("tests/compile/pass/65_authenticated_route_default.jadpo"),
        )
        .expect("authenticated-default route should analyze");
        let route = authenticated.syntax.sources[0]
            .file
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                jadpo_syntax::Declaration::Route(route) => Some(route),
                _ => None,
            })
            .expect("fixture should contain a route");
        assert!(
            !route.public,
            "omitting `auth: none` must retain authentication"
        );
    }
}
