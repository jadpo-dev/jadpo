//! The provider plan (area 11, "Calling"; chunk 7.2c): each connection to
//! a provider package, read from `app.jadpo` and the package's
//! `provider.jadpo`, and each operation of its package, its input
//! and output numbered in the plan's JSON table so the runtime writes a
//! request and reads an answer with the codecs `json.read` and
//! `json.write` use, a provider package's field and variant spellings on
//! the wire included.

use jadpo_check::lookups::Lookup;
use jadpo_db::{Db, Project};
use jadpo_graph::{Key, TypeRef};
use jadpo_std::provider::{
    Address, AwsPlan, ConnectionPlan, CredentialScheme, DEFAULT_TIMEOUT_MS, IdempotencyPlan,
    OperationPlan, PathPart, Protocol, ProviderPlan, RecordedPlan, WebhookEvent, WebhookPlan,
};
use jadpo_std::receiving::{AcknowledgePlan, Mapped, QueueEvent, QueuePlan, ReceivePlan};
use jadpo_std::subscriber::Repeat;

/// The provider plan, numbering each operation's input and output in
/// `json_types` (the plan's JSON table's types), and reading each setting
/// through `setting`.
pub(crate) fn plan(
    db: &dyn Db,
    project: Project,
    types: &mut crate::types::Types,
    setting: &dyn Fn(&str) -> Option<u32>,
    json_types: &mut Vec<TypeRef>,
) -> ProviderPlan {
    let lookup = Lookup::new(db, project);
    let operations = jadpo_check::providers::operations(db, project);
    let mut number = |ty: TypeRef| -> u32 {
        let at = match json_types.iter().position(|known| *known == ty) {
            Some(at) => at,
            None => {
                json_types.push(ty);
                json_types.len() - 1
            }
        };
        u32::try_from(at).unwrap_or(u32::MAX)
    };
    let mut out = ProviderPlan {
        application: jadpo_check::routes::openapi::title(db, project),
        ..ProviderPlan::default()
    };
    for connection in &jadpo_check::connections::package_connections(db, project).connections {
        let reach = &connection.reach;
        let address = if reach.hosts {
            match connection.environment.as_deref().and_then(setting) {
                Some(environment) => Address::Hosts {
                    live: reach.live.clone(),
                    sandbox: reach.sandbox.clone(),
                    environment,
                },
                None => Address::Unknown,
            }
        } else {
            connection
                .host
                .as_deref()
                .and_then(setting)
                .map_or(Address::Unknown, Address::Setting)
        };
        let credential = match (
            reach.credential.as_deref(),
            connection.credential.as_deref(),
        ) {
            (Some(scheme), Some(name)) => {
                let scheme = match scheme {
                    "bearer" => Some(CredentialScheme::Bearer),
                    "basic" => Some(CredentialScheme::Basic),
                    "aws_sigv4" => connection
                        .key_id
                        .as_deref()
                        .and_then(setting)
                        .map(|key_id| CredentialScheme::AwsSigV4 { key_id }),
                    _ => None,
                };
                scheme.zip(setting(name))
            }
            _ => None,
        };
        let idempotency = reach.idempotency.as_ref().map(|given| IdempotencyPlan {
            header: given.header.clone(),
            max_length: given.max_length,
            in_use: given.in_use.clone(),
            reused: given.reused.clone(),
        });
        let keyed = idempotency
            .as_ref()
            .is_some_and(|plan| !plan.header.is_empty());
        let at = u32::try_from(out.connections.len()).unwrap_or(u32::MAX);
        out.connections.push(ConnectionPlan {
            name: connection.name.clone(),
            package: connection.package.clone(),
            address,
            credential,
            protocol: match reach.protocol.as_deref() {
                Some("aws_json") => Protocol::AwsJson,
                _ => Protocol::JsonHttp,
            },
            aws: reach.aws.as_ref().and_then(|aws| {
                Some(AwsPlan {
                    service: aws.service.clone()?,
                    target: aws.target.clone(),
                })
            }),
            timeout_ms: connection.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS),
            retry: connection.retry.unwrap_or_default(),
            idempotency,
            error_code: reach.error_code.clone(),
            sandbox: jadpo_check::connections::sandboxed(db, project, connection),
        });
        // Every operation, public or not: a package's own actions call
        // the ones it keeps to itself (chunk 7.6).
        for operation in operations.of(&connection.folder) {
            let mut planned = operation_plan(&lookup, types, &mut number, operation, at, keyed);
            planned.recording = recording(db, project, operation);
            out.operations.push(planned);
        }
    }
    // Every recording of the app's own integrations, decoded by `jadpo
    // test` and the publishing bar with its operation's decoders (chunk
    // 7.2d2). An installed package's were decoded when it was published.
    for record in project.records(db) {
        let Some((home, rest)) = jadpo_db::package_at(&record.path) else {
            continue;
        };
        let (false, ["recordings", file]) = (home.installed, rest.as_slice()) else {
            continue;
        };
        let Some(full) = file.strip_suffix(".json") else {
            continue;
        };
        let Ok(text) = &record.text else {
            continue;
        };
        let found = full
            .split_once('.')
            .and_then(|(resource, name)| operations.find(&home.folder, resource, name));
        out.recorded.push(RecordedPlan {
            file: record.path.clone(),
            package: home.versioned(),
            operation: full.to_string(),
            // Only its answer is read, so how it would be sent again
            // doesn't matter.
            plan: found.map(|operation| {
                operation_plan(&lookup, types, &mut number, operation, u32::MAX, false)
            }),
            text: text.clone(),
        });
    }
    // Each connection's webhooks (chunk 7.7): the body is read with its
    // event's codec, numbered here, and its payload kept with it.
    for route in &jadpo_check::webhooks::webhooks(db, project).routes {
        let (Some(scheme), Some(secret)) = (route.scheme(), setting(&route.secret)) else {
            continue;
        };
        out.webhooks.push(WebhookPlan {
            connection: route.connection.clone(),
            segments: route.segments.clone(),
            scheme,
            secret,
            event_id: route.event_id.clone(),
            event_type: route.event_type.clone(),
            occurred_at: route.occurred_at.clone(),
            store: route.store.clone(),
            events: route
                .events
                .iter()
                .map(|event| WebhookEvent {
                    wire_name: event.wire_name.clone(),
                    key: event.key.as_str().to_string(),
                    number: number(TypeRef::Node(event.key.clone())),
                })
                .collect(),
        });
    }
    // Each queue `receiving:` names (chunk 7.7c): a message's body is read
    // with its event's codec, numbered here, and its payload kept with it.
    for queue in &jadpo_check::receiving::receiving(db, project).queues {
        let inputs =
            |given: &[(String, jadpo_check::receiving::Input)]| -> Option<Vec<(String, Mapped)>> {
                given
                    .iter()
                    .map(|(member, input)| {
                        use jadpo_check::receiving::Input;
                        let mapped = match input {
                            Input::Config(name) => Mapped::Setting(setting(name)?),
                            Input::Text(text) => Mapped::Text(text.clone()),
                            Input::Int(n) => Mapped::Int(*n),
                            Input::Bool(flag) => Mapped::Bool(*flag),
                            Input::Handle => Mapped::Handle,
                        };
                        Some((member.clone(), mapped))
                    })
                    .collect()
            };
        let (Some(receive), Some(acknowledge)) =
            (inputs(&queue.receive.1), inputs(&queue.acknowledge.1))
        else {
            continue;
        };
        out.queues.push(QueuePlan {
            connection: queue.connection.clone(),
            type_at: queue.type_at.clone(),
            id_at: queue.id_at.clone(),
            events: queue
                .events
                .iter()
                .map(|event| QueueEvent {
                    wire_name: event.wire_name.clone(),
                    key: event.key.as_str().to_string(),
                    number: number(TypeRef::Node(event.key.clone())),
                })
                .collect(),
            store: queue.store.clone(),
            receive: ReceivePlan {
                operation: queue.receive.0.clone(),
                input: receive,
                messages: queue.messages.clone(),
                body: queue.body.clone(),
                handle: queue.handle.clone(),
            },
            acknowledge: AcknowledgePlan {
                operation: queue.acknowledge.0.clone(),
                input: acknowledge,
            },
        });
    }
    out
}

