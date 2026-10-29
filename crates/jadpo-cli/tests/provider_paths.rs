//! A provider operation's path placeholders (area 11, "How the input is
//! sent"): each takes its member as one segment, so a caller's value can't
//! name another operation, sent with the connection's credential
//! (security audit S1).

use std::path::Path;
use std::rc::Rc;

use jadpo_drivers::package::JsonHttp;
use jadpo_host_native::Runtime;
use jadpo_host_native::fetch::NativeFetch;
use jadpo_interp::{Executor, Prepared, Program};
use jadpo_runtime::capability::{Capability, Faked};
use jadpo_runtime::{Clock, Ctx, Env, MemoryHost};
use jadpo_value::Value;

use crate::support::fake_provider::FakeProvider;

fn files(sandbox: &str) -> Vec<(&'static str, String)> {
    vec![
        (
            "app.jadpo",
            r#"application {
    connections: {
        bank: {
            package: AcmeBank.v1
            domains: [accounts]
            credential: config.bank_api_key
            environment: config.bank_environment
        }
    }
}
"#
            .to_string(),
        ),
        (
            "config.jadpo",
            r#"config {
    bank_api_key: Text {
        secret: true
    }
    bank_environment: ProviderEnvironment
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_bank/v1/package.jadpo",
            "/// Acme Bank's API.\npackage AcmeBank.v1 {\n    licence: Licence.proprietary\n    jadpo: \"1.0\"\n}\n"
                .to_string(),
        ),
        (
            "shared/providers/acme_bank/v1/provider.jadpo",
            format!(
                r#"/// Acme Bank's API.
provider {{
    protocol: Protocol.json_http
    hosts: {{
        live: "https://api.acme-bank.example"
        sandbox: "{sandbox}"
    }}
    credential: CredentialScheme.bearer
}}
"#
            ),
        ),
        (
            "shared/providers/acme_bank/v1/resources/statements.jadpo",
            r#"public resource statements {
    get: {
        method: HttpMethod.get
        path: "/v1/accounts/{accountId}/statement"
        input: StatementLookup
        output: Statement
        changes: false
        source: PackageSource.written
    }
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_bank/v1/types/statement_lookup.jadpo",
            r#"/// Whose statement.
public type StatementLookup = Object {
    account_id: AccountRef {
        wire_name: "accountId"
    }
}
"#
            .to_string(),
        ),
        (
            "shared/providers/acme_bank/v1/types/account_ref.jadpo",
            "/// An account's reference.\npublic type AccountRef = Text {}\n".to_string(),
        ),
        (
            "shared/providers/acme_bank/v1/types/statement.jadpo",
            "/// A statement.\npublic type Statement = Object {\n    id: Text\n}\n".to_string(),
        ),
        (
            "domains/accounts/actions/statement_for.jadpo",
            r#"use app { bank }
use shared/providers/acme_bank

/// One account's statement, the account named by the caller.
action statement_for(account: AcmeBank.AccountRef) -> AcmeBank.Statement {
    return bank.statements.get(AcmeBank.StatementLookup {
        account_id: account
    })
}
"#
            .to_string(),
        ),
    ]
}

fn program(root: &Path, sandbox: &str) -> Program {
    let _ = std::fs::remove_dir_all(root);
    for (path, text) in files(sandbox) {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    let loaded = jadpo_db::load(root);
    let prepared = Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|problems| {
        panic!(
            "the project checks: {:?}",
            problems.iter().map(|p| &p.message).collect::<Vec<_>>()
        )
    });
    let variables = |name: &str| -> Option<String> {
        match name {
            "BANK_API_KEY" => Some("sk_test_s1".into()),
            "BANK_ENVIRONMENT" => Some("sandbox".into()),
            _ => None,
        }
    };
    let values = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .unwrap_or_else(|problems| panic!("the settings read: {problems:?}"));
    Program::new(prepared).with_config(&values)
}

/// Runs action `action` with `account` against a fake provider: what the
/// provider was sent, and how it ended.
fn called(
    name: &str,
    action: &str,
    account: &str,
) -> (Vec<crate::support::fake_provider::Request>, String) {
    let provider = FakeProvider::start();
    let root = std::env::temp_dir().join(format!(
        "jadpo-provider-paths-{name}-{}",
        std::process::id()
    ));
    let program = program(&root, &provider.url);
    let runtime = Runtime::new().expect("a runtime");
    provider.answer(200, r#"{"id": "st-1"}"#);
    provider.answer(200, r#"{"id": "st-1"}"#);
    let driver: Rc<dyn Capability> = Rc::new(JsonHttp::new(NativeFetch::new()));
    let host = Faked::new(MemoryHost::new()).with("bank", driver);
    let mut ctx = Ctx::of(
        &program,
        &host,
        Env {
            seed: Some(7),
            clock: Clock::TEST,
        },
    );
    let id = program.plan().find(action).expect("the action");
    let outcome = runtime.block_on(program.run(&mut ctx, id, vec![Value::text(account)]));
    let sent = provider.requests();
    let _ = std::fs::remove_dir_all(&root);
    (sent, format!("{outcome:?}"))
}

/// A value of `..`, `.` or nothing in a placeholder would leave its
/// segment: `/v1/accounts/{accountId}/statement` would go out as
/// `/v1/accounts/../statement`, which the provider (and, on Workers,
/// `fetch`'s own URL parser) reads as `/v1/statement`, another operation,
/// with the connection's credential. The call is a fault, and nothing is
/// sent.
#[test]
fn a_placeholder_that_would_leave_its_segment_sends_nothing() {
    for (name, value) in [("dots", ".."), ("dot", "."), ("empty", "")] {
        let (sent, outcome) = called(name, "statement_for", value);
        let paths: Vec<&str> = sent.iter().map(|request| request.path.as_str()).collect();
        assert!(
            paths.is_empty(),
            "`{value}` was sent: {paths:?} ({outcome})"
        );
        assert!(
            outcome.contains("would leave its path segment"),
            "`{value}` is a fault naming the placeholder: {outcome}"
        );
    }
}

/// A value holding dots that isn't a dot segment is one segment, escaped,
/// as before.
#[test]
fn a_placeholder_with_dots_inside_is_sent_as_one_segment() {
    let (sent, outcome) = called("inside", "statement_for", "a..b/c");
    let paths: Vec<&str> = sent.iter().map(|request| request.path.as_str()).collect();
    assert_eq!(paths, ["/v1/accounts/a..b%2Fc/statement"], "{outcome}");
}
