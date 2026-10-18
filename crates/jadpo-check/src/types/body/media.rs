//! A `Timecode` or a `SubtitleCue` literal's rules across its parts (area
//! 19, chunk 5.5p), proved while compiling where the literal writes the
//! parts a rule reads: a timecode's frame below its rate, drop-frame only
//! at 29.97 and 59.94 and never on a label it skips; a cue starting at 0 or
//! after, ending after it starts, and with no `-->` in its text. A part
//! built from a value waits on cross-field rules (chunk 5.5t), as an
//! address's postal code built from values does. The rules are
//! `jadpo-std`'s, which the runtime checks a value read from text or JSON
//! by.

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::standard::media::{SUBTITLE_CUE, TIMECODE, cue_broken, timecode_broken};
use jadpo_syntax::ast::{self, Expr};

use super::super::rules::Const;
use super::Body;
use crate::tree;

/// The value a literal writes for part `part`, if it writes the part.
fn value_of(record: &ast::RecordExpr, part: &str) -> Option<Expr> {
    tree::members(record.member_list())
        .into_iter()
        .find(|(field, _)| tree::name_text(field).as_deref() == Some(part))
        .and_then(|(_, member)| member.value())
}

/// A whole number the literal writes for `part`.
fn whole(record: &ast::RecordExpr, part: &str) -> Option<i64> {
    match Const::from_expr(&value_of(record, part)?) {
        Const::Number { value, .. } if value.scale == 0 => i64::try_from(value.mantissa).ok(),
        _ => None,
    }
}

/// A frame rate the literal writes as a `FrameRate` literal or its text.
fn rate(record: &ast::RecordExpr) -> Option<(i64, i64)> {
    match value_of(record, "rate")? {
        Expr::RecordExpr(rate) => Some((whole(&rate, "numerator")?, whole(&rate, "denominator")?)),
        written => match Const::from_expr(&written) {
            Const::Str(text) => jadpo_std::percentage::parse_fraction(&text),
            _ => None,
        },
    }
}

/// A duration the literal writes for `part`, in milliseconds.
fn millis(record: &ast::RecordExpr, part: &str) -> Option<i64> {
    match Const::from_expr(&value_of(record, part)?) {
        Const::Duration(text) => {
            jadpo_std::Dur::parse(&text).and_then(|duration| i64::try_from(duration.millis).ok())
        }
        _ => None,
    }
}

impl Body<'_> {
    /// A `Timecode` or a `SubtitleCue` literal's rules across its parts,
    /// where it writes them.
    pub(super) fn media_rules_hold(&mut self, name: &str, record: &ast::RecordExpr) {
        let broken = match name {
            TIMECODE => timecode_broken(
                whole(record, "minutes"),
                whole(record, "seconds"),
                whole(record, "frames"),
                rate(record),
                value_of(record, "drop_frame").and_then(|written| {
                    match Const::from_expr(&written) {
                        Const::Bool(flag) => Some(flag),
                        _ => None,
                    }
                }),
            ),
            SUBTITLE_CUE => {
                let text =
                    value_of(record, "text").and_then(|written| match Const::from_expr(&written) {
                        Const::Str(text) => Some(text),
                        _ => None,
                    });
                cue_broken(
                    millis(record, "start"),
                    millis(record, "end"),
                    text.as_deref(),
                )
            }
            _ => return,
        };
        let Some(broken) = broken else { return };
        let Some(written) = value_of(record, broken.part) else {
            return;
        };
        let what = if name == TIMECODE { "timecode" } else { "cue" };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_INVALID_LITERAL,
                format!(
                    "This {what}'s `{}` breaks a rule across its parts",
                    broken.part
                ),
            )
            .at(self.span(&written), broken.rule.label)
            .reason(broken.rule.reason)
            .next(broken.rule.next),
        );
    }
}
