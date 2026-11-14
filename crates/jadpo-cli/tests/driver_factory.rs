//! The connections table and the driver factory (chunk 8.1d): a checked
//! project's plan lists every connection once, in name order, with its
//! driver, the settings it is built from and the targets it runs on, and
//! the factory builds each connection's driver from it and the settings
//! the program started with, as a deployable will: the SMTP driver
//! signing in to its server with the password a secret setting gives,
//! over the native host's sockets. Nothing reaches a network: the SMTP
//! server is on this machine.

use std::rc::Rc;

use jadpo_host_native::Runtime;
use jadpo_interp::{Prepared, Program};
use jadpo_runtime::capability::{Answer, Call};
use jadpo_std::connections::{ConnectionDriver, Target};
use jadpo_value::Value;

use super::http::{jadpo, tempdir, write};
use crate::support::fake_smtp::FakeSmtp;

/// An app with a store of files, an SMTP server at `port` on this
/// machine, a model and a database, each reading its settings.
fn files(port: u16) -> Vec<(&'static str, String)> {
    vec![
        (
            "app.jadpo",
            format!(
                r#"application {{
    display_name: "Acme"
    connections: {{
        uploads: {{
            driver: Driver.disk
        }}
        mail: {{
            driver: Driver.smtp
            host: config.smtp_host
            port: {port}
            credential: {{
                username: config.smtp_username
                password: config.smtp_password
            }}
        }}
        local_model: {{
            driver: Driver.openai_compatible
            url: "http://localhost:11434/v1"
        }}
        main: {{
            driver: Driver.postgres
            url: config.database_url
        }}
    }}
    email: {{
        connection: mail
        from: "hello@acme.test"
    }}
}}

locales {{
    default: Locale.en_gb
    supported: [Locale.en_gb]
}}
"#
            ),
        ),
        (
            "config.jadpo",
            r#"config {
    database_url: PostgresUrl {}
    smtp_host: Text {
        default: "127.0.0.1"
    }
    smtp_password: Text {
        secret: true
    }
    smtp_username: Text {
        default: "acme"
    }
}
"#
            .to_string(),
        ),
    ]
}

fn variables(name: &str) -> Option<String> {
    match name {
        "SMTP_PASSWORD" => Some("s3cret".into()),
        "DATABASE_URL" => Some("postgresql://acme@db.internal:6543/app".into()),
        _ => None,
    }
}

fn message() -> Value {
    Value::record(
        "EmailMessage",
        vec![
            (Rc::from("from"), Value::text("hello@acme.test")),
            (Rc::from("to"), Value::text("ana@example.test")),
            (Rc::from("subject"), Value::text("Welcome, Ana")),
            (Rc::from("html"), Value::None),
            (Rc::from("text"), Value::text("Hi")),
            (Rc::from("attachments"), Value::List(Vec::new().into())),
        ],
    )
}

#[test]
fn a_deployable_builds_each_connections_driver_from_the_plan() {
    let server = FakeSmtp::start();
    let dir = tempdir::Dir::new("driver-factory");
    let root = dir.path().to_path_buf();
    for (path, text) in files(server.port) {
        write(&root, path, &text);
    }
    jadpo(&root, &["records", "update"]);
    let out = jadpo(&root, &["check"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    let loaded = jadpo_db::load(&root);
    let prepared = Prepared::load(&loaded.db, loaded.project).unwrap_or_else(|problems| {
        panic!(
            "the project checks: {:?}",
            problems.iter().map(|p| &p.message).collect::<Vec<_>>()
        )
    });
    let values = prepared
        .config(
            &variables,
            jadpo_runtime::config::Mode::Environment,
            &|_: &mut [u8]| false,
        )
        .unwrap_or_else(|problems| panic!("the settings read: {problems:?}"));
    let program = Program::new(prepared).with_config(&values);

    // One table, in name order, each driver with the targets it runs on.
    let table = program.runtime_plan().connections;
    let listed: Vec<(&str, &str, &[Target])> = table
        .iter()
        .map(|connection| {
            (
                connection.name.as_str(),
                connection.driver.name(),
                connection.targets.as_slice(),
            )
        })
        .collect();
    let everywhere: &[Target] = &[Target::Native, Target::Workers];
    assert_eq!(
        listed,
        [
            ("local_model", "Driver.openai_compatible", everywhere),
            ("mail", "Driver.smtp", everywhere),
            ("main", "Driver.postgres", everywhere),
            ("uploads", "Driver.disk", &[Target::Native][..]),
        ]
    );
    let Some(ConnectionDriver::Smtp(mail)) = table.get(1).map(|mail| &mail.driver) else {
        panic!("the SMTP connection's driver")
    };
    assert_eq!(mail.port, server.port);

    // Built as a deployable builds them: the disk isn't given natively
    // yet, so it alone is left unbuilt, with why.
    let drivers =
        jadpo_host_native::Drivers::build(table, program.started(), &jadpo_host_native::supplies());
    let unbuilt: Vec<String> = drivers.unbuilt().iter().map(ToString::to_string).collect();
    assert_eq!(
        unbuilt,
        ["`uploads`'s Driver.disk needs a disk, which this host doesn't give"]
    );
    assert!(drivers.capability("local_model").is_some());
    assert!(drivers.database("main").is_some());

    // The SMTP driver signs in with the secret setting's password.
    let driver = drivers.capability("mail").expect("the SMTP driver");
    let call = Call::new(
        "mail",
        jadpo_std::email::SEND,
        message(),
        jadpo_std::subscriber::Repeat::Unkeyed,
    );
    let answer = Runtime::new()
        .expect("runtime")
        .block_on(driver.call(&call));
    assert!(matches!(answer, Answer::Value(_)), "{answer:?}");
    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].signed_in,
        Some(("acme".to_string(), "s3cret".to_string()))
    );
    assert_eq!(received[0].to, ["ana@example.test"]);
}

