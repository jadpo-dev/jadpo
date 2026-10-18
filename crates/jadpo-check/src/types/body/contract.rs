//! What the standard type contract's calls add to an ordinary
//! standard-library call (area 19, "Every standard type meets one
//! contract"): `text.format`, `text.from` and `text.parse` each take every
//! standard type, and which types fit, which options a type takes and
//! needs, and what each call can fail with are read from the types' data
//! entries (`jadpo_std::standard`), never written per type here.

use jadpo_diag::{Diagnostic, rule};
use jadpo_std::standard::{Holds, StandardType, standard_type};
use jadpo_std::{Op, Operation, RuleKey};
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::super::table::Def;
use super::super::ty::{Prim, Ty, TypeRef};
use super::{Body, Fallible, node_key};
use crate::tree;

/// What a value is to the contract's operations.
enum Contract {
    /// A standard type, or a type built on one.
    Standard(&'static StandardType),
    /// A representation JSON carries as text (`Int`, `Decimal`, `Bool`),
    /// or text itself: what `text.from` and `text.parse` take besides the
    /// standard types (area 19, "Text").
    Representation(Prim),
    /// Neither, with why, after "`X` is".
    Neither(String),
}

impl Body<'_> {
    /// What `ty` is to the contract.
    fn contract_of(&self, ty: &Ty) -> Contract {
        // A payment or identity number is text to the runtime, but a
        // standard type to the contract (area 19, chunk 5.5i).
        if let Some(entry) = self.table.standard_text(ty) {
            return Contract::Standard(entry);
        }
        match self.table.structure(ty) {
            Ty::Prim(prim) => match jadpo_std::standard::of_builtin(prim) {
                Some(entry) => Contract::Standard(entry),
                None if matches!(prim, Prim::Int | Prim::Decimal | Prim::Bool | Prim::Text) => {
                    Contract::Representation(prim)
                }
                None => Contract::Neither(format!("built on `{}`", prim.name())),
            },
            Ty::Named(name) | Ty::Opaque(name, _) => match standard_type(&name) {
                Some(entry) => Contract::Standard(entry),
                None => match self.table.def(&name) {
                    Some(Def::Enum { .. }) => Contract::Neither(
                        "an enum the application declares, which has no text of its own (area 2): showing one is a `match`".to_string(),
                    ),
                    _ => Contract::Neither("an object, which has no single text form".to_string()),
                },
            },
            other => Contract::Neither(format!("`{other}`")),
        }
    }

