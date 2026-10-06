// Transparent observer of the native generated main-entry listener boundary.
// It records successful native serve calls; it does not create a listener.
const nativeServe = Bun.serve;
let listenerStarts = 0;
(Bun as any).serve = function (...arguments_: any[]) {
  const server = (nativeServe as any).apply(Bun, arguments_);
  listenerStarts++;
  // Positive startup control exits only after native startup can log readiness.
  setTimeout(() => { server.stop(true); process.exit(0); }, 0);
  return server;
};
process.on("exit", code => {
  console.log(JSON.stringify({ kind: "checkpoint_startup_observation", exitCode: code, listenerStarts }));
});
