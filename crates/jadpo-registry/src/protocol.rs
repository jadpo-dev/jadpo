//! The registry protocol's table (area 11, "The registry protocol"): each
//! request, what it answers and how it fails, and each answer as a type.
//!
//! It is served at `https://jadpo.dev/registry/v1/` as JSON, failures in
//! the failure envelope (area 4), versioned by its path. Reads need no
//! sign-in, and the `jadpo` tool never signs in. Every answer's field is
//! always sent, `null` where it has no value, as a Jadpo application
//! writes an output.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::log::{Hash, LogEntry};
use crate::names::{Digest, NameError, ScopedName, Version};
use crate::shape::{Shape, check_value};

/// Where the protocol is served, below the registry's origin.
pub const BASE: &str = "/registry/v1/";

/// The registry the `jadpo` tool talks to.
pub const PUBLIC: &str = "https://jadpo.dev/registry/v1/";

/// The methods the protocol uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "GET" => Method::Get,
            "POST" => Method::Post,
            "PUT" => Method::Put,
            "PATCH" => Method::Patch,
            "DELETE" => Method::Delete,
            _ => return None,
        })
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a request answers with when it succeeds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// JSON of the named type.
    Json(&'static str),
    /// A tar archive (`application/x-tar`).
    Tar,
}

/// A failure the protocol names for a request: its HTTP status and code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Failure {
    pub status: u16,
    pub code: &'static str,
    pub message: &'static str,
}

/// A path or query parameter.
#[derive(Clone, Copy, Debug)]
pub struct Param {
    pub name: &'static str,
    pub required: bool,
    pub shape: fn() -> Shape,
}

/// One row of the protocol's table.
#[derive(Clone, Copy, Debug)]
pub struct Endpoint {
    /// Its name in the table and the OpenAPI document's `operationId`.
    pub id: &'static str,
    pub method: Method,
    /// Its path below [`BASE`], each placeholder a whole segment:
    /// `packages/{publisher}/{name}`, where `publisher` is the handle
    /// with its `@`.
    pub path: &'static str,
    pub summary: &'static str,
    pub path_params: &'static [Param],
    pub query: &'static [Param],
    /// The JSON body's type, for a `POST`.
    pub body: Option<&'static str>,
    /// The success status.
    pub status: u16,
    pub answer: Answer,
    pub failures: &'static [Failure],
}

impl Endpoint {
    /// Its full path, as an OpenAPI document lists it.
    pub fn full_path(&self) -> String {
        format!("{BASE}{}", self.path)
    }
}

pub const PACKAGE_NOT_FOUND: Failure = Failure {
    status: 404,
    code: "package_not_found",
    message: "No package has this name.",
};
pub const RELEASE_NOT_FOUND: Failure = Failure {
    status: 404,
    code: "release_not_found",
    message: "The package has no release with this version.",
};
pub const ARCHIVE_NOT_FOUND: Failure = Failure {
    status: 404,
    code: "archive_not_found",
    message: "No release has this digest.",
};
pub const NOT_IN_LOG: Failure = Failure {
    status: 404,
    code: "not_in_log",
    message: "No release with this digest is in the log at this size.",
};
pub const SIZE_PAST_HEAD: Failure = Failure {
    status: 422,
    code: "size_past_head",
    message: "The log isn't that size yet, or the sizes are the wrong way round.",
};
pub const RELEASE_REQUEST_NOT_FOUND: Failure = Failure {
    status: 404,
    code: "release_request_not_found",
    message: "No release was started with this id.",
};
pub const REPOSITORY_REFUSED: Failure = Failure {
    status: 422,
    code: "repository_refused",
    message: "A release is made from a public git repository reached over HTTPS.",
};

fn publisher_param() -> Shape {
    Shape::Text(crate::shape::Text::Handle)
}
fn name_param() -> Shape {
    Shape::Text(crate::shape::Text::NamePart)
}
fn version_param() -> Shape {
    Shape::Text(crate::shape::Text::Version)
}
fn digest_param() -> Shape {
    Shape::Text(crate::shape::Text::Digest)
}
fn plain_text() -> Shape {
    Shape::Text(crate::shape::Text::Plain)
}
fn size_param() -> Shape {
    Shape::Integer
}
fn kind_param() -> Shape {
    Shape::Named("PackageKind")
}

