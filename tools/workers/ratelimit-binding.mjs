// Cloudflare's rate-limiting binding as Jadpo's tests stand it in under
// workerd, which has no binding of its own (chunk 8.2m2): `limit({ key })`
// counts `key` over a fixed window of `period` seconds and says only
// whether it is within `limit`, which is all the binding answers
// (`RateLimitOutcome` in workerd's own types). A test's workerd config
// gives it as a wrapped binding, `limit` and `period` as its inner
// bindings, as a Wrangler file's `simple: { limit, period }` does;
// `broken: true` makes every call throw, as a binding that can't be
// reached would, and `log: true` writes each answer to the console, so a
// test sees the binding was asked.
export default function (env) {
  const windows = new Map();
  return {
    async limit({ key }) {
      if (env.broken) {
        throw new Error("the rate-limiting binding can't be reached");
      }
      const now = Date.now();
      let window = windows.get(key);
      if (!window || now - window.start >= env.period * 1000) {
        window = { start: now, count: 0 };
        windows.set(key, window);
      }
      window.count += 1;
      const success = window.count <= env.limit;
      if (env.log) {
        console.log(`rate-limiting binding: ${key} ${success ? "within" : "over"}`);
      }
      return { success };
    },
  };
}
