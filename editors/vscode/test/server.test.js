const test = require("node:test");
const assert = require("node:assert");
const { findServer } = require("../server");

const files = (...present) => ({ exists: (p) => present.includes(p) });

test("a bare name is found on PATH", () => {
  const found = findServer("jadpo", {
    pathVariable: "/nope:/usr/local/bin",
    platform: "darwin",
    ...files("/usr/local/bin/jadpo"),
  });
  assert.deepStrictEqual(found, { command: "/usr/local/bin/jadpo" });
});

test("a bare name that is nowhere says so and how to fix it", () => {
  const found = findServer("jadpo", { pathVariable: "/nope", platform: "darwin", ...files() });
  assert.match(found.error, /isn't on your PATH/);
  assert.match(found.error, /jadpo\.server\.path/);
});

test("${workspaceFolder} points at the repository's own build", () => {
  const found = findServer("${workspaceFolder}/target/debug/jadpo", {
    workspaceFolder: "/work/jadpo",
    platform: "darwin",
    ...files("/work/jadpo/target/debug/jadpo"),
  });
  assert.deepStrictEqual(found, { command: "/work/jadpo/target/debug/jadpo" });
});

test("a path with nothing at it names the path", () => {
  const found = findServer("${workspaceFolder}/target/release/jadpo", {
    workspaceFolder: "/work/jadpo",
    platform: "darwin",
    ...files(),
  });
  assert.match(found.error, /No file at `\/work\/jadpo\/target\/release\/jadpo`/);
});

test("${workspaceFolder} with no folder open is an error, not a guess", () => {
  const found = findServer("${workspaceFolder}/jadpo", { platform: "darwin", ...files() });
  assert.match(found.error, /needs a folder open/);
});

test("~ is the home folder", () => {
  const found = findServer("~/bin/jadpo", {
    home: "/Users/me",
    platform: "darwin",
    ...files("/Users/me/bin/jadpo"),
  });
  assert.deepStrictEqual(found, { command: "/Users/me/bin/jadpo" });
});

test("an empty setting is an error", () => {
  assert.match(findServer("  ", { ...files() }).error, /empty/);
});
