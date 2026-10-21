//! The provider plan (area 11, "Calling"; chunk 7.2c): each connection to
//! a provider package, read from `app.jadpo` and the package's
//! `provider.jadpo`, and each operation of its package, its input
//! and output numbered in the plan's JSON table so the runtime writes a
//! request and reads an answer with the codecs `json.read` and
//! `json.write` use, a provider package's field and variant spellings on
//! the wire included.

use std::collections::HashMap;

use jadpo_check::lookups::Lookup;
use jadpo_check::providers::Operation;
use jadpo_db::{Db, Project};
use jadpo_graph::{Key, TypeRef};
use jadpo_std::provider::{
    Address, AwsPlan, Called, ConnectionPlan, CredentialScheme, DEFAULT_TIMEOUT_MS,
    IdempotencyPlan, OperationPlan, PathPart, PollPlan, Protocol, ProviderPlan, RecordedPlan,
    WebhookEvent, WebhookPlan, WebhookReply,
};
use jadpo_std::receiving::{
    AcknowledgePlan, CloudflareQueue, Mapped, QueueEvent, QueuePlan, QueueSource, ReceivePlan,
};
use jadpo_std::subscriber::Repeat;

/// Each provider operation's number in the plan's provider operations
/// (chunk 8.1d2), as [`plan`] lists them: every operation of each package
/// connection's package, public or not, a connection's together in its
/// package's order.
#[derive(Debug, Default)]
pub(crate) struct Numbering {
    /// Each operation's number, by its connection's name and its own,
    /// `bookings.create`.
    named: HashMap<(String, String), u32>,
    /// Each operation's place among its package's operations, by the
    /// package's folder and its name.
    places: HashMap<(String, String), u32>,
    /// Each connection's first operation's number, by the connection's
    /// number in the connections table; `u32::MAX` for none.
    first: Vec<u32>,
}

impl Numbering {
    /// The operations in number order, each with its connection's number
    /// in `table`.
    fn listed<'a>(
        db: &'a dyn Db,
        project: Project,
        table: &[jadpo_std::connections::Connection],
    ) -> Vec<(
        u32,
        &'a jadpo_check::connections::PackageConnection,
        &'a Operation,
    )> {
        let operations = jadpo_check::providers::operations(db, project);
        let mut out = Vec::new();
        for connection in &jadpo_check::connections::package_connections(db, project).connections {
            let Some((at, _)) = jadpo_std::connections::named(table, &connection.name) else {
                continue;
            };
            // Every operation, public or not: a package's own actions call
            // the ones it keeps to itself (chunk 7.6).
            for operation in operations.of(&connection.folder) {
                out.push((at, connection, operation));
            }
        }
        out
    }

    /// The numbering of `project`'s operations, its connections numbered
    /// by `table`.
    pub(crate) fn new(
        db: &dyn Db,
        project: Project,
        table: &[jadpo_std::connections::Connection],
    ) -> Numbering {
        let mut out = Numbering {
            first: vec![u32::MAX; table.len()],
            ..Numbering::default()
        };
        for (number, (at, connection, operation)) in Numbering::listed(db, project, table)
            .into_iter()
            .enumerate()
        {
            let number = u32::try_from(number).unwrap_or(u32::MAX);
            if let Some(first) = out.first.get_mut(at as usize) {
                *first = (*first).min(number);
            }
            out.named
                .entry((connection.name.clone(), operation.full_name()))
                .or_insert(number);
        }
        // Each operation's place among its package's, as `listed` lists
        // a connection's.
        let mut counted: HashMap<&str, u32> = HashMap::new();
        for operation in &jadpo_check::providers::operations(db, project).list {
            let place = counted.entry(operation.folder.as_str()).or_default();
            out.places
                .entry((operation.folder.clone(), operation.full_name()))
                .or_insert(*place);
            *place = place.saturating_add(1);
        }
        out
    }

    /// The number of operation `operation` of connection `connection`.
    pub(crate) fn number(&self, connection: &str, operation: &str) -> Option<u32> {
        self.named
            .get(&(connection.to_string(), operation.to_string()))
            .copied()
    }

    /// The operation a call's site in the file at `path` calls:
    /// `operation` of `connection`, or for a package action's own
    /// operation, its place among its package's.
    pub(crate) fn called(&self, path: &str, connection: &str, operation: &str) -> Called {
        if connection == jadpo_graph::body::OWN_CONNECTION {
            let place = jadpo_db::package_at(path).and_then(|(home, _)| {
                self.places
                    .get(&(home.folder, operation.to_string()))
                    .copied()
            });
            return place.map_or_else(Called::default, Called::Own);
        }
        self.number(connection, operation)
            .map_or_else(Called::default, Called::Operation)
    }
}

