import { afterAll, beforeAll, expect, test } from "bun:test";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

// runtime-target-v0.1: boundaries apply the same constraints as the compiler.
// type-system §4.3: literal and dynamic constructors validate the same contract.
// These cases preserve the compiler's Unicode scalar-count rule; they do not
// introduce normalization or count user-perceived grapheme clusters instead.
const root = mkdtempSync(join(tmpdir(), "jadpo-unicode-length-"));
afterAll(() => rmSync(root, { recursive: true, force: true }));
const compiler = Bun.env.JADPO_BIN ?? new URL("../../jadpo/target/debug/jadpo", import.meta.url).pathname;
const face = "😀"; // one Unicode scalar, two UTF-16 code units
const combining = "e\u0301"; // two scalars, often displayed as one grapheme
writeFileSync(join(root, "app.jadpo"), `
type One = Text { min_length: 1 max_length: 1 }
type Two = Text { min_length: 2 max_length: 2 }
type OneInput = Object { text: One }
type TwoInput = Object { text: Two }
type FieldInput = Object { text: Text { min_length: 2 max_length: 2 } }
type InheritedInput = Object { text: FieldInput.text }
type NullableInput = Object { text: FieldInput.text? }
type ListInput = Object { values: List<One> { min_length: 2 max_length: 2 } }
type RawInput = Object { text: Text }
function one_literal() -> One { return One("${face}") }
function two_literal() -> Two { return Two("${combining}") }
function dynamic_one(input: RawInput) -> One { return One(input.text) }
action echo_one(input: OneInput) -> OneInput { return input }
action echo_two(input: TwoInput) -> TwoInput { return input }
action echo_field(input: FieldInput) -> FieldInput { return input }
action echo_inherited(input: InheritedInput) -> InheritedInput { return input }
action echo_nullable(input: NullableInput) -> NullableInput { return input }
action echo_list(input: ListInput) -> ListInput { return input }
route GET /literal_one { auth: none output: One run: one_literal() }
route GET /literal_two { auth: none output: Two run: two_literal() }
route POST /one { auth: none input: OneInput output: OneInput run: echo_one(input) }
route POST /two { auth: none input: TwoInput output: TwoInput run: echo_two(input) }
route POST /field { auth: none input: FieldInput output: FieldInput run: echo_field(input) }
route POST /inherited { auth: none input: InheritedInput output: InheritedInput run: echo_inherited(input) }
route POST /nullable { auth: none input: NullableInput output: NullableInput run: echo_nullable(input) }
route POST /list { auth: none input: ListInput output: ListInput run: echo_list(input) }
route POST /dynamic { auth: none input: RawInput output: One run: dynamic_one(input) }
`);
const build = Bun.spawnSync([compiler, "build", root], { stdout: "pipe", stderr: "pipe" });
if (build.exitCode !== 0) throw new Error(`Unicode contract fixture failed to compile:\n${build.stdout}\n${build.stderr}`);
const app = await import(pathToFileURL(join(root, "build/target/app.ts")).href);
let server: ReturnType<typeof Bun.serve>;
beforeAll(() => { server = Bun.serve({ hostname: "127.0.0.1", port: 0, fetch: app.handleRequest }); });
afterAll(() => server?.stop(true));
async function post(path: string, body: unknown) {
  return fetch(new URL(path, server.url), {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body),
  });
}
async function accepts(path: string, body: unknown) {
  const response = await post(path, body);
  expect(response.status, `${path}: ${JSON.stringify(body)}`).toBe(200);
  expect(await response.json()).toEqual(body);
}
async function rejects(path: string, body: unknown) {
  const response = await post(path, body);
  expect(response.status, `${path}: ${JSON.stringify(body)}`).toBe(422);
  expect((await response.json()).error.code).toBe("invalid_input");
}

test("one supplementary character satisfies the same one-scalar boundary as ASCII", async () => {
  for (const text of ["a", "é", face, "𐐀"]) await accepts("/one", { text });
  for (const text of ["", "ab", combining, face + face]) await rejects("/one", { text });
});
test("minimum length does not count a supplementary character twice", async () => {
  for (const text of ["", "a", face]) await rejects("/two", { text });
  for (const text of ["ab", combining, face + "a", face + face]) await accepts("/two", { text });
});
test("field-local and inherited lengths use scalar counts", async () => {
  for (const path of ["/field", "/inherited"]) {
    await accepts(path, { text: face + "a" });
    await accepts(path, { text: combining });
    await rejects(path, { text: face });
    await rejects(path, { text: face + combining });
  }
});
test("nullable wrappers preserve scalar-length constraints for present values", async () => {
  await accepts("/nullable", { text: null });
  await accepts("/nullable", { text: face + face });
  await rejects("/nullable", { text: face });
});
test("collection length remains an element count", async () => {
  await accepts("/list", { values: ["a", face] });
  await accepts("/list", { values: [face, face] });
  for (const values of [[], [face], ["a", "b", "c"], [face, face, face]]) await rejects("/list", { values });
});
test("literals accepted by the compiler execute with the same length contract", async () => {
  for (const [path, text] of [["/literal_one", face], ["/literal_two", combining]]) {
    const response = await fetch(new URL(path, server.url));
    expect(response.status).toBe(200);
    expect(await response.json()).toBe(text);
  }
});
test("dynamic constructors enforce the same scalar bounds", async () => {
  for (const text of ["a", face, "𐐀"]) {
    const response = await post("/dynamic", { text });
    expect(response.status).toBe(200);
    expect(await response.json()).toBe(text);
  }
  for (const text of ["", combining, face + face]) {
    const response = await post("/dynamic", { text });
    // The supported runtime contains invalid dynamic construction as a defect;
    // this regression does not change its separately documented failure model.
    expect(response.status).toBe(500);
    const body = await response.json();
    expect(body.error.code).toBe("internal_fault");
    expect(body.error).not.toHaveProperty("stack");
  }
});
test("the compiler rejects the same multi-scalar literals as a one-scalar type", () => {
  for (const [index, text] of [combining, face + face].entries()) {
    const probe = join(root, `rejected-literal-${index}`); mkdirSync(probe);
    writeFileSync(join(probe, "app.jadpo"), `type One = Text { min_length: 1 max_length: 1 }\nfunction invalid_literal() -> One { return One(${JSON.stringify(text)}) }`);
    const check = Bun.spawnSync([compiler, "check", probe, "--diagnostic-format=json"], { stdout: "pipe", stderr: "pipe" });
    expect(check.exitCode).not.toBe(0);
    expect(check.stdout.toString() + check.stderr.toString()).toContain("TYPE_INVALID_LITERAL");
  }
});
