/** Generic microbenchmark scaffold; Bun and Node run this exact strict TypeScript. */
import { serve } from '@hono/node-server';
import { OpenAPIHono, createRoute, z } from '@hono/zod-openapi';
import { createMongoAbility, subject, type MongoAbility, type ForcedSubject } from '@casl/ability';
import { SignJWT, jwtVerify } from 'jose';
import { drizzle } from 'drizzle-orm/postgres-js';
import { pgTable, text, uuid } from 'drizzle-orm/pg-core';
import { asc, eq, sql as statement } from 'drizzle-orm';
import postgres from 'postgres';
import { v7 as uuidv7 } from 'uuid';
import { randomUUID } from 'node:crypto';
import { scalarText } from './validation.js';

const config = z.object({ DATABASE_URL: z.url(), SIGNING_KEY: z.string().min(32), PORT: z.coerce.number().int().positive() }).parse(process.env);
const key = new TextEncoder().encode(config.SIGNING_KEY);
const client = postgres(config.DATABASE_URL, { max: 20, prepare: false });
const db = drizzle(client);
const users = pgTable('user', { id: uuid().primaryKey(), handle: text().notNull().unique() });
const todos = pgTable('todo', { id: uuid().primaryKey(), owner_id: uuid().notNull().references(() => users.id), title: text().notNull() });
const samples = pgTable('sample', { id: uuid().primaryKey(), title: text().notNull(), payload: text().notNull() });
type Principal = { kind: 'user'; id: string; subject: string };
type Vars = { principal: Principal; requestId: string };
const app = new OpenAPIHono<{ Variables: Vars }>({ defaultHook: result => { if (!result.success) throw new HttpFailure(422, 'invalid_input'); } });
const title = scalarText(1, 200);
const handle = scalarText(1, 80);
const todo = z.object({ id: z.uuid(), title }).strict().openapi('TodoView');
const sample = todo.extend({ payload: scalarText(0, 65536) }).strict().openapi('SampleView');
const failure = z.object({ code: z.string(), message: z.string(), request_id: z.string() }).strict();
const messages: Record<string, string> = {
  authentication_required: 'Sign in first.', invalid_credentials: 'The credentials are invalid or have expired.',
  ambiguous_credentials: 'The request carries more than one credential.', invalid_input: 'Request input is invalid.',
  handle_taken: 'That handle is taken.', sample_not_found: 'That sample was not found.',
  fault: 'The request could not be completed.', rate_limited: 'Too many requests.',
};
class HttpFailure extends Error { readonly status: 400 | 401 | 403 | 404 | 409 | 422 | 429 | 500; readonly code: string; constructor(status: 400 | 401 | 403 | 404 | 409 | 422 | 429 | 500, code: string) { super(code); this.status = status; this.code = code; } }
type TodoSubject = { owner_id: string } & ForcedSubject<'Todo'>;
type AppAbility = MongoAbility<['read' | 'create', 'Todo' | TodoSubject]>;
const abilityFor = (principal: Principal) => createMongoAbility<AppAbility>([ { action: 'read', subject: 'Todo', conditions: { owner_id: principal.id } }, { action: 'create', subject: 'Todo', conditions: { owner_id: principal.id } } ]);
const windows = new Map<string, { start: number; count: number }>();
function rateLimit(key: string, allowance: number) {
  const now = Date.now();
  let window = windows.get(key);
  if (!window || now - window.start >= 60000) {
    if (windows.size >= 120000) windows.delete(windows.keys().next().value!);
    window = { start: now, count: 0 }; windows.set(key, window);
  }
  if (++window.count > allowance) throw new HttpFailure(429, 'rate_limited');
}
app.use('*', async (c, next) => {
  c.set('requestId', randomUUID());
  if (c.req.path !== '/api/todos' && c.req.path !== '/health/ready') rateLimit('address ' + (c.req.header('x-forwarded-for') ?? 'unknown'), 120);
  await next();
});
app.onError((error, c) => {
  const known = error instanceof HttpFailure;
  const code = known ? error.code : 'fault';
  if (!known) console.error(JSON.stringify({ level: 'error', code, request_id: c.get('requestId') }));
  return c.json({ error: failure.parse({ code, message: messages[code], request_id: c.get('requestId') }) }, known ? error.status : 500);
});
// One closed principal, credential selection before validation; bounded signed
// bearer traffic performs no credential/principal authority query on either app.
app.use('/api/todos', async (c, next) => {
  const authorization = c.req.header('authorization');
  if (!authorization) { rateLimit('address ' + (c.req.header('x-forwarded-for') ?? 'unknown'), 120); throw new HttpFailure(401, 'authentication_required'); }
  if (authorization.includes(',')) throw new HttpFailure(401, 'ambiguous_credentials');
  if (!authorization.startsWith('Bearer ')) throw new HttpFailure(401, 'invalid_credentials');
  try {
    const { payload } = await jwtVerify(authorization.slice(7), key, { algorithms: ['HS256'], issuer: 'jadpo-bench', audience: 'jadpo-bench' });
    const claims = z.object({ sub: z.uuid(), handle: z.string(), iat: z.number(), exp: z.number() }).parse(payload);
    if (claims.exp - claims.iat > 300) throw new Error('lifetime');
    c.set('principal', { kind: 'user', id: claims.sub, subject: claims.handle });
  } catch { throw new HttpFailure(401, 'invalid_credentials'); }
  rateLimit('user ' + c.get('principal').id, 600);
  await next();
});
const register = createRoute({ method: 'post', path: '/api/register', request: { body: { content: { 'application/json': { schema: z.object({ handle }).strict() } } } }, responses: { 201: { description: 'Registered', content: { 'application/json': { schema: z.object({ id: z.uuid(), token: z.string() }).strict() } } } } });
app.openapi(register, async c => {
  const { handle } = c.req.valid('json');
  const id = uuidv7();
  await db.transaction(async tx => {
    const inserted = await tx.insert(users).values({ id, handle }).onConflictDoNothing().returning();
    if (!inserted.length) throw new HttpFailure(409, 'handle_taken');
  });
  const token = await new SignJWT({ handle }).setProtectedHeader({ alg: 'HS256' }).setSubject(id).setIssuer('jadpo-bench').setAudience('jadpo-bench').setIssuedAt().setExpirationTime('5m').sign(key);
  return c.json({ id, token }, 201);
});
// The policy boundary supplies the mandatory scope and projection together.
function ownerScope(principal: Principal, action: 'read' | 'create') {
  if (!abilityFor(principal).can(action, subject('Todo', { owner_id: principal.id }))) throw new HttpFailure(403, 'invalid_credentials');
  return eq(todos.owner_id, principal.id);
}
async function readTodos(principal: Principal) {
  return db.select({ id: todos.id, title: todos.title }).from(todos).where(ownerScope(principal, 'read')).orderBy(statement`${todos.title} collate "C" asc`, asc(todos.id)).limit(50);
}
app.openapi(createRoute({ method: 'get', path: '/api/todos', responses: { 200: { description: 'Owner todos', content: { 'application/json': { schema: z.array(todo) } } } } }), async c => {
  return c.json(z.array(todo).parse(await readTodos(c.get('principal'))), 200);
});
app.openapi(createRoute({ method: 'post', path: '/api/todos', request: { body: { content: { 'application/json': { schema: z.object({ title }).strict() } } } }, responses: { 201: { description: 'Created', content: { 'application/json': { schema: todo } } } } }), async c => {
  const principal = c.get('principal');
  const row = { id: uuidv7(), owner_id: principal.id, title: c.req.valid('json').title };
  if (!abilityFor(principal).can('create', subject('Todo', row))) throw new HttpFailure(403, 'invalid_credentials');
  const [saved] = await db.transaction(tx => tx.insert(todos).values(row).returning({ id: todos.id, title: todos.title }));
  return c.json(todo.parse(saved), 201);
});
app.openapi(createRoute({ method: 'get', path: '/api/samples/{id}', request: { params: z.object({ id: z.uuid() }) }, responses: { 200: { description: 'Sample', content: { 'application/json': { schema: sample } } } } }), async c => {
  const [row] = await db.select().from(samples).where(eq(samples.id, c.req.valid('param').id)).limit(1);
  if (!row) throw new HttpFailure(404, 'sample_not_found');
  return c.json(sample.parse(row), 200);
});
let readyUntil = 0;
app.get('/health/ready', async c => {
  if (Date.now() >= readyUntil) { await db.execute(statement`select 1`); readyUntil = Date.now() + 1000; }
  return c.json({ status: 'ready' });
});
app.doc('/openapi.json', { openapi: '3.1.0', info: { title: 'Focused todo benchmark', version: '1' } });
// Validation and failures use one safe mapper, with no body or secret logging.
app.notFound(() => { throw new HttpFailure(404, 'sample_not_found'); });
const runtime = globalThis as unknown as { Bun?: { serve: (options: { hostname: string; port: number; fetch: typeof app.fetch }) => { stop: () => void } } };
// Check required application columns before accepting traffic. The shared DDL
// was migrated by the harness; native additionally checks its migration ledger.
await db.select().from(users).limit(0);
await db.select().from(todos).limit(0);
await db.select().from(samples).limit(0);
const server = runtime.Bun ? runtime.Bun.serve({ hostname: '127.0.0.1', port: config.PORT, fetch: app.fetch }) : serve({ hostname: '127.0.0.1', port: config.PORT, fetch: app.fetch });
async function stop() { if ('stop' in server) server.stop(); else server.close(); await client.end({ timeout: 5 }); }
process.on('SIGTERM', () => { void stop().then(() => process.exit(0)); });
