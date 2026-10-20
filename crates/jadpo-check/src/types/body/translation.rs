//! A translation call (area 10, "Using a message"):
//! `translations.<domain>.<message>(field: value, ..., locale:)`, typed by
//! the message's fields, and `translations.<domain>.<enum>(value, locale:)`
//! for an enum with wording. The scope is a domain, or `shared`; the result
//! is `PresentationText`, which can be shown but never stored.

use jadpo_db::Ownership;
use jadpo_diag::{Diagnostic, nearest, rule};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::calls::plural;
use super::facts::Reach;
use super::{Body, CallKind, is_literal, node_key};
use crate::lookups::Name;
use crate::translations::{
    Callable, ParamSpec, SHARED, accepts, callable, callable_keys, is_scope, package_of,
};
use crate::types::{Prim, Ty, TypeRef};
use jadpo_std::translation::ParamKind;

impl Body<'_> {
    pub(super) fn translation_call(
        &mut self,
        call: &ast::CallExpr,
        callee: &Expr,
        rest: &[String],
        written: &str,
    ) -> (Ty, CallKind, Reach) {
        let text = Ty::Prim(Prim::PresentationText);
        let [scope, key] = rest else {
            self.args_only(call);
            self.message_unknown(callee, written, rest, Why::NoScope);
            return (text, CallKind::Pure(written.to_string()), Reach::Unknown);
        };
        if !is_scope(self.db, self.project, scope) {
            self.args_only(call);
            match package_of(self.db, self.project, scope) {
                Some(home) => self.through_package(callee, written, &home, key),
                None => self.message_unknown(callee, written, rest, Why::NotAScope),
            }
            return (text, CallKind::Pure(written.to_string()), Reach::Unknown);
        }
        let found = callable(
            self.db,
            self.project,
            Name::new(self.db, scope.clone()),
            Name::new(self.db, key.clone()),
        )
        .clone();
        let path = format!("{scope}.{key}");
        match found {
            Callable::Found { params, symbol, .. } => {
                self.reaches(callee, scope, &symbol);
                let reach = self.translation_arguments(call, &path, &params, written);
                (text, CallKind::Pure(written.to_string()), reach)
            }
            // Its declaration has a mistake, reported there.
            Callable::Broken => {
                self.args_only(call);
                let reach = Reach::Translation {
                    key: path,
                    arity: Self::arguments(call).len(),
                };
                (text, CallKind::Pure(written.to_string()), reach)
            }
            Callable::Unknown => {
                self.args_only(call);
                self.message_unknown(callee, written, rest, Why::NoMessage);
                (text, CallKind::Pure(written.to_string()), Reach::Unknown)
            }
        }
    }

    /// `Postcodes.translations.<message>(...)`: a library package's public
    /// message or enum with wording (area 10, "Jadpo's and packages'
    /// text"), called as a domain's is, through the package's name.
    pub(super) fn package_translation_call(
        &mut self,
        call: &ast::CallExpr,
        callee: &Expr,
        package: &str,
        rest: &[String],
        written: &str,
    ) -> (Ty, CallKind, Reach) {
        let text = Ty::Prim(Prim::PresentationText);
        let home = jadpo_db::symbols(self.db, self.project)
            .homes
            .get(package)
            .and_then(|homes| homes.first())
            .cloned();
        let scope = home
            .as_ref()
            .filter(|home| package_of(self.db, self.project, &home.name).is_some())
            .map(|home| home.name.clone());
        let (Some(scope), [key]) = (scope, rest) else {
            self.args_only(call);
            let span = self.span(callee);
            let reason = if rest.len() == 1 {
                format!(
                    "Package `{package}` carries no translations: a library package's messages are `type X = Message` in its `translations/` folder."
                )
            } else {
                format!(
                    "A package's message is called through the package and the message: `{package}.translations.<message>(...)`."
                )
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TRANSLATION_MESSAGE_UNKNOWN,
                    format!("`{written}` is not a message"),
                )
                .at(span, "no such message")
                .reason(reason)
                .next(format!(
                    "Run `jadpo package show {package}` to see what it offers"
                )),
            );
            return (text, CallKind::Pure(written.to_string()), Reach::Unknown);
        };
        let found = callable(
            self.db,
            self.project,
            Name::new(self.db, scope.clone()),
            Name::new(self.db, key.clone()),
        )
        .clone();
        let path = format!("{scope}.{key}");
        match found {
            Callable::Found { params, symbol, .. } if symbol.public => {
                let reach = self.translation_arguments(call, &path, &params, written);
                (text, CallKind::Pure(written.to_string()), reach)
            }
            Callable::Found { symbol, .. } => {
                self.args_only(call);
                let span = self.span(callee);
                self.out.push(
                    Diagnostic::new(
                        &rule::PACKAGE_NOT_PUBLIC,
                        format!("`{written}` isn't public"),
                    )
                    .at(span, "not part of the package's API")
                    .reason(format!(
                        "A package offers only what it marks `public`; `{}` is a message in `{}` that only the package itself words, and may change in any version.",
                        crate::translations::bare_name(&symbol),
                        symbol.path
                    ))
                    .next(format!("Use a message `{package}` marks `public` instead")),
                );
                (text, CallKind::Pure(written.to_string()), Reach::Unknown)
            }
            Callable::Broken => {
                self.args_only(call);
                let reach = Reach::Translation {
                    key: path,
                    arity: Self::arguments(call).len(),
                };
                (text, CallKind::Pure(written.to_string()), reach)
            }
            Callable::Unknown => {
                self.args_only(call);
                let span = self.span(callee);
                let offered: Vec<String> = crate::translations::subjects(
                    self.db,
                    self.project,
                    Name::new(self.db, scope.clone()),
                )
                .list
                .iter()
                .filter(|subject| subject.callable() && subject.symbol.public)
                .map(|subject| subject.key.clone())
                .collect();
                let near = nearest(key, offered.iter().map(String::as_str)).map(str::to_string);
                let listed: Vec<String> = offered.iter().map(|name| format!("`{name}`")).collect();
                let reason = match &near {
                    Some(_) => format!(
                        "`{key}` is not a public message or enum package `{package}` words.{}",
                        super::did_you_mean(near.as_deref())
                    ),
                    None if offered.is_empty() => {
                        format!("Package `{package}` offers no message to call.")
                    }
                    None => format!(
                        "`{key}` is not a public message or enum package `{package}` words. It offers {}.",
                        crate::and_list(&listed)
                    ),
                };
                let problem = Diagnostic::new(
                    &rule::TRANSLATION_MESSAGE_UNKNOWN,
                    format!("`{written}` is not a message"),
                )
                .at(span.clone(), "no such message")
                .reason(reason);
                self.out.push(match near {
                    Some(near) => problem
                        .next(format!("Use `{package}.translations.{near}`"))
                        .edit(span, format!("{package}.translations.{near}")),
                    None => problem.next(format!(
                        "Run `jadpo package show {package}` to see what it offers"
                    )),
                });
                (text, CallKind::Pure(written.to_string()), Reach::Unknown)
            }
        }
    }

    /// `translations.postcodes.<message>(...)`: a package's message
    /// written as a domain's is, reported with the call to write.
    fn through_package(
        &mut self,
        callee: &Expr,
        written: &str,
        home: &jadpo_db::PackageHome,
        key: &str,
    ) {
        let span = self.span(callee);
        let right = format!("{}.translations.{key}", home.code);
        self.out.push(
            Diagnostic::new(
                &rule::TRANSLATION_MESSAGE_UNKNOWN,
                format!("`{written}` is not a message"),
            )
            .at(span.clone(), "a package's message")
            .reason(format!(
                "`{}` is a package, not a domain: a library package's messages are called through its name in code, as its other names are, `{right}(...)`.",
                home.name
            ))
            .next(format!("Call `{right}(...)`"))
            .edit(span, right),
        );
    }

    /// Whether the code calling may name a message of this scope: a
    /// domain's messages are its own, and `shared/translations/`'s are
    /// everyone's (area 1, "Files and names").
    fn reaches(&mut self, callee: &Expr, scope: &str, symbol: &jadpo_db::Symbol) {
        let span = self.span(callee);
        let name = &symbol.name;
        match (self.ownership, &symbol.ownership) {
            (Ownership::Domain(ours), Ownership::Domain(theirs))
                if ours != theirs && !symbol.public =>
            {
                self.out.push(
                    Diagnostic::new(
                        &rule::VISIBILITY_PRIVATE_CROSS_DOMAIN,
                        format!("`{name}` is private to domain `{theirs}`"),
                    )
                    .at(span, "private")
                    .reason(format!(
                        "A message is worded in the domain that uses it, so another domain may call it only when it is `public`; `{name}` is a message in `{}`.",
                        symbol.path
                    ))
                    .next(format!(
                        "Move `{name}` and its wording to `shared/translations/` and call it as `translations.shared.{}`, or mark it `public`",
                        crate::case::to_snake(name)
                    )),
                );
            }
            (Ownership::Shared, Ownership::Domain(theirs)) => {
                self.out.push(
                    Diagnostic::new(
                        &rule::LAYOUT_CALL_DIRECTION,
                        format!("Shared code uses `{name}` from domain `{theirs}`"),
                    )
                    .at(span, "domain code")
                    .reason(format!(
                        "`shared/` holds code every domain may use, so it can't depend on any one domain; `{name}` is a message in `{}`.",
                        symbol.path
                    ))
                    .next(format!(
                        "Move `{name}` and its wording to `shared/translations/`, or stop `shared/` calling it"
                    )),
                );
            }
            _ => {}
        }
        let _ = scope;
    }

    fn message_unknown(&mut self, callee: &Expr, written: &str, rest: &[String], why: Why) {
        let span = self.span(callee);
        let scope_names: Vec<String> = {
            let table = jadpo_db::symbols(self.db, self.project);
            let mut all: Vec<String> = table.domains.iter().cloned().collect();
            all.push(SHARED.to_string());
            all
        };
        let (reason, next, suggestion) = match why {
            Why::NoScope => {
                let key = rest.first().map_or("message", String::as_str);
                (
                    format!(
                        "A message is called through the domain that words it: `translations.<domain>.{key}(...)`, or `translations.shared.{key}(...)` for a message in `shared/translations/`."
                    ),
                    format!(
                        "Write `translations.<domain>.{key}(...)`, naming the domain whose `translations/` folder declares it"
                    ),
                    None,
                )
            }
            Why::NotAScope => {
                let scope = rest.first().map_or("", String::as_str);
                let near =
                    nearest(scope, scope_names.iter().map(String::as_str)).map(str::to_string);
                (
                    format!(
                        "`{scope}` is neither a domain nor `shared`. A message is called as `translations.<domain>.<message>(...)`.{}",
                        super::did_you_mean(near.as_deref())
                    ),
                    match &near {
                        Some(near) => format!(
                            "Use `translations.{near}.{}`",
                            rest.get(1).map_or("", String::as_str)
                        ),
                        None => "Name the domain whose `translations/` folder declares the message"
                            .to_string(),
                    },
                    near.map(|near| {
                        format!(
                            "translations.{near}.{}",
                            rest.get(1).map_or("", String::as_str)
                        )
                    }),
                )
            }
            Why::NoMessage => {
                let scope = rest.first().map_or("", String::as_str);
                let key = rest.get(1).map_or("", String::as_str);
                let all = callable_keys(self.db, self.project, scope);
                let near = nearest(key, all.iter().map(String::as_str)).map(str::to_string);
                let reason = if all.is_empty() {
                    format!(
                        "`{scope}` has no message or enum with wording: a message is a `type X = Message` in `{}translations/`, and its language files word it.",
                        if scope == SHARED {
                            "shared/".to_string()
                        } else {
                            format!("domains/{scope}/")
                        }
                    )
                } else if near.is_some() {
                    format!(
                        "`{key}` is not a message or enum `{scope}` words.{}",
                        super::did_you_mean(near.as_deref())
                    )
                } else {
                    let listed: Vec<String> = all.iter().map(|name| format!("`{name}`")).collect();
                    format!(
                        "`{key}` is not a message or enum `{scope}` words. It has {}.",
                        crate::and_list(&listed)
                    )
                };
                (
                    reason,
                    match &near {
                        Some(near) => format!("Use `translations.{scope}.{near}`"),
                        None => format!(
                            "Declare `{}` as a `type ... = Message` in `translations/`, and word it in each language file",
                            crate::case::to_camel(key)
                        ),
                    },
                    near.map(|near| format!("translations.{scope}.{near}")),
                )
            }
        };
        let problem = Diagnostic::new(
            &rule::TRANSLATION_MESSAGE_UNKNOWN,
            format!("`{written}` is not a message"),
        )
        .at(span.clone(), "no such message")
        .reason(reason)
        .next(next);
        self.out.push(match suggestion {
            Some(text) => problem.edit(span, text),
            None => problem,
        });
    }

    /// The arguments of a call of a message whose fields are known.
    fn translation_arguments(
        &mut self,
        call: &ast::CallExpr,
        key: &str,
        fields: &[ParamSpec],
        written: &str,
    ) -> Reach {
        // The locale, then each field by name.
        let locale_ty = self
            .table
            .resolve_std(&TypeRef::Path(vec!["Locale".to_string()], Vec::new()));
        let mut params: Vec<(String, Ty)> = vec![("locale".to_string(), locale_ty.clone())];
        params.extend(fields.iter().map(|field| {
            let ty = match &field.enum_type {
                Some(name) => Ty::Named(name.clone()),
                None => Ty::Error,
            };
            (field.name.clone(), ty)
        }));
        let args = Self::arguments(call);
        let mut slots: Vec<Option<usize>> = Vec::new();
        let mut unknown = false;
        let mut filled = vec![false; params.len()];
        for (index, arg) in args.iter().enumerate() {
            let slot = match Self::arg_name(arg) {
                Some(name) => {
                    let found = params.iter().position(|(p, _)| *p == name);
                    if found.is_none()
                        && let Some(node) = arg.name()
                    {
                        unknown = true;
                        self.unknown_argument(written, &name, &params, self.span(&node));
                    }
                    found
                }
                // A value given without its name is a field's, in order;
                // the locale is always named.
                None => (index + 1 < params.len()).then_some(index + 1),
            };
            if let Some(slot) = slot {
                filled[slot] = true;
            }
            slots.push(slot);
        }
        self.facts
            .slots
            .insert(node_key(call.syntax()), slots.clone());
        let mut locale_value = None;
        for (arg, slot) in args.iter().zip(&slots) {
            let Some(value) = arg.expr() else { continue };
            let Some(slot) = slot else {
                if !is_literal(&value) {
                    self.infer(&value);
                }
                continue;
            };
            if *slot == 0 {
                self.check(&value, &locale_ty);
                locale_value = Some(value);
                continue;
            }
            let Some(field) = fields.get(*slot - 1) else {
                continue;
            };
            if field.kind == ParamKind::Choice {
                let wanted = params.get(*slot).map_or(Ty::Error, |(_, ty)| ty.clone());
                self.check(&value, &wanted);
                continue;
            }
            let ty = (!is_literal(&value)).then(|| self.infer(&value));
            self.built_on_argument(&value, ty, accepts(field.kind), written, &field.name);
        }
        let given = args.len();
        let extra = slots.iter().filter(|slot| slot.is_none()).count();
        let missing: Vec<String> = params
            .iter()
            .zip(&filled)
            .filter(|(_, filled)| !**filled)
            .map(|((name, _), _)| format!("`{name}`"))
            .collect();
        if !unknown && (extra > 0 || !missing.is_empty()) {
            let span = self.arg_list_span(call);
            let shown: Vec<String> = params.iter().map(|(name, _)| name.clone()).collect();
            let next = if missing.is_empty() {
                if extra == 1 {
                    "Remove the extra value".to_string()
                } else {
                    format!("Remove the {extra} extra values")
                }
            } else {
                format!("Pass {} as well", crate::and_list(&missing))
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!(
                        "`{written}` takes {}, not {given}",
                        plural(params.len(), "value")
                    ),
                )
                .at(span, format!("{given} given"))
                .reason(format!(
                    "`{written}({})` has {}: the `locale:` and a value for each placeholder of the message, each given once, by name.",
                    shown.join(", "),
                    plural(params.len(), "parameter")
                ))
                .next(next),
            );
        }
        self.locale_value(call, locale_value, written);
        Reach::Translation {
            key: key.to_string(),
            arity: params.len(),
        }
    }
}

/// Why a call names no message.
#[derive(Clone, Copy)]
enum Why {
    /// `translations.order_shipped(...)`: no domain.
    NoScope,
    /// The first name is neither a domain nor `shared`.
    NotAScope,
    /// The domain declares no such message or enum.
    NoMessage,
}
