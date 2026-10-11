//! Bound admission and completion; owning COMMIT alone issues an invocation.
//! These native components do not activate an authored job or choose its profile.
use super::{line, snake_case, sql_identifier, ts_string, type_name, TargetGenerator};
use jadpo_syntax::NameExpression;

fn last(path: &NameExpression) -> &str {
    &path.path.last().expect("finished field path").text
}
fn variant(path: &str) -> &str {
    path.rsplit_once('.').expect("finished enum variant").1
}

impl TargetGenerator<'_> {
    pub(super) fn delivery_invocation_bridges(&self, output: &mut String) {
        if self.project.delivery_model().bindings().is_empty() {
            return;
        }
        // Neither storage facts nor a released nested savepoint can mint this
        // token. Only the closed, root-client owning transaction below does so.
        line(
            output,
            "const deliveryInvocations = new WeakMap<object, any>();",
        );
        line(
            output,
            "const deliveryOutcomes = new WeakMap<object, any>();",
        );
        line(output, "async function prepareDeliveryInvocationInternal(binding: string, proof: unknown, intentId: string, at: unknown, profile: Readonly<{ executionMs: number; leaseMs: number }>, operatorGrant: unknown | null) {");
        line(
            output,
            "  const operationTime = validateInstant(at, \"delivery.operationTime\");",
        );
        line(output, "  if (profile === null || typeof profile !== \"object\" || Reflect.ownKeys(profile).sort().join(\",\") !== \"executionMs,leaseMs\" || !Number.isSafeInteger(profile.executionMs) || !Number.isSafeInteger(profile.leaseMs) || profile.executionMs <= 0 || profile.leaseMs <= 0 || profile.leaseMs > profile.executionMs) invalid(\"delivery.profile\", \"an explicit finite execution/lease profile\");");
        line(output, "  readCurrentDeliveryCredentialProof(proof); if (operatorGrant !== null) readCurrentDeliveryOperatorGrant(operatorGrant);");
        line(output, "  let credential: string; switch (binding) {");
        for binding in self.project.delivery_model().bindings() {
            let effect = binding.service_effect_for_lowering();
            let credential = effect
                .credential_slot
                .strip_prefix("config.")
                .expect("checked credential slot");
            line(
                output,
                &format!(
                    "    case {}: credential = configuration.{credential}; break;",
                    ts_string(binding.binding_identity())
                ),
            );
        }
        line(
            output,
            "    default: throw new AuthenticationFault(\"authority_invariant\", 500);",
        );
        line(output, "  }");
        // Explicit profile values are a native component input only; no public
        // default or owner approval is inferred. The approved cumulative limits
        // cannot be reset by a later invocation.
        line(output, "  const limits = Object.freeze({ maxInvocations: 3, lifetimeMs: 3_600_000, executionMs: profile.executionMs, leaseMs: profile.leaseMs });");
        line(output, "  const started = performance.now();");
        line(output, "  const admissionPersistence = operatorGrant === null ? rootPersistence : rootPersistence.withDeadline(started + profile.executionMs); const prepared = await admissionPersistence.withOperationTime(operationTime).transaction((tx: any) => tx.prepare_delivery_invocation(binding, proof, intentId, limits, operatorGrant));");
        line(output, "  if (prepared === null) return null; if (operatorGrant !== null) { readCurrentDeliveryOperatorGrant(operatorGrant); readCurrentDeliveryCredentialProof(proof); }");
        line(output, "  const token = Object.freeze({});");
        // databaseRemainingMs was sampled under the authoritative database
        // clock. Subtract the ENTIRE monotonic admission elapsed time; a slow
        // COMMIT can never extend a database lease by assuming host UTC agrees.
        line(output, "  deliveryInvocations.set(token, { binding, prepared, credential, deadlineAt: started + prepared.databaseRemainingMs, operatorGrant, proof, used: false });");
        line(output, "  return token;");
        line(output, "}");
        line(output, "export function prepareDeliveryInvocation(binding: string, proof: unknown, intentId: string, at: unknown, profile: Readonly<{ executionMs: number; leaseMs: number }>) { return prepareDeliveryInvocationInternal(binding, proof, intentId, at, profile, null); }");
        line(
            output,
            "export async function dispatchDeliveryInvocation(token: unknown) {",
        );
        line(output, "  const invocation = typeof token === \"object\" && token !== null ? deliveryInvocations.get(token) : undefined;");
        line(output, "  if (invocation === undefined || invocation.used) throw new AuthenticationFault(\"invalid_credentials\", 401);");
        line(output, "  if (invocation.operatorGrant !== null) { readCurrentDeliveryOperatorGrant(invocation.operatorGrant); readCurrentDeliveryCredentialProof(invocation.proof); } invocation.used = true;");
        line(
            output,
            "  const base = captureOperation(null, null, undefined, false);",
        );
        line(
            output,
            "  const context = Object.freeze({ ...base, deadlineAt: invocation.deadlineAt });",
        );
        line(output, "  let outcome: any;");
        line(output, "  try {");
        line(output, "    assertOperationDeadline(context);");
        line(
            output,
            "    let receipt: any; switch (invocation.binding) {",
        );
        for binding in self.project.delivery_model().bindings() {
            let d = binding.descriptor();
            let operation = d
                .service
                .operation
                .path
                .iter()
                .map(|p| p.text.as_str())
                .collect::<Vec<_>>()
                .join("__");
            line(output, &format!("      case {}: receipt = await __service_{operation}(invocation.prepared.payload as {}, context, invocation.credential); break;", ts_string(binding.binding_identity()), d.service.input.text));
        }
        line(
            output,
            "      default: throw new AuthenticationFault(\"authority_invariant\", 500);",
        );
        line(output, "    }");
        line(
            output,
            "    outcome = { kind: \"accepted\", receipt, observedAt: new Date().toISOString() }; ",
        );
        line(output, "  } catch (error) {");
        // Only exact checked adapter / declared failures establish no effect.
        // No raw error text or arbitrary caller classification enters storage.
        line(output, "    let noEffect: string | null = error instanceof RequestDeadlineFault || error instanceof ServiceAdapterFault && [\"unavailable\", \"deadline_exceeded\"].includes(error.kind) ? \"service_unavailable\" : null;");
        line(
            output,
            "    if (error instanceof DomainFailure) { switch (invocation.binding) {",
        );
        for binding in self.project.delivery_model().bindings() {
            let effect = binding.service_effect_for_lowering();
            let rejected = effect
                .outcome_mappings
                .iter()
                .find(|(s, _)| s == "provider.invalid_recipient")
                .expect("checked mapping")
                .1
                .as_str();
            let unavailable = effect
                .outcome_mappings
                .iter()
                .find(|(s, _)| s == "provider.rate_limited")
                .expect("checked mapping")
                .1
                .as_str();
            line(output, &format!("      case {}: if (error.failureName === {}) noEffect = \"recipient_rejected\"; else if (error.failureName === {}) noEffect = \"service_unavailable\"; break;", ts_string(binding.binding_identity()), ts_string(rejected), ts_string(unavailable)));
        }
        line(output, "    } }");
        line(output, "    const retryDelayMs = noEffect === \"service_unavailable\" ? Math.floor(Math.random() * Math.min(30_000, 1000 * 2 ** invocation.prepared.handle.invocation)) : 0;");
        line(output, "    outcome = noEffect === null ? { kind: \"unknown\" } : { kind: \"no_effect\", value: { failureClass: noEffect, providerAttempts: context.serviceAttemptBudget.attemptsUsed, retryDelayMs } };");
        line(output, "  }");
        line(output, "  const receiptToken = Object.freeze({}); deliveryOutcomes.set(receiptToken, { invocation, outcome }); return receiptToken;");
        line(output, "}");
        line(
            output,
            "export async function completeDeliveryInvocation(token: unknown) {",
        );
        line(output, "  const result = typeof token === \"object\" && token !== null ? deliveryOutcomes.get(token) : undefined;");
        line(output, "  if (result === undefined) throw new AuthenticationFault(\"invalid_credentials\", 401);");
        line(
            output,
            "  const context = captureOperation(null, null, undefined, false);",
        );
        line(output, "  const completed = await rootPersistence.withOperationTime(context.now).transaction((tx: any) => tx.complete_delivery_invocation(result.invocation.binding, result.invocation.prepared.handle, result.outcome));");
        line(
            output,
            "  deliveryOutcomes.delete(token as object); return completed;",
        );
        line(output, "}");
    }

    pub(super) fn delivery_invocation_methods(&self) -> String {
        let mut output = String::new();
        if self.project.delivery_model().bindings().is_empty() {
            return output;
        }
        line(&mut output, "async prepare_delivery_invocation(binding: string, proof: unknown, intentId: string, limits: DeliveryClaimLimits, operatorGrant: unknown | null = null) {");
        line(&mut output, "  requireTransaction(); if (operationTime === null) return fault(\"delivery.operation_time\");");
        line(&mut output, "  if (operatorGrant !== null) readCurrentDeliveryOperatorGrant(operatorGrant); return client.transaction(async () => {");
        line(
            &mut output,
            "    if (!await client.check_delivery_authority(binding, proof)) return null;",
        );
        line(&mut output, "    const intent = await client.read_delivery_intent(intentId); if (intent === null) return null;");
        line(&mut output, "    switch (binding) {");
        for binding in self.project.delivery_model().bindings() {
            let d = binding.descriptor();
            let s = &d.selection;
            let phase = binding.selection_for_lowering();
            let todo = self.records.get(&s.entity.text).expect("finished source");
            let relationship = last(&s.required_owner);
            let (fk, reference) = todo
                .fields
                .iter()
                .find_map(|f| {
                    let r = f.reference.as_ref()?;
                    (r.relationship.as_ref()?.text == relationship)
                        .then_some((f.name.text.as_str(), r))
                })
                .expect("checked owner FK");
            let owner_target = type_name(&reference.target);
            let (owner_entity, owner_id) =
                owner_target.split_once('.').expect("checked owner target");
            let col = |alias: &str, field: &str| {
                format!("{alias}.{}", sql_identifier(&snake_case(field)))
            };
            let owner_active = phase
                .owner_active
                .0
                .rsplit_once('.')
                .expect("checked owner active")
                .1;
            line(
                &mut output,
                &format!("      case {}: {{", ts_string(binding.binding_identity())),
            );
            line(&mut output, &format!("        if (intent.job !== {} || intent.sourceOperation !== {} || intent.sourceEntity !== {} || intent.orderingKey !== intent.sourceEntityId || intent.payloadVersion !== \"reminder.v1\") return fault(\"delivery.binding_intent\");", ts_string(&binding.schedule().job), ts_string(&format!("{}.selection", binding.binding_identity())), ts_string(&s.entity.text)));
            let (Some(todo_visible), Some(owner_visible)) = (
                self.lifecycle_visibility_sql(&s.visible.text),
                self.lifecycle_visibility_sql(&s.owner_visible.text),
            ) else {
                line(
                    &mut output,
                    "        return fault(\"delivery.visibility_unsupported\");",
                );
                line(&mut output, "      }");
                continue;
            };
            // Match selection's authority -> source/owner -> ordering-key lock
            // order. Completion takes source first too, avoiding key/source inversion.
            let query = format!("SELECT {} AS recipient FROM (SELECT {} FROM {} WHERE {todo_visible}) t JOIN (SELECT {} FROM {} WHERE {owner_visible}) u ON {} = {} WHERE {} = $1 AND {} IS NOT NULL AND {} < $2 AND {} = $3 AND {} IS NULL AND {} = $4", col("u", last(&d.service.payload.to)), [last(&s.identity), last(&s.due), last(&s.open_field), last(&s.unsent), fk].iter().map(|f|sql_identifier(&snake_case(f))).collect::<Vec<_>>().join(", "),sql_identifier(&snake_case(&s.entity.text)),[owner_id,last(&d.service.payload.to),owner_active].iter().map(|f|sql_identifier(&snake_case(f))).collect::<Vec<_>>().join(", "),sql_identifier(&snake_case(owner_entity)),col("t",fk),col("u",owner_id),col("t",last(&s.identity)),col("t",last(&s.due)),col("t",last(&s.due)),col("t",last(&s.open_field)),col("t",last(&s.unsent)),col("u",owner_active));
            line(&mut output, &format!("        const sources = await execute(\"delivery.admission.source\", {} + (postgres !== null ? \" FOR SHARE OF t, u\" : \"\"), [intent.sourceEntityId, persistenceInstant(operationTime, postgres !== null), {}, {}]);", ts_string(&query), ts_string(last(&s.open_variant)), ts_string(variant(phase.owner_active.1))));
            line(
                &mut output,
                "        if (sources.length > 1) return fault(\"delivery.source_cardinality\");",
            );
            line(&mut output, &format!("        const revision = sources.length === 0 ? null : await client.read_delivery_schedule_revision({}, intent.sourceEntityId);", ts_string(&s.entity.text)));
            line(&mut output, "        const payload = validateDeliveryPayload(binding, intent.payload); if (payload.idempotency_key !== intent.intentId) return fault(\"delivery.payload_identity\");");
            line(&mut output, "        if (revision !== intent.sourceRevision || sources.length !== 1 || sources[0].recipient !== payload.to) { if (operatorGrant !== null) { readCurrentDeliveryCredentialProof(proof); readCurrentDeliveryOperatorGrant(operatorGrant); return null; } await client.cancel_delivery_intent(intentId); return null; }");
            // A newer, currently admitted revision supersedes older same-source
            // pending/retry-wait records that cannot have had a provider effect.
            // Cancel under the normal key lock before FIFO claim; unknown/running
            // predecessors remain active and continue to block the successor.
            line(&mut output, "        if (operatorGrant === null) { const stale = await execute(\"delivery.admission.stale_predecessors\", `SELECT p.\"intent_id\" FROM \"__jadpo_deliveries_v1\" p JOIN \"__jadpo_deliveries_v1\" c ON c.\"intent_id\" = $1 WHERE p.\"job\" = c.\"job\" AND p.\"source_operation\" = c.\"source_operation\" AND p.\"source_entity\" = c.\"source_entity\" AND p.\"source_entity_id\" = c.\"source_entity_id\" AND p.\"ordering_key\" = c.\"ordering_key\" AND p.\"key_sequence\" < c.\"key_sequence\" AND p.\"source_revision\" <> $2 AND p.\"state\" IN ('pending', 'retry_wait') ORDER BY p.\"key_sequence\" LIMIT 500`, [intentId, revision]); for (const predecessor of stale) await client.cancel_delivery_intent(predecessor.intent_id); }");
            line(&mut output, "        const handle = operatorGrant === null ? await client.claim_delivery_intent(intentId, limits) : await client.claim_delivery_resolved_replay(intentId, limits); if (handle === null) return null;");
            // Recheck crypto issuer after all awaited lock/claim work but before
            // committing the checkpoint. Live SQL role rows remain locked.
            line(
                &mut output,
                "        if (!await client.check_delivery_authority(binding, proof)) return null;",
            );
            line(
                &mut output,
                "        if (!await client.checkpoint_delivery_dispatch(handle)) return null;",
            );
            line(&mut output, "        const clockSql = postgres !== null ? \"to_char(clock_timestamp() AT TIME ZONE 'UTC', 'YYYY-MM-DD\\\"T\\\"HH24:MI:SS.MS\\\"Z\\\"')\" : \"strftime('%Y-%m-%dT%H:%M:%fZ', 'now')\";");
            line(&mut output, "        const clocks = await execute(\"delivery.admission.time\", `SELECT ${clockSql} AS now`); if (clocks.length !== 1) return fault(\"delivery.admission.time\");");
            line(&mut output, "        const databaseRemainingMs = Math.min(Date.parse(handle.leaseUntil), Date.parse(handle.executionDeadline), Date.parse(handle.lifetimeDeadline)) - Date.parse(clocks[0].now);");
            line(
                &mut output,
                "        readCurrentDeliveryCredentialProof(proof); if (operatorGrant !== null) readCurrentDeliveryOperatorGrant(operatorGrant);",
            );
            line(&mut output, "        return Object.freeze({ handle, payload, sourceEntityId: intent.sourceEntityId, sourceRevision: intent.sourceRevision, databaseRemainingMs });");
            line(&mut output, "      }");
        }
        line(
            &mut output,
            "      default: return fault(\"delivery.binding\");",
        );
        line(&mut output, "    }");
        line(&mut output, "  });");
        line(&mut output, "},");
        line(&mut output, "async complete_delivery_invocation(binding: string, handle: DeliveryClaimHandle, outcome: any) {");
        line(
            &mut output,
            "  requireTransaction(); return client.transaction(async () => {",
        );
        line(&mut output, "    const intent = await client.read_delivery_intent(handle.intentId); if (intent === null) return false;");
        line(&mut output, "    switch (binding) {");
        for binding in self.project.delivery_model().bindings() {
            let d = binding.descriptor();
            let s = &d.selection;
            line(
                &mut output,
                &format!("      case {}: {{", ts_string(binding.binding_identity())),
            );
            line(&mut output, &format!("        if (intent.job !== {} || intent.sourceOperation !== {} || intent.sourceEntity !== {} || intent.orderingKey !== intent.sourceEntityId || intent.payloadVersion !== \"reminder.v1\") return fault(\"delivery.binding_intent\");", ts_string(&binding.schedule().job), ts_string(&format!("{}.selection", binding.binding_identity())), ts_string(&s.entity.text)));
            let table = sql_identifier(&snake_case(&s.entity.text));
            let id = sql_identifier(&snake_case(last(&s.identity)));
            let sent = sql_identifier(&snake_case(last(&d.completion.field)));
            line(&mut output, &format!("        const source = await execute(\"delivery.completion.source_lock\", {} + (postgres !== null ? \" FOR UPDATE\" : \"\"), [intent.sourceEntityId]);", ts_string(&format!("SELECT {id} FROM {table} WHERE {id} = $1"))));
            line(
                &mut output,
                "        if (source.length > 1) return fault(\"delivery.source_cardinality\");",
            );
            line(&mut output, "        if (outcome.kind === \"unknown\") return client.record_delivery_unknown(handle);");
            line(&mut output, "        if (outcome.kind === \"no_effect\") return client.record_delivery_mail_no_effect(handle, outcome.value);");
            line(
                &mut output,
                "        if (outcome.kind !== \"accepted\") return fault(\"delivery.outcome\");",
            );
            line(
                &mut output,
                "        if (operationTime === null) return fault(\"delivery.operation_time\");",
            );
            line(&mut output, "        if (!await client.acknowledge_delivery_mail(handle, outcome.receipt, outcome.observedAt)) return false;");
            // No role/owner/visibility cut after committed admission. The only
            // application write guard is original revision + still unsent.
            line(&mut output, &format!("        if (source.length === 1 && await client.read_delivery_schedule_revision({}, intent.sourceEntityId) === intent.sourceRevision) {{", ts_string(&s.entity.text)));
            line(&mut output, "          const receipt = await execute(\"delivery.completion.observation\", 'SELECT observed_at FROM \"__jadpo_delivery_mail_receipts_v1\" WHERE intent_id = $1 AND claim_id = $2 AND generation = $3', [handle.intentId, handle.claimId, handle.generation]); if (receipt.length !== 1) return fault(\"delivery.receipt_observation\");");
            // This is a semantic entity change, not an ordinary policy mutation
            // or a touch helper (which would add a forbidden visibility cut).
            // Resolve compiler-owned field roles, never the name updated_at.
            let source = self.records.get(&s.entity.text).expect("finished source");
            let mut assignments = vec![format!("{sent} = $1")];
            let mut values =
                "persistenceInstant(receipt[0].observed_at, postgres !== null)".to_owned();
            for field in &source.fields {
                if field.generated == Some(jadpo_syntax::GeneratedFieldRole::CreateOrChange) {
                    assignments.push(format!(
                        "{} = $2",
                        sql_identifier(&snake_case(&field.name.text))
                    ));
                }
            }
            if assignments.len() > 1 {
                values.push_str(", persistenceInstant(operationTime, postgres !== null)");
            }
            let identity_parameter = if assignments.len() > 1 { "$3" } else { "$2" };
            values.push_str(", intent.sourceEntityId");
            line(&mut output, &format!("          const changed = await execute(\"delivery.completion.sent\", {}, [{values}]);", ts_string(&format!("UPDATE {table} SET {} WHERE {id} = {identity_parameter} AND {sent} IS NULL RETURNING *", assignments.join(", ")))));
            line(
                &mut output,
                "          if (changed.length > 1) return fault(\"delivery.source_cardinality\");",
            );
            if self.entity_has_derived_representations(&s.entity.text) {
                line(&mut output, &format!("          if (changed.length === 1) await recordAuthorityChange({}, \"update\", changed[0] as Record<string, unknown>, operationTime, postgres, sqlite);", ts_string(&s.entity.text)));
            }
            line(&mut output, "        }");
            line(&mut output, "        return true;");
            line(&mut output, "      }");
        }
        line(
            &mut output,
            "      default: return fault(\"delivery.binding\");",
        );
        line(&mut output, "    }");
        line(&mut output, "  });");
        line(&mut output, "},");
        output
    }
}
