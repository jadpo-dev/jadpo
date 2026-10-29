import { expect, it } from "vitest";
import { addressKey, DefaultLimits } from "../src/limits.ts";
it("uses the declared per-principal and per-address defaults with bounded memory", () => {
  let now = 0;
  const limits = new DefaultLimits(() => now, 4096);
  const user = {
    kind: "user",
    id: crypto.randomUUID(),
    subject: "alice",
  } as const;
  for (let i = 0; i < 600; i++)
    expect(limits.take("127.0.0.1", user).limited).toBe(false);
  expect(limits.take("127.0.0.2", user).limited).toBe(true);
  for (let i = 0; i < 120; i++)
    expect(limits.take("127.0.0.1").limited).toBe(false);
  expect(limits.take("127.0.0.1").limited).toBe(true);
  now = 60000;
  expect(limits.take("127.0.0.1").limited).toBe(false);
  for (let i = 0; i < 1000; i++) limits.take("fixture-" + i);
  expect(limits.bytes).toBeLessThanOrEqual(4096);
});
it("groups IPv6 addresses by64bits and normalizes mapped IPv4 peers", () => {
  expect(addressKey("2001:db8:abcd:1234::1")).toBe(
    addressKey("2001:db8:abcd:1234:ffff::2"),
  );
  expect(addressKey("::ffff:127.0.0.1")).toBe("127.0.0.1");
});
