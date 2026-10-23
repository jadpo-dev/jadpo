import { z } from "zod";

function connectionUrl(
  value: string,
  protocols: readonly string[],
  socket: boolean,
): boolean {
  try {
    const url = new URL(value);
    if (!protocols.includes(url.protocol) || url.hash) return false;
    if (url.port && (+url.port < 1 || +url.port > 65535)) return false;
    if (!url.hostname)
      return (
        socket &&
        Boolean(url.searchParams.get("host")?.startsWith("/")) &&
        url.pathname.length > 1
      );
    if (url.hostname.includes(" ") || url.hostname === ".") return false;
    return url.pathname.length > 1;
  } catch {
    return false;
  }
}
export const PostgresUrl = z
  .string()
  .refine((value) => connectionUrl(value, ["postgres:", "postgresql:"], true));
export const RedisUrl = z.string().refine((value) => {
  try {
    const url = new URL(value);
    return (
      ["redis:", "rediss:"].includes(url.protocol) &&
      Boolean(url.hostname) &&
      !url.hash &&
      /^\/(?:\d+)?$/.test(url.pathname || "/")
    );
  } catch {
    return false;
  }
});
const HttpsUrl = z
  .string()
  .url()
  .refine((value) => {
    try {
      const url = new URL(value);
      return (
        url.protocol === "https:" && !url.username && !url.password && !url.hash
      );
    } catch {
      return false;
    }
  });
const Origin = HttpsUrl.refine((value) => new URL(value).origin === value);
const configuration = {
  DATABASE_URL: PostgresUrl,
  BROWSER_ORIGIN: Origin,
  OIDC_ISSUER: HttpsUrl,
  OIDC_AUDIENCE: z.string().min(1).max(1024),
  SESSION_SIGNING_KEY: z.string().regex(/^[\x21-\x7e]{32,}$/),
  SESSION_SIGNING_KEY_PREVIOUS: z
    .string()
    .regex(/^[\x21-\x7e]{32,}$/)
    .optional(),
  MAIL_API_KEY: z.string().min(1),
  CACHE_URL: RedisUrl.optional(),
} as const;
export class ConfigError extends Error {
  readonly problems: {
    variable: string;
    problem: "setting_missing" | "setting_invalid";
  }[];
  constructor(problems: ConfigError["problems"]) {
    super("Configuration refused.");
    this.problems = problems;
  }
}
export function loadConfig(
  environment: Readonly<Record<string, string | undefined>>,
) {
  const problems: ConfigError["problems"] = [];
  for (const [variable, schema] of Object.entries(configuration)) {
    if (!schema.safeParse(environment[variable]).success)
      problems.push({
        variable,
        problem:
          environment[variable] === undefined
            ? "setting_missing"
            : "setting_invalid",
      });
  }
  if (problems.length) throw new ConfigError(problems);
  const parsed = z.object(configuration).parse(environment);
  return Object.freeze({
    databaseUrl: parsed.DATABASE_URL,
    browserOrigin: parsed.BROWSER_ORIGIN,
    issuer: parsed.OIDC_ISSUER,
    audience: parsed.OIDC_AUDIENCE,
    signingKey: parsed.SESSION_SIGNING_KEY,
    previousKey: parsed.SESSION_SIGNING_KEY_PREVIOUS,
    mailApiKey: parsed.MAIL_API_KEY,
    cacheUrl: parsed.CACHE_URL,
    retentionMs: 30 * 86400000,
  });
}
export type Config = ReturnType<typeof loadConfig>;