const PACKAGE_PARAMS: &[Param] = &[
    Param {
        name: "publisher",
        required: true,
        shape: publisher_param,
    },
    Param {
        name: "name",
        required: true,
        shape: name_param,
    },
];

/// The protocol's table: one row per request, in the order area 11 lists
/// them, with `log/consistency` after `log/proof`.
pub const ENDPOINTS: &[Endpoint] = &[
    Endpoint {
        id: "package",
        method: Method::Get,
        path: "packages/{publisher}/{name}",
        summary: "The package and each of its releases.",
        path_params: PACKAGE_PARAMS,
        query: &[],
        body: None,
        status: 200,
        answer: Answer::Json("Package"),
        failures: &[PACKAGE_NOT_FOUND],
    },
    Endpoint {
        id: "release",
        method: Method::Get,
        path: "packages/{publisher}/{name}/releases/{version}",
        summary: "One release: where it came from, its surface, facts, change record, written account and index in the log.",
        path_params: &[
            Param {
                name: "publisher",
                required: true,
                shape: publisher_param,
            },
            Param {
                name: "name",
                required: true,
                shape: name_param,
            },
            Param {
                name: "version",
                required: true,
                shape: version_param,
            },
        ],
        query: &[],
        body: None,
        status: 200,
        answer: Answer::Json("Release"),
        failures: &[PACKAGE_NOT_FOUND, RELEASE_NOT_FOUND],
    },
    Endpoint {
        id: "archive",
        method: Method::Get,
        path: "archives/{digest}",
        summary: "The release's files as a tar archive whose files, unpacked, have this digest, as the lock computes it.",
        path_params: &[Param {
            name: "digest",
            required: true,
            shape: digest_param,
        }],
        query: &[],
        body: None,
        status: 200,
        answer: Answer::Tar,
        failures: &[ARCHIVE_NOT_FOUND],
    },
    Endpoint {
        id: "log_head",
        method: Method::Get,
        path: "log/head",
        summary: "The log's size and root hash.",
        path_params: &[],
        query: &[],
        body: None,
        status: 200,
        answer: Answer::Json("LogHead"),
        failures: &[],
    },
    Endpoint {
        id: "log_proof",
        method: Method::Get,
        path: "log/proof/{digest}",
        summary: "The release's index and the hashes that prove it is in the log at the size given.",
        path_params: &[Param {
            name: "digest",
            required: true,
            shape: digest_param,
        }],
        query: &[Param {
            name: "size",
            required: true,
            shape: size_param,
        }],
        body: None,
        status: 200,
        answer: Answer::Json("InclusionProof"),
        failures: &[NOT_IN_LOG, SIZE_PAST_HEAD],
    },
    Endpoint {
        id: "log_consistency",
        method: Method::Get,
        path: "log/consistency",
        summary: "The hashes that prove the log at size `to` extends the log at size `from`.",
        path_params: &[],
        query: &[
            Param {
                name: "from",
                required: true,
                shape: size_param,
            },
            Param {
                name: "to",
                required: true,
                shape: size_param,
            },
        ],
        body: None,
        status: 200,
        answer: Answer::Json("ConsistencyProof"),
        failures: &[SIZE_PAST_HEAD],
    },
    Endpoint {
        id: "search",
        method: Method::Get,
        path: "search",
        summary: "Ranked results, the standard library's first, each naming a package and declaration, with why it matched.",
        path_params: &[],
        query: &[
            Param {
                name: "q",
                required: true,
                shape: plain_text,
            },
            Param {
                name: "from",
                required: false,
                shape: plain_text,
            },
            Param {
                name: "to",
                required: false,
                shape: plain_text,
            },
            Param {
                name: "kind",
                required: false,
                shape: kind_param,
            },
        ],
        body: None,
        status: 200,
        answer: Answer::Json("SearchAnswer"),
        failures: &[],
    },
    Endpoint {
        id: "query_advisories",
        method: Method::Post,
        path: "advisories/query",
        summary: "The advisories against the installed packages and versions given.",
        path_params: &[],
        query: &[],
        body: Some("AdvisoryQuery"),
        status: 200,
        answer: Answer::Json("AdvisoryAnswer"),
        failures: &[],
    },
    Endpoint {
        id: "start_release",
        method: Method::Post,
        path: "releases",
        summary: "Starts a release, or a dry run, of a repository at a tag, and answers its id.",
        path_params: &[],
        query: &[],
        body: Some("StartRelease"),
        status: 202,
        answer: Answer::Json("StartedRelease"),
        failures: &[REPOSITORY_REFUSED],
    },
    Endpoint {
        id: "release_request",
        method: Method::Get,
        path: "releases/{id}",
        summary: "A release's state, with its facts and change record once checked.",
        path_params: &[Param {
            name: "id",
            required: true,
            shape: plain_text,
        }],
        query: &[],
        body: None,
        status: 200,
        answer: Answer::Json("ReleaseRequest"),
        failures: &[RELEASE_REQUEST_NOT_FOUND],
    },
];

