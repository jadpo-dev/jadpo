// The project's generated files (area 14, "The `build/` folder"), for
// "Jadpo: Open Generated File", without touching VS Code, so a plain
// `node --test` can check it. `jadpo check` writes them; this only lists
// what is there.

const fs = require("fs");
const path = require("path");

/** Folders under build/ that hold builds, not files to read. */
const SKIPPED = new Set(["rust", "target"]);

/** What a person can read in the editor. */
const READABLE = new Set([".md", ".json", ".sql", ".txt", ".html", ".csv", ".yaml", ".yml", ".toml", ".xml"]);

/** The folder at or above `start` holding `app.jadpo`, as every command finds it. */
function projectRoot(start) {
  let folder = path.resolve(start);
  for (;;) {
    if (fs.existsSync(path.join(folder, "app.jadpo"))) {
      return folder;
    }
    const parent = path.dirname(folder);
    if (parent === folder) {
      return undefined;
    }
    folder = parent;
  }
}

/** Each readable file under `root`/build/, relative to it: its own files first, then each folder's, by path. */
function generatedFiles(root) {
  const build = path.join(root, "build");
  const found = [];
  const walk = (folder, relative) => {
    let entries;
    try {
      entries = fs.readdirSync(folder, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (entry.name.startsWith(".")) {
        continue;
      }
      const inner = relative ? `${relative}/${entry.name}` : entry.name;
      if (entry.isDirectory()) {
        if (!(relative === "" && SKIPPED.has(entry.name))) {
          walk(path.join(folder, entry.name), inner);
        }
      } else if (entry.isFile() && READABLE.has(path.extname(entry.name))) {
        found.push(inner);
      }
    }
  };
  walk(build, "");
  const top = (file) => (file.includes("/") ? 1 : 0);
  return found.sort((a, b) => top(a) - top(b) || (a < b ? -1 : a > b ? 1 : 0));
}

/** The picker's rows: a separator naming each folder, then its files. */
function pickItems(files) {
  const items = [];
  let group;
  for (const file of files) {
    const folder = path.posix.dirname(file);
    const heading = folder === "." ? "build" : `build/${folder}`;
    if (heading !== group) {
      items.push({ separator: heading });
      group = heading;
    }
    items.push({ label: path.posix.basename(file), description: `build/${file}`, file });
  }
  return items;
}

module.exports = { projectRoot, generatedFiles, pickItems };
