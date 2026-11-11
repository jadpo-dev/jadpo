# `cli.domain_arguments`

A `jadpo domain` command names a move it doesn't make, a domain the project doesn't have, a new name that is taken or isn't one, or a split or merge the domains can't take.

## Why Jadpo refuses it

`jadpo domain` moves a domain's files and every call into them, and keeps each declaration's identity. A domain's name is written before every call into it, so a name that is generic, taken or not a domain's would leave calls that read wrongly or mean two things; and a domain splits only where nothing references across, so a split that would cut a reference in two is refused before anything changes.

## What it looks like

A new project, whose `hello` domain is renamed `misc`, a name that says nothing about what the domain holds:

```sh run project=e-domain-arguments expect=cli.domain_arguments
jadpo new todo > /dev/null
cd todo
jadpo domain rename hello misc
```

```text output
error: `misc` is too generic for a domain
  reason: A domain is named after the business area it covers, such as `ordering`, so a reader knows what is in it.
  next: Run `jadpo domain rename hello <new-name>` with the name of a business area (agent)
  rule: cli.domain_arguments
  help: https://jadpo.dev/docs/errors/cli.domain_arguments
```

`(agent)` at the end of the `next:` line means your coding agent can make the fix itself.

## How to fix it

**If the name isn't one for a domain**, name the business area the domain covers, in lower_snake_case:

```sh
jadpo domain rename hello greetings
```

The folder, every call into the domain and its `use` lines take the new name, and the identity registry gets one line.

**If a split crosses references**, move what the named declarations reach too, as the `next:` line's command does, or keep the domain whole when everything in it is connected:

```sh
jadpo domain split planning notes Note add_note all_notes list_notes
```

The notes, their tests and their query move to the new domain together, so nothing that stays reaches them.

**If a merge finds a name or a file in both domains**, rename one of the two first, then merge:

```sh
jadpo rename reminders.TodoTitle ReminderTitle
```

The type no longer shares its name with `planning`'s, so the merge can put both in one domain.

## Related

- [`jadpo domain`](../language/14-tooling.md#jadpo-domain)
- [Domains](../language/01-project-layout-and-modules.md#domains)
- [Stable identity](../language/18-records-and-manifest.md#stable-identity)
