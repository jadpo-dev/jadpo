//! The Workers host's integration tests, one binary (docs/compiler-runtime.md
//! §2.7): the host under workerd, Cloudflare's own Workers runtime, on this
//! machine.

// The native host's test authority: what the servers workerd reaches are
// signed by.
#[allow(dead_code)]
#[path = "../../jadpo-host-native/tests/authority.rs"]
mod authority;
mod bed;
mod connections;
mod passkeys;
mod workerd;
