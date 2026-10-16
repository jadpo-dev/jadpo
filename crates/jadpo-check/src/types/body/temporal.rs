//! What `temporal`'s calls add to an ordinary standard-library call (area
//! 10, "Zones and local times" and "Showing dates and times to people"): a
//! clock time and a format written out, options that mean something only
//! for some values and one that those values need, and the conditions a
//! call fails on unless an option settles them, which its `attempt` maps
//! by the option's name as a construction's broken rules are mapped (area
//! 2).

use jadpo_diag::{Diagnostic, Span, nearest, rule};
use jadpo_std::{BuiltinFormat, Operation, TimeFormatSpec};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::rules::Const;
use super::super::table::Def;
use super::super::ty::{Prim, TIME_FORMAT, Ty};
use super::{Body, Fallible, did_you_mean, is_literal, node_key};
use crate::case::starts_upper;
use crate::tree::{self, dotted};

/// Why an option means nothing for the value a call is given.
fn unused_reason(option: &str, written: &str, shown: &str, built_on: Prim) -> (String, String) {
    match option {
        "zone" if built_on == Prim::ZonedTime => (
            format!("`{shown}` is seen in its own zone"),
            format!(
                "`zone:` says which zone an `Instant` is shown in; `{shown}` is a `ZonedTime`, which carries the zone it is seen in, so `{written}` shows it there."
            ),
        ),
        "zone" => (
            format!("`{shown}` has no clock time"),
            format!(
                "`zone:` says which zone an `Instant` is shown in; `{shown}` is a date, with no clock time, so it reads the same in every zone."
            ),
        ),
        _ => (
            format!("`{shown}` has no clock time"),
            format!(
                "`{option}:` chooses an instant when a local time falls in a daylight-saving gap or overlap; `{written}` on a `{shown}` moves a date, which has no clock time, so no such time arises."
            ),
        ),
    }
}

