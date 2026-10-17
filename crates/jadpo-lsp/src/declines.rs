//! "Decline with a reason" and "Remove decline" (area 14, "Editor
//! support"; area 1, "Declining a judgement call", chunk 5.4b): quick
//! fixes that write and remove `decline` lines through the code `jadpo
//! declines clear` runs (`jadpo_check::declines`), so the editor and the
//! command agree on what a decline is and where it goes.
//!
//! "Decline with a reason" is offered on a finding of a declinable rule:
//! it writes `decline <rule>: ""` above the declaration the finding is on,
//! and `decline.reason_required` then asks for the reason until it is
//! written. "Remove decline" is offered on a `decline` line.

use lsp_types::{
    CodeAction, CodeActionKind, DocumentChanges, OneOf, OptionalVersionedTextDocumentIdentifier,
    TextDocumentEdit, TextEdit, WorkspaceEdit,
};

use jadpo_check::declines;
use jadpo_diag::Edit;

use crate::convert;
use crate::server::Server;

/// The actions' titles, as area 1 names them.
pub const DECLINE: &str = "Decline with a reason";
pub const REMOVE: &str = "Remove decline";

impl Server {
    /// The decline quick fixes for `start..end` of `path`.
    pub(crate) fn decline_actions(&self, path: &str, start: u32, end: u32) -> Vec<CodeAction> {
        let Some(loaded) = self.loaded() else {
            return Vec::new();
        };
        let (db, project) = (&loaded.db, loaded.project);
        let overlaps = |from: u32, to: u32| from <= end && start <= to;
        let mut out = Vec::new();
        for problem in jadpo_check::check(db, project) {
            let Some(label) = &problem.primary else {
                continue;
            };
            if label.span.path != path || !overlaps(label.span.start, label.span.end) {
                continue;
            }
            if let Some(edit) = declines::insertion(db, project, &problem)
                && let Some(action) = self.action(format!("{DECLINE} ({})", problem.rule.id), &edit)
            {
                out.push(action);
            }
        }
        let Some(file) = loaded.file(path) else {
            return out;
        };
        let text = file.text(db);
        for decline in declines::declines_in(db, file) {
            if overlaps(decline.span.start, decline.span.end)
                && let Some(action) =
                    self.action(REMOVE.to_string(), &declines::removal(text, &decline))
            {
                out.push(action);
            }
        }
        out
    }

    /// A quick fix making `edit`.
    fn action(&self, title: String, edit: &Edit) -> Option<CodeAction> {
        let loaded = self.loaded()?;
        let text = loaded.file(&edit.span.path)?.text(&loaded.db).clone();
        let uri = convert::uri_of(&self.uri_root(loaded).join(&edit.span.path))?;
        let range = convert::range(&text, edit.span.start, edit.span.end, self.encoding());
        Some(CodeAction {
            title,
            kind: Some(CodeActionKind::QUICKFIX),
            edit: Some(WorkspaceEdit {
                document_changes: Some(DocumentChanges::Edits(vec![TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier { uri, version: None },
                    edits: vec![OneOf::Left(TextEdit {
                        range,
                        new_text: edit.replacement.clone(),
                    })],
                }])),
                ..WorkspaceEdit::default()
            }),
            ..CodeAction::default()
        })
    }
}
