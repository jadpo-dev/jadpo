//! What `temporal`'s calls add to an ordinary standard-library call (area
//! 10, "Zones and local times"): a clock time written out, options that
//! mean something only for a local time, and the conditions a call fails
//! on unless an option settles them, which its `attempt` maps by the
//! option's name as a construction's broken rules are mapped (area 2).

use jadpo_diag::{Diagnostic, Span, rule};
use jadpo_std::Operation;
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::rules::Const;
use super::super::ty::{Prim, Ty};
use super::{Body, Fallible, is_literal, node_key};
use crate::tree;

impl Body<'_> {
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
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_TEMPORAL_OPTION_UNUSED,
                    format!("`{name}:` means nothing for a `{shown}`"),
                )
                .at(span, format!("`{shown}` has no clock time"))
                .reason(format!(
                    "`{name}:` chooses an instant when a local time falls in a daylight-saving gap or overlap; `{written}` on a `{shown}` moves a date, which has no clock time, so no such time arises."
                ))
                .next(format!("Remove `{name}: ...`"))
                .edit(removal, ""),
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
