//! What code records for its telemetry (area 16, "Custom metrics"; chunk
//! O1): `telemetry.<metric>.add(value, label: ...)`, `record` or `set`, a
//! metric its own domain's `monitoring.jadpo` declares, recorded the way
//! its kind is, every label its `by:` lists given once, each an enum or a
//! `Bool`; and `telemetry.note(name: value, ...)`, each attribute text, a
//! number, a `Bool` or an enum. A personal field, or a value of a type
//! that is personal data whatever holds it, is refused as a label or an
//! attribute (`telemetry.personal_data`): it would leave the app for the
//! monitoring service. Everything else is `telemetry.record_invalid`.

use jadpo_db::Ownership;
use jadpo_diag::{Diagnostic, nearest, rule};
use jadpo_std::telemetry::{MetricKind, NOTE, Recording};
use jadpo_syntax::ast::{self, Expr};

use super::super::ty::{Prim, Ty};
use super::facts::Reach;
use super::{Body, CallKind};
use crate::types::Def;

const NOTE_REASON: &str = "`telemetry.note(name: value)` adds an attribute to the request's trace, for a fact no span carries: each value text, a number, a `Bool` or an enum, never a personal field.";

/// What a label or an attribute may be.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Carried {
    /// A metric's label: an enum or a `Bool`.
    Label,
    /// A note's attribute: text, a number, a `Bool` or an enum.
    Attribute,
}

