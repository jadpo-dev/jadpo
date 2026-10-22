// Teaching example 23's booking route, `POST /repairs`, and the readiness
// check the runtime answers for every app, `GET /health/ready`, hand-written in Bun and TypeScript for the
// performance spike (tools/bench/README.md; roadmap chunk 8.3). It does
// what the compiled app does for the same request: the same validation
// and error bodies, the same per-address limit of 120 a minute, the same
// SQL statements in the same transactions at the same isolation levels
// (read from PostgreSQL's statement log of the native build), and the
// same response. It is the app a developer would write by hand with a
// pooled client, so it prepares its statements (BUN_PREPARE=0 doesn't).
//
//   DATABASE_URL=postgresql://... PORT=8080 bun server.ts

import postgres from "postgres";

const sql = postgres(process.env.DATABASE_URL!, {
  max: Number(process.env.BUN_POOL ?? 10),
  onnotice: () => {},
});
const prepare = process.env.BUN_PREPARE !== "0";
const q = (text: string, params: unknown[]) => sql.unsafe(text, params as never[], { prepare });

const ITEMS = new Set(["bike", "lamp", "knitwear", "toaster", "microwave"]);
const LIMIT = 120;
const WINDOW_MS = 60_000;
const MAX_COUNTERS = 200_000;
const counters = new Map<string, { start: number; count: number }>();
let readyAt = 0;

function hex(bytes: number): string {
  const b = crypto.getRandomValues(new Uint8Array(bytes));
  let s = "";
  for (const x of b) s += x.toString(16).padStart(2, "0");
  return s;
}

function json(status: number, body: unknown, requestId: string, extra: Record<string, string> = {}) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json", "x-request-id": requestId, ...extra },
  });
}

function failure(status: number, code: string, message: string, requestId: string, details?: unknown) {
  const error: Record<string, unknown> = { code, message, request_id: requestId };
  if (details) error.details = details;
  return json(status, { error }, requestId);
}

// X-Forwarded-For with one trusted proxy: the last address, else the
// connection's own.
function clientAddress(request: Request, server: Bun.Server): string {
  const header = request.headers.get("x-forwarded-for");
  if (header) {
    const parts = header.split(",");
    const last = parts[parts.length - 1].trim();
    if (last) return last;
  }
  return server.requestIP(request)?.address ?? "unknown";
}

function overLimit(address: string): boolean {
  const now = Date.now();
  let window = counters.get(address);
  if (!window || now - window.start >= WINDOW_MS) {
    if (!window && counters.size >= MAX_COUNTERS) {
      counters.delete(counters.keys().next().value!);
    }
    window = { start: now, count: 0 };
    counters.set(address, window);
  }
  window.count += 1;
  return window.count > LIMIT;
}

const iso = (d: Date | null) => (d === null ? null : new Date(d).toISOString());
const traceparent = (trace: string) => `00-${trace}-${hex(8)}-01`;

const OUTBOX = `INSERT INTO "jadpo_outbox" ("id", "event", "payload", "emitted_at", "traceparent", "encoding", "depth") SELECT "o"."id", "o"."event", "o"."payload"::jsonb, $4, $5, $6, $7 FROM unnest($1::uuid[], $2::text[], $3::text[]) AS "o"("id", "event", "payload")`;
const DELIVERIES = `INSERT INTO "jadpo_deliveries" ("id", "outbox_id", "subscriber", "status", "attempts", "next_attempt_at") SELECT "d"."id", "d"."outbox_id", "d"."subscriber", 'pending', 0, $4 FROM unnest($1::uuid[], $2::uuid[], $3::text[]) AS "d"("id", "outbox_id", "subscriber")`;
const CLAIM = `UPDATE "jadpo_deliveries" SET "status" = 'running', "attempts" = "attempts" + 1, "last_tried_at" = $2, "next_attempt_at" = $3, "last_failure" = CASE WHEN "status" = 'running' THEN $4::jsonb ELSE "last_failure" END WHERE "id" = $1 AND "status" IN ('pending', 'running') AND "next_attempt_at" <= $2 RETURNING "attempts"`;
const HANDLED = `SELECT "id" FROM "jadpo_handled_deliveries" WHERE "id" = $1`;
const MARK_HANDLED = `INSERT INTO "jadpo_handled_deliveries" ("id", "handled_at") VALUES ($1, $2)`;
const SETTLE = `UPDATE "jadpo_deliveries" SET "status" = $3, "next_attempt_at" = $4, "last_failure" = $5 WHERE "id" = $1 AND "status" IN ('running', 'sending') AND "attempts" = $2 RETURNING "id"`;
const REPAIR_COLUMNS = `"id", "item", "symptom", "bench_id", "ticket_printed_at", "booked_at"`;
const LEASE = 120_000;
const LEASE_EXPIRED = `{"code": "lease_expired"}`;

