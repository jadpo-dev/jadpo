// Compiler-private ordinary point context. Raw policy rows keep their native values.
type PointInstantDescriptor = readonly [field: string, valueAlias: string, typeAlias: string, nullable: boolean];
type PointInstantMetadata = Readonly<{ entity: string; postgres: boolean; fields: ReadonlyMap<string, Readonly<{ value: unknown; type: unknown; nullable: boolean }>> }>;
const pointInstantMetadata = new WeakMap<object, PointInstantMetadata>();
function capturePointInstants(rows: readonly unknown[], entity: string, postgres: boolean, descriptors: readonly PointInstantDescriptor[]): void {
  for (const raw of rows) {
    if (typeof raw !== "object" || raw === null) continue;
    const row = raw as Record<string, unknown>;
    const fields = new Map<string, Readonly<{ value: unknown; type: unknown; nullable: boolean }>>();
    for (const [field, valueAlias, typeAlias, nullable] of descriptors) {
      fields.set(field, Object.freeze({ value: postgres ? row[valueAlias] : row[field], type: postgres ? row[typeAlias] : null, nullable }));
      if (postgres) { delete row[valueAlias]; delete row[typeAlias]; }
    }
    pointInstantMetadata.set(row, Object.freeze({ entity, postgres, fields }));
  }
}
export function decodePointInstant(row: Record<string, unknown>, entity: string, field: string, nullable: boolean, path: string, sqliteDecode: (value: unknown, path: string) => unknown): unknown {
  const fail = (): never => { throw new PersistenceFault(`decode.${entity}`, "data", new Error("stored point Instant contradicts checked type, precision, range or context")); };
  const context = pointInstantMetadata.get(row);
  const info = context?.fields.get(field);
  if (!context || context.entity !== entity || context.postgres !== (postgres !== null) || !info || info.nullable !== nullable) return fail();
  if (!context.postgres) return sqliteDecode(row[field], path);
  if (info.type !== "timestamp with time zone") return fail();
  if (info.value === null) return nullable ? null : fail();
  if (typeof info.value !== "string") return fail();
  const match = /^(-?)([0-9]+)(?:\.([0-9]{1,6}))?$/u.exec(info.value);
  if (match === null) return fail();
  const magnitude = BigInt(match[2]) * 1000000n + BigInt((match[3] ?? "").padEnd(6, "0"));
  const micros = match[1] ? -magnitude : magnitude;
  if (micros % 1000n !== 0n) return fail();
  const milliseconds = micros / 1000n;
  if (milliseconds < -62135596800000n || milliseconds > 253402300799999n) return fail();
  return new Date(Number(milliseconds)).toISOString();
}
