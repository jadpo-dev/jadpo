// The HTTP application: sign-in, request reading and the failure envelope
// around each route of src/routes.ts.
import { Hono } from "hono";
import type { Context } from "hono";
import { AuthenticationService, COOKIE } from "./auth.ts";
import type { Config } from "./config.ts";
import type { Store } from "./db/store.ts";
import { Desk } from "./desk.ts";
import {
  AppError,
  invalidInput,
  newRequestId,
  publicFailure,
  structuredLog,
} from "./errors.ts";
import type { Logger, Problem } from "./errors.ts";
import { routes } from "./routes.ts";
import { uniqueJson } from "./strict-json.ts";
import type { Route } from "./routes.ts";
import { Id, problemsOf } from "./schemas.ts";

const MAX_BODY = 1048576;

async function readBody(request: Request): Promise<string> {
  const declared = Number(request.headers.get("content-length") ?? 0);
  if (declared > MAX_BODY) throw new AppError("request_too_large");
  // A GET's body never reaches a Request; that one came is enough to know.
  if (request.method === "GET" || request.method === "HEAD")
    return declared > 0 || request.headers.has("transfer-encoding") ? " " : "";
  const bytes = new Uint8Array(await request.arrayBuffer());
  if (bytes.length > MAX_BODY) throw new AppError("request_too_large");
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new AppError("invalid_request");
  }
}

/** Everything a route reads, decoded: its placeholders, then its query
 * string or body. Every problem found is reported at once. */
async function readInput(c: Context, route: Route, request: Request) {
  const problems: Problem[] = [];
  const params: Record<string, string> = {};
  const given = c.req.param();
  for (const [, name = ""] of route.path.matchAll(/\{([^}]+)\}/g)) {
    const id = Id.safeParse(decodeURIComponent(given[name] ?? ""));
    if (id.success) params[name] = id.data;
    else problems.push({ field: name, rule: "format" });
  }
  const url = new URL(request.url);
  const text = await readBody(request);
  let input: unknown = undefined;
  if (route.query) {
    if (text) throw invalidInput([{ field: "", rule: "unknown_field" }]);
    const query: Record<string, string> = {};
    for (const key of new Set(url.searchParams.keys())) {
      const values = url.searchParams.getAll(key);
      if (values.length > 1) problems.push({ field: key, rule: "type" });
      else query[key] = (values[0] ?? "").normalize("NFC");
    }
    const result = route.query.safeParse(query);
    if (result.success) input = result.data;
    else problems.push(...problemsOf(result.error, query));
  } else if (url.search.length > 1) {
    throw invalidInput(
      [...new Set(url.searchParams.keys())]
        .sort()
        .map((field) => ({ field, rule: "unknown_field" })),
    );
  }
  if (route.body) {
    const type = request.headers
      .get("content-type")
      ?.split(";")[0]
      ?.trim()
      .toLowerCase();
    if (type !== "application/json") throw new AppError("invalid_request");
    let json: unknown;
    try {
      // Strict JSON: a member named twice or an unpaired surrogate is
      // unreadable, not silently resolved.
      json = uniqueJson(text, MAX_BODY);
    } catch {
      throw new AppError("invalid_request");
    }
    // Anything but an object is `type`, and each field it must have is
    // `required`, as it would be in an empty object.
    if (json === null || typeof json !== "object" || Array.isArray(json)) {
      problems.push({ field: "", rule: "type" });
      json = {};
    }
    const result = route.body.safeParse(json);
    if (result.success) input = result.data;
    else problems.push(...problemsOf(result.error, json));
  } else if (!route.query && text) {
    // A route that reads no body was sent one.
    throw invalidInput([{ field: "", rule: "unknown_field" }]);
  }
  if (problems.length) throw invalidInput(problems);
  return { params, input };
}

export function createApplication(options: {
  config: Config;
  store: Store;
  clock?: () => number;
  logger?: Logger;
}) {
  const { config, store } = options;
  const clock = options.clock ?? Date.now;
  const logger = options.logger ?? structuredLog;
  const auth = new AuthenticationService(config, store, clock);
  const desk = new Desk(store, clock, config);
  // `/me/` is `/me`: a trailing slash names the same route.
  const app = new Hono({ strict: false });

  // An action's failure, or any fault, is logged by its code and request
  // id; refused sign-ins and unreadable input are only answered.
  const fail = (
    c: Context,
    error: unknown,
    requestId: string,
    byAction = false,
  ) => {
    const safe = publicFailure(error, requestId);
    if (byAction || safe.status === 500)
      logger({
        event: "request_failed",
        request_id: requestId,
        code: safe.body.error.code,
      });
    if (
      safe.body.error.code === "invalid_credentials" &&
      c.req.header("cookie")?.includes(`${COOKIE}=`)
    )
      c.header(
        "set-cookie",
        `${COOKIE}=; HttpOnly; Secure; SameSite=Strict; Path=/; Max-Age=0`,
      );
    return c.json(safe.body, safe.status);
  };

  for (const route of routes) {
    const path = route.path.replaceAll(/\{([^}]+)\}/g, ":$1");
    app.on(route.method, path, async (c) => {
      const requestId = newRequestId();
      c.header("x-request-id", requestId);
      try {
        const identity = await auth.authenticate(c.req.raw);
        const { params, input } = await readInput(c, route, c.req.raw);
        let output: unknown;
        try {
          output = await route.run(
            desk,
            identity.principal,
            params,
            input as never,
          );
        } catch (error) {
          return fail(c, error, requestId, true);
        }
        if (identity.renewal)
          c.header(identity.renewal[0], identity.renewal[1]);
        if (route.status === 204) return c.body(null, 204);
        return c.json(output as object, route.status);
      } catch (error) {
        return fail(c, error, requestId);
      }
    });
  }

  // A path no route has is 404; a path with routes, but not for this
  // method, is 405 with the methods it answers.
  app.notFound((c) => {
    const requestId = newRequestId();
    c.header("x-request-id", requestId);
    const path = new URL(c.req.url).pathname;
    const methods = routes
      .filter((route) =>
        new RegExp(
          "^" + route.path.replaceAll(/\{[^}]+\}/g, "[^/]+") + "$",
        ).test(path),
      )
      .map((route) => route.method);
    if (methods.length) c.header("allow", [...new Set(methods)].join(", "));
    return fail(
      c,
      new AppError(methods.length ? "method_not_allowed" : "route_not_found"),
      requestId,
    );
  });
  app.onError((error, c) => {
    const requestId = newRequestId();
    c.header("x-request-id", requestId);
    return fail(c, error, requestId);
  });
  return { app, auth, desk };
}
