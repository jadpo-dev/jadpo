//! What the `telemetry:` section sends and how (area 16, "Telemetry";
//! chunk O1): plain data the plan carries into every deployable, so both
//! executors record and send the same traces, metrics and logs.
//!
//! - [`TelemetryPlan`]: the section as checked, its settings by number,
//!   or `None` where the runtime reads the standard OpenTelemetry
//!   variables instead ([`ENDPOINT_VARIABLE`], [`HEADERS_VARIABLE`]).
//! - [`MetricPlan`]: each `metric` a domain's `monitoring.jadpo`
//!   declares, by the name it is sent under, `billing.checkouts_completed`.
//! - [`Recording`]: what one `telemetry.` call records, which the graph
//!   carries at its site, so both executors hand the runtime the same
//!   thing.

use serde::{Deserialize, Serialize};

/// The variable the endpoint is read from when `endpoint:` is left out.
pub const ENDPOINT_VARIABLE: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";

/// The variable the credential's headers are read from when
/// `credential:` is left out: `name=value` pairs, comma-separated, as the
/// OpenTelemetry specification writes them.
pub const HEADERS_VARIABLE: &str = "OTEL_EXPORTER_OTLP_HEADERS";

/// The namespace code records through.
pub const NAMESPACE: &str = "telemetry";

/// The call that adds an attribute to the current trace.
pub const NOTE: &str = "note";

/// What the `telemetry:` section says, checked.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TelemetryPlan {
    /// The setting `endpoint:` reads, by number; `None` when it is left
    /// out, and [`ENDPOINT_VARIABLE`] is read.
    pub endpoint: Option<u32>,
    /// `credential:`; `None` when it is left out, and
    /// [`HEADERS_VARIABLE`] is read.
    pub credential: Option<Credential>,
    pub send: Send,
    pub detail: Detail,
}

impl Default for TelemetryPlan {
    /// What an app with no `endpoint:`, `credential:`, `send:` or
    /// `detail:` sends: traces and metrics, logs from `LogLevel.warn`, and
    /// the whole trace of every request that failed, faulted, crossed a
    /// threshold or took a path new to the process, with one ordinary
    /// trace a path each minute.
    fn default() -> TelemetryPlan {
        TelemetryPlan {
            endpoint: None,
            credential: None,
            send: Send::default(),
            detail: Detail::default(),
        }
    }
}

/// `credential: { header: value: }`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Credential {
    /// The header's name, as written.
    pub header: String,
    /// The setting its value is read from, by number.
    pub value: u32,
}

/// `send:`, which signals go out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Send {
    pub traces: bool,
    pub metrics: bool,
    /// The lowest level of log sent.
    pub logs: Level,
}

impl Default for Send {
    fn default() -> Send {
        Send {
            traces: true,
            metrics: true,
            logs: Level::Warn,
        }
    }
}

/// A `LogLevel`, OpenTelemetry's six severities, in their order (area
/// 19): the runtime orders them, as no enum is ordered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl Level {
    pub const ALL: [Level; 6] = [
        Level::Trace,
        Level::Debug,
        Level::Info,
        Level::Warn,
        Level::Error,
        Level::Fatal,
    ];

    /// Its variant's name in `LogLevel`.
    pub fn word(self) -> &'static str {
        match self {
            Level::Trace => "trace",
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Error => "error",
            Level::Fatal => "fatal",
        }
    }

    pub fn from_word(word: &str) -> Option<Level> {
        Level::ALL.into_iter().find(|level| level.word() == word)
    }

    /// OpenTelemetry's `SeverityNumber`: the first of its four in each
    /// range.
    pub fn severity_number(self) -> u8 {
        match self {
            Level::Trace => 1,
            Level::Debug => 5,
            Level::Info => 9,
            Level::Warn => 13,
            Level::Error => 17,
            Level::Fatal => 21,
        }
    }

    /// OpenTelemetry's `SeverityText`: `INFO`.
    pub fn severity_text(self) -> &'static str {
        match self {
            Level::Trace => "TRACE",
            Level::Debug => "DEBUG",
            Level::Info => "INFO",
            Level::Warn => "WARN",
            Level::Error => "ERROR",
            Level::Fatal => "FATAL",
        }
    }
}