/// The row named `id`.
pub fn endpoint(id: &str) -> &'static Endpoint {
    ENDPOINTS
        .iter()
        .find(|e| e.id == id)
        .unwrap_or_else(|| panic!("no endpoint `{id}`"))
}

/// Checks a JSON answer against the shape the table gives `endpoint`.
pub fn check_answer(endpoint: &Endpoint, json: &serde_json::Value) -> Result<(), String> {
    match endpoint.answer {
        Answer::Json(name) => check_value(&Shape::Named(name), json),
        Answer::Tar => Err("this request answers a tar archive, not JSON".into()),
    }
}

// The answers and bodies.

/// Whether a package reaches an outside API or is library code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    Provider,
    Library,
}

impl PackageKind {
    pub const VALUES: &[&str] = &["provider", "library"];

    pub fn as_str(self) -> &'static str {
        match self {
            PackageKind::Provider => "provider",
            PackageKind::Library => "library",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "provider" => Some(PackageKind::Provider),
            "library" => Some(PackageKind::Library),
            _ => None,
        }
    }
}

/// A package's publisher.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Publisher {
    pub handle: crate::names::Handle,
    pub verified: bool,
    pub official: bool,
}

/// One release as a package lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseSummary {
    pub version: Version,
    pub digest: Digest,
    /// When it went live, RFC 3339 in UTC.
    pub released_at: String,
    pub yanked: bool,
    pub urgent: bool,
}

/// `GET packages/{publisher}/{name}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub package: ScopedName,
    pub description: String,
    pub kind: PackageKind,
    /// Its name in code, `Postcodes`, and its version label, `v2`.
    pub code_name: String,
    pub label: String,
    pub publisher: Publisher,
    pub maintained: bool,
    /// The package that carries it on, once it has been handed over.
    pub successor: Option<ScopedName>,
    /// Oldest first.
    pub releases: Vec<ReleaseSummary>,
}

/// A host a package reaches and the operations that reach it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostFact {
    pub host: String,
    pub operations: Vec<String>,
}

/// A personal-data field and a host it reaches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonalDataFact {
    pub field: String,
    pub host: String,
    /// The operation that sends it.
    pub operation: String,
    pub categories: Vec<String>,
    /// The value only decides what is sent, and isn't itself sent.
    pub decided_only: bool,
}

/// An operation or function and the failures it can give.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailureFact {
    pub declaration: String,
    pub failures: Vec<String>,
}

/// A declaration and the standard type it could use.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StandardCandidate {
    pub declaration: String,
    pub standard: String,
}

/// A release's facts (area 11, "What a release shows"), each worked out
/// by the verifier from the compiled graph; the Jadpo releases it works
/// with are the release's own field, since they grow after it is checked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Facts {
    pub kind: PackageKind,
    pub pure: bool,
    pub hosts: Vec<HostFact>,
    /// The operations that change something at the provider.
    pub changing_calls: Vec<String>,
    pub personal_data: Vec<PersonalDataFact>,
    pub erasure: bool,
    /// Types an app may store.
    pub storable_types: Vec<String>,
    pub failures: Vec<FailureFact>,
    pub confirmed_operations: Vec<String>,
    /// When the last passing contract run was, RFC 3339 in UTC.
    pub last_contract_run: Option<String>,
    pub standard_candidates: Vec<StandardCandidate>,
}