/// A Redis connection (chunk Q8): the plan lists it with the setting its
/// address is read from, and the factory builds the driver a host counts
/// rate limits in, which signs in with the secret address's password and
/// counts there; an address the driver can't read is kept unbuilt, saying
/// why and never the address.
#[test]
fn a_redis_connection_is_built_into_the_store_rate_limits_are_counted_in() {
    use jadpo_drivers::redis::testing::{Double, TcpDouble};

    let double = TcpDouble::start(Double::signing_in(None, "s3cret"));
    let dir = tempdir::Dir::new("driver-factory-redis");
    let root = dir.path().to_path_buf();
    write(
        &root,
        "app.jadpo",
        "application {\n    connections: {\n        cache: {\n            driver: Driver.redis\n            url: config.redis_url\n        }\n    }\n    rate_limits: {\n        store: cache\n    }\n}\n",
    );
    write(&root, "config.jadpo", "config {\n    redis_url: RedisUrl\n}\n");
    jadpo(&root, &["records", "update"]);
    let out = jadpo(&root, &["check"]);
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    let loaded = jadpo_db::load(&root);
    let prepared = Prepared::load(&loaded.db, loaded.project).expect("the project checks");
    let build = |url: String| {
        let values = prepared
            .config(
                &|name: &str| (name == "REDIS_URL").then(|| url.clone()),
                jadpo_runtime::config::Mode::Environment,
                &|_: &mut [u8]| false,
            )
            .unwrap_or_else(|problems| panic!("the settings read: {problems:?}"));
        let program = Program::new(prepared.clone()).with_config(&values);
        let table = program.runtime_plan().connections;
        let cache = table
            .iter()
            .find(|connection| connection.name == "cache")
            .expect("the Redis connection");
        assert!(matches!(
            cache.driver,
            ConnectionDriver::Redis { url: Some(_) }
        ));
        jadpo_host_native::Drivers::build(table, program.started(), &jadpo_host_native::supplies())
    };

    // The implied `main` has no address here; the store is built.
    let drivers = build(format!("redis://:s3cret@127.0.0.1:{}", double.port()));
    assert!(
        drivers
            .unbuilt()
            .iter()
            .all(|unbuilt| unbuilt.connection != "cache"),
        "{:?}",
        drivers.unbuilt()
    );
    let store = drivers.counting_store("cache").expect("the store");
    let limit = jadpo_std::http::RateLimit {
        requests: 1,
        window_ms: 60_000,
    };
    let answers = Runtime::new().expect("runtime").block_on(async {
        [
            store.count("address 203.0.113.9", limit).await,
            store.count("address 203.0.113.9", limit).await,
        ]
    });
    assert_eq!(answers[0], Ok(jadpo_runtime::Allowed::Yes));
    assert!(
        matches!(answers[1], Ok(jadpo_runtime::Allowed::No { .. })),
        "{answers:?}"
    );
    assert_eq!(double.state().sent[0], "AUTH");

    let unread = build("redis://:s3cret@127.0.0.1:6379/0?ssl=true".to_string());
    let unbuilt: Vec<String> = unread
        .unbuilt()
        .iter()
        .filter(|unbuilt| unbuilt.connection == "cache")
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        unbuilt,
        ["`cache`'s Driver.redis can't be built: its `url:` has options after `?`, which the Redis driver doesn't read"]
    );
    assert!(!unbuilt[0].contains("s3cret"));
    assert!(unread.counting_store("cache").is_none());
}
