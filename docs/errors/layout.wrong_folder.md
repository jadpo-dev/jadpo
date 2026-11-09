# `layout.wrong_folder`

A declaration is in a folder that doesn't hold its kind.

## Why Jadpo refuses it

Each folder of a domain holds one kind of declaration, so its path says what
a file is. An action in `types/` would be missed by anyone looking for the
domain's actions, and the file it should be in stays empty of it.

## What it looks like

An action is declared in `types/`.

```jadpo check project=e-folder file=app.jadpo hidden
application {}
```

`domains/planning/types/complete_todo.jadpo`:

```jadpo check project=e-folder file=domains/planning/types/complete_todo.jadpo expect=layout.wrong_folder
action complete_todo() -> Unit {}
```

```text output
error: Action `complete_todo` is in `types/`
 --> domains/planning/types/complete_todo.jadpo:1:8
  |
1 | action complete_todo() -> Unit {}
  |        ^^^^^^^^^^^^^ belongs in `actions/`
  reason: `types/` holds types shared by several files in the domain; every action has its own file in `actions/`.
  next: Move `complete_todo` to `domains/planning/actions/complete_todo.jadpo` (agent)
  rule: layout.wrong_folder
  help: https://jadpo.dev/docs/errors/layout.wrong_folder

check failed: 1 error, 2 files
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

Root files such as `app.jadpo` hold settings, and code belongs in a domain.
A root file holding code is refused too:

`app.jadpo`:

```jadpo check project=e-folder-root file=app.jadpo expect=layout.wrong_folder
application {}

action ping() -> Unit {}
```

```text output
error: Action `ping` is in `app.jadpo`
 --> app.jadpo:3:8
  |
3 | action ping() -> Unit {}
  |        ^^^^ belongs in a domain
  reason: The root files hold the application's settings; code lives in a domain under `domains/`.
  next: Move `ping` to `domains/<domain>/actions/` (agent)
  rule: layout.wrong_folder
  help: https://jadpo.dev/docs/errors/layout.wrong_folder

check failed: 1 error, 1 file
```

A test file holds tests and only what they use, and `shared/emails/` holds
the layout and partials every domain's emails share, so anything else in
either is refused the same way.

## How to fix it

**If the declaration is right and the folder is wrong**, move it to the
folder `next:` names: `domains/planning/actions/complete_todo.jadpo`.

```jadpo check project=e-folder-fix file=domains/planning/actions/complete_todo.jadpo
action complete_todo() -> Unit {}
```

```jadpo check project=e-folder-fix file=app.jadpo hidden
application {}
```

The action is now among the domain's other actions.

## Related

- [The layout in detail](../language/01-project-layout-and-modules.md#the-layout-in-detail)
- [Domains](../language/01-project-layout-and-modules.md#domains)
- [Files and names](../language/01-project-layout-and-modules.md#files-and-names)
