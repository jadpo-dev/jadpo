//! What answers the registry (area 11, "The verifier"): a credential on
//! every request but `/health`, and one verification at a time, so no
//! package's run can take another's time or memory. A verification takes
//! longer than a connection's calls may, so it is started and asked after.
//!
//! A verification is named by what it verifies. Asking again for the same
//! one answers with the same id and runs nothing twice, and one that
//! couldn't be checked is tried again.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

use jadpo_registry::http::{Fetched, Handler, Incoming, Server};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use crate::git::GitError;
use crate::request::{is_commit, is_digest, is_tag};
use crate::verification::{Outcome, Request, Verifier};

/// The shortest credential taken.
pub const SHORTEST_TOKEN: usize = 32;

/// How many verifications may wait.
pub const WAITING: usize = 16;

/// How many finished verifications are kept to be asked after: each holds
/// an archive, in a machine whose memory the next run needs.
const KEPT: usize = 8;

/// What the service does beside the verifier.
#[derive(Clone)]
pub struct Options {
    /// How many may wait before a request is refused.
    pub waiting: usize,
    /// Told of each verification as it ends.
    pub log: Arc<dyn Fn(String) + Send + Sync>,
}

impl std::fmt::Debug for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Options")
            .field("waiting", &self.waiting)
            .finish_non_exhaustive()
    }
}

impl Default for Options {
    fn default() -> Self {
        Options {
            waiting: WAITING,
            log: Arc::new(|_| {}),
        }
    }
}

#[derive(Clone, Debug)]
enum Job {
    Waiting,
    Finished(Outcome),
}

#[derive(Debug, Default)]
struct Jobs {
    jobs: HashMap<String, Job>,
    finished: VecDeque<String>,
    queue: VecDeque<(String, Request)>,
    stopping: bool,
}

type Shared = Arc<(Mutex<Jobs>, Condvar)>;

fn locked(shared: &Shared) -> std::sync::MutexGuard<'_, Jobs> {
    shared
        .0
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A service running, stopped when dropped.
#[derive(Debug)]
pub struct Service {
    server: Option<Server>,
    shared: Shared,
    worker: Option<JoinHandle<()>>,
}

impl Service {
    /// Starts answering at `address`, or says why it can't.
    pub fn start(
        address: &str,
        token: &str,
        verifier: Verifier,
        options: Options,
    ) -> Result<Service, String> {
        if token.len() < SHORTEST_TOKEN {
            return Err(format!(
                "its credential has to be at least {SHORTEST_TOKEN} characters"
            ));
        }
        let shared: Shared = Arc::new((Mutex::new(Jobs::default()), Condvar::new()));
        let verifier = Arc::new(verifier);
        let handler = handler(
            digest(token),
            verifier.clone(),
            shared.clone(),
            options.waiting,
        );
        let server = Server::start_on(address, handler)
            .map_err(|error| format!("`{address}` can't be listened on: {error}"))?;
        let worker = {
            let shared = shared.clone();
            let log = options.log.clone();
            std::thread::spawn(move || work(&verifier, &shared, &*log))
        };
        Ok(Service {
            server: Some(server),
            shared,
            worker: Some(worker),
        })
    }

    pub fn address(&self) -> std::net::SocketAddr {
        self.server.as_ref().map_or_else(
            || std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
            Server::address,
        )
    }

