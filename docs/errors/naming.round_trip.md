# `naming.round_trip`

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
