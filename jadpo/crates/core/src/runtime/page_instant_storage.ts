// Compiler-private page decode preserves the authored page fault operation.
export function decodePageInstant(row: Record<string, unknown>, entity: string, field: string, nullable: boolean, path: string, operation: string, sqliteDecode: (value: unknown, path: string) => unknown): unknown {
  try { return decodePointInstant(row, entity, field, nullable, path, sqliteDecode); }
  catch (error) {
    if (error instanceof PersistenceFault && error.kind !== "data") throw error;
    throw new PersistenceFault(operation, "data", error);
  }
}
