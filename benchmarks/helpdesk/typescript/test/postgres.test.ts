// The desk against a real PostgreSQL: TEST_DATABASE_URL names a database
// this suite may empty. It applies drizzle/ itself when the tables are
// missing. `npm test` leaves it out; run it with `npm run test:integration`.
import { readFileSync, readdirSync } from "node:fs";
import { afterAll, beforeAll, beforeEach, describe, expect, it } from "vitest";
import { createApplication } from "../src/app.ts";
import { digest } from "../src/auth.ts";
import { loadConfig } from "../src/config.ts";
import { Store } from "../src/db/store.ts";
import { Jobs } from "../src/jobs.ts";

const url = process.env.TEST_DATABASE_URL;
if (!url)
  throw Error(
    "TEST_DATABASE_URL required: an empty PostgreSQL database for this suite.",
  );

const NOW = Date.parse("2026-01-15T12:00:00Z");
const EARLIER = "2026-01-14T09:00:00Z";
const id = (group: number, n: number) =>
  `00000000-0000-4000-8000-${String(group).padStart(4, "0")}${String(n).padStart(8, "0")}`;
const [ANN, BEN, CAT, DAN, EVE] = [1, 2, 3, 4, 5].map((n) => id(1, n));
const [ACME, GLOBEX] = [id(2, 1), id(2, 2)];
const [M_ANN, M_BEN, M_CAT, M_DAN, M_EVE] = [1, 2, 3, 4, 5].map((n) =>
  id(3, n),
);
const [CORA, GAIL] = [id(4, 1), id(4, 2)];
const [T_OPEN, T_BIN, G_OPEN] = [id(5, 1), id(5, 2), id(5, 3)];
const NOTE = id(7, 1);
const people = { ann: ANN, ben: BEN, cat: CAT, dan: DAN, eve: EVE };
type Who = keyof typeof people;
const secret = (who: Who) =>
  "jadpo_token_" + digest(who).slice(0, 64).padEnd(64, "0");

let now = NOW;
const store = new Store(url ?? "");
const config = loadConfig({
  DATABASE_URL: url,
  BROWSER_ORIGIN: "https://desk.example.test",
  SESSION_SIGNING_KEY: "k".repeat(43),
});
const { app, desk } = createApplication({
  config,
  store,
  clock: () => now,
  logger: () => {},
});
const jobs = new Jobs(
  desk,
  () => now,
  () => {},
);

async function call(
  who: Who | null,
  method: string,
  path: string,
  body?: unknown,
) {
  const headers: Record<string, string> = {};
  if (who) headers.authorization = "Bearer " + secret(who);
  if (body !== undefined) headers["content-type"] = "application/json";
  const response = await app.fetch(
    new Request("http://desk.test" + path, {
      method,
      headers,
      ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    }),
  );
  const text = await response.text();
  return {
    status: response.status,
    headers: response.headers,
    json: (text ? JSON.parse(text) : null) as Record<string, unknown> & {
      error?: { code: string; request_id: string };
    },
    text,
  };
}
const sql = (statement: string) => store.client.unsafe(statement);

