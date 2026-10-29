import { expect } from "vitest";
import { SignJWT, exportJWK, generateKeyPair } from "jose";
import { AuthenticationService } from "../src/auth.ts";
import { Egress, type Transport } from "../src/egress.ts";
import { loadConfig } from "../src/config.ts";
import { RequestScope } from "../src/runtime.ts";
import type { Store } from "../src/db/store.ts";

// Every discovery/JWKS answer is supplied by this local injected transport.
export async function resolverNegatives(store: Store, url: string) {
  let now = 1768478400000;
  const config = loadConfig({
    DATABASE_URL: url,
    BROWSER_ORIGIN: "https://todo.example.test",
    OIDC_ISSUER: "https://identity.example/tenant/",
    OIDC_AUDIENCE: "golden-todo",
    SESSION_SIGNING_KEY: "x".repeat(43),
    MAIL_API_KEY: "private-provider-secret",
  });
  const pair = await generateKeyPair("EdDSA");
  const key = {
    ...(await exportJWK(pair.publicKey)),
    alg: "EdDSA",
    kid: "key",
  };
  const token = await new SignJWT({
    sub: "alice",
    iss: config.issuer,
    aud: config.audience,
    exp: now / 1000 + 86400,
  })
    .setProtectedHeader({ alg: "EdDSA", kid: "key" })
    .sign(pair.privateKey);
  const metadata = JSON.stringify({
    issuer: config.issuer,
    jwks_uri: "https://keys.example:8443/jwks",
  });
  const keys = JSON.stringify({ keys: [key] });
  const scopes: RequestScope[] = [];
  const scope = (abort = new AbortController()) => {
    const value = new RequestScope(abort.signal);
    scopes.push(value);
    return value;
  };
  const authenticate = (
    service: AuthenticationService,
    lifetime = scope(),
    credential = token,
  ) =>
    store.run(lifetime, () =>
      service.authenticate(
        new Request("https://todo.example.test/todos", {
          headers: { authorization: "Bearer " + credential },
        }),
        lifetime,
      ),
    );
  const service = (transport: Transport) =>
    new AuthenticationService(config, store, () => now, new Egress(transport));
  try {
    for (const [document, jwks] of [
      ["{", keys],
      [
        JSON.stringify({
          issuer: "https://wrong.example",
          jwks_uri: "https://keys.example/jwks",
        }),
        keys,
      ],
      [
        JSON.stringify({
          issuer: config.issuer,
          jwks_uri: "http://keys.example/jwks",
        }),
        keys,
      ],
      [metadata + " ".repeat(65537), keys],
      [metadata, "{"],
      [metadata, JSON.stringify({ keys: [] })],
      [
        metadata,
        JSON.stringify({ keys: Array.from({ length: 65 }, () => key) }),
      ],
      [metadata, keys + " ".repeat(262145)],
    ]) {
      let fetched = 0;
      const auth = service((input) => {
        fetched++;
        return Promise.resolve({
          status: 200,
          body: input.url.endsWith("openid-configuration") ? document! : jwks!,
        });
      });
      await expect(authenticate(auth)).rejects.toMatchObject({
        code: "authentication_misconfigured",
      });
      const attempts = fetched;
      await expect(authenticate(auth)).rejects.toMatchObject({
        code: "authentication_misconfigured",
      });
      expect(fetched).toBe(attempts);
    }
    let fetched = 0;
    let fail = true;
    const auth = service((input) => {
      fetched++;
      return Promise.resolve({
        status: fail ? 503 : 200,
        body: input.url.endsWith("openid-configuration") ? metadata : keys,
      });
    });
    await expect(authenticate(auth)).rejects.toMatchObject({
      code: "authentication_unavailable",
    });
    await expect(authenticate(auth)).rejects.toMatchObject({
      code: "authentication_unavailable",
    });
    expect(fetched).toBe(1);
    now += 30000;
    fail = false;
    expect((await authenticate(auth)).principal.kind).toBe("user");
    expect(fetched).toBe(3);
    now += 299999;
    expect((await authenticate(auth)).principal.kind).toBe("user");
    expect(fetched).toBe(3);
    now++;
    fail = true;
    await expect(authenticate(auth)).rejects.toMatchObject({
      code: "authentication_unavailable",
    });
    await expect(authenticate(auth)).rejects.toMatchObject({
      code: "authentication_unavailable",
    });
    expect(fetched).toBe(4);

    // Simultaneous callers share one refresh; cancelling a waiter leaves its owner intact.
    let release!: () => void;
    let entered!: () => void;
    const barrier = new Promise<void>((resolve) => {
      release = resolve;
    });
    const ready = new Promise<void>((resolve) => {
      entered = resolve;
    });
    let concurrentFetches = 0;
    const concurrent = service(async (input) => {
      concurrentFetches++;
      if (input.url.endsWith("openid-configuration")) {
        entered();
        await barrier;
      }
      return {
        status: 200,
        body: input.url.endsWith("openid-configuration") ? metadata : keys,
      };
    });
    const owner = authenticate(concurrent);
    await ready;
    const abort = new AbortController();
    const waiter = authenticate(concurrent, scope(abort));
    const rejectedWaiter = expect(waiter).rejects.toMatchObject({
      code: "unavailable",
    });
    abort.abort();
    await rejectedWaiter;
    release();
    expect((await owner).principal.kind).toBe("user");
    expect(concurrentFetches).toBe(2);

    let cancelledFetches = 0;
    const cancelled = service(
      (_input, lifetime) =>
        new Promise((_resolve, reject) => {
          cancelledFetches++;
          lifetime.signal.addEventListener(
            "abort",
            () => {
              reject(Error("fixture cancelled"));
            },
            { once: true },
          );
        }),
    );
    const cancelOwner = new AbortController();
    const pending = authenticate(cancelled, scope(cancelOwner));
    const rejectedOwner = expect(pending).rejects.toMatchObject({
      code: "authentication_unavailable",
    });
    cancelOwner.abort();
    await rejectedOwner;
    await expect(authenticate(cancelled)).rejects.toMatchObject({
      code: "authentication_unavailable",
    });
    expect(cancelledFetches).toBe(1);

    // Signature failure can mean same-kid key replacement, but only after cooldown.
    let rolloverFetches = 0;
    let rolloverKey = key;
    const rollover = service((input) => {
      rolloverFetches++;
      return Promise.resolve({
        status: 200,
        body: input.url.endsWith("openid-configuration")
          ? metadata
          : JSON.stringify({ keys: [rolloverKey] }),
      });
    });
    expect((await authenticate(rollover)).principal.kind).toBe("user");
    const replacement = await generateKeyPair("EdDSA");
    rolloverKey = {
      ...(await exportJWK(replacement.publicKey)),
      alg: "EdDSA",
      kid: "key",
    };
    const replacementToken = await new SignJWT({
      sub: "alice",
      iss: config.issuer,
      aud: config.audience,
      exp: now / 1000 + 86400,
    })
      .setProtectedHeader({ alg: "EdDSA", kid: "key" })
      .sign(replacement.privateKey);
    await expect(
      authenticate(rollover, scope(), replacementToken),
    ).rejects.toMatchObject({ code: "invalid_credentials" });
    expect(rolloverFetches).toBe(2);
    now += 30000;
    expect(
      (await authenticate(rollover, scope(), replacementToken)).principal.kind,
    ).toBe("user");
    expect(rolloverFetches).toBe(4);

    let timedFetches = 0;
    const timed = service(
      (_input, lifetime) =>
        new Promise((_resolve, reject) => {
          timedFetches++;
          expect(lifetime.remaining()).toBeLessThanOrEqual(3000);
          lifetime.signal.addEventListener(
            "abort",
            () => {
              reject(Error("fixture fetch timeout"));
            },
            { once: true },
          );
        }),
    );
    const started = performance.now();
    await expect(authenticate(timed)).rejects.toMatchObject({
      code: "authentication_unavailable",
    });
    expect(performance.now() - started).toBeLessThan(4000);
    await expect(authenticate(timed)).rejects.toMatchObject({
      code: "authentication_unavailable",
    });
    expect(timedFetches).toBe(1);

    // Exact document/key limits are valid; explicitly encryption-only keys are ignored.
    const bounded = service((input) =>
      Promise.resolve({
        status: 200,
        body: input.url.endsWith("openid-configuration")
          ? metadata.padEnd(65536, " ")
          : JSON.stringify({
              keys: [
                key,
                ...Array.from({ length: 63 }, () => ({ use: "enc" })),
              ],
            }).padEnd(262144, " "),
      }),
    );
    expect((await authenticate(bounded)).principal.kind).toBe("user");
  } finally {
    scopes.forEach((value) => {
      value.close();
    });
  }
}
