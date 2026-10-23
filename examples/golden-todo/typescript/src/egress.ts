import { lookup } from "node:dns/promises";
import { request } from "node:https";
import { BlockList, isIP } from "node:net";
import { AppError } from "./errors.ts";
import type { RequestScope } from "./runtime.ts";

const denied = new BlockList();
const deniedV6 = new BlockList();
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
for (const [base, prefix] of [
  ["::", 96],
  ["::ffff:0:0", 96],
  ["64:ff9b::", 96],
  ["64:ff9b:1::", 48],
  ["100::", 64],
  ["2001::", 23],
  ["2001:db8::", 32],
  ["2002::", 16],
  ["fc00::", 7],
  ["fe80::", 10],
  ["fec0::", 10],
  ["ff00::", 8],
] as const)
  deniedV6.addSubnet(base, prefix, "ipv6");
export function publicAddress(address: string): boolean {
  const family = isIP(address);
  return (
    family !== 0 &&
    !(family === 4
      ? denied.check(address, "ipv4")
      : deniedV6.check(address, "ipv6"))
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
    (url.port && url.port !== "443") ||
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
}>;
export type Transport = (
  input: OutboundRequest,
  scope: RequestScope,
) => Promise<{ status: number; body: string }>;
export const httpsTransport: Transport = async (input, scope) => {
  const url = publicHttps(input.url);
  scope.check();
  const hostname = url.hostname.replace(/^\[|\]$/g, "");
  const addresses = await lookup(hostname, { all: true });
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
          if (bytes > 524288) outgoing.destroy(new AppError("unavailable"));
          else chunks.push(chunk);
        });
        incoming.on("error", () => {
          reject(new AppError("unavailable"));
        });
        incoming.on("end", () => {
          resolve({
            status: incoming.statusCode ?? 0,
            body: Buffer.concat(chunks).toString("utf8"),
          });
        });
      },
    );
    outgoing.on("timeout", () => {
      outgoing.destroy(new AppError("unavailable"));
    });
    outgoing.on("error", () => {
      reject(new AppError("unavailable"));
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
    publicHttps(input.url);
    scope.check();
    const answer = await this.transport(input, scope);
    scope.check();
    // Transports do not follow redirects; never interpret redirects as success.
    if (answer.status >= 300 && answer.status < 400)
      throw new AppError("unavailable");
    if (Buffer.byteLength(answer.body) > 524288)
      throw new AppError("unavailable");
    return answer;
  }
}
