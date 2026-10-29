// Drives the real `jadpo lsp` through the protocol library the extension's
// language client is built on, so the handshake, diagnostics, hover, go to
// definition, references, completion, formatting and quick fixes are
// proven to reach a standard client. The
// server's own behaviour is tested in crates/jadpo-cli/tests/lsp.rs and
// crates/jadpo-lsp/tests; this only proves the wire. It runs against the
// binary in JADPO_BIN, or target/debug/jadpo of this checkout (cargo build
// -p jadpo-cli), and is skipped without one.

const test = require("node:test");
const assert = require("node:assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawn } = require("child_process");
const { pathToFileURL } = require("url");
const { StreamMessageReader, StreamMessageWriter } = require("vscode-jsonrpc/node");
const { createProtocolConnection } = require("vscode-languageserver-protocol/node");

const binary = process.env.JADPO_BIN || path.join(__dirname, "../../../target/debug/jadpo");
const FILES = {
  "app.jadpo": "application {}\n",
  "domains/notes/types/body.jadpo": "/// The text of a note.\ntype Body = Text {\n    min_length: 1\n}\n",
  "domains/notes/actions/save_note.jadpo":
    "use notes/types { Body }\n\n/// Saves a note.\naction save_note(body: Body) -> Body {\n    return body\n}\n",
  "domains/notes/actions/broken.jadpo":
    "use notes/types { Body }\n\naction broken(body: Body) -> Body {\n    return bdy\n}\n",
};

test("jadpo lsp answers a standard client", { skip: !fs.existsSync(binary) && `no ${binary}` }, async () => {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "jadpo-vscode-")));
  for (const [file, text] of Object.entries(FILES)) {
    fs.mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    fs.writeFileSync(path.join(root, file), text);
  }
  const uri = (file) => pathToFileURL(path.join(root, file)).toString();

  const child = spawn(binary, ["lsp"], { cwd: root });
  const connection = createProtocolConnection(
    new StreamMessageReader(child.stdout),
    new StreamMessageWriter(child.stdin),
  );
  const diagnostics = new Map();
  const waiting = new Map();
  connection.onNotification("textDocument/publishDiagnostics", (params) => {
    diagnostics.set(params.uri, params.diagnostics);
    waiting.get(params.uri)?.(params.diagnostics);
  });
  connection.onRequest("client/registerCapability", () => null);
  connection.listen();
  const published = (u) =>
    diagnostics.has(u)
      ? Promise.resolve(diagnostics.get(u))
      : new Promise((resolve) => waiting.set(u, resolve));

  try {
    const init = await connection.sendRequest("initialize", {
      processId: process.pid,
      rootUri: pathToFileURL(root).toString(),
      workspaceFolders: [{ uri: pathToFileURL(root).toString(), name: "notes" }],
      capabilities: { workspace: { didChangeWatchedFiles: { dynamicRegistration: true } } },
    });
    assert.strictEqual(init.capabilities.hoverProvider, true);
    assert.strictEqual(init.capabilities.definitionProvider, true);
    assert.strictEqual(init.capabilities.referencesProvider, true);
    assert.strictEqual(init.capabilities.documentFormattingProvider, true);
    for (const trigger of [".", ":", "2"]) {
      assert.ok(init.capabilities.completionProvider.triggerCharacters.includes(trigger), trigger);
    }
    await connection.sendNotification("initialized", {});

    const problems = await published(uri("domains/notes/actions/broken.jadpo"));
    assert.strictEqual(problems.length, 1);
    assert.strictEqual(problems[0].code, "type.unknown_value");

    const save = { uri: uri("domains/notes/actions/save_note.jadpo") };
    const hover = await connection.sendRequest("textDocument/hover", {
      textDocument: save,
      position: { line: 3, character: 9 },
    });
    assert.match(hover.contents.value, /action save_note\(body: Body\) -> Body/);

    const definition = await connection.sendRequest("textDocument/definition", {
      textDocument: save,
      position: { line: 3, character: 25 },
    });
    assert.strictEqual(definition.uri, uri("domains/notes/types/body.jadpo"));

    const references = await connection.sendRequest("textDocument/references", {
      textDocument: save,
      position: { line: 3, character: 25 },
      context: { includeDeclaration: true },
    });
    assert.deepStrictEqual(
      references.map((r) => r.uri).sort(),
      [
        uri("domains/notes/actions/broken.jadpo"),
        uri("domains/notes/actions/broken.jadpo"),
        uri("domains/notes/actions/broken.jadpo"),
        uri("domains/notes/actions/save_note.jadpo"),
        uri("domains/notes/actions/save_note.jadpo"),
        uri("domains/notes/actions/save_note.jadpo"),
        uri("domains/notes/types/body.jadpo"),
      ],
    );

    const formatting = await connection.sendRequest("textDocument/formatting", {
      textDocument: save,
      options: { tabSize: 4, insertSpaces: true },
    });
    assert.deepStrictEqual(formatting, []);

    const fixes = await connection.sendRequest("textDocument/codeAction", {
      textDocument: { uri: uri("domains/notes/actions/broken.jadpo") },
      range: { start: { line: 3, character: 12 }, end: { line: 3, character: 12 } },
      context: { diagnostics: problems },
    });
    assert.ok(fixes.some((action) => action.kind === "quickfix" && action.edit), JSON.stringify(fixes));

    const completion = await connection.sendRequest("textDocument/completion", {
      textDocument: save,
      position: { line: 4, character: 11 },
    });
    assert.ok(completion.items.some((item) => item.label === "body"));

    await connection.sendRequest("shutdown");
    await connection.sendNotification("exit");
    await new Promise((resolve) => child.on("exit", resolve));
  } finally {
    connection.dispose();
    child.kill();
    fs.rmSync(root, { recursive: true, force: true });
  }
});