impl Body<'_> {
    /// A `telemetry.` call: see the module.
    pub(super) fn telemetry_call(
        &mut self,
        call: &ast::CallExpr,
        callee: &Expr,
        rest: &[String],
        written: &str,
    ) -> (Ty, CallKind, Reach) {
        let effect = CallKind::Effect(written.to_string());
        let unit = Ty::Prim(Prim::Unit);
        match rest {
            [note] if note == NOTE => match self.note(call, written) {
                Some(recording) => (unit, effect, Reach::Telemetry(recording)),
                None => (unit, effect, Reach::Unknown),
            },
            [metric, operation] => match self.metric_call(call, callee, metric, operation, written)
            {
                Some(recording) => (unit, effect, Reach::Telemetry(recording)),
                None => (unit, effect, Reach::Unknown),
            },
            _ => {
                self.args_only(call);
                let span = self.span(callee);
                self.out.push(
                    Diagnostic::new(
                        &rule::TELEMETRY_RECORD_INVALID,
                        format!("`{written}` isn't something code records"),
                    )
                    .at(span, "not a metric or a note")
                    .reason("Code records a metric its domain declares, `telemetry.<metric>.add(1, label: value)`, and notes a fact on the request's trace, `telemetry.note(name: value)`.")
                    .next("Record a metric declared in the domain's `monitoring.jadpo`, or call `telemetry.note`"),
                );
                (unit, effect, Reach::Unknown)
            }
        }
    }

    fn domain(&self) -> Option<String> {
        match self.ownership {
            Ownership::Domain(domain) => Some(domain.clone()),
            _ => None,
        }
    }

    /// `telemetry.note(name: value, ...)`.
    fn note(&mut self, call: &ast::CallExpr, written: &str) -> Option<Recording> {
        let args = Self::arguments(call);
        if args.is_empty() {
            let span = self.arg_list_span(call);
            self.out.push(
                Diagnostic::new(
                    &rule::TELEMETRY_RECORD_INVALID,
                    format!("`{written}` notes nothing"),
                )
                .at(span, "no attribute")
                .reason(NOTE_REASON)
                .next("Give each attribute by name, such as `telemetry.note(plan: account.plan)`"),
            );
            return None;
        }
        let mut names = Vec::new();
        let mut fine = true;
        for arg in &args {
            let Some(value) = arg.expr() else {
                fine = false;
                continue;
            };
            let Some(name) = arg.name().and_then(|name| crate::tree::name_text(&name)) else {
                let span = self.span(arg);
                self.infer(&value);
                self.out.push(
                    Diagnostic::new(
                        &rule::TELEMETRY_RECORD_INVALID,
                        "A note's attribute needs a name",
                    )
                    .at(span, "no name")
                    .reason(NOTE_REASON)
                    .next("Name the attribute, such as `telemetry.note(plan: account.plan)`"),
                );
                fine = false;
                continue;
            };
            if names.contains(&name) {
                let span = self.span(arg);
                self.out.push(
                    Diagnostic::new(
                        &rule::TELEMETRY_RECORD_INVALID,
                        format!("`{name}` is noted twice"),
                    )
                    .at(span, "given again")
                    .reason(NOTE_REASON)
                    .next(format!("Give `{name}` once")),
                );
                fine = false;
            }
            fine &= self.carried(&value, &name, Carried::Attribute, written);
            names.push(name);
        }
        fine.then_some(Recording::Note { names })
    }

    /// `telemetry.<metric>.<operation>(value, label: ...)`.
    fn metric_call(
        &mut self,
        call: &ast::CallExpr,
        callee: &Expr,
        metric: &str,
        operation: &str,
        written: &str,
    ) -> Option<Recording> {
        let domain = self.domain();
        let metrics = crate::telemetry::metrics(self.db, self.project);
        let Some(declared) = metrics.find(domain.as_deref(), metric).cloned() else {
            self.args_only(call);
            if metrics.declared(domain.as_deref(), metric) {
                return None;
            }
            let span = self.span(callee);
            let names = metrics.names_in(domain.as_deref());
            let near = nearest(metric, names.iter().copied());
            let place = match &domain {
                Some(domain) => format!("`domains/{domain}/{}`", crate::telemetry::FILE),
                None => "this project".to_string(),
            };
            let problem = Diagnostic::new(
                &rule::TELEMETRY_RECORD_INVALID,
                format!("`{metric}` isn't a metric {place} declares"),
            )
            .at(span, "no such metric")
            .reason(format!(
                "Code records the metrics its own domain declares in its `{}`, by name: `metric {metric} = Counter {{ }}`.",
                crate::telemetry::FILE
            ));
            self.out.push(match near {
                Some(near) => problem.next(format!("Use `telemetry.{near}` instead")),
                None => problem.next(format!(
                    "Declare `metric {metric} = Counter {{ }}` in {place}"
                )),
            });
            return None;
        };
        let kind = declared.kind;
        if operation != kind.operation() {
            self.args_only(call);
            let span = self.span(callee);
            self.out.push(
                Diagnostic::new(
                    &rule::TELEMETRY_RECORD_INVALID,
                    format!(
                        "`{metric}` is a `{}`, recorded with `{}`",
                        kind.word(),
                        kind.operation()
                    ),
                )
                .at(span, format!("not `{}`", kind.operation()))
                .reason("A `Counter` only rises, recorded with `add`; a `Measure` is a distribution, recorded with `record`; a `Gauge` is a value at a moment, recorded with `set`.")
                .next(format!("Write `telemetry.{metric}.{}(...)`", kind.operation())),
            );
            return None;
        }
        let args = Self::arguments(call);
        let mut fine = true;
        let value = match args.first() {
            Some(first) if first.name().is_none() => first.expr(),
            _ => None,
        };
        let Some(value) = value else {
            self.args_only(call);
            let span = self.arg_list_span(call);
            self.out.push(
                Diagnostic::new(
                    &rule::TELEMETRY_RECORD_INVALID,
                    format!("`{written}` is given no value"),
                )
                .at(span, "no value first")
                .reason(format!(
                    "A metric is recorded with its value first, then each label it is counted by: `telemetry.{metric}.{}(1, ...)`.",
                    kind.operation()
                ))
                .next("Pass the value first"),
            );
            return None;
        };
        fine &= self.metric_value(&value, kind, metric);
        let mut labels = Vec::new();
        for arg in args.iter().skip(1) {
            let Some(label_value) = arg.expr() else {
                fine = false;
                continue;
            };
            let Some(name) = arg.name().and_then(|name| crate::tree::name_text(&name)) else {
                self.infer(&label_value);
                let span = self.span(arg);
                self.out.push(
                    Diagnostic::new(
                        &rule::TELEMETRY_RECORD_INVALID,
                        format!("`{metric}`'s label needs a name"),
                    )
                    .at(span, "no name")
                    .reason(
                        "After the value, each label is given by its name in the metric's `by:`.",
                    )
                    .next("Name the label, as `by:` lists it"),
                );
                fine = false;
                continue;
            };
            if !declared.labels.contains(&name) || labels.contains(&name) {
                self.infer(&label_value);
                let span = self.span(arg);
                let listed = if declared.labels.is_empty() {
                    "no labels".to_string()
                } else {
                    crate::and_list(
                        &declared
                            .labels
                            .iter()
                            .map(|label| format!("`{label}`"))
                            .collect::<Vec<_>>(),
                    )
                };
                let again = labels.contains(&name);
                self.out.push(
                    Diagnostic::new(
                        &rule::TELEMETRY_RECORD_INVALID,
                        if again {
                            format!("`{metric}` is given `{name}` twice")
                        } else {
                            format!("`{metric}` isn't counted by `{name}`")
                        },
                    )
                    .at(
                        span,
                        if again {
                            "given again"
                        } else {
                            "not in its `by:`"
                        },
                    )
                    .reason(format!(
                        "`{metric}`'s `by:` lists {listed}, each given once."
                    ))
                    .next(if again {
                        format!("Give `{name}` once")
                    } else {
                        format!("Remove `{name}:`, or add it to `{metric}`'s `by:`")
                    }),
                );
                fine = false;
                continue;
            }
            fine &= self.carried(&label_value, &name, Carried::Label, written);
            labels.push(name);
        }
        let missing: Vec<&String> = declared
            .labels
            .iter()
            .filter(|label| !labels.contains(label))
            .collect();
        if !missing.is_empty() && fine {
            let span = self.arg_list_span(call);
            let names =
                crate::and_list(&missing.iter().map(|m| format!("`{m}`")).collect::<Vec<_>>());
            self.out.push(
                Diagnostic::new(
                    &rule::TELEMETRY_RECORD_INVALID,
                    format!("`{written}` doesn't give {names}"),
                )
                .at(span, "a label missing")
                .reason(format!(
                    "Every value of `{metric}` is counted by each label its `by:` lists, so each is given."
                ))
                .next(format!(
                    "Add {}",
                    missing
                        .iter()
                        .map(|m| format!("`{m}: ...`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            );
            fine = false;
        }
        fine.then(|| Recording::Metric {
            metric: declared.sent_as(),
            kind,
            labels,
        })
    }

    /// Whether `value` is what a metric of `kind` records: a whole number
    /// a `Counter` adds, a number or a duration a `Measure` records, a
    /// number a `Gauge` is set to.
    fn metric_value(&mut self, value: &Expr, kind: MetricKind, metric: &str) -> bool {
        let ty = self.infer(value);
        if matches!(ty, Ty::Error) {
            return false;
        }
        let prim = self.table.base_prim(&ty);
        let takes: &[Prim] = match kind {
            MetricKind::Counter => &[Prim::Int],
            MetricKind::Measure => &[Prim::Int, Prim::Decimal, Prim::Duration],
            MetricKind::Gauge => &[Prim::Int, Prim::Decimal],
        };
        if prim.is_some_and(|prim| takes.contains(&prim)) {
            return true;
        }
        let span = self.span(value);
        let wanted = match kind {
            MetricKind::Counter => "a whole number, `Int`",
            MetricKind::Measure => "an `Int`, a `Decimal` or a `Duration`",
            MetricKind::Gauge => "an `Int` or a `Decimal`",
        };
        self.out.push(
            Diagnostic::new(
                &rule::TELEMETRY_RECORD_INVALID,
                format!("`{metric}` can't record a `{ty}`"),
            )
            .at(span, format!("a `{ty}`"))
            .reason(format!("A `{}` records {wanted}.", kind.word()))
            .next(format!("Pass {wanted}")),
        );
        false
    }

    /// Whether `value`, given as label or attribute `name`, is one it may
    /// carry, and no personal field.
    fn carried(&mut self, value: &Expr, name: &str, carried: Carried, written: &str) -> bool {
        let ty = self.infer(value);
        if matches!(ty, Ty::Error) {
            return false;
        }
        if let Some(personal) = self.personal(value, &ty) {
            let span = self.span(value);
            let what = match carried {
                Carried::Label => "a label",
                Carried::Attribute => "an attribute",
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TELEMETRY_PERSONAL_DATA,
                    format!("`{written}` would send {personal} as {what}"),
                )
                .at(span, "personal data")
                .reason("A metric's labels and a note's attributes go to the monitoring service as they are, with nothing hashed, so a personal field is never one: the people it is about never agreed to it leaving the app this way.")
                .next(format!("Give `{name}:` something that isn't personal data, or remove it")),
            );
            return false;
        }
        let plain_enum = self.plain_enum(&ty);
        let prim = self.table.base_prim(&ty);
        let fits = match carried {
            Carried::Label => plain_enum || prim == Some(Prim::Bool),
            Carried::Attribute => {
                plain_enum
                    || matches!(
                        prim,
                        Some(Prim::Bool | Prim::Int | Prim::Decimal | Prim::Text)
                    )
            }
        };
        if fits {
            return true;
        }
        let span = self.span(value);
        let (title, reason, next) = match carried {
            Carried::Label => (
                format!("A label can't be a `{ty}`"),
                "A metric's labels are enums or `Bool`, so the number of series it makes stays bounded.".to_string(),
                format!("Give `{name}:` an enum or a `Bool`"),
            ),
            Carried::Attribute => (
                format!("A note can't carry a `{ty}`"),
                NOTE_REASON.to_string(),
                format!("Give `{name}:` text, a number, a `Bool` or an enum"),
            ),
        };
        self.out.push(
            Diagnostic::new(&rule::TELEMETRY_RECORD_INVALID, title)
                .at(span, format!("a `{ty}`"))
                .reason(reason)
                .next(next),
        );
        false
    }

    /// Whether `ty` is an enum whose variants carry no data.
    fn plain_enum(&self, ty: &Ty) -> bool {
        match self.table.structure(ty) {
            Ty::Named(name) => matches!(
                self.table.def(&name),
                Some(Def::Enum { variants }) if variants.iter().all(|v| v.fields.is_none())
            ),
            _ => false,
        }
    }

    /// What `value` is that is personal data, said for a message: a field
    /// marked `personal:` read off a record, or a value of a type that is
    /// personal data whatever holds it.
    fn personal(&mut self, value: &Expr, ty: &Ty) -> Option<String> {
        if let Expr::FieldExpr(field) = value
            && let Some(receiver) = field.expr()
            && let Some(name) = field
                .name_ref()
                .and_then(|name| crate::tree::name_ref_text(&name))
        {
            let host = self.infer(&receiver);
            if let Ty::Named(host) = self.table.structure(&host) {
                let marks = crate::ai_personal::marks(self.db, self.project);
                if marks
                    .mark(&host, &name)
                    .is_some_and(|mark| mark.category.is_some())
                {
                    return Some(format!("`{host}.{name}`, marked `personal:`,"));
                }
            }
        }
        let standard = crate::ai_personal::standard_personal(self.table, ty);
        standard.first().map(|(name, category)| {
            format!(
                "{}, which is `PersonalData.{category}`,",
                crate::ai_personal::a_type(name)
            )
        })
    }
}
