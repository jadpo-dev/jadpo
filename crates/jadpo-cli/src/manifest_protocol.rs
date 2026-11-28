//! The manifest protocol (area 16, "The manifest protocol"; chunk O3):
//! how a monitoring product is given the manifest each build's telemetry
//! decodes against, so that every product reads `jadpo.build`,
//! `jadpo.identity` and `jadpo.source` the same way.
//!
//! A product serves two requests below its telemetry endpoint, read with
//! the credential the telemetry is sent with:
//!
//! - `PUT jadpo/v1/builds/{build}`, the build's manifest as its body,
//!   answers `201` with `{ "build": … }` when the product didn't hold it
//!   and `200` when it did, keeping the manifest it was first sent;
//! - `GET jadpo/v1/builds/{build}` answers `200` with that manifest.
//!
//! Its failures are in `{ "error": { "code": …, "message": … } }`:
//! `unauthorised` (401), `build_not_found` (404), `too_large` (413, over
//! [`MOST`]) and `manifest_invalid` (422, not this protocol's manifest, or
//! one naming another build). [`check_manifest`] is what `manifest_invalid`
//! means; [`conformance`] is the test a product's CI runs with `jadpo
//! manifest protocol <endpoint>`, against its own endpoint, with a sample
//! build whose id no build has.

use serde_json::{Value, json};

use crate::hq::{self, Address, Answer};

/// Where the requests are, below the endpoint.
pub const PATH: &str = "/jadpo/v1/builds/";

/// The most a manifest may be: far past any app's.
pub const MOST: usize = 16 << 20;

/// The sample build the conformance test sends, an id no build is given:
/// a build id is the first twelve hex digits of a SHA-256.
pub const SAMPLE_BUILD: &str = "b_000000000000";

/// The sample build's manifest, the same in every release, so a product
/// that holds it from an earlier run still conforms.
pub fn sample() -> Value {
    json!({
        "build": SAMPLE_BUILD,
        "compiler": "0.0.0",
        "deployable": "api",
        "nodes": [
            {
                "domain": "planning",
                "file": "domains/planning/actions/complete_todo.jadpo",
                "id": "action complete_todo",
                "kind": "action",
                "lines": [4, 22],
                "name": "complete_todo"
            },
            {
                "domain": "planning",
                "file": "domains/planning/queries/todos_for_owner.jadpo",
                "id": "query todos_for_owner",
                "kind": "query",
                "lines": [3, 9],
                "name": "todos_for_owner"
            }
        ],
        "protocol": 1,
        "target": "native"
    })
}

