import { describe, expect, it } from "vitest";
import {
  CreateTodo,
  PatchTodo,
  TodoView,
  ListTodos,
  Instant,
  Id,
  Revision,
} from "../src/schemas.ts";
import { loadConfig, ConfigError } from "../src/config.ts";
import { selectCredential } from "../src/auth.ts";
import { Admission, RequestScope } from "../src/runtime.ts";
import { AppError, publicFailure, safeLogRecord } from "../src/errors.ts";

describe("strict public boundaries", () => {
  it("CREATE-002 / REL-003 rejects ownership, lifecycle and generated inputs", () => {
    for (const field of [
      "owner_id",
      "deleted_at",
      "reminder_revision",
      "created_at",
      "email",
    ]) {
      expect(
        CreateTodo.safeParse({ title: "Safe", [field]: "injected" }).success,
      ).toBe(false);
      expect(PatchTodo.safeParse({ [field]: "injected" }).success).toBe(false);
    }
  });
  it("PATCH-001/002/003 preserves omission, null and empty patch", () => {
    expect(PatchTodo.parse({})).toEqual({});
    expect(Object.hasOwn(PatchTodo.parse({ title: "Edited" }), "due_at")).toBe(
      false,
    );
    expect(PatchTodo.parse({ due_at: null })).toEqual({ due_at: null });
  });
  it("rejects invalid input and private output instead of stripping fields", () => {
    expect(CreateTodo.safeParse({ title: "" }).success).toBe(false);
    expect(CreateTodo.safeParse({ title: "x".repeat(201) }).success).toBe(
      false,
    );
    expect(ListTodos.safeParse({ page_size: "101" }).success).toBe(false);
    expect(TodoView.safeParse({ owner_id: crypto.randomUUID() }).success).toBe(
      false,
    );
  });
});

describe("credential selection runs before any verifier", () => {
  it("AUTH-004/005 rejects valid beside invalid, duplicate slots and malformed slots", () => {
    for (const headers of [
      { cookie: "todo_session=good", authorization: "Bearer invalid" },
      { cookie: "todo_session=one; todo_session=two" },
      { authorization: "Bearer one, Bearer two" },
      { cookie: "todo_session=", authorization: "Bearer good" },
    ])
      expect(() => selectCredential(new Headers(headers))).toThrowError(
        AppError,
      );
  });
  it("AUTH-001 refuses missing and never accepts URL credentials", () => {
    expect(() => selectCredential(new Headers())).toThrowError(
      "Authentication required.",
    );
    expect(() =>
      selectCredential(new Headers({ authorization: "Basic value" })),
    ).toThrowError("Invalid credentials.");
  });
});

describe("aggregate safe startup", () => {
  const good = {
    DATABASE_URL: "postgres://localhost/golden",
    BROWSER_ORIGIN: "https://todo.example.test",
    OIDC_ISSUER: "https://identity.example/tenant/",
    OIDC_AUDIENCE: "golden-todo",
    SESSION_SIGNING_KEY: "x".repeat(43),
    MAIL_API_KEY: "private-provider-secret",
  };
  it("CONFIG-001/002 reports all declarations without values", () => {
    try {
      loadConfig({
        ...good,
        DATABASE_URL: "postgres://user:password@",
        OIDC_ISSUER: "not a url",
        MAIL_API_KEY: undefined,
      });
      throw Error("accepted");
    } catch (error) {
      expect(error).toBeInstanceOf(ConfigError);
      expect(JSON.stringify(error)).not.toMatch(
        /password|private-provider-secret|not a url/,
      );
      expect(
        (error as ConfigError).problems.map((p) => p.variable).sort(),
      ).toEqual(["DATABASE_URL", "MAIL_API_KEY", "OIDC_ISSUER"]);
    }
  });
  it("retains explicitly configured local sockets", () => {
    expect(
      loadConfig({
        ...good,
        DATABASE_URL: "postgres:///golden?host=/var/run/postgresql",
      }).databaseUrl,
    ).toContain("/var/run/postgresql");
  });
});

describe("request lifetime", () => {
  it("admission releases exactly once and bounds simultaneous work", () => {
    const admission = new Admission(1);
    const release = admission.enter();
    expect(() => admission.enter()).toThrowError("Unavailable.");
    release();
    release();
    expect(admission.active).toBe(0);
    admission.enter()();
    expect(admission.active).toBe(0);
  });
  it("links client cancellation and rejects further work", () => {
    const abort = new AbortController();
    const scope = new RequestScope(abort.signal, 1000);
    abort.abort();
    expect(scope.signal.aborted).toBe(true);
    expect(() => scope.check()).toThrowError(AppError);
    scope.close();
  });
  it("redacts unexpected failures", () => {
    expect(
      JSON.stringify(
        publicFailure(
          new Error("SELECT password@ private-provider-secret"),
          "request",
        ),
      ),
    ).not.toMatch(/SELECT|password|secret/);
  });
});

it("serializes an explicit safe log whitelist even for structurally wider objects", () => {
  const event = {
    event: "request_failed",
    request_id: crypto.randomUUID(),
    code: "internal_fault",
    secret: "private-provider-secret",
  } as const;
  expect(Object.keys(safeLogRecord(event)).sort()).toEqual([
    "code",
    "event",
    "request_id",
  ]);
  expect(JSON.stringify(safeLogRecord(event))).not.toContain(
    "private-provider-secret",
  );
});

describe("Jadpo value codecs", () => {
  it("normalizes millisecond Instants before digesting decoded input", () => {
    expect(Instant.parse("2026-01-15T13:00:00.100+01:00")).toBe(
      "2026-01-15T12:00:00.100Z",
    );
    expect(Instant.parse("2026-01-15T12:00:00.000Z")).toBe(
      "2026-01-15T12:00:00Z",
    );
    for (const value of ["0000-01-01T00:00:00Z", "9999-12-30T22:00:00.999Z"])
      expect(Instant.parse(value)).toBe(value);
    for (const value of [
      "2026-01-15T12:00:00.0001Z",
      "9999-12-30T22:00:01Z",
      "9999-12-31T00:00:00Z",
      "0000-01-01T00:00:00+00:01",
      "2026-02-30T12:00:00Z",
    ])
      expect(Instant.safeParse(value).success, value).toBe(false);
  });
  it("accepts every UUID bit pattern and canonicalizes uppercase", () => {
    expect(Id.parse("ABCDEF01-2345-F678-0123-456789ABCDEF")).toBe(
      "abcdef01-2345-f678-0123-456789abcdef",
    );
    expect(Id.parse("00000000-0000-0000-0000-000000000000")).toBe(
      "00000000-0000-0000-0000-000000000000",
    );
    for (const value of [
      "abcdef012345f6780123456789abcdef",
      "abcdef01-2345-g678-0123-456789abcdef",
    ])
      expect(Id.safeParse(value).success).toBe(false);
  });
});

it("preserves the entire signed64 revision domain as canonical decimal text", () => {
  for (const value of [
    "-9223372036854775808",
    "0",
    "2147483648",
    "9007199254740993",
    "9223372036854775807",
  ])
    expect(Revision.parse(value)).toBe(value);
  for (const value of [
    "9223372036854775808",
    "-9223372036854775809",
    "01",
    "1.1",
    9007199254740992,
  ])
    expect(Revision.safeParse(value).success).toBe(false);
});
