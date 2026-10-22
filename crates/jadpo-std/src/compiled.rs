//! What compiled code reads of the runtime's plan besides what the
//! runtime's own parts read (roadmap chunk 8.1f; docs/compiler-runtime.md
//! §2.5, "Compiled code"): the plans of its sites, which the interpreter
//! reads from the compiler's plan as it walks a body, and what the action
//! boundary needs of each action. Generated code holds control flow and
//! numbers; everything a number names is here, in the plan's canonical
//! bytes, decoded once as the program starts.

use crate::Constant;
use crate::Isolation;
use crate::ai::{GenerateSite, StreamSite};
use crate::embed::EmbedSite;
use crate::policy::Gate;
use crate::provider::CallSite;
use crate::read::ReadPlan;
use crate::steps::RecordPlan;
use crate::write::WritePlan;

/// How much stack a Workers deployable's WebAssembly module is linked with
/// (`-z stack-size`, which `jadpo build` passes): its own stack, in its
/// memory, below everything else (`--stack-first`), where compiled code
/// keeps what it borrows; a thousand frames of a large function take a
/// few megabytes of it (chunk 8.4a). It is part of the memory every
/// instance starts with.
pub const WASM_STACK_BYTES: usize = 8 * 1024 * 1024;

/// The plans a body's sites name by number.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SitesPlan {
    /// Every literal and field default, decoded for the type it goes
    /// into, by the number its site names (area 2, question 15).
    pub constants: Vec<Constant>,
    /// Each query's read, by the number of its plan.
    pub queries: Vec<ReadPlan>,
    /// Each entity's `get`, by the number of the entity's plan.
    pub gets: Vec<ReadPlan>,
    /// Each `create`, `update` and `delete` site's write, a lifecycle's
    /// transitions among them, by the number of its plan.
    pub writes: Vec<WritePlan>,
    /// Each call that leaves the process from a body, numbered in the
    /// order the callables and their sites are.
    pub calls: Vec<CallPlan>,
}

/// One outside call's site, decoded (areas 11 and 21).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CallPlan {
    /// A provider package's operation.
    Provider(CallSite),
    /// `ai.generate(prompt, as: T)`.
    Generate(GenerateSite),
    /// `ai.stream(prompt)`.
    Stream(StreamSite),
    /// `ai.embed(text, as: V)` or `ai.embed_all(texts, as: V)`.
    Embed(EmbedSite),
}

/// What the runtime's action boundary needs of an action or a fixture
/// (area 6), which the interpreter reads from its callable's plan.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActionPlan {
    /// Its identity key, which a record of its steps names it by.
    pub key: String,
    /// The level a transaction of its own begins at (area 6).
    pub isolation: Isolation,
    /// Who may call it, when root `policy { actions: }` lists it and its
    /// line depends on no record (area 9).
    pub gate: Option<Gate>,
    /// The store its events are kept in when it begins no transaction of
    /// its own (area 12).
    pub outbox: Option<String>,
    /// For an action whose longest way makes several changing calls:
    /// where the runtime keeps its record and its step shape (area 12).
    pub record: Option<RecordPlan>,
}
