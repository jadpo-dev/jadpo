import { Hono } from "hono";
import type { Context } from "hono";
import { OpenAPIHono, z } from "@hono/zod-openapi";
import { sql } from "drizzle-orm";
import type { Config } from "./config.ts";
import type { Store } from "./db/store.ts";
import { databaseCode } from "./db/store.ts";
import { AuthenticationService } from "./auth.ts";
import { Egress } from "./egress.ts";
import { TodoService } from "./todos.ts";
import { Admission, RequestScope } from "./runtime.ts";
import { uniqueJson } from "./strict-json.ts";
import { Idempotency } from "./idempotency.ts";
import { DefaultLimits } from "./limits.ts";
import type { Peer } from "./host.ts";
import type { Principal } from "./schemas.ts";
import type { Clock } from "./runtime.ts";
import {
  AppError,
  publicFailure,
  structuredLog,
  newRequestId,
} from "./errors.ts";
import type { Logger } from "./errors.ts";
import {
  CreateTodo,
  PatchTodo,
  ListTodos,
  TodoView,
  TodoPage,
  UserWithTodos,
  ExchangedToken,
  Failure,
  Id,
  IdempotencyKey,
} from "./schemas.ts";

const none = z.undefined();
export const routeSchemas = [
  {
    method: "post",
    path: "/todos",
    input: CreateTodo,
    output: TodoView,
    status: 201,
    access: "user",
    effect: "Todo.create",
  },
  {
    method: "get",
    path: "/todos",
    input: ListTodos,
    output: TodoPage,
    status: 200,
    access: "user",
    effect: "Todo.list",
  },
  {
    method: "get",
    path: "/todos/{todo_id}",
    input: none,
    output: TodoView,
    status: 200,
    access: "user",
    effect: "Todo.read",
  },
  {
    method: "patch",
    path: "/todos/{todo_id}",
    input: PatchTodo,
    output: TodoView,
    status: 200,
    access: "user",
    effect: "Todo.patch",
  },
  {
    method: "delete",
    path: "/todos/{todo_id}",
    input: none,
    output: none,
    status: 204,
    access: "user",
    effect: "Todo.soft_delete",
  },
  {
    method: "get",
    path: "/users/{user_id}/todos",
    input: none,
    output: UserWithTodos,
    status: 200,
    access: "self",
    effect: "User.read+Todo.children",
  },
  {
    method: "delete",
    path: "/users/{user_id}",
    input: none,
    output: none,
    status: 204,
    access: "fresh_self",
    effect: "User.disable",
  },
  {
    method: "post",
    path: "/auth/exchange",
    input: none,
    output: ExchangedToken,
    status: 200,
    access: "fresh_service_key",
    effect: "credential.exchange",
  },
] as const;
export function apiDocument() {
  const openapi = new OpenAPIHono();
  const registry = openapi.openAPIRegistry;
  registry.registerComponent("securitySchemes", "browser_session", {
    type: "apiKey",
    in: "cookie",
    name: "todo_session",
  });
  registry.registerComponent("securitySchemes", "bearer", {
    type: "http",
    scheme: "bearer",
  });
  for (const route of routeSchemas) {
    const body =
      route.input !== none && route.method !== "get"
        ? {
            body: {
              required: true,
              content: { "application/json": { schema: route.input } },
            },
          }
        : {};
    registry.registerPath({
      method: route.method,
      path: route.path,
      security: [{ browser_session: [] }, { bearer: [] }],
      request: {
        ...body,
        ...(route.path.includes("{todo_id}")
          ? { params: z.object({ todo_id: Id }) }
          : route.path.includes("{user_id}")
            ? { params: z.object({ user_id: Id }) }
            : {}),
        ...(route.method !== "get" && route.path !== "/auth/exchange"
          ? {
              headers: z.object({
                "Idempotency-Key": IdempotencyKey.optional(),
              }),
            }
          : {}),
        ...(route.method === "get" && route.path === "/todos"
          ? { query: ListTodos }
          : {}),
      },
      responses: {
        400: {
          description: "Invalid request",
          content: { "application/json": { schema: Failure } },
        },
        404: {
          description: "Concealed not found",
          content: { "application/json": { schema: Failure } },
        },
        409: {
          description: "Conflict",
          content: { "application/json": { schema: Failure } },
        },
        413: {
          description: "Request too large",
          content: { "application/json": { schema: Failure } },
        },
        429: {
          description: "Rate limited",
          content: { "application/json": { schema: Failure } },
        },
        500: {
          description: "Internal fault",
          content: { "application/json": { schema: Failure } },
        },
        504: {
          description: "Deadline exceeded",
          content: { "application/json": { schema: Failure } },
        },
        [route.status]: {
          description: "Success",
          ...(route.status === 204
            ? {}
            : { content: { "application/json": { schema: route.output } } }),
        },
        401: {
          description: "Authentication refused",
          content: { "application/json": { schema: Failure } },
        },
        403: {
          description: "Policy refused",
          content: { "application/json": { schema: Failure } },
        },
        422: {
          description: "Invalid input",
          content: { "application/json": { schema: Failure } },
        },
        503: {
          description: "Unavailable",
          content: { "application/json": { schema: Failure } },
        },
      },
      "x-golden-policy": route.access,
      "x-golden-effect": route.effect,
    });
  }
  return openapi.getOpenAPI31Document({
    openapi: "3.1.0",
    info: { title: "Golden todo", version: "todo-v0.2" },
  });
}
async function body(request: Request, scope: RequestScope): Promise<string> {
  if (
    ["GET", "HEAD"].includes(request.method) &&
    (Number(request.headers.get("content-length") ?? 0) > 0 ||
      request.headers.has("transfer-encoding"))
  )
    throw new AppError("invalid_input");
  const reader = request.body?.getReader();
  if (!reader) return "";
  let size = 0;
  const chunks: Uint8Array[] = [];
  const cancel = () => {
    void reader.cancel().catch(() => {});
  };
  scope.signal.addEventListener("abort", cancel, { once: true });
  try {
    while (true) {
      scope.check();
      const part = await reader.read();
      scope.check();
      if (part.done) break;
      size += part.value.length;
      if (size > 1048576) {
        await reader.cancel();
        throw new AppError("request_too_large");
      }
      chunks.push(part.value);
    }
    return new TextDecoder("utf-8", { fatal: true }).decode(
      Buffer.concat(chunks),
    );
  } catch (error) {
    if (error instanceof AppError) throw error;
    throw new AppError("invalid_input");
  } finally {
    scope.signal.removeEventListener("abort", cancel);
    reader.releaseLock();
  }
}
function parse<T>(schema: z.ZodType<T>, value: unknown): T {
  const answer = schema.safeParse(value);
  if (!answer.success) throw new AppError("invalid_input");
  return answer.data;
}
export function createApplication(options: {
  config: Config;
  store: Store;
  clock?: Clock;
  egress?: Egress;
  logger?: Logger;
  admission?: Admission;
}) {
  const { config, store } = options;
  const clock = options.clock ?? Date.now;
  const auth = new AuthenticationService(
    config,
    store,
    clock,
    options.egress ?? new Egress(),
  );
  const service = new TodoService(store, clock);
  const receipts = new Idempotency(store, clock);
  const admission = options.admission ?? new Admission();
  const logger = options.logger ?? structuredLog;
  const limits = new DefaultLimits(clock);
  type AppContext = Context<{ Bindings: Peer }>;
  const address = (c: AppContext) =>
    c.env?.peer ?? c.env?.incoming?.socket.remoteAddress ?? "unknown";
  const limit = (c: AppContext, principal?: Principal) => {
    const result = limits.take(address(c), principal);
    c.header("ratelimit-policy", `"default";q=${result.limit};w=60`);
    c.header(
      "ratelimit",
      `"default";r=${result.remaining};t=${result.seconds}`,
    );
    if (result.limited) {
      c.header("retry-after", String(result.seconds));
      throw new AppError("rate_limited");
    }
  };
  const app = new Hono<{ Bindings: Peer }>();
  app.use("*", async (c, next) => {
    c.header("x-request-id", newRequestId());
    await next();
  });
  app.use("/health/*", async (c, next) => {
    c.header("cache-control", "no-store");
    await next();
  });
  for (const probe of ["/health/live", "/health/ready"])
    app.options(probe, (c) => {
      c.header("allow", "GET, HEAD, OPTIONS");
      return c.body(null, 204);
    });
  app.get("/health/live", (c) => c.json({ status: "live" }));
  let readiness:
    | {
        until: number;
        answer: {
          status: "ready" | "not_ready";
          stores: { main: "ready" | "unavailable" };
          integrations: { reminder_mail: "unchecked" };
        };
      }
    | undefined;
  app.get("/health/ready", async (c) => {
    if (readiness && readiness.until > clock())
      return c.json(
        readiness.answer,
        readiness.answer.status === "ready" ? 200 : 503,
      );
    const scope = new RequestScope(c.req.raw.signal, 2000);
    try {
      await store.run(scope, async () => {
        await store.db.execute(sql`select 1`);
      });
      readiness = {
        until: clock() + 1000,
        answer: {
          status: "ready",
          stores: { main: "ready" },
          integrations: { reminder_mail: "unchecked" },
        },
      };
      return c.json(readiness.answer);
    } catch {
      readiness = {
        until: clock() + 1000,
        answer: {
          status: "not_ready",
          stores: { main: "unavailable" },
          integrations: { reminder_mail: "unchecked" },
        },
      };
      return c.json(readiness.answer, 503);
    } finally {
      scope.close();
    }
  });
  for (const route of routeSchemas) {
    const path = route.path.replaceAll(/\{([^}]+)\}/g, ":$1");
    app.on(route.method.toUpperCase(), path, async (c) => {
      const requestId = newRequestId();
      c.header("x-request-id", requestId);
      let release: (() => void) | undefined;
      let counted = false;
      const scope = new RequestScope(c.req.raw.signal, 10000);
      return await store.run(scope, async () => {
        try {
          release = admission.enter();
          // Bounded reads, including routes whose body must be empty, happen before handlers.
          const raw = await body(c.req.raw, scope);
          const request = new Request(c.req.raw.url, {
            method: c.req.method,
            headers: c.req.raw.headers,
            ...(raw && !["GET", "HEAD"].includes(c.req.method)
              ? { body: raw }
              : {}),
          });
          if (route.path === "/auth/exchange") {
            const exchanged = await auth.exchange(request, scope);
            counted = true;
            limit(c, exchanged.principal);
            if (!exchanged.output) throw new AppError("internal_fault");
            scope.check();
            c.header("cache-control", "no-store");
            return c.json(ExchangedToken.parse(exchanged.output));
          }
          const identity = await auth.authenticate(
            request,
            scope,
            route.access === "fresh_self",
          );
          counted = true;
          limit(c, identity.principal);
          const url = new URL(request.url);
          if (route.path !== "/todos" || route.method !== "get")
            if (url.search) throw new AppError("invalid_input");
          const rawTodoId = c.req.param("todo_id");
          const rawUserId = c.req.param("user_id");
          const todoId =
            rawTodoId === undefined ? undefined : parse(Id, rawTodoId);
          const targetUser =
            rawUserId === undefined ? undefined : parse(Id, rawUserId);
          const principal = identity.principal;
          const replay: {
            value: { status: number; body: string } | undefined;
          } = { value: undefined };
          const mutate = (decoded: unknown, action: () => Promise<unknown>) =>
            receipts.execute({
              principal,
              key: request.headers.get("idempotency-key"),
              route: route.method + " " + route.path,
              decoded: { todoId, targetUser, input: decoded },
              status: route.status,
              output: route.output,
              scope,
              requestId,
              action,
              onReplay: (response) => {
                replay.value = response;
              },
            });
          let output: unknown;
          if (route.method === "get" && route.path === "/todos") {
            if (raw) throw new AppError("invalid_input");
            for (const key of url.searchParams.keys())
              if (url.searchParams.getAll(key).length !== 1)
                throw new AppError("invalid_input");
            output = await service.list(
              principal,
              parse(ListTodos, Object.fromEntries(url.searchParams)),
              scope,
            );
          } else if (route.method === "post" || route.method === "patch") {
            const contentType = request.headers
              .get("content-type")
              ?.split(";")[0]
              ?.trim()
              .toLowerCase();
            if (
              !contentType ||
              !/^application\/(?:json|[a-z0-9!#$&^_.+-]+\+json)$/.test(
                contentType,
              )
            )
              throw new AppError("invalid_request");
            let input: unknown;
            try {
              input = uniqueJson(raw, 1048576);
            } catch {
              throw new AppError("invalid_request");
            }
            if (route.method === "post") {
              const decoded = parse(CreateTodo, input);
              output = await mutate(decoded, () =>
                service.create(principal, decoded, scope),
              );
            } else {
              const decoded = parse(PatchTodo, input);
              output = await mutate(decoded, () =>
                service.patch(principal, parse(Id, todoId), decoded, scope),
              );
            }
          } else {
            if (raw) throw new AppError("invalid_input");
            if (route.method === "get")
              output = targetUser
                ? await service.userTodos(principal, targetUser, scope)
                : await service.get(principal, parse(Id, todoId), scope);
            else if (targetUser)
              output = await mutate(undefined, () =>
                service.disable(principal, targetUser, scope),
              );
            else
              output = await mutate(undefined, () =>
                service.remove(principal, parse(Id, todoId), scope),
              );
          }
          scope.check();
          if (replay.value) {
            const headers = new Headers(c.res.headers);
            if (replay.value.body)
              headers.set("content-type", "application/json");
            return new Response(replay.value.body || null, {
              status: replay.value.status,
              headers,
            });
          }
          // Mutation outputs were parsed within the transaction; this checks the common response boundary too.
          const validated = route.output.parse(output);
          if (identity.renewal && !replay.value)
            c.header(identity.renewal[0], identity.renewal[1]);
          return route.status === 204
            ? c.body(null, 204)
            : c.json(validated as object, route.status);
        } catch (error) {
          if (!counted) {
            try {
              limit(c);
            } catch (limited) {
              error = limited;
            }
          }
          const safe = publicFailure(
            !(error instanceof AppError) && databaseCode(error)
              ? new AppError("unavailable")
              : error,
            requestId,
          );
          logger({
            event: "request_failed",
            request_id: requestId,
            code: safe.body.error.code,
          });
          if (
            safe.body.error.code === "invalid_credentials" &&
            c.req.header("cookie")?.includes("todo_session=")
          )
            c.header(
              "set-cookie",
              "todo_session=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0",
            );
          return c.json(safe.body, safe.status);
        } finally {
          scope.close();
          release?.();
        }
      });
    });
  }
  app.notFound((c) => {
    const requestId = newRequestId();
    c.header("x-request-id", requestId);
    const path = new URL(c.req.url).pathname;
    const matches = routeSchemas.filter((route) =>
      new RegExp("^" + route.path.replaceAll(/\{[^}]+\}/g, "[^/]+") + "$").test(
        path,
      ),
    );
    const probe = path === "/health/live" || path === "/health/ready";
    const methods = probe
      ? ["GET", "HEAD", "OPTIONS"]
      : [...new Set(matches.map((route) => route.method.toUpperCase()))];
    if (methods.includes("GET")) methods.push("HEAD");
    if (methods.length && !methods.includes("OPTIONS")) methods.push("OPTIONS");
    try {
      if (!probe) limit(c);
      if (methods.length) c.header("allow", methods.join(", "));
      if (c.req.method === "OPTIONS" && methods.length)
        return c.body(null, 204);
      const safe = publicFailure(
        new AppError(methods.length ? "method_not_allowed" : "route_not_found"),
        requestId,
      );
      return c.json(safe.body, safe.status);
    } catch (error) {
      const safe = publicFailure(error, requestId);
      return c.json(safe.body, safe.status);
    }
  });
  app.onError((_error, c) => {
    const requestId = newRequestId();
    const safe = publicFailure(new AppError("internal_fault"), requestId);
    c.header("x-request-id", requestId);
    return c.json(safe.body, 500);
  });
  return { app, auth, service, admission, limits };
}
