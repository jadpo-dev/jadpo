# `type.map_key_boundary`

An `Input` or an `Output` holds a map keyed by a type built on something other than `Text` or `Uuid`, though JSON writes a map's keys as text.

## Why Jadpo refuses it

An `Input` or an `Output` crosses a boundary as JSON, where a map is an
object and an object's keys are text. A key built on a time, a number or
an enum would have to be written as text one way and read back another,
and two keys that print the same would clash. Jadpo won't pick a text
form for the key, because a client reads it.

## What it looks like

main.jadpo:

```jadpo check project=e-map-key expect=type.map_key_boundary
type DueAt = Instant {}

type Weight = Int {
    min: 0
}

type DueWeights = Output {
    weights: Map<DueAt, Weight>
}
```

```text output
error: `DueWeights` holds a map keyed by `DueAt`
 --> main.jadpo:8:14
  |
8 |     weights: Map<DueAt, Weight>
  |              ^^^^^^^^^^^^^^^^^^ keys are `DueAt`
  reason: `DueWeights` crosses a boundary as JSON, where a map is an object and its keys are text; `DueAt` is built on `Instant`.
  next: Key the map by a type built on `Text` or `Uuid`, or send a list of objects instead (agent)
  rule: type.map_key_boundary
  help: https://jadpo.dev/docs/errors/type.map_key_boundary

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

The question is whether the client looks entries up by key.

**If it reads every entry in turn**, send a list of objects, each with its
key as a field:

```jadpo fragment
type DueWeight = Output {
    due_at: DueAt
    weight: Weight
}

type DueWeights = Output {
    weights: List<DueWeight>
}
```

```jadpo check project=e-map-key-fix-1 hidden
type DueAt = Instant {}

type Weight = Int {
    min: 0
}

type DueWeight = Output {
    due_at: DueAt
    weight: Weight
}

type DueWeights = Output {
    weights: List<DueWeight>
}
```

The JSON is a list of objects, each with its time in `due_at` as an
`Instant` is written and its weight beside it.

**If it looks entries up by a name**, key the map by a type built on
`Text`:

```jadpo fragment
type Slot = Text {
    max_length: 20
}

type DueWeights = Output {
    weights: Map<Slot, Weight>
}
```

```jadpo check project=e-map-key-fix-2 hidden
type Weight = Int {
    min: 0
}

type Slot = Text {
    max_length: 20
}

type DueWeights = Output {
    weights: Map<Slot, Weight>
}
```

The JSON is an object whose keys are the slots' names.

## Related

- [Collections](../language/02-values-and-types.md#collections)
- [Maps](../language/19-standard-library.md#maps)
