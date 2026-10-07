//! Closed, nonexecuting obligations. Candidate data is not a checked binding.
//! Only the eventual atomic core finish may issue authority or private nodes.
use jadpo_semantic::{DeliveryTypeCandidate, NodeKind, SemanticGraph};
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct DeliveryAuthorityCandidate {
    application: String,
    strategy: String,
    validator: String,
    credential_identity: String,
    credential_principal: String,
    verifier: String,
    credential_active: (String, String),
    expires: String,
    revoked: String,
    service_identity: String,
    resolution: String,
    resolution_authority: String,
    service_active: (String, String),
    principal_identity: String,
    membership: String,
    membership_identity: String,
    member: String,
    role_field: String,
    role: String,
    store: String,
}
impl DeliveryAuthorityCandidate {
    // Each argument is an independently resolved identity, not a source blob.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        application: String,
        strategy: String,
        validator: String,
        credential_identity: String,
        credential_principal: String,
        verifier: String,
        credential_active: (String, String),
        expires: String,
        revoked: String,
        service_identity: String,
        resolution: String,
        resolution_authority: String,
        service_active: (String, String),
        principal_identity: String,
        membership: String,
        membership_identity: String,
        member: String,
        role_field: String,
        role: String,
        store: String,
    ) -> Self {
        Self {
            application,
            strategy,
            validator,
            credential_identity,
            credential_principal,
            verifier,
            credential_active,
            expires,
            revoked,
            service_identity,
            resolution,
            resolution_authority,
            service_active,
            principal_identity,
            membership,
            membership_identity,
            member,
            role_field,
            role,
            store,
        }
    }
    fn facts(&self) -> Value {
        json!({
            "application": self.application, "validator": self.validator,
            "credential_identity": self.credential_identity,
            "credential_principal_reference": {"field": self.credential_principal, "target": self.service_identity},
            "credential_verifier": self.verifier,
            "credential_active": {"field": self.credential_active.0, "equals": self.credential_active.1},
            "credential_expires": self.expires, "credential_revoked": self.revoked,
            "service_identity": self.service_identity, "service_resolution": self.resolution,
            "resolution_authority": self.resolution_authority,
            "service_active": {"field": self.service_active.0, "equals": self.service_active.1},
            "principal_identity_mapping": {"source": self.service_identity, "target": self.principal_identity},
            "membership": self.membership, "membership_identity": self.membership_identity,
            "member_reference": {"field": self.member, "target": self.service_identity},
            "role_field": self.role_field, "role": self.role, "scope": "application",
            "authority_store": self.store,
        })
    }
}

/// Borrowed lowering identities, obtainable from a finished opaque binding.
/// This carries no mutable candidate, live authority or ordinary policy grant.
pub(crate) struct DeliveryAuthorityLowering<'a> {
    pub(crate) strategy: &'a str,
    pub(crate) credential_identity: &'a str,
    pub(crate) credential_principal: &'a str,
    pub(crate) verifier: &'a str,
    pub(crate) credential_active: (&'a str, &'a str),
    pub(crate) expires: &'a str,
    pub(crate) revoked: &'a str,
    pub(crate) service_identity: &'a str,
    pub(crate) resolution_authority: &'a str,
    pub(crate) service_active: (&'a str, &'a str),
    pub(crate) membership_identity: &'a str,
    pub(crate) member: &'a str,
    pub(crate) role_field: &'a str,
    pub(crate) role: &'a str,
}

