# Jadpo documentation

**Jadpo — one language for the whole web.** A programming language for
software written mainly by LLMs and reviewed by people.

## What is current

| Document | What it is |
|---|---|
| [Roadmap](roadmap.md) | The only delivery tracker, one chunk per session |
| [Performance catalogue](performance-experiments.md) | Performance hypotheses in impact order, dependencies, retained results and the cross-session experiment routine |
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
- Delivery progress is a tick on the [roadmap](roadmap.md). Performance
  hypotheses and experiment results go in the single
  [performance catalogue](performance-experiments.md), linking retained raw
  records and existing architecture findings rather than another evidence log.
- Superseded material moves to the [archive](archive/README.md), with a
  pointer to what replaced it.
