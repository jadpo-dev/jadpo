//! The runtime's plan (chunk 8.1a): everything the runtime reads of a
//! program besides its callables' bodies, as one owned value of plain
//! data. `jadpo-plan` builds it from a checked project, and an executor
//! hands it to the runtime, which builds what each of its parts runs with
//! from it (docs/compiler-runtime.md §2.5 and §2.6). Compiled code holds
//! it as a constant, in its canonical bytes ([`RuntimePlan::encode`]),
//! and decodes it once as it starts ([`RuntimePlan::decode`]).

use serde::{Deserialize, Serialize};

use crate::ai::AiPlan;
use crate::batch::BatchPlan;
use crate::canonical;
use crate::config::ConfigPlan;
use crate::embed::PromptRecording;
use crate::files::FilesConnection;
use crate::http::{JsonTable, RouteTable};
use crate::prompt::PromptPlan;
use crate::provider::ProviderPlan;
use crate::schedule::SchedulesPlan;
use crate::subscriber::EventsPlan;
use crate::types::{FailurePlan, ShapePlan, ValidatorPlan};
use crate::write::PurgePlan;

/// What the runtime reads of a program: see the module.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimePlan {
    /// Each type's rules along its chain, by number: what a value is
    /// checked against wherever it is decoded.
    pub validators: Vec<ValidatorPlan>,
    /// Each object's, entity's and variant's fields, by number.
    pub shapes: Vec<ShapePlan>,
    /// Each declared failure, by number: what the runtime raises.
    pub failures: Vec<FailurePlan>,
    /// The JSON table: the codec of every type kept as its contract's JSON
    /// or read and written by `json.read` and `json.write`, and each event
    /// type's number in it (area 19, chunk 5.5g; area 12, chunk 5.9b).
    pub json: JsonTable,
    /// The settings and the codecs their values are read by (area 10).
    pub config: ConfigPlan,
    /// Every route, the codecs its types cross the boundary with, who can
    /// sign in and where a client's address is read (areas 7, 8 and 10).
    pub routes: RouteTable,
    /// Every subscriber and email, and where an action that writes no
    /// store keeps its events (areas 12 and 16).
    pub events: EventsPlan,
    /// Each `batches.each` call's run, by the number its site decodes
    /// (area 12, chunk 6.4).
    pub batches: Vec<BatchPlan>,
    /// The schedules, where their state is kept, and the stores the tick
    /// sweeps (area 12, chunk 6.2).
    pub schedules: SchedulesPlan,
    /// Each lifecycle's purge, by entity, in entity order (area 5).
    pub purges: Vec<PurgePlan>,
    /// The connections that keep files (area 16, chunk 6.4b).
    pub file_stores: Vec<FilesConnection>,
    /// The connections to provider packages, their operations, webhooks,
    /// queues and polls (area 11).
    pub providers: ProviderPlan,
    /// The connection `ai:` names and its models (area 21), when the
    /// application asks a model.
    pub ai: Option<AiPlan>,
    /// Every prompt that checks (area 21).
    pub prompts: PromptPlan,
    /// Every recording of a prompt's answer, which `jadpo test` reads
    /// (area 21, "Tests").
    pub prompt_recordings: Vec<PromptRecording>,
}

/// Why bytes didn't read as a runtime plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unreadable {
    /// They don't start with [`RuntimePlan::HEADER`]: not a plan, or one
    /// written by a Jadpo that writes another format.
    NotThisFormat,
    /// They start as one but don't hold one.
    Malformed(canonical::Error),
}

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unreadable::NotThisFormat => f.write_str("not a runtime plan this Jadpo writes"),
            Unreadable::Malformed(error) => write!(f, "a malformed runtime plan: {error}"),
        }
    }
}

impl std::error::Error for Unreadable {}

impl RuntimePlan {
    /// What the bytes of a plan start with: what they are, and the format,
    /// which changes whenever the plan's shape does.
    pub const HEADER: &'static [u8] = b"jadpo runtime plan 1\n";

    /// Its canonical bytes: [`RuntimePlan::HEADER`], then the plan in the
    /// canonical byte encoding. The same plan always gives the same bytes;
    /// the one refusal is a value the encoding has no form for, which no
    /// plan holds.
    pub fn encode(&self) -> Result<Vec<u8>, canonical::Error> {
        let body = canonical::to_bytes(self)?;
        let mut bytes = Vec::with_capacity(Self::HEADER.len() + body.len());
        bytes.extend_from_slice(Self::HEADER);
        bytes.extend_from_slice(&body);
        Ok(bytes)
    }

    /// The plan `bytes` hold, as [`RuntimePlan::encode`] wrote it.
    pub fn decode(bytes: &[u8]) -> Result<RuntimePlan, Unreadable> {
        let body = bytes
            .strip_prefix(Self::HEADER)
            .ok_or(Unreadable::NotThisFormat)?;
        canonical::from_bytes(body).map_err(Unreadable::Malformed)
    }
}
