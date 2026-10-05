//! Closed finished-binding scheduler orchestration. Still no public activation
//! or numeric defaults: native components require an explicit test/host profile.
use super::{line, ts_string, TargetGenerator};

impl TargetGenerator<'_> {
    pub(super) fn delivery_scheduler_bridges(&self, output: &mut String) {
        if self.project.delivery_model().bindings().is_empty() {
            return;
        }
        line(output, "export async function runDeliveryScheduleActivation(binding: string, proof: unknown, profile: Readonly<{ executionMs: number; leaseMs: number }>, at: unknown = undefined) {");
        line(
            output,
            "  const context = captureOperation(null, null, undefined, false);",
        );
        // Optional native clock input is a component-test/host seam, never an
        // authored callable or a substitution of scheduled_for for clock.now.
        line(output, "  const operationTime = at === undefined ? context.now : validateInstant(at, \"delivery.activation.operationTime\");");
        line(output, "  if (profile === null || typeof profile !== \"object\" || Reflect.ownKeys(profile).sort().join(\",\") !== \"executionMs,leaseMs\" || !Number.isSafeInteger(profile.executionMs) || !Number.isSafeInteger(profile.leaseMs) || profile.leaseMs <= 0 || profile.executionMs < profile.leaseMs) invalid(\"delivery.profile\", \"an explicit finite execution/lease profile\");");
        line(output, "  profile = Object.freeze({ executionMs: profile.executionMs, leaseMs: profile.leaseMs });");
        line(output, "  readCurrentDeliveryCredentialProof(proof);");
        line(output, "  let intervalMs: number; switch (binding) {");
        for binding in self.project.delivery_model().bindings() {
            line(
                output,
                &format!(
                    "    case {}: intervalMs = {}; break;",
                    ts_string(binding.binding_identity()),
                    binding.schedule().interval_ms
                ),
            );
        }
        line(
            output,
            "    default: throw new AuthenticationFault(\"authority_invariant\", 500);",
        );
        line(output, "  }");
        line(output, "  const started = performance.now();");
        line(output, "  const readCursor = (value: unknown) => { const cursor = validateDeliverySelectionCursor(binding, value); if (Date.parse(cursor.dueAt) >= Date.parse(cursor.operationTime)) invalid(\"delivery.cursor\", \"coordinates before their recorded selection snapshot\"); return cursor; };");
        line(output, "  const staged = await rootPersistence.withOperationTime(operationTime).transaction(async (tx: any) => {");
        line(
            output,
            "    await tx.tick_delivery_schedule(binding, intervalMs, operationTime);",
        );
        line(output, "    const handle = await tx.claim_delivery_activation(binding, operationTime, profile); if (handle === null) return null;");
        // Continuation coordinates are read from the compiler's durable row.
        // Only their snapshot changes on a new activation; the typed tuple and
        // exact binding cannot come from a user-supplied cursor/ordinary query.
        // Validate every retained coordinate before the owning transaction can
        // publish its new generation. A resumed staged page must not bypass the
        // checked cursor boundary merely because selection is unnecessary.
        line(output, "    const previous = handle.cursor === null ? null : readCursor(handle.cursor);");
        line(output, "    let page = handle.page; if (page !== null && page.after !== null) page = Object.freeze({ intentIds: page.intentIds, after: readCursor(page.after) });");
        line(output, "    if (page === null) {");
        line(output, "      const after = previous === null ? null : validateDeliverySelectionCursor(binding, { ...previous, operationTime });");
        line(
            output,
            "      const selected = await tx.select_delivery_intents(binding, proof, after);",
        );
        line(output, "      page = Object.freeze({ intentIds: Object.freeze(selected.intents.map((intent: any) => intent.intentId)), after: selected.after });");
        line(output, "      if (!await tx.stage_delivery_activation_page(handle, page.intentIds, page.after)) throw new RequestDeadlineFault();");
        line(output, "    }");
        line(
            output,
            "    const clocks = await tx.delivery_activation_clock();",
        );
        line(output, "    readCurrentDeliveryCredentialProof(proof);");
        line(output, "    return Object.freeze({ handle, page, remainingMs: Math.min(Date.parse(handle.leaseUntil), Date.parse(handle.executionDeadline)) - Date.parse(clocks) });");
        line(output, "  });");
        // Owning COMMIT acknowledgement precedes every invocation/HTTP call.
        // A slow commit is charged in full against the database-time remainder.
        line(output, "  if (staged === null) return Object.freeze({ status: \"idle\", selected: 0, attempted: 0, continued: false });");
        line(output, "  const deadlineAt = started + staged.remainingMs;");
        line(output, "  let attempted = 0;");
        line(output, "  for (const intentId of staged.page.intentIds) {");
        line(output, "    if (performance.now() >= deadlineAt) return Object.freeze({ status: \"partial\", selected: staged.page.intentIds.length, attempted, continued: true });");
        line(output, "    const token = await prepareDeliveryInvocation(binding, proof, intentId, operationTime, profile);");
        line(output, "    if (token !== null) {");
        // The closed lexical WeakMap is compiler-owned, not another public
        // deadline setter or an authored invocation capability.
        line(output, "      const invocation = deliveryInvocations.get(token); invocation.deadlineAt = Math.min(invocation.deadlineAt, deadlineAt);");
        line(
            output,
            "      const outcome = await dispatchDeliveryInvocation(token);",
        );
        line(
            output,
            "      const completed = await completeDeliveryInvocation(outcome);",
        );
        line(output, "      if (completed === false || completed === null) return Object.freeze({ status: \"partial\", selected: staged.page.intentIds.length, attempted, continued: true });");
        line(output, "    }");
        line(output, "    attempted++;");
        line(output, "  }");
        line(output, "  const finished = await rootPersistence.transaction((tx: any) => tx.finish_delivery_activation(staged.handle));");
        line(output, "  return Object.freeze({ status: finished ? \"finished\" : \"partial\", selected: staged.page.intentIds.length, attempted, continued: !finished || staged.page.after !== null });");
        line(output, "}");
    }
}
