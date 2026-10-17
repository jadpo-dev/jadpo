//! "Make this a setting" (area 14, "Editor support"; area 10, "Making a
//! constant a setting", chunk 5.3f): a code action on a constant's
//! declaration or any read of it, which runs the code `jadpo config move`
//! runs (`jadpo_check::setting_move`) and refuses what it refuses, shown
//! as the action's reason for being unavailable.
//!
//! The edit is made in the server's own database and sent as one
//! workspace edit, each changed file replaced whole, a new `config.jadpo`
//! created and an emptied file deleted; the server then puts its texts
//! back, since the editor applying the edit is what changes them. An
//! editor that resolves code actions gets the edit only once it is chosen.

use jadpo_check::setting_move;
use lsp_types::{
    CodeAction, CodeActionDisabled, CodeActionKind, CodeActionOrCommand, CodeActionParams,
    CodeActionResponse, CreateFile, DeleteFile, DocumentChangeOperation, DocumentChanges, OneOf,
    OptionalVersionedTextDocumentIdentifier, Position, Range, ResourceOp, TextDocumentEdit,
    TextEdit, Uri, WorkspaceEdit,
};
use serde_json::json;

use crate::convert;
use crate::server::Server;

/// The action's title, as area 14 names it.
pub const TITLE: &str = "Make this a setting";

impl Server {
    /// The code actions for `params`' range: "Make this a setting" on a
    /// constant at its start, unavailable with the reason it is refused;
    /// then the swap for a standard type's finding (`crate::fixes`) and the
    /// decline quick fixes (`crate::declines`).
    pub(crate) fn code_actions(&mut self, params: &CodeActionParams) -> Option<CodeActionResponse> {
        let uri = params.text_document.uri.clone();
        let (path, offset) = self.place(&uri, params.range.start)?;
        let end = self
            .place(&uri, params.range.end)
            .map_or(offset, |(_, end)| end);
        let mut out = Vec::new();
        if let Some(action) = self.make_setting(&path, offset, !self.resolves_edits) {
            out.push(CodeActionOrCommand::CodeAction(CodeAction {
                data: Some(json!({ "uri": uri.as_str(), "offset": offset })),
                ..action
            }));
        }
        out.extend(
            self.swap_actions(&uri, &path, offset, end)
                .into_iter()
                .map(CodeActionOrCommand::CodeAction),
        );
        out.extend(
            self.decline_actions(&path, offset, end)
                .into_iter()
                .map(CodeActionOrCommand::CodeAction),
        );
        (!out.is_empty()).then_some(out)
    }

    /// The edit of an action [`Server::code_actions`] offered without one.
    pub(crate) fn resolve(&mut self, action: CodeAction) -> CodeAction {
        let place = action.data.as_ref().and_then(|data| {
            let uri = data["uri"].as_str()?.parse::<Uri>().ok()?;
            let offset = u32::try_from(data["offset"].as_u64()?).ok()?;
            Some((self.relative(&uri)?, offset))
        });
        let Some((path, offset)) = place else {
            return action;
        };
        if action
            .data
            .as_ref()
            .and_then(|data| data["action"].as_str())
            == Some(crate::fixes::DATA)
        {
            return match self.swap(&path, offset, true) {
                Some(resolved) => CodeAction {
                    data: action.data,
                    ..resolved
                },
                None => action,
            };
        }
        match self.make_setting(&path, offset, true) {
            Some(resolved) => CodeAction {
                data: action.data,
                ..resolved
            },
            None => action,
        }
    }

    /// The project-relative path and byte offset of `position` in `uri`.
    fn place(&self, uri: &Uri, position: Position) -> Option<(String, u32)> {
        let path = self.relative(uri)?;
        let loaded = self.loaded()?;
        let text = loaded.file(&path)?.text(&loaded.db).clone();
        Some((path, convert::offset(&text, position, self.encoding())?))
    }

    /// The action for the constant at `offset` of `path`, with its edit
    /// when `edit` is set; `None` when no constant is there.
    fn make_setting(&mut self, path: &str, offset: u32, edit: bool) -> Option<CodeAction> {
        let loaded = self.loaded()?;
        let file = loaded.file(path)?;
        let id = setting_move::at(&loaded.db, loaded.project, file, offset)?;
        let action = |disabled: Option<String>| CodeAction {
            title: TITLE.to_string(),
            kind: Some(CodeActionKind::REFACTOR_REWRITE),
            disabled: disabled.map(|reason| CodeActionDisabled { reason }),
            ..CodeAction::default()
        };
        let errors = jadpo_check::check(&loaded.db, loaded.project)
            .into_iter()
            .filter(|problem| !setting_move::answered(&loaded.db, loaded.project, &id, problem))
            .count();
        if errors > 0 {
            return Some(action(Some(
                "The project has errors; fix them first, as `jadpo check` lists them".to_string(),
            )));
        }
        let plan = match setting_move::plan(&loaded.db, loaded.project, &id) {
            Ok(plan) => plan,
            Err(refusals) => {
                let reasons: Vec<String> = refusals
                    .iter()
                    .map(|problem| problem.message.clone())
                    .collect();
                return Some(action(Some(reasons.join("; "))));
            }
        };
        if !edit {
            return Some(action(None));
        }
        let loaded = self.loaded_mut()?;
        let moved = match setting_move::apply(loaded, &plan) {
            Ok(moved) => moved,
            Err(stopped) => {
                let first = stopped
                    .problems
                    .first()
                    .map_or_else(String::new, |problem| problem.message.clone());
                return Some(action(Some(format!(
                    "The project wouldn't check after the move: {first}"
                ))));
            }
        };
        setting_move::restore(loaded, &moved);
        let edit = self.workspace_edit(&moved.files)?;
        Some(CodeAction {
            edit: Some(edit),
            ..action(None)
        })
    }

    /// The changed files as one workspace edit: each replaced whole, a new
    /// one created and an emptied one deleted.
    pub(crate) fn workspace_edit(
        &self,
        files: &[setting_move::FileChange],
    ) -> Option<WorkspaceEdit> {
        let loaded = self.loaded()?;
        let root = self.uri_root(loaded).to_path_buf();
        let encoding = self.encoding();
        let mut operations = Vec::new();
        for change in files {
            let uri = convert::uri_of(&root.join(&change.path))?;
            match (&change.before, &change.after) {
                (_, None) => operations.push(DocumentChangeOperation::Op(ResourceOp::Delete(
                    DeleteFile { uri, options: None },
                ))),
                (before, Some(after)) => {
                    let before = before.clone().unwrap_or_default();
                    if change.before.is_none() {
                        operations.push(DocumentChangeOperation::Op(ResourceOp::Create(
                            CreateFile {
                                uri: uri.clone(),
                                options: None,
                                annotation_id: None,
                            },
                        )));
                    }
                    let whole: Range = convert::range(&before, 0, before.len() as u32, encoding);
                    operations.push(DocumentChangeOperation::Edit(TextDocumentEdit {
                        text_document: OptionalVersionedTextDocumentIdentifier {
                            uri,
                            version: None,
                        },
                        edits: vec![OneOf::Left(TextEdit {
                            range: whole,
                            new_text: after.clone(),
                        })],
                    }));
                }
            }
        }
        Some(WorkspaceEdit {
            document_changes: Some(DocumentChanges::Operations(operations)),
            ..WorkspaceEdit::default()
        })
    }
}