// Claims a delivery as the runtime does before running its subscriber:
// false when another has it or it was handled already.
async function claim(id: string): Promise<number | null> {
  const now = new Date();
  const claimed = await q(CLAIM, [id, now, new Date(now.getTime() + LEASE), LEASE_EXPIRED]);
  if (claimed.length === 0) return null;
  const handled = await q(HANDLED, [id]);
  return handled.length === 0 ? (claimed[0].attempts as number) : null;
}

async function book(item: string, symptom: string, trace: string) {
  const now = new Date();
  const repairId = Bun.randomUUIDv7();
  const bookedEvent = Bun.randomUUIDv7();
  const findBench = Bun.randomUUIDv7();
  // The action: the repair, its event and the event's delivery, at once.
  await sql.begin("ISOLATION LEVEL READ COMMITTED", async (tx) => {
    const t = (text: string, params: unknown[]) => tx.unsafe(text, params as never[], { prepare });
    await t(
      `INSERT INTO "repair" (${REPAIR_COLUMNS})\nVALUES ($1, $2, $3, $4, $5, $6)\nRETURNING ${REPAIR_COLUMNS}`,
      [repairId, item, symptom, null, null, now],
    );
    const emitted = new Date();
    await t(OUTBOX, [[bookedEvent], ["type cafe.RepairBooked"], [JSON.stringify({ repair_id: repairId, item })], emitted, traceparent(trace), 1, 0]);
    await t(DELIVERIES, [[findBench], [bookedEvent], ["subscriber find_a_bench"], emitted]);
  });

  // `settle:`: find_a_bench, then what it emits, while the request waits.
  const attempts = await claim(findBench);
  if (attempts !== null) {
    const assignedEvent = Bun.randomUUIDv7();
    const printDelivery = Bun.randomUUIDv7();
    await sql.begin("ISOLATION LEVEL REPEATABLE READ", async (tx) => {
      const t = (text: string, params: unknown[]) => tx.unsafe(text, params as never[], { prepare });
      const [repair] = await t(`SELECT ${REPAIR_COLUMNS}\n\tFROM "repair"\n\tWHERE "id" = $1`, [repairId]);
      if (!repair) throw new Error("repair_not_found");
      const [bench] = await t(`SELECT "id", "item", "fixer"\n\tFROM "bench"\n\tWHERE "item" = $1\n\t  AND "fixer" IS NOT NULL`, [repair.item]);
      if (!bench) throw new Error("no_fixer_free");
      await t(`UPDATE "repair"\n\tSET "bench_id" = $1\n\tWHERE "id" = $2\n\t  AND $3\n\tRETURNING ${REPAIR_COLUMNS}`, [bench.id, repairId, true]);
      const emitted = new Date();
      await t(OUTBOX, [[assignedEvent], ["type cafe.BenchAssigned"], [JSON.stringify({ repair_id: repairId, bench_id: bench.id })], emitted, traceparent(trace), 1, 1]);
      await t(DELIVERIES, [[printDelivery], [assignedEvent], ["subscriber print_on_assignment"], emitted]);
      await t(MARK_HANDLED, [findBench, emitted]);
    });
    await q(SETTLE, [findBench, attempts, "succeeded", null, null]);

    const printed = await claim(printDelivery);
    if (printed !== null) {
      await sql.begin("ISOLATION LEVEL READ COMMITTED", async (tx) => {
        const t = (text: string, params: unknown[]) => tx.unsafe(text, params as never[], { prepare });
        await t(`UPDATE "repair"\n\tSET "ticket_printed_at" = $1\n\tWHERE "id" = $2\n\t  AND $3\n\tRETURNING ${REPAIR_COLUMNS}`, [now, repairId, true]);
        await t(MARK_HANDLED, [printDelivery, new Date()]);
      });
      await q(SETTLE, [printDelivery, printed, "succeeded", null, null]);
    }
  }

  // `answer: ticket_for(result.id)`.
  return sql.begin("ISOLATION LEVEL REPEATABLE READ", async (tx) => {
    const [ticket] = await tx.unsafe(
      `SELECT "id", "item", "bench_id", "ticket_printed_at"\n\tFROM "repair"\n\tWHERE "id" = $1`,
      [repairId] as never[],
      { prepare },
    );
    return ticket;
  });
}

