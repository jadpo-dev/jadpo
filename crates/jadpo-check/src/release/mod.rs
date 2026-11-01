//! What a release shows (area 11, "What a release shows", "The changelog",
//! "Search", "Yanks, takedowns and advisories"; chunk R3): a package's
//! facts ([`facts`]), the change record since its last release
//! ([`changes`]) and the fingerprint of every public declaration
//! ([`fingerprints`]), each worked out from the compiled package and never
//! from anything its publisher says. `jadpo package check` prints them, and
//! `jadpo package verify --json` writes them for the registry.

mod changes;
mod facts;
mod fingerprints;

pub use changes::{Change, ChangeEntry, behaviour_changed, personal_data_changes, surface_changes};
pub use facts::{
    Facts, FailureFact, HostFact, PersonalDataFact, StandardCandidate, facts, named_holder,
};
pub use fingerprints::{Fingerprint, Shape, fingerprints};
