import { z } from "zod";

const UNITS: Record<string, number> = {
  ms: 1,
  s: 1000,
  m: 60000,
  h: 3600000,
  d: 86400000,
};
/** A duration as `30d`, `24h`, `15m`, `30s` or `500ms`, or ISO 8601's `PT24H`. */
export function parseDuration(text: string): number | undefined {
  const short = /^(\d{1,9})(ms|s|m|h|d)$/.exec(text);
  if (short) return Number(short[1]) * (UNITS[short[2] ?? ""] ?? 0);
  const iso =
    /^P(?:(\d{1,6})D)?(?:T(?:(\d{1,7})H)?(?:(\d{1,9})M)?(?:(\d{1,9})S)?)?$/.exec(
      text,
    );
  if (!iso || text === "P" || text.endsWith("T")) return undefined;
  const [, d, h, m, s] = iso;
  return (
    Number(d ?? 0) * 86400000 +
    Number(h ?? 0) * 3600000 +
    Number(m ?? 0) * 60000 +
    Number(s ?? 0) * 1000
  );
}

function postgresUrl(value: string): boolean {
  try {
    const url = new URL(value);
    if (!["postgres:", "postgresql:"].includes(url.protocol)) return false;
    // A Unix socket is given as ?host=/path, which postgres.js reads.
    if (!url.hostname)
      return Boolean(url.searchParams.get("host")?.startsWith("/"));
    return url.pathname.length > 1;
  } catch {
    return false;
  }
}
const Origin = z.string().refine((value) => {
  try {
    const url = new URL(value);
    return url.protocol === "https:" && url.origin === value;
  } catch {
    return false;
  }
});
const Duration = z
  .string()
  .refine((value) => (parseDuration(value) ?? 0) > 0)
  .transform((value) => parseDuration(value) ?? 0);

const settings = {
  DATABASE_URL: z.string().refine(postgresUrl),
  BROWSER_ORIGIN: Origin,
  SESSION_SIGNING_KEY: z.string().regex(/^[\x21-\x7e]{32,}$/),
  SESSION_SIGNING_KEY_PREVIOUS: z
    .string()
    .regex(/^[\x21-\x7e]{32,}$/)
    .optional(),
  DELETED_TICKET_RETENTION: Duration.default(30 * 86400000),
  FIRST_RESPONSE_TARGET: Duration.default(24 * 3600000),
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

/** Every setting checked before anything starts; a refusal names each
 * variable and never its value. */
export function loadConfig(
  environment: Readonly<Record<string, string | undefined>>,
) {
  const problems: ConfigError["problems"] = [];
  for (const [variable, schema] of Object.entries(settings))
    if (!schema.safeParse(environment[variable]).success)
      problems.push({
        variable,
        problem:
          environment[variable] === undefined
            ? "setting_missing"
            : "setting_invalid",
      });
  if (problems.length) throw new ConfigError(problems);
  const parsed = z.object(settings).parse(environment);
  return Object.freeze({
    databaseUrl: parsed.DATABASE_URL,
    browserOrigin: parsed.BROWSER_ORIGIN,
    signingKey: parsed.SESSION_SIGNING_KEY,
    previousKey: parsed.SESSION_SIGNING_KEY_PREVIOUS,
    deletedTicketRetentionMs: parsed.DELETED_TICKET_RETENTION,
    firstResponseTargetMs: parsed.FIRST_RESPONSE_TARGET,
  });
}
export type Config = ReturnType<typeof loadConfig>;
