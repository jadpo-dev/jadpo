import { isIP } from "node:net";
import { AppError } from "./errors.ts";
import type { Principal } from "./schemas.ts";
import type { Clock } from "./runtime.ts";

export function addressKey(address: string): string {
  const lower = address.toLowerCase().split("%")[0] ?? "unknown";
  if (lower.startsWith("::ffff:") && isIP(lower.slice(7)) === 4)
    return lower.slice(7);
  if (isIP(lower) !== 6) return lower;
  const [left = "", right = ""] = lower.split("::");
  const head = left ? left.split(":") : [];
  const tail = right ? right.split(":") : [];
  const parts = lower.includes("::")
    ? [
        ...head,
        ...Array<string>(8 - head.length - tail.length).fill("0"),
        ...tail,
      ]
    : head;
  return (
    "ipv6:" +
    parts
      .slice(0, 4)
      .map((part) => parseInt(part, 16).toString(16))
      .join(":") +
    "/64"
  );
}
export type LimitResult = {
  limit: number;
  remaining: number;
  seconds: number;
  limited: boolean;
};
export class DefaultLimits {
  readonly clock: Clock;
  readonly maxBytes: number;
  #entries = new Map<string, { count: number; ends: number; bytes: number }>();
  #bytes = 0;
  constructor(clock: Clock, maxBytes = 16 * 1024 * 1024) {
    this.clock = clock;
    this.maxBytes = maxBytes;
  }
  take(address: string, principal?: Principal): LimitResult {
    const key = principal
      ? principal.kind + ":" + principal.id
      : "address:" + addressKey(address);
    const limit = principal ? 600 : 120;
    const now = this.clock();
    let entry = this.#entries.get(key);
    if (entry && entry.ends <= now) {
      this.#entries.delete(key);
      this.#bytes -= entry.bytes;
      entry = undefined;
    }
    if (!entry) {
      const bytes = 128 + Buffer.byteLength(key);
      while (this.#bytes + bytes > this.maxBytes) {
        const oldest = this.#entries.keys().next().value;
        if (oldest === undefined) throw new AppError("unavailable");
        this.#bytes -= this.#entries.get(oldest)?.bytes ?? 0;
        this.#entries.delete(oldest);
      }
      entry = { count: 0, ends: now + 60000, bytes };
      this.#entries.set(key, entry);
      this.#bytes += bytes;
    }
    entry.count++;
    return {
      limit,
      remaining: Math.max(0, limit - entry.count),
      seconds: Math.max(1, Math.ceil((entry.ends - now) / 1000)),
      limited: entry.count > limit,
    };
  }
  reset(): void {
    this.#entries.clear();
    this.#bytes = 0;
  }
  get bytes() {
    return this.#bytes;
  }
}
