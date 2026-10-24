/** Connected local-provider comparison, with the same durable receipt and safe GET.
 * Same persisted workload,100 selections, four lanes, conditional leases and
 * handled marker + receipt in one transaction. Not the golden application.
 */
import postgres from 'postgres';
import { createServer } from 'node:http';
import { v7 as uuidv7 } from 'uuid';

const providerUrl = process.env.PROVIDER_URL;
if (!providerUrl) throw new Error('PROVIDER_URL required');
const breakerSql = {
  "READ": "SELECT \"connection\", \"state\", \"failures\", \"probe_at\", \"probe_until\", \"pauses\" FROM \"jadpo_circuit_breakers\" WHERE \"connection\" = ANY($1::text[])",
  "TAKE_PROBE": "UPDATE \"jadpo_circuit_breakers\" SET \"state\" = 'probing', \"probe_id\" = $2, \"probe_until\" = $3 WHERE \"connection\" = $1 AND ((\"state\" = 'open' AND \"probe_at\" <= $4) OR (\"state\" = 'probing' AND (\"probe_until\" IS NULL OR \"probe_until\" <= $4))) RETURNING \"pauses\"",
  "HOLD": "UPDATE \"jadpo_deliveries\" SET \"status\" = 'held', \"attempts\" = \"attempts\" - 1, \"next_attempt_at\" = NULL, \"held_for\" = $3, \"held_since\" = $4 WHERE \"id\" = $1 AND \"status\" = 'running' AND \"attempts\" = $2 AND EXISTS (SELECT 1 FROM \"jadpo_circuit_breakers\" AS \"b\" WHERE \"b\".\"connection\" = $3 AND \"b\".\"state\" <> 'closed') RETURNING \"id\"",
  "FAILED": "INSERT INTO \"jadpo_circuit_breakers\" AS \"b\" (\"connection\", \"state\", \"failures\", \"opened_at\", \"pauses\", \"probe_at\", \"probe_id\", \"probe_until\", \"alerted_at\") VALUES ($1, 'closed', 1, NULL, 0, NULL, NULL, NULL, NULL) ON CONFLICT (\"connection\") DO UPDATE SET \"failures\" = \"b\".\"failures\" + 1, \"state\" = CASE WHEN \"b\".\"state\" = 'closed' AND \"b\".\"failures\" + 1 >= $3 THEN 'open' WHEN \"b\".\"state\" = 'probing' AND \"b\".\"probe_id\" = $5::uuid THEN 'open' ELSE \"b\".\"state\" END, \"opened_at\" = CASE WHEN \"b\".\"state\" = 'closed' AND \"b\".\"failures\" + 1 >= $3 THEN $2 ELSE \"b\".\"opened_at\" END, \"pauses\" = CASE WHEN \"b\".\"state\" = 'closed' AND \"b\".\"failures\" + 1 >= $3 THEN 1 WHEN \"b\".\"state\" = 'probing' AND \"b\".\"probe_id\" = $5::uuid THEN \"b\".\"pauses\" + 1 ELSE \"b\".\"pauses\" END, \"probe_at\" = CASE WHEN \"b\".\"state\" = 'closed' AND \"b\".\"failures\" + 1 >= $3 THEN $4 WHEN \"b\".\"state\" = 'probing' AND \"b\".\"probe_id\" = $5::uuid THEN $6 ELSE \"b\".\"probe_at\" END, \"probe_id\" = CASE WHEN \"b\".\"state\" = 'probing' AND \"b\".\"probe_id\" = $5::uuid THEN NULL ELSE \"b\".\"probe_id\" END, \"probe_until\" = CASE WHEN \"b\".\"state\" = 'probing' AND \"b\".\"probe_id\" = $5::uuid THEN NULL ELSE \"b\".\"probe_until\" END RETURNING \"state\", \"failures\"",
  "PROBE_UNUSED": "UPDATE \"jadpo_circuit_breakers\" SET \"state\" = 'open', \"probe_id\" = NULL, \"probe_until\" = NULL WHERE \"connection\" = $1 AND \"state\" = 'probing' AND \"probe_id\" = $2 RETURNING \"connection\"",
  "SUCCEEDED": "WITH \"b\" AS (UPDATE \"jadpo_circuit_breakers\" SET \"state\" = 'closed', \"failures\" = 0, \"opened_at\" = NULL, \"pauses\" = 0, \"probe_at\" = NULL, \"probe_id\" = NULL, \"probe_until\" = NULL, \"alerted_at\" = NULL WHERE \"connection\" = $1 AND (\"state\" <> 'closed' OR \"failures\" > 0) RETURNING \"connection\"), \"d\" AS (UPDATE \"jadpo_deliveries\" SET \"status\" = 'pending', \"next_attempt_at\" = $2, \"held_ms\" = COALESCE(\"held_ms\", 0) + GREATEST(0, (extract(epoch FROM ($2::timestamptz - COALESCE(\"held_since\", $2::timestamptz))) * 1000)::bigint), \"held_for\" = NULL, \"held_since\" = NULL WHERE \"status\" = 'held' AND \"held_for\" IN (SELECT \"connection\" FROM \"b\") RETURNING \"id\") SELECT (SELECT count(*) FROM \"b\"), (SELECT count(*) FROM \"d\")"
} as const;
// Bun gets the same idiomatic single-statement claim on both revisions.
const claimSql = "WITH \"claimed\" AS (UPDATE \"jadpo_deliveries\" SET \"status\" = 'running', \"attempts\" = \"attempts\" + 1, \"last_tried_at\" = $2, \"next_attempt_at\" = $3, \"last_failure\" = CASE WHEN \"status\" = 'running' THEN $4::jsonb ELSE \"last_failure\" END WHERE \"id\" = $1 AND \"status\" IN ('pending', 'running') AND \"next_attempt_at\" <= $2 RETURNING \"attempts\") SELECT \"c\".\"attempts\", \"b\".\"connection\", \"b\".\"state\", \"b\".\"failures\", \"b\".\"probe_at\", \"b\".\"probe_until\", \"b\".\"pauses\" FROM \"claimed\" AS \"c\" LEFT JOIN \"jadpo_circuit_breakers\" AS \"b\" ON \"b\".\"connection\" = ANY($5::text[]) ORDER BY \"b\".\"connection\"";
const databaseUrl = process.env.DATABASE_URL;
if (!databaseUrl) throw new Error('DATABASE_URL is required');
// Normalized shared-test socket URIs need postgres-js's explicit host option;
// consuming only this key preserves the requested endpoint/startup settings.
const parsed = new URL(databaseUrl);
const socketHost = parsed.searchParams.get('host');
const socketOptions: { host?: string } = {};
if (socketHost?.startsWith('/')) {
  socketOptions.host = socketHost;
  parsed.searchParams.delete('host');
}
const db = postgres(parsed.toString(), { ...socketOptions, max: 4, prepare: false });
async function tick() {
  const now = new Date();
  const lease = new Date(now.getTime() + 16 * 60 * 1000);
  const due = await db<{ id: string; payload: unknown; emitted_at: Date }[]>`
    SELECT d.id,o.payload,COALESCE(d.retried_at,o.emitted_at)+COALESCE(d.held_ms,0)*interval '1 millisecond' AS emitted_at FROM jadpo_deliveries d JOIN jadpo_outbox o ON o.id=d.outbox_id
    WHERE (d.status='pending' AND d.next_attempt_at<=${now})
       OR (d.status='running' AND d.next_attempt_at<=${now})
    ORDER BY d.next_attempt_at, d.id LIMIT 100`;
  const readable: { id: string; ticket: string; emitted_at: Date }[] = [];
  for (const item of due) {
    const body = item.payload as Record<string, unknown> | null;
    if (!body || typeof body !== 'object' || Object.keys(body).length !== 1 ||
        typeof body.ticket !== 'string' || [...body.ticket].length < 1 || [...body.ticket].length > 80) {
      await db`UPDATE jadpo_deliveries SET status='failed',next_attempt_at=NULL,
        last_failure=${db.json({code:'payload_unreadable',problem:'ticket must be text of 1..80 characters in a one-field object'})} WHERE id=${item.id} AND status IN ('pending','running')`;
    } else { readable.push({...item, ticket: body.ticket}); }
  }
  let cursor = 0;
  let completed = 0;
  const lanes = await Promise.allSettled(Array.from({ length: 4 }, async () => {
    for (;;) {
      const item = readable[cursor++];
      if (!item) break;
      const claimed = await db.unsafe<{attempts:number;state?:string;probe_at?:Date;probe_until?:Date}[]>(claimSql,
        [item.id,now,lease,db.json({code:'lease_expired'}),db.array(['checks'])]);
      if (!claimed.length) continue;
      const already = await db`SELECT id FROM jadpo_handled_deliveries WHERE id=${item.id}`;
      if (already.length) {
        await db`UPDATE jadpo_deliveries SET status='succeeded',next_attempt_at=NULL,last_failure=NULL
          WHERE id=${item.id} AND status='running' AND attempts=${claimed[0]!.attempts}`;
        completed++;
        continue;
      }
      if (claimed[0]!.attempts > 10 || now.getTime() - item.emitted_at.getTime() > 86400000) {
        await db`UPDATE jadpo_deliveries SET status='failed',attempts=attempts-1,next_attempt_at=NULL
          WHERE id=${item.id} AND status='running' AND attempts=${claimed[0]!.attempts}`;
        continue;
      }
      const probe = claimed[0];
      if (probe?.state && probe.state !== 'closed') {
        let taken = false;
        if ((probe.state === 'open' && (!probe.probe_at || new Date(probe.probe_at).getTime() <= now.getTime())) ||
            (probe.state === 'probing' && (!probe.probe_until || new Date(probe.probe_until).getTime() <= now.getTime()))) {
          taken = (await db.unsafe(breakerSql.TAKE_PROBE, ['checks',item.id,lease,now])).length > 0;
        }
        if (!taken && (await db.unsafe(breakerSql.HOLD, [item.id,claimed[0]!.attempts,'checks',now])).length) continue;
      }
      let writingHandled = false;
      let answered = false;
      const feedback = async () => {
        const ended = new Date();
        try {
          if (answered) await db.unsafe(breakerSql.SUCCEEDED, ['checks',ended]);
          else await db.unsafe(breakerSql.FAILED, ['checks',ended,5,new Date(ended.getTime()+60000),item.id,new Date(ended.getTime()+120000)]);
        } catch { /* Feedback failure never turns a successful action into a retry. */ }
      };
      try {
      const response = await fetch(`${providerUrl}/tickets/${encodeURIComponent(item.ticket)}`, {
        headers: { authorization: 'Bearer benchmark-key' }, signal: AbortSignal.timeout(5000),
      });
      if (response.status >= 500 || [408,429].includes(response.status)) throw {code:'provider_unavailable'};
      answered = true;
      if (response.status !== 200) throw {code:'provider_refused'};
      const payload = await response.json() as {ticket?:unknown};
      if (payload.ticket !== item.ticket) throw {code:'provider_refused'};
      await db.begin(async tx => {
        const handled = await tx`SELECT id FROM jadpo_handled_deliveries WHERE id=${item.id}`;
        if (!handled.length) {
          await tx`INSERT INTO receipt(id,ticket) VALUES(${uuidv7()},${item.ticket})`;
          writingHandled = true;
          await tx`INSERT INTO jadpo_handled_deliveries(id,handled_at) VALUES(${item.id},${now})`;
        }
      });
      await feedback();
      await db`UPDATE jadpo_deliveries SET status='succeeded',next_attempt_at=NULL,last_failure=NULL
        WHERE id=${item.id} AND status='running' AND attempts=${claimed[0]!.attempts}`;
      completed++;
      } catch (error) {
        const code = (error as {code?:string}).code;
        const terminal = code === 'provider_refused' || (!writingHandled && (code?.startsWith('23') || code?.startsWith('22') || code?.startsWith('42')));
        const ceiling = Math.min(21600000,30000*2**Math.min(20,claimed[0]!.attempts-1));
        const next = new Date(now.getTime()+Math.max(1000,Math.floor(Math.random()*(ceiling+1))));
        const again = !terminal && claimed[0]!.attempts<10 && next.getTime()<=item.emitted_at.getTime()+86400000;
        await feedback();
        await db`UPDATE jadpo_deliveries SET status=${again ? 'pending' : 'failed'},
          next_attempt_at=${again ? next : null},
          last_failure=${db.json({code:terminal ? (code === 'provider_refused' ? 'already_recorded' : 'internal_fault') : 'unavailable', ...(terminal ? (code === 'provider_refused' ? {kind:'conflict'} : {}) : {kind:'unavailable'})})}
          WHERE id=${item.id} AND status='running' AND attempts=${claimed[0]!.attempts}`;
      }
    }
  }));
  // Drain every admitted lane before returning an error or accepting shutdown.
  for (const lane of lanes) if (lane.status === 'rejected') throw lane.reason;
  return { selected: due.length, completed, pid: process.pid };
}

if (process.argv.includes('--serve')) {
  let stopping = false;
  const server = createServer(async (request, response) => {
    if (stopping) { response.writeHead(503).end(); return; }
    if (request.method !== 'POST' || request.url !== '/tick') {
      response.writeHead(404).end(); return;
    }
    try {
      const answer = await tick();
      response.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify(answer));
    } catch (error) {
      response.writeHead(500, { 'content-type': 'application/json' }).end(JSON.stringify({ error: String(error) }));
    }
  });
  server.requestTimeout = 120_000;
  server.listen(Number(process.env.PORT), '127.0.0.1');
  for (const signal of ['SIGTERM', 'SIGINT'] as const) process.once(signal, () => {
    stopping = true;
    server.close(() => { void db.end({ timeout: 5 }); });
    server.closeIdleConnections();
  });
} else {
  try { console.log(JSON.stringify(await tick())); }
  finally { await db.end({ timeout: 5 }); }
}
