//! Each API's own OpenAPI document, served at `<prefix>/openapi.json`
//! (area 7, "OpenAPI"; chunk 7.2): every API that has a document, that is
//! routes of its own or the passkey routes, unless its entry in `apis: { }`
//! says `serve_openapi: false`. The route is the runtime's, listed like
//! any generated route: in the API's record (`generated openapi`), its
//! OpenAPI document (`x-jadpo-generated`) and `jadpo routes`.
//!
//! It is under the API's own default authentication: a signed-in caller,
//! by any method the API accepts, so an admin API's document needs
//! sign-in. Where the API accepts no way to sign in that is built, every
//! route it has lets anyone call it, so its document tells no one more
//! than its routes do, and anyone may read it.

use jadpo_db::{Db, Project};
use jadpo_std::http::{Authentication, OPENAPI_PATH};

/// One API's served document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Served {
    /// The API's name.
    pub api: String,
    /// The path's segments: the API's prefix, then `openapi.json`.
    pub segments: Vec<String>,
    pub authentication: Authentication,
}

impl Served {
    /// `/v1/openapi.json`.
    pub fn path(&self) -> String {
        self.segments
            .iter()
            .map(|segment| format!("/{segment}"))
            .collect()
    }
}

/// Every API's served document, in API name order.
pub fn served(db: &dyn Db, project: Project) -> Vec<Served> {
    let apis = super::api_table(db, project);
    let groups = super::groups(db, project);
    let identity = crate::identity::identity(db, project);
    // The passkey routes belong to the first API that accepts browser
    // sessions (area 8, "Passkeys"), routes of its own or not.
    let passkey_api = identity.passkeys().and_then(|_| {
        apis.apis
            .iter()
            .find(|api| {
                super::apis::accepted_methods(db, project, &api.name)
                    .contains(&jadpo_std::AuthenticationMethod::BrowserSession)
            })
            .map(|api| api.name.as_str())
    });
    apis.apis
        .iter()
        .filter(|api| api.serve_openapi)
        .filter(|api| {
            passkey_api == Some(api.name.as_str())
                || groups.routes().any(|(group, _)| group.api == api.name)
        })
        .map(|api| {
            let signs_in = super::apis::accepted_methods(db, project, &api.name)
                .into_iter()
                .any(|method| identity.built(method));
            Served {
                api: api.name.clone(),
                segments: api
                    .prefix
                    .iter()
                    .cloned()
                    .chain([OPENAPI_PATH.to_string()])
                    .collect(),
                authentication: if signs_in {
                    Authentication::Required
                } else {
                    Authentication::Public
                },
            }
        })
        .collect()
}

/// API `api`'s served document, if it serves one.
pub fn served_by(db: &dyn Db, project: Project, api: &str) -> Option<Served> {
    served(db, project)
        .into_iter()
        .find(|served| served.api == api)
}
