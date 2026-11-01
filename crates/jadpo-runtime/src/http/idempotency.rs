//! Idempotent writes (area 7, "Idempotent writes"): every route but a `GET`
//! honours an `Idempotency-Key` a client sends, entirely in the runtime, so
//! the action never sees it. Keys live in the `jadpo_idempotency_keys`
//! system table of the store the action writes (area 5, "System tables").
//!
//! - Before the action runs, the key is claimed for the caller in one
//!   statement of its own ([`CLAIM`], run with `Database::query_once`): a
//!   row with no response yet, kept only as long as the route's deadline
//!   (and a margin), so a repeat that arrives meanwhile is told the first
//!   is still running (409 `conflict`), and a claim a crashed process left
//!   behind lapses by itself. The same statement gives back what the key is
//!   already kept with when it can't claim it.
//! - A key already kept answers instead of the action: the stored response
//!   when the request's digest matches, 422 `idempotency_key_reused` when
//!   it doesn't.
//! - After the action, its response is written over the claim, kept for 24
//!   hours: a success in the action's own transaction, before it commits;
//!   a failure the action returned (whose writes were rolled back) in one
//!   statement of its own. `unavailable`, a fault or a passed deadline
//!   stores nothing and drops the claim, in one statement of its own, so a
//!   retry runs again.
//! - A lookup ignores a row past its `kept_until`; deleting such rows comes
//!   with the runtime's minute tick (area 12, roadmap chunk 6.2).

use jadpo_std::http::{IDEMPOTENCY_KEEP_MS, IDEMPOTENCY_KEY_MAX, RoutePlan, Segment};
use jadpo_value::Value;
use jadpo_value::json::Codecs;

use super::{Request, Response};
use crate::{Ctx, SqlValue};

/// The header a client sends its key in.
pub const HEADER: &str = "idempotency-key";

/// How long past its route's deadline a claim is kept, for the commit that
/// follows the action.
const CLAIM_MARGIN_MS: i64 = 5_000;

/// The scope a request no one signed in to keeps its keys under: every
/// such caller is the same anonymous one (area 8 keeps a signed-in
/// caller's under `user <id>` or `service <id>`).
pub const ANONYMOUS: &str = "anonymous";

/// A request's key, as the runtime keeps it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Guard {
    /// The store the action writes, which keeps the key.
    pub store: String,
    /// Whose key it is.
    pub scope: String,
    pub key: String,
    /// The request's digest, which a repeat must match.
    pub digest: String,
    /// When the request started, in milliseconds since the epoch: the
    /// claim's `stored_at`, which names this request's claim.
    pub at: i64,
}

/// The key a request sends: `Ok(None)` without one, `Err(())` when it
/// isn't 1 to 255 printable ASCII characters.
pub(crate) fn key(request: &Request) -> Result<Option<String>, ()> {
    let Some(key) = request.header(HEADER) else {
        return Ok(None);
    };
    let valid = !key.is_empty()
        && key.len() <= IDEMPOTENCY_KEY_MAX
        && key
            .bytes()
            .all(|byte| byte.is_ascii_graphic() || byte == b' ');
    if valid {
        Ok(Some(key.to_string()))
    } else {
        Err(())
    }
}