    /// The value given to `text.format` or `text.from`, when it is one the
    /// call takes, with the standard type it is; a representation, such as
    /// a number `text.format` shows (area 19, "Numbers"), is no standard
    /// type.
    pub(super) fn contract_value(
        &mut self,
        op: Op,
        value: &Expr,
        ty: Option<Ty>,
        written: &str,
    ) -> Option<Option<&'static StandardType>> {
        let ty = match ty {
            Some(ty) => ty,
            None => self.infer(value),
        };
        if ty.has_error() {
            return None;
        }
        // A range is shown as its ends are, with their options (area 19).
        if op == Op::TextFormat
            && !self.table.is_nullable(&ty)
            && let Some(item) = super::calls::range_item(&self.table.structure(&ty))
            && standard_type(jadpo_std::RANGE).is_some_and(|entry| {
                matches!(self.contract_of(&ty), Contract::Standard(found) if found == entry)
            })
        {
            return match self.contract_of(&item) {
                Contract::Standard(entry) => Some(Some(entry)),
                // A number, in the locale's digits, with no options.
                Contract::Representation(Prim::Int | Prim::Decimal) => Some(None),
                Contract::Representation(prim) => {
                    self.out.push(
                        Diagnostic::new(
                            &rule::TYPE_MISMATCH,
                            format!("`{ty}` can't be used where a standard type's value is required"),
                        )
                        .at(self.span(value), format!("this is `{ty}`"))
                        .reason(format!(
                            "`{written}` shows a range as its ends are shown; `{ty}`'s ends are built on `{}`, which has no format of its own yet.",
                            prim.name()
                        ))
                        .next("Pass a standard type's value here"),
                    );
                    None
                }
                Contract::Neither(_) => None,
            };
        }
        let (fits, entry, why) = match (op, self.contract_of(&ty)) {
            (_, _) if self.table.is_nullable(&ty) => (
                false,
                None,
                format!("`{ty}` may be `none`, and `{written}` takes a value"),
            ),
            (Op::TextFrom, Contract::Standard(entry)) if entry.text.is_none() => (
                false,
                None,
                format!("`{ty}` has no single text form, so it has no canonical text"),
            ),
            // A value built on `Text` already is text (area 19, "Text").
            (Op::TextFrom, Contract::Standard(entry))
                if matches!(entry.holds, Holds::Text { .. })
                    || matches!(entry.holds, Holds::Builtin(builtin) if builtin.representation() == Prim::Text) =>
            {
                (
                    false,
                    None,
                    format!(
                        "`{ty}` is built on `Text` already, and a parameter written `Text` takes it as it is"
                    ),
                )
            }
            (_, Contract::Standard(entry)) => (true, Some(entry), String::new()),
            (Op::TextFrom, Contract::Representation(Prim::Text)) => (
                false,
                None,
                format!("`{ty}` is built on `Text`, so it is text already"),
            ),
            (Op::TextFrom, Contract::Representation(_)) => (true, None, String::new()),
            // A number for people, in the locale's digits and separators.
            (Op::TextFormat, Contract::Representation(Prim::Int | Prim::Decimal)) => {
                (true, None, String::new())
            }
            (_, Contract::Representation(prim)) => (
                false,
                None,
                format!(
                    "`{ty}` is built on `{}`, which has no format of its own yet",
                    prim.name()
                ),
            ),
            (_, Contract::Neither(what)) => (false, None, format!("`{ty}` is {what}")),
        };
        if !fits {
            let wanted = match op {
                Op::TextFrom => "a value with a canonical text",
                _ => "a standard type's value",
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{ty}` can't be used where {wanted} is required"),
                )
                .at(self.span(value), format!("this is `{ty}`"))
                .reason(format!(
                    "`{written}` takes a standard type's value, or a value of a type built on one{}; {why}.",
                    if op == Op::TextFrom {
                        ", an `Int`, a `Decimal` or a `Bool`"
                    } else {
                        ""
                    }
                ))
                .next(format!("Pass {wanted} here")),
            );
            return None;
        }
        Some(entry)
    }

    /// `as:` on `text.parse`: a type written out, a standard type or one
    /// built on a standard type or on `Text`, `Int`, `Decimal` or `Bool`.
    /// The operation is given the name of the standard type or
    /// representation whose text it reads, and the call gives the type.
    pub(super) fn type_argument(&mut self, value: &Expr, written: &str) -> Option<Ty> {
        let target = match TypeRef::from_expr(value) {
            // `as: Range<CalendarDate>` (area 19).
            TypeRef::Path(names, args)
                if names.len() == 1 && names[0] == jadpo_std::RANGE && args.len() == 1 =>
            {
                Some(self.table.resolve(&TypeRef::Path(names, args)))
            }
            TypeRef::Path(names, args) if args.is_empty() && !names.is_empty() => {
                let head_is_type = names
                    .first()
                    .is_some_and(|name| name.starts_with(|c: char| c.is_ascii_uppercase()));
                if head_is_type {
                    self.resolved_by(|names, out| names.value_name(value, out));
                    Some(self.table.resolve(&TypeRef::Path(names, args)))
                } else {
                    None
                }
            }
            _ => None,
        };
        let Some(target) = target else {
            if !super::is_literal(value) {
                self.infer(value);
            }
            let shown = tree::written(value.syntax());
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{written}`'s `as:` is a type written out, not `{shown}`"),
                )
                .at(self.span(value), "not a type")
                .reason(format!(
                    "`{written}` reads text as the type `as:` names, which is written out, such as `as: Email`: a value can't say which type to read."
                ))
                .next("Write the type itself, such as `as: Email`"),
            );
            return None;
        };
        if target.has_error() {
            return None;
        }
        // A range is read end by end, as its item type: a standard type or
        // a representation itself, whose canonical text is all that is
        // checked, since a range of a type with rules of its own has no
        // construction to check them (area 19).
        if let Some(item) = super::calls::range_item(&target)
            && let Ty::Opaque(name, _) = &target
            && name == jadpo_std::RANGE
        {
            if !self.range_ordered(&item) {
                self.out
                    .push(super::range_unordered(&item, self.span(value)));
                return None;
            }
            let reads = match (&item, self.contract_of(&item)) {
                (Ty::Prim(_), Contract::Standard(entry)) => Some(entry.name),
                (Ty::Prim(_), Contract::Representation(prim)) => Some(prim.name()),
                _ => None,
            };
            let Some(reads) = reads else {
                self.out.push(
                    Diagnostic::new(
                        &rule::TYPE_MISMATCH,
                        format!("`{written}` can't read text as `{target}`"),
                    )
                    .at(self.span(value), format!("`{item}` has rules of its own"))
                    .reason(format!(
                        "`{written}` reads a range's ends by their canonical text alone, so it reads a range of a standard type or a representation, such as `Range<Int>`; `{item}`'s rules would go unchecked."
                    ))
                    .next(format!(
                        "Read a range of what `{item}` is built on, such as `as: Range<{}>`",
                        self.table
                            .representation(&item)
                            .map_or("Int", |prim| prim.name())
                    )),
                );
                return None;
            };
            let key = node_key(value.syntax());
            self.facts.types.insert(key, Ty::Prim(Prim::Text));
            self.facts
                .formats
                .insert(key, format!("{}<{reads}>", jadpo_std::RANGE));
            return Some(target);
        }
        let reads = match self.contract_of(&target) {
            Contract::Standard(entry) if entry.text.is_some() => Some(entry.name),
            Contract::Representation(prim) => Some(prim.name()),
            _ => None,
        };
        let Some(reads) = reads else {
            let why = match self.contract_of(&target) {
                Contract::Standard(_) => "has no single text form".to_string(),
                Contract::Neither(what) => format!("is {what}"),
                Contract::Representation(_) => String::new(),
            };
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`{written}` can't read text as `{target}`"),
                )
                .at(self.span(value), format!("`{target}` has no canonical text"))
                .reason(format!(
                    "`{written}` reads a standard type's canonical text, or text as a type built on `Text`, `Int`, `Decimal` or `Bool`; `{target}` {why}."
                ))
                .next("Read text as a standard type, or a type built on one or on a representation"),
            );
            return None;
        };
        let key = node_key(value.syntax());
        self.facts.types.insert(key, Ty::Prim(Prim::Text));
        self.facts.formats.insert(key, reads.to_string());
        Some(target)
    }

    /// What `text.parse` can fail with: `malformed` unless it reads text
    /// as text, and every rule of the type it gives that the standard type
    /// or representation it reads doesn't already guarantee, as a
    /// construction's.
    pub(super) fn parse_fallible(&mut self, call: &ast::CallExpr, raw: &Expr, target: &Ty) {
        let base = match self.contract_of(target) {
            Contract::Standard(entry) => match entry.holds {
                Holds::Builtin(builtin) => Some(Ty::Prim(builtin)),
                // Any text is `Html` once it is made safe (chunk 5.5o), so
                // reading one is reading text.
                Holds::Text { .. } if jadpo_std::standard::markup::sanitises(entry.name) => {
                    Some(Ty::Prim(Prim::Text))
                }
                // Its own check, or its range, is what `malformed` already
                // covers.
                Holds::Text { .. } | Holds::Whole { .. } => Some(Ty::Named(entry.name.to_string())),
                // Read as the type it is built on, its bounds checked as a
                // construction's (chunk 5.5i3).
                Holds::Bounded { of, .. } => Some(Ty::Prim(of)),
                _ => None,
            },
            Contract::Representation(prim) => Some(Ty::Prim(prim)),
            Contract::Neither(_) => None,
        };
        let mut rules: Vec<String> = Vec::new();
        if base != Some(Ty::Prim(Prim::Text)) {
            rules.push(RuleKey::Malformed.word().to_string());
        }
        // A number and a unit of another kind (area 19).
        if matches!(base, Some(Ty::Prim(Prim::Quantity(_)))) {
            rules.push(RuleKey::UnknownUnit.word().to_string());
        }
        // A whole number is read within its standard type's range (chunk
        // 5.5n): only a narrower range the type `as:` names can break.
        let range = base
            .as_ref()
            .and_then(|base| match base {
                Ty::Named(name) => standard_type(name),
                _ => None,
            })
            .and_then(|entry| match entry.holds {
                Holds::Whole { min, max } => Some((min, max)),
                _ => None,
            });
        if let Some((min, max)) = range {
            let chain = self.table.rule_chain(target);
            let (min, max) = (
                jadpo_std::Num::whole(i128::from(min)),
                jadpo_std::Num::whole(i128::from(max)),
            );
            if chain
                .iter()
                .any(|(_, own)| own.min.is_some_and(|n| n.compare(min).is_gt()))
            {
                rules.push(RuleKey::Min.word().to_string());
            }
            if chain
                .iter()
                .any(|(_, own)| own.max.is_some_and(|n| n.compare(max).is_lt()))
            {
                rules.push(RuleKey::Max.word().to_string());
            }
        } else if let Some(base) = base {
            rules.extend(self.verdict(raw, &base, target).breakable);
        }
        self.facts
            .parsed
            .insert(node_key(call.syntax()), target.clone());
        // The call's site is the type it gives, whose rules the runtime
        // checks once the text is read, however the call is reached (an
        // `attempt` checks its call without inferring it).
        self.facts
            .types
            .insert(node_key(call.syntax()), target.clone());
        let fallible = if rules.is_empty() {
            Fallible::Never
        } else {
            Fallible::Construction(rules)
        };
        self.facts
            .constructions
            .insert(node_key(call.syntax()), fallible);
    }

    /// The rules `text.parse` can break, by key and as written: `malformed`
    /// and the type's own.
    pub(super) fn parse_rules(&self, call: &ast::CallExpr) -> Vec<(&'static str, String)> {
        let mut rules = vec![(
            RuleKey::Malformed.word(),
            RuleKey::Malformed.word().to_string(),
        )];
        if let Some(target) = self.facts.parsed.get(&node_key(call.syntax())) {
            if matches!(self.table.representation(target), Some(Prim::Quantity(_))) {
                rules.push((
                    RuleKey::UnknownUnit.word(),
                    RuleKey::UnknownUnit.word().to_string(),
                ));
            }
            rules.extend(self.rules_of(target));
        }
        rules
    }

    /// `zone:` and `format:` on `text.format`: each means something only
    /// for a type whose data entry takes it, and a type that takes one
    /// needs it; a number takes neither.
    pub(super) fn contract_options(
        &mut self,
        table: &Operation,
        call: &ast::CallExpr,
        args: &[ast::Arg],
        slots: &[Option<usize>],
        (entry, shown): (Option<&'static StandardType>, &Ty),
        written: &str,
    ) {
        let given = |slot: usize| {
            args.iter()
                .zip(slots)
                .find(|(_, at)| **at == Some(slot))
                .map(|(arg, _)| arg)
        };
        self.form_option(table, args, slots, entry, shown);
        for (slot, param) in table.params.iter().enumerate() {
            if !param.by_type || param.name == "form" {
                continue;
            }
            let name = param.name;
            match (
                entry.and_then(|entry| entry.format_option(name)),
                given(slot),
            ) {
                (None, Some(arg)) => {
                    let span = arg
                        .name()
                        .map_or_else(|| self.span(arg), |name| self.span(&name));
                    let removal = self.argument_removal(arg);
                    let (label, reason) = option_unused(name, entry, shown);
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
                (Some(option), None) if option.needed => {
                    let span = call
                        .expr()
                        .map_or_else(|| self.span(call), |callee| self.span(&callee));
                    let problem = if name == "zone" {
                        Diagnostic::new(
                            &rule::TYPE_TEMPORAL_ZONE_REQUIRED,
                            format!("`{written}` needs the zone to show `{shown}` in"),
                        )
                        .at(span, "no `zone:`")
                        .reason(
                            "An `Instant` is one moment for everyone, and its date and clock time depend on where it is read; the server never guesses a zone, so `zone:` says which.",
                        )
                        .next("Add `zone: ...` with the reader's zone, such as `zone: Zone.europe_london`")
                    } else {
                        Diagnostic::new(
                            &rule::TYPE_TEMPORAL_FORMAT_INVALID,
                            format!("`{written}` needs a format to show `{shown}` in"),
                        )
                        .at(span, "no `format:`")
                        .reason(
                            "A time is shown to a person in a format written out, which the server never guesses: `format:` names a built-in one, `TimeFormat.` followed by `date_`, `time_` or `date_time_` and `full`, `long`, `medium` or `short`, or the name of a format declared as `type Name = TimeFormat { ... }`.",
                        )
                        .next("Add `format: ...`, such as `format: TimeFormat.date_long`")
                    };
                    self.out.push(problem);
                }
                _ => {}
            }
        }
    }
}

impl Body<'_> {
    /// `country:` on `text.parse` (area 19, chunk 5.5l2): it reads a phone
    /// number or a postal code as people write it in a country, so it
    /// means nothing for any other type.
    pub(super) fn country_option(
        &mut self,
        table: &Operation,
        args: &[ast::Arg],
        slots: &[Option<usize>],
        entry: Option<&'static StandardType>,
        shown: &Ty,
    ) {
        let Some(slot) = table
            .params
            .iter()
            .position(|param| param.name == "country")
        else {
            return;
        };
        let Some(arg) = args
            .iter()
            .zip(slots)
            .find(|(_, at)| **at == Some(slot))
            .map(|(arg, _)| arg)
        else {
            return;
        };
        if entry.is_some_and(|entry| jadpo_std::standard::reads_by_country(entry.name)) {
            return;
        }
        let span = arg
            .name()
            .map_or_else(|| self.span(arg), |name| self.span(&name));
        let removal = self.argument_removal(arg);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_TEMPORAL_OPTION_UNUSED,
                format!("`country:` means nothing for a `{shown}`"),
            )
            .at(span, "not read by country")
            .reason(format!(
                "`country:` reads a `PhoneNumber` or a `PostalCode` as it is written in that country; `{shown}` is read from its canonical text alone."
            ))
            .next("Remove `country: ...`")
            .edit(removal, ""),
        );
    }

    /// `form:` on `text.parse` and `text.from` (area 19, question 22): a
    /// `CodeForm` written out, for a closed list with data in that code
    /// system.
    pub(super) fn form_option(
        &mut self,
        table: &Operation,
        args: &[ast::Arg],
        slots: &[Option<usize>],
        entry: Option<&'static StandardType>,
        shown: &Ty,
    ) {
        let Some(slot) = table.params.iter().position(|param| param.name == "form") else {
            return;
        };
        let Some(arg) = args
            .iter()
            .zip(slots)
            .find(|(_, at)| **at == Some(slot))
            .map(|(arg, _)| arg)
        else {
            return;
        };
        let Some(value) = arg.expr() else {
            return;
        };
        let written = tree::written(value.syntax());
        let form = written
            .strip_prefix("CodeForm.")
            .and_then(jadpo_std::CodeForm::from_name);
        let Some(form) = form else {
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_MISMATCH,
                    format!("`form:` is a code system written out, not `{written}`"),
                )
                .at(self.span(&value), "not written out")
                .reason(
                    "`form:` says which of a closed list's code systems the text is in, which is known where the call is written, as `form: CodeForm.alpha3`: a value can't say, and a list with no codes in it would have nothing to read.",
                )
                .next("Write the code system itself, such as `form: CodeForm.alpha3`"),
            );
            return;
        };
        if entry.and_then(|entry| entry.form_codes(form)).is_some() {
            return;
        }
        let span = arg
            .name()
            .map_or_else(|| self.span(arg), |name| self.span(&name));
        let removal = self.argument_removal(arg);
        let has: Vec<String> = entry
            .and_then(|entry| jadpo_std::standard::closed_list(entry.name))
            .map(|list| {
                list.forms
                    .iter()
                    .map(|(form, _)| format!("`CodeForm.{}`", form.name()))
                    .collect()
            })
            .unwrap_or_default();
        let others = if has.is_empty() {
            "it is written in its own code alone".to_string()
        } else {
            format!("it has {}", crate::and_list(&has))
        };
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_TEMPORAL_OPTION_UNUSED,
                format!("`form: {written}` means nothing for a `{shown}`"),
            )
            .at(span, format!("no `{}` codes", form.name()))
            .reason(format!(
                "`form:` reads or writes a closed list's code in another of its code systems; `{shown}` has no codes in `{}`, and {others}.",
                form.name()
            ))
            .next("Remove `form: ...`")
            .edit(removal, ""),
        );
    }
}

