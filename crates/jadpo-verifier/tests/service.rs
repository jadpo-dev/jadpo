//! What the registry reaches: a credential on every request, requests
//! checked before anything runs, one verification at a time.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use jadpo_registry::http::{Fetched, fetch_authorized};
use jadpo_registry::protocol::Method;
use jadpo_verifier::service::{Options, Service};
use serde_json::{Value, json};

use super::support::{Dir, repository, sandbox, script};
use super::verification::{report, says, stand_in, verifier};

const TOKEN: &str = "a-credential-the-registry-holds-0123456789";

fn call(
    service: &Service,
    method: Method,
    path: &str,
    body: Option<&Value>,
    token: Option<&str>,
) -> Fetched {
    let authorization = token.map(|token| format!("Bearer {token}"));
    let body = body.map(|body| serde_json::to_vec(body).expect("JSON"));
    fetch_authorized(
        method,
        &format!("http://{}{path}", service.address()),
        body.as_deref(),
        authorization.as_deref(),
    )
    .expect("an answer")
}

fn body(answer: &Fetched) -> Value {
    serde_json::from_slice(&answer.body).expect("JSON")
}

fn code(answer: &Fetched) -> String {
    body(answer)["error"]["code"]
        .as_str()
        .expect("a code")
        .to_string()
}

struct Up {
    service: Service,
    url: String,
    commit: String,
    _dirs: (Dir, Dir, Dir),
}

fn up(name: &str, jadpo: impl FnOnce(&Path) -> std::path::PathBuf, options: Options) -> Option<Up> {
    let sandbox = sandbox()?;
    let repositories = Dir::new(&format!("{name}-repositories"));
    let scratch = Dir::new(&format!("{name}-scratch"));
    let tools = Dir::new(&format!("{name}-tools"));
    let root = repositories.path().join("package");
    std::fs::create_dir_all(&root).expect("a folder");
    let commit = repository(&root, &[("package.jadpo", "package P.v1 {}\n")]);
    let jadpo = jadpo(tools.path());
    let verifier = verifier(sandbox, repositories.path(), scratch.path(), jadpo);
    let service = Service::start("127.0.0.1:0", TOKEN, verifier, options).expect("it starts");
    Some(Up {
        service,
        url: format!("file://{}", root.display()),
        commit,
        _dirs: (repositories, scratch, tools),
    })
}

fn honest(dir: &Path) -> std::path::PathBuf {
    let facts = json!({ "kind": "library" });
    let bar = json!([{ "item": "tests", "state": "passed", "detail": "1 test" }]);
    stand_in(
        dir,
        &report(true, &facts, &[]),
        &says(&report(true, &facts, &[("bar", bar)])),
    )
}

