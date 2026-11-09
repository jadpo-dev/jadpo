# `semantic.own_domain_qualification`

Code calls an action or a query in its own domain through the domain's name.

## Why Jadpo refuses it

The dot in a call names who owns what you call, so a dot means another
domain. Code calling its own domain's actions and queries uses their bare
names, and a dot then always tells a reader the call leaves the domain.

## What it looks like

```jadpo check project=e-own-domain file=app.jadpo hidden
application {}
```

domains/planning/actions/snooze_todo.jadpo:


```jadpo check project=e-own-domain file=domains/planning/actions/snooze_todo.jadpo
action snooze_todo() -> Unit {}
```

domains/planning/actions/tidy_up.jadpo:


```jadpo check project=e-own-domain file=domains/planning/actions/tidy_up.jadpo expect=semantic.own_domain_qualification
action tidy_up() -> Unit {
    planning.snooze_todo()
}
```

```text output
error: `snooze_todo` is in this file's own domain
 --> domains/planning/actions/tidy_up.jadpo:2:5
  |
2 |     planning.snooze_todo()
  |     ^^^^^^^^ own domain
  reason: The dot names who owns what you call, and `snooze_todo` belongs to `planning` itself, so code there calls it by its bare name.
  next: Remove `planning.` and import `snooze_todo` on a `use` line; `jadpo fmt` adds it (agent)
  rule: semantic.own_domain_qualification
  help: https://jadpo.dev/docs/errors/semantic.own_domain_qualification

check failed: 1 error, 3 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the call is to its own domain**, import it and call it by its bare
name:

```jadpo fragment
use planning/actions { snooze_todo }

action tidy_up() -> Unit {
    snooze_todo()
}
```

```jadpo check project=e-own-domain-fix file=domains/planning/actions/tidy_up.jadpo hidden
use planning/actions { snooze_todo }

action tidy_up() -> Unit {
    snooze_todo()
}
```

```jadpo check project=e-own-domain-fix file=app.jadpo hidden
application {}
```

```jadpo check project=e-own-domain-fix file=domains/planning/actions/snooze_todo.jadpo hidden
action snooze_todo() -> Unit {}
```

The call now reads as staying inside `planning`.

## Related

- [Calls name their owner](../language/01-project-layout-and-modules.md#calls-name-their-owner)
