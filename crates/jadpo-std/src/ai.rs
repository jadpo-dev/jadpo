//! The `ai` capability as plain data (area 21, "The `ai` capability" and
//! "Typed output"; chunk 5.5v3): the connection `ai:` names and the models
//! it lists, and what each `ai.generate` call's site holds, so both
//! executors ask a model the same way.

/// What `max_output:` is when a call leaves it out: the most tokens the
/// model may answer with.
pub const DEFAULT_MAX_OUTPUT: u32 = 1_000;

/// The most `max_output:` may be.
pub const MOST_MAX_OUTPUT: u32 = 1_000_000;

/// What `timeout:` is when a call leaves it out, in milliseconds: 25
/// seconds, so `ai.generate`'s 2 attempts fit a route's longest deadline,
/// 60 seconds (Richard, 2026-10-20).
pub const DEFAULT_TIMEOUT_MS: u64 = 25_000;

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

/// `ai.generate`'s five conditions, as an `attempt`'s arms, a test's
/// `fakes:` and `jadpo calls script` name them.
pub const CONDITIONS: [crate::RuleKey; 5] = [
    crate::RuleKey::Refused,
    crate::RuleKey::Malformed,
    crate::RuleKey::TimedOut,
    crate::RuleKey::OverBudget,
    crate::RuleKey::TooLong,
];

/// The keys `ai:` takes besides `limits:`: `embedding_dimensions:` since
/// chunk 5.5v4, `personal_data:` since chunk 5.5v5b.
pub const KEYS: [&str; 4] = [
    "connection",
    "models",
    "embedding_dimensions",
    PERSONAL_DATA,
];

/// `personal_data: { allowed: }`, the personal-data categories the
/// application's prompts and embedded texts may send its model (area 21,
/// "Personal data sent to a model"; chunk 5.5v5b).
pub const PERSONAL_DATA: &str = "personal_data";

/// The one key `personal_data:` takes.
pub const ALLOWED: &str = "allowed";

/// Area 15's personal-data categories, as `PersonalData.<name>` names
/// them, each with whether it is sensitive (GDPR's special categories and
/// CCPA/CPRA's extra ones), in the page's order.
pub const CATEGORIES: [(&str, bool); 14] = [
    ("identifying", false),
    ("contact", false),
    ("financial", false),
    ("online", false),
    ("demographic", false),
    ("government_id", true),
    ("location", true),
    ("health", true),
    ("biometric", true),
    ("genetic", true),
    ("beliefs", true),
    ("ethnicity", true),
    ("sex_life", true),
    ("criminal", true),
];

/// The enum area 15's categories are values of.
pub const PERSONAL_DATA_ENUM: &str = "PersonalData";

/// The keys of [`KEYS`] every `ai:` gives.
pub const REQUIRED_KEYS: [&str; 2] = ["connection", "models"];

/// `limits:`, which `ai:` must give too, with `per_day:` in it (chunk
/// 5.5v5): nothing is unlimited.
pub const LIMITS: &str = "limits";

/// The keys area 21 gives `ai:` that later work builds, each with the
/// work that does.
pub const LATER_KEYS: [(&str, &str); 1] = [(
    "keep_content",
    "area 15's encrypted fields, after the first build",
)];

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
    /// What fills `ai` through it.
    pub driver: AiDriver,
    /// Each model `models:` names, with the number of the `ModelId`
    /// setting it reads.
    pub models: Vec<(String, u32)>,
    pub schema: SchemaMode,
    /// What the application may spend (area 21, "Budgets"; chunk 5.5v5):
    /// always given by a checked program, since `per_day:` is required.
    pub limits: Option<AiLimits>,
}

/// `ai: { limits: }` (area 21, "Budgets"; chunk 5.5v5), in tokens, with
/// the store its counts are kept in.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AiLimits {
    /// The database the day's counts are kept in: `limits: { store: }`,
    /// or the application's only one.
    pub store: String,
    /// The most every `ai` call of one request, delivery or schedule run
    /// may use together.
    pub per_request: Option<u64>,
    /// The most the application's calls may use in a day, from midnight
    /// UTC.
    pub per_day: u64,
    /// The number of the `Money` setting the day's estimated cost may not
    /// pass, `per_day: { cost: }`.
    pub cost: Option<u32>,
    /// The most each caller's calls may use in a day.
    pub per_caller_per_day: Option<u64>,
}

/// One row of a package's price table (area 21, "How a provider fills
/// it", `models: { prices: }`): what a model costs per million tokens
/// from a day on, in millionths of its currency's major unit, so `0.40`
/// USD a million is `400_000`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AiPrice {
    /// The model's name after its provider, `gpt-4.1-mini`.
    pub model: String,
    /// The day the price holds from, `2026-04-14`.
    pub from: String,
    /// The currency's ISO 4217 code, `USD`.
    pub currency: String,
    pub input: u64,
    pub cached: u64,
    pub output: u64,
}

/// What fills `ai` (area 21, "How a provider fills it").
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AiDriver {
    /// The built-in `Driver.openai_compatible { url: }`, the base its
    /// `/chat/completions` is under (chunk 5.5v3).
    OpenAiCompatible(AiUrl),
    /// A provider package's `provides/ai.jadpo` (chunk 5.5v3b).
    Package(Box<PackageAi>),
}

