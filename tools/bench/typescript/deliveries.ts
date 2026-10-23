/** Specialized database-only comparison: no provider, framework or language claims.
 * Same persisted workload,100 selections, four lanes, conditional leases and
 * handled marker + receipt in one transaction. Not the golden application.
 */
import postgres from 'postgres';
import { v7 as uuidv7 } from 'uuid';

const databaseUrl = process.env.DATABASE_URL;
if (!databaseUrl) throw new Error('DATABASE_URL is required');
const db = postgres(databaseUrl, { max: 4, prepare: false });
const now = new Date();
const lease = new Date(now.getTime() + 16 * 60 * 1000);
try {
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
  await Promise.all(Array.from({ length: 4 }, async () => {
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
  console.log(JSON.stringify({ selected: due.length, completed }));
} finally {
  await db.end({ timeout: 5 });
}