/// The request's digest, which a repeat must match: its route's key (the
/// method and the path as declared, one per route), then each placeholder
/// and its input (the query string, the headers it reads, the body) as the
/// route decoded them, each written back through its codec, the contract's
/// own JSON. That JSON writes a secret value as its kind alone
/// (`<redacted Password>`), so whoever reads the keys table can't test a
/// guessed password or token against a digest (chunk 5.8a): a repeat that
/// differs only in a secret is the same request.
pub fn digest(
    codecs: &Codecs<'_>,
    route: &RoutePlan,
    params: &[Option<Value>],
    input: Option<&Value>,
) -> String {
    let mut hasher = blake3::Hasher::new();
    let mut part = |bytes: &[u8]| {
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    };
    part(route.key.as_bytes());
    let placeholders = route.segments.iter().filter_map(|segment| match segment {
        Segment::Param { codec, .. } => Some(*codec),
        Segment::Fixed(_) => None,
    });
    let written = |codec, value: Option<&Value>| {
        let mut out = String::new();
        if let Some(value) = value {
            codecs.write(codec, value, &mut out);
        }
        out
    };
    for (codec, value) in placeholders.zip(params) {
        part(written(codec, value.as_ref()).as_bytes());
    }
    if let Some(plan) = &route.input {
        part(written(plan.codec, input).as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// What claiming a key found.
#[derive(Debug)]
pub enum Claim {
    /// The key is this request's now: run the action.
    Claimed,
    /// The key was kept with this request's response: answer with it.
    Replay(Box<Response>),
    /// The key was kept by a route that waits for its follow-on work
    /// (area 7, `settle:`; chunk 6.3), with the arguments of the query it
    /// answers with, as JSON, and the status it answered: run the query
    /// again rather than the action, showing how far the work has got.
    Answer { status: u16, args: String },
    /// The key was kept with a different request.
    Reused,
    /// A request with the key is still running.
    InFlight,
    /// The store couldn't be reached.
    Unavailable,
}

/// Claims the key, or, when another request holds it or kept a response
/// with it, gives back what it is kept with: a first column `true` when
/// this request claimed it, otherwise `false` and the kept row's digest,
/// status and response. A row past its `kept_until` is claimed over. All
/// of it reads the one snapshot the statement starts with, so a claim
/// another request made after that is found as neither, which is a request
/// still running.
pub(crate) const CLAIM: &str = "WITH \"claimed\" AS (INSERT INTO \"jadpo_idempotency_keys\" (\"scope\", \"key\", \"request_digest\", \"status\", \"response\", \"stored_at\", \"kept_until\") VALUES ($1, $2, $3, NULL, NULL, $4, $5) ON CONFLICT (\"scope\", \"key\") DO UPDATE SET \"request_digest\" = EXCLUDED.\"request_digest\", \"status\" = NULL, \"response\" = NULL, \"stored_at\" = EXCLUDED.\"stored_at\", \"kept_until\" = EXCLUDED.\"kept_until\" WHERE \"jadpo_idempotency_keys\".\"kept_until\" <= EXCLUDED.\"stored_at\" RETURNING \"scope\") SELECT TRUE, NULL::text, NULL::smallint, NULL::jsonb FROM \"claimed\" UNION ALL SELECT FALSE, \"request_digest\", \"status\", \"response\" FROM \"jadpo_idempotency_keys\" WHERE \"scope\" = $1 AND \"key\" = $2 AND \"kept_until\" > $4 AND NOT EXISTS (SELECT 1 FROM \"claimed\")";

/// Writes the response over this request's claim, giving back the row it
/// wrote, so on its own it says whether the claim was still this
/// request's.
pub const STORE: &str = "UPDATE \"jadpo_idempotency_keys\" SET \"status\" = $4, \"response\" = $5, \"kept_until\" = $6 WHERE \"scope\" = $1 AND \"key\" = $2 AND \"stored_at\" = $3 AND \"status\" IS NULL RETURNING \"scope\"";

pub(crate) const RELEASE: &str = "DELETE FROM \"jadpo_idempotency_keys\" WHERE \"scope\" = $1 AND \"key\" = $2 AND \"stored_at\" = $3 AND \"status\" IS NULL";

fn micros(millis: i64) -> SqlValue {
    SqlValue::Timestamp(millis.saturating_mul(1000))
}

/// Claims `guard`'s key for a request whose route's deadline is
/// `deadline_ms`, or finds what it is already kept with, in one statement.
pub async fn claim(ctx: &mut Ctx<'_>, guard: &Guard, deadline_ms: u32) -> Claim {
    let Some(database) = ctx.host().database(&guard.store) else {
        return Claim::Unavailable;
    };
    let lease = guard
        .at
        .saturating_add(i64::from(deadline_ms))
        .saturating_add(CLAIM_MARGIN_MS);
    let found = database
        .query_once(
            CLAIM,
            &[
                SqlValue::Text(guard.scope.clone()),
                SqlValue::Text(guard.key.clone()),
                SqlValue::Text(guard.digest.clone()),
                micros(guard.at),
                micros(lease),
            ],
        )
        .await;
    let Ok(rows) = found else {
        return Claim::Unavailable;
    };
    match rows.first().map(Vec::as_slice) {
        Some([SqlValue::Bool(true), ..]) => Claim::Claimed,
        Some(
            [
                SqlValue::Bool(false),
                SqlValue::Text(digest),
                status,
                response,
            ],
        ) => {
            if *digest != guard.digest {
                Claim::Reused
            } else if let Some(answer) = answer(status, response) {
                answer
            } else {
                replay(status, response).map_or(Claim::InFlight, |response| {
                    Claim::Replay(Box::new(response))
                })
            }
        }
        // Claimed by a request the statement's snapshot doesn't see yet:
        // still running, as far as this request can tell.
        _ => Claim::InFlight,
    }
}

/// A kept answer of a route that waits for its follow-on work: its
/// status and its query's arguments; `None` for any other row.
fn answer(status: &SqlValue, response: &SqlValue) -> Option<Claim> {
    let status = match status {
        SqlValue::Int2(status) => u16::try_from(*status).ok()?,
        _ => return None,
    };
    let SqlValue::Json(kept) = response else {
        return None;
    };
    let kept: serde_json::Value = serde_json::from_str(kept).ok()?;
    let args = kept.get("answer").and_then(serde_json::Value::as_str)?;
    Some(Claim::Answer {
        status,
        args: args.to_string(),
    })
}

/// A kept response, or `None` while its request is still running.
fn replay(status: &SqlValue, response: &SqlValue) -> Option<Response> {
    let status = match status {
        SqlValue::Int2(status) => u16::try_from(*status).ok()?,
        _ => return None,
    };
    let SqlValue::Json(kept) = response else {
        return None;
    };
    let kept: serde_json::Value = serde_json::from_str(kept).ok()?;
    let body = kept.get("body").and_then(serde_json::Value::as_str)?;
    let code = kept
        .get("code")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let headers = if body.is_empty() {
        Vec::new()
    } else {
        vec![("content-type".to_string(), "application/json".to_string())]
    };
    Some(Response {
        status,
        headers,
        body: body.as_bytes().to_vec(),
        code,
        ..Response::default()
    })
}

/// The parameters that write `response` over `guard`'s claim, with
/// [`STORE`].
pub fn store_params(guard: &Guard, response: &Response) -> Vec<SqlValue> {
    let mut kept = serde_json::Map::new();
    kept.insert(
        "code".to_string(),
        response
            .code
            .clone()
            .map_or(serde_json::Value::Null, serde_json::Value::String),
    );
    kept.insert(
        "body".to_string(),
        serde_json::Value::String(String::from_utf8_lossy(&response.body).into_owned()),
    );
    vec![
        SqlValue::Text(guard.scope.clone()),
        SqlValue::Text(guard.key.clone()),
        micros(guard.at),
        SqlValue::Int2(i16::try_from(response.status).unwrap_or(i16::MAX)),
        SqlValue::Json(serde_json::Value::Object(kept).to_string()),
        micros(guard.at.saturating_add(IDEMPOTENCY_KEEP_MS)),
    ]
}

/// The parameters that keep, over `guard`'s claim, what a route that
/// waits for its follow-on work answers a repeat with (chunk 6.3): the
/// arguments of the query it answers with, `args`, as JSON, and `status`,
/// 202 until the work is known to have settled ([`SETTLED`]), so a
/// process that stops between the commit and the answer never has a
/// repeat say the work finished.
pub fn answer_params(guard: &Guard, status: u16, args: String) -> Vec<SqlValue> {
    let mut kept = serde_json::Map::new();
    kept.insert("code".to_string(), serde_json::Value::Null);
    kept.insert("body".to_string(), serde_json::Value::String(String::new()));
    kept.insert("answer".to_string(), serde_json::Value::String(args));
    vec![
        SqlValue::Text(guard.scope.clone()),
        SqlValue::Text(guard.key.clone()),
        micros(guard.at),
        SqlValue::Int2(i16::try_from(status).unwrap_or(i16::MAX)),
        SqlValue::Json(serde_json::Value::Object(kept).to_string()),
        micros(guard.at.saturating_add(IDEMPOTENCY_KEEP_MS)),
    ]
}

/// Sets the status `$4` a kept answer repeats (chunk 6.3), once the work
/// its request set off has settled.
pub(crate) const SETTLED: &str = "UPDATE \"jadpo_idempotency_keys\" SET \"status\" = $4 WHERE \"scope\" = $1 AND \"key\" = $2 AND \"stored_at\" = $3 AND \"response\" -> 'answer' IS NOT NULL RETURNING \"scope\"";

/// Keeps `status` as what `guard`'s answer repeats with, in one statement
/// of its own. Best effort: a kept answer that can't be changed repeats
/// 202, which never says the work finished when it may not have.
pub async fn settled(ctx: &mut Ctx<'_>, guard: &Guard, status: u16) {
    let Some(database) = ctx.host().database(&guard.store) else {
        return;
    };
    let _ = database
        .query_once(
            SETTLED,
            &[
                SqlValue::Text(guard.scope.clone()),
                SqlValue::Text(guard.key.clone()),
                micros(guard.at),
                SqlValue::Int2(i16::try_from(status).unwrap_or(i16::MAX)),
            ],
        )
        .await;
}

/// Drops `guard`'s claim, so a retry runs the action again, in one
/// statement of its own. Best effort: a claim that can't be dropped lapses
/// with its deadline.
pub async fn release(ctx: &mut Ctx<'_>, guard: &Guard) {
    let Some(database) = ctx.host().database(&guard.store) else {
        return;
    };
    let _ = database
        .query_once(
            RELEASE,
            &[
                SqlValue::Text(guard.scope.clone()),
                SqlValue::Text(guard.key.clone()),
                micros(guard.at),
            ],
        )
        .await;
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use jadpo_std::http::{Codec, CodecId, InputPlan, RoutePlan, Run, Segment};
    use jadpo_std::types::{FieldPlan, ShapeId, ShapePlan, ValidatorId};
    use jadpo_std::{Builtin, HttpMethod, SuccessStatus};
    use jadpo_value::json::Codecs;
    use jadpo_value::shape::Shape;
    use jadpo_value::{Record, Value};

    use super::digest;

    const TEXT: CodecId = CodecId(0);
    const FORM: CodecId = CodecId(1);
    const PATCH: CodecId = CodecId(2);
    const TEXTS: CodecId = CodecId(3);

    fn shape(ty: &str, fields: &[&str]) -> Shape {
        Shape::new(&ShapePlan {
            ty: ty.to_string(),
            open: false,
            variant: None,
            fields: fields
                .iter()
                .map(|name| FieldPlan {
                    key: format!("field {ty}.{name}"),
                    name: name.to_string(),
                    validator: ValidatorId(0),
                    omittable: true,
                    absent: Some(jadpo_std::Constant::None),
                    none_left_out: false,
                    section: None,
                    wire: None,
                })
                .collect(),
        })
    }

    /// A route with `params` text placeholders and, given one, an input.
    fn route(key: &str, params: usize, input: Option<CodecId>) -> RoutePlan {
        let mut segments = vec![Segment::Fixed("accounts".into())];
        for at in 0..params {
            segments.push(Segment::Param {
                name: format!("p{at}"),
                codec: TEXT,
            });
        }
        RoutePlan {
            key: key.into(),
            method: HttpMethod::Post,
            segments,
            api: 0,
            input: input.map(|codec| InputPlan {
                codec,
                query: false,
                headers: Vec::new(),
            }),
            output: None,
            status: SuccessStatus::NoContent,
            deadline_ms: 10_000,
            run: Run::Action(0),
            settle: None,
            stream: false,
            keys: Some("main".into()),
            args: Vec::new(),
            authentication: jadpo_std::http::Authentication::Public,
            caller: None,
            confirm: false,
            file: "domains/accounts/routes/accounts.jadpo".into(),
            line: 1,
        }
    }

    /// Runs `check` with the codecs a sign-up form, a patch and a list of
    /// text are read and written with.
    fn with_codecs(check: impl FnOnce(&Codecs<'_>)) {
        let codecs = [
            Codec::Scalar {
                builtin: Builtin::Text,
                validator: ValidatorId(0),
                int_as_number: true,
            },
            Codec::Object {
                shape: ShapeId(0),
                fields: vec![TEXT, TEXT],
                validator: None,
            },
            Codec::Object {
                shape: ShapeId(1),
                fields: vec![TEXT],
                validator: None,
            },
            Codec::List {
                item: TEXT,
                validator: ValidatorId(0),
            },
        ];
        let shapes = [
            shape("SignUpForm", &["handle", "chosen_password"]),
            shape("NotePatch", &["text"]),
        ];
        check(&Codecs {
            codecs: &codecs,
            validators: &[],
            shapes: &shapes,
            coder: None,
        });
    }

    fn form(handle: &str, password: &str) -> Value {
        Value::record(
            "SignUpForm",
            vec![
                ("handle".into(), Value::text(handle)),
                (
                    "chosen_password".into(),
                    Value::secret("Password", password),
                ),
            ],
        )
    }

    /// Whoever reads the keys table can't test a password guess against a
    /// sign-up's digest: whatever the password, it is the same (chunk
    /// 5.8a). So is a secret in a placeholder.
    #[test]
    fn a_digest_leaves_out_every_secret_value() {
        with_codecs(|codecs| {
            let sign_up = route("route POST /accounts/sign-up", 0, Some(FORM));
            let of = |input: &Value| digest(codecs, &sign_up, &[], Some(input));
            assert_eq!(
                of(&form("ann", "hunter2hunter2")),
                of(&form("ann", "correct horse battery"))
            );
            let reset = route("route POST /accounts/{p0}", 1, None);
            let token = |text: &str| [Some(Value::secret("BearerToken", text))];
            assert_eq!(
                digest(codecs, &reset, &token("abc"), None),
                digest(codecs, &reset, &token("xyz"), None)
            );
        });
    }

    /// Everything else a request says counts, so a repeat must send the
    /// same request.
    #[test]
    fn a_digest_covers_the_route_and_everything_else_the_request_says() {
        with_codecs(|codecs| {
            let sign_up = route("route POST /accounts/sign-up", 0, Some(FORM));
            let of = |input: &Value| digest(codecs, &sign_up, &[], Some(input));
            assert_ne!(of(&form("ann", "x")), of(&form("bo", "x")));
            let sign_in = route("route POST /accounts/sign-in", 0, Some(FORM));
            assert_ne!(
                of(&form("ann", "x")),
                digest(codecs, &sign_in, &[], Some(&form("ann", "x")))
            );
            let named = route("route POST /accounts/{p0}", 1, None);
            let name = |text: &str| [Some(Value::text(text))];
            assert_ne!(
                digest(codecs, &named, &name("a"), None),
                digest(codecs, &named, &name("b"), None)
            );
            // Parts are told apart, not run together.
            let listed = route("route POST /accounts/lists", 0, Some(TEXTS));
            let texts =
                |items: &[&str]| Value::list(items.iter().map(|text| Value::text(text)).collect());
            assert_ne!(
                digest(codecs, &listed, &[], Some(&texts(&["ab", "c"]))),
                digest(codecs, &listed, &[], Some(&texts(&["a", "bc"])))
            );
            // A field left out isn't one sent as `none`: a patch tells them
            // apart.
            let patched = route("route POST /accounts/notes", 0, Some(PATCH));
            let patch = |omitted: Option<Rc<[u32]>>| {
                Value::Record(Rc::new(Record {
                    ty: Rc::from("NotePatch"),
                    fields: vec![(Rc::from("text"), Value::None)],
                    omitted,
                }))
            };
            assert_ne!(
                digest(codecs, &patched, &[], Some(&patch(None))),
                digest(
                    codecs,
                    &patched,
                    &[],
                    Some(&patch(Some(Rc::from(&[0u32][..]))))
                )
            );
        });
    }
}