/// The provider plan, numbering each operation's input and output in
/// `json_types` (the plan's JSON table's types), each operation's
/// connection by its number in `table`, the plan's connections table, each
/// operation the runtime calls by its number in `numbering`, and reading
/// each setting through `setting`.
pub(crate) fn plan(
    db: &dyn Db,
    project: Project,
    types: &mut crate::types::Types,
    setting: &dyn Fn(&str) -> Option<u32>,
    json_types: &mut Vec<TypeRef>,
    table: &[jadpo_std::connections::Connection],
    numbering: &Numbering,
) -> (ProviderPlan, Recordings) {
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
    let mut recordings = Recordings::default();
    // Numbered as `numbering` numbers them (chunk 8.1d2).
    for (at, _, operation) in Numbering::listed(db, project, table) {
        let keyed = jadpo_std::connections::package(table, at)
            .and_then(|plan| plan.idempotency.as_ref())
            .is_some_and(|plan| !plan.header.is_empty());
        let planned = operation_plan(&lookup, types, &mut number, operation, at, keyed);
        if let (Some(text), Ok(numbered)) = (
            recording(db, project, operation),
            u32::try_from(out.operations.len()),
        ) {
            recordings.operations.insert(numbered, text);
        }
        out.operations.push(planned);
    }
    out.first = numbering.first.clone();
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
        recordings.recorded.push(RecordedPlan {
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
            handshake: route.handshake(),
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
                    // Answered by the app's action, its reply encoded with
                    // the package's type (chunk 7.7b).
                    reply: event.reply.as_ref().and_then(|(reply, _, _)| {
                        Some(WebhookReply {
                            action: route.answering(&event.key)?.as_str().to_string(),
                            number: number(TypeRef::Node(reply.clone())),
                        })
                    }),
                })
                .collect(),
        });
    }
    // Each connection's polls the app subscribes to (chunk 7.6c): the
    // event made from the answer is written with its codec, numbered here.
    for poll in &jadpo_check::polls::polls(db, project).polls {
        out.polls.push(PollPlan {
            connection: poll.connection.clone(),
            name: poll.name.clone(),
            operation: poll.call.clone(),
            called: numbering
                .number(&poll.connection, &poll.call)
                .unwrap_or(u32::MAX),
            every_ms: poll.every_ms,
            event: poll.event.as_str().to_string(),
            number: number(TypeRef::Node(poll.event.clone())),
            fields: poll.fields.clone(),
            store: poll.store.clone(),
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
        use jadpo_check::receiving::SourceModel;
        let source = match &queue.source {
            SourceModel::Package {
                receive,
                messages,
                body,
                handle,
                acknowledge,
            } => {
                let (Some(receive_input), Some(acknowledge_input)) =
                    (inputs(&receive.1), inputs(&acknowledge.1))
                else {
                    continue;
                };
                QueueSource::Package {
                    receive: ReceivePlan {
                        operation: receive.0.clone(),
                        called: numbering
                            .number(&queue.connection, &receive.0)
                            .unwrap_or(u32::MAX),
                        input: receive_input,
                        messages: messages.clone(),
                        body: body.clone(),
                        handle: handle.clone(),
                    },
                    acknowledge: AcknowledgePlan {
                        operation: acknowledge.0.clone(),
                        called: numbering
                            .number(&queue.connection, &acknowledge.0)
                            .unwrap_or(u32::MAX),
                        input: acknowledge_input,
                    },
                }
            }
            // Pushed to the consumer on Workers (chunk 7.7c2).
            SourceModel::Cloudflare(cloudflare) => QueueSource::Cloudflare(CloudflareQueue {
                queue: cloudflare.queue.clone(),
                dead_letter_queue: cloudflare.dead_letter_queue.clone(),
            }),
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
            source,
        });
    }
    (out, recordings)
}

/// How calls through package connection `connection` are made: its
/// entry's driver in the plan's connections table (chunk 8.1d), each
/// setting read through `setting`.
pub(crate) fn connection_plan(
    db: &dyn Db,
    project: Project,
    connection: &jadpo_check::connections::PackageConnection,
    setting: &dyn Fn(&str) -> Option<u32>,
) -> ConnectionPlan {
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
                "header" => Some(CredentialScheme::Header),
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
        window_ms: given.window_ms,
        new_key_on: given.new_key_on.clone(),
        replays_failures: given.replays_failures,
    });
    ConnectionPlan {
        package: connection.package.clone(),
        address,
        credential,
        credential_header: reach.credential_header.clone(),
        headers: reach.headers.clone(),
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
        // The package's cap, or the connection's lower one (chunk
        // 7.6c).
        max_concurrent: match (reach.max_concurrent, connection.max_concurrent) {
            (Some(package), Some(own)) => Some(package.min(own)),
            (package, own) => package.or(own),
        },
    }
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
        changes: operation.changes,
        // Each failure's plan is numbered, so the runtime raises it as
        // the call's failure.
        failures: operation
            .failures
            .iter()
            .filter_map(|name| jadpo_check::graph::declaration_key(lookup, name))
            .filter(|key: &Key| types.failure_id(key).is_some())
            .map(|key| key.as_str().to_string())
            .collect(),
    }
}

/// What the plan's dev and test section keeps of the provider packages'
/// recordings (chunk 8.1e): each operation's text, by its number, and
/// every recording of the app's own integrations.
#[derive(Debug, Default)]
pub(crate) struct Recordings {
    pub(crate) operations: std::collections::BTreeMap<u32, String>,
    pub(crate) recorded: Vec<RecordedPlan>,
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
