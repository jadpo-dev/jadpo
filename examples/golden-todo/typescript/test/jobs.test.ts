import { describe, expect, it } from "vitest";
import { deliveryKey, retryDisposition } from "../src/jobs.ts";

describe("durable reminder safety", () => {
  it("derives stable provider keys from a delivery, with distinct schedule identities", () => {
    const a = crypto.randomUUID();
    const b = crypto.randomUUID();
    expect(deliveryKey(a)).toBe(deliveryKey(a));
    expect(deliveryKey(a)).not.toBe(deliveryKey(b));
    expect(deliveryKey(a)).toMatch(/^jd1_[a-f0-9]{64}$/);
  });
  it("parks uncertain sends at the24h window less1h margin", () => {
    expect(retryDisposition(0, 23 * 3600000 - 1)).toBe("pending");
    expect(retryDisposition(0, 23 * 3600000)).toBe("outcome_unknown");
    expect(retryDisposition(null, 24 * 3600000)).toBe("pending");
  });
});