/// Whether `text` is a build id: `b_` and twelve lower-case hex digits.
pub fn is_build(text: &str) -> bool {
    text.strip_prefix("b_").is_some_and(|digits| {
        digits.len() == 12
            && digits
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Whether `body` is a manifest of this protocol for `build`, or why
/// not: what a product answers `manifest_invalid` for. Members it doesn't
/// know are left alone, since an addition keeps the protocol's number.
pub fn check_manifest(body: &Value, build: &str) -> Result<(), String> {
    if !body.is_object() {
        return Err("It isn't a JSON object.".to_string());
    }
    if body["protocol"].as_i64() != Some(jadpo_graph::manifest::PROTOCOL) {
        return Err(format!(
            "Its `protocol` isn't {}.",
            jadpo_graph::manifest::PROTOCOL
        ));
    }
    match body["build"].as_str() {
        Some(named) if named == build => {}
        Some(named) => {
            return Err(format!("It is `{named}`'s manifest, sent as `{build}`'s."));
        }
        None => return Err("It names no build.".to_string()),
    }
    for member in ["deployable", "compiler"] {
        if !body[member].as_str().is_some_and(|text| !text.is_empty()) {
            return Err(format!("Its `{member}` isn't text."));
        }
    }
    if !matches!(body["target"].as_str(), Some("native" | "wasm")) {
        return Err("Its `target` is neither `native` nor `wasm`.".to_string());
    }
    let Some(nodes) = body["nodes"].as_array() else {
        return Err("It has no `nodes`.".to_string());
    };
    let mut last: Option<&str> = None;
    for node in nodes {
        let Some(id) = node["id"].as_str() else {
            return Err("A node has no `id`.".to_string());
        };
        for member in ["kind", "name", "file"] {
            if !node[member].as_str().is_some_and(|text| !text.is_empty()) {
                return Err(format!("`{id}`'s `{member}` isn't text."));
            }
        }
        if !(node["domain"].is_null() || node["domain"].is_string()) {
            return Err(format!("`{id}`'s `domain` isn't text."));
        }
        let lines = node["lines"].as_array().map(|lines| {
            lines
                .iter()
                .map(Value::as_u64)
                .collect::<Option<Vec<u64>>>()
        });
        match lines {
            Some(Some(lines)) if lines.len() == 2 && 1 <= lines[0] && lines[0] <= lines[1] => {}
            _ => return Err(format!("`{id}`'s `lines` aren't its first and last.")),
        }
        if last.is_some_and(|last| last >= id) {
            return Err(format!("`{id}` is out of key order, or there twice."));
        }
        last = Some(id);
    }
    Ok(())
}

/// One request of the conformance test, and how its answer differed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mismatch {
    /// The request as the test names it, `PUT jadpo/v1/builds/b_000000000000`.
    pub request: String,
    pub problem: String,
}

/// What the test ran: how many checks, and each that failed.
#[derive(Clone, Debug)]
pub struct Conformance {
    pub checks: usize,
    pub mismatches: Vec<Mismatch>,
}

/// The conformance test: the protocol's requests against the product at
/// `address`, with `headers` as its credential. With a credential, a
/// request without it must be refused. A product that can't be reached
/// fails at the first request, and nothing more is sent.
pub(crate) fn conformance(address: &Address, headers: &[(String, String)]) -> Conformance {
    let sample = sample();
    let changed = {
        let mut changed = sample.clone();
        if let Some(nodes) = changed["nodes"].as_array_mut() {
            // In key order, after `action complete_todo`.
            nodes.insert(
                1,
                json!({
                    "file": "domains/planning/actions/start_todo.jadpo",
                    "id": "action start_todo",
                    "kind": "action",
                    "lines": [1, 8],
                    "name": "start_todo"
                }),
            );
        }
        changed
    };
    let named_elsewhere = "b_000000000001";
    let not_a_manifest = "b_000000000002";
    let unknown = "b_ffffffffffff";
    let mut runner = Runner {
        address,
        checks: 0,
        mismatches: Vec::new(),
    };
    let put = |build: &str| format!("PUT jadpo/v1/builds/{build}");
    let get = |build: &str| format!("GET jadpo/v1/builds/{build}");
    let kept = |answer: &Answer| -> Option<String> {
        expect(answer, &[200], None).or_else(|| {
            let held: Value = serde_json::from_str(&answer.body).unwrap_or(Value::Null);
            (held != sample).then(|| {
                "answers a manifest other than the one it was first sent: a build's first manifest is the one kept".to_string()
            })
        })
    };
    let reached = runner.run(
        put(SAMPLE_BUILD),
        "PUT",
        SAMPLE_BUILD,
        headers,
        Some(&sample),
        &|answer| expect(answer, &[201, 200], None).or_else(|| named(answer, SAMPLE_BUILD)),
    );
    if !reached {
        return runner.done();
    }
    runner.run(
        format!("{} again", put(SAMPLE_BUILD)),
        "PUT",
        SAMPLE_BUILD,
        headers,
        Some(&sample),
        &|answer| expect(answer, &[200], None).or_else(|| named(answer, SAMPLE_BUILD)),
    );
    runner.run(get(SAMPLE_BUILD), "GET", SAMPLE_BUILD, headers, None, &kept);
    runner.run(
        format!("{} with another manifest", put(SAMPLE_BUILD)),
        "PUT",
        SAMPLE_BUILD,
        headers,
        Some(&changed),
        &|answer| {
            expect(answer, &[200], None).or_else(|| {
                let after = hq::request(
                    address,
                    "GET",
                    &format!("{PATH}{SAMPLE_BUILD}"),
                    headers,
                    None,
                );
                match after {
                    Ok(after) => kept(&after).map(|_| {
                        "replaced the manifest it held: a build's first manifest is the one kept"
                            .to_string()
                    }),
                    Err(why) => Some(why),
                }
            })
        },
    );
    runner.run(
        format!("{} naming another build", put(named_elsewhere)),
        "PUT",
        named_elsewhere,
        headers,
        Some(&sample),
        &|answer| expect(answer, &[422], Some("manifest_invalid")),
    );
    runner.run(
        format!("{} that isn't a manifest", put(not_a_manifest)),
        "PUT",
        not_a_manifest,
        headers,
        Some(&json!({ "protocol": jadpo_graph::manifest::PROTOCOL })),
        &|answer| expect(answer, &[422], Some("manifest_invalid")),
    );
    runner.run(get(unknown), "GET", unknown, headers, None, &|answer| {
        expect(answer, &[404], Some("build_not_found"))
    });
    if !headers.is_empty() {
        runner.run(
            format!("{} without the credential", get(SAMPLE_BUILD)),
            "GET",
            SAMPLE_BUILD,
            &[],
            None,
            &|answer| expect(answer, &[401], Some("unauthorised")),
        );
    }
    runner.done()
}

/// Runs the test's requests one at a time, counting them and keeping
/// how each differed.
struct Runner<'a> {
    address: &'a Address,
    checks: usize,
    mismatches: Vec<Mismatch>,
}

impl Runner<'_> {
    /// Sends one request and judges its answer; `false` when nothing
    /// answered.
    fn run(
        &mut self,
        name: String,
        method: &str,
        build: &str,
        headers: &[(String, String)],
        body: Option<&Value>,
        judge: &dyn Fn(&Answer) -> Option<String>,
    ) -> bool {
        self.checks += 1;
        let text = body.map(Value::to_string);
        match hq::request(
            self.address,
            method,
            &format!("{PATH}{build}"),
            headers,
            text.as_deref(),
        ) {
            Ok(answer) => {
                if let Some(problem) = judge(&answer) {
                    self.mismatches.push(Mismatch {
                        request: name,
                        problem,
                    });
                }
                true
            }
            Err(why) => {
                self.mismatches.push(Mismatch {
                    request: name,
                    problem: why,
                });
                false
            }
        }
    }

    fn done(self) -> Conformance {
        Conformance {
            checks: self.checks,
            mismatches: self.mismatches,
        }
    }
}