/// `detail:`, how much of each request is sent.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Detail {
    /// `full_traces:`, when a request's whole trace is sent.
    pub full_traces: Vec<Full>,
    /// `samples: { per_path: }`: how many ordinary traces each path sends
    /// within `every_ms`.
    pub per_path: u32,
    /// `samples: { every: }`, in milliseconds.
    pub every_ms: u64,
}

impl Default for Detail {
    fn default() -> Detail {
        Detail {
            full_traces: Full::ALL.to_vec(),
            per_path: 1,
            every_ms: 60_000,
        }
    }
}

impl Detail {
    pub fn full(&self, when: Full) -> bool {
        self.full_traces.contains(&when)
    }
}

/// A `Trace` variant: when a whole trace is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Full {
    /// The request failed.
    Failures,
    /// It faulted.
    Faults,
    /// It crossed a threshold's per-request limit.
    Slow,
    /// Its path is new to this process.
    NewPaths,
}

impl Full {
    pub const ALL: [Full; 4] = [Full::Failures, Full::Faults, Full::Slow, Full::NewPaths];

    /// Its variant's name in `Trace`.
    pub fn word(self) -> &'static str {
        match self {
            Full::Failures => "failures",
            Full::Faults => "faults",
            Full::Slow => "slow",
            Full::NewPaths => "new_paths",
        }
    }

    pub fn from_word(word: &str) -> Option<Full> {
        Full::ALL.into_iter().find(|full| full.word() == word)
    }
}

/// The enum `full_traces:` lists the variants of.
pub const TRACE_ENUM: &str = "Trace";

/// A metric's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MetricKind {
    /// A count that only rises, recorded with `add`.
    Counter,
    /// A distribution, such as a duration or an amount, recorded with
    /// `record`.
    Measure,
    /// A value at a moment, recorded with `set`.
    Gauge,
}

impl MetricKind {
    pub const ALL: [MetricKind; 3] = [MetricKind::Counter, MetricKind::Measure, MetricKind::Gauge];

    /// The base it is declared with: `Counter`.
    pub fn word(self) -> &'static str {
        match self {
            MetricKind::Counter => "Counter",
            MetricKind::Measure => "Measure",
            MetricKind::Gauge => "Gauge",
        }
    }

    pub fn from_word(word: &str) -> Option<MetricKind> {
        MetricKind::ALL.into_iter().find(|kind| kind.word() == word)
    }

    /// The operation it is recorded with: `add`.
    pub fn operation(self) -> &'static str {
        match self {
            MetricKind::Counter => "add",
            MetricKind::Measure => "record",
            MetricKind::Gauge => "set",
        }
    }
}

/// One `metric` declaration.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MetricPlan {
    /// The name it is sent under: its domain's and its own,
    /// `billing.checkouts_completed`.
    pub name: String,
    pub kind: MetricKind,
    /// Its `by:`, in the order written.
    pub labels: Vec<String>,
    /// Its `///` line, sent as the metric's description.
    pub description: Option<String>,
    /// `ms` for a `Measure` of durations, which is recorded in
    /// milliseconds; `None` otherwise.
    pub unit: Option<String>,
}

/// What one `telemetry.` call records.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Recording {
    /// `telemetry.<metric>.add(value, label: ...)`, or `record` or `set`:
    /// the metric by the name it is sent under, its kind, and its labels
    /// in the order of the arguments after the value.
    Metric {
        metric: String,
        kind: MetricKind,
        labels: Vec<String>,
    },
    /// `telemetry.note(name: value, ...)`: each attribute's name, in the
    /// order of the arguments.
    Note { names: Vec<String> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_in_opentelemetry_s_order() {
        assert!(Level::Trace < Level::Debug);
        assert!(Level::Warn < Level::Error);
        assert_eq!(Level::from_word("warn"), Some(Level::Warn));
        assert_eq!(Level::Info.severity_number(), 9);
        assert_eq!(Level::Fatal.severity_text(), "FATAL");
    }

    #[test]
    fn the_defaults_are_the_pages() {
        let plan = TelemetryPlan::default();
        assert!(plan.send.traces && plan.send.metrics);
        assert_eq!(plan.send.logs, Level::Warn);
        assert_eq!(plan.detail.full_traces, Full::ALL);
        assert_eq!((plan.detail.per_path, plan.detail.every_ms), (1, 60_000));
    }
}