/// What happened to a declaration in a release (area 11, "The changelog").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Change {
    Added,
    Removed,
    Retyped,
    NewFailure,
    NewHost,
    PersonalDataSent,
    BehaviourChanged,
}

impl Change {
    pub const VALUES: &[&str] = &[
        "added",
        "removed",
        "retyped",
        "new_failure",
        "new_host",
        "personal_data_sent",
        "behaviour_changed",
    ];
}

/// One entry of a release's change record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeEntry {
    pub declaration: String,
    /// What the declaration is: `function`, `type`, `provider`.
    pub kind: String,
    pub change: Change,
    /// What changed about it: the failure, the host, the field and where
    /// it now goes, the new facts and the old, or the example that failed.
    pub detail: Option<String>,
    /// The author's reason, where they gave one.
    pub why: Option<String>,
}

/// `GET packages/{publisher}/{name}/releases/{version}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub package: ScopedName,
    pub version: Version,
    pub repository: String,
    pub tag: String,
    pub commit: String,
    pub digest: Digest,
    /// Its `surface.jadpo_record`.
    pub surface: String,
    pub facts: Facts,
    pub change_record: Vec<ChangeEntry>,
    /// The written account, by a model, from its source.
    pub account: String,
    /// The Jadpo releases it compiles and passes its tests with.
    pub jadpo_releases: Vec<Version>,
    pub log_index: u64,
}

/// `GET log/head`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogHead {
    pub size: u64,
    pub root: Hash,
}

/// `GET log/proof/{digest}?size=`: the release's entry and index, and the
/// hashes that prove it is in the log of `size` leaves. The root isn't
/// sent: the client proves against the head it holds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InclusionProof {
    pub entry: LogEntry,
    pub index: u64,
    pub size: u64,
    pub hashes: Vec<Hash>,
}

/// `GET log/consistency?from=&to=`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsistencyProof {
    pub from: u64,
    pub to: u64,
    pub hashes: Vec<Hash>,
}

/// One search result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchResult {
    /// The package, or `null` for the standard library.
    pub package: Option<ScopedName>,
    pub declaration: String,
    pub why: String,
}

/// `GET search`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchAnswer {
    pub results: Vec<SearchResult>,
}

/// An installed package and version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Installed {
    pub package: ScopedName,
    pub version: Version,
}

/// `POST advisories/query`'s body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvisoryQuery {
    pub packages: Vec<Installed>,
}

/// An advisory: the declarations it is against, the versions from `from`
/// to `to` inclusive, and the packages carrying a copy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Advisory {
    pub id: String,
    pub package: ScopedName,
    pub declarations: Vec<String>,
    pub from: Version,
    pub to: Version,
    pub summary: String,
    pub copies: Vec<ScopedName>,
}

/// `POST advisories/query`'s answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdvisoryAnswer {
    pub advisories: Vec<Advisory>,
}

/// `POST releases`'s body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartRelease {
    pub repository: String,
    pub tag: String,
    pub dry_run: bool,
}

/// `POST releases`'s answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartedRelease {
    pub id: String,
}

/// Where a release request is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseState {
    Verifying,
    Failed,
    Pending,
    Cancelled,
    Live,
}

impl ReleaseState {
    pub const VALUES: &[&str] = &["verifying", "failed", "pending", "cancelled", "live"];
}

/// One row of the publishing bar.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BarRow {
    pub check: String,
    pub passed: bool,
    pub detail: String,
}

/// `GET releases/{id}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseRequest {
    pub id: String,
    pub state: ReleaseState,
    /// The bar's rows once checked; every row when it failed.
    pub bar: Vec<BarRow>,
    pub facts: Option<Facts>,
    pub change_record: Option<Vec<ChangeEntry>>,
}

// Requests.

/// One request of the table, as the `jadpo` tool sends it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Package {
        package: ScopedName,
    },
    Release {
        package: ScopedName,
        version: Version,
    },
    Archive {
        digest: Digest,
    },
    LogHead,
    LogProof {
        digest: Digest,
        size: u64,
    },
    LogConsistency {
        from: u64,
        to: u64,
    },
    Search {
        q: String,
        from: Option<String>,
        to: Option<String>,
        kind: Option<PackageKind>,
    },
    QueryAdvisories(AdvisoryQuery),
    StartRelease(StartRelease),
    ReleaseRequest {
        id: String,
    },
}

