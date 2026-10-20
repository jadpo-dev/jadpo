# Changelog

## 1.0.0

- first release

Fills the `ai` capability through Anthropic's Messages API: `ai.generate`
with its structured outputs (`output_config.format`), and `ai.stream` as
its streamed events. Anthropic has no embeddings endpoint, so `ai.embed`
isn't mapped. Each model's context window and price are as Anthropic's
pages gave them on 2026-10-20.
