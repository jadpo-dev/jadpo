// Compiler-private aggregate initialization lifetime. Cancellation is advisory;
// the readiness controller retains ownership until the actual initializer settles.
function createInitializationControl(setCancel?: (cancel: () => void) => void) {
  let cancelled = false;
  let current: { cancel(): unknown } | null = null;
  let budgetConnection: SQL | null = null;
  let deadlineAt: number | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let expired = false;
  const reason = new Error("persistence initialization cancelled");
  const expire = () => { expired = true; if (cancelled) return; cancelled = true; try { current?.cancel(); } catch {} };
  const check = () => { if (deadlineAt !== null && performance.now() >= deadlineAt) expire(); if (cancelled) throw reason; };
  setCancel?.(() => {
    if (cancelled) return;
    cancelled = true;
    current?.cancel();
  });
  return {
    get cancelled() { return cancelled; },
    get expired() { return expired; },
    reason,
    check,
    setBudget(connection: SQL, deadline: number) {
      budgetConnection = connection; deadlineAt = deadline; check();
      timer = setTimeout(expire, Math.max(0, deadline - performance.now()));
    },
    dispose() { if (timer !== null) clearTimeout(timer); timer = null; },
    async execute<T>(query: Promise<T> & { execute(): unknown; cancel(): unknown }): Promise<T> {
      check();
      if (budgetConnection !== null && deadlineAt !== null) {
        const remaining = String(Math.max(1, Math.ceil(deadlineAt - performance.now())));
        const limits = budgetConnection.unsafe("SELECT set_config('statement_timeout', $1, true), set_config('lock_timeout', $1, true)", [remaining]);
        current = limits;
        try { limits.execute(); await limits; check(); }
        finally { if (current === limits) current = null; }
      }
      check();
      current = query;
      try {
        query.execute();
        const result = await query;
        check();
        return result;
      } finally {
        if (current === query) current = null;
      }
    },
  };
}
