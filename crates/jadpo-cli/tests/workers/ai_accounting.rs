//! Refusal crosses the actual compiled Wasm PostgreSQL driver. No model
//! endpoint is contacted: a caller whose window is full is refused first.
use super::{Database, Workerd, jadpo};
use crate::ai_providers::{STRICT, desk, write};
use crate::deployable::{apply, migrations, rows};
use serde_json::Value;

#[test]
fn an_atomic_budget_refusal_is_recognised_by_the_compiled_worker() {
    if !jadpo_test::live() {
        return;
    }
    let provider = crate::support::fake_provider::FakeProvider::start();
    let root = desk("worker-accounting", &provider.url, STRICT, "");
    let app_path = root.join("app.jadpo");
    let app = std::fs::read_to_string(&app_path).unwrap()
        .replacen("    connections: {", "    connections: {\n        limits: { driver: Driver.cloudflare_rate_limits namespace: 1001 }\n        main: { driver: Driver.postgres url: config.database_url }", 1)
        .replacen("    ai: {", "    rate_limits: { store: limits }\n    deployables: {\n        worker: { serves: [main] target: Target.wasm }\n    }\n    ai: {", 1)
        .replace("        limits: {\n            per_day:", "        limits: {\n            per_caller_per_day: 1100\n            per_day:");
    std::fs::write(&app_path, app).unwrap();
    let config = std::fs::read_to_string(root.join("config.jadpo"))
        .unwrap()
        .replacen("config {", "config {\n    database_url: PostgresUrl", 1);
    write(&root, "config.jadpo", &config);
    write(
        &root,
        "domains/support/failures/budget_spent.jadpo",
        "/// This caller's daily budget is spent.\ntype BudgetSpent = Failure {\n    kind: FailureKind.unavailable\n    message: \"The daily model budget is spent.\"\n}\n",
    );
    let action_path = root.join("domains/support/actions/answer_customer.jadpo");
    let action = std::fs::read_to_string(&action_path)
        .unwrap()
        .replace(
            "{ AssistantUnavailable, QuestionTooLong }",
            "{ AssistantUnavailable, QuestionTooLong, BudgetSpent }",
        )
        .replace(
            "fails AssistantUnavailable, QuestionTooLong",
            "fails AssistantUnavailable, QuestionTooLong, BudgetSpent",
        )
        .replace(
            "over_budget: AssistantUnavailable",
            "over_budget: BudgetSpent",
        );
    std::fs::write(action_path, action).unwrap();
    jadpo(&root, &["api", "update"]);
    jadpo(&root, &["build", "worker"]);
    let database = Database::on_tcp();
    apply(&database.native, &migrations(&root));
    let digest = jadpo_value::hash::sha256(b"address 192.0.2.41");
    let caller: String = digest[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    rows(
        &database.native,
        &format!(
            "INSERT INTO jadpo_ai_usage SELECT (CURRENT_TIMESTAMP AT TIME ZONE 'UTC')::date + day_delta, 'caller', '{caller}', 0, 0, 0, 0, 1090, 0, 0, NULL, '2000-01-01' FROM generate_series(-1, 1) AS days(day_delta)"
        ),
    );
    let before = rows(
        &database.native,
        "SELECT * FROM jadpo_ai_usage ORDER BY day, scope, key",
    );
    assert_eq!(
        before.len(),
        3,
        "adjacent UTC caller windows cover midnight"
    );
    let out = root.join("build/target/worker");
    let wrangler: Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("wrangler.json")).unwrap()).unwrap();
    let worker = Workerd::start(
        &out,
        &wrangler,
        &[
            ("DATABASE_URL", &database.worker),
            ("ACME_KEY", "local-fixture-only"),
        ],
    );
    for _ in 0..2 {
        let (status, body) = worker.send(
            "POST",
            "/answers",
            &[("CF-Connecting-IP", "192.0.2.41")],
            r#"{"question":"Sesame?"}"#,
        );
        assert_eq!(status, 503, "{body}\n{}", worker.log());
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            body["error"]["code"],
            "budget_spent",
            "{body}\n{}",
            worker.log()
        );
        assert_eq!(
            rows(
                &database.native,
                "SELECT * FROM jadpo_ai_usage ORDER BY day, scope, key"
            ),
            before,
            "APP insertion rolled back with unchanged caller"
        );
    }
    assert!(
        provider.requests().is_empty(),
        "refusal never contacts the local provider"
    );
    drop(worker);
}
