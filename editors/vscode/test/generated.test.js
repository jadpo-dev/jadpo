const test = require("node:test");
const assert = require("node:assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { projectRoot, generatedFiles, pickItems } = require("../generated");

function folder(files) {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "jadpo-generated-")));
  for (const file of files) {
    fs.mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    fs.writeFileSync(path.join(root, file), "");
  }
  return root;
}

test("the project is the nearest folder with app.jadpo at or above the start", () => {
  const root = folder(["app.jadpo", "domains/notes/actions/save.jadpo"]);
  assert.strictEqual(projectRoot(path.join(root, "domains/notes/actions")), root);
  assert.strictEqual(projectRoot(root), root);
  fs.rmSync(root, { recursive: true, force: true });
});

test("no app.jadpo anywhere above is no project", () => {
  const root = folder(["notes.jadpo"]);
  assert.strictEqual(projectRoot(root), undefined);
  fs.rmSync(root, { recursive: true, force: true });
});

test("generated files are build/'s readable files, the top level first", () => {
  const root = folder([
    "app.jadpo",
    "build/manifest.json",
    "build/catalogue/libraries.md",
    "build/catalogue/libraries.json",
    "build/catalogue/failures.md",
    "build/report/database.md",
    "build/openapi/openapi.json",
    "build/test/last.json",
    // Cargo's work and the runnable app are not for reading.
    "build/rust/cache/lib.rs",
    "build/target/kiosk/kiosk.wasm",
    "build/target/kiosk/wrangler.json",
    "build/.hidden/x.json",
    "build/catalogue/blob.bin",
  ]);
  assert.deepStrictEqual(generatedFiles(root), [
    "manifest.json",
    "catalogue/failures.md",
    "catalogue/libraries.json",
    "catalogue/libraries.md",
    "openapi/openapi.json",
    "report/database.md",
    "test/last.json",
  ]);
  fs.rmSync(root, { recursive: true, force: true });
});

test("a project with no build/ has nothing generated", () => {
  const root = folder(["app.jadpo"]);
  assert.deepStrictEqual(generatedFiles(root), []);
  fs.rmSync(root, { recursive: true, force: true });
});

test("the picker groups files under their folder, each with its path", () => {
  const items = pickItems(["manifest.json", "catalogue/failures.md", "catalogue/libraries.md"]);
  assert.deepStrictEqual(items, [
    { separator: "build" },
    { label: "manifest.json", description: "build/manifest.json", file: "manifest.json" },
    { separator: "build/catalogue" },
    { label: "failures.md", description: "build/catalogue/failures.md", file: "catalogue/failures.md" },
    { label: "libraries.md", description: "build/catalogue/libraries.md", file: "catalogue/libraries.md" },
  ]);
});