const server = Bun.serve({
  port: Number(process.env.PORT ?? 8080),
  hostname: process.env.HOST ?? "127.0.0.1",
  async fetch(request, server) {
    const requestId = `req_${hex(8)}`;
    const url = new URL(request.url);
    // Outside the rate limits, its answer reused for a second, as the
    // runtime's is (jadpo_runtime::http::health).
    if (request.method === "GET" && url.pathname === "/health/ready") {
      if (Date.now() - readyAt >= 1000) {
        await q("SELECT 1", []);
        readyAt = Date.now();
      }
      return json(200, { status: "ready", stores: { main: "ready" }, integrations: {} }, requestId, {
        "cache-control": "no-store",
      });
    }
    if (overLimit(clientAddress(request, server))) {
      return failure(429, "rate_limited", "Too many requests.", requestId);
    }
    if (request.method !== "POST" || url.pathname !== "/repairs") {
      return failure(404, "not_found", "No route answers this request.", requestId);
    }
    let input: unknown;
    try {
      input = await request.json();
    } catch {
      return failure(400, "invalid_request", "Request could not be read.", requestId);
    }
    if (typeof input !== "object" || input === null || Array.isArray(input)) {
      return failure(400, "invalid_request", "Request could not be read.", requestId);
    }
    const { item, symptom } = input as Record<string, unknown>;
    const problems: { field: string; rule: string }[] = [];
    for (const field of Object.keys(input)) {
      if (field !== "item" && field !== "symptom") problems.push({ field, rule: "unknown" });
    }
    if (typeof item !== "string" || !ITEMS.has(item)) problems.push({ field: "item", rule: "type" });
    if (typeof symptom !== "string") problems.push({ field: "symptom", rule: "type" });
    else {
      const length = [...symptom].length;
      if (length < 3) problems.push({ field: "symptom", rule: "min_length" });
      else if (length > 300) problems.push({ field: "symptom", rule: "max_length" });
    }
    if (problems.length > 0) {
      return failure(422, "invalid_input", "Request input is invalid.", requestId, { problems });
    }
    const ticket = await book(item as string, symptom as string, hex(16));
    return json(
      201,
      { id: ticket.id, item: ticket.item, bench_id: ticket.bench_id, ticket_printed_at: iso(ticket.ticket_printed_at) },
      requestId,
    );
  },
  error() {
    return new Response(JSON.stringify({ error: { code: "internal", message: "Something went wrong." } }), {
      status: 500,
      headers: { "content-type": "application/json" },
    });
  },
});

console.log(JSON.stringify({ event: "started", address: `${server.hostname}:${server.port}` }));
