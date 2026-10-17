# CLI design language (area 14)

How everything `jadpo` prints looks and behaves, designed before it is
built (area 14, "CLI design language", 2026-10-13). Twelve screens are drawn
in dark and light terminals in `build/cli-design/index.html` (generated).

## Principles

1. **Calm by default.** A passing run is one line. Passing items are
   counted, not listed. Nothing moves unless work is running.
2. **Colour carries meaning, never decoration.** Each token means one
   thing. Colour goes on symbols, severity words, carets and the next
   command; sentences stay in the terminal's own colour. Everything coloured
   also has a symbol or word, so output reads the same without colour.
3. **Every screen answers what happened, then what next.** The last lines
   are the outcome and, when one exists, the one command to run next.
4. **One shape everywhere.** Compiler errors, test failures and incidents
   share one headline, one label column and one field order, in rich, plain
   and `--json` output.

## Output modes

| Mode | When | Looks like |
|---|---|---|
| Rich | The stream is a terminal and `TERM` is not `dumb` | Colour, symbols, alignment, live lines |
| Plain | Piped, `TERM=dumb`, or `JADPO_PLAIN=1` | No colour, symbols or cursor movement; area 14's `error:` / `-->` form; pickers become numbered lists |
| JSON | `--json` | One JSON document on stdout, nothing else |

Results go to stdout; progress, spinners and prompts to stderr, each stream
checked separately. `JADPO_PLAIN=1` is the screen-reader and log setting:
readers announce every redraw and every gutter glyph.

**Colour**, first match wins: `--color=always|never`; non-empty `NO_COLOR`
(off, keeping bold, layout and symbols); `FORCE_COLOR` `0` off, `1`/`2`/`3`
at 16/256/truecolor; `CLICOLOR_FORCE`; otherwise on in rich mode. Depth:
`FORCE_COLOR`, else `COLORTERM=truecolor|24bit` or Windows Terminal
(`WT_SESSION`) for truecolor, `TERM=*-256color` for 256, else 16.

**Palette.** The exact token values are used only when Jadpo knows the
background: `JADPO_THEME=dark|light`, else an OSC 11 reply within 50 ms
(asked once, only when stdin and stderr are terminals and not in tmux or
screen, which answer unreliably), else `COLORFGBG`. Otherwise the
terminal's own 16 colours, so the user's theme decides, as with `gh`; muted
is then SGR 2 (dim), never bright black, which Solarized Dark makes the
background colour.

**Symbols** are Unicode unless `JADPO_ASCII=1`, `TERM=linux`, or a Windows
console other than Windows Terminal or VS Code. **Animation** runs only when
stderr is a terminal and `CI` is unset; otherwise lines are append-only.

## Colour tokens

Contrast is the worst case over black, `#0c0c0c` (Campbell), `#1e1e1e`
(VS Code), `#282c34` (One Dark) and `#002b36` (Solarized) for dark, and
white, `#fdf6e3` and `#eeeeee` for light; all pass 4.5:1. 256-colour
indexes are from 16–255, which themes do not remap.

| Token | Means | 16 | Dark | 256 | Light | 256 |
|---|---|---|---|---|---|---|
| `failure` | Wrong, and it stops you: errors, failed tests and runs, faults | 31 | `#ff7b70` 5.5 | 209 (5.9) | `#c3262f` 5.0 | 124 (6.4) |
| `warning` | Works, but should change: alerts, retries | 33 | `#ebb54a` 7.5 | 179 (6.8) | `#8b5400` 5.4 | 94 (4.9) |
| `success` | Done and correct: passed, succeeded, wrote, added | 32 | `#4fd69c` 7.6 | 79 (7.9) | `#0f7a55` 4.6 | 23 (6.5) |
| `accent` | Now and next: what is running, the selection, the prompt, the command to run next | 36 | `#5cc3f0` 7.0 | 74 (5.7) | `#0b649f` 5.4 | 24 (6.1) |
| `decision` | Needs a person: `(human)` owners, outcome unknown, a choice to write down | 35 | `#d9a1f7` 6.9 | 183 (7.6) | `#8b3db8` 5.2 | 91 (6.8) |
| `muted` | Secondary: labels, gutters, times, counts, paths in lists | 2 (dim) | `#969da8` 5.1 | 247 (5.2) | `#5d6470` 5.1 | 241 (5.3) |

Body text is the terminal's own foreground; nothing has a background fill.
Some light themes render 16-colour yellow, cyan and dim faintly, which is
why colour only ever repeats a symbol or word.

**Type.** Bold marks the one thing to read first in a block: headline, test
name, group heading, a key. Underline is only for links (OSC 8 where
supported, address printed in full). No italics, which many consoles drop.

## Symbols

Single-width; no emoji, variation selectors or Nerd Font glyphs. `✓ ✗ ›` and
the spinner are East Asian Width "neutral"; shapes, arrows and box drawing
are "ambiguous" and go double-width in CJK terminals set that way, where
`JADPO_ASCII=1` is the answer.

