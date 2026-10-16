//! Client addresses (area 10, "Client addresses"; chosen in chunk 5.1c):
//! `application { client_address: { source: ClientAddressSource.x hops: n } }`
//! says where the address of the client a request came from is read, and
//! how many proxies of the deployment's own to believe.
//!
//! Read once per project ([`client_address`]) into a [`ClientAddress`]:
//! the plan `jadpo-plan` puts in the route table, which the runtime
//! chooses an address from, and the problems in how it is written. A
//! `hops:` is a whole number from 1, or the `Int` setting of that name,
//! whose type declares `min:` of at least 1, so a count below 1 never
//! reaches a start.

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::http::{ClientAddressPlan, Hops};
use jadpo_std::{Builtin, ClientAddressSource, Num};
use jadpo_syntax::ast::{self, AstNode, Expr};

use crate::config::singletons;
use crate::lookups::Lookup;
use crate::tree::{self, dotted, node_span};
use crate::types::{Const, TypeView};

/// The only options of a `client_address:` block.
const OPTIONS: [&str; 2] = ["source", "hops"];

/// The project's client address declaration, with its problems.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClientAddress {
    /// What it reads cleanly as; `None` when nothing is declared, and when
    /// what is declared has a problem.
    pub plan: Option<ClientAddressPlan>,
    pub problems: Vec<Diagnostic>,
}

/// The `client_address:` of `app.jadpo`'s `application { }`.
#[salsa::tracked(returns(ref))]
pub fn client_address(db: &dyn Db, project: Project) -> ClientAddress {
    let mut out = ClientAddress::default();
    let mut first: Option<Span> = None;
    for found in singletons(db, project, "application") {
        for (key, member) in tree::members(found.list.clone()) {
            if tree::name_text(&key).as_deref() != Some("client_address") {
                continue;
            }
            let at = node_span(&found.path, key.syntax());
            if let Some(first) = &first {
                out.problems.push(
                    Diagnostic::new(
                        &rule::CONFIGURATION_CLIENT_ADDRESS_DUPLICATE,
                        "`client_address:` is declared twice",
                    )
                    .at(at, "declared again")
                    .also(first.clone(), "first declared here")
                    .reason("The client's address is read one way for the whole application, so every request is counted by the same rule.")
                    .next("Keep one `client_address:` and remove the other"),
                );
                continue;
            }
            first = Some(at.clone());
            let mut reader = Reader {
                db,
                project,
                path: &found.path,
                problems: &mut out.problems,
            };
            out.plan = reader.read(&at, &member);
        }
    }
    out
}

/// The problems in the project's client address declaration.
pub fn client_address_problems(db: &dyn Db, project: Project) -> &[Diagnostic] {
    &client_address(db, project).problems
}

struct Reader<'a> {
    db: &'a dyn Db,
    project: Project,
    path: &'a str,
    problems: &'a mut Vec<Diagnostic>,
}

/// What a `hops:` came to.
enum Count {
    /// It reads cleanly.
    Read(Hops),
    /// It has a problem, reported; or it is a setting no one declares,
    /// reported where it is read.
    Bad,
}