/// Why `text.format`'s option means nothing for a type: a standard type's,
/// or a number's, which has no entry.
fn option_unused(option: &str, entry: Option<&StandardType>, shown: &Ty) -> (String, String) {
    let time = entry
        .is_some_and(|entry| matches!(entry.format.key, jadpo_std::standard::FormatKey::DateTime));
    let holds = entry.map(|entry| entry.holds);
    let kind = entry.map_or("number", |entry| entry.name);
    match option {
        "zone" if holds == Some(Holds::Builtin(Prim::ZonedTime)) => (
            format!("`{shown}` is seen in its own zone"),
            format!(
                "`zone:` says which zone an `Instant` is shown in; `{shown}` is a `ZonedTime`, which carries the zone it is seen in, so `text.format` shows it there."
            ),
        ),
        "zone"
            if entry.is_some_and(|entry| matches!(entry.name, "LocalTime" | "LocalDateTime")) =>
        {
            (
                format!("`{shown}` has no zone"),
                format!(
                    "`zone:` says which zone an `Instant` is shown in; `{shown}` is a local time, with no zone, so it is shown as its clock reads."
                ),
            )
        }
        "zone" if time || holds == Some(Holds::Object(&jadpo_std::standard::DATE_RANGE_FIELDS)) => {
            (
                format!("`{shown}` has no clock time"),
                format!(
                    "`zone:` says which zone an `Instant` is shown in; `{shown}` is a date, with no clock time, so it reads the same in every zone."
                ),
            )
        }
        "unit" => (
            format!("`{shown}` isn't a quantity"),
            format!(
                "`unit:` says which unit a physical quantity is shown in; `{shown}` is a {}, which `text.format` shows in its own words, with no unit to choose.",
                if entry.is_some() {
                    format!("`{kind}`")
                } else {
                    kind.to_string()
                }
            ),
        ),
        _ => (
            format!("`{shown}` isn't a time"),
            format!(
                "`{option}:` says how a time is shown to a person; `{shown}` is a {}, which `text.format` shows in its own words, with nothing to choose.",
                if entry.is_some() {
                    format!("`{kind}`")
                } else {
                    kind.to_string()
                }
            ),
        ),
    }
}
