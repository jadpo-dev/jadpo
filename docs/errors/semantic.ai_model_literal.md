# `semantic.ai_model_literal`

A model's name is written out in code instead of coming from a `ModelId` setting.

## Why Jadpo refuses it

A model is a setting, so moving to a newer one is a configuration change,
reviewed like any other. Written in code, it could change only with a new
build. A setting's `default:` and `test:`, and a test's `config:`, give a
setting its value, so they are the places that may write one.

## What it looks like

main.jadpo:

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

`(agent)` at the end of the `next:` line means your coding agent can make
the fix itself.

## How to fix it

**If the code needs this model**, declare a `ModelId` setting with the
model as its `default:`, and read that:

config.jadpo:

```jadpo check project=e-ai-model-literal-fix file=config.jadpo
config {
    fallback_model: ModelId {
        default: "openai/gpt-4.1-mini"
    }
}
```

main.jadpo:

```jadpo check project=e-ai-model-literal-fix
/// The model drafts fall back on when the usual one is busy.
action fallback_model() -> ModelId {
    return config.fallback_model
}
```

The model can now be changed for a deployment without a new build.

## Related

- [Connecting a provider](../language/21-ai.md#connecting-a-provider)
- [Operations](../language/21-ai.md#operations)