impl Reader<'_> {
    fn read(&mut self, at: &Span, member: &ast::Member) -> Option<ClientAddressPlan> {
        let Some(list) = member.block() else {
            self.problems.push(
                Diagnostic::new(
                    &rule::CONFIGURATION_CLIENT_ADDRESS_INVALID,
                    "`client_address:` isn't a block",
                )
                .at(at.clone(), "a block goes here")
                .reason("The client's address is read from a `source:` and, for the sources that count proxies in a header, the number of proxies of the deployment's own: `client_address: { source: ClientAddressSource.forwarded_for hops: config.proxy_hops }`.")
                .next("Write `client_address: { source: ClientAddressSource.<source> }`, with `hops:` for `forwarded_for` and `forwarded`"),
            );
            return None;
        };
        let mut source: Option<(Option<ClientAddressSource>, Span)> = None;
        let mut hops: Option<(Expr, Span)> = None;
        let mut shaped = true;
        for (key, option) in tree::members(Some(list)) {
            let Some(name) = tree::name_text(&key) else {
                continue;
            };
            let key_span = node_span(self.path, key.syntax());
            match name.as_str() {
                "source" => {
                    let value = option.value();
                    let span = value
                        .as_ref()
                        .map_or_else(|| key_span.clone(), |v| node_span(self.path, v.syntax()));
                    source = Some((value.as_ref().and_then(|v| self.source(v, &span)), span));
                }
                "hops" => {
                    if let Some(value) = option.value() {
                        let span = node_span(self.path, value.syntax());
                        hops = Some((value, span));
                    } else {
                        shaped = false;
                        self.problems.push(hops_invalid(
                            "`hops:` has no value",
                            key_span,
                            "no value",
                            "`hops:` gives the number of proxies of the deployment's own in front of the app.",
                            WRITE,
                        ));
                    }
                }
                other => {
                    shaped = false;
                    let near = nearest(other, OPTIONS);
                    let problem = Diagnostic::new(
                        &rule::CONFIGURATION_CLIENT_ADDRESS_INVALID,
                        format!("`{other}` isn't an option of `client_address:`"),
                    )
                    .at(key_span.clone(), "unknown option")
                    .reason(format!(
                        "`client_address:` takes `source:` and `hops:`, and nothing else; which proxies to trust is a count and not a list.{}",
                        near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
                    ));
                    self.problems.push(match near {
                        Some(near) => problem
                            .next(format!("Use `{near}` instead of `{other}`"))
                            .edit(key_span, near),
                        None => problem.next(format!("Remove `{other}:`")),
                    });
                }
            }
        }
        let Some((source, source_span)) = source else {
            if shaped {
                self.problems.push(
                    Diagnostic::new(
                        &rule::CONFIGURATION_CLIENT_ADDRESS_INVALID,
                        "`client_address:` has no `source:`",
                    )
                    .at(at.clone(), "no `source:`")
                    .reason("The client's address is read from one of four sources: the connection, `X-Forwarded-For`, `Forwarded` or Cloudflare's `CF-Connecting-IP`.")
                    .next("Add `source: ClientAddressSource.<source>`"),
                );
            }
            return None;
        };
        let source = source?;
        if !shaped {
            return None;
        }
        let counts = matches!(
            source,
            ClientAddressSource::ForwardedFor | ClientAddressSource::Forwarded
        );
        match (counts, hops) {
            (true, None) => {
                self.problems.push(hops_invalid(
                    format!("`{}` needs `hops:`", source.name()),
                    source_span,
                    "no `hops:`",
                    &format!(
                        "`{}` counts the proxies of the deployment's own from the right of the header, and the count is never guessed: `hops: 1` for one load balancer in front of the app.",
                        source.name()
                    ),
                    "Add `hops: <number of proxies>`, such as `hops: 1`",
                ));
                None
            }
            (true, Some((value, span))) => match self.hops(&value, &span) {
                Count::Read(hops) => Some(ClientAddressPlan {
                    source,
                    hops: Some(hops),
                }),
                Count::Bad => None,
            },
            (false, Some((_, span))) => {
                self.problems.push(
                    Diagnostic::new(
                        &rule::CONFIGURATION_CLIENT_ADDRESS_HOPS_UNUSED,
                        format!("`hops:` means nothing with `{}`", source.name()),
                    )
                    .at(span, "no proxies are counted")
                    .reason(if source == ClientAddressSource::Cloudflare {
                        "Cloudflare sets `CF-Connecting-IP` on every request from its edge and overwrites a caller's, so there is no list to count from the right."
                    } else {
                        "`connection` reads the connection's own address and no header, so there is nothing to count."
                    })
                    .next("Remove `hops:`"),
                );
                None
            }
            (false, None) => Some(ClientAddressPlan { source, hops: None }),
        }
    }

    /// `ClientAddressSource.x`.
    fn source(&mut self, value: &Expr, span: &Span) -> Option<ClientAddressSource> {
        let written = tree::written(value.syntax());
        let names = match Const::from_expr(value) {
            Const::Path(names) => names,
            _ => Vec::new(),
        };
        if let [host, variant] = names.as_slice()
            && host == ClientAddressSource::NAME
        {
            if let Some(source) = ClientAddressSource::from_name(variant) {
                return Some(source);
            }
            let near = nearest(variant, ClientAddressSource::NAMES.iter().copied());
            let problem = self.unknown(&written, span).reason(format!(
                "`{}` has no value called `{variant}`; the four are {}.{}",
                ClientAddressSource::NAME,
                value_list(),
                near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
            ));
            self.problems.push(match near {
                Some(near) => problem
                    .next(format!(
                        "Use `{}.{near}` instead",
                        ClientAddressSource::NAME
                    ))
                    .edit(
                        span.clone(),
                        format!("{}.{near}", ClientAddressSource::NAME),
                    ),
                None => problem.next("Use one of the four sources"),
            });
            return None;
        }
        let problem = self.unknown(&written, span).reason(format!(
            "`source:` is a `{}`, written `{}.<source>`: {}.",
            ClientAddressSource::NAME,
            ClientAddressSource::NAME,
            value_list()
        ));
        self.problems
            .push(problem.next("Write `source: ClientAddressSource.forwarded_for`, for instance"));
        None
    }

    fn unknown(&self, written: &str, span: &Span) -> Diagnostic {
        Diagnostic::new(
            &rule::CONFIGURATION_CLIENT_ADDRESS_SOURCE_UNKNOWN,
            format!("`{written}` isn't a source of the client's address"),
        )
        .at(span.clone(), "not a `ClientAddressSource`")
    }

    /// `hops: 2` or `hops: config.proxy_hops`.
    fn hops(&mut self, value: &Expr, span: &Span) -> Count {
        let shape = "`hops:` is how many proxies of the deployment's own sit in front of the app: a whole number from 1, or an `Int` setting whose type declares `min:` of at least 1.";
        let written = tree::written(value.syntax());
        if let Some((first, rest)) = dotted(value)
            && tree::name_ref_text(&first).as_deref() == Some("config")
            && let [name] = rest.as_slice()
            && let Some(name) = tree::name_ref_text(name)
        {
            return self.setting(&name, span, shape);
        }
        match Const::from_expr(value) {
            Const::Number {
                value,
                decimal: false,
            } if value.compare(Num::whole(1)).is_ge() => match u32::try_from(value.mantissa) {
                Ok(count) if value.scale == 0 => Count::Read(Hops::Count(count)),
                _ => {
                    self.problems.push(hops_invalid(
                        format!("`{written}` can't be a number of proxies"),
                        span.clone(),
                        "too large",
                        "No chain of proxies is that long; a count past what a header can hold would never find an address.",
                        WRITE,
                    ));
                    Count::Bad
                }
            },
            Const::Number { decimal: false, .. } => {
                self.problems.push(hops_invalid(
                    format!("`{written}` can't be a number of proxies"),
                    span.clone(),
                    "below 1",
                    shape,
                    WRITE,
                ));
                Count::Bad
            }
            _ => {
                self.problems.push(hops_invalid(
                    format!("`{written}` can't be a number of proxies"),
                    span.clone(),
                    "not a whole number",
                    shape,
                    WRITE,
                ));
                Count::Bad
            }
        }
    }

    /// `config.<name>`: an `Int` that is never below 1.
    fn setting(&mut self, name: &str, span: &Span, shape: &str) -> Count {
        let config = crate::config::config(self.db, self.project);
        // A name no setting has is reported where it is read (area 10).
        let Some(found) = config.setting(name) else {
            return Count::Bad;
        };
        let lookup = Lookup::new(self.db, self.project);
        if lookup.base_prim(&found.ty) != Some(Builtin::Int) {
            self.problems.push(hops_invalid(
                format!("`hops:` reads `{name}`, which isn't an `Int`"),
                span.clone(),
                format!("`{name}` is `{}`", found.ty),
                shape,
                WRITE,
            ));
            return Count::Bad;
        }
        let min = lookup
            .rule_chain(&found.ty)
            .into_iter()
            .find_map(|(_, rules)| rules.min);
        if !min.is_some_and(|min| min.compare(Num::whole(1)).is_ge()) {
            self.problems.push(hops_invalid(
                format!("`hops:` reads `{name}`, whose type doesn't declare `min: 1`"),
                span.clone(),
                format!("`{name}` may be below 1"),
                "A count below 1 would trust no proxy at all, so the setting's type has to rule it out when the app starts: `type ProxyHops = Int { min: 1 }`.",
                &format!("Give `{name}` a type that declares `min:` of at least 1"),
            ));
            return Count::Bad;
        }
        Count::Read(Hops::Setting(name.to_string()))
    }
}

/// The four sources, for messages.
fn value_list() -> String {
    ClientAddressSource::NAMES
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn hops_invalid(
    message: impl Into<String>,
    span: Span,
    label: impl Into<String>,
    reason: &str,
    next: &str,
) -> Diagnostic {
    Diagnostic::new(
        &rule::CONFIGURATION_CLIENT_ADDRESS_HOPS_INVALID,
        message.into(),
    )
    .at(span, label.into())
    .reason(reason.to_string())
    .next(next.to_string())
}

/// What to write instead, for a `hops:` that has a problem.
const WRITE: &str = "Write a whole number from 1, such as `hops: 1`, or read an `Int` setting that declares `min: 1`";
