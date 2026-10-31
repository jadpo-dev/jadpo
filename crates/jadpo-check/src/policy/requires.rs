//! A membership's `requires:` (area 9, "Roles that need a way in"; chunk
//! A10): for each of its roles, how the holder must have signed in for
//! the role to count on a request.
//!
//! ```jadpo fragment
//! membership: {
//!     scope: MembershipScope.application
//!     member: patron_id
//!     role: role
//!     requires: {
//!         StaffRole.admin: {
//!             sign_in: [SignInMethod.google]
//!             strength: AuthenticationStrength.multi_factor
//!         }
//!         StaffRole.front_desk: {
//!             sign_in: [SignInMethod.google]
//!         }
//!     }
//! }
//! ```
//!
//! - Each line names one of the role enum's values once, and once any is
//!   written every value is (`policy.requirements_incomplete`);
//!   `StaffRole.x: {}` needs nothing.
//! - `sign_in:` lists the ways in, any of which will do: a sign-in
//!   provider `sign_in: { providers: }` names, or `external_token`.
//!   `strength:` is `AuthenticationStrength.multi_factor`. Anything else,
//!   an empty list or a role given twice is `policy.requirement_invalid`;
//!   a value the enum hasn't is `type.unknown_enum_variant`.
//! - A line no way in the app declares could ever meet is
//!   `policy.requirement_unsatisfiable`.

use std::collections::BTreeSet;

use jadpo_db::{Db, Project};
use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_std::{AuthenticationStrength, SignInMethod};
use jadpo_syntax::ast::{self, AstNode, Expr, MemberItem};

use crate::lookups::Lookup;
use crate::tree::{self, node_span};
use crate::types::{Def, TypeView};

/// What a role's holder must have signed in with for it to count.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Requirement {
    /// `sign_in:`: the ways in, by `SignInMethod` name, any of which will
    /// do; `None` for any.
    pub sign_in: Option<Vec<String>>,
    /// `strength: AuthenticationStrength.multi_factor`.
    pub multi_factor: bool,
}

impl Requirement {
    /// Whether it needs nothing: `{}`.
    pub fn none(&self) -> bool {
        self.sign_in.is_none() && !self.multi_factor
    }

    /// Its lines as the permissions report and `access.jadpo_record`
    /// write them, one each: `sign_in SignInMethod.google`, `strength
    /// AuthenticationStrength.multi_factor`; none for `{}`.
    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        for way in self.sign_in.iter().flatten() {
            out.push(format!("sign_in {}.{way}", SignInMethod::NAME));
        }
        if self.multi_factor {
            out.push(format!(
                "strength {}.{}",
                AuthenticationStrength::NAME,
                AuthenticationStrength::MultiFactor.name()
            ));
        }
        out
    }

    /// In a sentence: "signed in through google with a second factor".
    pub fn describe(&self) -> String {
        let way = match &self.sign_in {
            Some(ways) => {
                let ways: Vec<String> = ways.iter().map(|way| format!("`{way}`")).collect();
                format!("signed in through {}", crate::or_list(&ways))
            }
            None => "signed in".to_string(),
        };
        if self.multi_factor {
            format!("{way} with a second factor")
        } else {
            way
        }
    }
}

/// The ways in an application declares and how strongly each can sign a
/// person in, for telling whether a requirement can ever be met.
#[derive(Clone, Debug, Default)]
pub struct Ways {
    /// Each declared way that may be required (a provider, or
    /// `external_token`), all of which can give a second factor through
    /// their tokens' `amr`.
    pub required: BTreeSet<String>,
    /// Whether a passkey, a second factor itself, is declared.
    pub passkey: bool,
}

impl Ways {
    pub fn of(db: &dyn Db, project: Project) -> Ways {
        let identity = crate::identity::identity(db, project);
        let mut required: BTreeSet<String> = identity
            .providers
            .iter()
            .map(|provider| provider.name.clone())
            .collect();
        if identity.external_token.is_some() {
            required.insert(SignInMethod::ExternalToken.name().to_string());
        }
        Ways {
            required,
            passkey: identity.passkey.is_some(),
        }
    }

    /// Whether some way the app declares meets `requirement`.
    pub fn meet(&self, requirement: &Requirement) -> bool {
        match &requirement.sign_in {
            Some(ways) => ways.iter().any(|way| self.required.contains(way)),
            None if requirement.multi_factor => self.passkey || !self.required.is_empty(),
            None => true,
        }
    }
}

