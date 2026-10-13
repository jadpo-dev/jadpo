# Language reference

One page per language area, numbered in reading order. These pages are the
source of truth for documentation; `tools/language_docs.py` checks them and
renders them as web pages into `build/language-site/`.

```sh
python3 tools/language_docs.py --check   # also run by tools/verify.py
python3 tools/language_docs.py           # check and render, including decisions.html, the decision log built from every page
```

## Writing a page

- Front matter gives `title`, `area`, `status`, `checked-with` and `rules`: the
  compiler error IDs this area owns (globs allowed). Each rule belongs to one
  area, and the check fails until every owned rule has an example.
- Every non-fragment ` ```jadpo ` block shows how the archived v0 compiler
  treated it: its result is frozen in `reference/v0-results.json`, and a block
  whose source changes no longer matches and fails the check, so proposed code
  belongs in `fragment` blocks. Attributes after the language name:
  - `project=name` groups blocks into one multi-file project;
  - `file=path` sets the file's path inside it (default `main.jadpo`);
  - `expect=rule.id` means the project must fail with that error; otherwise it
    must compile and be canonically formatted;
  - `hidden` compiles the block without showing it;
  - `fragment` shows the block without compiling it.
- Under `## Errors`, each `### \`rule.id\`` heading is followed by its example.
  The page shows the v0 compiler's real output. The new compiler's errors are
  designed per area as each roadmap chunk builds it.
- `::: today` … `:::` wraps the archived v0 compiler's syntax for something since decided differently; it renders collapsed.
- A line starting with `>` renders as a note, used to mark sections a decision will change before the rework lands.
- Once `status: decided`, the rendered page leads with the `Decided …` and `Decisions` sections and collapses today's reference body and the remaining review notes.
- `## Review` holds status, consistency notes, open questions (a numbered list
  of `**Question?** context *Recommendation:* …`) and decisions. Generated
  public documentation leaves it out.
