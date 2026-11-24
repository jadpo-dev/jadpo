import postgres from "postgres";
import { drizzle } from "drizzle-orm/postgres-js";
import type { PostgresJsDatabase } from "drizzle-orm/postgres-js";
import { AppError } from "../errors.ts";
import * as schema from "./schema.ts";

export type Database = PostgresJsDatabase<typeof schema>;
/** What a transaction body may use: the same query builder, bound to it. */
export type Tx = Parameters<Parameters<Database["transaction"]>[0]>[0];

// postgres.js reads URL query keys as startup parameters and doesn't take
// libpq's ?host=/socket/folder; pass that one as its own option.
function connection(url: string) {
  const parsed = new URL(url);
  const host = parsed.searchParams.get("host");
  if (host?.startsWith("/")) {
    parsed.searchParams.delete("host");
    return { url: parsed.toString(), host };
  }
  return { url, host: undefined };
}

/** The SQLSTATE of a PostgreSQL error, here or as its cause. */
export function databaseCode(error: unknown): string | undefined {
  if (error && typeof error === "object") {
    if (
      "code" in error &&
      typeof error.code === "string" &&
      /^[0-9A-Z]{5}$/.test(error.code)
    )
      return error.code;
    if ("cause" in error) return databaseCode(error.cause);
  }
  return undefined;
}

/** The constraint a unique or foreign-key violation broke, if it was one. */
export function brokenConstraint(
  error: unknown,
  code: "23505" | "23503",
): string | undefined {
  if (databaseCode(error) !== code) return undefined;
  let current: unknown = error;
  while (current && typeof current === "object") {
    if ("constraint_name" in current) return String(current.constraint_name);
    current = "cause" in current ? current.cause : undefined;
  }
  return undefined;
}

export class Store {
  readonly client: postgres.Sql;
  readonly db: Database;
  constructor(
    url: string,
    observe?: (query: string, parameters: readonly unknown[]) => void,
  ) {
    const target = connection(url);
    this.client = postgres(target.url, {
      ...(target.host ? { host: target.host } : {}),
      max: 8,
      connect_timeout: 2,
      idle_timeout: 20,
      onnotice: () => {},
      connection: { statement_timeout: 10000 },
      debug: observe
        ? (_connection: number, query: string, parameters: unknown[]) => {
            observe(query, parameters);
          }
        : false,
    });
    this.db = drizzle(this.client, { schema });
  }

  /** One serializable transaction, retried when PostgreSQL reports a
   * serialization conflict. A store that can't be reached is `unavailable`. */
  async transaction<T>(body: (tx: Tx) => Promise<T>): Promise<T> {
    for (let attempt = 0; ; attempt++) {
      try {
        return await this.db.transaction(body, {
          isolationLevel: "serializable",
        });
      } catch (error) {
        const code = databaseCode(error);
        if ((code === "40001" || code === "40P01") && attempt < 4) continue;
        throw unreachable(error);
      }
    }
  }

  /** A read outside any transaction. */
  async read<T>(body: (db: Database) => Promise<T>): Promise<T> {
    try {
      return await body(this.db);
    } catch (error) {
      throw unreachable(error);
    }
  }

  async close(): Promise<void> {
    await this.client.end({ timeout: 1 });
  }
}

// Failures of our own pass through; anything the database or the network
// raised becomes `unavailable`, with nothing of it reaching the client.
function unreachable(error: unknown): unknown {
  if (error instanceof AppError) return error;
  if (databaseCode(error) || isConnectionError(error))
    return new AppError("unavailable");
  return error;
}
function isConnectionError(error: unknown): boolean {
  return (
    error instanceof Error &&
    /ECONNREFUSED|ECONNRESET|CONNECT_TIMEOUT|CONNECTION_|EPIPE/.test(
      `${String((error as { code?: unknown }).code)} ${error.message}`,
    )
  );
}
