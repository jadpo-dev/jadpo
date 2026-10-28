# Jadpo documentation

Jadpo is an experimental programming language for web backends written mainly
by LLMs and reviewed by people. Start with the [project introduction](../README.md),
[teaching examples](../examples/README.md) or the [language map](language/README.md).

## What is current

| Document | What it is |
|---|---|
| [Roadmap](roadmap.md) | The only list of outstanding work: the delivery plan, one chunk per session |
| [Language specification](language/README.md) | Twenty-one areas, all decided; the source of truth for what Jadpo means. The rendered decision log collects every decision |
| [Compiler and runtime architecture](compiler-runtime.md) | The graph-centred build approach (§1) and the architecture |
| [CLI design language](cli-design-language.md) | How every CLI screen and generated page looks |
| [Vision](vision.md) | Why Jadpo exists and who it is for |

Checks run with `python3 tools/verify.py`.

## Where to record things

- A language decision goes on its area page under `## Review` → Decisions.
- A process rule goes in [AGENTS.md](../AGENTS.md).
- Progress is a tick on the [roadmap](roadmap.md): one line naming where the
  chunk's work is written. Its full text lives in git history.