| Meaning | Symbol | ASCII | Token |
|---|---|---|---|
| Success, answered prompt | `✓` | `+` | success |
| Failure, error, fault | `✗` | `x` | failure |
| Alert | `▲` | `!` | warning |
| Needs a person | `◆` | `@` | decision |
| Running (static) | `●` | `*` | accent |
| Running (live) | `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` | `- \ \| /` | accent |
| Pending | `○` | `o` | muted |
| Paused | `‖` | `=` | muted |
| Skipped, no value | `–` | `-` | muted |
| Prompt | `?` | `?` | accent |
| Next; leads to | `→` | `->` | accent in hints, muted in paths |
| Selected row | `›` | `>` | accent |
| Separator | `·` | `,` | muted |
| Gutter, tree | `│ ├─ └─` | `\| \|- '-` | muted |
| Progress | `━━━───` | `###---` | accent, then muted |
| Cut to fit | `…` | `...` | muted |
| Added, removed, changed | `+ - ~` | same | success, failure, warning; only in indented change lists, never column 0 |

## Layout

**The grid.** A status symbol sits in column 0 and its text starts in
column 2; lines without a symbol start in column 2 (Vercel's two-cell
gutter). Labels sit in columns 2–9, lower case, no colon in rich mode;
values start in column 12. The location line is always at indent 2.
Right-aligned values end at column 80. One blank line between blocks, never
two. Lines fit 80 columns; prose wraps, values are cut with `…` and are
whole in `--json`. Commands, paths and links are never cut or broken: a
long one gets a line of its own so it copies whole.

- **Headline.** Symbol, severity word in its colour and bold, two spaces,
  the subject in bold. The words are a closed set: `error`, `failed`,
  `fault`, `alert`, `decision`, `outcome unknown`. Identifiers belong on
  the `rule` line. *Amended 2026-10-17 (Richard, area 1, question 11):*
  `warning` left the set in chunk 5.4b, since every finding is an error;
  a declinable error's `next` offers its `decline` line.
- **Errors.** Area 14's fields in its order: path, code frame,
  `reason`, `next` (`(agent)` muted, `(human)` in decision colour),
  `in use` when a rename needs the names taken (area 1's
  `semantic.duplicate_declaration`: one name and its file per line, in
  two columns, the files muted as paths in lists are), `impact` only when
  it says something, `rule`, `help`. Failed tests and
  incidents use the same headline and labels.
- **Code frames.** A muted `│` gutter with right-aligned line numbers, the
  source as written, and `^` carets in the severity colour with a label of a
  few words. Secondary spans use muted `-`.
- **Command header.** Only for commands that take over a second or open a
  session: bold `jadpo <command>`, then muted facts joined by `·`.
- **Status line.** Symbol and a past-tense sentence (`✓ Wrote …`), its time
  right-aligned and muted when over 100 ms.
- **Lists and tables.** Bold group headings; no borders; muted lower-case
  headers; at least two spaces between columns; numbers right-aligned; state
  as symbol plus word. Under 80 columns a table becomes one block per row
  and optional columns drop.
- **Traces.** Muted `└─ │`; only failures and retries are coloured, so the
  wrong value stays the one red thing in a test report.
- **Diffs.** Change lists use `+ - ~`. Test values use `expected` and
  `actual`, with only the actual value coloured.
- **Prompts and pickers.** `?`, the question in bold, a muted hint. Once
  answered the prompt collapses to `✓ question  answer`, so scrollback
  reads as a record. Pickers filter as you type and mark the selection with
  `›` and an accent bold name. Without a terminal the command fails and
  lists the valid values (decided in area 14).
- **Keys.** One map in every mode: `enter` run, `a` run all, `f` run
  failed, `o` open report, `w` watch, `/` search, `esc` back, `q` quit.
  Every live key is in the key bar, written lower case, bold key then muted
  action, two spaces between pairs.
- **Summary footer.** Last: symbol and outcome in bold and its colour, two
  spaces, then facts joined by `·` with problem counts in their colour and
  time last: `✗ 1 failed  87 passed · 88 tests in 2.4 s`.
- **Hint.** At most one, after the footer: `→ jadpo check  confirms the
  migration matches`. Arrow and command in accent, reason muted, command
  exactly as typed. Left out when the output already says what to do.

### Timing

