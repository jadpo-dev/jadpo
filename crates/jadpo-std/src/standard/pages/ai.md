# TokenCount

How many tokens a prompt or an answer holds, by its model's own count.

Use it to count one side of a model call. For a whole call's use, use `TokenUsage`.

```jadpo
TokenCount(350)
```

- It is a whole number from 0. In JSON it is a plain number.

# TokenUsage

The tokens one call to a model used: what it read, what it wrote and what its cache served.

Use it to track what model calls cost. For one number alone, use `TokenCount`.

```jadpo
TokenUsage { input: 1200 output: 350 cached: none }
```

- `cached` is `none` where a provider doesn't say. It can't be above `input`.

# ChatMessage

One message in a conversation with an AI model: who wrote it and what it says.

Use it to build the history you send to a model. The `ChatRole` says who wrote it.

```jadpo
ChatMessage { role: ChatRole.user content: "Summarise this page." }
```

# ChatRole

Who wrote a message in a conversation with a model: `system`, `user`, `assistant` or `tool`.

Use it as part of a `ChatMessage`.

```jadpo
ChatRole.user
```
