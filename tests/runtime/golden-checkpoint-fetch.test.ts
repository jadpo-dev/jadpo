import { expect, test } from "bun:test";
import { installCheckpointFetchPolicy } from "../../tools/golden_checkpoint_fetch.ts";

const issuer = "https://issuer.test";
const discovery = `${issuer}/.well-known/openid-configuration`;
const jwksUri = `${issuer}/jwks`;
const jwk = { kty: "RSA", kid: "test-key", n: "AQ", e: "AQAB", alg: "RS256", use: "sig" };

test("checkpoint fetch policy permits exact loopback and OIDC targets, then rejects every other destination", async () => {
  const providerCalls: string[] = [];
  const rejectedCalls: string[] = [];
  const nativeRequests: string[] = [];
  const nativeRedirectModes: Array<RequestRedirect | undefined> = [];
  const externalFollowUps: string[] = [];
  const previousFetch = globalThis.fetch;
  const restore = installCheckpointFetchPolicy({
    issuer, jwksUri, localOrigin: () => "http://127.0.0.1:43127", jwk: () => jwk,
    providerCalls, rejectedCalls,
    nativeFetch: async (input, init) => {
      const requestUrl = input instanceof Request ? input.url : String(input);
      nativeRequests.push(requestUrl);
      nativeRedirectModes.push(init?.redirect);
      if (new URL(requestUrl).pathname === "/redirect") {
        if (init?.redirect === "error") throw new TypeError("redirect mode error");
        externalFollowUps.push("https://unexpected.example/redirect-target");
        return new Response("unexpected follow-up");
      }
      return new Response("loopback response");
    },
  });

  try {
    const local = await fetch("http://127.0.0.1:43127/checkpoint");
    expect(await local.text()).toBe("loopback response");
    await expect(fetch("http://127.0.0.1:43127/redirect", { redirect: "follow" })).rejects.toThrow(
      "redirect mode error",
    );
    const metadata = await fetch(discovery, { redirect: "error", credentials: "omit" });
    expect(await metadata.json()).toEqual({ issuer, jwks_uri: jwksUri });
    const keys = await fetch(jwksUri, { redirect: "error", credentials: "omit" });
    expect(await keys.json()).toEqual({ keys: [jwk] });
    await expect(fetch("https://unexpected.example/metadata")).rejects.toThrow(
      "Checkpoint fetch refused an unexpected destination",
    );
    await expect(fetch("http://user:secret@127.0.0.1:43127/checkpoint")).rejects.toThrow(
      "Checkpoint fetch refused an unexpected destination",
    );

    expect(providerCalls).toEqual([discovery, jwksUri]);
    expect(rejectedCalls).toEqual(["https://unexpected.example", "http://127.0.0.1:43127"]);
    expect(nativeRequests).toEqual([
      "http://127.0.0.1:43127/checkpoint",
      "http://127.0.0.1:43127/redirect",
    ]);
    expect(nativeRedirectModes).toEqual(["error", "error"]);
    expect(externalFollowUps).toEqual([]);
  } finally {
    restore();
  }

  expect(globalThis.fetch).toBe(previousFetch);
});
