# `ai.settings_invalid`

`ai:` names `connection:`, a connection with `driver: Driver.openai_compatible` or a package that fills `ai`, and `models:`, each model a `ModelId` setting,
with `default` among them. A key it doesn't take, a connection that
answers no model, a model that isn't a `ModelId` setting and `models:`
without `default` are each this error; `keep_content:`, not built yet, is `toolchain.not_built_yet`.

```jadpo check project=e-ai-settings-invalid file=config.jadpo
config {
    drafting_model: Text {
        default: "meta-llama/Llama-3.1-8B-Instruct"
    }
    model_url: Url {
        default: "http://localhost:8000/v1"
    }
}
```

```jadpo check project=e-ai-settings-invalid file=app.jadpo expect=ai.settings_invalid
application {
    connections: {
        local_model: {
            driver: Driver.openai_compatible
            url: config.model_url
        }
    }
    ai: {
        connection: local_model
        models: {
            default: config.drafting_model
        }
        limits: {
            per_day: {
                tokens: 2000000
            }
        }
    }
}
```

```text output
error: Model `default` must be a `ModelId` setting
 --> app.jadpo:11:22
   |
11 |             default: config.drafting_model
   |                      ^^^^^^^^^^^^^^^^^^^^^ `drafting_model` is `Text`
  reason: Each model is a `ModelId` setting, its provider and name as Hugging Face writes a repository, `openai/gpt-4.1-mini`, so a model is never anything else.
  next: Declare `drafting_model: ModelId` in `config.jadpo` (agent)
  rule: ai.settings_invalid
  help: docs/errors/ai.settings_invalid.md

check failed: 1 error, 2 files
```
