//! A registry on this machine that tests run against: the protocol's
//! table served over HTTP from memory, with its own log, and the OpenAPI
//! document the table gives at `openapi.json`. Tests fill it through its
//! methods (a release is logged as it is added) and change what it
//! answers to try a client against a registry that misbehaves.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;

use crate::archive;
use crate::http::{Fetched, Incoming, Server};
use crate::log::{Log, LogEntry};
use crate::names::{Digest, Handle, ScopedName, Version};
use crate::protocol::{
    ARCHIVE_NOT_FOUND, Advisory, AdvisoryAnswer, BarRow, ConsistencyProof, Facts, Failure,
    FingerprintAnswer, InclusionProof, Method, NOT_IN_LOG, PACKAGE_NOT_FOUND, Package, PackageKind,
    PublishedFingerprint, Publisher, RELEASE_NOT_FOUND, RELEASE_REQUEST_NOT_FOUND,
    REPOSITORY_REFUSED, Refusal, Release, ReleaseRequest, ReleaseState, ReleaseSummary, Request,
    SIZE_PAST_HEAD, SearchAnswer, SearchResult, StartedRelease,
};

/// A release to add: its package's details, used when the package is new,
/// and the release's own.
#[derive(Clone, Debug)]
pub struct ReleaseSeed {
    pub package: ScopedName,
    pub version: Version,
    pub files: Vec<(String, Vec<u8>)>,
    pub description: String,
    pub kind: PackageKind,
    pub code_name: String,
    pub label: String,
    pub repository: String,
    pub tag: String,
    pub commit: String,
    pub surface: String,
    pub facts: Facts,
    pub change_record: Vec<crate::protocol::ChangeEntry>,
    pub account: String,
    pub jadpo_releases: Vec<Version>,
    pub released_at: String,
}

impl ReleaseSeed {
    /// A release of `package` at `version` with `files`, and plain details
    /// for the rest.
    pub fn new(package: ScopedName, version: Version, files: Vec<(String, Vec<u8>)>) -> Self {
        let code_name: String = package
            .name()
            .split('-')
            .map(|part| {
                let mut chars = part.chars();
                chars
                    .next()
                    .map(|c| c.to_ascii_uppercase().to_string() + chars.as_str())
                    .unwrap_or_default()
            })
            .collect();
        let major = version
            .as_str()
            .split('.')
            .next()
            .unwrap_or("0")
            .to_string();
        Self {
            repository: format!(
                "https://github.com/{}/{}",
                package.publisher(),
                package.name()
            ),
            tag: format!("v{version}"),
            commit: "0".repeat(40),
            description: format!("{code_name}, for tests."),
            kind: PackageKind::Provider,
            label: format!("v{major}"),
            code_name,
            surface: String::new(),
            facts: Facts {
                kind: PackageKind::Provider,
                publisher: Some(package.handle()),
                pure: false,
                hosts: Vec::new(),
                changing_calls: Vec::new(),
                personal_data: Vec::new(),
                erasure: false,
                storable_types: Vec::new(),
                failures: Vec::new(),
                confirmed_operations: Vec::new(),
                last_contract_run: None,
                standard_candidates: Vec::new(),
            },
            change_record: Vec::new(),
            account: String::new(),
            jadpo_releases: Vec::new(),
            released_at: "2026-10-31T09:00:00Z".into(),
            package,
            version,
            files,
        }
    }
}

#[derive(Debug, Default)]
struct State {
    publishers: BTreeMap<Handle, (bool, bool)>,
    packages: BTreeMap<ScopedName, Package>,
    releases: BTreeMap<(ScopedName, String), Release>,
    archives: BTreeMap<Digest, Vec<u8>>,
    log: Log,
    advisories: Vec<Advisory>,
    /// What search reads: the standard library's declarations, then each
    /// package's, with their written accounts.
    standard: Vec<(String, String)>,
    accounts: Vec<(ScopedName, String, String)>,
    /// Each release's public functions and actions, by source fingerprint,
    /// as the verifier wrote them.
    fingerprints: Vec<PublishedFingerprint>,
    /// Each release started, with its repository, tag, whether it was a
    /// dry run and the publisher the client said it was for.
    requests: Vec<(ReleaseRequest, String, String, bool, Option<Handle>)>,
}

