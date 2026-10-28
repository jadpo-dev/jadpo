# `type.map_key_boundary`

A map in an `Input` or an `Output` is a JSON object, whose keys are text, so
its key type is built on `Text` or `Uuid` (question 5):

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
  help: docs/errors/type.map_key_boundary.md

check failed: 1 error, 1 file
```
