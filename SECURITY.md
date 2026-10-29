# Security policy

## Reporting a vulnerability

Please report a vulnerability privately, through GitHub's private
vulnerability reporting: on [richardwillars/jadpo](https://github.com/richardwillars/jadpo),
open **Security**, then **Report a vulnerability**. Don't open a public
issue, pull request or discussion for it.

Say what is affected (a revision or release), what an attacker can do, and
how to reproduce it: a Jadpo project, a request, a command. A failing test
is ideal.

## What is in scope

Everything in this repository:

- the compiler and the checks it promises (`jadpo check`): policy, field
  restrictions, tenancy, identity, secrets and what package code may do;
- the runtime and both hosts, native and Cloudflare Workers, and the code
  `jadpo build` generates;
- the `jadpo` command, including the Rust toolchain it fetches, `jadpo
  dev` and the package commands;
- the VS Code extension in `editors/vscode`;
- the packages under `packages/`;
- the GitHub workflows and the release artefacts they build.

An app built with Jadpo is its owner's: report a flaw in its own code to
them. Report a flaw in a third-party package to its publisher, unless the
compiler let it do something [package code may not](docs/language/11-integrations.md#what-package-code-may-do),
which is in scope here. Floods of requests that need no flaw in Jadpo to
work are out of scope.

## What to expect

Jadpo is experimental and has no release yet, so fixes land on `main`.
Expect an acknowledgement within a week, then a fix, or an explanation of
why it isn't one. We agree with you when the report and its fix are
published, within 90 days of the report at the latest, and credit you
unless you would rather not be named.

How Jadpo is defended, and against whom, is in [docs/security.md](docs/security.md).
