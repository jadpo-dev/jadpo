//! The real compiled Worker uses the same HTTPS issuer and PostgreSQL
//! fixture as the native runtime. Only the test's outbound service maps
//! the public fixture names to a server with a generated trusted CA.
use super::{Database, Workerd, jadpo};
use crate::external_tokens::{
    self as fixture, AUDIENCE, ISSUER, Provider, SUBJECT, claims, token, token_with,
};
use serde_json::{Value, json};

#[test]
fn external_subjects_are_checked_by_the_compiled_worker_runtime() {
    if !jadpo_test::live() {
        return;
    }
    let root = fixture::project("workers");
    jadpo(&root, &["build", "worker"]);
    let out = root.join("build/target/worker");
    let wrangler: Value = serde_json::from_str(
        &std::fs::read_to_string(out.join("wrangler.json")).expect("Wrangler"),
    )
    .expect("JSON");
    assert_eq!(
        wrangler["compatibility_flags"],
        json!(["global_fetch_strictly_public"])
    );
    let database = Database::on_tcp();
    crate::deployable::apply(&database.native, &crate::deployable::migrations(&root));
    fixture::seed(&database.native);
    let provider = Provider::start();
    let asymmetric: Vec<_> = ["RS256", "ES256"]
        .iter()
        .map(|algorithm| fixture::provider_token(algorithm, SUBJECT))
        .collect();
    provider.state.lock().expect("state").extra_keys =
        asymmetric.iter().map(|(_, key)| key.clone()).collect();
    provider.state.lock().expect("provider").delay_ms = 1_200;
    let slow = Workerd::start_with_provider(
        &out,
        &wrangler,
        &[
            ("DATABASE_URL", &database.worker),
            ("ISSUER", ISSUER),
            ("AUDIENCE", AUDIENCE),
        ],
        Some(&provider),
    );
    let (status, body) = slow.send(
        "GET",
        "/self",
        &[("Authorization", &format!("Bearer {}", token(SUBJECT)))],
        "",
    );
    assert_eq!(
        status, 504,
        "outer route deadline covers provider discovery: {body}"
    );
    drop(slow);
    {
        let mut state = provider.state.lock().expect("state");
        state.delay_ms = 0;
        state.paths.clear();
    }
    let worker = Workerd::start_with_provider(
        &out,
        &wrangler,
        &[
            ("DATABASE_URL", &database.worker),
            ("ISSUER", ISSUER),
            ("AUDIENCE", AUDIENCE),
        ],
        Some(&provider),
    );
    let credential = token(SUBJECT);
    for _ in 0..2 {
        let (status, body) = worker.send(
            "GET",
            "/self",
            &[("Authorization", &format!("Bearer {credential}"))],
            "",
        );
        assert_eq!(status, 200, "{body}\n{}", worker.log());
        let body: Value = serde_json::from_str(&body).expect("JSON");
        assert_eq!(body["email"], "stored@example.test");
    }
    for (credential, _) in &asymmetric {
        let (status, body) = worker.send(
            "GET",
            "/self",
            &[("Authorization", &format!("Bearer {credential}"))],
            "",
        );
        assert_eq!(status, 200, "{body}\n{}", worker.log());
    }
    assert_eq!(provider.state.lock().expect("provider").paths.len(), 2);
    let (status, body) = worker.send(
        "GET",
        "/self",
        &[
            ("Authorization", &format!("Bearer {credential}")),
            ("Cookie", "records_session=invalid"),
        ],
        "",
    );
    assert_eq!(status, 401, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).expect("JSON")["error"]["code"],
        "ambiguous_credentials"
    );
    let mut wrong = claims(SUBJECT);
    wrong["aud"] = "another-app".into();
    let wrong = token_with(json!({"alg":"EdDSA","kid":"fixture"}), wrong);
    let (status, body) = worker.send(
        "GET",
        "/self",
        &[("Authorization", &format!("Bearer {wrong}"))],
        "",
    );
    assert_eq!(status, 401, "{body}");
    assert_eq!(provider.state.lock().expect("provider").paths.len(), 2);
    crate::deployable::rows(&database.native, "UPDATE \"reader\" SET \"active\"=false");
    let (status, body) = worker.send(
        "GET",
        "/self",
        &[("Authorization", &format!("Bearer {credential}"))],
        "",
    );
    assert_eq!(status, 403, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).expect("JSON")["error"]["code"],
        "user_disabled"
    );
}
