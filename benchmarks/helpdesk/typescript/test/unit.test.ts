import { describe, expect, it } from "vitest";
import { parseDuration } from "../src/config.ts";
import { open, selectCredential, sign } from "../src/auth.ts";
import { inventory } from "../src/inventory.ts";
import { Jobs } from "../src/jobs.ts";
import type { Desk } from "../src/desk.ts";
import { AppError } from "../src/errors.ts";
import {
  CreateCustomer,
  ListTickets,
  NewMember,
  PatchCustomer,
  decode,
} from "../src/schemas.ts";
import { instant } from "../src/views.ts";

const problems = (run: () => unknown) => {
  try {
    run();
  } catch (error) {
    if (error instanceof AppError) return error.problems;
  }
  throw Error("expected invalid_input");
};

describe("input decoding", () => {
  it("names each field and rule, declared fields first, unknown ones by name", () => {
    expect(
      problems(() =>
        decode(NewMember, {
          user_id: "x",
          role: "boss",
          display_name: "",
          zeta: 1,
          alpha: 2,
        }),
      ),
    ).toEqual([
      { field: "user_id", rule: "format" },
      { field: "role", rule: "variant" },
      { field: "display_name", rule: "min_length" },
      { field: "alpha", rule: "unknown_field" },
      { field: "zeta", rule: "unknown_field" },
    ]);
  });
  it("tells a missing or null field from one of the wrong type", () => {
    expect(
      problems(() => decode(NewMember, { role: 5, user_id: null })),
    ).toEqual([
      { field: "user_id", rule: "required" },
      { field: "role", rule: "type" },
      { field: "display_name", rule: "required" },
    ]);
  });
  it("counts characters after composing text to NFC", () => {
    const composed = decode(CreateCustomer, {
      name: "é".repeat(120),
      email: "a@b.test",
    });
    expect(composed.name).toBe("é".repeat(120));
    expect(
      problems(() =>
        decode(CreateCustomer, { name: "x".repeat(121), email: "a@b" }),
      ),
    ).toEqual([
      { field: "name", rule: "max_length" },
      { field: "email", rule: "format" },
    ]);
  });
  it("keeps a patch's null only where the field can be empty", () => {
    expect(decode(PatchCustomer, { phone: null })).toEqual({ phone: null });
  });
  it("reads a page size from the query string within 1 to 100", () => {
    expect(decode(ListTickets, { page_size: "7" })).toEqual({ page_size: 7 });
    expect(problems(() => decode(ListTickets, { page_size: "0" }))).toEqual([
      { field: "page_size", rule: "min" },
    ]);
    expect(problems(() => decode(ListTickets, { page_size: "101" }))).toEqual([
      { field: "page_size", rule: "max" },
    ]);
    expect(problems(() => decode(ListTickets, { page_size: "x" }))).toEqual([
      { field: "page_size", rule: "type" },
    ]);
  });
});

describe("values on the wire", () => {
  it("writes instants in UTC, with milliseconds only when there are some", () => {
    expect(instant(new Date(1768478400000))).toBe("2026-01-15T12:00:00Z");
    expect(instant(new Date(1768478400250))).toBe("2026-01-15T12:00:00.250Z");
  });
  it("reads durations as settings give them", () => {
    expect(parseDuration("30d")).toBe(30 * 86400000);
    expect(parseDuration("PT24H")).toBe(86400000);
    expect(parseDuration("soon")).toBeUndefined();
  });
});

describe("credentials", () => {
  it("opens only what the signing key signed", () => {
    const key = "k".repeat(32);
    const signed = sign('{"a":1}', key);
    expect(open(signed, [key])).toBe('{"a":1}');
    expect(open(signed, ["o".repeat(32)])).toBeUndefined();
  });
  it("takes exactly one credential", () => {
    expect(() => selectCredential(new Headers())).toThrow("Sign in first.");
    expect(() =>
      selectCredential(
        new Headers({ cookie: "desk_session=a", authorization: "Bearer b" }),
      ),
    ).toThrow("Ambiguous credentials.");
    expect(
      selectCredential(new Headers({ authorization: "Bearer jadpo_token_x" })),
    ).toEqual({ kind: "bearer", value: "jadpo_token_x" });
  });
});

describe("the route inventory", () => {
  it("lists all 44 routes once each, with {name} placeholders", () => {
    const routes = inventory().routes.map((r) => `${r.method} ${r.path}`);
    expect(routes).toHaveLength(44);
    expect(new Set(routes).size).toBe(44);
    expect(routes).toContain("DELETE /tickets/{ticket_id}/tags/{tag_id}");
  });
});

describe("schedules", () => {
  it("runs on a grid from the first tick, once however many runs were missed", async () => {
    let now = 0;
    const runs: number[] = [];
    const desk = {
      flagBreaches: () => {
        runs.push(now);
        return Promise.resolve(0);
      },
      purgeBinned: () => Promise.resolve(0),
      forgetEndedSessions: () => Promise.resolve(0),
    } as unknown as Desk;
    const jobs = new Jobs(
      desk,
      () => now,
      () => {},
    );
    await jobs.tick(); // Seeds the grid: the first run is due at 5 minutes.
    now = 299999;
    await jobs.tick();
    now = 300000;
    await jobs.tick();
    now = 3600000;
    await jobs.tick();
    now = 3600001;
    await jobs.tick();
    expect(runs).toEqual([300000, 3600000]);
    jobs.reset();
    now = 7200000;
    await jobs.tick();
    expect(runs).toHaveLength(2);
  });
});