/// How `answer` differs from one of `statuses`, with the failure `code`
/// in the envelope when one is expected.
fn expect(answer: &Answer, statuses: &[u16], code: Option<&str>) -> Option<String> {
    let said: Value = serde_json::from_str(&answer.body).unwrap_or(Value::Null);
    let wanted = statuses
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(" or ");
    let wanted = match code {
        Some(code) => format!("{wanted} `{code}`"),
        None => wanted,
    };
    if !statuses.contains(&answer.status) {
        let got = match said["error"]["code"].as_str() {
            Some(code) => format!("{} `{code}`", answer.status),
            None => answer.status.to_string(),
        };
        return Some(format!("answered {got}, not {wanted}"));
    }
    if let Some(code) = code
        && said["error"]["code"].as_str() != Some(code)
    {
        return Some(format!(
            "answered {} without `{code}` in the failure envelope",
            answer.status
        ));
    }
    None
}

/// How an answer to a `PUT` differs from naming `build`.
fn named(answer: &Answer, build: &str) -> Option<String> {
    let said: Value = serde_json::from_str(&answer.body).unwrap_or(Value::Null);
    (said["build"].as_str() != Some(build)).then(|| {
        format!(
            "answered {} without `{{ \"build\": \"{build}\" }}`",
            answer.status
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_is_a_manifest_and_a_broken_one_says_why() {
        assert_eq!(check_manifest(&sample(), SAMPLE_BUILD), Ok(()));
        assert!(is_build(SAMPLE_BUILD) && !is_build("b_00000000000g") && !is_build("b_0"));
        assert_eq!(
            check_manifest(&sample(), "b_000000000001"),
            Err("It is `b_000000000000`'s manifest, sent as `b_000000000001`'s.".to_string())
        );
        let mut unordered = sample();
        unordered["nodes"].as_array_mut().expect("nodes").reverse();
        assert!(
            check_manifest(&unordered, SAMPLE_BUILD)
                .expect_err("out of order")
                .contains("key order")
        );
        let mut added = sample();
        added["paths"] = json!([]);
        assert_eq!(
            check_manifest(&added, SAMPLE_BUILD),
            Ok(()),
            "an addition keeps the protocol"
        );
        assert!(check_manifest(&json!({ "protocol": 1 }), SAMPLE_BUILD).is_err());
    }
}
