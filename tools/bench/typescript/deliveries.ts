/** Specialized database-only comparison: no provider, framework or language claims.
 * Same persisted workload,100 selections, four lanes, conditional leases and
 * handled marker + receipt in one transaction. Not the golden application.
 */
import postgres from 'postgres';
import { createServer } from 'node:http';
import { v7 as uuidv7 } from 'uuid';

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
    SELECT d.id,o.payload,o.emitted_at FROM jadpo_deliveries d JOIN jadpo_outbox o ON o.id=d.outbox_id
    WHERE (d.status='pending' AND d.next_attempt_at<=${now})
       OR (d.status='running' AND d.next_attempt_at<=${now})
    ORDER BY o.emitted_at, d.id LIMIT 100`;
  const readable: { id: string; ticket: string; emitted_at: Date }[] = [];
  for (const item of due) {
    const body = item.payload as Record<string, unknown> | null;
    if (!body || typeof body !== 'object' || Object.keys(body).length !== 1 ||
        typeof body.ticket !== 'string' || [...body.ticket].length < 1 || [...body.ticket].length > 80) {
      await db`UPDATE jadpo_deliveries SET status='failed',next_attempt_at=NULL,
        last_failure=${db.json({code:'payload_unreadable'})} WHERE id=${item.id} AND status IN ('pending','running')`;
    } else { readable.push({...item, ticket: body.ticket}); }
  }
  let cursor = 0;
  let completed = 0;
  const lanes = await Promise.allSettled(Array.from({ length: 4 }, async () => {
    for (;;) {
      const item = readable[cursor++];
      if (!item) break;
      const claimed = await db`
        UPDATE jadpo_deliveries SET status='running', attempts=attempts+1,
          next_attempt_at=${lease},last_tried_at=${now}
        WHERE id=${item.id} AND status IN ('pending','running') AND next_attempt_at<=${now}
        RETURNING attempts`;
      if (!claimed.length) continue;
      if (claimed[0]!.attempts > 10 || now.getTime() - item.emitted_at.getTime() > 86400000) {
        await db`UPDATE jadpo_deliveries SET status='failed',attempts=attempts-1,next_attempt_at=NULL
          WHERE id=${item.id} AND status='running' AND attempts=${claimed[0]!.attempts}`;
        continue;
      }
      let writingHandled = false;
      try {
      await db.begin(async tx => {
        const handled = await tx`SELECT id FROM jadpo_handled_deliveries WHERE id=${item.id}`;
        if (!handled.length) {
          await tx`INSERT INTO receipt(id,ticket) VALUES(${uuidv7()},${item.ticket})`;
          writingHandled = true;
          await tx`INSERT INTO jadpo_handled_deliveries(id,handled_at) VALUES(${item.id},${now})`;
        }
      });
      await db`UPDATE jadpo_deliveries SET status='succeeded',next_attempt_at=NULL,last_failure=NULL
        WHERE id=${item.id} AND status='running' AND attempts=${claimed[0]!.attempts}`;
      completed++;
      } catch (error) {
        const code = (error as {code?:string}).code;
        const terminal = !writingHandled && (code?.startsWith('23') || code?.startsWith('22') || code?.startsWith('42'));
        await db`UPDATE jadpo_deliveries SET status=${terminal ? 'failed' : 'pending'},
          next_attempt_at=${terminal ? null : new Date(now.getTime()+1000)},
          last_failure=${db.json({code:terminal ? 'internal_fault' : 'unavailable'})}
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
