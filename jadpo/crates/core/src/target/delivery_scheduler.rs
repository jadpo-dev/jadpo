//! Closed finished-binding scheduler orchestration. Still no public activation
//! or numeric defaults: native components require an explicit test/host profile.
use super::{line, ts_string, TargetGenerator};

impl TargetGenerator<'_> {
    pub(super) fn delivery_scheduler_bridges(&self, output: &mut String) {
        if self.project.delivery_model().bindings().is_empty() {
            return;
        }
        line(output, "export async function runDeliveryScheduleActivation(binding: string, proof: unknown, activationProfile: Readonly<{ executionMs: number; leaseMs: number }>, deliveryProfile: Readonly<{ executionMs: number; leaseMs: number }>, at: unknown = undefined) {");
        line(
            output,
            "  const context = captureOperation(null, null, undefined, false);",
        );
        // Optional native clock input is a component-test/host seam, never an
        // authored callable or a substitution of scheduled_for for clock.now.
        line(output, "  const operationTime = at === undefined ? context.now : validateInstant(at, \"delivery.activation.operationTime\");");
        // Singleton pass budgets and immutable per-intent budgets are separate
        // authorities. Neither missing input may fall back to the other profile.
        line(output, "  for (const [name, profile] of Object.entries({ activation: activationProfile, delivery: deliveryProfile })) { if (profile === null || typeof profile !== \"object\" || Reflect.ownKeys(profile).sort().join(\",\") !== \"executionMs,leaseMs\" || !Number.isSafeInteger(profile.executionMs) || !Number.isSafeInteger(profile.leaseMs) || profile.leaseMs <= 0 || profile.executionMs < profile.leaseMs) invalid(`delivery.${name}Profile`, \"an explicit finite execution/lease profile\"); }");
        line(output, "  activationProfile = Object.freeze({ executionMs: activationProfile.executionMs, leaseMs: activationProfile.leaseMs });");
        line(output, "  deliveryProfile = Object.freeze({ executionMs: deliveryProfile.executionMs, leaseMs: deliveryProfile.leaseMs });");
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
        line(output, "    const handle = await tx.claim_delivery_activation(binding, operationTime, activationProfile); if (handle === null) return null;");
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
        line(output, "    return Object.freeze({ handle, page, remainingMs: Math.min(Date.parse(handle.leaseUntil), Date.parse(handle.executionDeadline)) - Date.parse(clocks), executionRemainingMs: Date.parse(handle.executionDeadline) - Date.parse(clocks) });");
        line(output, "  });");
        // Owning COMMIT acknowledgement precedes every invocation/HTTP call.
        // A slow commit is charged in full against the database-time remainder.
        line(output, "  if (staged === null) return Object.freeze({ status: \"idle\", selected: 0, attempted: 0, continued: false });");
        line(output, "  let handle = staged.handle, deadlineAt = started + staged.remainingMs;");
        line(output, "  const executionDeadlineAt = started + staged.executionRemainingMs;");
        line(output, "  let attempted = 0; const failures: Readonly<JsonObject>[] = []; const publicFailures = () => failures.length === 0 ? {} : { failures: Object.freeze([...failures]) };");
        line(output, "  for (const intentId of staged.page.intentIds) {");
        line(output, "    if (performance.now() >= deadlineAt) return Object.freeze({ status: \"partial\", selected: staged.page.intentIds.length, attempted, continued: true, ...publicFailures() });");
        // Renew only the same still-live activation, outside provider I/O. The
        // acknowledged renewal transaction precedes each intent admission; the
        // provider stays bounded by this lease and the immutable execution cap.
        line(output, "    const renewalStarted = performance.now();");
        line(output, "    const renewed = await rootPersistence.transaction(async (tx: any) => { readCurrentDeliveryCredentialProof(proof); const next = await tx.renew_delivery_activation(handle); if (next === null) return null; const now = await tx.delivery_activation_clock(); readCurrentDeliveryCredentialProof(proof); return { handle: next, remainingMs: Math.min(Date.parse(next.leaseUntil), Date.parse(next.executionDeadline)) - Date.parse(now) }; });");
        line(output, "    if (renewed === null) return Object.freeze({ status: \"partial\", selected: staged.page.intentIds.length, attempted, continued: true, ...publicFailures() });");
        line(output, "    handle = renewed.handle; deadlineAt = Math.min(executionDeadlineAt, renewalStarted + renewed.remainingMs);");
        line(output, "    if (performance.now() >= deadlineAt) return Object.freeze({ status: \"partial\", selected: staged.page.intentIds.length, attempted, continued: true, ...publicFailures() });");
        line(output, "    const token = await prepareDeliveryInvocation(binding, proof, intentId, operationTime, deliveryProfile);");
        line(output, "    if (token !== null) {");
        // The closed lexical WeakMap is compiler-owned, not another public
        // deadline setter or an authored invocation capability.
        line(output, "      const invocation = deliveryInvocations.get(token); invocation.deadlineAt = Math.min(invocation.deadlineAt, deadlineAt);");
        line(
            output,
            "      const outcome = await dispatchDeliveryInvocation(token);",
        );
        line(output, "      const publicFailure = deliveryOutcomes.get(outcome)?.publicFailure; if (publicFailure !== null && publicFailure !== undefined) failures.push(publicFailure);");
        line(
            output,
            "      const completed = await completeDeliveryInvocation(outcome);",
        );
        line(output, "      if (completed === false || completed === null) return Object.freeze({ status: \"partial\", selected: staged.page.intentIds.length, attempted, continued: true, ...publicFailures() });");
        line(output, "    }");
        line(output, "    attempted++;");
        line(output, "  }");
        line(output, "  const finished = await rootPersistence.transaction((tx: any) => tx.finish_delivery_activation(handle));");
        line(output, "  return Object.freeze({ status: finished ? \"finished\" : \"partial\", selected: staged.page.intentIds.length, attempted, continued: !finished || staged.page.after !== null, ...publicFailures() });");
        line(output, "}");
        // Trusted native provisioning is distinct from ordinary authored calls:
        // raw service credential stays lexical, resolves through the checked
        // authentication host each run, and never substitutes provider secrets.
        // No callback, synthetic principal, authored operation clock or profile
        // default can enter this closed binding host.
        line(output, "export function createDeliveryJobHost(binding: string, credential: string, activationProfile: Readonly<{ executionMs: number; leaseMs: number }>, deliveryProfile: Readonly<{ executionMs: number; leaseMs: number }>) {");
        line(output, "  let strategy: string, intervalMs: number; switch (binding) {");
        for binding in self.project.delivery_model().bindings() {
            let strategy = binding.authority_for_lowering().strategy;
            line(output, &format!("    case {}: strategy = {}; intervalMs = {}; break;", ts_string(binding.binding_identity()), ts_string(strategy), binding.schedule().interval_ms));
        }
        line(output, "    default: throw new AuthenticationFault(\"authority_invariant\", 500);");
        line(output, "  }");
        line(output, "  if (!Number.isSafeInteger(intervalMs) || intervalMs <= 0) invalid(\"delivery.interval\", \"a portable positive interval\");");
        line(output, "  if (typeof credential !== \"string\" || credential.length === 0) throw new AuthenticationFault(\"invalid_credentials\", 401);");
        line(output, "  const checkedProfile = (name: string, value: Readonly<{ executionMs: number; leaseMs: number }>) => { if (value === null || typeof value !== \"object\" || Reflect.ownKeys(value).sort().join(\",\") !== \"executionMs,leaseMs\") invalid(`delivery.${name}Profile`, \"an explicit finite execution/lease profile\"); const copy = Object.freeze({ executionMs: value.executionMs, leaseMs: value.leaseMs }); if (!Number.isSafeInteger(copy.executionMs) || !Number.isSafeInteger(copy.leaseMs) || copy.leaseMs <= 0 || copy.executionMs < copy.leaseMs) invalid(`delivery.${name}Profile`, \"an explicit finite execution/lease profile\"); return copy; };");
        line(output, "  activationProfile = checkedProfile(\"activation\", activationProfile); deliveryProfile = checkedProfile(\"delivery\", deliveryProfile);");
        line(output, "  let running: Promise<any> | null = null, timer: ReturnType<typeof setTimeout> | null = null, started = false, stopped = false;");
        line(output, "  const runOnce = () => { if (stopped) invalid(\"delivery.host\", \"a live host\"); if (running !== null) return running; const work = (async () => { const at = captureOperation(null, null, undefined, false).now; const proof = await authenticationHost().prepareDeliveryCredential(strategy, credential, Date.parse(at)); return runDeliveryScheduleActivation(binding, proof, activationProfile, deliveryProfile, at); })(); running = work; void work.finally(() => { if (running === work) running = null; }).catch(() => {}); return work; };");
        line(output, "  const schedule = (continued: boolean) => { if (stopped) return; const now = Date.now(); const dueAt = continued ? now : (Math.floor(now / intervalMs) + 1) * intervalMs; timer = setTimeout(async () => { timer = null; if (stopped) return; if (Date.now() < dueAt) { schedule(false); return; } try { const outcome = await runOnce(); schedule(outcome.status === \"finished\" && outcome.continued); } catch (error) { console.error(JSON.stringify({ schemaVersion: 1, kind: \"operational_log_event\", eventName: \"delivery.activation_failed\", binding, code: error instanceof AuthenticationFault ? error.code : \"job_fault\" })); schedule(false); } }, Math.min(2_147_483_647, Math.max(0, dueAt - now))); };");
        line(output, "  return Object.freeze({ runOnce, async start() { if (started || stopped) invalid(\"delivery.host\", \"one start before stop\"); started = true; try { const outcome = await runOnce(); schedule(outcome.status === \"finished\" && outcome.continued); return outcome; } catch (error) { started = false; throw error; } }, async stop() { stopped = true; if (timer !== null) clearTimeout(timer); timer = null; if (running !== null) await running; } });");
        line(output, "}");
    }
}
