import { describe, expect, it } from "vitest";
import { Egress } from "../src/egress.ts";
import { MailProvider, ProviderFailure } from "../src/provider.ts";
import { RequestScope } from "../src/runtime.ts";
const payload = {
  to: "alice@example.test",
  title: "Original",
  todo_id: "00000000-0000-4000-8000-000000000011",
  revision: "1",
};
describe("bounded keyed provider calls", () => {
  it("retries transient failures under the same key/payload with bounded full-jitter waits", async () => {
    const calls: unknown[] = [];
    const waits: number[] = [];
    let count = 0;
    let guards = 0;
    const provider = new MailProvider(
      new Egress((input) => {
        calls.push(input);
        return Promise.resolve(
          ++count < 3
            ? { status: 503, body: "private-provider-secret" }
            : { status: 200, body: '{"accepted_at":"2026-01-15T12:00:00Z"}' },
        );
      }),
      "private-provider-secret",
      () => 0.5,
      (ms) => {
        waits.push(ms);
        return Promise.resolve();
      },
    );
    const scope = new RequestScope(new AbortController().signal);
    try {
      await provider.send(payload, "jd1_key", scope, () => {
        guards++;
        return Promise.resolve();
      });
    } finally {
      scope.close();
    }
    expect(calls).toHaveLength(3);
    expect(calls[0]).toMatchObject({
      url: "https://mail.example.invalid/v1/reminders",
      method: "POST",
      headers: { "idempotency-key": "jd1_key" },
      body: JSON.stringify({ ...payload, revision: "1" }),
    });
    expect(calls[1]).toEqual(calls[0]);
    expect(calls[2]).toEqual(calls[0]);
    expect(waits).toEqual([100, 200]);
    expect(guards).toBe(3);
  });
  it("does not retry permanent credential refusal and exposes no provider body", async () => {
    let calls = 0;
    const provider = new MailProvider(
      new Egress(() => {
        calls++;
        return Promise.resolve({
          status: 401,
          body: "private-provider-secret",
        });
      }),
      "private-provider-secret",
    );
    const scope = new RequestScope(new AbortController().signal);
    try {
      await expect(
        provider.send(payload, "jd1_key", scope, () => Promise.resolve()),
      ).rejects.toBeInstanceOf(ProviderFailure);
    } finally {
      scope.close();
    }
    expect(calls).toBe(1);
  });
  it("rechecks authority/window after waiting and before the next send", async () => {
    let calls = 0;
    let allowed = true;
    const provider = new MailProvider(
      new Egress(() => {
        calls++;
        return Promise.resolve({ status: 503, body: "" });
      }),
      "private-provider-secret",
      () => 0.5,
      () => {
        allowed = false;
        return Promise.resolve();
      },
    );
    const scope = new RequestScope(new AbortController().signal);
    try {
      await expect(
        provider.send(payload, "jd1_key", scope, () =>
          allowed ? Promise.resolve() : Promise.reject(Error("held")),
        ),
      ).rejects.toThrow("held");
    } finally {
      scope.close();
    }
    expect(calls).toBe(1);
  });
});
