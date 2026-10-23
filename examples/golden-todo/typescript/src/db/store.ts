import postgres from "postgres";
import { drizzle } from "drizzle-orm/postgres-js";
import { sql } from "drizzle-orm";
import type { PostgresJsDatabase } from "drizzle-orm/postgres-js";
import { AsyncLocalStorage } from "node:async_hooks";
import { AppError } from "../errors.ts";
import type { RequestScope } from "../runtime.ts";
import { currentTransaction, inTransaction } from "../transaction-context.ts";
import * as schema from "./schema.ts";
export type Database = Pick<
  PostgresJsDatabase<typeof schema>,
  "select" | "insert" | "update" | "delete" | "execute"
>;
const scopes = new AsyncLocalStorage<RequestScope>();
function cancellable<T extends postgres.Sql | postgres.TransactionSql>(
  client: T,
): T {
  return new Proxy(client, {
    get(target, property, receiver) {
      if (property !== "unsafe")
        return Reflect.get(target, property, receiver) as unknown;
      return (...args: Parameters<postgres.Sql["unsafe"]>) => {
        const scope = scopes.getStore();
        scope?.check();
        const query = target.unsafe(...args);
        if (scope) {
          const abort = () => {
            query.cancel();
          };
          scope.signal.addEventListener("abort", abort, { once: true });
          // Query.cancel interrupts active PostgreSQL work. Awaiters still wait for
          // the driver's cancellation response and transaction ROLLBACK cleanup.
          void query
            .finally(() => {
              scope.signal.removeEventListener("abort", abort);
            })
            .catch(() => {});
        }
        return query;
      };
    },
  });
}
// postgres-js treats URL query keys as startup parameters and does not consume
// libpq's explicit Unix-socket host. Map that already-valid URL form to its
// driver option, removing only the consumed key; pool/security settings stay here.
function postgresConnection(url: string) {
  const parsed = new URL(url);
  const host = parsed.searchParams.get("host");
  const options: { host?: string } = {};
  if (host?.startsWith("/")) {
    options.host = host;
    parsed.searchParams.delete("host");
    return { url: parsed.toString(), options };
  }
  return { url, options };
}

export class Store {
  readonly client: postgres.Sql;
  readonly db: PostgresJsDatabase<typeof schema>;
  readonly transactions: TransactionPool;
  constructor(
    url: string,
    observe?: (query: string, parameters: readonly unknown[]) => void,
  ) {
    const connection = postgresConnection(url);
    this.client = postgres(connection.url, {
      ...connection.options,
      max: 5,
      connect_timeout: 2,
      idle_timeout: 20,
      onnotice: () => {},
      debug: observe
        ? (_connection, query, parameters) => {
            observe(query, parameters);
          }
        : false,
    });
    this.db = drizzle(cancellable(this.client), { schema });
    this.transactions = new TransactionPool(
      Array.from({ length: 5 }, () =>
        postgres(connection.url, {
          ...connection.options,
          max: 1,
          connect_timeout: 2,
          idle_timeout: 20,
          onnotice: () => {},
          debug: observe
            ? (_connection, query, parameters) => {
                observe(query, parameters);
              }
            : false,
        }),
      ),
    );
  }
  async run<T>(scope: RequestScope, body: () => Promise<T>): Promise<T> {
    return await scopes.run(scope, body);
  }
  async transaction<T>(
    scope: RequestScope,
    body: (db: Database) => Promise<T>,
  ): Promise<T> {
    const existing = currentTransaction();
    if (existing?.owner === this) {
      scope.check();
      return await body(existing.database);
    }
    return await this.run(scope, async () => {
      for (let attempt = 0; ; attempt++) {
        scope.check();
        try {
          const release = await this.transactions.acquire(scope);
          const client = release.client;
          let begun = false;
          try {
            const scoped = cancellable(client);
            await scoped.unsafe("BEGIN ISOLATION LEVEL SERIALIZABLE");
            begun = true;
            const db = drizzle(scoped, { schema });
            await db.execute(
              sql`select set_config('statement_timeout', ${String(scope.remaining())}, true)`,
            );
            const value = await inTransaction(
              { owner: this, database: db, scope },
              () => body(db),
            );
            scope.check();
            await scoped.unsafe("COMMIT");
            begun = false;
            return value;
          } finally {
            // Cleanup is deliberately outside cancellation: the transaction must
            // finish rolling back before its exclusively owned connection returns.
            try {
              if (begun) await client.unsafe("ROLLBACK");
            } finally {
              release.release();
            }
          }
        } catch (error) {
          scope.check();
          const code = databaseCode(error);
          if ((code === "40001" || code === "40P01") && attempt < 2) continue;
          if (code === "23503") throw new AppError("relationship_conflict");
          if (error instanceof AppError) throw error;
          if (code || error instanceof postgres.PostgresError)
            throw new AppError("unavailable");
          throw error;
        }
      }
    });
  }
  async close(): Promise<void> {
    await Promise.all([
      this.client.end({ timeout: 1 }),
      ...this.transactions.clients.map((client) => client.end({ timeout: 1 })),
    ]);
  }
}
export function databaseCode(error: unknown): string | undefined {
  if (error && typeof error === "object") {
    if ("code" in error && typeof error.code === "string") return error.code;
    if ("cause" in error) return databaseCode(error.cause);
  }
  return undefined;
}

// Exclusive clients make BEGIN itself cancellable without relying on the driver's
// internal begin queue. Waiters are removed on abort, so cancelled work cannot
// start a transaction later when a connection becomes free.
class TransactionPool {
  readonly clients: postgres.Sql[];
  #free: postgres.Sql[];
  #waiters: {
    scope: RequestScope;
    resolve: (client: postgres.Sql) => void;
    reject: (error: unknown) => void;
    abort: () => void;
  }[] = [];
  constructor(clients: postgres.Sql[]) {
    this.clients = clients;
    this.#free = [...clients];
  }
  async acquire(scope: RequestScope) {
    scope.check();
    let client = this.#free.pop();
    if (!client)
      client = await new Promise<postgres.Sql>((resolve, reject) => {
        const waiter = { scope, resolve, reject, abort: () => {} };
        waiter.abort = () => {
          this.#waiters = this.#waiters.filter((item) => item !== waiter);
          reject(new AppError("unavailable"));
        };
        scope.signal.addEventListener("abort", waiter.abort, { once: true });
        this.#waiters.push(waiter);
      });
    const owned = client;
    let released = false;
    return {
      client: owned,
      release: () => {
        if (released) return;
        released = true;
        const next = this.#waiters.shift();
        if (next) {
          next.scope.signal.removeEventListener("abort", next.abort);
          next.resolve(owned);
        } else this.#free.push(owned);
      },
    };
  }
}