/// A package's `provide ai { generate: }`, as data: the operation it
/// calls, what its input is made of, and where the answer and what it
/// says about it are in the operation's output.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PackageAi {
    /// `chat.completions.create`, one of the connection's operations.
    pub operation: String,
    /// Each member of its input, by its spelling on the wire.
    pub input: Vec<(String, AiInput)>,
    /// Whether the input reads `request.instructions`: the instructions
    /// are then left out of `request.messages`, which otherwise opens
    /// with them as the `system` message.
    pub instructions_apart: bool,
    /// Where the answer's text is in the output, by field names; `0` is
    /// a list's first item.
    pub answer: Vec<String>,
    /// Where the model's refusal is, by field names: the call is
    /// `refused` when it holds anything.
    pub refused_when: Option<Vec<String>>,
    /// Where the answer as sent says it was cut off, dotted, and the JSON
    /// value there that says so.
    pub cut_off_when: Option<(String, String)>,
    /// Where the tokens used are, by field names; counted by chunk
    /// 5.5v5's budgets.
    pub usage: Option<AiUsage>,
    /// Where in the request the structured mode's envelope is written,
    /// dotted; `None` where the mode takes none.
    pub schema_at: Option<String>,
    /// Each model's context window in tokens, by the model's name as its
    /// `ModelId` writes it after its provider, from the package's table.
    pub contexts: Vec<(String, u64)>,
    /// Each model's prices, from the package's table, when it has one.
    pub prices: Vec<AiPrice>,
}

impl PackageAi {
    /// The price of the model named `name` on `day` (`2026-10-20`): the
    /// row for it holding from the latest day not after it.
    pub fn price(&self, name: &str, day: &str) -> Option<&AiPrice> {
        self.prices
            .iter()
            .filter(|price| price.model == name && price.from.as_str() <= day)
            .max_by(|a, b| a.from.cmp(&b.from))
    }

    /// The context window of the model named `name`, if the package's
    /// table has it.
    pub fn context(&self, name: &str) -> Option<u64> {
        self.contexts
            .iter()
            .find(|(model, _)| model == name)
            .map(|(_, tokens)| *tokens)
    }
}

/// A value a package's `generate:` gives a member of its operation's
/// input: data, never code.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AiInput {
    /// `request.model`: the model's name after its provider.
    Model,
    /// `request.max_output`.
    MaxOutput,
    /// `request.instructions`: the prompt's instructions, the schema's
    /// added where the mode asks for it there.
    Instructions,
    /// `{ each: request.messages  as: { } }`: one object for each message,
    /// each member by its spelling on the wire.
    Messages(Vec<(String, AiInput)>),
    /// `item.role` and `item.content`, inside `as:`.
    Role,
    Content,
    /// The value of the setting numbered here, from the connection's
    /// setting of the package (`settings.<name>`).
    Setting(u32),
    Text(String),
    Int(i64),
    Bool(bool),
}

/// Where a package's answer says how many tokens it used, by field
/// names; `None` for one written as `0`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AiUsage {
    pub input: Option<Vec<String>>,
    pub output: Option<Vec<String>>,
    pub cached: Option<Vec<String>>,
}

/// How many tokens `text` is taken to be when it is checked against a
/// model's context before sending: one for each four bytes of its UTF-8,
/// rounded up, the rule of thumb OpenAI gives. The provider's own count
/// is what its usage reports.
pub fn estimated_tokens(text: &str) -> u64 {
    (text.len() as u64).div_ceil(4)
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

/// `AiStream`, what `ai.stream` gives and a streaming route sends (area
/// 21, "The `ai` capability"; area 7, "Streaming a model's answer"; chunk
/// 5.5v6).
pub const AI_STREAM: &str = "AiStream";

/// One `ai.stream(prompt)` call's site, decoded (area 21; chunk 5.5v6):
/// the prompt it renders and its options. It asks once (`attempts:` is
/// always 1), for text, so it has no type and no schema.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StreamSite {
    /// The prompt's name, `answer_question`.
    pub prompt: String,
    /// The prompt's place in the plan's prompts, and the number of each of
    /// its fields' types in the JSON table, in the entry's order.
    pub entry: u32,
    pub fields: Vec<u32>,
    /// The model's name in `ai: { models: }`.
    pub model: String,
    pub max_output: u32,
    pub timeout_ms: u64,
}

/// What a streaming route sends, as server-sent events (area 7,
/// "Streaming a model's answer"): each piece of the answer as it comes,
/// then exactly one of the two that end it.
pub mod events {
    /// The next part of the answer: `{"text": "..."}`.
    pub const PIECE: &str = "piece";
    /// The answer is whole: `{}`. Always the last.
    pub const DONE: &str = "done";
    /// The answer stopped before it was whole: the failure envelope
    /// (area 4). Always the last.
    pub const ERROR: &str = "error";
    /// The `Content-Type` a stream is sent with.
    pub const CONTENT_TYPE: &str = "text/event-stream";
}
