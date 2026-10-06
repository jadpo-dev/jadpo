import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import {
  handleRequest,
  operationalEventToOpenTelemetry,
  operationalEventToProvider,
  reportRuntimeFault,
  type OperationalLogEvent,
} from "../../examples/jadpo-seed/build/target/app.ts";

let server: ReturnType<typeof Bun.serve>;

beforeAll(() => {
  for (let attempt = 0; attempt < 20; attempt += 1) {
    const port = 30_000 + Math.floor(Math.random() * 20_000);
    try {
      server = Bun.serve({ port, fetch: handleRequest });
      return;
    } catch {
      // A parallel process won this port; try another bounded candidate.
    }
  }
  throw new Error("could not allocate an HTTP acceptance-test port");
});

afterAll(() => {
  server?.stop(true);
});

async function post(body: unknown): Promise<Response> {
  return fetch(new URL("/registrations", server.url), {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

describe("jadpo-seed generated HTTP target", () => {
  test("keeps exception canaries out of operational telemetry and provider adapters", () => {
    const canaries = [
      "credential-canary-do-not-log",
      "authorization-header-canary",
      "request-body-canary",
      "connection-string-canary",
      "customer-data-canary",
      "raw-exception-canary",
    ];
    const output: string[] = [];
    const originalError = console.error;
    const originalDebug = Bun.env.JADPO_DEBUG_TARGET_STACKS;
    console.error = (...values: unknown[]) => output.push(values.join(" "));
    delete Bun.env.JADPO_DEBUG_TARGET_STACKS;
    try {
      reportRuntimeFault(
        "RUNTIME_UNHANDLED_FAULT",
        "route:POST:/registrations",
        "src_test",
        "req_test",
        {
          credential: canaries[0],
          authorization: canaries[1],
          body: canaries[2],
          connectionString: canaries[3],
          customer: canaries[4],
          message: canaries[5],
        },
      );
    } finally {
      console.error = originalError;
      if (originalDebug === undefined) delete Bun.env.JADPO_DEBUG_TARGET_STACKS;
      else Bun.env.JADPO_DEBUG_TARGET_STACKS = originalDebug;
    }

    expect(output).toHaveLength(1);
    for (const canary of canaries) expect(output[0]).not.toContain(canary);
    const event = JSON.parse(output[0]) as OperationalLogEvent;
    expect(event).toMatchObject({
      kind: "operational_log_event",
      semanticOperationId: "route:POST:/registrations",
      attributes: {},
    });
    const telemetry = JSON.stringify(operationalEventToOpenTelemetry(event));
    const provider = JSON.stringify(operationalEventToProvider(event));
    for (const canary of canaries) {
      expect(telemetry).not.toContain(canary);
      expect(provider).not.toContain(canary);
    }
  });

  test("accepts valid input and serialises the exact output", async () => {
    const response = await post({
      email: "person@example.com",
      invite_code: "welcome_1",
    });

    expect(response.status).toBe(200);
    expect(response.headers.get("x-request-id")).toMatch(/^req_/);
    expect(await response.json()).toEqual({ email: "person@example.com" });
  });

  test("decodes and validates typed route path bindings for an inline action", async () => {
    const response = await fetch(new URL("/registrations/person%40example.com", server.url));

    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ email: "person@example.com" });
  });

  test("contains invalid typed route path values behind the safe value envelope", async () => {
    const response = await fetch(new URL("/registrations/not-an-email", server.url));
    const body = await response.json();

    expect(response.status).toBe(422);
    expect(body).toMatchObject({ error: { code: "invalid_value" } });
    expect(JSON.stringify(body)).not.toContain("not-an-email");
  });

  test("rejects malformed semantic values before the action runs", async () => {
    const response = await post({
      email: "not-an-email",
      invite_code: "welcome_1",
    });
    const body = (await response.json()) as Record<string, unknown>;

    expect(response.status).toBe(422);
    expect(body).toMatchObject({
      error: {
        code: "invalid_input",
        message: "Request body value is invalid.",
      },
    });
    expect(JSON.stringify(body)).not.toContain("not-an-email");
  });

  test("enforces constrained semantic input values", async () => {
    const response = await post({
      email: "person@example.com",
      invite_code: "x",
    });

    expect(response.status).toBe(422);
    expect(await response.json()).toMatchObject({
      error: { code: "invalid_input" },
    });
  });

  test("rejects malformed JSON with the same safe boundary envelope", async () => {
    const response = await fetch(new URL("/registrations", server.url), {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: "{",
    });

    expect(response.status).toBe(400);
    expect(await response.json()).toMatchObject({
      error: { code: "invalid_request" },
    });
  });

  test("enforces the canonical closed input shape", async () => {
    const response = await post({
      email: "person@example.com",
      invite_code: "welcome_1",
      administrator: true,
    });

    expect(response.status).toBe(422);
    expect(await response.json()).toMatchObject({
      error: { code: "invalid_input" },
    });
  });

  test("maps the declared domain failure without disclosing internal context", async () => {
    const response = await post({
      email: "person@example.com",
      invite_code: "reserved",
    });
    const body = await response.json();
    const encoded = JSON.stringify(body);

    expect(response.status).toBe(422);
    expect(body).toMatchObject({
      error: {
        code: "invite_code_rejected",
        message: "That invite code cannot be used.",
      },
    });
    expect(encoded).not.toContain('"invite_code":');
    expect(encoded).not.toContain("reserved");
    expect((body as { error: Record<string, unknown> }).error).not.toHaveProperty(
      "details",
    );
  });
});
