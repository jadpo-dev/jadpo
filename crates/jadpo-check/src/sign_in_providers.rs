//! Sign-in providers (area 8, "Providers"; chunk A7b): each `Driver.oidc`
//! connection, read with its settings, and the names `sign_in: {
//! providers: [...] }` gives, which `SignInMethod` gains a value for.
//!
//! - **A connection on `Driver.oidc`** takes `issuer:` (a `Url`
//!   setting), `client_id:` (a `Text` setting) and `credential:` (a
//!   secret setting), each `config.<name>`, and nothing else. The runtime
//!   reads the rest from the provider's discovery document.
//! - **The names `providers:` gives** are read from the syntax alone
//!   ([`declared`]), since the type table reads them and the identity
//!   reads the type table.

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_syntax::ast::{self, AstNode, Expr};

use crate::config::config;
use crate::data::ConnectionKind;
use crate::lookups::Lookup;
use crate::tree::{self, name_text, node_span};
use crate::types::{Prim, TypeView};

/// One connection on `Driver.oidc`, read cleanly: the settings its
/// issuer, its client ID and its client secret are read from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Provider {
    pub name: String,
    pub issuer: String,
    pub client_id: String,
    pub secret: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Providers {
    /// Each that reads cleanly, as declared.
    pub list: Vec<Provider>,
    /// Every connection on `Driver.oidc`, read cleanly or not.
    pub declared: Vec<String>,
    pub problems: Vec<Diagnostic>,
}

impl Providers {
    pub fn named(&self, name: &str) -> Option<&Provider> {
        self.list.iter().find(|provider| provider.name == name)
    }
}

/// Its settings, and the type each reads.
const SETTINGS: [(&str, &str); 3] = [
    ("issuer", "Url"),
    ("client_id", "Text"),
    ("credential", "Text { secret: true }"),
];

/// Each connection with `driver: Driver.oidc`.
#[salsa::tracked(returns(ref))]
pub fn oidc_connections(db: &dyn Db, project: Project) -> Providers {
    let mut out = Providers::default();
    let lookup = Lookup::new(db, project);
    for found in crate::config::singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some("connections") {
                continue;
            }
            for (name, entry) in tree::members(member.block()) {
                let Some(connection) = name_text(&name) else {
                    continue;
                };
                if crate::data::connection_kind(entry.block()) != ConnectionKind::SignIn {
                    continue;
                }
                out.declared.push(connection.clone());
                let mut read: [Option<String>; 3] = [None, None, None];
                let mut clean = true;
                let members = tree::members(entry.block());
                for (setting, option) in &members {
                    let word = name_text(setting).unwrap_or_default();
                    let span = option.value().map_or_else(
                        || node_span(&found.path, option.syntax()),
                        |value| node_span(&found.path, value.syntax()),
                    );
                    if word == "driver" {
                        continue;
                    }
                    let Some(at) = SETTINGS.iter().position(|(known, _)| *known == word) else {
                        clean = false;
                        out.problems.push(
                            Diagnostic::new(
                                &rule::CONNECTION_KEY_INVALID,
                                format!("`{word}` isn't a setting of `{connection}`"),
                            )
                            .at(span, "not an OpenID Connect setting")
                            .reason("A sign-in provider on `Driver.oidc` takes `issuer:`, `client_id:` and `credential:`, each read from a setting; its discovery document gives the rest.")
                            .next(format!("Remove `{word}:`")),
                        );
                        continue;
                    };
                    let (_, wanted) = SETTINGS[at];
                    let field = option.value().as_ref().and_then(reads);
                    let declared = field
                        .as_ref()
                        .and_then(|field| config(db, project).setting(field));
                    let fits = declared.is_some_and(|setting| {
                        let prim = lookup.base_prim(&setting.ty);
                        match word.as_str() {
                            "issuer" => prim == Some(Prim::Url),
                            "client_id" => prim == Some(Prim::Text),
                            _ => setting.secret,
                        }
                    });
                    if fits {
                        read[at] = field;
                        continue;
                    }
                    clean = false;
                    // A setting no one declared is the configuration's own
                    // error, reported where `config.` is read.
                    if field.is_some() && declared.is_none() {
                        continue;
                    }
                    let (title, reason) = if word == "credential" {
                        (
                            format!("`{connection}`'s `credential:` must read a secret setting"),
                            "The client secret is a credential, so it is read from a `secret: true` setting (area 10), never written in source or kept where a plain setting is.".to_string(),
                        )
                    } else {
                        (
                            format!("`{connection}`'s `{word}:` must read a `{wanted}` setting"),
                            format!(
                                "A sign-in provider's `{word}:` is deployment configuration: a `{wanted}` setting, read as `config.<name>`, never written in source."
                            ),
                        )
                    };
                    out.problems.push(
                        Diagnostic::new(&rule::CONNECTION_VALUE_INVALID, title)
                            .at(span, "not the setting it needs")
                            .reason(reason)
                            .next(format!(
                                "Declare `<name>: {wanted}` in `config.jadpo`, and write `{word}: config.<name>`"
                            )),
                    );
                }
                for (word, wanted) in SETTINGS {
                    let written = members
                        .iter()
                        .any(|(setting, _)| name_text(setting).as_deref() == Some(word));
                    if written {
                        continue;
                    }
                    clean = false;
                    out.problems.push(
                        Diagnostic::new(
                            &rule::CONNECTION_KEY_MISSING,
                            format!("`{connection}` has no `{word}:`"),
                        )
                        .at(node_span(&found.path, name.syntax()), format!("no `{word}:`"))
                        .reason("A sign-in provider on `Driver.oidc` needs its issuer, the client ID the provider gave the app and its client secret, each read from a setting.")
                        .next(format!("Add `{word}: config.<name>`, reading a `{wanted}` setting")),
                    );
                }
                if let ([Some(issuer), Some(client_id), Some(secret)], true) = (read, clean) {
                    out.list.push(Provider {
                        name: connection,
                        issuer,
                        client_id,
                        secret,
                    });
                }
            }
        }
    }
    out
}

