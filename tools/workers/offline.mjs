// Keeps a Node process off the network (chunk 8.2k): preloaded with
// `--import` into `wrangler deploy --dry-run`, which checks a Wrangler file
// with no account, every DNS lookup and every outbound connection is
// refused and written to standard error as one line starting
// `jadpo-offline:`, so a test that finds the line knows Wrangler reached
// for the network, and where. Wrangler passes its options on to the
// process it starts, so that process is kept off the network too.
import dns from "node:dns";
import net from "node:net";

const refused = (what, where) => {
  process.stderr.write(`jadpo-offline: ${what} ${where}\n`);
  return new Error(`jadpo-offline: ${what} ${where} refused`);
};

const connect = net.Socket.prototype.connect;
net.Socket.prototype.connect = function (...args) {
  const options = typeof args[0] === "object" && args[0] !== null ? args[0] : {};
  // A pipe to a process Wrangler starts itself, such as esbuild's, stays.
  if (typeof options.path === "string" || typeof args[0] === "string") {
    return connect.apply(this, args);
  }
  const host = options.host ?? (typeof args[1] === "string" ? args[1] : "localhost");
  const port = options.port ?? args[0];
  const error = refused("connect", `${host}:${port}`);
  process.nextTick(() => this.destroy(error));
  return this;
};

for (const name of ["lookup", "resolve", "resolve4", "resolve6", "resolveAny"]) {
  const original = dns[name];
  if (typeof original !== "function") continue;
  dns[name] = function (host, ...rest) {
    const callback = rest.findLast((arg) => typeof arg === "function");
    const error = refused(name, host);
    if (callback) process.nextTick(() => callback(error));
    else throw error;
  };
}
for (const name of ["lookup", "resolve", "resolve4", "resolve6", "resolveAny"]) {
  if (typeof dns.promises[name] !== "function") continue;
  dns.promises[name] = async (host) => {
    throw refused(name, host);
  };
}
