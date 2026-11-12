//! Rename (area 14, "Editor support"; chunk D8b): `textDocument/
//! prepareRename` and `textDocument/rename` run `jadpo_check::rename`, the
//! code `jadpo rename` runs, so the editor renames the same places, writes
//! the same identity line (a renamed entity field keeps its column), the
//! same records and the same moved files, and refuses the same names.
//!
//! Shaped before it was built (impeccable's shape pass against the CLI
//! design language, run unattended): the editor owns the screen, a box
//! for the new name and the edit's preview, so the server's part is the
//! words of a refusal, each `jadpo rename`'s own headline and reason, as
//! one sentence pair the editor shows in its message (`` `AddTask` isn't
//! a name for an action. An action's name is written in lower_snake_case,
//! as `add_task` is. ``). The next step is left out, since it names the
//! command line. What can't be renamed (a parameter, a route, a test) is
//! refused when the editor first asks, before a name is typed.
//!
//! As "Make this a setting" is, the rename is made in the server's own
//! database and sent as one workspace edit, each changed file replaced
//! whole and the registry created when there is none; a declaration's
//! file, and the test beside it, is renamed rather than deleted and
//! created, so an editor tab open on it follows it. The server then puts
//! its texts back, since the editor applying the edit is what changes
//! them.

use jadpo_check::rename;
use jadpo_diag::Diagnostic;
use lsp_types::{
    DocumentChangeOperation, PrepareRenameResponse, RenameFile, RenameParams, ResourceOp,
    TextDocumentPositionParams, WorkspaceEdit,
};

use crate::convert;
use crate::server::Server;

/// A refusal as the editor shows it: the headline, then its reason.
fn said(problem: &Diagnostic) -> String {
    match &problem.reason {
        Some(reason) => format!("{}. {reason}", problem.message),
        None => problem.message.clone(),
    }
}

impl Server {
    /// The name under the cursor and its range when `jadpo rename` renames
    /// what it refers to; why not when it doesn't; `None` when no name is
    /// there.
    pub(crate) fn prepare_rename(
        &self,
        params: &TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>, String> {
        let Some((path, offset)) = self.place(&params.text_document.uri, params.position) else {
            return Ok(None);
        };
        let Some(loaded) = self.loaded() else {
            return Ok(None);
        };
        let Some(file) = loaded.file(&path) else {
            return Ok(None);
        };
        if let Some(problem) = rename::unparsed(&loaded.db, loaded.project).first() {
            return Err(unparsed(problem));
        }
        let Some((target, (start, end))) = rename::at(&loaded.db, loaded.project, file, offset)
        else {
            return Ok(None);
        };
        target.map_err(|problem| said(&problem))?;
        let text = file.text(&loaded.db);
        Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: convert::range(text, start, end, self.encoding()),
            placeholder: text[start as usize..end as usize].to_string(),
        }))
    }

    /// The workspace edit that renames what the name under the cursor
    /// refers to `params.new_name`, as `jadpo rename` would; why not when
    /// it refuses or stops.
    pub(crate) fn rename(
        &mut self,
        params: &RenameParams,
    ) -> Result<Option<WorkspaceEdit>, String> {
        let position = &params.text_document_position;
        let Some((path, offset)) = self.place(&position.text_document.uri, position.position)
        else {
            return Ok(None);
        };
        let Some(loaded) = self.loaded() else {
            return Ok(None);
        };
        let Some(file) = loaded.file(&path) else {
            return Ok(None);
        };
        if let Some(problem) = rename::unparsed(&loaded.db, loaded.project).first() {
            return Err(unparsed(problem));
        }
        let Some((target, _)) = rename::at(&loaded.db, loaded.project, file, offset) else {
            return Ok(None);
        };
        let key = target.map_err(|problem| said(&problem))?;
        let plan = rename::plan(&loaded.db, loaded.project, &key, &params.new_name)
            .map_err(|problem| said(&problem))?;
        let Some(loaded) = self.loaded_mut() else {
            return Ok(None);
        };
        let renamed = rename::apply(loaded, &plan).map_err(|stopped| {
            let first = stopped.problems.first().map_or_else(String::new, said);
            format!("The rename would add an error: {first}")
        })?;
        rename::restore(loaded, &renamed);
        Ok(self.renaming_edit(&renamed.files, plan.moves()))
    }

    /// The changed files as one workspace edit, as [`Server::workspace_edit`]
    /// makes it, except that each of `moves` is a file renamed, then
    /// edited from its text under its old name.
    fn renaming_edit(
        &self,
        files: &[jadpo_check::setting_move::FileChange],
        moves: &[(String, String)],
    ) -> Option<WorkspaceEdit> {
        let loaded = self.loaded()?;
        let root = self.uri_root(loaded).to_path_buf();
        let mut operations = Vec::new();
        for (from, to) in moves {
            operations.push(DocumentChangeOperation::Op(ResourceOp::Rename(
                RenameFile {
                    old_uri: convert::uri_of(&root.join(from))?,
                    new_uri: convert::uri_of(&root.join(to))?,
                    options: None,
                    annotation_id: None,
                },
            )));
        }
        let mut changes = Vec::new();
        for change in files {
            if moves.iter().any(|(from, _)| *from == change.path) {
                continue;
            }
            let mut change = change.clone();
            if let Some((from, _)) = moves.iter().find(|(_, to)| *to == change.path) {
                change.before = files
                    .iter()
                    .find(|other| other.path == *from)
                    .and_then(|other| other.before.clone());
                if change.before.is_none() {
                    // Not changed by the rename beyond its move: renamed as
                    // it is.
                    continue;
                }
            }
            changes.push(change);
        }
        let edit = self.workspace_edit(&changes)?;
        if let Some(lsp_types::DocumentChanges::Operations(rest)) = edit.document_changes {
            operations.extend(rest);
        }
        Some(WorkspaceEdit {
            document_changes: Some(lsp_types::DocumentChanges::Operations(operations)),
            ..WorkspaceEdit::default()
        })
    }
}

/// A file that doesn't parse, which stops a rename as it stops the
/// command.
fn unparsed(problem: &Diagnostic) -> String {
    let path = problem
        .primary
        .as_ref()
        .map_or("A file", |label| label.span.path.as_str());
    format!(
        "{path} doesn't parse, so nothing is renamed until it does: {}",
        problem.message
    )
}
