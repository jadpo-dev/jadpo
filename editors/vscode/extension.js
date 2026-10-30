// Starts `jadpo lsp` and connects the editor to it. Everything the editor
// shows about Jadpo code (errors, hover, completion, go to definition,
// references, formatting and quick fixes) comes from the compiler through
// that server, which also says what starts completion (a word, `.`, `:`
// or a digit); this file contains no parser of its own.
//
// What the extension shows itself follows docs/cli-design-language.md:
// calm when all is well (a status item saying which `jadpo` runs, no
// notification), and when something goes wrong, what happened and the
// one step to take next.

const path = require("path");
const { execFile } = require("child_process");
const vscode = require("vscode");
const { LanguageClient, State, TransportKind } = require("vscode-languageclient/node");
const { findServer } = require("./server");
const { projectRoot, generatedFiles, pickItems } = require("./generated");

let client;
let output;
let status;
/** `missing`, `starting`, `running`, `stopped` or `failed`. */
let serverState;

const RESTART = { title: "Restart", command: "jadpo.restartServer" };
const SETTINGS = { title: "Open Settings", command: "jadpo.openServerSetting" };
const LOG = { title: "Show Log", command: "jadpo.showServerLog" };

function workspaceFolder() {
  const folders = vscode.workspace.workspaceFolders;
  return folders && folders.length > 0 ? folders[0].uri.fsPath : undefined;
}

function serverSetting() {
  return vscode.workspace.getConfiguration("jadpo").get("server.path", "");
}

/** The status item for a state of the server. */
function show(state, { text, detail, command, busy = false, error = false }) {
  serverState = state;
  status.text = text;
  status.detail = detail;
  status.command = command;
  status.busy = busy;
  status.severity = error ? vscode.LanguageStatusSeverity.Error : vscode.LanguageStatusSeverity.Information;
}

/** An error notification with one button, which runs `command` when chosen. */
async function tell(message, command) {
  const choice = await vscode.window.showErrorMessage(message, command.title);
  if (choice === command.title) {
    await vscode.commands.executeCommand(command.command);
  }
}