/// Why a request couldn't be read, as the failure envelope says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// No row of the table has this method and path: 404 `not_found`.
    NotFound,
    /// The body isn't JSON of the right shape: 400 `invalid_request`.
    InvalidRequest(String),
    /// A placeholder or query value is missing or breaks its rule: 422
    /// `invalid_input`, naming the field.
    InvalidInput { field: String, rule: &'static str },
}

impl Request {
    pub fn endpoint(&self) -> &'static Endpoint {
        endpoint(match self {
            Request::Package { .. } => "package",
            Request::Release { .. } => "release",
            Request::Archive { .. } => "archive",
            Request::LogHead => "log_head",
            Request::LogProof { .. } => "log_proof",
            Request::LogConsistency { .. } => "log_consistency",
            Request::Search { .. } => "search",
            Request::QueryAdvisories(_) => "query_advisories",
            Request::StartRelease(_) => "start_release",
            Request::ReleaseRequest { .. } => "release_request",
        })
    }

    pub fn method(&self) -> Method {
        self.endpoint().method
    }

    /// The path and query string below the registry's origin.
    pub fn target(&self) -> String {
        let path = match self {
            Request::Package { package } => {
                format!(
                    "packages/{}/{}",
                    package.publisher_segment(),
                    package.name()
                )
            }
            Request::Release { package, version } => format!(
                "packages/{}/{}/releases/{version}",
                package.publisher_segment(),
                package.name()
            ),
            Request::Archive { digest } => format!("archives/{digest}"),
            Request::LogHead => "log/head".into(),
            Request::LogProof { digest, size } => format!("log/proof/{digest}?size={size}"),
            Request::LogConsistency { from, to } => format!("log/consistency?from={from}&to={to}"),
            Request::Search { q, from, to, kind } => {
                let mut query = format!("search?q={}", encode(q));
                if let Some(from) = from {
                    query.push_str(&format!("&from={}", encode(from)));
                }
                if let Some(to) = to {
                    query.push_str(&format!("&to={}", encode(to)));
                }
                if let Some(kind) = kind {
                    query.push_str(&format!("&kind={}", kind.as_str()));
                }
                query
            }
            Request::QueryAdvisories(_) => "advisories/query".into(),
            Request::StartRelease(_) => "releases".into(),
            Request::ReleaseRequest { id } => format!("releases/{}", encode(id)),
        };
        format!("{BASE}{path}")
    }

    /// The JSON body, for a `POST`.
    pub fn body(&self) -> Option<String> {
        match self {
            Request::QueryAdvisories(query) => serde_json::to_string(query).ok(),
            Request::StartRelease(start) => serde_json::to_string(start).ok(),
            _ => None,
        }
    }

    /// Reads a request from its method, its target (path and query
    /// string) and its body, by the table.
    pub fn parse(method: Method, target: &str, body: &[u8]) -> Result<Request, Refusal> {
        let (path, query) = match target.split_once('?') {
            Some((path, query)) => (path, Some(query)),
            None => (target, None),
        };
        let rest = path.strip_prefix(BASE).ok_or(Refusal::NotFound)?;
        let segments: Vec<String> = rest
            .split('/')
            .map(|s| decode(s, false).ok_or(Refusal::NotFound))
            .collect::<Result<_, _>>()?;
        let endpoint = ENDPOINTS
            .iter()
            .find(|e| e.method == method && matches_path(e.path, &segments))
            .ok_or(Refusal::NotFound)?;
        let mut params = std::collections::BTreeMap::new();
        for (pattern, segment) in endpoint.path.split('/').zip(&segments) {
            if let Some(name) = pattern.strip_prefix('{').and_then(|p| p.strip_suffix('}')) {
                params.insert(name, segment.as_str());
            }
        }
        let query = read_query(endpoint, query)?;
        let invalid = |field: &str| Refusal::InvalidInput {
            field: field.to_string(),
            rule: "pattern",
        };
        let package = || -> Result<ScopedName, Refusal> {
            let publisher = params["publisher"];
            let name = params["name"];
            if !publisher.starts_with('@') || publisher.contains('/') {
                return Err(invalid("publisher"));
            }
            ScopedName::parse(&format!("{publisher}/{name}")).map_err(|_| {
                if crate::names::Handle::parse(publisher).is_err() {
                    invalid("publisher")
                } else {
                    invalid("name")
                }
            })
        };
        let digest = || Digest::parse(params["digest"]).map_err(|_: NameError| invalid("digest"));
        let size = |name: &str| -> Result<u64, Refusal> {
            let text = query.get(name).ok_or_else(|| Refusal::InvalidInput {
                field: name.to_string(),
                rule: "required",
            })?;
            let good = !text.is_empty()
                && text.len() <= 15
                && text.bytes().all(|b| b.is_ascii_digit())
                && (text == "0" || !text.starts_with('0'));
            good.then(|| text.parse().ok())
                .flatten()
                .ok_or_else(|| invalid(name))
        };
        Ok(match endpoint.id {
            "package" => Request::Package {
                package: package()?,
            },
            "release" => Request::Release {
                package: package()?,
                version: Version::parse(params["version"]).map_err(|_| invalid("version"))?,
            },
            "archive" => Request::Archive { digest: digest()? },
            "log_head" => Request::LogHead,
            "log_proof" => Request::LogProof {
                digest: digest()?,
                size: size("size")?,
            },
            "log_consistency" => Request::LogConsistency {
                from: size("from")?,
                to: size("to")?,
            },
            "search" => Request::Search {
                q: query
                    .get("q")
                    .filter(|q| !q.is_empty())
                    .ok_or_else(|| Refusal::InvalidInput {
                        field: "q".into(),
                        rule: "required",
                    })?
                    .clone(),
                from: query.get("from").cloned(),
                to: query.get("to").cloned(),
                kind: match query.get("kind") {
                    None => None,
                    Some(kind) => {
                        Some(
                            PackageKind::parse(kind).ok_or_else(|| Refusal::InvalidInput {
                                field: "kind".into(),
                                rule: "one_of",
                            })?,
                        )
                    }
                },
            },
            "query_advisories" => Request::QueryAdvisories(read_body(body)?),
            "start_release" => Request::StartRelease(read_body(body)?),
            "release_request" => Request::ReleaseRequest {
                id: params["id"].to_string(),
            },
            other => unreachable!("endpoint `{other}` isn't read"),
        })
    }
}

