// Transparent observer of the native generated main-entry listener boundary.
// It records successful native serve calls; it does not create a listener.
import { Server } from "node:net";
import { driverInitializationCount } from "./golden_sql_observer.ts";
const nativeServe = Bun.serve;
const nativeListen = Server.prototype.listen;
let listenerAttempts = 0;
Server.prototype.listen = function (...arguments_: any[]) {
  listenerAttempts++;
  this.once("listening", () => { listenerStarts++; setTimeout(() => this.close(() => process.exit(0)), 0); });
  return (nativeListen as any).apply(this, arguments_);
};
let listenerStarts = 0;
(Bun as any).serve = function (...arguments_: any[]) {
  listenerAttempts++;
  const server = (nativeServe as any).apply(Bun, arguments_);
  listenerStarts++;
  // Positive startup control exits only after native startup can log readiness.
  setTimeout(() => { server.stop(true); process.exit(0); }, 0);
  return server;
};
process.on("exit", code => {
  console.log(JSON.stringify({ kind: "checkpoint_startup_observation", exitCode: code, listenerStarts, listenerAttempts, driverInitializations: driverInitializationCount() }));
});
