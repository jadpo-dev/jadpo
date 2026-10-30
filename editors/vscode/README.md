# Jadpo for VS Code

Jadpo language support. The extension starts `jadpo lsp`, the language
server built into the `jadpo` command, so what VS Code shows about your code
is the compiler's own view: the errors `jadpo check` gives, while you type.
The extension has no parser of its own. It adds the colours, and the
language server does everything else.

## What it does

- **Errors as you type**, in the same shape `jadpo check` prints them: what
  is wrong, why, and the next step. You see them for the files you have
  open and for the rest of the project.
- **Quick fixes.** When an error's next step is an obvious repair, a quick
  fix makes it: a misspelt name (`Use keep instead of kepe`), a missing or
  wrong `use` line, or moving a type to a file of its own once a second
  file uses it (the move `jadpo fmt --layout` makes, imports included).
  There are three more:
  - **Decline with a reason** writes the `decline` line a judgement-call
    error asks for, and **Remove decline** removes one.
  - **Swap for the standard type** swaps your type for the standard type
    it copies.
  - **Make this a setting** moves a constant into `config.jadpo`.
- **Format on save**: the layout `jadpo fmt` writes, imports fixed and
  declarations in order. Formatting is on for `.jadpo` files by default
  (`editor.formatOnSave` under `[jadpo]`).
- **Hover** over a name to see its declaration written as Jadpo and its
  `///` description, along with:
  - the failures it can produce, and what calls or uses it;
  - for a query, the SQL it sends, as `jadpo sql` shows it.
- **Go to definition**, from every use of a name and from a `use` line.
  From a `translations.` call it goes to the message and to its wording in
  each language file.
- **Find references**: every place a name is used, read from the
  compiler's own resolution. A private name of the same spelling in
  another domain is never counted.
- **Completion** as you type a word, a `.`, a `:` or a digit:
  - names in scope, with the `use` line a name needs;
  - a value's fields, and a namespace's operations with their signatures;
  - an enum's variants, and the units that fit after a number (`2` then
    `m` where a `Duration` goes);
  - named arguments, type names, the rules after `decline`, and settings
    and messages.

  A call goes in with a placeholder for each required argument.
- **Open Generated File**: a list of the files `jadpo check` writes in
  `build/`. These are the manifest, the library and failure catalogues, the
  reports and OpenAPI. The refresh button runs `jadpo check` again.
- **Highlighting** of keywords, declarations, types, strings, numbers and
  quantities (`15m`), comments, operators, route paths and `use` lines. It
  also gives you comment toggling, bracket matching and indentation.

## Install Jadpo first

The extension needs the `jadpo` command. Each [release](https://github.com/richardwillars/jadpo/releases)
comes with installers:

- **macOS and Linux**:

      curl --proto '=https' --tlsv1.2 -LsSf https://github.com/richardwillars/jadpo/releases/latest/download/jadpo-cli-installer.sh | sh

- **Windows**, in PowerShell:

      powershell -ExecutionPolicy Bypass -c "irm https://github.com/richardwillars/jadpo/releases/latest/download/jadpo-cli-installer.ps1 | iex"

- **npm**: `npm install -g jadpo`.
- **Homebrew**: each release comes with a formula, `jadpo.rb`.

The shell and PowerShell installers put `jadpo` in Cargo's folder,
`~/.cargo/bin` (on Windows, `%USERPROFILE%\.cargo\bin`), and add it to your
`PATH`. If VS Code was open while you installed, restart it so that it sees
the new `PATH`. Then open the folder that holds your project's `app.jadpo`.

## How it finds `jadpo`

It runs the `jadpo` it finds on your `PATH`, from your project's folder, so
the `jadpo` launcher gives it the version your project names. You need no
setting. People working on the compiler can set `jadpo.server.path` to the
path of their own build.

The status bar shows which `jadpo` is running whenever a `.jadpo` file is
open: `jadpo 0.1.0`, with the binary's path when you point at it. Click it
to restart the language server.

If no binary is found, the item reads `jadpo not found`, and a message says
where it looked and opens the setting. If the binary is found but doesn't
start, it reads `jadpo lsp didn't start`, and the log has the reason.

The language server reads the project from the first folder open in the
window. It finds the project by walking up to `app.jadpo`, as every `jadpo`
command does.

## Settings

| Setting | Default | |
|---|---|---|
| `jadpo.server.path` | empty | An override for people working on the compiler: the binary that runs `jadpo lsp`, instead of the `jadpo` on `PATH`. A bare name is looked up on `PATH`; a path is used as it is. `${workspaceFolder}` is the first folder open, and a leading `~` is your home folder. Changing it restarts the server. |
| `jadpo.trace.server` | `off` | `messages` or `verbose` logs the messages between VS Code and the server in the Jadpo log. |

## Commands

| Command | Does |
|---|---|
| Jadpo: Open Generated File | Lists the files `jadpo check` wrote in `build/` and opens the one you choose. The refresh button runs `jadpo check` again. With nothing generated yet, it offers to run it. |
| Jadpo: Restart Language Server | Stops and starts `jadpo lsp`, for example after you install a new `jadpo`. |
| Jadpo: Show Language Server Log | Opens **Output > Jadpo**, which has what the server and `jadpo check` printed. |

## Not here yet

- **Rename** isn't here yet. It will come with `jadpo rename`, which keeps
  a renamed entity field's stored data.
- **Email templates** aren't highlighted.

## Developing the extension

From a checkout of the Jadpo repository:

1. Run `cargo build -p jadpo-cli`.
2. Open `editors/vscode` in VS Code and press F5. A second window runs
   the extension.
3. In that window, open a Jadpo project, and set `jadpo.server.path` to
   the full path of the checkout's `target/debug/jadpo`.

To run the tests, run `npm ci && npm test` in `editors/vscode`. The tests
start that binary through the protocol library the client uses. The
language server's own tests are in `crates/jadpo-lsp/tests`.

To make a package, run `npm run package`, which writes `jadpo-<version>.vsix`.

`tools/checks/editor_grammar.py` is part of `python3 tools/verify.py` and needs no
Node. The highlighting grammar (`syntaxes/jadpo.tmLanguage.json`) is written
by hand, since colouring reads a line at a time and the lexer does not, but
the word lists are not: the check reads the reserved words and contextual
keywords the lexer knows, the HTTP methods it starts a route path after, and
the once-per-project declarations (`application`, `config`, ...), and fails
when the grammar leaves one out or names one the lexer doesn't reserve. When a
keyword is added to the language, add it to the grammar in the group that
fits.