/// Operation `operation` as a call through connection number `at` sends
/// it, its types numbered by `number`; `keyed` when its provider takes an
/// idempotency key.
fn operation_plan(
    lookup: &Lookup<'_>,
    types: &mut crate::types::Types,
    number: &mut dyn FnMut(TypeRef) -> u32,
    operation: &jadpo_check::providers::Operation,
    at: u32,
    keyed: bool,
) -> OperationPlan {
    let input = operation
        .input
        .as_ref()
        .map(|(ty, _)| number(jadpo_check::graph::type_ref(lookup, ty)));
    let output = match &operation.output {
        jadpo_check::types::Ty::Prim(jadpo_check::types::Prim::Unit) => None,
        ty => Some(number(jadpo_check::graph::type_ref(lookup, ty))),
    };
    let repeat = match (operation.changes, operation.idempotent, keyed) {
        (false, _, _) => Repeat::Safe,
        (true, true, true) => Repeat::Keyed,
        // Marked idempotent with no key: repeating it as it is
        // changes nothing (a `PUT` that replaces, a `DELETE`).
        (true, true, false) => Repeat::Safe,
        (true, false, _) => Repeat::Unkeyed,
    };
    OperationPlan {
        connection: at,
        name: operation.full_name(),
        method: operation.method,
        path: operation
            .path
            .iter()
            .map(|part| match part {
                jadpo_check::providers::PathPart::Fixed(text) => PathPart::Fixed(text.clone()),
                jadpo_check::providers::PathPart::Field(name) => PathPart::Field(name.clone()),
            })
            .collect(),
        input,
        output,
        // The whole answer as sent, and the package's function that
        // turns it into the output (chunk 7.2c2).
        decode: operation.decode.as_ref().map(|(wire, decode)| {
            (
                number(jadpo_check::graph::type_ref(lookup, wire)),
                decode.as_str().to_string(),
            )
        }),
        output_at: operation.output_at.clone(),
        failed_when: operation
            .failed_when
            .as_ref()
            .map(|when| (when.path.clone(), when.equals.clone())),
        repeat,
        // Each failure's plan is numbered, so the runtime raises it as
        // the call's failure.
        failures: operation
            .failures
            .iter()
            .filter_map(|name| jadpo_check::graph::declaration_key(lookup, name))
            .filter(|key: &Key| types.failure_id(key).is_some())
            .map(|key| key.as_str().to_string())
            .collect(),
        recording: None,
    }
}

/// The text of `operation`'s recording, `recordings/<resource>.<operation>.json`
/// in its package (chunk 7.2d2).
fn recording(
    db: &dyn Db,
    project: Project,
    operation: &jadpo_check::providers::Operation,
) -> Option<String> {
    let path = format!(
        "{}/recordings/{}.json",
        operation.folder,
        operation.full_name()
    );
    project
        .records(db)
        .iter()
        .find(|record| record.path == path)
        .and_then(|record| record.text.clone().ok())
}
