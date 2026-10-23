import { expect } from "vitest";
import { CompactSign, SignJWT, exportJWK, generateKeyPair } from "jose";
import { AuthenticationService } from "../src/auth.ts";
import { Egress } from "../src/egress.ts";
import { loadConfig } from "../src/config.ts";
import { publicFailure } from "../src/errors.ts";
import { RequestScope } from "../src/runtime.ts";
import type { Store } from "../src/db/store.ts";

export async function externalNegatives(store: Store, url: string) {
  const now = 1768478400000;
  const seconds = now / 1000;
  const config = loadConfig({
    DATABASE_URL: url,
    BROWSER_ORIGIN: "https://todo.example.test",
    OIDC_ISSUER: "https://identity.example/tenant/",
    OIDC_AUDIENCE: "golden-todo",
    SESSION_SIGNING_KEY: "x".repeat(43),
    MAIL_API_KEY: "private-provider-secret",
  });
  for (const alg of ["EdDSA", "RS256", "ES256"] as const) {
    const pair = await generateKeyPair(alg);
    const jwk = {
      ...(await exportJWK(pair.publicKey)),
      kid: "local-" + alg,
      alg,
    };
    let fetched = 0;
    let providedKeys = [jwk];
    const egress = new Egress(async (input) => {
      await new Promise<void>((resolve) => {
        setImmediate(resolve);
      });
      fetched++;
      if (input.url.endsWith("/.well-known/openid-configuration"))
        return {
          status: 200,
          body: JSON.stringify({
            issuer: config.issuer,
            jwks_uri: "https://keys.example/jwks",
          }),
        };
      expect(input.url).toBe("https://keys.example/jwks");
      return { status: 200, body: JSON.stringify({ keys: providedKeys }) };
    });
    const service = new AuthenticationService(config, store, () => now, egress);
    const authenticate = async (token: string) => {
      const scope = new RequestScope(new AbortController().signal);
      try {
        return await store.run(scope, () =>
          service.authenticate(
            new Request("https://todo.example.test/todos", {
              headers: { authorization: "Bearer " + token },
            }),
            scope,
          ),
        );
      } finally {
        scope.close();
      }
    };
    const claims = {
      sub: "alice",
      iss: config.issuer,
      aud: config.audience,
      exp: seconds + 300,
    };
    const signed = async (
      payload: Record<string, unknown>,
      header: Record<string, unknown> = {},
    ) =>
      await new SignJWT(payload)
        .setProtectedHeader({ alg, kid: jwk.kid, ...header })
        .sign(pair.privateKey);
    expect(
      (
        await authenticate(
          await signed({
            ...claims,
            email: "attacker@example.test",
            scope: "admin",
          }),
        )
      ).principal,
    ).toEqual({
      kind: "user",
      id: "00000000-0000-4000-8000-000000000001",
      subject: "alice",
    });
    expect(fetched).toBe(2);
    for (const extra of [
      { exp: undefined },
      { exp: seconds - 31 },
      { exp: seconds + 300.5 },
      { nbf: seconds + 31 },
      { iat: seconds + 31 },
      { iat: 0.5 },
      { exp: 9e15 },
      { iss: "https://wrong.example/" },
      { aud: "wrong" },
    ]) {
      await expect(
        authenticate(await signed({ ...claims, ...extra })),
      ).rejects.toThrow();
    }
    const refused = await authenticate(
      await signed({ ...claims, sub: "unprovisioned" }),
    ).then(
      () => null,
      (error: unknown) => publicFailure(error, "req_0000000000000000"),
    );
    expect(refused).toMatchObject({
      status: 403,
      body: { error: { code: "sign_in_refused" } },
    });
    expect(JSON.stringify(refused)).not.toContain("unprovisioned");
    await expect(
      authenticate(await signed({ ...claims, iss: "https://wrong.example/" })),
    ).rejects.toMatchObject({ code: "invalid_credentials" });
    await expect(
      authenticate(await signed(claims, { cty: "JWT" })),
    ).rejects.toThrow();
    const raw =
      '{"sub":"alice","iss":' +
      JSON.stringify(config.issuer) +
      ',"aud":"golden-todo","exp":' +
      (seconds + 300) +
      ',"nested":{"x":1,"\\u0078":2}}';
    const duplicate = await new CompactSign(Buffer.from(raw))
      .setProtectedHeader({ alg, kid: jwk.kid })
      .sign(pair.privateKey);
    await expect(authenticate(duplicate)).rejects.toThrow();
    const critical = await new CompactSign(Buffer.from(JSON.stringify(claims)))
      .setProtectedHeader({
        alg,
        kid: jwk.kid,
        crit: ["fixture"],
        fixture: true,
      })
      .sign(pair.privateKey, { crit: { fixture: true } });
    await expect(authenticate(critical)).rejects.toThrow();
    // Ignored remote-key headers never alter the trusted source or produce egress.
    expect(
      (
        await authenticate(
          await signed(claims, {
            jku: "https://127.0.0.1/keys",
            x5u: "https://127.0.0.1/cert",
          }),
        )
      ).principal.kind,
    ).toBe("user");
    expect(fetched).toBe(2);
    await expect(
      authenticate(await signed(claims, { kid: 1 })),
    ).rejects.toThrow("Invalid credentials.");
    await expect(
      authenticate(await signed(claims, { cty: 1 })),
    ).rejects.toThrow("Invalid credentials.");
    const replacement = await generateKeyPair(alg);
    providedKeys = [
      jwk,
      {
        ...(await exportJWK(replacement.publicKey)),
        kid: "replacement-" + alg,
        alg,
      },
    ];
    service.clearKeys();
    const rollover = await new SignJWT(claims)
      .setProtectedHeader({ alg })
      .sign(replacement.privateKey);
    expect((await authenticate(rollover)).principal.kind).toBe("user");
    await store.client.unsafe(
      "UPDATE \"user\" SET status='disabled' WHERE authentication_subject='alice'",
    );
    await expect(authenticate(await signed(claims))).rejects.toThrow(
      "User disabled.",
    );
    await store.client.unsafe(
      "UPDATE \"user\" SET status='active' WHERE authentication_subject='alice'",
    );
  }
}
