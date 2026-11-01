# `cli.approval_arguments`

A `jadpo approval` command is given an argument it doesn't take, names a `--before` git doesn't know, or has nothing to compare with.

## Why Jadpo refuses it

The approval report is the difference between this change and its before side, the default branch at its merge base. `jadpo approval` reports on the whole project, so it takes no path or name, and with no before side there is no difference to report.

## What it looks like

A word is given after the command:

```jadpo check project=e-approval-arguments file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```sh run project=e-approval-arguments expect=cli.approval_arguments
jadpo approval lending
```

```text output
error: `jadpo approval` takes no arguments
  reason: It was given `lending`.
  next: Run `jadpo approval` (agent)
  rule: cli.approval_arguments
  help: https://jadpo.dev/docs/errors/cli.approval_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

A project git doesn't hold has nothing to compare with:

```sh run project=e-approval-arguments expect=cli.approval_arguments
jadpo approval
```

```text output
error: Nothing to compare with: not a git repository
  reason: The approval report is the difference between this change and its before side, the default branch at its merge base, and there is none.
  next: Commit the project to git with a default branch, or name a base with `--before <branch or commit>` (agent)
  rule: cli.approval_arguments
  help: https://jadpo.dev/docs/errors/cli.approval_arguments
```

## How to fix it

**If you meant to explain this branch's change**, run it with nothing after it, in a project committed to git:

```sh
jadpo approval
```

It writes `build/review/approval.md` and `.json`, comparing with the default branch at its merge base.

**If you meant to compare with something else**, such as the commit a deployed build's events carry, name it:

```sh
jadpo approval --before 8d113e6
```

It writes the same report, comparing with that commit.

## Related

- [Reviewing a change](../language/17-debugging-and-review.md#reviewing-a-change)
- [Comparing two builds](../language/18-records-and-manifest.md#comparing-two-builds)
- [`jadpo approval`](../language/14-tooling.md#jadpo-approval)