impl Body<'_> {
    /// `locale:` on a format: the application declares the locales it
    /// supports (area 10, "Locales"), and a locale written out is one of
    /// them. A `Locale` passed in is a supported one by how it is read.
    pub(super) fn locale_argument(
        &mut self,
        call: &ast::CallExpr,
        args: &[ast::Arg],
        slots: &[Option<usize>],
        table: &Operation,
        written: &str,
    ) {
        let locales = crate::locales::locales(self.db, self.project);
        if !locales.declared {
            // Files handed to the compiler on their own have no
            // application to declare locales (area 1).
            if self.project.layout(self.db) == jadpo_db::Layout::Project {
                let span = call
                    .expr()
                    .map_or_else(|| self.span(call), |callee| self.span(&callee));
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_TEMPORAL_LOCALES_REQUIRED,
                        format!("`{written}` needs the application's locales"),
                    )
                    .at(span, "no locales declared")
                    .reason("Text for a person is written in one of the locales the application supports. None is declared, so there is nothing to check `locale:` against, and nothing for a request with no usable `Accept-Language` to fall back to.")
                    .next("Declare them in `app.jadpo`, for instance `locales { default: Locale.en_gb supported: [Locale.en_gb] }`"),
                );
            }
            return;
        }
        let Some(plan) = &locales.plan else { return };
        let Some(slot) = table.params.iter().position(|p| p.name == "locale") else {
            return;
        };
        let Some(value) = args
            .iter()
            .zip(slots)
            .find(|(_, at)| **at == Some(slot))
            .and_then(|(arg, _)| arg.expr())
        else {
            return;
        };
        let Some((ordinal, variant)) = crate::locales::literal_locale(&value) else {
            return;
        };
        if plan.supports(ordinal) {
            return;
        }
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_LOCALE_UNSUPPORTED,
                format!("`Locale.{variant}` isn't a locale this application supports"),
            )
            .at(self.span(&value), "not in `locales`")
            .reason(format!(
                "`{written}` writes text in the locales listed in `locales {{ supported: }}`, in `app.jadpo`; `Locale.{variant}` is not one of them, and the server never guesses a language."
            ))
            .next(format!(
                "Add `Locale.{variant}` to `supported:` in `app.jadpo`, or use a locale that is listed"
            )),
        );
    }

    /// `at:`: a clock time written as text, `"09:30"`, checked here, since
    /// no value holds a time of day on its own (area 10, "Time types").
    pub(super) fn clock_argument(&mut self, value: &Expr, written: &str) {
        let span = self.span(value);
        let shown = tree::written(value.syntax());
        if is_literal(value) {
            if let Const::Str(text) = Const::from_expr(value)
                && jadpo_std::clock_millis(&text).is_some()
            {
                self.check(value, &Ty::Prim(Prim::Text));
                return;
            }
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_TEMPORAL_CLOCK_INVALID,
                    format!("`{shown}` isn't a clock time"),
                )
                .at(span, "not `HH:MM`")
                .reason(format!(
                    "`{written}`'s `at:` is a clock time written `HH:MM`, or `HH:MM:SS` with seconds, each part two digits, from `00:00` to `23:59`."
                ))
                .next("Write the time with two-digit hours and minutes, such as `\"09:30\"`"),
            );
            return;
        }
        self.infer(value);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_TEMPORAL_CLOCK_INVALID,
                format!("`{written}`'s `at:` is written out, not passed as a value"),
            )
            .at(span, "a value")
            .reason(
                "There is no time-of-day type, so no value holds a clock time on its own: `at:` is the clock time written as text, `\"09:30\"`, which is checked while compiling.",
            )
            .next("Write the clock time itself, such as `\"09:30\"`"),
        );
    }

    /// `format:`: a built-in format, `TimeFormat.date_long`, or a declared
    /// one by its name, written out, since no value holds a format (area
    /// 10, "Showing dates and times to people"). The argument is lowered
    /// as the format's text.
    pub(super) fn format_argument(
        &mut self,
        value: &Expr,
        written: &str,
    ) -> Option<TimeFormatSpec> {
        let span = self.span(value);
        let shown = tree::written(value.syntax());
        let names = dotted(value).filter(|(first, _)| starts_upper(&super::text(first)));
        let Some((first, rest)) = names else {
            if !is_literal(value) {
                self.infer(value);
            }
            let message = if is_literal(value) {
                format!("`{shown}` isn't a time format")
            } else {
                format!("`{written}`'s `format:` is written out, not passed as a value")
            };
            self.out.push(
                Diagnostic::new(&rule::TYPE_TEMPORAL_FORMAT_INVALID, message)
                    .at(span, if is_literal(value) { "not a format" } else { "a value" })
                    .reason(format_reason(written))
                    .next("Write the format itself, such as `TimeFormat.date_long`, or a format's name"),
            );
            return None;
        };
        let before = self.resolved.len() + self.out.len();
        self.resolved_by(|names, out| names.value_name(value, out));
        if self.resolved.len() + self.out.len() != before {
            return None;
        }
        let head = super::text(&first);
        let rest: Vec<String> = rest.iter().map(super::text).collect();
        let found = match rest.as_slice() {
            [] => match self.table.def(&head) {
                Some(Def::Format { spec }) => Some(spec.clone()),
                _ => None,
            },
            [member] if head == TIME_FORMAT => {
                BuiltinFormat::from_name(member).map(TimeFormatSpec::Builtin)
            }
            _ => None,
        };
        if let Some(spec) = found {
            let key = node_key(value.syntax());
            self.facts.types.insert(key, Ty::Prim(Prim::Text));
            self.facts.formats.insert(key, spec.written());
            return Some(spec);
        }
        let near = match rest.as_slice() {
            [member] if head == TIME_FORMAT => nearest(
                member,
                BuiltinFormat::ALL.iter().map(|format| format.name()),
            )
            .map(|near| format!("{TIME_FORMAT}.{near}")),
            _ => None,
        };
        let problem = Diagnostic::new(
            &rule::TYPE_TEMPORAL_FORMAT_INVALID,
            format!("`{shown}` isn't a time format"),
        )
        .at(span.clone(), "not a format")
        .reason(format!(
            "{}{}",
            format_reason(written),
            did_you_mean(near.as_deref())
        ));
        self.out.push(match near {
            Some(near) => problem
                .next(format!("Use `{near}`"))
                .edit(span, near),
            None => problem.next(
                "Write a built-in format such as `TimeFormat.date_long`, or declare one with `type Name = TimeFormat { ... }`",
            ),
        });
        None
    }

    /// A format that shows a clock time or a zone, given a value with
    /// neither.
    pub(super) fn format_needs_time(
        &mut self,
        value: &Expr,
        format: &Expr,
        spec: &TimeFormatSpec,
        written: &str,
    ) {
        let Some(ty) = self.facts.types.get(&node_key(value.syntax())).cloned() else {
            return;
        };
        if ty.has_error()
            || self.table.representation(&ty) != Some(Prim::CalendarDate)
            || !spec.needs_time()
        {
            return;
        }
        let shown = tree::written(format.syntax());
        let what = match spec {
            TimeFormatSpec::Builtin(_) => "a clock time".to_string(),
            TimeFormatSpec::Components(parts) => {
                let listed: Vec<String> = parts
                    .iter()
                    .filter(|(component, _)| component.needs_time())
                    .map(|(component, _)| format!("`{}`", component.name()))
                    .collect();
                crate::and_list(&listed)
            }
        };
        let span = self.span(format);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_TEMPORAL_FORMAT_REQUIRES_TIME,
                format!("`{shown}` shows a clock time, which `{ty}` doesn't have"),
            )
            .at(span, format!("shows {what}"))
            .reason(format!(
                "`{ty}` is a date, with no clock time or zone, so `{written}` can show only its weekday, day, month and year."
            ))
            .next(
                "Use a date format such as `TimeFormat.date_long`, or format an `Instant` or a `ZonedTime` instead",
            ),
        );
    }

    /// The options a call gives that mean nothing for its value, and the
    /// conditions it can fail on that no option settles.
    pub(super) fn std_options(
        &mut self,
        table: &Operation,
        call: &ast::CallExpr,
        args: &[ast::Arg],
        slots: &[Option<usize>],
        written: &str,
    ) {
        // What the first argument is built on, which `dst:` depends on.
        let first = args
            .iter()
            .zip(slots)
            .find(|(_, slot)| **slot == Some(0))
            .and_then(|(arg, _)| arg.expr())
            .and_then(|value| self.facts.types.get(&node_key(value.syntax())).cloned());
        let built_on = first
            .as_ref()
            .filter(|ty| !ty.has_error())
            .and_then(|ty| self.table.representation(ty));
        let given = |slot: usize| {
            args.iter()
                .zip(slots)
                .find(|(_, at)| **at == Some(slot))
                .map(|(arg, _)| arg)
        };
        for (slot, param) in table.params.iter().enumerate() {
            let (Some(needs), Some(arg), Some(built_on)) = (param.only_for, given(slot), built_on)
            else {
                continue;
            };
            if built_on == needs {
                continue;
            }
            let name = param.name;
            let span = arg
                .name()
                .map_or_else(|| self.span(arg), |name| self.span(&name));
            let removal = self.argument_removal(arg);
            let shown = first.as_ref().map(ToString::to_string).unwrap_or_default();
            let (label, reason) = unused_reason(name, written, &shown, built_on);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_TEMPORAL_OPTION_UNUSED,
                    format!("`{name}:` means nothing for a `{shown}`"),
                )
                .at(span, label)
                .reason(reason)
                .next(format!("Remove `{name}: ...`"))
                .edit(removal, ""),
            );
        }
        // An option the value needs, as `zone:` for an `Instant` shown to
        // a person.
        for (slot, param) in table.params.iter().enumerate() {
            let (true, Some(needs), Some(built_on)) = (param.needed, param.only_for, built_on)
            else {
                continue;
            };
            if built_on != needs || given(slot).is_some() {
                continue;
            }
            let shown = first.as_ref().map(ToString::to_string).unwrap_or_default();
            let span = call
                .expr()
                .map_or_else(|| self.span(call), |callee| self.span(&callee));
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_TEMPORAL_ZONE_REQUIRED,
                    format!("`{written}` needs the zone to show `{shown}` in"),
                )
                .at(span, "no `zone:`")
                .reason(format!(
                    "An `Instant` is one moment for everyone, and its date and clock time depend on where it is read; the server never guesses a zone, so `{}:` says which.",
                    param.name
                ))
                .next(format!(
                    "Add `{}: ...` with the reader's zone, such as `{}: Zone.europe_london`",
                    param.name, param.name
                )),
            );
        }
        let mut open: Vec<String> = Vec::new();
        for condition in table.conditions {
            let Some(slot) = table
                .params
                .iter()
                .position(|param| param.name == condition.option)
            else {
                continue;
            };
            let param = &table.params[slot];
            if param.only_for.is_some_and(|needs| built_on != Some(needs)) {
                continue;
            }
            let settled = given(slot)
                .and_then(ast::Arg::expr)
                .is_some_and(|value| settles(&value, param.kind));
            if !settled {
                open.push(condition.key.word().to_string());
            }
        }
        if !open.is_empty() {
            self.facts
                .constructions
                .insert(node_key(call.syntax()), Fallible::Construction(open));
        }
    }

    /// `clock.advance(duration)`: a test moves its own clock forward, and
    /// nothing else moves any clock (area 13, "Controlling time").
    pub(super) fn advance_clock(
        &mut self,
        call: &ast::CallExpr,
        callee: &Expr,
        written: &str,
    ) -> (Ty, super::CallKind, super::facts::Reach) {
        if !self.test {
            let span = self.span(callee);
            let a_or_an = if self.word.starts_with(['a', 'e', 'i', 'o', 'u']) {
                "An"
            } else {
                "A"
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_TEST_CLOCK_CONTEXT,
                    format!("{a_or_an} {} can't move the clock", self.word),
                )
                .at(span, "moves the clock")
                .reason("Only a test moves its clock: elsewhere the time is the operation's `clock.now`, fixed while it runs, and a later request or schedule run reads a later one.")
                .next("Move the clock in a test with `clock.advance(...)`, or remove this line"),
            );
            self.args_only(call);
            return (
                Ty::Prim(Prim::Unit),
                super::CallKind::Effect(written.to_string()),
                super::facts::Reach::Unknown,
            );
        }
        let args = Self::arguments(call);
        if let [arg] = args.as_slice()
            && arg.name().is_none()
            && let Some(value) = arg.expr()
        {
            let inferred = (!is_literal(&value)).then(|| self.infer(&value));
            self.built_on_argument(&value, inferred, &[Prim::Duration], written, "by");
        } else {
            self.args_only(call);
            let span = self.arg_list_span(call);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!("`{written}` takes one `Duration`"),
                )
                .at(span, format!("{} given", args.len()))
                .reason("`clock.advance(1h)` moves the test's clock on by one duration.")
                .next("Pass how far to move the clock, such as `1h`"),
            );
        }
        (
            Ty::Prim(Prim::Unit),
            super::CallKind::Effect(written.to_string()),
            super::facts::Reach::AdvanceClock,
        )
    }

    /// The span that removes an argument, its comma with it.
    fn argument_removal(&self, arg: &ast::Arg) -> Span {
        let node = arg.syntax();
        let start = node
            .first_token()
            .and_then(|token| {
                std::iter::successors(token.prev_token(), |t| t.prev_token())
                    .find(|t| !t.kind().is_trivia())
            })
            .filter(|token| token.kind() == jadpo_syntax::SyntaxKind::COMMA)
            .map_or_else(
                || u32::from(node.text_range().start()),
                |token| u32::from(token.text_range().start()),
            );
        Span::new(self.path, start, u32::from(node.text_range().end()))
    }
}

/// What `format:` takes.
fn format_reason(written: &str) -> String {
    format!(
        "`{written}`'s `format:` is written out: a built-in format, `TimeFormat.` followed by `date_`, `time_` or `date_time_` and `full`, `long`, `medium` or `short`, or the name of a format declared as `type Name = TimeFormat {{ ... }}`."
    )
}

/// Whether an option is written as one of its enum's values other than
/// `reject`, which is the only way it settles its condition while
/// compiling: a value chosen at runtime might be `reject`.
fn settles(value: &Expr, kind: jadpo_std::Kind) -> bool {
    let jadpo_std::Kind::Enum(name) = kind else {
        return false;
    };
    let written = tree::written(value.syntax());
    let Some((host, variant)) = written.split_once('.') else {
        return false;
    };
    host == name && variant != "reject" && jadpo_std::variant_ordinal(name, variant).is_some()
}