beforeAll(async () => {
  const [exists] = await sql("SELECT to_regclass('public.ticket') AS t");
  if (!exists?.t)
    for (const file of readdirSync("drizzle")
      .filter((name) => name.endsWith(".sql"))
      .sort())
      await sql(readFileSync(`drizzle/${file}`, "utf8"));
});
afterAll(async () => {
  await store.close();
});
beforeEach(async () => {
  now = NOW;
  jobs.reset();
  await sql(
    'TRUNCATE "workspace", "user", "jadpo_sessions" RESTART IDENTITY CASCADE',
  );
  for (const [who, user] of Object.entries(people)) {
    await sql(
      `INSERT INTO "user" VALUES ('${user}','${who}','active','${who}@staff.example.test','${EARLIER}')`,
    );
    await sql(
      `INSERT INTO jadpo_sessions(id,principal_id,verifier,started_at,ends_at) VALUES ('${crypto.randomUUID()}','${user}','${digest(secret(who as Who))}','${EARLIER}','2026-02-14T11:59:00Z')`,
    );
  }
  await sql(
    `INSERT INTO workspace VALUES ('${ACME}','Acme','${EARLIER}','${EARLIER}'),('${GLOBEX}','Globex','${EARLIER}','${EARLIER}')`,
  );
  const members = [
    [M_ANN, ACME, ANN, "owner"],
    [M_BEN, ACME, BEN, "admin"],
    [M_CAT, ACME, CAT, "agent"],
    [M_DAN, ACME, DAN, "viewer"],
    [M_EVE, GLOBEX, EVE, "owner"],
  ];
  for (const [m, w, u, role] of members)
    await sql(
      `INSERT INTO membership VALUES ('${m}','${w}','${u}','${role}','${role} name','${EARLIER}','${EARLIER}')`,
    );
  await sql(
    `INSERT INTO customer VALUES ('${CORA}','${ACME}','Cora','cora@c.test',NULL,'${EARLIER}','${EARLIER}'),('${GAIL}','${GLOBEX}','Gail','gail@c.test',NULL,'${EARLIER}','${EARLIER}')`,
  );
  const ticket = (t: string, w: string, c: string, deleted: string | null) =>
    sql(
      `INSERT INTO ticket(id,workspace_id,customer_id,subject,status,priority,respond_by,deleted_at,created_at,updated_at) VALUES ('${t}','${w}','${c}','Help','open','normal','2026-01-15T09:00:00Z',${deleted ? `'${deleted}'` : "NULL"},'${EARLIER}','${EARLIER}')`,
    );
  await ticket(T_OPEN, ACME, CORA, null);
  await ticket(T_BIN, ACME, CORA, "2026-01-15T12:00:00Z");
  await ticket(G_OPEN, GLOBEX, GAIL, null);
  await sql(
    `INSERT INTO note VALUES ('${NOTE}','${ACME}','${T_OPEN}','${CAT}','secret note','${EARLIER}','${EARLIER}')`,
  );
});

describe("signing in", () => {
  it("refuses a request with no credential, in the failure envelope", async () => {
    const response = await call(null, "GET", "/workspaces");
    expect(response.status).toBe(401);
    expect(response.json.error?.code).toBe("authentication_required");
    expect(response.headers.get("x-request-id")).toBe(
      response.json.error?.request_id,
    );
  });
  it("refuses a disabled person", async () => {
    await sql(`UPDATE "user" SET status='disabled' WHERE id='${DAN}'`);
    expect((await call("dan", "GET", "/me")).json.error?.code).toBe(
      "user_disabled",
    );
  });
});

describe("workspace walls", () => {
  it("shows an outsider nothing: empty lists, records not found", async () => {
    expect(
      (await call("eve", "GET", `/workspaces/${ACME}/members`)).json,
    ).toEqual([]);
    expect(
      (await call("eve", "GET", `/workspaces/${ACME}/tickets`)).json,
    ).toEqual({ items: [], next: null });
    expect((await call("eve", "GET", `/tickets/${T_OPEN}`)).status).toBe(404);
    const created = await call("eve", "POST", `/workspaces/${ACME}/customers`, {
      name: "Eve",
      email: "eve@c.test",
    });
    expect(created.status).toBe(403);
  });
  it("refuses a reference into another workspace", async () => {
    const response = await call("cat", "POST", `/workspaces/${ACME}/tickets`, {
      customer_id: GAIL,
      subject: "Wrong desk",
    });
    expect(response.status).toBe(403);
    expect(response.json.error?.code).toBe("not_permitted");
  });
});

