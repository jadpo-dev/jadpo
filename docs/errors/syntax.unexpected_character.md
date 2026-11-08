# `syntax.unexpected_character`

A file holds a character that isn't part of Jadpo.

## Why Jadpo refuses it

Jadpo source is names, literals, brackets and operators such as `+` and
`==`, and this character is none of them. Names use ASCII letters, digits and
`_`, and any other character belongs in a string. A carriage return alone at
the end of a line is this error too, since it would join that line to the
next.

## What it looks like

main.jadpo:

```jadpo check project=e-unexpected-character expect=syntax.unexpected_character
type Count = Int {
    min: 0 $
}
```

```text output
error: `$` isn't part of Jadpo
 --> main.jadpo:2:12
  |
2 |     min: 0 $
  |            ^ not Jadpo
  reason: Jadpo source is names, literals, brackets and operators such as `+` and `==`; this character has no meaning.
  next: Remove `$` (agent)
  rule: syntax.unexpected_character
  help: https://jadpo.dev/docs/errors/syntax.unexpected_character

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the character shouldn't be there**, remove it:

```jadpo check project=e-unexpected-character-fix
type Count = Int {
    min: 0
}
```

`Count` now has a minimum of `0`.

**If it is part of some text**, put it inside a string, in double quotes:

```jadpo check project=e-unexpected-character-fix-2
type Note = Text {}

function dollars() -> Note {
    return "$"
}
```

`dollars` now gives back the text `$`.

## Related

- [Source files](../language/01-project-layout-and-modules.md#source-files)
- [Naming](../language/01-project-layout-and-modules.md#naming)