impl Ways {
    /// Whether a request to an API accepting `methods` can carry a sign-in
    /// that meets `requirement` (chunk A10): a provider's or a passkey's
    /// sign-in comes as a browser session, `external_token`'s as its
    /// token, and an API token or a service key carries neither.
    pub fn carried(
        &self,
        requirement: &Requirement,
        methods: &[jadpo_std::AuthenticationMethod],
    ) -> bool {
        use jadpo_std::AuthenticationMethod as M;
        let carries = |way: &str| {
            if way == SignInMethod::ExternalToken.name() {
                methods.contains(&M::ExternalToken)
            } else {
                methods.contains(&M::BrowserSession)
            }
        };
        match &requirement.sign_in {
            Some(ways) => ways
                .iter()
                .any(|way| self.required.contains(way) && carries(way)),
            None if requirement.multi_factor => {
                (self.passkey && methods.contains(&M::BrowserSession))
                    || self.required.iter().any(|way| carries(way))
            }
            None => true,
        }
    }
}

/// The `SignInMethod` values a requirement may name: a provider or
/// `external_token`, never a way Jadpo's own sign-in gives (a magic link
/// or a passkey), since those prove nothing about the company's accounts.
fn requirable(way: &str) -> bool {
    way != SignInMethod::MagicLink.name() && way != SignInMethod::Passkey.name()
}

/// `requires:`'s lines for a membership whose role field holds `roles`,
/// read from `member`, with their problems; each line by its role's
/// value, in the order written.
pub(crate) fn read(
    db: &dyn Db,
    project: Project,
    lookup: &Lookup<'_>,
    path: &str,
    roles: &str,
    member: &ast::Member,
    problems: &mut Vec<Diagnostic>,
) -> Vec<(String, Requirement)> {
    let variants: Vec<String> = match lookup.def(roles) {
        Some(Def::Enum { variants }) => variants.iter().map(|v| v.name.clone()).collect(),
        _ => Vec::new(),
    };
    let methods: Vec<String> = match lookup.def(SignInMethod::NAME) {
        Some(Def::Enum { variants }) => variants.iter().map(|v| v.name.clone()).collect(),
        _ => SignInMethod::NAMES.iter().map(|n| n.to_string()).collect(),
    };
    let key_at = member.member_key().map_or_else(
        || node_span(path, member.syntax()),
        |key| node_span(path, key.syntax()),
    );
    let Some(block) = member.block() else {
        problems.push(invalid(
            key_at,
            "`requires:` is a block of role lines".to_string(),
            "not a block",
            format!(
                "`requires:` gives each of `{roles}`'s values a line saying how its holder must have signed in, such as `{roles}.{}: {{ sign_in: [SignInMethod.<provider>] }}`.",
                variants.first().map_or("<value>", String::as_str)
            ),
            "Write `requires: { ... }` with a line for each role",
        ));
        return Vec::new();
    };
    let mut out: Vec<(String, Requirement)> = Vec::new();
    let mut seen: Vec<(String, Span)> = Vec::new();
    for item in block.member_items() {
        let MemberItem::Member(line) = item else {
            continue;
        };
        let Some(key) = line.member_key() else {
            continue;
        };
        let at = node_span(path, key.syntax());
        let names: Vec<String> = key.names().filter_map(|n| tree::name_text(&n)).collect();
        let written = tree::written(key.syntax());
        let role = match names.as_slice() {
            [head, value] if *head == roles && variants.contains(value) => value.clone(),
            [head, value] if *head == roles => {
                problems.push(unknown_variant(at, roles, value, &variants));
                continue;
            }
            _ => {
                problems.push(invalid(
                    at,
                    format!("`{written}` isn't one of `{roles}`'s values"),
                    "not a role this membership holds",
                    format!(
                        "Each line of `requires:` names one of the membership's roles as `{roles}.<value>`, the enum its `role:` field holds."
                    ),
                    &format!("Name a role as `{roles}.<value>`"),
                ));
                continue;
            }
        };
        if let Some((_, first)) = seen.iter().find(|(name, _)| *name == role) {
            problems.push(
                invalid(
                    at,
                    format!("`{roles}.{role}` is given twice in `requires:`"),
                    "given again",
                    "A role's requirement is said once, so there is never a question of which counts.".to_string(),
                    "Remove one line, keeping the requirement you mean",
                )
                .also(first.clone(), "first given here"),
            );
            continue;
        }
        seen.push((role.clone(), at.clone()));
        let requirement = read_line(path, &line, &methods, problems);
        if let Some(requirement) = requirement {
            out.push((role, requirement));
        }
    }
    // Once any role says how it is held, every one does, so a value added
    // to the enum later is decided rather than open by default.
    if !seen.is_empty() || block.member_items().next().is_none() {
        let missing: Vec<String> = variants
            .iter()
            .filter(|value| !seen.iter().any(|(name, _)| name == *value))
            .map(|value| format!("`{roles}.{value}`"))
            .collect();
        if !missing.is_empty() {
            problems.push(
                Diagnostic::new(
                    &rule::POLICY_REQUIREMENTS_INCOMPLETE,
                    format!("`requires:` says nothing of {}", crate::and_list(&missing)),
                )
                .at(key_at.clone(), "not every role")
                .reason(format!(
                    "Once a membership says how one role must be signed in to, it says so of every role, so a role added to `{roles}` later is decided, not open by default; `{roles}.<value>: {{}}` needs nothing."
                ))
                .next(format!(
                    "Add a line for {}, `{{}}` where any sign-in will do",
                    crate::and_list(&missing)
                )),
            );
        }
    }
    let ways = Ways::of(db, project);
    for (role, requirement) in &out {
        if ways.meet(requirement) {
            continue;
        }
        let at = seen
            .iter()
            .find(|(name, _)| name == role)
            .map_or_else(|| key_at.clone(), |(_, span)| span.clone());
        let declared: Vec<String> = ways
            .required
            .iter()
            .map(|way| format!("`{}.{way}`", SignInMethod::NAME))
            .collect();
        problems.push(
            Diagnostic::new(
                &rule::POLICY_REQUIREMENT_UNSATISFIABLE,
                format!("No one can hold `{roles}.{role}`: nothing the app signs people in with is {}", requirement.describe()),
            )
            .at(at, "can never be met")
            .reason(if declared.is_empty() {
                "A role counts only for someone whose sign-in meets its requirement, and the app declares no sign-in provider, `external_token:` or passkey that could.".to_string()
            } else {
                format!(
                    "A role counts only for someone whose sign-in meets its requirement; the ways in the app declares that a requirement may name are {}.",
                    crate::and_list(&declared)
                )
            })
            .next("Name a way in the app declares, or declare the one you mean in `authentication.jadpo`"),
        );
    }
    out
}

