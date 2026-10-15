// Where the `jadpo` binary is, without touching VS Code, so a plain
// `node --test` can check it.

const fs = require("fs");
const os = require("os");
const path = require("path");

/** Expands `${workspaceFolder}` and a leading `~` in the setting. */
function expand(setting, { workspaceFolder, home = os.homedir() }) {
  let value = setting.trim();
  if (value.includes("${workspaceFolder}")) {
    if (!workspaceFolder) {
      return { error: "`${workspaceFolder}` needs a folder open in the window." };
    }
    value = value.split("${workspaceFolder}").join(workspaceFolder);
  }
  if (value === "~" || value.startsWith("~/") || value.startsWith("~\\")) {
    value = path.join(home, value.slice(1));
  }
  return { value };
}

function isFile(candidate) {
  try {
    return fs.statSync(candidate).isFile();
  } catch {
    return false;
  }
}

/**
 * The file `setting` names: a path as it stands, or a bare name found on
 * `PATH`. Returns `{ command }` or `{ error }` with a message to show.
 */
function findServer(setting, env = {}) {
  const {
    workspaceFolder,
    pathVariable = process.env.PATH || "",
    platform = process.platform,
    exists = isFile,
    home,
  } = env;
  const expanded = expand(setting || "jadpo", { workspaceFolder, home });
  if (expanded.error) {
    return { error: expanded.error };
  }
  const value = expanded.value;
  if (value === "") {
    return { error: "`jadpo.server.path` is empty." };
  }
  const windows = platform === "win32";
  const suffixes = windows ? ["", ".exe", ".cmd", ".bat"] : [""];
  if (value.includes("/") || (windows && value.includes("\\"))) {
    const resolved = path.resolve(workspaceFolder || ".", value);
    const found = suffixes.map((s) => resolved + s).find(exists);
    return found
      ? { command: found }
      : { error: `No file at \`${resolved}\` (from \`jadpo.server.path\`).` };
  }
  const separator = windows ? ";" : ":";
  for (const folder of pathVariable.split(separator).filter(Boolean)) {
    const found = suffixes.map((s) => path.join(folder, value + s)).find(exists);
    if (found) {
      return { command: found };
    }
  }
  return {
    error: `\`${value}\` isn't on your PATH. Install Jadpo, or set \`jadpo.server.path\` to the binary.`,
  };
}

module.exports = { findServer, expand };
