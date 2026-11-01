# `cli.ai_arguments`

A `jadpo ai` command names no action, one it doesn't have, or a prompt the project doesn't declare.

## Why Jadpo refuses it

`jadpo ai show` prints what one prompt sends, `record` keeps the model's answer to one, and `usage` shows what they cost. Each acts on a prompt the project declares, by its name, so a name that isn't one is refused rather than matched to another.

## What it looks like

A project with no prompts, asked to show one:

```jadpo check project=e-ai-arguments file=app.jadpo hidden
application {}
```

```sh run project=e-ai-arguments expect=cli.ai_arguments
jadpo ai show welcome
```

```text output
error: `welcome` isn't a prompt in this project
  reason: This project declares no prompt.
  next: Declare a prompt in a domain's `prompts/` folder (agent)
  rule: cli.ai_arguments
  help: https://jadpo.dev/docs/errors/cli.ai_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

An action `jadpo ai` doesn't have is refused with the ones it does:

```sh run project=e-ai-arguments expect=cli.ai_arguments
jadpo ai explain
```

```text output
error: `explain` isn't something `jadpo ai` does
  reason: `jadpo ai` can `show`, `record` or `usage`.
  next: Run `jadpo ai show <prompt>` to see what a prompt sends (agent)
  rule: cli.ai_arguments
  help: https://jadpo.dev/docs/errors/cli.ai_arguments
```

## How to fix it

**If the prompt isn't declared yet**, declare it in a domain's `prompts/` folder, with its text in `<name>.prompt` beside it, then show it:

```sh
jadpo ai show welcome
```

It prints the prompt's instructions, the conversation's turns and the request it sends, without calling the model.

**If you meant another action**, use one of `show`, `record` and `usage`:

```sh
jadpo ai usage
```

It shows every prompt's usage today and this month.

## Related

- [`jadpo ai show` and `jadpo ai record`](../language/14-tooling.md#jadpo-ai-show-and-jadpo-ai-record)
- [`jadpo ai usage`](../language/14-tooling.md#jadpo-ai-usage)
- [Prompts as files](../language/21-ai.md#prompts-as-files)
