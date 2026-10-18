# Language reference

Twenty-one areas, all decided, one page per area, numbered in reading order.
These pages are the source of truth for documentation;
`tools/language_docs.py` checks them and renders them as web pages into
`build/language-site/`.

```sh
python3 tools/language_docs.py --check   # also run by tools/verify.py
python3 tools/language_docs.py           # check and render, including decisions.html, the decision log built from every page
```

## Writing a page

- Front matter gives `title`, `area`, `status`, `checked-with` (what checks
  its examples: this checkout's `jadpo` for pages with ` ```jadpo check `
  blocks, and the commit that produced the frozen results for blocks still
  on them) and `rules`: the
  compiler error IDs this area owns (globs allowed). Each rule belongs to one
  area, and the check fails until every owned rule has an example.
- A ` ```jadpo check ` block is checked by the new `jadpo`, built from this
  checkout, with `jadpo check`, which grows with each roadmap chunk: the
  block must check cleanly and be canonically formatted (`jadpo fmt
  --check`), or, with `expect=rule.id`, fail with that rule and be followed
  by a ` ```text output ` block holding the exact plain output (`python3
  tools/language_docs.py --bless` rewrites it). A project with an
  `app.jadpo` block is checked as a project, with the layout rules on;
  without one, its blocks are loose files. Each chunk converts its areas'
  blocks as it builds them.
- Every other non-fragment ` ```jadpo ` block is checked against a frozen
  result in `reference/v0-results.json`, by project name and source, so
  changing its source fails the check; proposed code belongs in `fragment`
  blocks, and each chunk replaces these blocks with `check` blocks as it
  builds its area. Attributes after the language name:
  - `project=name` groups blocks into one multi-file project;
  - `file=path` sets the file's path inside it (default `main.jadpo`);
  - `expect=rule.id` means the project must fail with that error; otherwise it
    must compile and be canonically formatted;
  - `hidden` compiles the block without showing it;
  - `fragment` shows the block without compiling it.
- Under `## Errors`, each `### \`rule.id\`` heading is followed by its example.
  A `check` example shows the compiler's real output; an unmarked one keeps
  its frozen result until its area is built. Each area's errors are designed
  as the roadmap chunk that builds it.
- A line starting with `>` renders as a note.
- Once `status: decided`, a page with `### Decided …` sections under
  `## Review` leads with them and `Decisions`, and collapses the rest of its
  body; every decided page collapses its remaining review notes.
- `## Review` holds status, consistency notes, open questions (a numbered list
  of `**Question?** context *Recommendation:* …`) and decisions. Generated
  public documentation leaves it out.