async function start() {
  const folder = workspaceFolder();
  const found = findServer(serverSetting(), { workspaceFolder: folder });
  if (found.error) {
    output.error(`Not started: ${found.error}`);
    show("missing", { text: "jadpo not found", detail: found.error, command: SETTINGS, error: true });
    // Not awaited: activation doesn't wait for the notification to close.
    void tell(`The Jadpo language server didn't start: ${found.error}`, SETTINGS);
    return;
  }
  show("starting", { text: "jadpo", detail: `Starting ${found.command} lsp`, busy: true });
  const run = {
    command: found.command,
    args: ["lsp"],
    transport: TransportKind.stdio,
    options: { cwd: folder },
  };
  const started = new LanguageClient(
    "jadpo",
    "Jadpo",
    { run, debug: run },
    {
      documentSelector: [{ scheme: "file", language: "jadpo" }],
      outputChannel: output,
    },
  );
  client = started;
  started.onDidChangeState((event) => {
    if (client === started && event.newState === State.Stopped && serverState === "running") {
      show("stopped", { text: "jadpo lsp stopped", detail: found.command, command: RESTART, error: true });
    }
  });
  try {
    await started.start();
    const version = started.initializeResult?.serverInfo?.version;
    output.info(`Started ${found.command} lsp${version ? `, version ${version}` : ""}`);
    show("running", { text: version ? `jadpo ${version}` : "jadpo", detail: found.command, command: RESTART });
  } catch (error) {
    if (client === started) {
      client = undefined;
    }
    output.error(`${found.command} lsp didn't start: ${error}`);
    show("failed", { text: "jadpo lsp didn't start", detail: found.command, command: LOG, error: true });
    void tell(
      `\`${found.command} lsp\` didn't start. Check the log, or set \`jadpo.server.path\` to a Jadpo that has the \`lsp\` command.`,
      LOG,
    );
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

/** The project the active file, or else the first folder, is in. */
function currentProject() {
  const active = vscode.window.activeTextEditor?.document;
  const start = active && active.uri.scheme === "file" ? path.dirname(active.uri.fsPath) : workspaceFolder();
  return start ? projectRoot(start) : undefined;
}

/** Runs `jadpo check` in `root`, which writes build/, its output in the log; whether it ran. */
async function runCheck(root) {
  const found = findServer(serverSetting(), { workspaceFolder: workspaceFolder() });
  if (found.error) {
    await tell(`\`jadpo check\` didn't run: ${found.error}`, SETTINGS);
    return false;
  }
  const ran = await vscode.window.withProgress(
    { location: vscode.ProgressLocation.Window, title: "Running jadpo check" },
    () =>
      new Promise((resolve) => {
        output.info(`Running ${found.command} check in ${root}`);
        const env = { ...process.env, JADPO_PLAIN: "1" };
        execFile(found.command, ["check"], { cwd: root, env }, (error, stdout, stderr) => {
          for (const line of `${stdout}${stderr}`.trimEnd().split("\n")) {
            output.appendLine(line);
          }
          // `jadpo check` writes build/ whether or not the project has
          // errors, which the editor shows already: only not running is news.
          if (error && typeof error.code !== "number") {
            output.error(`${found.command} check didn't run: ${error.message}`);
            resolve(error.message);
            return;
          }
          resolve(true);
        });
      }),
  );
  if (ran !== true) {
    await tell(`\`jadpo check\` didn't run: ${ran}`, LOG);
    return false;
  }
  return true;
}

/** "Jadpo: Open Generated File": a picker over build/, as `jadpo check` last wrote it. */
async function openGeneratedFile() {
  const root = currentProject();
  if (!root) {
    vscode.window.showInformationMessage("No Jadpo project here. Open the folder that holds `app.jadpo`.");
    return;
  }
  if (generatedFiles(root).length === 0) {
    const run = "Run jadpo check";
    const choice = await vscode.window.showInformationMessage(
      "Nothing generated yet: `jadpo check` writes the files in `build/`.",
      run,
    );
    if (choice !== run || !(await runCheck(root)) || generatedFiles(root).length === 0) {
      return;
    }
  }
  const picker = vscode.window.createQuickPick();
  const fill = () => {
    picker.items = pickItems(generatedFiles(root)).map((item) =>
      item.separator
        ? { label: item.separator, kind: vscode.QuickPickItemKind.Separator }
        : { label: item.label, description: item.description, file: item.file },
    );
  };
  picker.title = "Open Generated File";
  picker.placeholder = "build/, as jadpo check last wrote it";
  picker.matchOnDescription = true;
  picker.buttons = [{ iconPath: new vscode.ThemeIcon("refresh"), tooltip: "Run jadpo check" }];
  fill();
  picker.onDidTriggerButton(async () => {
    picker.busy = true;
    picker.enabled = false;
    if (await runCheck(root)) {
      fill();
    }
    picker.busy = false;
    picker.enabled = true;
  });
  picker.onDidAccept(async () => {
    const chosen = picker.selectedItems[0];
    picker.hide();
    if (chosen?.file) {
      const file = vscode.Uri.file(path.join(root, "build", ...chosen.file.split("/")));
      await vscode.window.showTextDocument(file, { preview: true });
    }
  });
  picker.onDidHide(() => picker.dispose());
  picker.show();
}

async function activate(context) {
  // vscode-languageclient 10 logs through a `LogOutputChannel` (`.error`, `.info`),
  // which only `{ log: true }` gives.
  output = vscode.window.createOutputChannel("Jadpo", { log: true });
  status = vscode.languages.createLanguageStatusItem("jadpo.server", { language: "jadpo" });
  status.name = "Jadpo language server";
  context.subscriptions.push(
    output,
    status,
    vscode.commands.registerCommand("jadpo.restartServer", async () => {
      await stop();
      await start();
    }),
    vscode.commands.registerCommand("jadpo.showServerLog", () => output.show()),
    vscode.commands.registerCommand("jadpo.openServerSetting", () =>
      vscode.commands.executeCommand("workbench.action.openSettings", "jadpo.server.path"),
    ),
    vscode.commands.registerCommand("jadpo.openGeneratedFile", openGeneratedFile),
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
