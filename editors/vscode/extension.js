// Starts `jadpo lsp` and connects the editor to it. Everything the editor
// shows about Jadpo code (errors, hover, completion, go to definition) comes
// from the compiler through that server, which also says what starts
// completion (a word, `.`, `:` or a digit); this file contains no parser of
// its own.

const vscode = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");
const { findServer } = require("./server");

let client;
let output;

function workspaceFolder() {
  const folders = vscode.workspace.workspaceFolders;
  return folders && folders.length > 0 ? folders[0].uri.fsPath : undefined;
}

async function start() {
  const folder = workspaceFolder();
  const setting = vscode.workspace.getConfiguration("jadpo").get("server.path", "jadpo");
  const found = findServer(setting, { workspaceFolder: folder });
  if (found.error) {
    output.appendLine(`Not started: ${found.error}`);
    const choice = await vscode.window.showErrorMessage(
      `Jadpo language server not started: ${found.error}`,
      "Open Settings",
    );
    if (choice === "Open Settings") {
      await vscode.commands.executeCommand("workbench.action.openSettings", "jadpo.server.path");
    }
    return;
  }
  const run = {
    command: found.command,
    args: ["lsp"],
    transport: TransportKind.stdio,
    options: { cwd: folder },
  };
  client = new LanguageClient(
    "jadpo",
    "Jadpo",
    { run, debug: run },
    {
      documentSelector: [{ scheme: "file", language: "jadpo" }],
      outputChannel: output,
    },
  );
  try {
    await client.start();
    output.appendLine(`Started ${found.command} lsp`);
  } catch (error) {
    client = undefined;
    output.appendLine(`Could not start ${found.command} lsp: ${error}`);
    const choice = await vscode.window.showErrorMessage(
      `Could not start \`${found.command} lsp\`. It needs a Jadpo build that has the \`lsp\` command.`,
      "Show Log",
    );
    if (choice === "Show Log") {
      output.show();
    }
  }
}

async function stop() {
  const old = client;
  client = undefined;
  if (old) {
    try {
      await old.stop();
    } catch {
      // A server that has already gone is the same as one that stopped.
    }
  }
}

async function activate(context) {
  // vscode-languageclient 10 logs through a `LogOutputChannel` (`.error`, `.info`),
  // which only `{ log: true }` gives.
  output = vscode.window.createOutputChannel("Jadpo", { log: true });
  context.subscriptions.push(
    output,
    vscode.commands.registerCommand("jadpo.restartServer", async () => {
      await stop();
      await start();
    }),
    vscode.commands.registerCommand("jadpo.showServerLog", () => output.show()),
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (event.affectsConfiguration("jadpo.server")) {
        await stop();
        await start();
      }
    }),
  );
  await start();
}

async function deactivate() {
  await stop();
}

module.exports = { activate, deactivate };
