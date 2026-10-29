# Agent instructions

This is a Jadpo application. Jadpo is a statically typed language for web
backends, written by coding agents and reviewed by people; no model has
seen it before, so its documentation is your only source. This app is
built with Jadpo {version}; `jadpo version` prints the compiler's version.

The documentation for this version is in `../docs/`:

1. Read `../docs/primer.md` once before you change anything: the layout,
   the syntax, the constructs nearly every app uses and where to look next.
2. For a task, read `../docs/index.json`, pick the topics by title and
   summary, and read only their pages, `../docs/topics/<id>.md`.
3. A compiler error names its rule; read `../docs/errors/<rule>.md` only if
   the error's own reason and fix are not enough.
4. Commands are in `../docs/commands/jadpo-<command>.md`, and `jadpo help`
   lists them.

Check with `jadpo check` and run the tests with `jadpo test`.
