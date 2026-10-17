//! "Swap for the standard ..." (area 14, "Editor support"; area 19,
//! question 4; chunk 5.5a2): the quick fix on `type.standard_clash` and
//! `type.standard_exists`, which runs the code `jadpo fix <name> swap`
//! runs (`jadpo_check::fixes`) and is unavailable, with the errors it
//! would add, where the command stops.
//!
//! The rename needs a name, which a quick fix can't ask for, so it stays
//! `jadpo fix <name> rename <NewName>` until the editor's rename comes
//! with `jadpo rename`. As "Make this a setting" is, the edit is made in
//! the server's own database and sent as one workspace edit, and an
//! editor that resolves code actions gets it only once it is chosen.

use jadpo_check::fixes::{self, Fix, FixKind};
use lsp_types::{CodeAction, CodeActionDisabled, CodeActionKind, Uri};
use serde_json::json;

use crate::server::Server;

/// The start of the action's title, as area 14 names it: "Swap for the
/// standard `InstantRange`".
pub const SWAP: &str = "Swap for the standard";

/// The `data` an unresolved swap carries, to tell it from the others.
pub(crate) const DATA: &str = "swap";

impl Server {
    /// The swaps offered for the findings at `start..end` of `path`.
    pub(crate) fn swap_actions(
        &mut self,
        uri: &Uri,
        path: &str,
        start: u32,
        end: u32,
    ) -> Vec<CodeAction> {
        let Some(loaded) = self.loaded() else {
            return Vec::new();
        };
        let offsets: Vec<u32> = fixes::findings(&loaded.db, loaded.project)
            .into_iter()
            .filter(|finding| {
                finding.path == path
                    && finding.span.start <= end
                    && start <= finding.span.end
                    && finding.fixes.contains(&FixKind::Swap)
            })
            .map(|finding| finding.span.start)
            .collect();
        let resolves = self.resolves_edits;
        offsets
            .into_iter()
            .filter_map(|offset| {
                let action = self.swap(path, offset, !resolves)?;
                Some(CodeAction {
                    data: Some(json!({ "uri": uri.as_str(), "offset": offset, "action": DATA })),
                    ..action
                })
            })
            .collect()
    }

    /// The swap for the finding at `offset` of `path`, with its edit when
    /// `edit` is set; `None` when no finding there takes one.
    pub(crate) fn swap(&mut self, path: &str, offset: u32, edit: bool) -> Option<CodeAction> {
        let loaded = self.loaded()?;
        let file = loaded.file(path)?;
        let finding = fixes::at(&loaded.db, loaded.project, file, offset)
            .into_iter()
            .find(|finding| finding.fixes.contains(&FixKind::Swap))?;
        let title = format!("{SWAP} `{}`", finding.standard);
        let action = |disabled: Option<String>| CodeAction {
            title: title.clone(),
            kind: Some(CodeActionKind::QUICKFIX),
            disabled: disabled.map(|reason| CodeActionDisabled { reason }),
            ..CodeAction::default()
        };
        let plan = match fixes::plan(&loaded.db, loaded.project, &finding, &Fix::Swap) {
            Ok(plan) => plan,
            Err(refusals) => {
                let reasons: Vec<String> = refusals.iter().map(|p| p.message.clone()).collect();
                return Some(action(Some(reasons.join("; "))));
            }
        };
        if !edit {
            return Some(action(None));
        }
        let loaded = self.loaded_mut()?;
        let fixed = match fixes::apply(loaded, &plan) {
            Ok(fixed) => fixed,
            Err(stopped) => {
                let first = stopped
                    .problems
                    .first()
                    .map_or_else(String::new, |problem| problem.message.clone());
                return Some(action(Some(format!(
                    "The swap would add an error: {first}"
                ))));
            }
        };
        fixes::restore(loaded, &fixed);
        let edit = self.workspace_edit(&fixed.files)?;
        Some(CodeAction {
            edit: Some(edit),
            ..action(None)
        })
    }
}