/// A registry in memory, shared by every clone and server of it.
#[derive(Clone, Debug, Default)]
pub struct LocalRegistry {
    state: Arc<Mutex<State>>,
}

/// A served local registry, stopped when dropped.
#[derive(Debug)]
pub struct Served {
    server: Server,
}

impl Served {
    /// The protocol's address: `http://127.0.0.1:<port>/registry/v1/`.
    pub fn url(&self) -> String {
        format!("http://{}{}", self.server.address(), crate::protocol::BASE)
    }
}

impl LocalRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Serves the registry over HTTP on `127.0.0.1`.
    pub fn serve(&self) -> std::io::Result<Served> {
        let registry = self.clone();
        let server = Server::start(Arc::new(move |incoming: Incoming| {
            registry.handle(&incoming)
        }))?;
        Ok(Served { server })
    }

    /// Sets whether `handle`'s publisher is verified and official.
    pub fn publisher(&self, handle: &str, verified: bool, official: bool) {
        let handle = Handle::parse(handle).expect("a publisher's handle");
        let mut state = self.state();
        state
            .publishers
            .insert(handle.clone(), (verified, official));
        for package in state.packages.values_mut() {
            if package.publisher.handle == handle {
                package.publisher.verified = verified;
                package.publisher.official = official;
            }
        }
    }

    /// Adds a release, live at once: its archive is kept and its entry is
    /// added to the log.
    pub fn release(&self, seed: ReleaseSeed) -> Release {
        let archive = archive::pack(&seed.files).expect("the seed's files pack");
        let view: Vec<(&str, &[u8])> = seed
            .files
            .iter()
            .map(|(path, bytes)| (path.as_str(), bytes.as_slice()))
            .collect();
        let digest = Digest::of_files(&view);
        let mut state = self.state();
        let handle = seed.package.handle();
        let (verified, official) = state.publishers.get(&handle).copied().unwrap_or_default();
        let index = state.log.append(&LogEntry {
            package: seed.package.clone(),
            version: seed.version.clone(),
            digest: digest.clone(),
        });
        let package = state
            .packages
            .entry(seed.package.clone())
            .or_insert_with(|| Package {
                package: seed.package.clone(),
                description: seed.description.clone(),
                kind: seed.kind,
                code_name: seed.code_name.clone(),
                label: seed.label.clone(),
                publisher: Publisher {
                    handle,
                    verified,
                    official,
                },
                maintained: true,
                successor: None,
                releases: Vec::new(),
            });
        package.releases.push(ReleaseSummary {
            version: seed.version.clone(),
            digest: digest.clone(),
            released_at: seed.released_at.clone(),
            yanked: false,
            urgent: false,
        });
        package.releases.sort_by(|a, b| a.version.cmp(&b.version));
        let release = Release {
            package: seed.package.clone(),
            version: seed.version.clone(),
            repository: seed.repository,
            tag: seed.tag,
            commit: seed.commit,
            digest: digest.clone(),
            surface: seed.surface,
            facts: seed.facts,
            change_record: seed.change_record,
            account: seed.account,
            jadpo_releases: seed.jadpo_releases,
            log_index: index,
        };
        state
            .releases
            .insert((seed.package, seed.version.to_string()), release.clone());
        state.archives.insert(digest, archive);
        release
    }

    fn summary(
        &self,
        package: &ScopedName,
        version: &Version,
        change: impl FnOnce(&mut ReleaseSummary),
    ) {
        let mut state = self.state();
        let summary = state
            .packages
            .get_mut(package)
            .and_then(|p| p.releases.iter_mut().find(|r| &r.version == version))
            .expect("a release the registry holds");
        change(summary);
    }

    /// Yanks a release: it stays in the log and its archive stays served.
    pub fn yank(&self, package: &ScopedName, version: &Version) {
        self.summary(package, version, |r| r.yanked = true);
    }

    /// Marks a release urgent.
    pub fn urgent(&self, package: &ScopedName, version: &Version) {
        self.summary(package, version, |r| r.urgent = true);
    }

    /// Marks a package unmaintained, or handed to `successor`.
    pub fn unmaintained(&self, package: &ScopedName, successor: Option<ScopedName>) {
        let mut state = self.state();
        let package = state
            .packages
            .get_mut(package)
            .expect("a package the registry holds");
        package.maintained = false;
        package.successor = successor;
    }

    pub fn advisory(&self, advisory: Advisory) {
        self.state().advisories.push(advisory);
    }

    /// A standard-library declaration search can find, with its account.
    pub fn standard(&self, declaration: &str, account: &str) {
        self.state()
            .standard
            .push((declaration.to_string(), account.to_string()));
    }

    /// A package's public declaration search can find, with its account.
    pub fn account(&self, package: &ScopedName, declaration: &str, account: &str) {
        self.state().accounts.push((
            package.clone(),
            declaration.to_string(),
            account.to_string(),
        ));
    }

    /// A release's public function or action, by its source fingerprint,
    /// for `POST fingerprints/query`.
    pub fn fingerprint(&self, fingerprint: PublishedFingerprint) {
        self.state().fingerprints.push(fingerprint);
    }

    /// Every release started: its repository, tag and whether it was a dry
    /// run.
    pub fn started(&self) -> Vec<(String, String, bool)> {
        self.state()
            .requests
            .iter()
            .map(|(_, repository, tag, dry_run, _)| (repository.clone(), tag.clone(), *dry_run))
            .collect()
    }

    /// The publisher each release started was said to be for.
    pub fn started_for(&self) -> Vec<Option<Handle>> {
        self.state()
            .requests
            .iter()
            .map(|(.., publisher)| publisher.clone())
            .collect()
    }

    /// Moves a started release to `state`, with the bar's rows (each a
    /// check, whether it passed and why).
    pub fn settle(&self, id: &str, state: ReleaseState, bar: Vec<(String, bool, String)>) {
        let mut guard = self.state();
        let request = guard
            .requests
            .iter_mut()
            .find(|(r, ..)| r.id == id)
            .map(|(r, ..)| r)
            .expect("a release the registry started");
        request.state = state;
        request.bar = bar
            .into_iter()
            .map(|(check, passed, detail)| BarRow {
                check,
                passed,
                detail,
            })
            .collect();
    }

    /// A started release as the verifier answered it, which `settle` leaves
    /// out: holds it to its publisher (area 11, "Releasing") and moves it to
    /// `pending`, or to `failed` with the bar's `publisher` row when it isn't
    /// that publisher's to release. `code_name` is its name in code.
    pub fn checked(
        &self,
        id: &str,
        code_name: &str,
        facts: Facts,
        bar: Vec<(String, bool, String)>,
    ) {
        let mut guard = self.state();
        let repository = guard
            .requests
            .iter()
            .find(|(request, ..)| request.id == id)
            .map(|(_, repository, ..)| repository.clone())
            .expect("a release the registry started");
        let refused = refusal(&guard, &repository, code_name, facts.publisher.as_ref());
        let failed = refused.is_some() || bar.iter().any(|(_, passed, _)| !passed);
        let mut rows: Vec<BarRow> = bar
            .into_iter()
            .map(|(check, passed, detail)| BarRow {
                check,
                passed,
                detail,
            })
            .collect();
        rows.push(match refused {
            Some(detail) => BarRow {
                check: "publisher".into(),
                passed: false,
                detail,
            },
            None => BarRow {
                check: "publisher".into(),
                passed: true,
                detail: facts
                    .publisher
                    .as_ref()
                    .map_or_else(String::new, ToString::to_string),
            },
        });
        let request = guard
            .requests
            .iter_mut()
            .find(|(request, ..)| request.id == id)
            .map(|(request, ..)| request)
            .expect("a release the registry started");
        request.state = if failed {
            ReleaseState::Failed
        } else {
            ReleaseState::Pending
        };
        request.bar = rows;
        request.facts = Some(facts);
        request.change_record = Some(Vec::new());
    }

    /// Says `package`'s releases come from `repository`.
    pub fn repository_of(&self, package: &ScopedName, repository: &str) {
        let mut state = self.state();
        for ((held, _), release) in &mut state.releases {
            if held == package {
                release.repository = repository.to_string();
            }
        }
    }

    /// The head of the registry's log.
    pub fn head(&self) -> crate::protocol::LogHead {
        self.state().log.head()
    }

    /// Answers one HTTP request.
    pub fn handle(&self, incoming: &Incoming) -> Fetched {
        let openapi = format!("{}openapi.json", crate::protocol::BASE);
        if incoming.target == openapi {
            return if incoming.method == "GET" {
                json(200, &crate::openapi::document())
            } else {
                envelope(404, "not_found", "No route has this method and path.", None)
            };
        }
        let Some(method) = Method::parse(&incoming.method) else {
            return envelope(404, "not_found", "No route has this method and path.", None);
        };
        match Request::parse(method, &incoming.target, &incoming.body) {
            Ok(request) => self.answer(&request),
            Err(Refusal::NotFound) => {
                envelope(404, "not_found", "No route has this method and path.", None)
            }
            Err(Refusal::InvalidRequest(_)) => {
                envelope(400, "invalid_request", "The request can't be read.", None)
            }
            Err(Refusal::InvalidInput { field, rule }) => envelope(
                422,
                "invalid_input",
                "Request value is invalid.",
                Some(serde_json::json!({ "problems": [{ "field": field, "rule": rule }] })),
            ),
        }
    }

    /// Answers a request of the table.
    pub fn answer(&self, request: &Request) -> Fetched {
        let mut state = self.state();
        match request {
            Request::Package { package } => match state.packages.get(package) {
                Some(found) => json(200, found),
                None => failure(PACKAGE_NOT_FOUND),
            },
            Request::Release { package, version } => {
                if !state.packages.contains_key(package) {
                    return failure(PACKAGE_NOT_FOUND);
                }
                match state.releases.get(&(package.clone(), version.to_string())) {
                    Some(release) => json(200, release),
                    None => failure(RELEASE_NOT_FOUND),
                }
            }
            Request::Archive { digest } => match state.archives.get(digest) {
                Some(bytes) => Fetched {
                    status: 200,
                    content_type: Some("application/x-tar".into()),
                    body: bytes.clone(),
                },
                None => failure(ARCHIVE_NOT_FOUND),
            },
            Request::LogHead => json(200, &state.log.head()),
            Request::LogProof { digest, size } => {
                if *size > state.log.size() {
                    return failure(SIZE_PAST_HEAD);
                }
                let Some(index) = state.log.index_of(digest).filter(|i| i < size) else {
                    return failure(NOT_IN_LOG);
                };
                let entry = state.log.entry(index).cloned().expect("a logged entry");
                let hashes = state.log.inclusion_proof(index, *size).expect("in the log");
                json(
                    200,
                    &InclusionProof {
                        entry,
                        index,
                        size: *size,
                        hashes,
                    },
                )
            }
            Request::LogConsistency { from, to } => match state.log.consistency_proof(*from, *to) {
                Some(hashes) => json(
                    200,
                    &ConsistencyProof {
                        from: *from,
                        to: *to,
                        hashes,
                    },
                ),
                None => failure(SIZE_PAST_HEAD),
            },
            Request::Search { q, kind, .. } => {
                let words: Vec<String> = q
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| !w.is_empty())
                    .map(str::to_lowercase)
                    .collect();
                let matched = |text: &str| -> Vec<String> {
                    let lower = text.to_lowercase();
                    words
                        .iter()
                        .filter(|w| lower.contains(w.as_str()))
                        .cloned()
                        .collect()
                };
                let mut results = Vec::new();
                for (declaration, account) in &state.standard {
                    let found = matched(&format!("{declaration} {account}"));
                    if !found.is_empty() {
                        results.push(SearchResult {
                            package: None,
                            declaration: declaration.clone(),
                            why: format!("Its account mentions {}.", found.join(", ")),
                        });
                    }
                }
                for (package, declaration, account) in &state.accounts {
                    let kind_matches = kind
                        .is_none_or(|k| state.packages.get(package).is_some_and(|p| p.kind == k));
                    let found = matched(&format!("{declaration} {account}"));
                    if kind_matches && !found.is_empty() {
                        results.push(SearchResult {
                            package: Some(package.clone()),
                            declaration: declaration.clone(),
                            why: format!("Its account mentions {}.", found.join(", ")),
                        });
                    }
                }
                json(200, &SearchAnswer { results })
            }
            Request::QueryAdvisories(query) => {
                let advisories = state
                    .advisories
                    .iter()
                    .filter(|a| {
                        query.packages.iter().any(|installed| {
                            installed.package == a.package
                                && a.from <= installed.version
                                && installed.version <= a.to
                        })
                    })
                    .cloned()
                    .collect();
                json(200, &AdvisoryAnswer { advisories })
            }
            Request::QueryFingerprints(query) => {
                // The whole bucket of each prefix, never only what matches:
                // the client compares the fingerprints itself. Of a
                // declaration's releases with one fingerprint, the newest
                // that isn't yanked.
                let yanked = |f: &PublishedFingerprint| {
                    state.packages.get(&f.package).is_none_or(|package| {
                        package
                            .releases
                            .iter()
                            .any(|r| r.version == f.version && r.yanked)
                    })
                };
                let mut fingerprints: Vec<PublishedFingerprint> = Vec::new();
                for f in &state.fingerprints {
                    let hex = f.source.as_str().trim_start_matches("sha256:");
                    if yanked(f) || !query.prefixes.iter().any(|p| hex.starts_with(p.as_str())) {
                        continue;
                    }
                    match fingerprints.iter_mut().find(|kept| {
                        kept.package == f.package
                            && kept.declaration == f.declaration
                            && kept.source == f.source
                    }) {
                        Some(kept) if kept.version < f.version => kept.version = f.version.clone(),
                        Some(_) => {}
                        None => fingerprints.push(f.clone()),
                    }
                }
                json(200, &FingerprintAnswer { fingerprints })
            }
            Request::StartRelease(start) => {
                if !start.repository.starts_with("https://") {
                    return failure(REPOSITORY_REFUSED);
                }
                // A release from a repository another publisher releases
                // from is refused at once, before any check, when the
                // client says whose it is.
                if let Some(publisher) = &start.publisher
                    && let Some(earlier) = state.releases.values().find(|release| {
                        release.repository == start.repository
                            && release.package.handle() != *publisher
                    })
                {
                    return envelope(
                        REPOSITORY_REFUSED.status,
                        REPOSITORY_REFUSED.code,
                        &format!(
                            "`{}` releases `{}`, which is `{}`'s, not `{publisher}`'s.",
                            start.repository,
                            earlier.package,
                            earlier.package.handle()
                        ),
                        None,
                    );
                }
                let id = format!("rel_{:04}", state.requests.len() + 1);
                state.requests.push((
                    ReleaseRequest {
                        id: id.clone(),
                        state: ReleaseState::Verifying,
                        bar: Vec::new(),
                        facts: None,
                        change_record: None,
                    },
                    start.repository.clone(),
                    start.tag.clone(),
                    start.dry_run,
                    start.publisher.clone(),
                ));
                json(202, &StartedRelease { id })
            }
            Request::ReleaseRequest { id } => {
                match state.requests.iter().find(|(r, ..)| &r.id == id) {
                    Some((request, ..)) => json(200, request),
                    None => failure(RELEASE_REQUEST_NOT_FOUND),
                }
            }
        }
    }
}

