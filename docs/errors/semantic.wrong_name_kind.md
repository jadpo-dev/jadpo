# `semantic.wrong_name_kind`

A name is used as a type or a failure, and it is something else.

## Why Jadpo refuses it

Each place in the code expects one kind of thing: a field's type, a
callable's `fails` list. A name of the wrong kind would have no meaning
there, so Jadpo reports it rather than guess what you meant.

## What it looks like

main.jadpo:

```jadpo check project=e-wrong-kind expect=semantic.wrong_name_kind
type Count = Int {
    min: 0
}

function doubled(count: Count) -> Count {
    return Count(count * 2)
}

type Pair = Object {
    first: doubled
}
```

```text output
error: `doubled` is a function, not a type
 --> main.jadpo:10:12
   |
10 |     first: doubled
   |            ^^^^^^^ expected a type
  reason: Here Jadpo expects a type, such as `Count` or `List<Count>`.
  next: Name a type here (agent)
  rule: semantic.wrong_name_kind
  help: https://jadpo.dev/docs/errors/semantic.wrong_name_kind

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

A `fails` list, a `reject` and a failure mapping expect a failure:

```jadpo check project=e-wrong-kind-failure expect=semantic.wrong_name_kind
type Count = Int {
    min: 0
}

function doubled(count: Count) fails Count -> Count {
    return Count(count * 2)
}
```

```text output
error: `Count` is a type, not a failure
 --> main.jadpo:5:38
  |
5 | function doubled(count: Count) fails Count -> Count {
  |                                      ^^^^^ expected a failure
  reason: `fails`, `reject` and failure mappings name failures: types declared `= Failure`.
  next: Name a failure here (agent)
  rule: semantic.wrong_name_kind
  help: https://jadpo.dev/docs/errors/semantic.wrong_name_kind

check failed: 1 error, 1 file
```

## How to fix it

**If the field should hold a value**, name its type:

```jadpo fragment
type Pair = Object {
    first: Count
}
```

```jadpo check project=e-wrong-kind-fix-1 hidden
type Count = Int {
    min: 0
}

function doubled(count: Count) -> Count {
    return Count(count * 2)
}

type Pair = Object {
    first: Count
}
```

`Pair.first` now holds a `Count`.

**If the callable has a failure**, declare one and name it:

```jadpo fragment
type TooLarge = Failure {
    kind: FailureKind.conflict
}

function doubled(count: Count) fails TooLarge -> Count {
    if (count > 1000) {
        reject TooLarge
    }
    return Count(count * 2)
}
```

```jadpo check project=e-wrong-kind-fix-2 hidden
type Count = Int {
    min: 0
}

type TooLarge = Failure {
    kind: FailureKind.conflict
}

function doubled(count: Count) fails TooLarge -> Count {
    if (count > 1000) {
        reject TooLarge
    }
    return Count(count * 2)
}
```

`fails` now names something a caller can handle.

## Related

- [Failures](../language/04-failures.md)
- [Signatures and settings](../language/03-logic.md#signatures-and-settings)
