import { afterAll, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const root = mkdtempSync(join(tmpdir(), "jadpo-artifact-contracts-"));
afterAll(() => rmSync(root, { recursive: true, force: true }));
const compiler = Bun.env.JADPO_BIN ?? new URL("../../jadpo/target/debug/jadpo", import.meta.url).pathname;
writeFileSync(join(root, "app.jadpo"), `
type Label = Text { min_length: 2 max_length: 8 }
type Root = Object {
    label: Label { min_length: 4 max_length: 6 }
    maybe: Label? { min_length: 4 max_length: 6 }
}
type Link = Object { label: Root.label { max_length: 5 } maybe: Root.maybe }
type ExactLabel = Link.label { min_length: 5 }
type Named = Object { label: ExactLabel }
type Quantity = Int { min: 0 max: 10 }
type BoundedInput = Object { amount: Quantity { min: 2 max: 4 } }
type InlineInput = Object { nested: Object { label: Link.label } }
type OptionalInput = Object { maybe: Link.maybe optional }
type CollectionInput = Object { labels: List<Link.label> maybe: List<Link.maybe> }
type Payload = Enum { pending { label: Link.label { min_length: 5 } } absent }
type PayloadInput = Object { value: Payload }
entity Customer { id: Uuid identity: id }
type ReferenceInput = Object { customer: Customer.Ref }
type DenialInput = Object { ticket: ExactLabel }
type Accepted = Object { ok: Bool }
type CreatedItem = Object { id: Text }
type ChoiceInput = Object { ticket: ExactLabel alternate: Bool }
failure OtherDeclined { kind: Rejected code: "other_declined" public { ticket: ExactLabel } }
failure Declined {
    kind: Rejected code: "declined"
    public { ticket: ExactLabel }
    internal { diagnostic: Text }
}
action root_echo(input: Root) -> Root { return input }
action link_echo(input: Link) -> Link { return input }
action bounded_echo(input: BoundedInput) -> BoundedInput { return input }
action inline_echo(input: InlineInput) -> InlineInput { return input }
action named_echo(input: Named) -> Named { return input }
action optional_echo(input: OptionalInput) -> OptionalInput { return input }
action collection_echo(input: CollectionInput) -> CollectionInput { return input }
action payload_echo(input: PayloadInput) -> PayloadInput { return input }
action reference_echo(input: ReferenceInput) -> ReferenceInput { return input }
action choose_failure(input: ChoiceInput) fails Declined, OtherDeclined -> Accepted {
    if (input.alternate) { reject OtherDeclined { ticket: input.ticket } }
    reject Declined { ticket: input.ticket diagnostic: "private-artifact-canary" }
}
action deny(input: DenialInput) fails Declined -> Accepted {
    reject Declined { ticket: input.ticket diagnostic: "private-artifact-canary" }
}
action create_item() -> CreatedItem {
    return CreatedItem { id: "item-1" }
}
action remove_item() -> Unit { }
route POST /root { auth: none input: Root output: Root run: root_echo(input) }
route POST /link { auth: none input: Link output: Link run: link_echo(input) }
route POST /bounded { auth: none input: BoundedInput output: BoundedInput run: bounded_echo(input) }
route POST /inline { auth: none input: InlineInput output: InlineInput run: inline_echo(input) }
route POST /named { auth: none input: Named output: Named run: named_echo(input) }
route POST /optional { auth: none input: OptionalInput output: OptionalInput run: optional_echo(input) }
route POST /collection { auth: none input: CollectionInput output: CollectionInput run: collection_echo(input) }
route POST /payload { auth: none input: PayloadInput output: PayloadInput run: payload_echo(input) }
route POST /reference { auth: none input: ReferenceInput output: ReferenceInput run: reference_echo(input) }
route POST /choice { auth: none input: ChoiceInput output: Accepted run: choose_failure(input) }
route POST /deny { auth: none input: DenialInput output: Accepted run: deny(input) }
route POST /created { auth: none output: CreatedItem run: create_item() success: created }
route DELETE /empty { auth: none run: remove_item() success: no_content }
`);
function build() {
  const result = Bun.spawnSync([compiler, "build", root], { stdout: "pipe", stderr: "pipe" });
  if (result.exitCode !== 0) throw new Error(`Artifact contract fixture failed:\n${result.stdout}\n${result.stderr}`);
}
build();
const openapi = JSON.parse(readFileSync(join(root, "build/openapi/openapi.json"), "utf8"));
const schemas = openapi.components.schemas;
const app = await import(pathToFileURL(join(root, "build/target/app.ts")).href);

// Deliberately bounded independent JSON Schema evaluator. Every assertion
// keyword in this fixture's schemas is handled; unsupported keywords fail the
// test instead of being ignored. This is not a general schema implementation.
const supported = new Set(["$ref", "type", "properties", "required", "additionalProperties", "anyOf", "allOf", "oneOf", "const", "enum", "minLength", "maxLength", "minimum", "maximum", "pattern", "format", "items", "uniqueItems", "discriminator"]);
function permits(schema: any, value: any, depth = 0): boolean {
  if (depth > 50) throw new Error("Schema cycle/depth exceeds fixture contract");
  for (const keyword of Object.keys(schema)) {
    if (!supported.has(keyword)) throw new Error(`Unsupported schema assertion: ${keyword}`);
  }
  const next = (part: any, data = value) => permits(part, data, depth + 1);
  if (schema.$ref) {
    expect(schema.$ref.startsWith("#/components/schemas/")).toBe(true);
    const target = schemas[schema.$ref.slice("#/components/schemas/".length)];
    expect(target).toBeDefined();
    if (!next(target)) return false;
  }
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
    if (schema.format !== undefined && schema.format !== "uuid") throw new Error(`Unsupported fixture format: ${schema.format}`);
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
async function response(path: string, value: any) {
  return app.handleRequest(new Request(`https://artifact.test${path}`, {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(value),
  }));
}
async function agrees(path: string, body: any, accepted: boolean) {
  const operation = openapi.paths[path].post;
  const result = await response(path, body);
  // The independent concrete expectation is asserted against each side.
  expect(result.status, `${path} runtime: ${JSON.stringify(body)}`).toBe(accepted ? 200 : 422);
  expect(permits(operation.requestBody.content["application/json"].schema, body), `${path} schema: ${JSON.stringify(body)}`).toBe(accepted);
  if (accepted) {
    const output = await result.json();
    expect(output).toEqual(body);
    expect(permits(operation.responses["200"].content["application/json"].schema, output)).toBe(true);
  }
}

test("field-local and ancestor constraints agree at the public boundary", async () => {
  for (const label of ["a", "abc", "four", "five5", "sixsix", "seven77", "ninechars"]) {
    await agrees("/root", { label, maybe: null }, label.length >= 4 && label.length <= 6);
  }
});
test("field chains and named refinements preserve intersections", async () => {
  for (const label of ["abc", "four", "five5", "sixsix"]) {
    await agrees("/link", { label, maybe: null }, label.length >= 4 && label.length <= 5);
    await agrees("/named", { label }, label.length === 5);
  }
});
test("numeric field bounds intersect with named parent bounds", async () => {
  for (const amount of [-1, 0, 1, 2, 3, 4, 5, 11]) {
    await agrees("/bounded", { amount }, amount >= 2 && amount <= 4);
  }
});
test("inline nested shapes retain field contracts without exposing synthesized names", async () => {
  await agrees("/inline", { nested: { label: "five5" } }, true);
  await agrees("/inline", { nested: { label: "abc" } }, false);
  await agrees("/inline", { nested: { label: "five5", extra: true } }, false);
  expect(JSON.stringify(openapi)).not.toContain("__jadpo_");
});
test("nullable and omitted values preserve shape independently of constraints", async () => {
  for (const body of [{}, { maybe: null }, { maybe: "four" }, { maybe: "a" }, { maybe: "seven77" }, { extra: "four" }]) {
    const accepted = !Object.hasOwn(body, "extra") && (!Object.hasOwn(body, "maybe") || body.maybe === null || body.maybe === "four");
    await agrees("/optional", body, accepted);
  }
});
test("collection elements preserve local field constraints and nullable values", async () => {
  for (const [body, accepted] of [
    [{ labels: ["four", "five5"], maybe: [null, "four"] }, true],
    [{ labels: ["sixsix"], maybe: [] }, false],
    [{ labels: [null], maybe: [] }, false],
    [{ labels: [], maybe: ["a"] }, false],
    [{ labels: [], maybe: null }, false],
    [{ labels: [], maybe: [] }, true],
  ] as const) await agrees("/collection", body, accepted);
});
test("tagged enum payload fields retain their constraints and closed shape", async () => {
  for (const [value, accepted] of [
    [{ tag: "pending", label: "five5" }, true],
    [{ tag: "pending", label: "four" }, false],
    [{ tag: "pending", label: "sixsix" }, false],
    [{ tag: "pending" }, false],
    [{ tag: "absent" }, true],
    [{ tag: "absent", label: "five5" }, false],
  ] as const) await agrees("/payload", { value }, accepted);
});
test("entity references use the identity wire contract", async () => {
  for (const [customer, accepted] of [
    ["018f57d0-bf42-4f25-9417-000000000001", true],
    ["invalid", false], [null, false], [{ id: "018f57d0-bf42-4f25-9417-000000000001" }, false],
  ] as const) await agrees("/reference", { customer }, accepted);
});
test("failure public fields are typed and internal fields remain absent", async () => {
  const result = await response("/deny", { ticket: "five5" });
  expect(result.status).toBe(422);
  const body = await result.json();
  expect(body.error.details.ticket).toBe("five5");
  expect(body.error.code).toBe("declined");
  expect(body.error).not.toHaveProperty("diagnostic");
  expect(schemas).not.toHaveProperty("Declined.internal.diagnostic");
  expect(JSON.stringify(body)).not.toContain("private-artifact-canary");
  const schema = openapi.paths["/deny"].post.responses["422"].content["application/json"].schema;
  expect(permits(schema, body)).toBe(true);
  expect(permits(schema, { error: { ...body.error, details: { ticket: 123 } } })).toBe(false);
  expect(permits(schema, { error: { ...body.error, details: { ticket: "four" } } })).toBe(false);
  expect(permits(schema, { error: { ...body.error, details: { ...body.error.details, diagnostic: "private-artifact-canary" } } })).toBe(false);
});
test("every declared failure remains representable when HTTP status is shared", async () => {
  const schema = openapi.paths["/choice"].post.responses["422"].content["application/json"].schema;
  for (const alternate of [false, true]) {
    const result = await response("/choice", { ticket: "five5", alternate });
    expect(result.status).toBe(422);
    const body = await result.json();
    expect(body.error.code).toBe(alternate ? "other_declined" : "declined");
    expect(permits(schema, body), `declared ${body.error.code} must remain in the response contract`).toBe(true);
  }
});
test("created routes return a typed 201 response and publish it in OpenAPI", async () => {
  const response = await app.handleRequest(new Request("http://local/created", { method: "POST" }));
  expect(response.status).toBe(201);
  expect(await response.json()).toEqual({ id: "item-1" });
  expect(openapi.paths["/created"].post.responses["201"].content["application/json"].schema).toEqual({ $ref: "#/components/schemas/CreatedItem" });
  const inventory = JSON.parse(readFileSync(join(root, "build/inventory/routes.json"), "utf8"));
  expect(inventory.schema_version).toBe(2);
  const route = inventory.routes.find((entry: any) => entry.route === "POST /created");
  expect(route.success).toEqual({ kind: "created", http_status: 201 });
});
test("no-content routes return an empty 204 response and omit an OpenAPI body schema", async () => {
  const response = await app.handleRequest(new Request("http://local/empty", { method: "DELETE" }));
  expect(response.status).toBe(204);
  expect(await response.text()).toBe("");
  expect(response.headers.get("x-request-id")).toMatch(/^req_/);
  expect(openapi.paths["/empty"].delete.responses["204"]).toEqual({ description: "No content" });
  expect(openapi.paths["/empty"].delete.responses["204"]).not.toHaveProperty("content");
  const inventory = JSON.parse(readFileSync(join(root, "build/inventory/routes.json"), "utf8"));
  const route = inventory.routes.find((entry: any) => entry.route === "DELETE /empty");
  expect(route.success).toEqual({ kind: "no_content", http_status: 204 });
});
test("unchanged builds reproduce contract artifacts byte for byte", () => {
  const files = ["openapi/openapi.json", "validators/plan.json", "inventory/routes.json", "inventory/callables.json", "audit/failures.json", "compatibility/public-failure-codes.json", "app.meta.json"];
  const initial = files.map((path) => readFileSync(join(root, "build", path), "utf8"));
  build();
  expect(files.map((path) => readFileSync(join(root, "build", path), "utf8"))).toEqual(initial);
});
