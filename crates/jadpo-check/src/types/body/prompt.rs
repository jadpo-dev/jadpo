//! `prompts.render(prompt)` (area 21, "Prompts as files"): a value of a
//! prompt's type rendered as the messages a model is sent, the
//! instructions as the system message, the conversation's turns, and the
//! request as the user message. It is pure, so a test renders a prompt
//! without a model; it reads only the value's type, never the prompt's
//! words, which the prompt check reads.

use jadpo_diag::{Diagnostic, rule};
use jadpo_syntax::ast::{self, Expr};

use super::super::ty::{Ty, TypeRef};
use super::facts::Reach;
use super::{Body, CallKind};

impl Body<'_> {
    pub(super) fn prompt_render(
        &mut self,
        call: &ast::CallExpr,
        written: &str,
    ) -> (Ty, CallKind, Reach) {
        let message = self.table.resolve(&TypeRef::Path(
            vec![jadpo_std::standard::ai::CHAT_MESSAGE.to_string()],
            Vec::new(),
        ));
        let result = Ty::List(Box::new(message));
        let pure = CallKind::Pure(written.to_string());
        let args = Self::arguments(call);
        let value: Option<Expr> = match args.as_slice() {
            [arg] if arg.name().is_none() => arg.expr(),
            _ => None,
        };
        let Some(value) = value else {
            self.args_only(call);
            let span = self.arg_list_span(call);
            self.out.push(
                Diagnostic::new(
                    &rule::TYPE_ARGUMENT_COUNT,
                    format!("`{written}` takes one prompt"),
                )
                .at(span, format!("{} given", args.len()))
                .reason("`prompts.render(AnswerQuestion { ... })` renders one value of a prompt's type as the messages a model is sent.")
                .next("Pass one prompt, built with its fields"),
            );
            return (result, pure, Reach::Unknown);
        };
        let ty = self.infer(&value);
        if ty.has_error() {
            return (result, pure, Reach::Unknown);
        }
        if let Some(key) = crate::prompts::prompt_of(self.table, &ty) {
            return (result, pure, Reach::Prompt(key));
        }
        let span = self.span(&value);
        self.out.push(
            Diagnostic::new(
                &rule::TYPE_MISMATCH,
                format!("`{ty}` can't be used where a prompt is required"),
            )
            .at(span, format!("this is `{ty}`"))
            .reason("`prompts.render` renders a prompt: a value of a `type X = Prompt { ... }`, whose fields fill the placeholders of its `.prompt` file.")
            .next("Pass a prompt, such as `AnswerQuestion { ... }`, built with its fields"),
        );
        (result, pure, Reach::Unknown)
    }
}
