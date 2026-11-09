# `naming.round_trip`

An `UpperCamelCase` name has consecutive capitals, so it doesn't convert to its file name and back unchanged.

## Why Jadpo refuses it

Each name converts to a file name and back, so a tool can find a declaration
from its file and the other way round. `HTMLSnippet` becomes
`html_snippet.jadpo`, which reads back as `HtmlSnippet`, so the two directions
disagree. One spelling means every name has exactly one file.

## What it looks like

```jadpo check project=e-round-trip expect=naming.round_trip
type HTMLSnippet = Text {
    max_length: 2000
}
```

```text output
error: `HTMLSnippet` has consecutive capitals
 --> main.jadpo:1:6
  |
1 | type HTMLSnippet = Text {
  |      ^^^^^^^^^^^ reads back as `HtmlSnippet`
  reason: A name must convert to its file name and back unchanged: `HTMLSnippet` becomes `html_snippet.jadpo`, which reads back as `HtmlSnippet`.
  next: Rename it `HtmlSnippet` (agent)
  rule: naming.round_trip
  help: https://jadpo.dev/docs/errors/naming.round_trip

check failed: 1 error, 1 file
```

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the name holds an abbreviation**, capitalise only its first letter:

```jadpo check project=e-round-trip-fix
type HtmlSnippet = Text {
    max_length: 2000
}
```

The name now lives in `html_snippet.jadpo` and converts back unchanged.

## Related

- [Naming](../language/01-project-layout-and-modules.md#naming)
- [Files and names](../language/01-project-layout-and-modules.md#files-and-names)
