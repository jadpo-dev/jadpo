//! `jadpo package protocol <document>` (area 14; area 11, "The registry
//! protocol"): the conformance test of a registry application's OpenAPI
//! document against the registry protocol's table, so its CI runs the
//! `jadpo` it targets and needs no Rust. The document is a file, an
//! `https://` address, or an `http://` address on this machine, and the
//! test is `jadpo_registry::conformance::check`.
//!
//! A document that conforms is one line (docs/cli-design-language.md,
//! "Calm by default"):
//!
//! ```text
//! ✓ Conforms to the registry protocol  build/openapi/main.json · 10 requests
//! ```
//!
//! One that differs is one error, then each difference under the request
//! it is in, in the protocol's order, so the list reads as what to change,
//! then the footer. It exits 1, which is what stops the CI.
//!
//! ```text
//! ✗ error  `build/openapi/main.json` differs from the registry protocol
//!   reason  The `jadpo` tool is built against the protocol's table, ...
//!   next    Change the routes each line below names, then run this again (agent)
//!   rule    protocol.mismatch
//!
//! GET /registry/v1/log/proof
//! ✗ doesn't say it fails with `size_past_head` (422) in the failure envelope
//!
//! GET /registry/v1/search
//! ✗ answer.results: missing
//! ✗ answer.items: not in the protocol
//!
//! ✗ Differs from the registry protocol  3 differences · 2 requests · build/openapi/main.json
//! ```

use std::path::PathBuf;

use jadpo_diag::{Diagnostic, rule};
use jadpo_registry::conformance::{self, Mismatch};
use jadpo_registry::protocol::ENDPOINTS;
use jadpo_term::{Block, Fact, Json, Outcome, Row, RowState, Screen};

use crate::package::{arguments, plural};
use crate::run::{Call, Reply};

/// The most of a document that is fetched: far past any API's.
const MOST: usize = 64 << 20;

const HOLDS: &str = "The `jadpo` tool is built against the registry protocol's table, so a registry that differs from it can't serve `jadpo package add` or `jadpo package publish`.";

pub fn protocol(call: &Call) -> Reply {
    let Some(document) = call.arguments.get(1) else {
        return Reply::failed(arguments(
            "`jadpo package protocol` needs a document".to_string(),
            "It checks a registry application's OpenAPI document against the registry protocol: a file such as `build/openapi/main.json`, or the address it is served at.".to_string(),
            "Run `jadpo package protocol build/openapi/main.json`".to_string(),
        ));
    };
    if let Some(extra) = call.arguments.get(2) {
        return Reply::failed(arguments(
            format!("`jadpo package protocol` takes one document, not `{extra}` as well"),
            "It checks one registry application's document at a time.".to_string(),
            format!("Run `jadpo package protocol {document}`"),
        ));
    }
    let read = match read(call, document) {
        Ok(read) => read,
        Err(why) => return Reply::failed(unreadable(document, &why)),
    };
    report(document, &conformance::check(&read))
}

/// The document's JSON, or why it can't be read.
fn read(call: &Call, document: &str) -> Result<serde_json::Value, String> {
    let bytes = if document.contains("://") {
        crate::download::get_all(document, MOST)?
    } else {
        let cwd = call.cwd.clone().unwrap_or_else(|| PathBuf::from("."));
        std::fs::read(cwd.join(document)).map_err(|error| format!("{error}."))?
    };
    conformance::parse(&bytes)
}

fn unreadable(document: &str, why: &str) -> Diagnostic {
    let next = if why.contains("401") || why.contains("403") {
        "Give the API `authentication: []` so anyone may read its document, or give the file `jadpo check` writes, `build/openapi/<api>.json`"
    } else {
        "Give the file `jadpo check` writes, `build/openapi/<api>.json`, or the address its API serves it at"
    };
    Diagnostic::new(
        &rule::PROTOCOL_UNREADABLE,
        format!("`{document}` can't be read as an OpenAPI document"),
    )
    .reason(why.to_string())
    .next(next)
}

fn report(document: &str, found: &[Mismatch]) -> Reply {
    let total = ENDPOINTS.len();
    let data = [
        ("document", Json::from(document)),
        ("conforms", found.is_empty().into()),
        ("requests", Json::from(total)),
        (
            "mismatches",
            Json::array(found.iter().map(|mismatch| {
                Json::object([
                    ("operation", Json::from(mismatch.operation.as_str())),
                    ("problem", Json::from(mismatch.problem.as_str())),
                ])
            })),
        ),
    ];
    if found.is_empty() {
        let screen = Screen::new().with(Block::summary(
            Outcome::Success,
            "Conforms to the registry protocol",
            [
                Fact::plain(document),
                Fact::plain(plural(total, "request", "requests")),
            ],
        ));
        return Reply::screen(screen, Json::object(data));
    }
    let problem = Diagnostic::new(
        &rule::PROTOCOL_MISMATCH,
        format!("`{document}` differs from the registry protocol"),
    )
    .reason(HOLDS)
    .next("Change the routes each line below names, then run this again");
    let mut screen = Screen::new().with(Block::diagnostic_from(
        &problem,
        &std::collections::BTreeMap::new(),
        None,
    ));
    // Each request once, in the order `check` reached it: the protocol's.
    let mut requests: Vec<&str> = Vec::new();
    for mismatch in found {
        if !requests.contains(&mismatch.operation.as_str()) {
            requests.push(&mismatch.operation);
        }
    }
    for request in &requests {
        let rows = found
            .iter()
            .filter(|mismatch| mismatch.operation == *request)
            .map(|mismatch| Row::new(mismatch.problem.clone(), "").state(RowState::Failed))
            .collect();
        screen.push(Block::list(*request, rows));
    }
    let mut facts = vec![Fact::problem(plural(
        found.len(),
        "difference",
        "differences",
    ))];
    // `document` heads what is wrong with the whole, which isn't a request.
    let differing = requests.iter().filter(|r| **r != "document").count();
    if differing > 0 {
        facts.push(Fact::plain(plural(differing, "request", "requests")));
    }
    facts.push(Fact::plain(document));
    screen.push(Block::summary(
        Outcome::Failure,
        "Differs from the registry protocol",
        facts,
    ));
    Reply::Report {
        screen,
        data: Json::object(data),
        diagnostics: vec![problem],
        sources: std::collections::BTreeMap::new(),
    }
}
