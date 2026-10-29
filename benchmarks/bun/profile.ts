// Samples workerd's V8 CPU profile through its inspector while a load runs
// (benchmarks/README.md, "Where the time goes"), then prints where the
// samples fell: by kind (WebAssembly, JavaScript, garbage collection, idle)
// and the functions with the most self time.
//
//   bun profile.ts <inspector port> <seconds> <profile.cpuprofile>

const [port, seconds, out] = process.argv.slice(2);
const targets = (await (await fetch(`http://127.0.0.1:${port}/json/list`)).json()) as {
  webSocketDebuggerUrl: string;
}[];
const socket = new WebSocket(targets[0].webSocketDebuggerUrl);
await new Promise((resolve) => socket.addEventListener("open", resolve, { once: true }));
let id = 0;
const pending = new Map<number, (value: any) => void>();
socket.addEventListener("message", (event) => {
  const message = JSON.parse(String(event.data));
  if (message.id !== undefined) pending.get(message.id)?.(message.result);
});
const send = (method: string, params: object = {}) =>
  new Promise<any>((resolve) => {
    pending.set(++id, resolve);
    socket.send(JSON.stringify({ id, method, params }));
  });

await send("Profiler.enable");
await send("Profiler.setSamplingInterval", { interval: 100 });
await send("Profiler.start");
await Bun.sleep(Number(seconds) * 1000);
const { profile } = await send("Profiler.stop");
socket.close();
await Bun.write(out, JSON.stringify(profile));

type Node = { id: number; callFrame: { functionName: string; url: string }; hitCount?: number };
const nodes = new Map<number, Node>(profile.nodes.map((n: Node) => [n.id, n]));
const hits = new Map<number, number>();
for (const sample of profile.samples as number[]) hits.set(sample, (hits.get(sample) ?? 0) + 1);
const total = (profile.samples as number[]).length;
const kinds = new Map<string, number>();
const functions = new Map<string, number>();
for (const [nodeId, count] of hits) {
  const { functionName, url } = nodes.get(nodeId)!.callFrame;
  const kind = ["(idle)", "(garbage collector)", "(program)"].includes(functionName)
    ? functionName
    : url.startsWith("wasm://") || /^\$?[a-z_]+::/.test(functionName) || url === ""
      ? "WebAssembly or native"
      : "JavaScript";
  kinds.set(kind, (kinds.get(kind) ?? 0) + count);
  const name = `${functionName || "(anonymous)"} ${url.startsWith("wasm://") ? "[wasm]" : url ? `[${url.split("/").pop()}]` : ""}`;
  functions.set(name, (functions.get(name) ?? 0) + count);
}
const pct = (n: number) => `${((100 * n) / total).toFixed(1)}%`;
console.log(`samples: ${total}`);
for (const [kind, count] of [...kinds].sort((a, b) => b[1] - a[1])) console.log(`  ${pct(count).padStart(6)}  ${kind}`);
console.log("top self time:");
for (const [name, count] of [...functions].sort((a, b) => b[1] - a[1]).slice(0, 40)) {
  console.log(`  ${pct(count).padStart(6)}  ${name.slice(0, 140)}`);
}
