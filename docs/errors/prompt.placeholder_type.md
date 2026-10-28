# `prompt.placeholder_type`

A prompt sends text: a secret, bytes, a file, a value that may be `none`
or a second conversation can't be one of its fields, and a conversation
is sent as the turns, never written in the text.

```jadpo check project=e-prompt-placeholder-type file=app.jadpo hidden
application {}
```

```jadpo check project=e-prompt-placeholder-type file=domains/support/prompts/describe_photo.jadpo expect=prompt.placeholder_type
/// Describes the photo a customer sent of their order.
type DescribePhoto = Prompt {
    photo: Bytes
    reader_locale: Locale
}
```

```text output
error: `DescribePhoto.photo` is `Bytes`, which a prompt can't send
 --> domains/support/prompts/describe_photo.jadpo:3:5
  |
3 |     photo: Bytes
  |     ^^^^^ not text
  reason: A prompt is text. Images, audio and files are sent by a provider package's own operations, not in a prompt.
  next: Remove `photo`, or send what it holds as text (agent)
  rule: prompt.placeholder_type
  help: docs/language/21-ai.html#prompt-placeholder-type

check failed: 1 error, 2 files
```

```jadpo check project=e-prompt-placeholder-type file=domains/support/prompts/describe_photo.prompt hidden
# Instructions

Describe what the photo shows, in the language of {reader_locale}.

# Request

What is wrong with the order in the photo?
```
