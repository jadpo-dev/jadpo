//! A translation call (area 10, "Translations"):
//! `translations.<key>(locale:, ...)`, typed by the default locale's file.
//! The key is a key of that file; the arguments are the `locale:` and one
//! for each placeholder of the key's message there, each of the kind its
//! placeholder takes; and the result is `PresentationText`.

use jadpo_diag::{Diagnostic, nearest, rule};
use jadpo_std::translation::Placeholder;
use jadpo_syntax::ast::{self, AstNode, Expr};

use super::facts::Reach;
use super::calls::plural;
use super::{Body, CallKind, is_literal, node_key};
use crate::lookups::Name;
use crate::translations::{Lookup, accepts, keys, lookup};
use crate::types::{Prim, Ty, TypeRef};

impl Body<'_> {
    pub(super) fn translation_call(
        &mut self,
        call: &ast::CallExpr,
        callee: &Expr,
        key: &str,
        written: &str,
    ) -> (Ty, CallKind, Reach) {
        let text = Ty::Prim(Prim::PresentationText);
        let found = lookup(self.db, self.project, Name::new(self.db, key.to_string())).clone();
        match found {
            Lookup::Key(Some(placeholders)) => {
                let reach = self.translation_arguments(call, key, &placeholders, written);
                (text, CallKind::Pure(written.to_string()), reach)
            }
            // The message didn't read, or the default locale's file can't
            // say: reported where the file is.
            Lookup::Key(None) | Lookup::Broken => {
                self.args_only(call);
                let reach = Reach::Translation {
                    key: key.to_string(),
                    arity: Self::arguments(call).len(),
                };
                (text, CallKind::Pure(written.to_string()), reach)
            }
            Lookup::Absent | Lookup::Unknown => {
                self.args_only(call);
                self.key_unknown(callee, key, written, found == Lookup::Absent);
                (text, CallKind::Pure(written.to_string()), Reach::Unknown)
            }
        }
    }

    fn key_unknown(&mut self, callee: &Expr, key: &str, written: &str, nothing: bool) {
        let all = keys(self.db, self.project);
        let near = nearest(key, all.iter().map(String::as_str));
        let span = self.span(callee);
        let reason = if nothing {
            "Translations are keys of `translations/<locale>.json`, one file per supported locale, and the default locale's file defines them. The project has no such files.".to_string()
        } else if all.is_empty() {
            format!("`{key}` is not a key of the default locale's file, which defines no keys.")
        } else {
            let listed: Vec<String> = all.iter().map(|name| format!("`{name}`")).collect();
            format!(
                "`{key}` is not a key of the default locale's file. Its keys are {}.{}",
                crate::and_list(&listed),
                super::did_you_mean(near)
            )
        };
        let problem = Diagnostic::new(
            &rule::TRANSLATION_KEY_UNKNOWN,
            format!("`{written}` is not a translation key"),
        )
        .at(span.clone(), "no such key")
        .reason(reason);
        self.out.push(match near {
            Some(near) => problem
                .next(format!("Use `translations.{near}`"))
                .edit(span, format!("translations.{near}")),
            None if nothing => problem.next(
                "Add the key and its message to `translations/<locale>.json` for each supported locale",
            ),
            None => problem.next(format!(
                "Add `\"{key}\"` and its message to `translations/<locale>.json` for each supported locale, or use a key that exists"
            )),
        });
    }

    /// The arguments of a call of a key whose placeholders are known.
    fn translation_arguments(
        &mut self,
        call: &ast::CallExpr,
        key: &str,
        placeholders: &[Placeholder],
        written: &str,
    ) -> Reach {
        // The locale, then each placeholder by name.
        let locale_ty = self
            .table
            .resolve_std(&TypeRef::Path(vec!["Locale".to_string()], Vec::new()));
        let mut params: Vec<(String, Ty)> = vec![("locale".to_string(), locale_ty.clone())];
        params.extend(
            placeholders
                .iter()
                .map(|placeholder| (placeholder.name.clone(), Ty::Error)),
        );
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
                None => (index < params.len()).then_some(index),
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
            let Some(placeholder) = placeholders.get(*slot - 1) else {
                continue;
            };
            let ty = (!is_literal(&value)).then(|| self.infer(&value));
            self.built_on_argument(
                &value,
                ty,
                accepts(placeholder.kind),
                written,
                &placeholder.name,
            );
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
