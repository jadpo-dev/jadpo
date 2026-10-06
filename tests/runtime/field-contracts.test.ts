import { afterAll, describe, expect, test } from "bun:test";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

// Compile the same source used by these requests with the current compiler.
// There is no checked-in generated target or separately maintained JS oracle.
const root = mkdtempSync(join(tmpdir(), "jadpo-field-contracts-"));
afterAll(() => rmSync(root, { recursive: true, force: true }));
const compiler = Bun.env.JADPO_BIN ?? new URL("../../jadpo/target/debug/jadpo", import.meta.url).pathname;
writeFileSync(join(root, "app.jadpo"), `
type Label = Text { min_length: 2 max_length: 8 }
type Customer = Object {
    label: Label { min_length: 4 max_length: 6 }
    maybe: Label? { min_length: 4 max_length: 6 }
    note: Label { min_length: 4 } optional
}
type Link = Object {
    label: Customer.label { min_length: 5 }
    maybe: Customer.maybe
}
type Outer = Object { label: Link.label maybe: Link.maybe }
type NarrowLabel = Customer.label { max_length: 5 }
type NarrowInput = Object { label: NarrowLabel }
type NullableLabel = Link.maybe { max_length: 5 }
type NullableInput = Object { label: NullableLabel }
type RawInput = Object { raw: Text }
type Accepted = Object { ok: Bool }
action echo_customer(input: Customer) -> Customer { return input }
action echo_outer(input: Outer) -> Outer { return input }
action echo_narrow(input: NarrowInput) -> NarrowInput { return input }
action echo_nullable(input: NullableInput) -> NullableInput { return input }
function dynamic_label(input: RawInput) -> Customer.label { return Customer.label(input.raw) }
action validate_unused(input: RawInput) -> Accepted {
    var validated = Customer.label(input.raw)
    return Accepted { ok: true }
}
function literal_label() -> Customer.label { return Customer.label("valid") }
route POST /customer { auth: none input: Customer output: Customer run: echo_customer(input) }
route POST /outer { auth: none input: Outer output: Outer run: echo_outer(input) }
route POST /narrow { auth: none input: NarrowInput output: NarrowInput run: echo_narrow(input) }
route POST /nullable { auth: none input: NullableInput output: NullableInput run: echo_nullable(input) }
route POST /dynamic { auth: none input: RawInput output: Customer.label run: dynamic_label(input) }
route POST /unused { auth: none input: RawInput output: Accepted run: validate_unused(input) }
route GET /literal { auth: none output: Customer.label run: literal_label() }
`);
const build = Bun.spawnSync([compiler, "build", root], { stdout: "pipe", stderr: "pipe" });
if (build.exitCode !== 0) throw new Error(`Field contract fixture failed to build:\n${build.stdout}\n${build.stderr}`);
const app = await import(pathToFileURL(join(root, "build/target/app.ts")).href);
function request(path: string, body: unknown) {
  return new Request(`https://field.test${path}`, { method: "POST", body: JSON.stringify(body), headers: { "content-type": "application/json" } });
}
async function accepts(path: string, body: unknown) {
  const response = await app.handleRequest(request(path, body));
  const result = await response.json();
  expect(response.status).toBe(200);
  expect(result).toEqual(body);
}
async function rejects(path: string, body: unknown) {
  const response = await app.handleRequest(request(path, body));
  const result = await response.json();
  // GF-032 preserves 422 for decoded body contract violations.
  expect(response.status).toBe(422);
  expect(result.error.code).toBe("invalid_input");
  expect(result.error).not.toHaveProperty("stack");
}

