//! `jadpo manifest send [<deployable>]` and `jadpo manifest protocol
//! <endpoint>` (area 14, "`jadpo manifest`"; area 16, "The manifest
//! protocol"; chunk O3).
//!
//! `send` is run as a deploy happens. It checks the project as `jadpo
//! build` does, stopping at its errors, and gives the monitoring product
//! at the telemetry endpoint each deployable's manifest
//! ([`crate::manifest_protocol`]): `PUT jadpo/v1/builds/<build>`, read with
//! the credential the deployed app sends its telemetry with. The endpoint
//! and credential are read as the deployed app reads them, from the
//! environment and then `.env.local`, so nothing but those two settings
//! has to be set where it runs. Sent, it is one status line, a row for
//! each deployable:
//!
//! ```text
//! ✓ Sent 2 build manifests to https://api.honeycomb.io
//!     api     b_9a1f03c6d2e4 · new
//!     worker  b_77c0e1d2a3b4 · already held
//! ```
//!
//! An app with no `telemetry:` section, an endpoint no variable gives, a
//! product that can't be reached or refuses a manifest is
//! `manifest.send_failed`, exiting 1, so the deploy stops there; the
//! endpoint is named by its origin, never its path, and the credential
//! never.
//!
//! `protocol` is the conformance test a monitoring product's CI runs
//! against its own endpoint, with `OTEL_EXPORTER_OTLP_HEADERS` as its
//! credential when it has one. It needs no project. One that conforms is
//! one line; one that differs is one error, then each difference under
//! the request it is in, then the footer, as `jadpo package protocol`'s:
//!
//! ```text
//! ✓ Conforms to the manifest protocol  https://collector.example · 7 checks
//! ```

use std::path::PathBuf;

use jadpo_check::deployables::{Deployable, Deployables};
use jadpo_diag::{Decider, Diagnostic, nearest, rule};
use jadpo_graph::manifest::{Build, build_manifest};
use jadpo_runtime::config::Variables;
use jadpo_term::{Block, Fact, Json, Outcome, Row, RowState, Screen};

use crate::check::{Checked, JudgedOn, checked_before, reported};
use crate::fmt::project_needed;
use crate::help::VERSION;
use crate::hq;
use crate::manifest_protocol::{self, Conformance, PATH};
use crate::package::plural;
use crate::run::{Call, Reply, say};
use crate::stamp;

/// What `jadpo manifest` does.
pub const ACTIONS: &[crate::table::Value] = &[
    crate::table::Value {
        name: "send",
        description: "Give the monitoring product each build's manifest",
    },
    crate::table::Value {
        name: "protocol",
        description: "Check that a monitoring product follows the protocol",
    },
];

pub fn manifest(call: &Call) -> Reply {
    match call.arguments.first().map(String::as_str) {
        Some("send") => send(call),
        Some("protocol") => protocol(call),
        Some(other) => {
            let near = nearest(other, ACTIONS.iter().map(|action| action.name));
            Reply::failed(arguments(
                format!("`{other}` isn't something `jadpo manifest` does"),
                "`jadpo manifest send` gives the monitoring product each build's manifest; `jadpo manifest protocol <endpoint>` checks that a product follows the protocol.".to_string(),
                match near {
                    Some(near) => format!("Use `{near}` instead of `{other}`"),
                    None => "Run `jadpo manifest send`".to_string(),
                },
            ))
        }
        None => Reply::failed(arguments(
            "`jadpo manifest` needs an action".to_string(),
            "`jadpo manifest send` gives the monitoring product each build's manifest; `jadpo manifest protocol <endpoint>` checks that a product follows the protocol.".to_string(),
            "Run `jadpo manifest send`".to_string(),
        )),
    }
}

fn arguments(message: String, reason: String, next: String) -> Diagnostic {
    Diagnostic::new(&rule::CLI_MANIFEST_ARGUMENTS, message)
        .reason(reason)
        .next_for(next, Decider::Agent)
}

