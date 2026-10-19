# 22 · A garden centre's plant helpline

A garden centre answers gardeners' questions from its own care guides:
each guide is placed by an embedding model, as a vector of what it is
about; a question is placed the same way, and the guides nearest it are
found by meaning, not by words. What a model would be sent is written as
a prompt file, checked like code. It teaches stored vectors, the nearest
query, and prompts.

A vector type stands for the model that made its vectors, so two types
can't be compared. A query orders by `nearest` through an HNSW index in
pgvector, and gives each record with its distance, the same distance
`ai.distance` works out in code. A prompt is a type whose fields are its
placeholders, with its words in a `.prompt` file beside it; free text is
only ever sent as data. Making embeddings and calling a model are the
`ai` capability's, which a later chunk builds, so here the embedding
service's reply arrives as JSON and prompts are rendered, not sent.

## What it teaches

1. **Vector types.** `dimensions:` and `distance:`, a pgvector `vector(4)` column, and the first migration creating the extension. See `domains/helpline/types/guide_meaning.jadpo`, `domains/helpline/entities/care_guide.jadpo` and [area 21, Storing a vector](../../docs/language/21-ai.md#storing-a-vector).
2. **A vector from JSON.** `json.read` checks a vector's length against the type's `dimensions:`. See `domains/helpline/actions/place_guide.jadpo`.
3. **Distance in code.** `ai.distance` between two vectors of one type. See `domains/helpline/actions/likely_duplicate.jadpo`.
4. **The nearest query.** `order: [field nearest value]`, `where: ai.distance(...) <= ...`, `limit:`, and `List<Nearest<T>>`, each record with its `distance`. See `domains/helpline/queries/closest_guides.jadpo` and [area 21, The nearest-neighbour query](../../docs/language/21-ai.md#the-nearest-neighbour-query).
5. **A tuned index.** `kind: IndexKind.hnsw` with `m:` and `ef_construction:`, used in place of the index the query would derive. See `domains/helpline/entities/care_guide.jadpo`, `database/migrations/20261109T0900_initial/postgres.sql` and [area 21, The index](../../docs/language/21-ai.md#the-index).
6. **Prompts as files.** A `Prompt` type and its `.prompt` file, placeholders checked against its fields, closed values in the instructions and free text in the request. See `domains/helpline/prompts/answer_plant_question.jadpo`, `domains/helpline/prompts/answer_plant_question.prompt` and [area 21, Prompts as files](../../docs/language/21-ai.md#prompts-as-files).
7. **Rendering.** `prompts.render` gives the messages a model would be sent, a closing tag in free text escaped so it can't end its block. See `domains/helpline/prompts/answer_plant_question.test.jadpo`.
8. **A shared prompt and a conversation.** A prompt in `shared/prompts/`, whose `List<ChatMessage>` field is sent as the turns before the request. See `shared/prompts/summarise_call.jadpo`.
9. **Retrieval.** The guides nearest a question, rendered into the prompt with it. See `domains/helpline/actions/brief_for_question.jadpo`.

## Run it

From the repository root, build `jadpo` once, then run each command on
this project:

```sh
cargo build --release -p jadpo-cli
target/release/jadpo check examples/22-plant-helpline
target/release/jadpo test examples/22-plant-helpline
target/release/jadpo fmt --check examples/22-plant-helpline
cd examples/22-plant-helpline && ../../target/release/jadpo ai show answer_plant_question
```

- `check` ends with `checked: <n> files` and no errors.
- `test` needs PostgreSQL with pgvector 0.8.0 or later, whose iterative
  scans keep a filtered nearest search filling its limit, and ends with
  `tested: 7 passed, 2 pure tests, 5 action tests`. On a server with an
  older pgvector (Postgres.app ships 0.7.0) it stops before any test
  runs, with `migration.extension_outdated`.
- `ai show` prints the messages the prompt sends, each placeholder as the
  file writes it, inside the block it is sent in, and the prompt's
  version: the first characters of a digest of its two files.

## Try next

- In `answer_plant_question.prompt`, write `{gardener_question}` in the instructions too. `jadpo check` reports `prompt.free_text_in_instructions`: a gardener's words are only ever sent as data.
  <!-- try {"file": "domains/helpline/prompts/answer_plant_question.prompt", "replace": "Never name a pesticide.", "with": "Never name a pesticide. They asked {gardener_question}.", "check": ["prompt.free_text_in_instructions"]} -->
- In `answer_plant_question.prompt`, take the month out of the instructions. `jadpo check` reports `prompt.placeholder_unused`: a field nothing sends is a mistake.
  <!-- try {"file": "domains/helpline/prompts/answer_plant_question.prompt", "replace": "The gardener is asking in {asked_in}, so give the advice for that month.\n", "with": "", "check": ["prompt.placeholder_unused"]} -->
- In `closest_guides.jadpo`, take the `limit:` away. `jadpo check` reports `query.section_required`: a nearest search always says how many it wants.
  <!-- try {"file": "domains/helpline/queries/closest_guides.jadpo", "replace": "    limit: 3\n", "with": "", "check": ["query.section_required"]} -->
- Then read [23-repair-cafe](../23-repair-cafe/README.md), where work carries on after an action, in subscribers.