fn matches_path(pattern: &str, segments: &[String]) -> bool {
    let parts: Vec<&str> = pattern.split('/').collect();
    parts.len() == segments.len()
        && parts.iter().zip(segments).all(|(part, segment)| {
            if part.starts_with('{') {
                !segment.is_empty()
            } else {
                part == segment
            }
        })
}

/// The query string's members, refusing one the row doesn't read, one
/// given twice, or text that isn't valid UTF-8 once decoded.
fn read_query(
    endpoint: &Endpoint,
    query: Option<&str>,
) -> Result<std::collections::BTreeMap<String, String>, Refusal> {
    let mut out = std::collections::BTreeMap::new();
    let Some(query) = query.filter(|q| !q.is_empty()) else {
        return Ok(out);
    };
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = decode(name, true)
            .ok_or_else(|| Refusal::InvalidRequest("the query string isn't UTF-8".into()))?;
        let value = decode(value, true)
            .ok_or_else(|| Refusal::InvalidRequest("the query string isn't UTF-8".into()))?;
        if !endpoint.query.iter().any(|p| p.name == name) {
            return Err(Refusal::InvalidInput {
                field: name,
                rule: "unknown",
            });
        }
        if out.insert(name.clone(), value).is_some() {
            return Err(Refusal::InvalidInput {
                field: name,
                rule: "repeated",
            });
        }
    }
    Ok(out)
}

fn read_body<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, Refusal> {
    serde_json::from_slice(body).map_err(|e| Refusal::InvalidRequest(e.to_string()))
}

/// Percent-encodes `text` for a query string or path segment: every byte
/// but ASCII letters, digits and `-._~`.
pub fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Decodes percent-encoding, and `+` as a space in a query string;
/// `None` for a broken escape or bytes that aren't UTF-8.
pub fn decode(text: &str, query: bool) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hex = text.get(i + 1..i + 3)?;
                if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return None;
                }
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' if query => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}
