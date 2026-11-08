# `type.standard_clash`

A declaration has the name of a standard type, usually one a Jadpo release added after you named your own.

## Why Jadpo refuses it

A Jadpo release added a standard type with the name your app's own type
already has. One name can't mean two things, and an upgrade never changes
what code means silently, so `check` stops until the clash is settled. It
can't be declined, because two declarations can't share a name. Only you
know whether your type means what the standard one means, so Jadpo won't
pick.

## What it looks like

main.jadpo:

```jadpo check project=e-standard-clash expect=type.standard_clash
/// A room booking's reference, from before Jadpo had a `Uuid` of its own.
type Uuid = Text {
    max_length: 36
}
```

```text output
error: `Uuid` is the name of a standard type
 --> main.jadpo:2:6
  |
2 | type Uuid = Text {
  |      ^^^^ standard since Jadpo 0.1.0
  reason: Jadpo 0.1.0 added the standard type `Uuid`, and the application's own type can't share its name, or `Uuid` would mean two things. An upgrade never changes what code means silently, so `check` stops here until this type is swapped for the standard type or renamed.
  next: Run `jadpo fix Uuid swap` to use the standard `Uuid` in its place, or `jadpo fix Uuid rename <NewName>` to keep this one under a name of its own (agent)
  rule: type.standard_clash
  help: https://jadpo.dev/docs/errors/type.standard_clash

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether your type means what the standard type means.

**If it means the same thing**, swap it for the standard type:

```sh
jadpo fix Uuid swap
```

Your declaration goes, every reference to it now means the standard type,
and each field the shape matched is read by the standard field's name.

**If it means something else**, rename it:

```sh
jadpo fix Uuid rename RoomBookingReference
```

Your type keeps its meaning under the new name, and every reference to it
is renamed too.

## Related

- [A release that adds a standard type](../language/19-standard-library.md#a-release-that-adds-a-standard-type)
- [Swap to the standard type](../language/19-standard-library.md#swap-to-the-standard-type)
- [The rename](../language/19-standard-library.md#the-rename)
- [`jadpo fix`](../language/14-tooling.md#jadpo-fix)
