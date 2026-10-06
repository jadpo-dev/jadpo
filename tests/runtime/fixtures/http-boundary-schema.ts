// Deliberately bounded independent JSON Schema evaluator. Every assertion
// keyword in this fixture's schemas is handled; unsupported keywords fail the
// test instead of being ignored. This is not a general schema implementation.
const supported = new Set(["$ref", "type", "properties", "required", "additionalProperties", "anyOf", "allOf", "oneOf", "const", "enum", "minLength", "maxLength", "minimum", "maximum", "pattern", "format", "items", "uniqueItems", "discriminator", "not"]);
export function permits(schema: any, value: any, schemas: any, depth = 0): boolean {
  if (depth > 50) throw new Error("Schema cycle/depth exceeds fixture contract");
  for (const keyword of Object.keys(schema)) {
    if (!supported.has(keyword)) throw new Error(`Unsupported schema assertion: ${keyword}`);
  }
  const next = (part: any, data = value) => permits(part, data, schemas, depth + 1);
  if (schema.$ref) {
    if (!schema.$ref.startsWith("#/components/schemas/")) throw new Error("Unsupported schema reference");
    const target = schemas[schema.$ref.slice("#/components/schemas/".length)];
    if (target === undefined) throw new Error("Unresolved schema reference");
    if (!next(target)) return false;
  }
  if (schema.not && next(schema.not)) return false;
  if (schema.anyOf && !schema.anyOf.some((part: any) => next(part))) return false;
  if (schema.allOf && !schema.allOf.every((part: any) => next(part))) return false;
  if (schema.oneOf && schema.oneOf.filter((part: any) => next(part)).length !== 1) return false;
  if (Object.hasOwn(schema, "const") && JSON.stringify(value) !== JSON.stringify(schema.const)) return false;
  if (schema.enum && !schema.enum.includes(value)) return false;
  if (schema.type) {
    const matches = schema.type === "null" ? value === null
      : schema.type === "array" ? Array.isArray(value)
      : schema.type === "object" ? value !== null && typeof value === "object" && !Array.isArray(value)
      : schema.type === "integer" ? Number.isInteger(value)
      : typeof value === schema.type;
    if (!matches) return false;
  }
  if (typeof value === "string") {
    const length = Array.from(value).length;
    if (schema.minLength !== undefined && length < schema.minLength) return false;
    if (schema.maxLength !== undefined && length > schema.maxLength) return false;
    if (schema.pattern !== undefined && !new RegExp(schema.pattern, "u").test(value)) return false;
    if (schema.format === "uuid" && !/^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu.test(value)) return false;
    if (schema.format === "date-time" && !Number.isFinite(Date.parse(value))) return false;
    if (schema.format === "email" && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/u.test(value)) return false;
    if (schema.format === "uri") { try { new URL(value); } catch { return false; } }
    if (schema.format !== undefined && !["uuid", "date-time", "email", "uri"].includes(schema.format)) throw new Error(`Unsupported fixture format: ${schema.format}`);
  }
  if (typeof value === "number") {
    if (schema.minimum !== undefined && value < schema.minimum) return false;
    if (schema.maximum !== undefined && value > schema.maximum) return false;
  }
  if (Array.isArray(value)) {
    if (schema.items && !value.every((item) => next(schema.items, item))) return false;
    if (schema.uniqueItems && new Set(value.map((item) => JSON.stringify(item))).size !== value.length) return false;
  } else if (value !== null && typeof value === "object") {
    if (schema.required && !schema.required.every((name: string) => Object.hasOwn(value, name))) return false;
    for (const [key, entry] of Object.entries(value)) {
      if (schema.properties && Object.hasOwn(schema.properties, key)) {
        if (!next(schema.properties[key], entry)) return false;
      } else if (schema.additionalProperties === false) return false;
      else if (schema.additionalProperties && !next(schema.additionalProperties, entry)) return false;
    }
  }
  return true;
}