- Something prints within 100 ms (clig.dev).
- A spinner appears only when a step is still running after 300 ms
  (Vercel's delay; cargo waits 500 ms), so quick steps never flash.
- Spinner frames every 80 ms; a fast spinner reads as fast work. ASCII at
  130 ms. Other redraws at most every 100 ms.
- Elapsed time joins the live line after 2 s; a bar appears when the total
  is known and the step can pass 10 s, Nielsen's limit for attention.
- A finished step's live line is replaced by its permanent result line. No
  exit animation, no celebration.
- Interactive views redraw instantly on every key: keyboard actions repeat
  too often to animate.
- Watch mode appends a muted rule (`── 14:02:31 · saved <file> · 6 tests
  reach it ──`) and never clears the screen.

## Plain and `--json`

- Plain output never waits for a key and uses the decided words (`error:`,
  `-->`, `FAIL`) so it can be searched.
- `--json` writes one JSON document to stdout, newline-terminated, or one
  object per line for streaming commands. No prose, nothing on stderr unless
  Jadpo crashes, exit code unchanged, `schemaVersion` in every document.
- Interactive modes refuse `--json` and a missing terminal, naming the flags
  that do the same job.

## Voice and wording

- Short, plain British English (colour, behaviour, "isn't").
- What happened in the reader's terms, then an imperative next step: "Use
  `title` instead of `titel`".
- Source names in backticks, exactly as written.
- Done steps in the past tense ("Wrote"), live ones in the present
  ("Running tests"). Sentences start with a capital; labels, severity words
  and counts are lower case.
- No "please", "successfully", "Oops", exclamation marks or blame.
- Real plurals ("1 file"), never "file(s)". Units after a space (412 ms),
  24-hour times, ISO dates.

## Critique of the mockups

Impeccable's critique ran as two separate reviews (design and the
detector). The detector's nine findings were false positives (the drawn
cursor, ruled tables, `--` in flags). The design review scored 30/40;
everything it raised was fixed:

- Label columns differed (values at 10 or 12, inline in one screen): now
  one grid, values at column 12, location at indent 2.
- Headline words mixed `FAIL`, `outcome_unknown` and plain words: now a
  closed lower-case set (plain output keeps area 13's `FAIL`).
- Watch coloured the wrong value red and the same value green in the trace:
  traces now colour only failures and retries.
- Lines up to 100 columns: everything fits 80 except whole commands and
  links, which now sit on their own line.
- Keys differed between modes (`f` meant "file" and "failed"; `o` was not
  in the bar): one key map, all keys in the bar, lower case.
- Three summary formats, none for incidents: one footer template,
  incidents included; test failures dropped a second next command.
- Accent also meant "running": its meaning is now "now and next", and the
  truecolor values moved towards cyan to match ANSI 36.
- ASCII clashes: decision is `@`, separator `,`; change markers are in the
  table and never in column 0.
- Added a piped (plain) screen, so "reads the same without colour" is
  shown; `JADPO_PLAIN=1` covers screen readers.

## Assumptions in the mockups

- `jadpo incident` keeps a paste's events so later commands can name them
  by request id; area 17 only shows `--from-event <file>`.
- `jadpo schedules --env production`: area 14 says "against an
  environment" but names no flag.

## Sources

- GitHub CLI: [Primer CLI foundations](https://github.com/primer/design/blob/main/content/native/cli/foundations.mdx), [accessible `gh`](https://github.blog/engineering/user-experience/building-a-more-accessible-github-cli/)
- Vercel CLI: [UX guide](https://github.com/vercel/vercel/blob/main/packages/cli/.agents/skills/cli-ux/references/core.md), [spinner delay](https://github.com/vercel/vercel/blob/main/packages/cli/src/util/output/wait.ts)
- [Bun test reporters](https://bun.com/docs/test/reporters); [clack symbols](https://github.com/bombshell-dev/clack/blob/main/packages/prompts/src/common.ts); [Astro's Houston](https://astro.build/blog/welcome-world/)
- [rustc diagnostics](https://rustc-dev-guide.rust-lang.org/diagnostics.html); [cargo progress](https://github.com/rust-lang/cargo/blob/master/src/util/progress.rs); [Elm errors for humans](https://github.com/elm/elm-lang.org/blob/master/pages/news/compiler-errors-for-humans.elm)
- [Lip Gloss](https://github.com/charmbracelet/lipgloss); [pnpm reporters](https://pnpm.io/cli/install); [Turborepo](https://turborepo.dev/docs/reference/run); [Deno draw thread](https://github.com/denoland/deno/blob/main/cli/util/draw_thread.rs)
- [clig.dev](https://clig.dev/); [NO_COLOR](https://no-color.org/); [CLICOLOR](https://bixense.com/clicolors/); [`FORCE_COLOR`](https://nodejs.org/api/tty.html); [supports-color](https://github.com/chalk/supports-color); [COLORTERM](https://github.com/termstandard/colors)
- [Themes and the 16 colours](https://blog.xoria.org/terminal-colors/); [termenv](https://github.com/muesli/termenv); [Windows VT](https://learn.microsoft.com/en-us/windows/console/console-virtual-terminal-sequences); [is-unicode-supported](https://github.com/sindresorhus/is-unicode-supported); [figures](https://github.com/sindresorhus/figures)
- [UAX #11](https://www.unicode.org/reports/tr11/); [terminal width tests](https://www.jeffquast.com/post/ucs-detect-test-results/); [CLI accessibility study](https://research.google/pubs/accessibility-of-command-line-interfaces/)
- [cli-spinners](https://github.com/sindresorhus/cli-spinners/blob/main/spinners.json); [indicatif](https://docs.rs/indicatif/latest/indicatif/struct.ProgressDrawTarget.html); [Nielsen response times](https://www.nngroup.com/articles/response-times-3-important-limits/)