fn send_failed(message: String, reason: String, next: &str) -> Diagnostic {
    Diagnostic::new(&rule::MANIFEST_SEND_FAILED, message)
        .reason(reason)
        .next(next)
}

fn send(call: &Call) -> Reply {
    let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
    let Some(root) = jadpo_db::find_root(&cwd) else {
        return project_needed("manifest send");
    };
    if let Some(extra) = call.arguments.get(2) {
        return Reply::failed(arguments(
            format!("`jadpo manifest send` takes one deployable, not `{extra}` as well"),
            "It sends every deployable's manifest, or the one deployable it names.".to_string(),
            "Run `jadpo manifest send`".to_string(),
        ));
    }
    let started = std::time::Instant::now();
    say("Checking");
    let checked = match checked_before(call, &root, JudgedOn::Commit) {
        Ok(checked) => checked,
        Err(reply) => return *reply,
    };
    if !checked.diagnostics.is_empty() {
        return reported(
            &cwd,
            checked,
            started.elapsed(),
            "manifest not sent",
            call.flag("--full"),
        );
    }
    let Checked { loaded, .. } = checked;
    let db = &loaded.db;
    let project = loaded.project;
    let declared = jadpo_check::deployables::deployables(db, project);
    let chosen: Vec<&Deployable> = match call.arguments.get(1) {
        Some(name) => match declared.get(name) {
            Some(deployable) => vec![deployable],
            None => return Reply::failed(unknown(name, declared)),
        },
        None => declared.list.iter().collect(),
    };
    let lock = std::fs::read(root.join(jadpo_db::LOCK)).unwrap_or_default();
    let stores = stamp::stores(db, project);
    let graph = jadpo_check::graph::graph(db, project);
    let mut programs = Vec::new();
    for deployable in &chosen {
        match stamp::program(db, project, deployable, VERSION, &lock, &stores) {
            Ok(program) => programs.push((*deployable, program)),
            Err(why) => {
                return Reply::failed(send_failed(
                    format!("`{}`'s build couldn't be made", deployable.name),
                    format!("{why}. This is Jadpo's own mistake."),
                    "Report it with the project that shows it",
                ));
            }
        }
    }
    let Some((_, first)) = programs.first() else {
        return Reply::failed(send_failed(
            "The app has no deployable".to_string(),
            "There is nothing to send a manifest for.".to_string(),
            "Declare a deployable in `app.jadpo`",
        ));
    };
    let runtime = first.plan.runtime();
    let Some(section) = runtime.telemetry.as_ref() else {
        return Reply::failed(send_failed(
            "The app sends no telemetry, so its manifest has nowhere to go".to_string(),
            "A build's manifest goes to the endpoint the app's `telemetry:` section names, read with its credential, and `app.jadpo` has no `telemetry:` section.".to_string(),
            "Add a `telemetry:` section to `application { }`, naming where the app's telemetry goes",
        ));
    };
    let layered = match crate::config::Layered::new(&root, &call.environment) {
        Ok(layered) => layered,
        Err(problem) => return Reply::failed(*problem),
    };
    let variable = |name: &str| layered.variable(name);
    let (endpoint, headers) = match jadpo_runtime::telemetry::destination_by_variable(
        section,
        &runtime.config,
        &variable,
    ) {
        Ok(found) => found,
        Err(unsendable) => {
            return Reply::failed(send_failed(
                "The build's manifest has nowhere to go".to_string(),
                format!(
                    "{}. `jadpo manifest send` reads the endpoint and credential as the deployed app does, from the environment and then `.env.local`.",
                    upper_first(&unsendable.message())
                ),
                "Set it as the deployment does, then run this again",
            ));
        }
    };
    let origin = jadpo_runtime::telemetry::origin(&endpoint);
    let address = match hq::endpoint_address(&endpoint) {
        Ok(address) => address,
        Err(why) => {
            return Reply::failed(send_failed(
                format!("`{origin}` can't be sent to"),
                why,
                "Give the collector's address, a host, an optional port and path",
            ));
        }
    };
    let mut sent = Vec::new();
    for (deployable, program) in &programs {
        let reached = jadpo_check::deployables::reached(db, project, deployable);
        let body = build_manifest(
            graph,
            &Build {
                deployable: &deployable.name,
                target: deployable.builds_for().name(),
                build: &program.build,
                compiler: VERSION,
            },
            &reached.callables,
        )
        .to_text();
        let refused = |why: String| {
            let before = if sent.is_empty() {
                String::new()
            } else {
                format!(
                    " {} sent before it.",
                    sent.iter()
                        .map(|(name, _, _): &(String, String, &str)| format!("`{name}`'s was"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            Reply::failed(send_failed(
                format!("`{}`'s manifest wasn't sent", deployable.name),
                format!("{why}{before}"),
                "Check the endpoint and credential the deployment sets, then run this again",
            ))
        };
        if body.len() > manifest_protocol::MOST {
            return refused(format!(
                "It is {} bytes, past the protocol's 16 MiB.",
                body.len()
            ));
        }
        let path = format!("{PATH}{}", program.build);
        match hq::request_retrying(&address, "PUT", &path, &headers, Some(&body)) {
            Ok(answer) if answer.status == 201 || answer.status == 200 => {
                let held = if answer.status == 201 {
                    "new"
                } else {
                    "already"
                };
                sent.push((deployable.name.clone(), program.build.clone(), held));
            }
            Ok(answer) => {
                let said: serde_json::Value =
                    serde_json::from_str(&answer.body).unwrap_or_default();
                let code = said["error"]["code"]
                    .as_str()
                    .map_or_else(String::new, |code| format!(" `{code}`"));
                let message = said["error"]["message"]
                    .as_str()
                    .map_or_else(String::new, |message| format!(": {message}"));
                return refused(format!(
                    "`{origin}` answered {}{code}{message}",
                    answer.status
                ));
            }
            Err(why) => return refused(why),
        }
    }
    let rows = sent
        .iter()
        .map(|(name, build, held)| {
            let held = if *held == "new" {
                "new"
            } else {
                "already held"
            };
            (name.clone(), format!("{build} · {held}"))
        })
        .collect();
    let screen = Screen::new().with(Block::Status {
        outcome: Outcome::Success,
        text: format!(
            "Sent {} to {origin}",
            plural(sent.len(), "build manifest", "build manifests")
        ),
        rows,
    });
    let data = Json::object([
        ("endpoint", Json::from(origin.as_str())),
        (
            "builds",
            Json::array(sent.iter().map(|(name, build, held)| {
                Json::object([
                    ("deployable", Json::from(name.as_str())),
                    ("build", Json::from(build.as_str())),
                    ("held", Json::from(*held)),
                ])
            })),
        ),
    ]);
    Reply::screen(screen, data)
}

/// A name `jadpo manifest send` was given that isn't one of the
/// deployables.
fn unknown(name: &str, declared: &Deployables) -> Diagnostic {
    let names = declared.names();
    let near = nearest(name, names.iter().copied());
    let listed: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    let reason = if declared.declared {
        format!("`app.jadpo` declares {}.", listed.join(", "))
    } else {
        format!(
            "The app declares no `deployables:` in `app.jadpo`, so it has one deployable, `{}`.",
            jadpo_check::deployables::APP
        )
    };
    arguments(
        format!("`{name}` isn't one of the app's deployables"),
        reason,
        match near {
            Some(near) => format!("Run `jadpo manifest send {near}`"),
            None => "Run `jadpo manifest send` to send them all".to_string(),
        },
    )
}

fn upper_first(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

const HOLDS: &str = "Every monitoring product decodes Jadpo's telemetry against the manifests it is given through the protocol, so a product that answers differently can't be given them, or decodes them differently from the others.";

fn protocol(call: &Call) -> Reply {
    let Some(endpoint) = call.arguments.get(1) else {
        return Reply::failed(arguments(
            "`jadpo manifest protocol` needs an endpoint".to_string(),
            "It runs the manifest protocol's requests against a monitoring product's telemetry endpoint, the address the app's telemetry is sent to.".to_string(),
            "Run `jadpo manifest protocol https://collector.example`".to_string(),
        ));
    };
    if let Some(extra) = call.arguments.get(2) {
        return Reply::failed(arguments(
            format!("`jadpo manifest protocol` takes one endpoint, not `{extra}` as well"),
            "It checks one product's endpoint at a time.".to_string(),
            format!("Run `jadpo manifest protocol {endpoint}`"),
        ));
    }
    let origin = jadpo_runtime::telemetry::origin(endpoint);
    let address = match hq::endpoint_address(endpoint) {
        Ok(address) => address,
        Err(why) => {
            return Reply::failed(arguments(
                format!("`{endpoint}` isn't an endpoint"),
                why,
                "Give the product's telemetry endpoint, such as `https://collector.example`"
                    .to_string(),
            ));
        }
    };
    let headers = match call
        .environment
        .var(jadpo_std::telemetry::HEADERS_VARIABLE)
        .filter(|text| !text.is_empty())
    {
        Some(text) => match jadpo_runtime::telemetry::headers_of(text) {
            Some(headers) => headers,
            None => {
                return Reply::failed(arguments(
                    format!("`{}` can't be read", jadpo_std::telemetry::HEADERS_VARIABLE),
                    format!(
                        "It holds the product's credential as comma-separated `name=value` pairs, and `{}` isn't.",
                        jadpo_std::telemetry::HEADERS_VARIABLE
                    ),
                    "Write it as `name=value`, such as `x-api-key=…`".to_string(),
                ));
            }
        },
        None => Vec::new(),
    };
    report(&origin, &manifest_protocol::conformance(&address, &headers))
}

fn report(origin: &str, found: &Conformance) -> Reply {
    let data = [
        ("endpoint", Json::from(origin)),
        ("conforms", found.mismatches.is_empty().into()),
        ("checks", Json::from(found.checks)),
        (
            "mismatches",
            Json::array(found.mismatches.iter().map(|mismatch| {
                Json::object([
                    ("request", Json::from(mismatch.request.as_str())),
                    ("problem", Json::from(mismatch.problem.as_str())),
                ])
            })),
        ),
    ];
    if found.mismatches.is_empty() {
        let screen = Screen::new().with(Block::summary(
            Outcome::Success,
            "Conforms to the manifest protocol",
            [
                Fact::plain(origin),
                Fact::plain(plural(found.checks, "check", "checks")),
            ],
        ));
        return Reply::screen(screen, Json::object(data));
    }
    let problem = Diagnostic::new(
        &rule::MANIFEST_PROTOCOL_MISMATCH,
        format!("`{origin}` differs from the manifest protocol"),
    )
    .reason(HOLDS)
    .next("Change what each request below answers, then run this again");
    let mut screen = Screen::new().with(Block::diagnostic_from(
        &problem,
        &std::collections::BTreeMap::new(),
        None,
    ));
    for mismatch in &found.mismatches {
        screen.push(Block::list(
            mismatch.request.as_str(),
            vec![Row::new(mismatch.problem.clone(), "").state(RowState::Failed)],
        ));
    }
    screen.push(Block::summary(
        Outcome::Failure,
        "Differs from the manifest protocol",
        [
            Fact::problem(plural(found.mismatches.len(), "difference", "differences")),
            Fact::plain(plural(found.checks, "check", "checks")),
            Fact::plain(origin),
        ],
    ));
    Reply::Report {
        screen,
        data: Json::object(data),
        diagnostics: vec![problem],
        sources: std::collections::BTreeMap::new(),
    }
}