/// `config.<name>` as the name it reads.
fn reads(value: &Expr) -> Option<String> {
    let (first, rest) = tree::dotted(value)?;
    if tree::name_ref_text(&first).as_deref() != Some("config") || rest.len() != 1 {
        return None;
    }
    rest.first().and_then(tree::name_ref_text)
}

/// The names `authentication { sign_in: { providers: [...] } }` gives, in
/// order and once each, read from the syntax alone: `SignInMethod`'s
/// values after its own, whatever they name.
#[salsa::tracked(returns(ref))]
pub fn declared(db: &dyn Db, project: Project) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for found in crate::config::singletons(db, project, "authentication") {
        for (key, member) in tree::members(found.list.clone()) {
            if name_text(&key).as_deref() != Some("sign_in") {
                continue;
            }
            for (way, entry) in tree::members(member.block()) {
                if name_text(&way).as_deref() != Some("providers") {
                    continue;
                }
                for (name, _) in written(&found.path, &entry).unwrap_or_default() {
                    if !name.is_empty()
                        && !names.contains(&name)
                        && !jadpo_std::SignInMethod::NAMES.contains(&name.as_str())
                    {
                        names.push(name);
                    }
                }
            }
        }
    }
    names
}

/// `providers: [...]`'s entries as written: each name, empty for an entry
/// that isn't one, and where; `None` when it isn't a list.
pub(crate) fn written(path: &str, member: &ast::Member) -> Option<Vec<(String, Span)>> {
    let Some(Expr::ListExpr(list)) = member.value() else {
        return None;
    };
    Some(
        list.exprs()
            .map(|entry| {
                let at = node_span(path, entry.syntax());
                let name = match &entry {
                    Expr::NameRef(name) => tree::name_ref_text(name).unwrap_or_default(),
                    _ => String::new(),
                };
                (name, at)
            })
            .collect(),
    )
}
