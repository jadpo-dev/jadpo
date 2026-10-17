# Jadpo for VS Code

Highlighting for `.jadpo` files, and the compiler's own view of your project
through `jadpo lsp`: the errors `jadpo check` gives while you type, hover,
completion and go to definition. The extension has no parser of its own; everything but the
colours comes from the language server (area 14, "Editor support").

## What works

- **Errors** as you type, in the shape `jadpo check` prints them, on the
  files you have open and on the rest of the project.
- **Hover** over a name: its declaration written as Jadpo, its `///`
  description, the failures a function, action or query can produce, and what
  calls or uses it.
- **Completion** as you type a word, a `.`, a `:` or a digit: names in
  scope (with the `use` line a name needs), a value's fields, a namespace's
  operations with their signatures, failures and descriptions, an enum's
  variants (a `Locale`'s with its name, "British English"), the units after
  a number for the type it goes into (`2` then `m` where a `Duration` goes),
  a call's named arguments, type names, the rules after `decline`,
  settings after `config.`, messages after `translations.<domain>.`, and
  keywords. A call with required arguments goes in with a placeholder for
  each. What fits the type the place needs comes first.
- **Go to definition** on a name.
- **Make this a setting** (a refactoring) on a constant's declaration or any
  read of it: the move `jadpo config move` makes, into `config.jadpo`, with
  every read rewritten; where the move is refused, the action says why.
- **Highlighting**: keywords, declarations, types, strings with escapes,
  numbers and quantities (`15m`), `//` and `///` comments, operators, route
  paths with their `{placeholders}`, and `use` lines.
- Comment toggling, bracket matching, auto-closing brackets and quotes, and
  indentation.

## What doesn't yet

Find references, rename, formatting, most quick fixes, hover on a
query's SQL, and a command to open generated files. The server doesn't offer
them yet, and the extension shows only what the server offers.
Email templates (not `.jadpo` files) aren't highlighted. `use` lines are
highlighted before the compiler reads them (they arrive with imports).

## Install

You need a `jadpo` binary. From a checkout of this repository:

    cargo build --release -p jadpo-cli     # target/release/jadpo

Then either:

**Run from source** (to try it or change it): open `editors/vscode` in VS Code
and press F5. A second window opens with the extension loaded; open a Jadpo
project in it.

**Install a package**:

    cd editors/vscode
    npm install
    npm run package            # writes jadpo-0.1.0.vsix (not committed)

then in VS Code run "Extensions: Install from VSIX..." and pick the file, or
`code --install-extension jadpo-0.1.0.vsix`. It needs Node 22 or later and VS
Code 1.91 or later.

## Settings

| Setting | Default | |
|---|---|---|
| `jadpo.server.path` | `jadpo` | The binary that runs `jadpo lsp`. A bare name is looked up on `PATH`. To use the build in a checkout, set it to `${workspaceFolder}/target/release/jadpo` (or `target/debug/jadpo`); `${workspaceFolder}` is the first folder open, and a leading `~` is your home folder. Changing it restarts the server. |
| `jadpo.trace.server` | `off` | `messages` or `verbose` logs the protocol traffic to the Jadpo output channel. |

If the binary isn't found, a message says where it looked and offers to open
the setting; the log is in **Output > Jadpo** (command "Jadpo: Show Language
Server Log"). "Jadpo: Restart Language Server" restarts it, for example after
rebuilding `jadpo`.

The server reads the project from the first folder open in the window and
finds it by walking up to `app.jadpo`, so open the project's folder.

## Checks

    npm test                              # in editors/vscode
    python3 tools/editor_grammar.py --check   # from the repository root

`npm test` checks how the binary is found, and, when `target/debug/jadpo`
exists (or `JADPO_BIN` names a binary), starts the real `jadpo lsp` through
the protocol library the client is built on and checks the handshake, an
error, hover and go to definition. The server's behaviour is tested in
`crates/jadpo-cli/tests/lsp.rs`.

`tools/editor_grammar.py` is part of `python3 tools/verify.py` and needs no
Node. The highlighting grammar (`syntaxes/jadpo.tmLanguage.json`) is written
by hand, since colouring reads a line at a time and the lexer does not, but
the word lists are not: the check reads the reserved words and contextual
keywords the lexer knows, the HTTP methods it starts a route path after, and
the once-per-project declarations (`application`, `config`, ...), and fails
when the grammar leaves one out or names one the lexer doesn't reserve. When a
keyword is added to the language, add it to the grammar in the group that
fits.