    /// Waits for as long as it runs.
    pub fn wait(mut self) {
        if let Some(server) = self.server.take() {
            server.wait();
        }
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        drop(self.server.take());
        locked(&self.shared).stopping = true;
        self.shared.1.notify_all();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Runs what's waiting, one at a time.
fn work(verifier: &Verifier, shared: &Shared, log: &(dyn Fn(String) + Send + Sync)) {
    loop {
        let (id, request) = {
            let mut jobs = locked(shared);
            loop {
                if jobs.stopping {
                    return;
                }
                if let Some(next) = jobs.queue.pop_front() {
                    break next;
                }
                jobs = shared
                    .1
                    .wait(jobs)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        };
        // A bug here fails that verification and leaves the next to run.
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| verifier.verify(&request)))
                .unwrap_or_else(|_| Outcome::CouldNotCheck {
                    code: "verifier_failed",
                    message: "The verifier stopped while checking it.".to_string(),
                });
        log(format!(
            "{id} {} {}: {}",
            request.repository.url(),
            request.commit,
            match &outcome {
                Outcome::Checked(answer) if answer["passed"] == true => "passed",
                Outcome::Checked(_) => "failed",
                Outcome::CouldNotCheck { code, .. } => code,
            }
        ));
        let mut jobs = locked(shared);
        jobs.jobs.insert(id.clone(), Job::Finished(outcome));
        jobs.finished.push_back(id);
        while jobs.finished.len() > KEPT {
            if let Some(oldest) = jobs.finished.pop_front() {
                jobs.jobs.remove(&oldest);
            }
        }
    }
}

fn digest(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

/// Compares digests so that how long it takes says nothing of either.
fn same(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0u8, |all, (x, y)| all | (x ^ y)) == 0
}

fn json_answer(status: u16, body: &Value) -> Fetched {
    Fetched {
        status,
        content_type: Some("application/json".to_string()),
        body: serde_json::to_vec(body).unwrap_or_default(),
    }
}

/// The failure envelope (area 4, "What the client sees").
fn failure(status: u16, code: &str, message: &str) -> Fetched {
    json_answer(
        status,
        &json!({ "error": { "code": code, "message": message, "request_id": "req_verifier" } }),
    )
}

fn handler(token: [u8; 32], verifier: Arc<Verifier>, shared: Shared, waiting: usize) -> Handler {
    Arc::new(move |incoming: Incoming| {
        let method = incoming.method.as_str();
        let path = incoming.target.split('?').next().unwrap_or("");
        if path == "/health" {
            return if method == "GET" {
                json_answer(200, &json!({ "status": "ok" }))
            } else {
                failure(405, "method_not_allowed", "Only GET is served here.")
            };
        }
        let given = incoming
            .authorization
            .as_deref()
            .and_then(|value| value.strip_prefix("Bearer "));
        if !given.is_some_and(|given| same(&digest(given), &token)) {
            return failure(401, "unauthorized", "This needs the registry's credential.");
        }
        match (method, path) {
            ("POST", "/v1/resolve") => resolve(&verifier, &incoming.body),
            ("POST", "/v1/verifications") => start(&verifier, &shared, waiting, &incoming.body),
            ("GET", _) => match path.strip_prefix("/v1/verifications/") {
                Some(id) => asked(&shared, id),
                None => failure(404, "not_found", "There is nothing here."),
            },
            _ => failure(404, "not_found", "There is nothing here."),
        }
    })
}

fn body_of(body: &[u8]) -> Result<Value, Fetched> {
    serde_json::from_slice::<Value>(body)
        .ok()
        .filter(Value::is_object)
        .ok_or_else(|| failure(422, "invalid_body", "The body has to be a JSON object."))
}

fn repository_of(verifier: &Verifier, body: &Value) -> Result<crate::request::Repository, Fetched> {
    let text = body["repository"].as_str().unwrap_or("");
    verifier.git.repository(text).map_err(|why| {
        failure(
            422,
            "repository_refused",
            &format!("It can't be fetched: {why}."),
        )
    })
}

/// `POST /v1/resolve`: the commit a tag points at.
fn resolve(verifier: &Verifier, body: &[u8]) -> Fetched {
    let body = match body_of(body) {
        Ok(body) => body,
        Err(answer) => return answer,
    };
    let repository = match repository_of(verifier, &body) {
        Ok(repository) => repository,
        Err(answer) => return answer,
    };
    let tag = body["tag"].as_str().unwrap_or("");
    if !is_tag(tag) {
        return failure(422, "tag_refused", "That isn't a release tag's name.");
    }
    match verifier.resolve(&repository, tag) {
        Ok(Some(commit)) => json_answer(200, &json!({ "commit": commit })),
        Ok(None) => failure(404, "tag_not_found", "The repository has no such tag."),
        Err(GitError::Refused(why)) => failure(422, "repository_refused", &why),
        Err(GitError::Failed(why)) => failure(502, "repository_unreachable", &why),
        Err(GitError::TooMuch) => failure(502, "repository_unreachable", "It answered too much."),
    }
}

/// `POST /v1/verifications`: starts one, or finds it.
fn start(verifier: &Verifier, shared: &Shared, waiting: usize, body: &[u8]) -> Fetched {
    let body = match body_of(body) {
        Ok(body) => body,
        Err(answer) => return answer,
    };
    let repository = match repository_of(verifier, &body) {
        Ok(repository) => repository,
        Err(answer) => return answer,
    };
    let commit = body["commit"].as_str().unwrap_or("");
    if !is_commit(commit) {
        return failure(422, "commit_refused", "That isn't a commit's full name.");
    }
    let previous = match &body["previous_digest"] {
        Value::Null => None,
        Value::String(digest) if is_digest(digest) => Some(digest.clone()),
        _ => return failure(422, "digest_refused", "That isn't a release's digest."),
    };
    let named = format!(
        "{}\n{commit}\n{}\n",
        repository.url(),
        previous.as_deref().unwrap_or("")
    );
    let id: String = Sha256::digest(named.as_bytes())
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let mut jobs = locked(shared);
    let again = matches!(
        jobs.jobs.get(&id),
        None | Some(Job::Finished(Outcome::CouldNotCheck { .. }))
    );
    if again {
        if jobs.queue.len() >= waiting {
            return failure(
                503,
                "verifier_busy",
                "Too many are waiting: ask again later.",
            );
        }
        jobs.jobs.insert(id.clone(), Job::Waiting);
        jobs.finished.retain(|finished| *finished != id);
        jobs.queue.push_back((
            id.clone(),
            Request {
                repository,
                commit: commit.to_string(),
                previous,
            },
        ));
        shared.1.notify_all();
    }
    json_answer(202, &json!({ "id": id }))
}

/// `GET /v1/verifications/{id}`.
fn asked(shared: &Shared, id: &str) -> Fetched {
    let jobs = locked(shared);
    match jobs.jobs.get(id) {
        None => failure(
            404,
            "verification_not_found",
            "There is no such verification.",
        ),
        Some(Job::Waiting) => json_answer(
            200,
            &json!({ "id": id, "state": "verifying", "result": null, "failure": null }),
        ),
        Some(Job::Finished(Outcome::Checked(result))) => json_answer(
            200,
            &json!({ "id": id, "state": "checked", "result": result, "failure": null }),
        ),
        Some(Job::Finished(Outcome::CouldNotCheck { code, message })) => json_answer(
            200,
            &json!({
                "id": id,
                "state": "could_not_check",
                "result": null,
                "failure": { "code": code, "message": message },
            }),
        ),
    }
}
