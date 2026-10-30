# `semantic.ai_model_literal`

A model is a `ModelId` setting, so moving to a newer one is a
configuration change reviewed like any other; written in code, it could
change only with a new build. A setting's `default:` and `test:`, and a
test's `config:`, give a setting its value, so they may write one.

```jadpo check project=e-ai-model-literal expect=semantic.ai_model_literal
/// The model drafts fall back on when the usual one is busy.
function fallback_model() -> ModelId {
    return "openai/gpt-4.1-mini"
}
```

```text output
error: `"openai/gpt-4.1-mini"` writes a model in code
 --> main.jadpo:3:12
  |
3 |     return "openai/gpt-4.1-mini"
  |            ^^^^^^^^^^^^^^^^^^^^^ a `ModelId` written out
  reason: A model is a setting, so moving to a newer one is a configuration change reviewed like any other: a `ModelId` setting gives it a `default:`, `ai: { models: }` names it, and a call asks it with `model:`.
  next: Declare a `ModelId` setting with this `default:` in `config.jadpo`, name it in `ai: { models: }`, and ask it with `model:` (agent)
  rule: semantic.ai_model_literal
  help: https://jadpo.dev/docs/errors/semantic.ai_model_literal

check failed: 1 error, 1 file
```
