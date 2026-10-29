// The same Hono application uses each runtime's native HTTP binding.
export type Peer = {
  peer?: string;
  incoming?: { socket: { remoteAddress?: string } };
};
type Handler = (request: Request, peer: Peer) => Response | Promise<Response>;
type BunServer = {
  port: number;
  stop(closeActiveConnections?: boolean): void;
  requestIP(request: Request): { address: string } | null;
};
type BunApi = {
  serve(options: {
    hostname: string;
    port: number;
    idleTimeout: number;
    fetch: (
      request: Request,
      server: BunServer,
    ) => Response | Promise<Response>;
  }): BunServer;
};
export async function bind(
  handler: Handler,
  port: number,
  hostname = "127.0.0.1",
) {
  const bun = (globalThis as typeof globalThis & { Bun?: BunApi }).Bun;
  if (bun) {
    const server = bun.serve({
      hostname,
      port,
      idleTimeout: 10,
      fetch: (request, server) =>
        handler(request, {
          peer: server.requestIP(request)?.address ?? "unknown",
        }),
    });
    return {
      port: server.port,
      close: async () => {
        server.stop(true);
        await Promise.resolve();
      },
    };
  }
  const { serve } = await import("@hono/node-server");
  const server = serve({
    hostname,
    port,
    fetch: (request, bindings) => handler(request, bindings as Peer),
  });
  if ("headersTimeout" in server) server.headersTimeout = 10000;
  if ("requestTimeout" in server) server.requestTimeout = 30000;
  server.maxConnections = 1024;
  await new Promise<void>((resolve, reject) => {
    server.once("listening", resolve);
    server.once("error", reject);
  });
  const address = server.address();
  if (!address || typeof address === "string")
    throw Error("Listener unavailable.");
  return {
    port: address.port,
    close: async () => {
      await new Promise<void>((resolve, reject) => {
        server.close((error) => (error ? reject(error) : resolve()));
      });
    },
  };
}
