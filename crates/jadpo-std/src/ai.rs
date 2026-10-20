//! The `ai` capability as plain data (area 21, "The `ai` capability" and
//! "Typed output"; chunk 5.5v3): the connection `ai:` names and the models
//! it lists, and what each `ai.generate` call's site holds, so both
//! executors ask a model the same way.

/// What `max_output:` is when a call leaves it out: the most tokens the
/// model may answer with.
pub const DEFAULT_MAX_OUTPUT: u32 = 1_000;

/// The most `max_output:` may be.
pub const MOST_MAX_OUTPUT: u32 = 1_000_000;

/// What `timeout:` is when a call leaves it out, in milliseconds: 60
/// seconds.
pub const DEFAULT_TIMEOUT_MS: u64 = 60_000;

/// The longest `timeout:` may be, in milliseconds: 10 minutes.
pub const MOST_TIMEOUT_MS: u64 = 600_000;

/// What `attempts:` is when a call leaves it out: one answer and one
/// repair.
pub const DEFAULT_ATTEMPTS: u32 = 2;

/// The most `attempts:` may be.
pub const MOST_ATTEMPTS: u32 = 3;

/// The model a call asks when it names none: `default`.
pub const DEFAULT_MODEL: &str = "default";

/// The most times one attempt's request is sent while the provider can't
/// be reached or asks for it again later (`429`, a server's error), within
/// the call's `timeout:`.
pub const SENDS: u32 = 3;

/// The keys `ai:` takes now.
pub const KEYS: [&str; 2] = ["connection", "models"];

/// The keys area 21 gives `ai:` that later chunks build, each with the
/// chunk that does.
pub const LATER_KEYS: [(&str, &str); 4] = [
    ("embedding_dimensions", "5.5v4"),
    ("limits", "5.5v5"),
    ("personal_data", "5.5v5"),
    ("keep_content", "5.5v5"),
];

/// How far a provider's structured-output mode goes (area 21, "How a
/// provider fills it", `schema: { mode: }`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SchemaMode {
    /// It enforces a subset of JSON Schema: the schema is sent cut to the
    /// keywords it enforces, and the rest is checked by decoding.
    #[default]
    StrictSubset,
    /// It promises JSON, not a shape: the schema goes in the instructions.
    JsonOnly,
    /// No structured mode: the schema goes in the instructions.
    None,
}

/// Where a connection's address comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AiUrl {
    /// The setting numbered here, a `Url`.
    Setting(u32),
    /// Written out in `app.jadpo`.
    Written(String),
}

/// The application's `ai:` (area 21, "Connecting a provider").
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AiPlan {
    /// The connection every `ai` call goes through, `local_model`.
    pub connection: String,
    /// Its driver's address: `Driver.openai_compatible { url: }`, the
    /// base its `/chat/completions` is under.
    pub url: AiUrl,
    /// Each model `models:` names, with the number of the `ModelId`
    /// setting it reads.
    pub models: Vec<(String, u32)>,
    pub schema: SchemaMode,
}

impl AiPlan {
    /// The setting the model called `name` reads.
    pub fn model(&self, name: &str) -> Option<u32> {
        self.models
            .iter()
            .find(|(model, _)| model == name)
            .map(|(_, at)| *at)
    }
}

/// One `ai.generate(prompt, as: T)` call's site, decoded (area 21, "Typed
/// output").
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GenerateSite {
    /// The prompt's name, `answer_question`, as a test's fakes and the
    /// call log name it.
    pub prompt: String,
    /// The prompt's place in the plan's prompts, and the number of each of
    /// its fields' types in the JSON table, in the entry's order: what
    /// renders it.
    pub entry: u32,
    pub fields: Vec<u32>,
    /// The number of `T` in the JSON table, whose codec decodes the answer
    /// as `json.read` does.
    pub answer: u32,
    /// `T`'s JSON Schema, with each field's `///` description, as written
    /// at build time; `None` for a `T` built on `Text`, which asks for
    /// plain text.
    pub schema: Option<String>,
    /// Whether `T`'s JSON is an object: one that isn't is sent in a
    /// strict structured mode as the member `value` of one, since such a
    /// mode wants an object at its root.
    pub object: bool,
    /// The model's name in `ai: { models: }`.
    pub model: String,
    pub max_output: u32,
    pub timeout_ms: u64,
    pub attempts: u32,
}

/// The heading the schema is added under in the instructions, where the
/// provider has no structured mode.
pub const SCHEMA_HEADING: &str = "# Answer format";

/// The member a `T` whose JSON isn't an object is wrapped in, in a strict
/// structured mode.
pub const WRAPPED: &str = "value";