/// One role's `{ sign_in: [...]  strength: ... }`.
fn read_line(
    path: &str,
    line: &ast::Member,
    methods: &[String],
    problems: &mut Vec<Diagnostic>,
) -> Option<Requirement> {
    let at = line.member_key().map_or_else(
        || node_span(path, line.syntax()),
        |key| node_span(path, key.syntax()),
    );
    let Some(block) = line.block() else {
        problems.push(invalid(
            at,
            "A role's requirement is a block".to_string(),
            "not a block",
            "A role's line says how its holder must have signed in, as `{ sign_in: [SignInMethod.<provider>] }`, `{ strength: AuthenticationStrength.multi_factor }`, both, or `{}` for any sign-in.".to_string(),
            "Write the requirement as `{ ... }`",
        ));
        return None;
    };
    let mut requirement = Requirement::default();
    let mut clean = true;
    let mut given: Vec<String> = Vec::new();
    for (name, condition) in tree::members(Some(block)) {
        let word = tree::name_text(&name).unwrap_or_default();
        let span = node_span(path, name.syntax());
        if given.contains(&word) {
            clean = false;
            problems.push(invalid(
                span,
                format!("`{word}:` is given twice"),
                "given again",
                "A role's requirement says each condition once.".to_string(),
                &format!("Remove one `{word}:`"),
            ));
            continue;
        }
        given.push(word.clone());
        match word.as_str() {
            "sign_in" => {
                let Some(Expr::ListExpr(list)) = condition.value() else {
                    clean = false;
                    problems.push(invalid(
                        span,
                        "`sign_in:` takes a list of ways in".to_string(),
                        "not a list",
                        "`sign_in:` lists the ways in, any of which will do: `[SignInMethod.google]`.".to_string(),
                        "Write `sign_in: [SignInMethod.<provider>]`",
                    ));
                    continue;
                };
                let mut ways: Vec<String> = Vec::new();
                let entries: Vec<Expr> = list.exprs().collect();
                if entries.is_empty() {
                    clean = false;
                    problems.push(invalid(
                        span,
                        "`sign_in:` lists no way in".to_string(),
                        "empty",
                        "An empty list would let no one hold the role; leave `sign_in:` out for any way in, or name the ways that count.".to_string(),
                        "Name a way in, such as `SignInMethod.google`, or remove `sign_in:`",
                    ));
                    continue;
                }
                for entry in entries {
                    let entry_at = node_span(path, entry.syntax());
                    let written = tree::written(entry.syntax());
                    match enum_value(&entry, SignInMethod::NAME) {
                        Some(way) if methods.contains(&way) && requirable(&way) => {
                            if ways.contains(&way) {
                                clean = false;
                                problems.push(invalid(
                                    entry_at,
                                    format!("`{written}` is listed twice"),
                                    "listed again",
                                    "`sign_in:` lists each way in once.".to_string(),
                                    &format!("Remove one `{written}`"),
                                ));
                            } else {
                                ways.push(way);
                            }
                        }
                        Some(way) if methods.contains(&way) => {
                            clean = false;
                            problems.push(invalid(
                                entry_at,
                                format!("A role can't require `{written}`"),
                                "not a provider",
                                "A requirement names the company's own way in, a sign-in provider or `external_token`, so access ends where the company ends it; a magic link or a passkey proves only an address or a device. Use `strength: AuthenticationStrength.multi_factor` to ask for a second factor.".to_string(),
                                "Name a sign-in provider or `SignInMethod.external_token`",
                            ));
                        }
                        Some(way) => {
                            clean = false;
                            problems.push(unknown_variant(
                                entry_at,
                                SignInMethod::NAME,
                                &way,
                                methods,
                            ));
                        }
                        None => {
                            clean = false;
                            problems.push(invalid(
                                entry_at,
                                format!("`{written}` isn't a way in"),
                                "not a `SignInMethod`",
                                "`sign_in:` lists `SignInMethod` values: a provider `sign_in: { providers: }` names, or `external_token`.".to_string(),
                                "Write `SignInMethod.<provider>`",
                            ));
                        }
                    }
                }
                requirement.sign_in = Some(ways);
            }
            "strength" => {
                let written = condition
                    .value()
                    .map(|value| tree::written(value.syntax()))
                    .unwrap_or_default();
                let strengths: Vec<String> = AuthenticationStrength::NAMES
                    .iter()
                    .map(|n| n.to_string())
                    .collect();
                match condition
                    .value()
                    .and_then(|value| enum_value(&value, AuthenticationStrength::NAME))
                {
                    Some(value) if value == AuthenticationStrength::MultiFactor.name() => {
                        requirement.multi_factor = true;
                    }
                    Some(value) if strengths.contains(&value) => {
                        clean = false;
                        problems.push(invalid(
                            span,
                            format!("`strength: {written}` requires nothing"),
                            "every sign-in is this strong",
                            "Every sign-in proves at least one factor, so `primary` adds nothing; a requirement's `strength:` is `AuthenticationStrength.multi_factor`.".to_string(),
                            "Remove `strength:`, or write `AuthenticationStrength.multi_factor`",
                        ));
                    }
                    Some(value) => {
                        clean = false;
                        let at = condition
                            .value()
                            .map_or_else(|| span.clone(), |v| node_span(path, v.syntax()));
                        problems.push(unknown_variant(
                            at,
                            AuthenticationStrength::NAME,
                            &value,
                            &strengths,
                        ));
                    }
                    None => {
                        clean = false;
                        problems.push(invalid(
                            span,
                            format!("`strength: {written}` isn't an `AuthenticationStrength`"),
                            "not a strength",
                            "`strength:` is `AuthenticationStrength.multi_factor`.".to_string(),
                            "Write `strength: AuthenticationStrength.multi_factor`",
                        ));
                    }
                }
            }
            other => {
                clean = false;
                let near = jadpo_diag::nearest(other, ["sign_in", "strength"]);
                let problem = invalid(
                    span.clone(),
                    format!("`{other}` isn't something a role can require"),
                    "not a requirement",
                    format!(
                        "A role's line requires `sign_in:`, the ways in that count, and `strength:`, how strongly; nothing else.{}",
                        near.map_or_else(String::new, |near| format!(" Did you mean `{near}`?"))
                    ),
                    &match near {
                        Some(near) => format!("Use `{near}`"),
                        None => format!("Remove `{other}:`"),
                    },
                );
                problems.push(match near {
                    Some(near) => problem.edit(span, near),
                    None => problem,
                });
            }
        }
    }
    clean.then_some(requirement)
}

/// `Enum.value` as `value`, when `expr` names `enum_name`'s values.
fn enum_value(expr: &Expr, enum_name: &str) -> Option<String> {
    let (first, rest) = tree::dotted(expr)?;
    match rest.as_slice() {
        [value] if tree::name_ref_text(&first).as_deref() == Some(enum_name) => {
            tree::name_ref_text(value)
        }
        _ => None,
    }
}

fn invalid(span: Span, headline: String, label: &str, reason: String, next: &str) -> Diagnostic {
    Diagnostic::new(&rule::POLICY_REQUIREMENT_INVALID, headline)
        .at(span, label)
        .reason(reason)
        .next(next)
}

fn unknown_variant(span: Span, enum_name: &str, value: &str, values: &[String]) -> Diagnostic {
    let listed: Vec<String> = values.iter().map(|v| format!("`{v}`")).collect();
    Diagnostic::new(
        &rule::TYPE_UNKNOWN_ENUM_VARIANT,
        format!("`{enum_name}` has no variant `{value}`"),
    )
    .at(span, "not a variant")
    .reason(format!(
        "`{enum_name}`'s variants are {}.",
        crate::and_list(&listed)
    ))
    .next(format!("Use one of `{enum_name}`'s variants"))
}
