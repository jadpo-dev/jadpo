//! The runtime plan's dev and test section (chunk 8.1e): what only
//! `jadpo test` and `jadpo dev` read of a program, kept apart from what a
//! deployable runs with. Each test's identity key and what it gives
//! itself (its `config:` and `clock:`, area 13), and the recordings: each
//! provider operation's, which `jadpo dev`'s fake answers from, every
//! recording of the app's own integrations, which `jadpo test` decodes,
//! and every recording of a prompt's answer (area 21, "Tests").
//!
//! It exists only with the `testing` feature, as
//! [`crate::RuntimePlan::testing`], and is written after the rest of the
//! plan's bytes ([`crate::RuntimePlan::encode`]), so a deployable built
//! without the feature holds none of it and refuses bytes that do.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::embed::PromptRecording;
use crate::provider::RecordedPlan;

/// The dev and test section: see the module.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestingPlan {
    /// Each test's number among the callables, by its identity key
    /// (`test main "a large size is rejected"`): the other callables'
    /// keys are in [`crate::RuntimePlan::callables`].
    pub tests: BTreeMap<String, u32>,
    /// What each test that gives itself anything gives, by the test's
    /// number.
    pub setups: BTreeMap<u32, TestSetup>,
    /// Each provider operation's recording's text,
    /// `recordings/<resource>.<operation>.json` in its package, by the
    /// operation's number: what `jadpo dev`'s fake answers from with its
    /// placeholders filled (chunk 7.2d2).
    pub operation_recordings: BTreeMap<u32, String>,
    /// Every recording of the app's own integrations (area 11,
    /// "Provenance and confirmation"; chunk 7.2d2), which `jadpo test` and
    /// the publishing bar decode with its operation's decoders.
    pub recorded: Vec<RecordedPlan>,
    /// Every recording of a prompt's answer, which `jadpo test` reads
    /// (area 21, "Tests").
    pub prompt_recordings: Vec<PromptRecording>,
    /// What the test generated for each person entity makes and erases
    /// (area 13, "Tests generated for each route"; area 15, "Export";
    /// chunk W2b), by the entity's name: a person whose record no builder
    /// can make, or whose erasure isn't built, has none.
    pub erasures: Vec<ErasureTest>,
}

/// The test generated for a person entity (chunk W2b): data builders the
/// runner calls make the person and a record of each kind that belongs to
/// them, then the person is erased, and their export must hold nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErasureTest {
    /// The person entity, by name: `User`.
    pub person: String,
    /// The records it makes, in order, each after the records it refers
    /// to.
    pub makes: Vec<Making>,
    /// `<person>.erase` by the `id` of the person's record, its one input.
    pub erase: crate::write::WritePlan,
}

/// One record the test makes, through its entity's create, as a data
/// builder makes one (area 13, "Data builders").
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Making {
    /// Its entity, by name.
    pub entity: String,
    /// Why the test makes it.
    pub role: Role,
    /// The create, run with no caller, as a builder's in a test is.
    pub create: crate::write::WritePlan,
    /// What fills each of the create's inputs, in order.
    pub inputs: Vec<Fill>,
}

/// Why the test makes a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    /// The person's own record, which the test erases.
    Person,
    /// One record of a kind that belongs to the person, which their
    /// export holds.
    Theirs,
    /// One record of another person entity that refers to them, which
    /// belongs to them but isn't in their export: it is that person's.
    OtherPerson,
    /// A record one of the others refers to and can't be without.
    Needed,
}

/// What fills one input of a create the test makes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Fill {
    /// `none`, for a field that may be absent.
    Absent,
    /// A value of the type the plan's JSON table numbers `json`, keeping
    /// its rules, made as a fake's generated default is; `distinct` for a
    /// field a unique rule holds. It is kept as its field keeps it, by
    /// the validator numbered `validator`.
    Made {
        json: u32,
        distinct: bool,
        validator: Option<u32>,
    },
    /// The `id` of the record made at this place in the test's `makes`.
    Record(u32),
}

/// What a test gives itself (area 13, "Test settings").
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestSetup {
    /// The settings its `config:` changes, each by its number in the
    /// plan's settings with its value in a variable's spelling, read as
    /// the setting's type reads one.
    pub settings: Vec<(u32, String)>,
    /// Where its `clock:` starts its clock, in milliseconds since
    /// 1970-01-01T00:00:00Z.
    pub clock: Option<i64>,
    /// The value its `flags:` gives each flag it names, by the flag's
    /// number in the plan's flags and the value's place among the flag's
    /// values (area 16, "What the compiler checks"; chunk B9).
    pub flags: Vec<(u32, u32)>,
}

impl TestingPlan {
    /// Whether it holds nothing: a plan with no tests or recordings, whose
    /// bytes then end with the rest of the plan.
    pub fn is_empty(&self) -> bool {
        *self == TestingPlan::default()
    }
}
