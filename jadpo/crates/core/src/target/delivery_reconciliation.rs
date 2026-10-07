//! Purpose-sealed terminal reconciliation from an existing genuine durable proof.
//! This surface supplies no authored capability, provider lookup or new attempt.
use super::{line, snake_case, sql_identifier, ts_string, TargetGenerator};
impl TargetGenerator<'_> {
    pub(super) fn delivery_reconciliation_bridges(&self, output: &mut String) {
        if self.project.delivery_model().bindings().is_empty() {
            return;
        }
        line(output,"export function createDeliveryReceiptReconciler(binding: string, grant: unknown, profile: Readonly<{executionMs:number;leaseMs:number}>) {");
        line(output,"  if (arguments.length !== 3 || profile === null || typeof profile !== \"object\" || (Object.getPrototypeOf(profile) !== Object.prototype && Object.getPrototypeOf(profile) !== null) || Reflect.ownKeys(profile).sort().join(\",\") !== \"executionMs,leaseMs\") throw new AuthenticationFault(\"invalid_credentials\",401);");
        line(output,"  const execution = Object.getOwnPropertyDescriptor(profile, \"executionMs\"), lease = Object.getOwnPropertyDescriptor(profile, \"leaseMs\"); if (!execution || !(\"value\" in execution) || !lease || !(\"value\" in lease) || !Number.isSafeInteger(execution.value) || !Number.isSafeInteger(lease.value) || execution.value <= 0 || lease.value <= 0 || lease.value > execution.value) throw new AuthenticationFault(\"invalid_credentials\",401);");
        line(output,"  readCurrentDeliveryReceiptReconciliationGrant(grant); const selected = Object.freeze({executionMs:execution.value,leaseMs:lease.value}); switch (binding) {");
        for binding in self.project.delivery_model().bindings() {
            line(
                output,
                &format!("    case {}: break;", ts_string(binding.binding_identity())),
            );
        }
        line(
            output,
            "    default: throw new AuthenticationFault(\"invalid_credentials\",401); }",
        );
        line(output,"  return Object.freeze({ async reconcile(intentId: string) { if (arguments.length !== 1) throw new AuthenticationFault(\"invalid_credentials\",401); validateUuid(intentId,\"delivery.intent\"); readCurrentDeliveryReceiptReconciliationGrant(grant); const deadlineAt = performance.now() + selected.executionMs; const context = Object.freeze({ ...captureOperation(null,null,undefined,false), deadlineAt }); const completed = await rootPersistence.withDeadline(deadlineAt).withOperationTime(context.now).transaction((tx: any) => tx.reconcile_delivery_receipt(binding, grant, selected, intentId)); readCurrentDeliveryReceiptReconciliationGrant(grant); assertOperationDeadline(context); return completed; } });");
        line(output, "}");
    }
    pub(super) fn delivery_reconciliation_methods(&self) -> String {
        let mut output = String::new();
        if self.project.delivery_model().bindings().is_empty() {
            return output;
        }
        line(&mut output,"async record_delivery_acceptance_proof(binding: string, evidence: unknown) { requireTransaction(); const accepted = readCurrentDeliveryAcceptedEvidence(evidence); if (accepted.invocation.binding !== binding) return fault(\"delivery.acceptance.binding\"); switch (binding) {");
        for binding in self.project.delivery_model().bindings() {
            let selection = &binding.descriptor().selection;
            let table = sql_identifier(&snake_case(&selection.entity.text));
            let id = sql_identifier(&snake_case(
                &selection
                    .identity
                    .path
                    .last()
                    .expect("checked identity")
                    .text,
            ));
            line(
                &mut output,
                &format!("  case {}: {{", ts_string(binding.binding_identity())),
            );
            line(&mut output,&format!("    const source = await execute(\"delivery.acceptance.source_lock\", {} + (postgres !== null ? \" FOR UPDATE\" : \"\"), [accepted.invocation.prepared.sourceEntityId]); if (source.length > 1) return fault(\"delivery.source_cardinality\");",ts_string(&format!("SELECT {id} FROM {table} WHERE {id} = $1"))));
            line(
                &mut output,
                &format!(
                    "    await acceptanceProofs.record(evidence, {}); return; }}",
                    self.delivery_acceptance_contract(binding)
                ),
            );
        }
        line(
            &mut output,
            "  default: return fault(\"delivery.acceptance.binding\"); } },",
        );
        line(&mut output,"async reconcile_delivery_receipt(binding: string, grant: unknown, profile: Readonly<{executionMs:number;leaseMs:number}>, intentId: string) {");
        line(&mut output,"  requireTransaction(); readCurrentDeliveryReceiptReconciliationGrant(grant); if (operationTime === null) return fault(\"delivery.operation_time\"); const intent = await client.read_delivery_intent(intentId); readCurrentDeliveryReceiptReconciliationGrant(grant); if (intent === null) return false; switch (binding) {");
        for binding in self.project.delivery_model().bindings() {
            let d = binding.descriptor();
            let selection = &d.selection;
            let table = sql_identifier(&snake_case(&selection.entity.text));
            let last = |path: &jadpo_syntax::NameExpression| {
                path.path.last().expect("checked field").text.clone()
            };
            let id = sql_identifier(&snake_case(&last(&selection.identity)));
            let sent = sql_identifier(&snake_case(&last(&d.completion.field)));
            line(
                &mut output,
                &format!("  case {}: {{", ts_string(binding.binding_identity())),
            );
            line(&mut output,&format!("    if (intent.job !== {} || intent.sourceOperation !== {} || intent.sourceEntity !== {} || intent.orderingKey !== intent.sourceEntityId || intent.payloadVersion !== \"reminder.v1\") return fault(\"delivery.binding_intent\");",ts_string(&binding.schedule().job),ts_string(&format!("{}.selection",binding.binding_identity())),ts_string(&selection.entity.text)));
            line(&mut output,&format!("    const source = await execute(\"delivery.reconciliation.source_lock\", {} + (postgres !== null ? \" FOR UPDATE\" : \"\"), [intent.sourceEntityId]); readCurrentDeliveryReceiptReconciliationGrant(grant); if (source.length > 1) return fault(\"delivery.source_cardinality\");",ts_string(&format!("SELECT {id} FROM {table} WHERE {id} = $1"))));
            line(&mut output,&format!("    const resolved = await acceptanceProofs.resolve(intentId, {}, grant, profile); readCurrentDeliveryReceiptReconciliationGrant(grant); if (resolved === null) return false; if (resolved.alreadySucceeded) return true;",self.delivery_acceptance_contract(binding)));
            line(&mut output,&format!("    if (source.length === 1 && await client.read_delivery_schedule_revision({}, intent.sourceEntityId) === intent.sourceRevision) {{",ts_string(&selection.entity.text)));
            line(
                &mut output,
                "      readCurrentDeliveryReceiptReconciliationGrant(grant);",
            );
            let record = self
                .records
                .get(&selection.entity.text)
                .expect("checked source");
            let mut assignments = vec![format!("{sent} = $1")];
            let mut values =
                "persistenceInstant(resolved.proof.observed_at, postgres !== null)".to_owned();
            for field in &record.fields {
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
            let parameter = if assignments.len() > 1 { "$3" } else { "$2" };
            values.push_str(", intent.sourceEntityId");
            line(&mut output,&format!("      const changed = await execute(\"delivery.reconciliation.sent\", {}, [{values}]); readCurrentDeliveryReceiptReconciliationGrant(grant); if (changed.length > 1) return fault(\"delivery.source_cardinality\");",ts_string(&format!("UPDATE {table} SET {} WHERE {id} = {parameter} AND {sent} IS NULL RETURNING *",assignments.join(", ")))));
            if self.entity_has_derived_representations(&selection.entity.text) {
                line(&mut output,&format!("      if (changed.length === 1) await recordAuthorityChange({}, \"update\", changed[0] as Record<string, unknown>, operationTime, postgres, sqlite);",ts_string(&selection.entity.text)));
            }
            line(
                &mut output,
                "    } readCurrentDeliveryReceiptReconciliationGrant(grant); return true; }",
            );
        }
        line(
            &mut output,
            "  default: return fault(\"delivery.binding\"); } },",
        );
        output
    }
}
