// Compiler-private aggregate initialization lifetime. Cancellation is advisory;
// the readiness controller retains ownership until the actual initializer settles.
function createInitializationControl(setCancel?: (cancel: () => void) => void) {
  let cancelled = false;
  let current: { cancel(): unknown } | null = null;
  const reason = new Error("persistence initialization cancelled");
  setCancel?.(() => {
    cancelled = true;
    current?.cancel();
  });
  return {
    get cancelled() { return cancelled; },
    reason,
    async execute<T>(query: Promise<T> & { execute(): unknown; cancel(): unknown }): Promise<T> {
      if (cancelled) throw reason;
      current = query;
      try {
        query.execute();
        const result = await query;
        if (cancelled) throw reason;
        return result;
      } finally {
        if (current === query) current = null;
      }
    },
  };
}
