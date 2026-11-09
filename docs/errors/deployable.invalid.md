# `deployable.invalid`

A deployable is written in a form `deployables:` doesn't take.

## Why Jadpo refuses it

`deployables:` names each program the app is built into, each with a block
holding `serves:`, a list of APIs, `runs:`, a list of triggers, and
`target:`, and nothing else. Anything else leaves Jadpo not knowing what to
build, and it won't guess at a program you'd then deploy.

## What it looks like

```jadpo check project=e-deployable-invalid expect=deployable.invalid file=app.jadpo
application {
    deployables: {
        kiosk: {
            serves: main
            target: Target.native
        }
    }
}
```

```text output
error: `kiosk`'s `serves:` isn't a list
 --> app.jadpo:4:21
  |
4 |             serves: main
  |                     ^^^^ not a list
  reason: `serves:` is a list, `serves: [<api>]`.
  next: Write `serves: [<api>]` (agent)
  rule: deployable.invalid
  help: https://jadpo.dev/docs/errors/deployable.invalid

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A setting a deployable doesn't take is refused:

```jadpo check project=e-deployable-invalid-setting expect=deployable.invalid file=app.jadpo
application {
    deployables: {
        kiosk: {
            serve: [main]
            target: Target.native
        }
    }
}
```

```text output
error: `serve` isn't a setting of a deployable
 --> app.jadpo:4:13
  |
4 |             serve: [main]
  |             ^^^^^ not a setting
  reason: A deployable takes `serves:`, the APIs it serves, `runs:`, the background work it runs, and `target:`, what it is built for. Did you mean `serves`?
  next: Use `serves` instead of `serve` (agent)
  rule: deployable.invalid
  help: https://jadpo.dev/docs/errors/deployable.invalid

check failed: 1 error, 1 file
```

So is an entry listed twice:

```jadpo check project=e-deployable-invalid-twice expect=deployable.invalid file=app.jadpo
application {
    deployables: {
        kiosk: {
            serves: [main, main]
            target: Target.native
        }
    }
}
```

```text output
error: `kiosk`'s `serves:` lists `main` twice
 --> app.jadpo:4:28
  |
4 |             serves: [main, main]
  |                            ^^^^ listed again
  reason: Each entry of `serves:` is listed once.
  next: Remove the second `main` (agent)
  rule: deployable.invalid
  help: https://jadpo.dev/docs/errors/deployable.invalid

check failed: 1 error, 1 file
```

And a `deployables:` with nothing in it, which would build nothing:

```jadpo check project=e-deployable-invalid-none expect=deployable.invalid file=app.jadpo
application {
    deployables: {}
}
```

```text output
error: `deployables:` names no deployable
 --> app.jadpo:2:5
  |
2 |     deployables: {}
  |     ^^^^^^^^^^^ empty
  reason: With `deployables:` declared, the app is built into exactly the deployables it names, so an empty one would build nothing; without it, the app has one deployable, `app`, which serves every API and runs everything.
  next: Name a deployable, `deployables: { api: { serves: [main] } }`, or remove `deployables:` (agent)
  rule: deployable.invalid
  help: https://jadpo.dev/docs/errors/deployable.invalid

check failed: 1 error, 1 file
```

## How to fix it

**If a setting's form is wrong, or an entry is listed twice**, write what
the deployable serves as a list, each entry once:

```jadpo check project=e-deployable-invalid-list file=app.jadpo
application {
    deployables: {
        kiosk: {
            serves: [main]
            target: Target.native
        }
    }
}
```

`kiosk` now answers the routes of `main`.

**If `deployables:` is empty**, remove it:

```jadpo check project=e-deployable-invalid-removed file=app.jadpo
application {}
```

The app now builds into its one deployable, `app`, which serves every API
and runs everything.

## Related

- [Declared deployables](../language/14-tooling.md#declared-deployables)
