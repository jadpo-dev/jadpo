export type CheckpointFetchPolicyOptions = {
  issuer: string;
  jwksUri: string;
  localOrigin: () => string | undefined;
  jwk: () => JsonWebKey | undefined;
  providerCalls: string[];
  rejectedCalls: string[];
  nativeFetch?: typeof fetch;
};

export function installCheckpointFetchPolicy(options: CheckpointFetchPolicyOptions) {
  const previousFetch = globalThis.fetch;
  const nativeFetch = options.nativeFetch ?? previousFetch;
  const discoveryUrl = new URL(".well-known/openid-configuration", `${options.issuer.replace(/\/$/u, "")}/`).href;
  const jwksUrl = new URL(options.jwksUri).href;
  const installedFetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const requestUrl = input instanceof Request ? input.url : String(input);
    let destination: URL;
    try {
      destination = new URL(requestUrl);
    } catch {
      options.rejectedCalls.push("<invalid-url>");
      throw new Error("Checkpoint fetch refused an invalid destination");
    }

    const localOrigin = options.localOrigin();
    const isLoopback = destination.hostname === "127.0.0.1"
      || destination.hostname === "localhost"
      || destination.hostname === "[::1]";
    if (localOrigin && isLoopback && destination.username === "" && destination.password === ""
        && destination.origin === localOrigin) {
      // Keep redirects from escaping the wrapper's destination policy. The
      // checkpoint only needs the loopback response; following a redirect
      // would hand the destination decision back to native fetch.
      return nativeFetch(input, { ...init, redirect: "error" });
    }

    if (destination.href === discoveryUrl || destination.href === jwksUrl) {
      options.providerCalls.push(destination.href);
      const jwk = options.jwk();
      if (init?.redirect !== "error" || init?.credentials !== "omit" || !jwk) {
        throw new Error("OIDC provider mock received an unexpected request policy or missing JWKS fixture");
      }
      return Response.json(destination.href === discoveryUrl
        ? { issuer: options.issuer, jwks_uri: jwksUrl }
        : { keys: [jwk] });
    }

    options.rejectedCalls.push(destination.origin);
    throw new Error("Checkpoint fetch refused an unexpected destination");
  }) as typeof fetch;
  globalThis.fetch = installedFetch;
  return () => {
    if (globalThis.fetch === installedFetch) globalThis.fetch = previousFetch;
  };
}
