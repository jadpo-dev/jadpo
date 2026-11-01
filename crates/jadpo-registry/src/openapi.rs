//! The OpenAPI 3.1 document the protocol's table gives, as a Jadpo
//! application writes one (area 7, "OpenAPI"): every named shape a
//! component, every failure the failure envelope, and every request open
//! to a caller who hasn't signed in. The local registry serves it at
//! `openapi.json`; the conformance test holds it, and the registry
//! application's own, to the table.

use serde_json::{Map, Value, json};

use crate::protocol::{Answer, BASE, ENDPOINTS, Endpoint, Failure};
use crate::shape::{NAMES, Shape, Text, named};

fn reference(name: &str) -> Value {
    json!({ "$ref": format!("#/components/schemas/{name}") })
}

/// A shape's JSON Schema.
pub fn schema(shape: &Shape) -> Value {
    match shape {
        Shape::Text(kind) => {
            let mut out = json!({ "type": "string" });
            if *kind == Text::DateTime {
                out["format"] = json!("date-time");
            }
            if let Some(pattern) = kind.pattern() {
                out["pattern"] = json!(pattern);
            }
            out
        }
        Shape::Integer => {
            json!({ "type": "integer", "minimum": 0, "maximum": 9_007_199_254_740_991u64 })
        }
        Shape::Bool => json!({ "type": "boolean" }),
        Shape::Enum(values) => json!({ "type": "string", "enum": values }),
        Shape::List(inner) => json!({ "type": "array", "items": schema(inner) }),
        Shape::Nullable(inner) => json!({ "anyOf": [schema(inner), { "type": "null" }] }),
        Shape::Named(name) => reference(name),
        Shape::Object(fields) => {
            let mut properties = Map::new();
            let mut required = Vec::new();
            for field in fields {
                properties.insert(field.name.to_string(), schema(&field.shape));
                if !field.optional {
                    required.push(json!(field.name));
                }
            }
            json!({
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false,
            })
        }
    }
}

/// A failure's component name: `package_not_found` is `PackageNotFound`.
fn failure_name(code: &str) -> String {
    code.split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|c| c.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect()
}

/// The failure envelope (area 4) for one code.
fn envelope(failure: &Failure) -> Value {
    json!({
        "type": "object",
        "description": failure.message,
        "properties": {
            "error": {
                "type": "object",
                "properties": {
                    "code": { "type": "string", "const": failure.code },
                    "message": { "type": "string" },
                    "request_id": { "type": "string" },
                },
                "required": ["code", "message", "request_id"],
            }
        },
        "required": ["error"],
    })
}

fn operation(endpoint: &Endpoint) -> Value {
    let mut parameters = Vec::new();
    for param in endpoint.path_params {
        parameters.push(json!({
            "name": param.name,
            "in": "path",
            "required": true,
            "schema": schema(&(param.shape)()),
        }));
    }
    for param in endpoint.query {
        parameters.push(json!({
            "name": param.name,
            "in": "query",
            "required": param.required,
            "schema": schema(&(param.shape)()),
        }));
    }
    let mut responses = Map::new();
    let success = match endpoint.answer {
        Answer::Json(name) => json!({
            "description": "Success.",
            "content": { "application/json": { "schema": reference(name) } },
        }),
        Answer::Tar => json!({
            "description": "The release's files.",
            "content": { "application/x-tar": { "schema": { "type": "string", "contentMediaType": "application/x-tar" } } },
        }),
    };
    responses.insert(endpoint.status.to_string(), success);
    let mut statuses: Vec<u16> = endpoint.failures.iter().map(|f| f.status).collect();
    statuses.sort_unstable();
    statuses.dedup();
    for status in statuses {
        let at: Vec<&Failure> = endpoint
            .failures
            .iter()
            .filter(|f| f.status == status)
            .collect();
        let schema = if at.len() == 1 {
            reference(&failure_name(at[0].code))
        } else {
            json!({ "oneOf": at.iter().map(|f| reference(&failure_name(f.code))).collect::<Vec<_>>() })
        };
        let codes: Vec<String> = at.iter().map(|f| format!("`{}`", f.code)).collect();
        responses.insert(
            status.to_string(),
            json!({
                "description": format!("Fails with {}.", codes.join(" or ")),
                "content": { "application/json": { "schema": schema } },
            }),
        );
    }
    let mut out = json!({
        "operationId": endpoint.id,
        "description": endpoint.summary,
        "parameters": parameters,
        "responses": responses,
        "security": [],
    });
    if let Some(body) = endpoint.body {
        out["requestBody"] = json!({
            "required": true,
            "content": { "application/json": { "schema": reference(body) } },
        });
    }
    out
}

/// The protocol's document.
pub fn document() -> Value {
    let mut paths = Map::new();
    for endpoint in ENDPOINTS {
        let item = paths
            .entry(endpoint.full_path())
            .or_insert_with(|| json!({}));
        item[endpoint.method.as_str().to_ascii_lowercase()] = operation(endpoint);
    }
    paths.insert(
        format!("{BASE}openapi.json"),
        json!({ "get": {
            "operationId": "get_openapi_json",
            "description": "This document.",
            "responses": { "200": { "description": "This document.", "content": { "application/json": { "schema": { "type": "object" } } } } },
            "security": [],
        } }),
    );
    let mut schemas = Map::new();
    for name in NAMES {
        let shape = named(name).expect("every named shape is defined");
        schemas.insert((*name).to_string(), schema(&shape));
    }
    for endpoint in ENDPOINTS {
        for failure in endpoint.failures {
            schemas.insert(failure_name(failure.code), envelope(failure));
        }
    }
    json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Jadpo registry",
            "version": "v1",
            "description": "The registry protocol (area 11, \"The registry protocol\").",
        },
        "paths": paths,
        "components": { "schemas": schemas },
    })
}
