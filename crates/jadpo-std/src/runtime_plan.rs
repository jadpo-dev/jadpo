//! The runtime's plan (chunk 8.1a): everything the runtime reads of a
//! program besides its callables' bodies, as one owned value of plain
//! data. `jadpo-plan` builds it from a checked project, and an executor
//! hands it to the runtime, which builds what each of its parts runs with
//! from it (docs/compiler-runtime.md §2.5 and §2.6). Compiled code holds
//! it as a constant, in its canonical bytes ([`RuntimePlan::encode`]),
//! and decodes it once as it starts ([`RuntimePlan::decode`]). What only
//! `jadpo test` and `jadpo dev` read is its dev and test section,
//! [`crate::testing::TestingPlan`], which exists only with the `testing`
//! feature (chunk 8.1e).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ai::AiPlan;
use crate::batch::BatchPlan;
use crate::canonical;
use crate::compiled::{ActionPlan, SitesPlan};
use crate::config::ConfigPlan;
use crate::connections::Connection;
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
    /// Every connection, in name order, with its driver, the settings
    /// that driver is built from and the targets it runs on (area 11;
    /// chunk 8.1d): what a host builds each connection's driver from.
    pub connections: Vec<Connection>,
    /// The operations of the provider packages' connections, their
    /// webhooks, queues and polls (area 11).
    pub providers: ProviderPlan,
    /// The connection `ai:` names and its models (area 21), when the
    /// application asks a model.
    pub ai: Option<AiPlan>,
    /// Every prompt that checks (area 21).
    pub prompts: PromptPlan,
    /// The locales the application supports, when it declares them (area
    /// 10): what the standard library formats in and
    /// `locale.from_accept_language` chooses among, whichever executor
    /// runs it (chunk 8.1g).
    pub locales: Option<crate::LocalePlan>,
    /// Every message, enum and email subject's wording, by key (area 10):
    /// what a `translations.<message>` call writes, whichever executor runs
    /// it (chunk 8.1i).
    pub translations: crate::translation::TranslationPlan,
    /// Each callable's number, by its identity key, but a test's (chunk
    /// 8.1e): how the runtime finds a callable named in the plan by its
    /// key, a webhook reply's action or a package's own `decode:` and
    /// `encode:` functions.
    pub callables: BTreeMap<String, u32>,
    /// What compiled code's sites name by number (chunk 8.1f): every
    /// constant, each query's and entity `get`'s read, each write site's
    /// plan and each outside call's site.
    pub sites: SitesPlan,
    /// What the action boundary needs of each action and fixture, by its
    /// callable's number (chunk 8.1f).
    pub actions: BTreeMap<u32, ActionPlan>,
    /// What a record of several changing calls keeps of the build it
    /// started under (area 11, "Deploys"; chunk 7.5b), and the
    /// `SigningKey` setting its answers are sealed with, by number.
    pub build: String,
    pub steps_key: Option<u32>,
    /// The background work this program runs besides the routes it serves
    /// (area 14, "Declared deployables"; chunk 8.1o2): everything but in a
    /// declared deployable that runs less.
    pub runs: Runs,
    /// The dev and test section: each test's key and what it gives
    /// itself, and the recordings (chunk 8.1e). Written after the rest
    /// ([`RuntimePlan::encode`]), not as one of its fields.
    #[cfg(feature = "testing")]
    #[serde(skip)]
    pub testing: crate::testing::TestingPlan,
}

/// The background work a program runs besides the routes it serves (area
/// 14, "Declared deployables"; chunk 8.1o2): `runs: [Trigger.schedules,
/// Trigger.subscribers]`. `jadpo dev`, `jadpo test` and the one deployable
/// of an app that declares none run both. Whatever it runs, a program
/// answers its health checks and its tick does the runtime's own
/// housekeeping, deleting what has expired, which any instance may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Runs {
    /// `Trigger.schedules`: the schedules, each entity's purge and each
    /// poll on the tick, and a batch run the tick picks up again.
    pub schedules: bool,
    /// `Trigger.subscribers`: each delivery of an event or an email, after
    /// its action commits and from the outbox on the tick, and the queues
    /// `receiving:` reads.
    pub subscribers: bool,
}

impl Runs {
    /// Both: what a program runs unless a declared deployable says less.
    pub const EVERYTHING: Runs = Runs {
        schedules: true,
        subscribers: true,
    };
}

impl Default for Runs {
    fn default() -> Runs {
        Runs::EVERYTHING
    }
}

