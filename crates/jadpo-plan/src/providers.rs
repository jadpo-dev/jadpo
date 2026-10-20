//! The provider plan (area 11, "Calling"; chunk 7.2c): each connection to
//! a provider package, read from `app.jadpo` and the package's
//! `provider.jadpo`, and each public operation of its package, its input
//! and output numbered in the plan's JSON table so the runtime writes a
//! request and reads an answer with the codecs `json.read` and
//! `json.write` use, a provider package's field and variant spellings on
//! the wire included.

use jadpo_check::lookups::Lookup;
use jadpo_db::{Db, Project};
use jadpo_graph::{Key, TypeRef};
use jadpo_std::provider::{
    Address, ConnectionPlan, CredentialScheme, DEFAULT_TIMEOUT_MS, IdempotencyPlan, OperationPlan,
    PathPart, ProviderPlan,
};
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
            timeout_ms: connection.timeout_ms.unwrap_or(DEFAULT_TIMEOUT_MS),
            retry: connection.retry.unwrap_or_default(),
            idempotency,
            error_code: reach.error_code.clone(),
        });
        for operation in operations.of(&connection.folder) {
            if !operation.public {
                continue;
            }
            let input = operation
                .input
                .as_ref()
                .map(|(ty, _)| number(jadpo_check::graph::type_ref(&lookup, ty)));
            let output = match &operation.output {
                jadpo_check::types::Ty::Prim(jadpo_check::types::Prim::Unit) => None,
                ty => Some(number(jadpo_check::graph::type_ref(&lookup, ty))),
            };
            let repeat = match (operation.changes, operation.idempotent, keyed) {
                (false, _, _) => Repeat::Safe,
                (true, true, true) => Repeat::Keyed,
                // Marked idempotent with no key: repeating it as it is
                // changes nothing (a `PUT` that replaces, a `DELETE`).
                (true, true, false) => Repeat::Safe,
                (true, false, _) => Repeat::Unkeyed,
            };
            out.operations.push(OperationPlan {
                connection: at,
                name: operation.full_name(),
                method: operation.method,
                path: operation
                    .path
                    .iter()
                    .map(|part| match part {
                        jadpo_check::providers::PathPart::Fixed(text) => {
                            PathPart::Fixed(text.clone())
                        }
                        jadpo_check::providers::PathPart::Field(name) => {
                            PathPart::Field(name.clone())
                        }
                    })
                    .collect(),
                input,
                output,
                // The whole answer as sent, and the package's function
                // that turns it into the output (chunk 7.2c2).
                decode: operation.decode.as_ref().map(|(wire, decode)| {
                    (
                        number(jadpo_check::graph::type_ref(&lookup, wire)),
                        decode.as_str().to_string(),
                    )
                }),
                output_at: operation.output_at.clone(),
                failed_when: operation
                    .failed_when
                    .as_ref()
                    .map(|when| (when.path.clone(), when.equals.clone())),
                repeat,
                // Each failure's plan is numbered, so the runtime raises
                // it as the call's failure.
                failures: operation
                    .failures
                    .iter()
                    .filter_map(|name| jadpo_check::graph::declaration_key(&lookup, name))
                    .filter(|key: &Key| types.failure_id(key).is_some())
                    .map(|key| key.as_str().to_string())
                    .collect(),
            });
        }
    }
    out
}
