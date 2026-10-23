import { Resolver } from "node:dns/promises";
import { request } from "node:https";
import { BlockList, isIP } from "node:net";
import { assertOutsideTransaction } from "./transaction-context.ts";
import { AppError } from "./errors.ts";
import type { RequestScope } from "./runtime.ts";

const denied = new BlockList();
for (const [base, prefix] of [
  ["0.0.0.0", 8],
  ["10.0.0.0", 8],
  ["100.64.0.0", 10],
  ["127.0.0.0", 8],
  ["169.254.0.0", 16],
  ["172.16.0.0", 12],
  ["192.0.0.0", 24],
  ["192.0.2.0", 24],
  ["192.168.0.0", 16],
  ["198.18.0.0", 15],
  ["198.51.100.0", 24],
  ["203.0.113.0", 24],
  ["224.0.0.0", 4],
  ["240.0.0.0", 4],
] as const)
  denied.addSubnet(base, prefix, "ipv4");
function ipv6Segments(address: string): number[] {
  let text = address;
  if (text.includes(".")) {
    const at = text.lastIndexOf(":");
    const tail = text
      .slice(at + 1)
      .split(".")
      .map(Number);
    text =
      text.slice(0, at) +
      ":" +
      ((tail[0] ?? 0) * 256 + (tail[1] ?? 0)).toString(16) +
      ":" +
      ((tail[2] ?? 0) * 256 + (tail[3] ?? 0)).toString(16);
  }
  const [left = "", right = ""] = text.split("::");
  const head = left ? left.split(":") : [];
  const tail = right ? right.split(":") : [];
  return (
    text.includes("::")
      ? [
          ...head,
          ...Array<string>(8 - head.length - tail.length).fill("0"),
          ...tail,
        ]
      : head
  ).map((part) => parseInt(part, 16));
}
export function publicAddress(address: string): boolean {
  const family = isIP(address);
  if (family === 4) return !denied.check(address, "ipv4");
  if (family !== 6) return false;
  const segments = ipv6Segments(address);
  const first = segments[0] ?? 0;
  const second = segments[1] ?? 0;
  const four = (high: number, low: number) =>
    publicAddress([high >>> 8, high & 255, low >>> 8, low & 255].join("."));
  if (
    segments.slice(0, 5).every((segment) => segment === 0) &&
    (segments[5] === 0 || segments[5] === 65535)
  )
    return four(segments[6] ?? 0, segments[7] ?? 0);
  if (
    first === 0x64 &&
    second === 0xff9b &&
    segments.slice(2, 6).every((segment) => segment === 0)
  )
    return four(segments[6] ?? 0, segments[7] ?? 0);
  if (first === 0x2002) return four(segments[1] ?? 0, segments[2] ?? 0);
  if (first === 0x2001 && second === 0)
    return (
      four(segments[2] ?? 0, segments[3] ?? 0) &&
      four(~(segments[6] ?? 0) & 65535, ~(segments[7] ?? 0) & 65535)
    );
  return !(
    (first & 0xfe00) === 0xfc00 ||
    (first & 0xffc0) === 0xfe80 ||
    (first & 0xffc0) === 0xfec0 ||
    (first & 0xff00) === 0xff00 ||
    (first === 0x2001 && second < 0x200) ||
    (first === 0x2001 && second === 0xdb8) ||
    (first === 0x64 && second === 0xff9b)
  );
}
export function publicHttps(value: string): URL {
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new AppError("unavailable");
  }
  const hostname = url.hostname.replace(/^\[|\]$/g, "");
  if (
    url.protocol !== "https:" ||
    url.username ||
    url.password ||
    url.hash ||
    hostname === "localhost" ||
    hostname.endsWith(".localhost") ||
    (isIP(hostname) && !publicAddress(hostname))
  )
    throw new AppError("unavailable");
  return url;
}
export type OutboundRequest = Readonly<{
  url: string;
  method: "GET" | "POST";
  headers: Readonly<Record<string, string>>;
  body?: string;
  maxResponseBytes?: number;
}>;
export type Transport = (
  input: OutboundRequest,
  scope: RequestScope,
) => Promise<{
  status: number;
  body: string;
  headers?: Readonly<Record<string, string>>;
}>;
export class ResponseTooLarge extends Error {
  constructor() {
    super("Response too large.");
  }
}
export const httpsTransport: Transport = async (input, scope) => {
  const url = publicHttps(input.url);
  scope.check();
  const hostname = url.hostname.replace(/^\[|\]$/g, "");
  const literal = isIP(hostname);
  const resolver = new Resolver();
  const cancel = () => {
    resolver.cancel();
  };
  scope.signal.addEventListener("abort", cancel, { once: true });
  const timer = setTimeout(cancel, Math.min(10000, scope.remaining()));
  timer.unref();
  let addresses: { address: string; family: number }[];
  try {
    if (literal) addresses = [{ address: hostname, family: literal }];
    else {
      const results = await Promise.allSettled([
        resolver.resolve4(hostname),
        resolver.resolve6(hostname),
      ]);
      addresses = results.flatMap((result, index) =>
        result.status === "fulfilled"
          ? result.value.map((address) => ({
              address,
              family: index === 0 ? 4 : 6,
            }))
          : [],
      );
    }
  } finally {
    clearTimeout(timer);
    scope.signal.removeEventListener("abort", cancel);
  }
  scope.check();
  scope.check();
  if (
    !addresses.length ||
    addresses.some((item) => !publicAddress(item.address))
  )
    throw new AppError("unavailable");
  const pinned = addresses[0];
  if (!pinned) throw new AppError("unavailable");
  return await new Promise((resolve, reject) => {
    const outgoing = request(
      url,
      {
        method: input.method,
        family: pinned.family,
        headers: input.headers,
        signal: scope.signal,
        lookup: (_hostname, _options, callback) => {
          callback(null, pinned.address, pinned.family);
        },
        timeout: Math.min(scope.remaining(), 10000),
      },
      (incoming) => {
        let bytes = 0;
        const chunks: Buffer[] = [];
        incoming.on("data", (chunk: Buffer) => {
          bytes += chunk.length;
          if (bytes > (input.maxResponseBytes ?? 524288)) {
            const error = new ResponseTooLarge();
            reject(error);
            outgoing.destroy(error);
          } else chunks.push(chunk);
        });
        incoming.on("error", (error) => {
          reject(
            error instanceof ResponseTooLarge
              ? error
              : new AppError("unavailable"),
          );
        });
        incoming.on("end", () => {
          resolve({
            status: incoming.statusCode ?? 0,
            headers:
              typeof incoming.headers["retry-after"] === "string"
                ? { "retry-after": incoming.headers["retry-after"] }
                : {},
            body: Buffer.concat(chunks).toString("utf8"),
          });
        });
      },
    );
    outgoing.on("timeout", () => {
      outgoing.destroy(new AppError("unavailable"));
    });
    outgoing.on("error", (error) => {
      reject(
        error instanceof ResponseTooLarge ? error : new AppError("unavailable"),
      );
    });
    outgoing.end(input.body);
  });
};
export class Egress {
  readonly transport: Transport;
  constructor(transport: Transport = httpsTransport) {
    this.transport = transport;
  }
  async send(input: OutboundRequest, scope: RequestScope) {
    assertOutsideTransaction();
    publicHttps(input.url);
    scope.check();
    const answer = await this.transport(input, scope);
    scope.check();
    // Transports do not follow redirects; never interpret redirects as success.
    if (answer.status >= 300 && answer.status < 400)
      throw new AppError("unavailable");
    if (Buffer.byteLength(answer.body) > (input.maxResponseBytes ?? 524288))
      throw new ResponseTooLarge();
    return answer;
  }
}
