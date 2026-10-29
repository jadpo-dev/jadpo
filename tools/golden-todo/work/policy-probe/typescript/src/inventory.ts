import { z } from "zod";
import { getTableConfig } from "drizzle-orm/pg-core";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import { apiDocument, routeSchemas } from "./app.ts";
import { reminderMail } from "./provider.ts";
import { failures } from "./errors.ts";
import * as tables from "./db/schema.ts";

export function inventory() {
  return {
    contract: "todo-v0.2",
    formatVersion: 1,
    routes: routeSchemas.map((route) => ({
      method: route.method,
      path: route.path,
      access: route.access,
      effect: route.effect,
      inputSchema: z.toJSONSchema(route.input, {
        unrepresentable: "any",
        io: "input",
      }),
      outputSchema: z.toJSONSchema(route.output, {
        unrepresentable: "any",
        io: "output",
      }),
    })),
    openapi: apiDocument(),
    data: Object.values(tables).map((table) => {
      const config = getTableConfig(table);
      return {
        table: config.name,
        columns: config.columns.map((column) => ({
          name: column.name,
          type: column.getSQLType(),
          nullable: !column.notNull,
        })),
        relationships: config.foreignKeys.map((key) => {
          const reference = key.reference();
          return {
            columns: reference.columns.map((column) => column.name),
            target: getTableConfig(reference.foreignTable).name,
            fields: reference.foreignColumns.map((column) => column.name),
            onDelete: key.onDelete,
          };
        }),
      };
    }),
    failures: Object.entries(failures).map(([code, [status, message]]) => ({
      code,
      status,
      message,
    })),
    runtimeProbes: {
      paths: ["/health/live", "/health/ready"],
      applicationApi: false,
      authentication: false,
      rateLimits: false,
      cacheMs: 1000,
    },
    effectBoundaries: {
      identity: {
        issuer: "config.issuer",
        discovery: "public HTTPS",
        jwks: "discovery metadata; public HTTPS; distinct host permitted",
      },
      reminderMail: {
        ...reminderMail,
        credential: "config.mailApiKey",
        payload: "MailPayload",
        receipt: "MailReceipt",
        windowMs: 86400000,
        safeWindowMs: 82800000,
      },
    },
    secretSinks: {
      DATABASE_URL: "Store",
      SESSION_SIGNING_KEY: "AuthenticationService",
      SESSION_SIGNING_KEY_PREVIOUS: "AuthenticationService",
      MAIL_API_KEY: "Jobs → Egress header",
    },
    guarantees: [
      {
        claim:
          "Strict public input and output; NFC scalar text; private field exclusion",
        classification: "runtime validated",
        enforcedBy: ["schemas.ts", "app.ts", "todos.ts"],
        tests: [
          "boundaries.test.ts",
          "http.postgres.ts",
          "output rollback negative",
        ],
      },
      {
        claim:
          "Closed user/service principal and ordinary ownership update rejection",
        classification: "static rejection plus runtime validation",
        enforcedBy: ["schemas.ts", "policy.ts", "auth.ts"],
        tests: ["REL-003 production TypeScript diagnostic probe"],
      },
      {
        claim:
          "Policy filters before query limits, concealed lookup, bounded child query",
        classification: "test supported; general proof unavailable",
        enforcedBy: ["todos.ts", "policy.ts"],
        tests: ["shared READ/LIST/REL cases"],
      },
      {
        claim:
          "No provider IO in transactions; acknowledgement fencing and immutable preparation",
        classification: "runtime validated and test supported",
        enforcedBy: [
          "jobs.ts",
          "db/store.ts",
          "transaction-context.ts",
          "egress.ts",
        ],
        tests: [
          "shared JOB-001–004",
          "provider-in-transaction negative",
          "provider.test.ts",
          "jobs.postgres.ts",
        ],
      },
      {
        claim: "Secret-safe log/output projection",
        classification: "runtime validated; no general information-flow proof",
        enforcedBy: ["errors.ts", "schemas.ts"],
        tests: ["safe log whitelist", "configuration/output negatives"],
      },
      {
        claim:
          "Network/SQL capability import boundaries and forgotten promises",
        classification: "static lint rejection",
        enforcedBy: ["eslint.config.mjs"],
        tests: ["architecture.test.ts"],
      },
      {
        claim: "Default rate counters have a16MiB accounting budget",
        classification:
          "operationally enforced accounting; engine heap overhead not proved",
        enforcedBy: ["limits.ts"],
        tests: ["limits.test.ts"],
      },
    ],
    outstanding: [
      "Arbitrary handler policy correctness and information flow are not proved by TypeScript or the route effect labels.",
      "Real public DNS/TLS/edge deployment is outside local fixture evidence.",
      "Conformance and full repository-gate results must identify the own checkout and exact dependency histories; independent review remains separate.",
    ],
  };
}
if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === resolve(process.argv[1])
)
  process.stdout.write(JSON.stringify(inventory(), null, 2) + "\n");
