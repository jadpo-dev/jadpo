// The deployable `app` for Cloudflare Workers, written by `jadpo build`; don't edit it.
// Its module is instantiated once per isolate, and each handler hands
// the module's its event, the Worker's `env` and the invocation's `ctx`.
import { WorkerEntrypoint } from "cloudflare:workers";
import module from "./app.wasm";
import { initSync, fetch as handleFetch, scheduled as handleScheduled, queue as handleQueue, hand_on as handleHandOn } from "./app.js";

initSync({ module });

export default {
  fetch(request, env, ctx) {
    return handleFetch(request, env, ctx);
  },
  scheduled(controller, env, ctx) {
    return handleScheduled(controller, env, ctx);
  },
  queue(batch, env, ctx) {
    return handleQueue(batch, env, ctx);
  },
};

// Work another invocation of this Worker handed on, which this one
// carries on in its own time; reached only through the Worker's
// binding to itself.
export class JadpoHandOn extends WorkerEntrypoint {
  handOn(work) {
    return handleHandOn(work, this.env, this.ctx);
  }
}
