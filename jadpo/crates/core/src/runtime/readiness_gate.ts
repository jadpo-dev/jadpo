// Compiler-private host controller. Authored Jadpo cannot supply these callbacks.
// A response deadline does not interrupt native work or synchronous event-loop work.
type ReadinessGateOptions<Candidate> = {
  ready(): boolean;
  fatal(): boolean;
  needsProbe(): boolean;
  invalidation(): number;
  check(recovering: boolean, setCancel: (cancel: () => void) => void, deadlineAt: number): Promise<boolean>;
  prepare(recovering: boolean): Candidate;
  publish(candidate: Candidate): void;
  unavailable(): void;
  now?: () => number;
  schedule?: (callback: () => void, milliseconds: number) => unknown;
  clear?: (timer: unknown) => void;
};

export function createReadinessGate<Candidate>(options: ReadinessGateOptions<Candidate>) {
  const now = options.now ?? (() => performance.now());
  const schedule = options.schedule ?? ((callback, milliseconds) => setTimeout(callback, milliseconds));
  const clear = options.clear ?? (timer => clearTimeout(timer as ReturnType<typeof setTimeout>));
  type Attempt = {
    deadlineAt: number;
    invalidation: number;
    responseSettled: boolean;
    response: Promise<boolean>;
    resolve: (ready: boolean) => void;
    timer: unknown;
    cancel?: () => void;
  };
  let active: Attempt | null = null;
  let nextProbeAt = 0, nextAttemptAt = 0, retryMilliseconds = 250;

  const fail = (attempt: Attempt): void => {
    if (attempt.responseSettled) return;
    attempt.responseSettled = true;
    options.unavailable();
    nextAttemptAt = now() + retryMilliseconds;
    retryMilliseconds = Math.min(retryMilliseconds * 2, 5000);
    clear(attempt.timer);
    attempt.resolve(false);
    // Cancellation is advisory and must not prevent the response from settling.
    try { attempt.cancel?.(); } catch {}
    // Do not release active here. The aggregate native check still owns it.
  };
  const eligible = (attempt: Attempt): boolean => active === attempt
    && !attempt.responseSettled && now() < attempt.deadlineAt
    && !options.fatal() && options.invalidation() === attempt.invalidation;

  return {
    // Not async: later callers reuse the bounded response, never observe the raw
    // native promise or attach a new timer/completion observer to it.
    refresh(): Promise<boolean> {
      if (options.fatal()) return Promise.resolve(false);
      if (active !== null) {
        if (!active.responseSettled && now() >= active.deadlineAt) fail(active);
        return active.response;
      }
      const startedAt = now();
      if (options.ready() && !options.needsProbe() && startedAt < nextProbeAt) return Promise.resolve(true);
      if (!options.ready() && startedAt < nextAttemptAt) return Promise.resolve(false);
      let resolve!: (ready: boolean) => void;
      const response = new Promise<boolean>(settle => { resolve = settle; });
      const attempt: Attempt = { deadlineAt: startedAt + 1000, invalidation: options.invalidation(),
        responseSettled: false, response, resolve, timer: undefined };
      const recovering = !options.ready();
      active = attempt; // Reserve before invoking native work or any callback.
      const expire = () => {
        if (active !== attempt || attempt.responseSettled) return;
        const remaining = attempt.deadlineAt - now();
        if (remaining > 0) { attempt.timer = schedule(expire, remaining); return; }
        fail(attempt);
      };
      attempt.timer = schedule(expire, 1000);
      const finish = (healthy: boolean) => {
        try {
          if (!healthy || !eligible(attempt)) { fail(attempt); return; }
          const candidate = options.prepare(recovering);
          // Preparation may consume time or encounter a newer invalidation.
          if (!eligible(attempt)) { fail(attempt); return; }
          options.publish(candidate); // One synchronous, non-awaiting publication.
          attempt.responseSettled = true;
          nextProbeAt = now() + 1000; nextAttemptAt = 0; retryMilliseconds = 250;
          attempt.resolve(true);
        } catch { fail(attempt); }
        finally {
          clear(attempt.timer);
          if (active === attempt) active = null; // Only actual native settlement.
        }
      };
      try {
        // One handled observer per aggregate native attempt, regardless of callers.
        void options.check(recovering, cancel => { attempt.cancel = cancel; }, attempt.deadlineAt)
          .then(healthy => finish(healthy), () => finish(false));
      } catch { finish(false); }
      return response;
    },
  };
}