/// Why bytes didn't read as a runtime plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unreadable {
    /// They don't start with [`RuntimePlan::HEADER`]: not a plan, or one
    /// written by another version of Jadpo.
    NotThisFormat,
    /// They start as one but don't hold one.
    Malformed(canonical::Error),
    /// They hold a dev and test section, which a build without the
    /// `testing` feature has no part for (chunk 8.1e).
    Testing,
}

impl std::fmt::Display for Unreadable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Unreadable::NotThisFormat => f.write_str("not a runtime plan this Jadpo writes"),
            Unreadable::Malformed(error) => write!(f, "a malformed runtime plan: {error}"),
            Unreadable::Testing => {
                f.write_str("a runtime plan with a test section, which this build doesn't read")
            }
        }
    }
}

impl std::error::Error for Unreadable {}

impl RuntimePlan {
    /// What the bytes of a plan start with: what they are, and the
    /// version of the Jadpo that wrote them. A plan is only ever read by
    /// the program it was compiled into, built from the same Jadpo's
    /// runtime crates as the compiler that wrote it, so the version is all
    /// the header has to name and nothing bumps it by hand
    /// (docs/compiler-runtime.md §2.5, "The plan compiled code holds").
    pub const HEADER: &'static [u8] =
        concat!("jadpo runtime plan ", env!("CARGO_PKG_VERSION"), "\n").as_bytes();

    /// Its canonical bytes: [`RuntimePlan::HEADER`], then the plan in the
    /// canonical byte encoding, then, with the `testing` feature, its dev
    /// and test section in the same encoding unless that is empty. The
    /// same plan always gives the same bytes; the one refusal is a value
    /// the encoding has no form for, which no plan holds.
    pub fn encode(&self) -> Result<Vec<u8>, canonical::Error> {
        #[cfg(feature = "testing")]
        if !self.testing.is_empty() {
            let mut bytes = self.encode_deployable()?;
            bytes.extend_from_slice(&canonical::to_bytes(&self.testing)?);
            return Ok(bytes);
        }
        self.encode_deployable()
    }

    /// Its canonical bytes without its dev and test section: what a
    /// deployable holds, which a build without the `testing` feature
    /// reads (chunk 8.1e).
    pub fn encode_deployable(&self) -> Result<Vec<u8>, canonical::Error> {
        let body = canonical::to_bytes(self)?;
        let mut bytes = Vec::with_capacity(Self::HEADER.len() + body.len());
        bytes.extend_from_slice(Self::HEADER);
        bytes.extend_from_slice(&body);
        Ok(bytes)
    }

    /// The plan `bytes` hold, as [`RuntimePlan::encode`] wrote it. Without
    /// the `testing` feature, bytes holding a dev and test section are
    /// refused ([`Unreadable::Testing`]).
    pub fn decode(bytes: &[u8]) -> Result<RuntimePlan, Unreadable> {
        let body = bytes
            .strip_prefix(Self::HEADER)
            .ok_or(Unreadable::NotThisFormat)?;
        let (plan, rest): (RuntimePlan, &[u8]) =
            canonical::from_prefix(body).map_err(Unreadable::Malformed)?;
        if rest.is_empty() {
            return Ok(plan);
        }
        #[cfg(feature = "testing")]
        {
            let mut plan = plan;
            plan.testing = canonical::from_bytes(rest).map_err(Unreadable::Malformed)?;
            Ok(plan)
        }
        #[cfg(not(feature = "testing"))]
        Err(Unreadable::Testing)
    }

    /// The settings the runtime reads by name rather than by number
    /// (roadmap chunk 8.1o3): each way of signing in's signing key and the
    /// browser session's origin (area 8), and how many proxies are in
    /// front when a setting says (area 10, "Client addresses").
    pub fn setting_names(&self) -> Vec<&str> {
        let mut out = Vec::new();
        if let Some(identity) = &self.routes.identity {
            out.extend(
                identity
                    .signing_keys
                    .iter()
                    .map(|(_, setting)| setting.as_str()),
            );
            out.extend(identity.origin.as_deref());
            if let Some(external) = &identity.external_token {
                out.push(external.issuer.as_str());
                out.push(external.audience.as_str());
            }
        }
        if let Some(crate::http::Hops::Setting(name)) = self
            .routes
            .client_address
            .as_ref()
            .and_then(|plan| plan.hops.as_ref())
        {
            out.push(name.as_str());
        }
        out
    }

    /// The number of the callable with identity key `key`: a test's
    /// through the dev and test section, with the `testing` feature.
    pub fn callable(&self, key: &str) -> Option<u32> {
        if let Some(number) = self.callables.get(key) {
            return Some(*number);
        }
        #[cfg(feature = "testing")]
        if let Some(number) = self.testing.tests.get(key) {
            return Some(*number);
        }
        None
    }
}