describe("generated field construction and boundary contracts", () => {
  test("non-persistent route error handling does not reference persistence-only classes", () => {
    const generated = readFileSync(join(root, "build/target/app.ts"), "utf8");
    expect(generated).not.toContain("instanceof PersistenceFault");
    expect(generated).toContain("error instanceof RequestDeadlineFault");
  });
  test("a checked field constructor executes as validation", async () => {
    const response = await app.handleRequest(new Request("https://field.test/literal"));
    expect(response.status).toBe(200);
    expect(await response.json()).toBe("valid");
  });
  test("dynamic field constructors validate before their results are used or discarded", async () => {
    for (const path of ["/dynamic", "/unused"]) {
      const valid = await app.handleRequest(request(path, { raw: "valid" }));
      expect(valid.status).toBe(200);
      expect(await valid.json()).toEqual(path === "/dynamic" ? "valid" : { ok: true });
      for (const raw of ["a", "abc", "1234567", "123456789"]) {
        const invalid = await app.handleRequest(request(path, { raw }));
        // Validation after invocation is an internal contract defect in the
        // supported v0.1 target, not invalid HTTP input (runtime-target §Runtime boundaries).
        expect(invalid.status).toBe(500);
        const body = await invalid.json();
        expect(body.error.code).toBe("internal_fault");
        expect(body.error).not.toHaveProperty("details");
        expect(body.error).not.toHaveProperty("stack");
      }
    }
  });
  test("record boundaries enforce both parent and field-local lengths", async () => {
    for (const label of ["four", "123456"]) await accepts("/customer", { label, maybe: null });
    for (const label of ["a", "abc", "1234567", "123456789"]) await rejects("/customer", { label, maybe: null });
  });
  test("chained field references preserve every inherited refinement", async () => {
    for (const label of ["12345", "123456"]) await accepts("/outer", { label, maybe: null });
    for (const label of ["a", "abc", "four", "1234567", "123456789"]) await rejects("/outer", { label, maybe: null });
  });
  test("named refinements of field types retain parent and own constraints", async () => {
    for (const label of ["four", "12345"]) await accepts("/narrow", { label });
    for (const label of ["a", "abc", "123456", "123456789"]) await rejects("/narrow", { label });
  });
  test("nullable field references remain nullable through multiple records", async () => {
    await accepts("/outer", { label: "12345", maybe: null });
    await accepts("/outer", { label: "12345", maybe: "four" });
    await rejects("/outer", { label: "12345", maybe: "abc" });
    await rejects("/outer", { label: "12345", maybe: "1234567" });
    await rejects("/outer", { label: "12345" });
  });
  test("named nullable refinements accept null and enforce constraints when present", async () => {
    await accepts("/nullable", { label: null });
    await accepts("/nullable", { label: "four" });
    await accepts("/nullable", { label: "12345" });
    await rejects("/nullable", { label: "abc" });
    await rejects("/nullable", { label: "123456" });
  });
  test("omission bypasses only an optional field, never its supplied constraints", async () => {
    await accepts("/customer", { label: "four", maybe: null });
    await accepts("/customer", { label: "four", maybe: null, note: "four" });
    await rejects("/customer", { label: "four", maybe: null, note: "abc" });
    await rejects("/customer", { label: "four", maybe: null, note: null });
  });
  test("field validation evaluates a supplied nullable value once", async () => {
    let reads = 0;
    const decoded = { label: "12345", get maybe() { reads += 1; return "four"; } };
    const input = request("/outer", {});
    // Request.json is the boundary input. The accessor exposes duplicate reads
    // that ordinary JSON values would hide without changing generated code.
    Object.defineProperty(input, "json", { value: async () => decoded });
    const response = await app.handleRequest(input);
    expect(response.status).toBe(200);
    expect(await response.json()).toEqual({ label: "12345", maybe: "four" });
    expect(reads).toBe(1);
  });
});


test("JSON syntax remains 400 while unexpected decoder and validation faults stay internal", async () => {
  const malformed = new Request("https://field.test/customer", { method: "POST", body: "{" });
  const invalid = await app.handleRequest(malformed);
  expect(invalid.status).toBe(400);
  expect((await invalid.json()).error.code).toBe("invalid_request");
  const canary = "boundary-fault-canary-do-not-disclose";
  for (const validationFault of [false, true]) {
    const incoming = request("/customer", {});
    Object.defineProperty(incoming, "json", { value: async () => {
      if (!validationFault) throw new Error(canary);
      return new Proxy({}, { ownKeys() { throw new SyntaxError(canary); } });
    } });
    const response = await app.handleRequest(incoming);
    expect(response.status).toBe(500);
    const body = await response.json();
    expect(body.error.code).toBe("internal_fault");
    expect(JSON.stringify(body)).not.toContain(canary);
  }
});
