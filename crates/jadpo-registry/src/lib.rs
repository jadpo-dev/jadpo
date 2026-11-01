//! The registry protocol (area 11, "The registry protocol"): what the
//! `jadpo` tool and the registry application at jadpo.dev are each built
//! against, so neither side drifts.
//!
//! - [`protocol`] holds the table, one [`protocol::Endpoint`] per request,
//!   and each request and answer as a type.
//! - [`log`] is the public log, an append-only Merkle tree of releases
//!   hashed as RFC 6962 does with SHA-256, with its inclusion and
//!   consistency proofs and the checks a client makes of them.
//! - [`archive`] packs and unpacks a release's files as `GET
//!   archives/{digest}` serves them.
//! - [`local`] is a registry on this machine, served from the table, that
//!   tests run against.
//! - [`conformance`] checks a served OpenAPI document against the table;
//!   [`openapi`] is the document the table itself gives.
//!
//! The crate is used from outside this repository too: the registry
//! application's CI runs this crate's conformance test against its own
//! served document (`JADPO_REGISTRY_OPENAPI`, docs/compiler-runtime.md
//! §2.1).

pub mod archive;
pub mod conformance;
pub mod http;
pub mod local;
pub mod log;
pub mod names;
pub mod openapi;
pub mod protocol;
pub mod shape;
