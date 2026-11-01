# `cli.test_arguments`

A `jadpo test` command names a path that isn't there, or a seed, a kind or a `--name` it can't use.

## Why Jadpo refuses it

`jadpo test` runs the tests in a file, a folder or a project, in an order its seed sets. A path that isn't there, or a seed that isn't one, would run nothing, and saying it passed would be wrong.

## What it looks like

The seed isn't a number:

```jadpo check project=e-test-arguments file=app.jadpo hidden
application {
    jadpo: "0.2"
}
```

```sh run project=e-test-arguments expect=cli.test_arguments
jadpo test --seed first
```

```text output
error: `first` isn't an order seed
  reason: An order seed is a whole number of 0 or more, as a failed test's rerun command shows it.
  next: Give the seed from the rerun command, such as `--seed 48213` (agent)
  rule: cli.test_arguments
  help: https://jadpo.dev/docs/errors/cli.test_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

A path that isn't there is refused too:

```sh run project=e-test-arguments expect=cli.test_arguments
jadpo test tests/greet.test.jadpo
```

```text output
error: `tests/greet.test.jadpo` doesn't exist
  reason: There is no file or folder at this path.
  next: Give a `.test.jadpo` file, a file and a line as `<file>:<line>`, or a folder (agent)
  rule: cli.test_arguments
  help: https://jadpo.dev/docs/errors/cli.test_arguments
```

## How to fix it

**If you are rerunning a failed test**, use the seed its rerun command gives:

```sh
jadpo test --seed 48213
```

The tests run in the same order as the run that failed.

## Related

- [`jadpo test`](../language/14-tooling.md#jadpo-test)
- [Running tests](../language/14-tooling.md#running-tests)