/// Borrowed selected-recipient facts, never a generic read/update permission.
pub(crate) struct DeliverySelectionLowering<'a> {
    pub(crate) owner_active: (&'a str, &'a str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Selection,
    Admission,
    Dispatch,
    AcceptanceProof,
    Completion,
    Reconciliation,
}
impl Phase {
    fn name(self) -> &'static str {
        match self {
            Self::Selection => "selection",
            Self::Admission => "admission",
            Self::Dispatch => "dispatch",
            Self::AcceptanceProof => "acceptance_proof",
            Self::Reconciliation => "reconciliation",
            Self::Completion => "completion",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Requirement {
    NonNullStrictlyOverdueOpenUnsent,
    TodoVisible,
    OwnerActiveVisible,
    RecipientAuthorizedBeforeLimit,
    OrderedDueIdentityLimit500,
    KeysetContinuation,
    TransactionalUniqueRevisionIntent,
    ImmutablePayload,
    CurrentRevisionAndEligibility,
    ImmutableRecipientActiveAuthorized,
    LiveCredentialServiceMembershipRole,
    CredentialActive,
    CredentialNotExpired,
    CredentialNotRevoked,
    SelectedServiceActive,
    SelectedServiceIdentityResolved,
    SelectedMembershipPresentCurrent,
    SelectedRoleAuthorized,
    ImmutablePayloadComparison,
    ExactExternalOperation,
    SharedBudget,
    CommitFenceAndPossibleDispatchBeforeIo,
    AdmittedAuthorityOnly,
    ConsumeSharedAdmittedBudget,
    ProviderAttemptsTimeoutEgressOutcomes,
    NoTransactionAcrossIo,
    NoUnknownReplay,
    ExactKeyValidatedReceipt,
    RetainReceiptOnOriginalIntent,
    CurrentFence,
    OriginalRevisionEqualsCurrent,
    StillUnsent,
    CompilerObservationTimeOnly,
    CompletionTransaction,
    OpaqueCheckedAdapterAcceptedProof,
    InsertOnlyCanonicalProofSchema,
    ImmutableBindingContractAndClaimProfile,
    LiveOriginalFenceAtProofInsertAndPrecommitObservation,
    ProofOutsideProviderRetryCatch,
    ProofCommitAcknowledgedBeforePublication,
    MissingProofUnknownNoBackfill,
    PrivilegedPurposeCurrentOwner,
    ExactDurableAcceptedProof,
    ExpiredProofTerminalOnly,
    NoInvocationBudgetReset,
    ReconciliationTransaction,
}
impl Requirement {
    fn name(self) -> &'static str {
        match self {
            Self::NonNullStrictlyOverdueOpenUnsent => "non_null_strictly_overdue_open_unsent",
            Self::TodoVisible => "todo_visible",
            Self::OwnerActiveVisible => "owner_active_visible",
            Self::RecipientAuthorizedBeforeLimit => "recipient_authorized_before_limit",
            Self::OrderedDueIdentityLimit500 => "ordered_due_identity_limit_500",
            Self::KeysetContinuation => "due_identity_keyset_continuation",
            Self::TransactionalUniqueRevisionIntent => "transactional_unique_revision_intent",
            Self::ImmutablePayload => "immutable_payload",
            Self::CurrentRevisionAndEligibility => "current_revision_and_eligibility",
            Self::ImmutableRecipientActiveAuthorized => "immutable_recipient_active_authorized",
            Self::LiveCredentialServiceMembershipRole => "live_credential_service_membership_role",
            Self::CredentialActive => "credential_active",
            Self::CredentialNotExpired => "credential_not_expired",
            Self::CredentialNotRevoked => "credential_not_revoked",
            Self::SelectedServiceActive => "selected_service_active",
            Self::SelectedServiceIdentityResolved => "selected_service_identity_resolved",
            Self::SelectedMembershipPresentCurrent => {
                "selected_application_membership_present_current"
            }
            Self::SelectedRoleAuthorized => "selected_role_equals_and_authorizes_operation",
            Self::ImmutablePayloadComparison => "payload_equals_enrolled_immutable_payload",
            Self::ExactExternalOperation => "exact_external_operation",
            Self::SharedBudget => "remaining_shared_budget",
            Self::CommitFenceAndPossibleDispatchBeforeIo => {
                "commit_fence_and_possible_dispatch_before_io"
            }
            Self::AdmittedAuthorityOnly => "admitted_authority_only_no_extra_live_cut",
            Self::ConsumeSharedAdmittedBudget => "consume_shared_admitted_budget_without_reset",
            Self::ProviderAttemptsTimeoutEgressOutcomes => {
                "frozen_provider_attempts_timeout_egress_outcomes"
            }
            Self::NoTransactionAcrossIo => "no_transaction_across_provider_io",
            Self::NoUnknownReplay => "no_automatic_unknown_replay",
            Self::ExactKeyValidatedReceipt => "exact_key_validated_receipt",
            Self::RetainReceiptOnOriginalIntent => "retain_receipt_on_original_intent",
            Self::CurrentFence => "current_fencing_generation",
            Self::OriginalRevisionEqualsCurrent => "original_schedule_revision_equals_current",
            Self::StillUnsent => "todo_still_unsent",
            Self::CompilerObservationTimeOnly => "compiler_receipt_observation_time_only",
            Self::CompletionTransaction => "same_completion_transaction",
            Self::OpaqueCheckedAdapterAcceptedProof => "opaque_checked_adapter_accepted_proof",
            Self::InsertOnlyCanonicalProofSchema => "insert_only_canonical_proof_schema",
            Self::ImmutableBindingContractAndClaimProfile => {
                "immutable_binding_contract_claim_payload_profile"
            }
            Self::LiveOriginalFenceAtProofInsertAndPrecommitObservation => "live_original_fence_at_proof_insert_and_precommit_observation",
            Self::ProofOutsideProviderRetryCatch => "proof_storage_outside_provider_retry_catch",
            Self::ProofCommitAcknowledgedBeforePublication => {
                "proof_commit_acknowledged_before_publication"
            }
            Self::MissingProofUnknownNoBackfill => "missing_or_uncertain_proof_unknown_no_backfill",
            Self::PrivilegedPurposeCurrentOwner => {
                "privileged_reconciliation_purpose_current_owner"
            }
            Self::ExactDurableAcceptedProof => "exact_durable_genuine_accepted_proof",
            Self::ExpiredProofTerminalOnly => "elapsed_original_caps_terminal_bookkeeping_only",
            Self::NoInvocationBudgetReset => "no_new_invocation_renewal_budget_reset",
            Self::ReconciliationTransaction => {
                "receipt_state_matching_revision_sent_atomic_reconciliation"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
// Canonical target-name order; never allocation or authored source order.
enum PrivateTarget {
    AcceptedProof,
    Fence,
    InvocationBudget,
    Receipt,
    Intent,
    ScheduleRevision,
}
impl PrivateTarget {
    fn name(self) -> &'static str {
        match self {
            Self::Intent => "revision_intent",
            Self::Fence => "claim_fence",
            Self::Receipt => "receipt",
            Self::ScheduleRevision => "todo_schedule_revision",
            Self::InvocationBudget => "invocation_budget",
            Self::AcceptedProof => "accepted_proof",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct PhaseRow {
    phase: Phase,
    reads: BTreeSet<String>,
    predicates: BTreeSet<String>,
    writes: BTreeSet<String>,
    private_reads: BTreeSet<PrivateTarget>,
    private_writes: BTreeSet<PrivateTarget>,
    invokes: Option<String>,
    requirements: Vec<Requirement>,
}

/// Typed requests for the atomic core graph resolver, never authored edges.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum DeliveryTargetKind {
    Read,
    Predicate,
    Write,
    PrivateRead,
    PrivateWrite,
    Invoke,
    Validator,
    Membership,
    Role,
    ServicePrincipal,
}
impl DeliveryTargetKind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Predicate => "predicate",
            Self::Write => "write",
            Self::PrivateRead => "private_read",
            Self::PrivateWrite => "private_write",
            Self::Invoke => "invoke",
            Self::Validator => "selected_validator",
            Self::Membership => "selected_membership",
            Self::Role => "selected_role",
            Self::ServicePrincipal => "selected_service_principal",
        }
    }
}
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct DeliveryTargetCandidate {
    pub(crate) phase: &'static str,
    pub(crate) kind: DeliveryTargetKind,
    pub(crate) target: String,
}

/// Private construction and explicitly candidate-only output. No live grants,
/// ordinary policy effects, graph nodes or checked schedules are issued here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeliveryPhaseCandidate {
    job: String,
    binding: String,
    authority: DeliveryAuthorityCandidate,
    rows: [PhaseRow; 6],
    open: (String, String),
    order: [String; 2],
    selection: String,
    completion: String,
    operation: String,
    payload: [(String, String); 5],
    owner_active: (String, String),
}
impl DeliveryPhaseCandidate {
    pub(crate) fn selection_for_lowering(&self) -> DeliverySelectionLowering<'_> {
        DeliverySelectionLowering {
            owner_active: (&self.owner_active.0, &self.owner_active.1),
        }
    }
    pub(crate) fn authority_for_lowering(&self) -> DeliveryAuthorityLowering<'_> {
        let a = &self.authority;
        DeliveryAuthorityLowering {
            strategy: &a.strategy,
            credential_identity: &a.credential_identity,
            credential_principal: &a.credential_principal,
            verifier: &a.verifier,
            credential_active: (&a.credential_active.0, &a.credential_active.1),
            expires: &a.expires,
            revoked: &a.revoked,
            service_identity: &a.service_identity,
            resolution_authority: &a.resolution_authority,
            service_active: (&a.service_active.0, &a.service_active.1),
            membership_identity: &a.membership_identity,
            member: &a.member,
            role_field: &a.role_field,
            role: &a.role,
        }
    }
    pub(crate) fn graph_target_candidates(&self) -> Vec<DeliveryTargetCandidate> {
        let mut targets = Vec::new();
        for row in &self.rows {
            let mut add = |kind, target| {
                targets.push(DeliveryTargetCandidate {
                    phase: row.phase.name(),
                    kind,
                    target,
                })
            };
            for (kind, fields) in [
                (DeliveryTargetKind::Read, &row.reads),
                (DeliveryTargetKind::Predicate, &row.predicates),
                (DeliveryTargetKind::Write, &row.writes),
            ] {
                for field in fields {
                    add(kind, field.clone());
                }
            }
            for (kind, fields) in [
                (DeliveryTargetKind::PrivateRead, &row.private_reads),
                (DeliveryTargetKind::PrivateWrite, &row.private_writes),
            ] {
                for field in fields {
                    add(kind, format!("{}.{}", self.binding, field.name()));
                }
            }
            if let Some(operation) = &row.invokes {
                add(DeliveryTargetKind::Invoke, operation.clone());
            }
            for (kind, target) in [
                (
                    DeliveryTargetKind::Validator,
                    self.authority.validator.clone(),
                ),
                (
                    DeliveryTargetKind::Membership,
                    self.authority.membership.clone(),
                ),
                (DeliveryTargetKind::Role, self.authority.role.clone()),
                (
                    DeliveryTargetKind::ServicePrincipal,
                    self.authority
                        .principal_identity
                        .rsplit_once('.')
                        .map(|(principal, _)| principal.to_owned())
                        .unwrap_or_default(),
                ),
            ] {
                add(kind, target);
            }
        }
        targets.sort();
        targets
    }
    pub(crate) fn job_candidate(&self) -> &str {
        &self.job
    }
    pub(crate) fn binding_candidate(&self) -> &str {
        &self.binding
    }
    pub(super) fn resolve(
        types: &DeliveryTypeCandidate,
        authority: DeliveryAuthorityCandidate,
        owner_active: (String, String),
        graph: &SemanticGraph,
    ) -> Option<Self> {
        let d = types.descriptor_candidate();
        let field = |path: &jadpo_syntax::NameExpression| {
            let name = path
                .path
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>()
                .join(".");
            (graph.node(&name)?.kind == NodeKind::Field).then_some(name)
        };
        let identity = field(&d.selection.identity)?;
        let due = field(&d.selection.due)?;
        let sent = field(&d.selection.unsent)?;
        let status = field(&d.selection.open_field)?;
        let title = field(&d.service.payload.todo_title)?;
        let relationship = d
            .selection
            .required_owner
            .path
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join(".");
        if graph.node(&relationship)?.kind != NodeKind::Relationship {
            return None;
        }
        let email = format!("{}.email", d.selection.owner_visible.text);
        if graph.node(&email)?.kind != NodeKind::Field {
            return None;
        }
        let predicates = [
            format!("{}.lifecycle.visibility", d.selection.entity.text),
            format!("{}.lifecycle.visibility", d.selection.owner_visible.text),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        if !predicates.iter().all(|name| {
            graph
                .node(name)
                .is_some_and(|node| node.kind == NodeKind::LifecycleVisibility)
        }) {
            return None;
        }
        let operation = types.service_operation_candidate().to_owned();
        if graph.node(&operation)?.kind != NodeKind::ServiceOperation {
            return None;
        }
        let reads = [
            identity.clone(),
            due.clone(),
            sent.clone(),
            status.clone(),
            title,
            relationship,
            email.clone(),
            owner_active.0.clone(),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        let authority_reads = [
            authority.credential_identity.clone(),
            authority.credential_principal.clone(),
            authority.verifier.clone(),
            authority.credential_active.0.clone(),
            authority.expires.clone(),
            authority.revoked.clone(),
            authority.service_identity.clone(),
            authority.resolution_authority.clone(),
            authority.service_active.0.clone(),
            authority.membership_identity.clone(),
            authority.member.clone(),
            authority.role_field.clone(),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        let rows = [
            PhaseRow {
                phase: Phase::Selection,
                reads: reads.clone(),
                predicates: predicates.clone(),
                writes: BTreeSet::new(),
                private_reads: [PrivateTarget::Intent, PrivateTarget::ScheduleRevision]
                    .into_iter()
                    .collect(),
                private_writes: [PrivateTarget::Intent].into_iter().collect(),
                invokes: None,
                requirements: vec![
                    Requirement::NonNullStrictlyOverdueOpenUnsent,
                    Requirement::TodoVisible,
                    Requirement::OwnerActiveVisible,
                    Requirement::RecipientAuthorizedBeforeLimit,
                    Requirement::OrderedDueIdentityLimit500,
                    Requirement::KeysetContinuation,
                    Requirement::TransactionalUniqueRevisionIntent,
                    Requirement::ImmutablePayload,
                ],
            },
            PhaseRow {
                phase: Phase::Admission,
                reads: reads.union(&authority_reads).cloned().collect(),
                predicates,
                writes: BTreeSet::new(),
                private_reads: [
                    PrivateTarget::Intent,
                    PrivateTarget::Fence,
                    PrivateTarget::ScheduleRevision,
                    PrivateTarget::InvocationBudget,
                ]
                .into_iter()
                .collect(),
                private_writes: [
                    PrivateTarget::Intent,
                    PrivateTarget::Fence,
                    PrivateTarget::InvocationBudget,
                ]
                .into_iter()
                .collect(),
                invokes: None,
                requirements: vec![
                    Requirement::CurrentRevisionAndEligibility,
                    Requirement::ImmutableRecipientActiveAuthorized,
                    Requirement::LiveCredentialServiceMembershipRole,
                    Requirement::CredentialActive,
                    Requirement::CredentialNotExpired,
                    Requirement::CredentialNotRevoked,
                    Requirement::SelectedServiceActive,
                    Requirement::SelectedServiceIdentityResolved,
                    Requirement::SelectedMembershipPresentCurrent,
                    Requirement::SelectedRoleAuthorized,
                    Requirement::ImmutablePayloadComparison,
                    Requirement::ExactExternalOperation,
                    Requirement::SharedBudget,
                    Requirement::CommitFenceAndPossibleDispatchBeforeIo,
                ],
            },
            PhaseRow {
                phase: Phase::Dispatch,
                reads: BTreeSet::new(),
                predicates: BTreeSet::new(),
                writes: BTreeSet::new(),
                private_reads: [
                    PrivateTarget::Intent,
                    PrivateTarget::Fence,
                    PrivateTarget::InvocationBudget,
                ]
                .into_iter()
                .collect(),
                private_writes: BTreeSet::new(),
                invokes: Some(operation.clone()),
                requirements: vec![
                    Requirement::AdmittedAuthorityOnly,
                    Requirement::ConsumeSharedAdmittedBudget,
                    Requirement::ProviderAttemptsTimeoutEgressOutcomes,
                    Requirement::NoTransactionAcrossIo,
                    Requirement::NoUnknownReplay,
                ],
            },
            PhaseRow {
                phase: Phase::AcceptanceProof,
                reads: [identity.clone()].into_iter().collect(),
                predicates: BTreeSet::new(),
                writes: BTreeSet::new(),
                private_reads: [
                    PrivateTarget::Intent,
                    PrivateTarget::Fence,
                    PrivateTarget::InvocationBudget,
                ]
                .into_iter()
                .collect(),
                private_writes: [PrivateTarget::AcceptedProof].into_iter().collect(),
                invokes: None,
                requirements: vec![
                    Requirement::OpaqueCheckedAdapterAcceptedProof,
                    Requirement::InsertOnlyCanonicalProofSchema,
                    Requirement::ImmutableBindingContractAndClaimProfile,
                    Requirement::LiveOriginalFenceAtProofInsertAndPrecommitObservation,
                    Requirement::ProofOutsideProviderRetryCatch,
                    Requirement::ProofCommitAcknowledgedBeforePublication,
                    Requirement::MissingProofUnknownNoBackfill,
                    Requirement::AdmittedAuthorityOnly,
                ],
            },
            PhaseRow {
                phase: Phase::Completion,
                reads: [identity.clone(), sent.clone()].into_iter().collect(),
                predicates: BTreeSet::new(),
                writes: [sent.clone()].into_iter().collect(),
                private_reads: [
                    PrivateTarget::Intent,
                    PrivateTarget::Fence,
                    PrivateTarget::Receipt,
                    PrivateTarget::ScheduleRevision,
                ]
                .into_iter()
                .collect(),
                private_writes: [PrivateTarget::Intent, PrivateTarget::Receipt]
                    .into_iter()
                    .collect(),
                invokes: None,
                requirements: vec![
                    Requirement::ExactKeyValidatedReceipt,
                    Requirement::RetainReceiptOnOriginalIntent,
                    Requirement::CurrentFence,
                    Requirement::OriginalRevisionEqualsCurrent,
                    Requirement::StillUnsent,
                    Requirement::CompilerObservationTimeOnly,
                    Requirement::CompletionTransaction,
                ],
            },
            PhaseRow {
                phase: Phase::Reconciliation,
                reads: [identity.clone(), sent.clone()].into_iter().collect(),
                predicates: BTreeSet::new(),
                writes: [sent].into_iter().collect(),
                private_reads: [
                    PrivateTarget::AcceptedProof,
                    PrivateTarget::Intent,
                    PrivateTarget::Fence,
                    PrivateTarget::InvocationBudget,
                    PrivateTarget::Receipt,
                    PrivateTarget::ScheduleRevision,
                ]
                .into_iter()
                .collect(),
                private_writes: [PrivateTarget::Intent, PrivateTarget::Receipt]
                    .into_iter()
                    .collect(),
                invokes: None,
                requirements: vec![
                    Requirement::PrivilegedPurposeCurrentOwner,
                    Requirement::ExactDurableAcceptedProof,
                    Requirement::ImmutableBindingContractAndClaimProfile,
                    Requirement::ExpiredProofTerminalOnly,
                    Requirement::NoInvocationBudgetReset,
                    Requirement::MissingProofUnknownNoBackfill,
                    Requirement::OriginalRevisionEqualsCurrent,
                    Requirement::StillUnsent,
                    Requirement::CompilerObservationTimeOnly,
                    Requirement::AdmittedAuthorityOnly,
                    Requirement::ReconciliationTransaction,
                ],
            },
        ];
        let name = |p: &jadpo_syntax::NameExpression| {
            p.path
                .iter()
                .map(|part| part.text.as_str())
                .collect::<Vec<_>>()
                .join(".")
        };
        let job = types.schedule_candidate().job.clone();
        Some(Self {
            binding: format!("job.{job}.delivery.reminder_v1"),
            job,
            authority,
            rows,
            open: (status, name(&d.selection.open_variant)),
            order: [due, identity],
            selection: d.selection.name.text.clone(),
            completion: d.completion.name.text.clone(),
            operation,
            payload: [
                ("idempotency_key".into(), "generated_intent".into()),
                ("from".into(), name(&d.service.payload.from)),
                ("to".into(), email),
                ("todo_title".into(), name(&d.service.payload.todo_title)),
                ("due_at".into(), name(&d.service.payload.due_at)),
            ],
            owner_active,
        })
    }
    // Slot-level predicates are bound to resolved authority, never to runtime
    // values. They remain unproved; no predicate implies a new live cut.
    fn predicate_candidate(&self, requirement: Requirement) -> Option<Value> {
        let authority = &self.authority;
        let (targets, additional) = match requirement {
            Requirement::CredentialActive => (
                vec![authority.credential_active.0.clone()],
                json!({"equals": authority.credential_active.1}),
            ),
            Requirement::CredentialNotExpired => (vec![authority.expires.clone()], json!({})),
            Requirement::CredentialNotRevoked => (vec![authority.revoked.clone()], json!({})),
            Requirement::SelectedServiceActive => (
                vec![authority.service_active.0.clone()],
                json!({"equals": authority.service_active.1}),
            ),
            Requirement::SelectedServiceIdentityResolved => (
                vec![
                    authority.credential_principal.clone(),
                    authority.service_identity.clone(),
                    authority.principal_identity.clone(),
                ],
                json!({}),
            ),
            Requirement::SelectedMembershipPresentCurrent => (
                vec![
                    authority.membership_identity.clone(),
                    authority.member.clone(),
                    authority.service_identity.clone(),
                ],
                json!({"scope":"application", "store": authority.store}),
            ),
            Requirement::SelectedRoleAuthorized => (
                vec![authority.role_field.clone(), self.operation.clone()],
                json!({"equals": authority.role}),
            ),
            Requirement::ImmutablePayloadComparison => (
                vec![format!("{}.{}", self.binding, PrivateTarget::Intent.name())],
                json!({}),
            ),
            Requirement::ConsumeSharedAdmittedBudget => (
                vec![format!(
                    "{}.{}",
                    self.binding,
                    PrivateTarget::InvocationBudget.name()
                )],
                json!({}),
            ),
            _ => return None,
        };
        let mut fact =
            json!({"requirement": requirement.name(), "targets": targets, "established": false});
        fact.as_object_mut()?
            .extend(additional.as_object()?.clone());
        Some(fact)
    }
    /// Inspection of incomplete source obligations only, never checked export.
    pub fn candidate_facts(&self) -> Value {
        let mut facts = self.obligation_source_facts();
        facts["status"] = json!("incomplete_nonexecuting_candidate");
        facts
    }
    // No checked-authority label: the opaque core binding owns that context.
    pub(crate) fn obligation_source_facts(&self) -> Value {
        let authority = self.authority.facts();
        json!({"contract_version":2, "job": self.job, "binding": self.binding,
            "authority": authority, "selection": self.selection, "completion": self.completion,
            "operation": self.operation, "open": {"field": self.open.0, "variant": self.open.1},
            "order": self.order, "limit": 500, "payload_version": "reminder.v1",
            "owner_active": {"field": self.owner_active.0, "equals": self.owner_active.1},
            "payload": self.payload.iter().map(|(key, source)| (key.clone(), json!(source))).collect::<serde_json::Map<_,_>>(),
            "phases": self.rows.iter().map(|row| json!({
                "job": self.job, "binding": self.binding, "phase": row.phase.name(), "authority": authority,
                "read_targets": row.reads, "predicate_targets": row.predicates, "write_targets": row.writes,
                "private_read_targets": row.private_reads.iter().map(|target| format!("{}.{}", self.binding, target.name())).collect::<Vec<_>>(),
                "private_write_targets": row.private_writes.iter().map(|target| format!("{}.{}", self.binding, target.name())).collect::<Vec<_>>(),
                "invoke_target": row.invokes, "unproved_requirements": row.requirements.iter().map(|r| r.name()).collect::<Vec<_>>(),
                "unproved_predicates": row.requirements.iter().filter_map(|r| self.predicate_candidate(*r)).collect::<Vec<_>>(),
                "live_authority_established": false,
            })).collect::<Vec<_>>(), "runtime_supported": false, "execution_profile": null})
    }
}
