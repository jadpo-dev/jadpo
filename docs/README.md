# Jadpo documentation

Jadpo is an experimental programming language for web backends written mainly
by LLMs and reviewed by people. Start with the [project introduction](../README.md),
[teaching examples](../examples/README.md) or the [language map](language/README.md).

## What is current

| Document | What it is |
|---|---|
| [Roadmap](roadmap.md) | The only list of outstanding work: the delivery plan, one chunk per session |
| [Language specification](language/README.md) | Twenty-one areas, all decided; the source of truth for what Jadpo means. The rendered decision log collects every decision |
| [Compiler and runtime architecture](compiler-runtime.md) | The graph-centred build approach (§1) and, from roadmap chunk 0.4, the architecture |
| [Decision register](decision-register.md) | Decisions that are not about the language itself |
| [Vision](vision.md) and [product strategy](product-strategy.md) | Why Jadpo exists and who it is for |
| [Experiments](experiments) | Results of earlier performance and runtime experiments (native Rust, WebAssembly, Workers), the baseline for roadmap Phase 9 |
| [Archive](archive/README.md) | Everything superseded, kept for reference |

Checks run with `python3 tools/verify.py`.

## Where to record things

- A language decision goes on its area page under `## Review` → Decisions.
- Any other decision goes in the [decision register](decision-register.md).
- Progress is a tick on the [roadmap](roadmap.md), nothing else.
- Superseded material moves to the [archive](archive/README.md), with a
  pointer to what replaced it.
