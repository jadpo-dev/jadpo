//! The `ai` capability's embeddings and a prompt's recordings as plain
//! data (area 21, "The `ai` capability", "Embeddings and search" and
//! "Tests"; chunk 5.5v4): what each `ai.embed` and `ai.embed_all` call's
//! site holds, and each recording of a prompt's answer kept beside it, so
//! both executors embed, and read a recording, the same way.

/// What `timeout:` is when an `ai.embed` call leaves it out, in
/// milliseconds: 5 seconds, so a route embedding a query fits its
/// 10-second default deadline (Richard, 2026-10-20).
pub const DEFAULT_TIMEOUT_MS: u64 = 5_000;

/// The model an embedding call asks when it names none: `embedding`.
pub const DEFAULT_MODEL: &str = "embedding";

/// The most texts one `ai.embed_all` call takes; more is `too_long`,
/// with nothing sent.
pub const MOST_TEXTS: usize = 100;

/// The operation an embedding call is logged, recorded and scripted as,
/// for `ai.embed` and `ai.embed_all` alike: `local_model.embed`, and a
/// test's `fakes: { ai.embed: [...] }`.
pub const OPERATION: &str = "embed";

/// The most `embedding_dimensions:` may be: pgvector's most.
pub const MOST_DIMENSIONS: u64 = 16_000;

/// One `ai.embed(text, as: V)` or `ai.embed_all(texts, as: V)` call's
/// site, decoded (area 21, "The `ai` capability").
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct EmbedSite {
    /// The number of `V` in the JSON table, whose codec reads each vector
    /// the model answers with, its length and direction checked as
    /// `json.read` checks them.
    pub vector: u32,
    /// `V`'s `dimensions:`: how long each vector a fake makes is.
    pub dimensions: u64,
    /// The model's name in `ai: { models: }`.
    pub model: String,
    pub timeout_ms: u64,
    /// `ai.embed_all`: a list of texts given, a list of vectors in the
    /// same order back.
    pub all: bool,
}

/// The folder beside a prompt that keeps its recordings, one folder per
/// prompt: `prompts/recordings/answer_question/sesame.json`.
pub const RECORDINGS: &str = "recordings";

/// The member a recording keeps the answer in.
pub const ANSWER: &str = "answer";

/// The member a recording keeps the version of the prompt it was made
/// from in, `answer_question@864ad76b`.
pub const VERSION: &str = "version";

/// One recording of a prompt's answer (area 21, "Tests"; chunk 5.5v4),
/// `prompts/recordings/<prompt>/<name>.json`: the answer a model gave,
/// which a test's `fakes: { ai.<prompt>: [recorded("<name>")] }` answers
/// with, and every build decodes through each type the prompt is asked
/// for as.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PromptRecording {
    /// Its path in the project.
    pub file: String,
    /// The prompt's name, `answer_question`, from its folder.
    pub prompt: String,
    /// Its own name, `sesame`, from its file's.
    pub name: String,
    /// The prompt's version now, `answer_question@864ad76b`; `None` when
    /// no prompt of its name is beside it.
    pub version: Option<String>,
    /// The number of each type a call asks the prompt for as, in the JSON
    /// table: what its answer is decoded through.
    pub answers: Vec<u32>,
    /// Its text, as committed.
    pub text: String,
}
