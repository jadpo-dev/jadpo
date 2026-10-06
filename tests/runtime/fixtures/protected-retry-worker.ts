import { privateRetryApplication } from "./protected-retry-application";
const app = await privateRetryApplication(Bun.env.JADPO_PROTECTED_APP!, true);
await app.initializeApplication(JSON.parse(Bun.env.JADPO_PROTECTED_CONFIGURATION!));
const server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: request => app.handleRequest(request) });
console.log(JSON.stringify({ ready: true, port: server.port, modifiedAdmissionBit: true, releasedPublicRetry: false }));
await new Response(Bun.stdin).text();
server.stop(true);
process.exit(0);