/// Asks after `id` until it has ended.
fn ended(service: &Service, id: &str) -> Value {
    let started = Instant::now();
    loop {
        let answer = call(
            service,
            Method::Get,
            &format!("/v1/verifications/{id}"),
            None,
            Some(TOKEN),
        );
        assert_eq!(answer.status, 200);
        let value = body(&answer);
        if value["state"] != "verifying" {
            return value;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "it never ended"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn every_request_but_health_needs_the_credential() {
    let Some(up) = up("service-credential", honest, Options::default()) else {
        return;
    };
    let service = &up.service;
    let health = call(service, Method::Get, "/health", None, None);
    assert_eq!(health.status, 200);
    assert_eq!(body(&health)["status"], "ok");

    let start = json!({ "repository": up.url, "commit": up.commit, "previous_digest": null });
    for token in [None, Some("wrong"), Some(&TOKEN[..TOKEN.len() - 1])] {
        for (method, path, given) in [
            (Method::Post, "/v1/verifications", Some(&start)),
            (Method::Post, "/v1/resolve", Some(&start)),
            (Method::Get, "/v1/verifications/x", None),
        ] {
            let answer = call(service, method, path, given, token);
            assert_eq!(answer.status, 401, "{path} with {token:?}");
            assert_eq!(code(&answer), "unauthorized");
        }
    }
}

#[test]
fn a_short_credential_isnt_taken() {
    let Some(sandbox) = sandbox() else { return };
    let dir = Dir::new("service-short");
    let verifier = verifier(sandbox, dir.path(), dir.path(), dir.path().join("jadpo"));
    let error = Service::start("127.0.0.1:0", "short", verifier, Options::default());
    assert!(error.is_err());
}

#[test]
fn a_request_is_checked_before_anything_runs() {
    let Some(up) = up(
        "service-checks",
        |dir| script(dir, "jadpo", "exit 9"),
        Options::default(),
    ) else {
        return;
    };
    let service = &up.service;
    for (given, wanted) in [
        (
            json!({ "repository": "http://github.com/a/b", "commit": up.commit }),
            "repository_refused",
        ),
        (
            json!({ "repository": "https://169.254.169.254/a", "commit": up.commit }),
            "repository_refused",
        ),
        (
            json!({ "repository": "https://user:pw@github.com/a/b", "commit": up.commit }),
            "repository_refused",
        ),
        (
            json!({ "repository": "--upload-pack=id", "commit": up.commit }),
            "repository_refused",
        ),
        (json!({ "commit": up.commit }), "repository_refused"),
        (
            json!({ "repository": up.url, "commit": "main" }),
            "commit_refused",
        ),
        (
            json!({ "repository": up.url, "commit": format!("--{}", "a".repeat(38)) }),
            "commit_refused",
        ),
        (
            json!({ "repository": up.url, "commit": up.commit, "previous_digest": "x" }),
            "digest_refused",
        ),
    ] {
        let answer = call(
            service,
            Method::Post,
            "/v1/verifications",
            Some(&given),
            Some(TOKEN),
        );
        assert_eq!(answer.status, 422, "{given}");
        assert_eq!(code(&answer), wanted, "{given}");
    }
    let answer = call(
        service,
        Method::Post,
        "/v1/resolve",
        Some(&json!({ "repository": up.url, "tag": "--upload-pack=id" })),
        Some(TOKEN),
    );
    assert_eq!(code(&answer), "tag_refused");
    let answer = call(
        service,
        Method::Post,
        "/v1/verifications",
        Some(
            &json!({ "repository": up.url, "commit": up.commit, "previous_digest": null, "publisher": "acme" }),
        ),
        Some(TOKEN),
    );
    assert_eq!(code(&answer), "publisher_refused");
    let answer = call(
        service,
        Method::Get,
        "/v1/verifications/none",
        None,
        Some(TOKEN),
    );
    assert_eq!(answer.status, 404);
    assert_eq!(code(&answer), "verification_not_found");
}

#[test]
fn a_verification_is_started_and_asked_after() {
    let logged = Arc::new(Mutex::new(Vec::new()));
    let log = logged.clone();
    let options = Options {
        log: Arc::new(move |line| log.lock().expect("the log").push(line)),
        ..Options::default()
    };
    let Some(up) = up("service-flow", honest, options) else {
        return;
    };
    let service = &up.service;
    let given = json!({ "repository": up.url, "commit": up.commit, "previous_digest": null });
    let started = call(
        service,
        Method::Post,
        "/v1/verifications",
        Some(&given),
        Some(TOKEN),
    );
    assert_eq!(started.status, 202);
    let id = body(&started)["id"].as_str().expect("an id").to_string();
    let answer = ended(service, &id);
    assert_eq!(answer["state"], "checked", "{answer}");
    assert_eq!(answer["failure"], Value::Null);
    let result = &answer["result"];
    assert_eq!(result["passed"], true, "{result}");
    assert!(
        result["digest"]
            .as_str()
            .expect("a digest")
            .starts_with("sha256:")
    );

    // What a model reads comes on its own, not in the result.
    assert_eq!(result["reading"], Value::Null);
    let source = call(
        service,
        Method::Get,
        &format!("/v1/verifications/{id}/source"),
        None,
        Some(TOKEN),
    );
    assert_eq!(source.status, 200);
    assert_eq!(body(&source)["files"][0]["path"], "package.jadpo");
    assert_eq!(body(&source)["files"][0]["text"], "package P.v1 {}\n");
    let diff = call(
        service,
        Method::Get,
        &format!("/v1/verifications/{id}/diff"),
        None,
        Some(TOKEN),
    );
    assert_eq!(body(&diff)["diff"], Value::Null);
    assert_eq!(body(&diff)["truncated"], false);
    let unknown = call(
        service,
        Method::Get,
        "/v1/verifications/none/source",
        None,
        Some(TOKEN),
    );
    assert_eq!(unknown.status, 404);
    let refused = call(
        service,
        Method::Get,
        &format!("/v1/verifications/{id}/source"),
        None,
        None,
    );
    assert_eq!(refused.status, 401);

    // Asking again names the same one and runs nothing twice.
    let again = call(
        service,
        Method::Post,
        "/v1/verifications",
        Some(&given),
        Some(TOKEN),
    );
    assert_eq!(body(&again)["id"], id);
    assert_eq!(ended(service, &id)["result"]["digest"], result["digest"]);
    assert_eq!(logged.lock().expect("the log").len(), 1);
    assert!(!logged.lock().expect("the log")[0].contains(TOKEN));
}

#[test]
fn a_commit_that_couldnt_be_fetched_can_be_tried_again() {
    let logged = Arc::new(Mutex::new(0));
    let log = logged.clone();
    let options = Options {
        log: Arc::new(move |_| *log.lock().expect("the count") += 1),
        ..Options::default()
    };
    let Some(up) = up("service-again", honest, options) else {
        return;
    };
    let service = &up.service;
    let given = json!({ "repository": up.url, "commit": "b".repeat(40), "previous_digest": null });
    for runs in 1..=2 {
        let started = call(
            service,
            Method::Post,
            "/v1/verifications",
            Some(&given),
            Some(TOKEN),
        );
        let id = body(&started)["id"].as_str().expect("an id").to_string();
        let answer = ended(service, &id);
        assert_eq!(answer["state"], "could_not_check", "{answer}");
        assert_eq!(answer["failure"]["code"], "commit_not_fetched");
        assert_eq!(answer["result"], Value::Null);
        assert_eq!(*logged.lock().expect("the count"), runs);
    }
}

#[test]
fn a_verification_stopped_at_its_limits_leaves_the_next_to_run() {
    let Some(sandbox) = sandbox() else { return };
    let repositories = Dir::new("service-next-repositories");
    let scratch = Dir::new("service-next-scratch");
    let tools = Dir::new("service-next-tools");
    let root = repositories.path().join("package");
    std::fs::create_dir_all(&root).expect("a folder");
    let first = repository(&root, &[("package.jadpo", "package P.v1 {}\n")]);
    std::fs::write(root.join("a.jadpo"), "second\n").expect("a file");
    super::support::git(&root, &["add", "-A"]);
    super::support::git(&root, &["commit", "-q", "-m", "the second"]);
    let second = super::support::git(&root, &["rev-parse", "HEAD"]);
    let facts = json!({ "kind": "library" });
    // The commit is in no argument: a package's own files say which it is.
    let run = format!(
        "for arg in \"$@\"; do case \"$arg\" in */new) new=$arg;; esac; done\n\
         if [ -e \"$new/a.jadpo\" ]; then\n{}\nelse\nwhile :; do :; done\nfi",
        says(&report(true, &facts, &[]))
    );
    let jadpo = stand_in(tools.path(), &report(true, &facts, &[]), &run);
    let mut verifier = verifier(sandbox, repositories.path(), scratch.path(), jadpo);
    verifier.limits.cpu_seconds = 1;
    let service =
        Service::start("127.0.0.1:0", TOKEN, verifier, Options::default()).expect("it starts");
    let url = format!("file://{}", root.display());
    let start = |commit: &str| {
        let given = json!({ "repository": url, "commit": commit, "previous_digest": null });
        let answer = call(
            &service,
            Method::Post,
            "/v1/verifications",
            Some(&given),
            Some(TOKEN),
        );
        body(&answer)["id"].as_str().expect("an id").to_string()
    };
    let (spins, passes) = (start(&first), start(&second));
    let stopped = ended(&service, &spins);
    assert_eq!(stopped["result"]["passed"], false, "{stopped}");
    let next = ended(&service, &passes);
    assert_eq!(next["result"]["passed"], true, "{next}");
    assert_eq!(next["result"]["facts"], facts);
}

#[test]
fn too_many_waiting_are_refused_rather_than_kept() {
    let options = Options {
        waiting: 1,
        ..Options::default()
    };
    let slow = |dir: &Path| script(dir, "jadpo", "sleep 5; exit 9");
    let Some(up) = up("service-busy", slow, options) else {
        return;
    };
    let service = &up.service;
    let start = |commit: String| {
        let given = json!({ "repository": up.url, "commit": commit, "previous_digest": null });
        call(
            service,
            Method::Post,
            "/v1/verifications",
            Some(&given),
            Some(TOKEN),
        )
    };
    let statuses: Vec<u16> = (0..6u8)
        .map(|n| start(format!("{n:0>40}")).status)
        .collect();
    assert_eq!(statuses[0], 202);
    assert!(statuses.contains(&503), "{statuses:?}");
}
