import { expect, it } from "vitest";
import { exportJWK, generateKeyPair } from "jose";
import { readPublicKeys } from "../src/auth.ts";
it("validates every public key before caching a mixed JWKS", async () => {
  const pair = await generateKeyPair("EdDSA");
  const valid = {
    ...(await exportJWK(pair.publicKey)),
    alg: "EdDSA",
    kid: "good",
  };
  await expect(readPublicKeys({ keys: [valid] })).resolves.toHaveProperty(
    "keys",
  );
  for (const invalid of [
    { ...valid, kid: "bad", alg: "HS256" },
    { ...valid, kty: "unknown" },
    { ...valid, crv: "X25519" },
    { ...valid, x: "bad" },
    { ...valid, key_ops: ["verify"] },
    { ...valid, d: "secret" },
    { ...valid, y: valid.x },
    { kty: "RSA", n: "AQ", e: "Ag" },
    { kty: "EC", crv: "P-256", x: "A".repeat(43), y: "A".repeat(43) },
  ])
    await expect(readPublicKeys({ keys: [valid, invalid] })).rejects.toThrow();
});
