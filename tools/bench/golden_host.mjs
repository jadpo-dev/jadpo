// Controlled local transport for the production TypeScript application.
// Exactly one proxy is trusted, only when its actual TCP peer is loopback.
// Scheduling is external on every measured target; 9.1b measures job costs.
import { trustedPeer } from './golden_proxy.mjs';
import { loadConfig } from '../../examples/golden-todo/typescript/build/src/config.js';
import { Store } from '../../examples/golden-todo/typescript/build/src/db/store.js';
import { createApplication } from '../../examples/golden-todo/typescript/build/src/app.js';
import { bind } from '../../examples/golden-todo/typescript/build/src/host.js';

const config = loadConfig(process.env);
const store = new Store(config.databaseUrl);
const { app } = createApplication({ config, store });
const server = await bind((request, peer) => app.fetch(request, trustedPeer(request, peer)),
  Number(process.env.PORT), '127.0.0.1');
let closing = false;
async function close() {
  if (closing) return;
  closing = true;
  await server.close();
  await store.close();
}
process.once('SIGTERM', () => void close());
process.once('SIGINT', () => void close());
