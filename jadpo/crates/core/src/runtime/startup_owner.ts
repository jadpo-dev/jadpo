// Compiler-private initial native entry ownership. No authored callback, env role
// flag or absolute cross-process clock is accepted by this closed IPC protocol.
const sourceRevision = __JADPO_SOURCE_REVISION__;
const configurationDeclarations: readonly string[] = __JADPO_CONFIGURATION_DECLARATIONS__;
const startupMs = 10_000;
const entry = new URL("./app.ts", import.meta.url);
const nativeMain = Bun.pathToFileURL(Bun.main).href === entry.href;
type ChildOwner = { owner: string; deadlineAt: number; phase: "owned" | "validated" | "permitted" | "prepared" | "completed" | "ready"; current: boolean };
let childOwner: ChildOwner | null = null;
let continueReply: { resolve(): void; reject(error: Error): void } | null = null;
let completionReply: { resolve(): void; reject(error: Error): void } | null = null;
const failure = () => new Error("Native startup ownership unavailable.");
function exact(value: unknown, keys: readonly string[]): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    && Object.keys(value).sort().join(",") === [...keys].sort().join(",");
}
function liveChild(): ChildOwner {
  const value = childOwner;
  if (value === null || !value.current || !process.connected || performance.now() >= value.deadlineAt) throw failure();
  return value;
}
export function initialNativeStartupDeadline(): number | null {
  if (!nativeMain) return null;
  if (childOwner?.phase === "ready") return null;
  return liveChild().deadlineAt;
}
export async function authorizeNativeStartupBind(): Promise<void> {
  if (!nativeMain) return;
  const owner = liveChild();
  if (owner.phase !== "owned") throw failure();
  owner.phase = "validated";
  const reply = new Promise<void>((resolve, reject) => { continueReply = { resolve, reject }; });
  process.send!({ kind: "validated", owner: owner.owner, sourceRevision });
  await reply; liveChild();
}
export function checkNativeStartupBind(): void {
  if (!nativeMain) return;
  if (liveChild().phase !== "completed") throw failure();
}
export async function completeNativeStartup(): Promise<void> {
  if (!nativeMain) return;
  const owner = liveChild();
  if (owner.phase !== "permitted") throw failure();
  owner.phase = "prepared";
  const reply = new Promise<void>((resolve, reject) => { completionReply = { resolve, reject }; });
  process.send!({ kind: "prepared", owner: owner.owner, sourceRevision });
  await reply;
  if (liveChild().phase !== "completed") throw failure();
}
export function checkNativeStartupPublication(): void {
  if (!nativeMain) return;
  if (liveChild().phase !== "completed") throw failure();
}
export function acknowledgeNativeStartupPublication(): void {
  if (!nativeMain) return;
  checkNativeStartupPublication();
  const owner = childOwner!;
  owner.phase = "ready";
  // No further reply is awaited. The supervising parent still owns its original
  // deadline until this exact final phase packet arrives; stdout cannot clear it.
  process.send!({ kind: "ready", owner: owner.owner, sourceRevision });
}
async function acceptChildOwner(): Promise<void> {
  const bootAt = performance.now();
  let accept!: () => void, refuse!: (error: Error) => void;
  const owned = new Promise<void>((resolve, reject) => { accept = resolve; refuse = reject; });
  const invalidate = () => {
    if (childOwner !== null) childOwner.current = false;
    refuse(failure()); continueReply?.reject(failure()); completionReply?.reject(failure());
    process.exit(1); // Channel loss cannot authorize a later native bind.
  };
  process.once("disconnect", invalidate);
  process.on("message", (message: unknown) => {
    if (childOwner === null) {
      if (!exact(message, ["kind", "owner", "sourceRevision", "remainingMs"]) || message.kind !== "owned"
        || message.sourceRevision !== sourceRevision || typeof message.owner !== "string"
        || !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/u.test(message.owner)
        || typeof message.remainingMs !== "number" || !Number.isFinite(message.remainingMs)
        || message.remainingMs <= 0 || message.remainingMs > startupMs) return invalidate();
      // BOOT precedes the parent's remaining-budget receipt. Starting this cap
      // at BOOT (never OWNED receipt) only shortens the original parent budget.
      childOwner = { owner: message.owner, deadlineAt: bootAt + message.remainingMs, phase: "owned", current: true };
      try { liveChild(); accept(); } catch { invalidate(); }
      return;
    }
    let owner: ChildOwner;
    try { owner = liveChild(); } catch { return invalidate(); }
    if (!exact(message, ["kind", "owner", "sourceRevision"]) || message.owner !== owner.owner || message.sourceRevision !== sourceRevision) return invalidate();
    if (message.kind === "continue" && owner.phase === "validated" && continueReply !== null) {
      owner.phase = "permitted"; const reply = continueReply; continueReply = null; reply.resolve(); return;
    }
    if (message.kind === "complete" && owner.phase === "prepared" && completionReply !== null) {
      owner.phase = "completed"; const reply = completionReply; completionReply = null; reply.resolve(); return;
    }
    invalidate();
  });
  const handshakeTimeout = setTimeout(invalidate, startupMs);
  process.send!({ kind: "boot", sourceRevision });
  try { await owned; } finally { clearTimeout(handshakeTimeout); }
}
function safeStartupFailure(): void {
  console.error(JSON.stringify({ schemaVersion: 1, kind: "operational_log_event", eventName: "operation.failed", classification: "RUNTIME_STARTUP_FAILED", requestId: "startup", traceId: null, semanticOperationId: "runtime:start", sourceRevision, attributes: {} }));
}
function safeEventAttributes(event: Record<string, unknown>): boolean {
  if (event.eventName === "runtime.ready" || event.eventName === "runtime.live") {
    if (!exact(event.attributes, ["port", "readiness"])) return false;
    return typeof event.attributes.port === "number" && Number.isSafeInteger(event.attributes.port)
      && event.attributes.port >= 0 && event.attributes.port <= 65535
      && ["ready", "not_ready"].includes(event.attributes.readiness as string);
  }
  return exact(event.attributes, []);
}
function safeStructuredLine(line: string, startup: boolean): Record<string, unknown> | null {
  let value: unknown; try { value = JSON.parse(line); } catch { return null; }
  if (value === null || typeof value !== "object" || Array.isArray(value)) return null;
  const event = value as Record<string, unknown>;
  if (event.schemaVersion !== 1) return null;
  if (event.kind === "configuration_diagnostic") {
    return exact(event, ["schemaVersion", "kind", "code", "declaration"])
      && ["config_required", "config_invalid"].includes(event.code as string)
      && configurationDeclarations.includes(event.declaration as string) ? event : null;
  }
  // The compiler scheduler can fail its immediate pulse before final startup
  // acknowledgement. Its existing closed event carries no error/authority data.
  if (event.kind === "maintenance_audit" && event.eventName === "retention.schedule.failure") {
    return exact(event, ["schemaVersion", "kind", "eventName", "outcome"])
      && event.outcome === "failed" ? event : null;
  }
  if (startup && event.kind === "operational_log_event") {
    const identity = event.eventName === "operation.failed" && event.classification === "RUNTIME_STARTUP_FAILED" && event.requestId === "startup" && event.semanticOperationId === "runtime:start"
      || ["runtime.ready", "runtime.live"].includes(event.eventName as string) && ["ready", "live"].includes(event.classification as string) && event.requestId === "startup" && event.semanticOperationId === "runtime:start"
      || event.eventName === "dependency.unavailable" && event.classification === "RUNTIME_DEPENDENCY_UNAVAILABLE" && event.requestId === "readiness" && event.semanticOperationId === "health:ready";
    return identity && exact(event, ["schemaVersion", "kind", "eventName", "classification", "requestId", "traceId", "semanticOperationId", "sourceRevision", "attributes"])
      && event.sourceRevision === sourceRevision && event.traceId === null && safeEventAttributes(event) ? event : null;
  }
  if (startup) return null;
  if (event.kind === "operational_log_event" && !["transaction.attempt", "service.attempt"].includes(event.eventName as string)) {
    const names = ["runtime.ready", "runtime.live", "operation.failed", "dependency.unavailable"];
    return exact(event, ["schemaVersion", "kind", "eventName", "classification", "requestId", "traceId", "semanticOperationId", "sourceRevision", "attributes"])
      && names.includes(event.eventName as string) && event.sourceRevision === sourceRevision
      && typeof event.classification === "string" && ["ready", "live", "RUNTIME_STARTUP_FAILED", "RUNTIME_DEPENDENCY_UNAVAILABLE", "RUNTIME_UNHANDLED_FAULT", "RUNTIME_READ_UNAVAILABLE", "RUNTIME_SERVICE_MISCONFIGURED", "RUNTIME_OUTCOME_UNKNOWN"].includes(event.classification)
      && typeof event.requestId === "string" && event.requestId.length <= 128
      && (event.traceId === null || typeof event.traceId === "string" && event.traceId.length <= 128)
      && typeof event.semanticOperationId === "string" && event.semanticOperationId.length <= 1024
      && safeEventAttributes(event) ? event : null;
  }
  if (event.kind === "operational_log_event" && event.eventName === "transaction.attempt") {
    const keys = ["schemaVersion", "kind", "eventName", "operationId", "operation", "attempt", "database", "phase", "outcome", "retryableCause", "rollbackProven", "commitAcknowledged", "elapsedMs", "selectedDelayMs", "retryScheduled", "cancelled"];
    return exact(event, keys) && typeof event.operationId === "string" && /^[0-9a-f-]{36}$/u.test(event.operationId)
      && typeof event.operation === "string" && /^[A-Za-z0-9_.]{1,1024}$/u.test(event.operation)
      && ["sqlite", "postgres"].includes(event.database as string)
      && ["begin", "statement", "commit", "rollback"].includes(event.phase as string)
      && ["committed", "no_commit", "unknown"].includes(event.outcome as string)
      && (event.retryableCause === null || ["sqlite_busy", "sqlite_locked", "postgres_connection", "postgres_serialization", "postgres_deadlock", "delivery_schema_busy"].includes(event.retryableCause as string))
      && typeof event.attempt === "number" && Number.isSafeInteger(event.attempt) && event.attempt > 0
      && typeof event.elapsedMs === "number" && Number.isFinite(event.elapsedMs) && event.elapsedMs >= 0
      && (event.selectedDelayMs === null || typeof event.selectedDelayMs === "number" && Number.isFinite(event.selectedDelayMs) && event.selectedDelayMs >= 0)
      && ["rollbackProven", "commitAcknowledged", "retryScheduled", "cancelled"].every(key => typeof event[key] === "boolean") ? event : null;
  }
  if (event.kind === "operational_log_event" && event.eventName === "service.attempt") {
    return exact(event, ["schemaVersion", "kind", "eventName", "operationId", "attempt", "elapsedMs", "outcome", "retryScheduled", "retryDelayMs"])
      && typeof event.operationId === "string" && /^[0-9a-f-]{36}$/u.test(event.operationId)
      && typeof event.attempt === "number" && Number.isSafeInteger(event.attempt) && event.attempt > 0 && event.attempt <= 3
      && typeof event.elapsedMs === "number" && Number.isFinite(event.elapsedMs) && event.elapsedMs >= 0
      && ["accepted", "recipient_rejected", "misconfigured", "outcome_unknown", "temporarily_unavailable", "rate_limited", "pre_dispatch_failure"].includes(event.outcome as string)
      && typeof event.retryScheduled === "boolean"
      && (event.retryDelayMs === null || typeof event.retryDelayMs === "number" && Number.isFinite(event.retryDelayMs) && event.retryDelayMs >= 0) ? event : null;
  }
  if (event.kind === "maintenance_audit" && event.eventName === "retention.purge.batch") {
    const count = (value: unknown) => typeof value === "number" && Number.isSafeInteger(value) && value >= 0 && value <= 500;
    return exact(event, ["schemaVersion", "kind", "eventName", "operation", "entity", "clause", "sourceRevision", "retentionBinding", "attemptNow", "selectedCount", "committedCount", "rowIdentityTokens", "bound", "outcome"])
      && event.sourceRevision === sourceRevision && typeof event.entity === "string" && /^[A-Za-z_][A-Za-z0-9_]*$/u.test(event.entity)
      && event.operation === `retention_purge(${event.entity})` && event.clause === `${event.entity}.lifecycle.purge`
      && typeof event.retentionBinding === "string" && /^config\.[A-Za-z_][A-Za-z0-9_]*$/u.test(event.retentionBinding)
      && typeof event.attemptNow === "string" && /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/u.test(event.attemptNow)
      && count(event.selectedCount) && (event.committedCount === null || count(event.committedCount))
      && event.bound === 500 && ["committed", "rolled_back", "unknown"].includes(event.outcome as string)
      && Array.isArray(event.rowIdentityTokens) && event.rowIdentityTokens.length === event.selectedCount
      && event.rowIdentityTokens.every(token => typeof token === "string" && /^[0-9a-f]{16}$/u.test(token)) ? event : null;
  }
  if (event.kind === "authentication_audit") {
    return exact(event, ["schemaVersion", "kind", "eventName", "strategy", "serviceId", "credentialId", "occurredAt"])
      && ["service_credential.issued", "service_credential.exchanged", "service_credential.refreshed", "service_credential.revoked"].includes(event.eventName as string)
      && typeof event.strategy === "string" && /^[A-Za-z0-9_]{1,128}$/u.test(event.strategy)
      && ["serviceId", "credentialId"].every(key => typeof event[key] === "string" && /^[0-9a-f-]{36}$/u.test(event[key] as string))
      && typeof event.occurredAt === "number" && Number.isSafeInteger(event.occurredAt) && event.occurredAt >= 0 ? event : null;
  }
  return null;
}
async function superviseNativeEntry(): Promise<never> {
  const deadlineAt = performance.now() + startupMs, owner = crypto.randomUUID();
  let phase: "boot" | "owned" | "permitted" | "completing" | "ready" = "boot";
  let expired = false, failed = false, safeFailureReported = false;
  let pendingReadiness: Record<string, unknown> | null = null;
  const terminate = () => { failed = true; child.kill("SIGKILL"); };
  const child = Bun.spawn([process.execPath, "--no-install", "--env-file=/dev/null", Bun.main], {
    env: { ...Bun.env }, stdout: "pipe", stderr: "pipe",
    ipc(message: unknown) {
      if (failed || performance.now() >= deadlineAt) { expired = true; return terminate(); }
      if (phase === "boot") {
        if (!exact(message, ["kind", "sourceRevision"]) || message.kind !== "boot" || message.sourceRevision !== sourceRevision) return terminate();
        phase = "owned"; child.send({ kind: "owned", owner, sourceRevision, remainingMs: deadlineAt - performance.now() }); return;
      }
      if (!exact(message, ["kind", "owner", "sourceRevision"]) || message.owner !== owner || message.sourceRevision !== sourceRevision) return terminate();
      if (phase === "owned" && message.kind === "validated") { phase = "permitted"; child.send({ kind: "continue", owner, sourceRevision }); return; }
      if (phase === "permitted" && message.kind === "prepared") { phase = "completing"; child.send({ kind: "complete", owner, sourceRevision }); return; }
      if (phase === "completing" && message.kind === "ready") {
        phase = "ready"; clearTimeout(timer);
        if (pendingReadiness !== null) { console.log(JSON.stringify(pendingReadiness)); pendingReadiness = null; }
        return;
      }
      terminate();
    },
  });
  const timer = setTimeout(() => { expired = true; terminate(); }, Math.max(0, deadlineAt - performance.now()));
  const signal = (name: "SIGTERM" | "SIGINT") => { child.kill(name); };
  const term = () => signal("SIGTERM"), interrupt = () => signal("SIGINT");
  process.once("SIGTERM", term); process.once("SIGINT", interrupt);
  const startupOutputLimit = 65_536 + configurationDeclarations.reduce((total, declaration) => total + declaration.length * 2 + 256, 0);
  const lineLimit = Math.max(16_384, ...configurationDeclarations.map(declaration => declaration.length * 2 + 256));
  let startupBytes = 0;
  const pump = async (stream: ReadableStream<Uint8Array>, errorStream: boolean) => {
    const reader = stream.getReader(), decoder = new TextDecoder(); let pending = "", discarding = false;
    try {
      while (true) {
        const read = await reader.read(); if (read.done) break;
        if (phase !== "ready") { startupBytes += read.value.byteLength; if (startupBytes > startupOutputLimit) { terminate(); continue; } }
        const text = decoder.decode(read.value, { stream: true });
        for (const character of text) {
          if (character === "\n") {
            if (!discarding) {
              const event = safeStructuredLine(pending, phase !== "ready");
              if (event !== null) {
                if (event.eventName === "runtime.ready" || event.eventName === "runtime.live") {
                  // A line is observation, never final-handoff authority. Hold
                  // the bounded publication until the closed READY packet.
                  if (errorStream || !["completing", "ready"].includes(phase)) { pending = ""; continue; }
                  if (phase !== "ready") { if (performance.now() < deadlineAt) pendingReadiness = event; pending = ""; continue; }
                }
                if (event.classification === "RUNTIME_STARTUP_FAILED") safeFailureReported = true;
                (errorStream ? console.error : console.log)(JSON.stringify(event));
              }
            }
            pending = ""; discarding = false;
          } else if (!discarding) { if (pending.length >= lineLimit) { pending = ""; discarding = true; } else pending += character; }
        }
      }
    } finally { reader.releaseLock(); }
  };
  const output = Promise.allSettled([pump(child.stdout, false), pump(child.stderr, true)]);
  // The FIRST ESM dependency holds the parent for the entire serving child
  // lifetime. A success never falls through to parent database initialization.
  const exit = await child.exited;
  clearTimeout(timer); process.removeListener("SIGTERM", term); process.removeListener("SIGINT", interrupt);
  await output;
  const startupFailed = expired || failed || phase !== "ready";
  if (startupFailed && !safeFailureReported) safeStartupFailure();
  process.exit(startupFailed ? 1 : exit);
}
if (nativeMain) {
  try {
    if (typeof process.send === "function" && process.connected) await acceptChildOwner();
    else await superviseNativeEntry();
  } catch { safeStartupFailure(); process.exit(1); }
}