describe("roles", () => {
  it("never shows a viewer customers or notes", async () => {
    expect(
      (await call("dan", "GET", `/workspaces/${ACME}/customers`)).json,
    ).toEqual([]);
    expect((await call("dan", "GET", `/tickets/${T_OPEN}/notes`)).json).toEqual(
      [],
    );
    const customer = await call("dan", "GET", `/tickets/${T_OPEN}/customer`);
    expect(customer.status).toBe(404);
    expect(customer.text).not.toContain("cora@c.test");
  });
  it("shows colleagues' names, and a person's email only to them", async () => {
    const members = await call("dan", "GET", `/workspaces/${ACME}/members`);
    expect(members.text).not.toContain("@staff.example.test");
    expect((await call("dan", "GET", "/me")).json.email).toBe(
      "dan@staff.example.test",
    );
  });
  it("answers 403 for a record the caller may read but not change", async () => {
    const response = await call("dan", "PATCH", `/tickets/${T_OPEN}`, {
      priority: "low",
    });
    expect(response.status).toBe(403);
  });
  it("can't give a viewer a ticket", async () => {
    const response = await call("cat", "POST", `/tickets/${T_OPEN}/assign`, {
      assignee_id: M_DAN,
    });
    expect(response.json.error?.code).toBe("assignee_cannot_work");
  });
  it("keeps the last owner", async () => {
    expect(
      (await call("ann", "DELETE", `/members/${M_ANN}`)).json.error?.code,
    ).toBe("last_owner");
    expect(
      (await call("ann", "PATCH", `/members/${M_DAN}`, { role: "owner" }))
        .status,
    ).toBe(200);
    expect((await call("ann", "DELETE", `/members/${M_ANN}`)).status).toBe(204);
  });
});

describe("tickets", () => {
  it("the first reply records first_response_at and sets the ticket pending", async () => {
    expect(
      (await call("cat", "POST", `/tickets/${T_OPEN}/replies`, { body: "Hi" }))
        .status,
    ).toBe(201);
    const ticket = (await call("cat", "GET", `/tickets/${T_OPEN}`)).json;
    expect(ticket.status).toBe("pending");
    expect(ticket.first_response_at).toBe("2026-01-15T12:00:00Z");
  });
  it("pages newest first, and the cursor continues the list", async () => {
    for (const subject of ["One", "Two", "Three"]) {
      now += 1000;
      await call("cat", "POST", `/workspaces/${ACME}/tickets`, {
        customer_id: CORA,
        subject,
      });
    }
    const first = await call(
      "cat",
      "GET",
      `/workspaces/${ACME}/tickets?page_size=2`,
    );
    const items = first.json.items as { subject: string }[];
    expect(items.map((t) => t.subject)).toEqual(["Three", "Two"]);
    const next = await call(
      "cat",
      "GET",
      `/workspaces/${ACME}/tickets?page_size=2&after=${String(first.json.next)}`,
    );
    expect(
      (next.json.items as { subject: string }[]).map((t) => t.subject),
    ).toEqual(["One", "Help"]);
    expect(next.json.next).toBeNull();
  });
  it("hides binned tickets and purges them after 30 days", async () => {
    expect((await call("ann", "GET", `/tickets/${T_BIN}`)).status).toBe(404);
    await jobs.tick();
    now = NOW + 30 * 86400000 + 3600000;
    await jobs.tick();
    const [left] = await sql(`SELECT count(*)::int AS n FROM ticket`);
    expect(left?.n).toBe(2);
  });
  it("flags open tickets nobody answered by respond_by", async () => {
    now = NOW - 600000;
    await jobs.tick();
    now = NOW;
    await jobs.tick();
    const ticket = (await call("cat", "GET", `/tickets/${T_OPEN}`)).json;
    expect(ticket.breached_at).toBe("2026-01-15T12:00:00Z");
  });
});

describe("conflicts and input", () => {
  it("names the failure a unique rule maps to", async () => {
    const body = { name: "Dup", email: "cora@c.test" };
    const response = await call(
      "cat",
      "POST",
      `/workspaces/${ACME}/customers`,
      body,
    );
    expect(response.json.error?.code).toBe("customer_email_taken");
    expect(
      (await call("eve", "POST", `/workspaces/${GLOBEX}/customers`, body))
        .status,
    ).toBe(201);
  });
  it("lists each broken rule without its value", async () => {
    const response = await call("cat", "POST", `/workspaces/${ACME}/tickets`, {
      customer_id: "nope",
      subject: "",
      colour: "red",
    });
    expect(response.json.error).toMatchObject({
      code: "invalid_input",
      details: {
        problems: [
          { field: "customer_id", rule: "format" },
          { field: "subject", rule: "min_length" },
          { field: "colour", rule: "unknown_field" },
        ],
      },
    });
  });
});