/// Why a release from `repository`, naming `publisher`, isn't that
/// publisher's to release: it names none, its repository's earlier releases
/// are someone else's, or the name it would take is released from elsewhere.
fn refusal(
    state: &State,
    repository: &str,
    code_name: &str,
    publisher: Option<&Handle>,
) -> Option<String> {
    let Some(publisher) = publisher else {
        return Some("its `package.jadpo` names no publisher".to_string());
    };
    if let Some(earlier) = state
        .releases
        .values()
        .find(|release| release.repository == repository)
        .filter(|release| release.package.handle() != *publisher)
    {
        return Some(format!(
            "it names `{publisher}`, and `{repository}` releases `{}`, which is `{}`'s",
            earlier.package,
            earlier.package.handle()
        ));
    }
    let name = ScopedName::of_code(publisher, code_name)?;
    state
        .releases
        .values()
        .find(|release| release.package == name && release.repository != repository)
        .map(|release| {
            format!(
                "`{name}` is released from `{}`, and this is `{repository}`",
                release.repository
            )
        })
}

fn json(status: u16, value: &impl Serialize) -> Fetched {
    Fetched {
        status,
        content_type: Some("application/json".into()),
        body: serde_json::to_vec(value).expect("an answer is JSON"),
    }
}

fn failure(failure: Failure) -> Fetched {
    envelope(failure.status, failure.code, failure.message, None)
}

/// The failure envelope (area 4, "What the client sees").
fn envelope(status: u16, code: &str, message: &str, details: Option<serde_json::Value>) -> Fetched {
    let mut error = serde_json::json!({
        "code": code,
        "message": message,
        "request_id": "req_local",
    });
    if let Some(details) = details {
        error["details"] = details;
    }
    json(status, &serde_json::json!({ "error": error }))
}
