# Developing the Jadpo VS Code extension


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
