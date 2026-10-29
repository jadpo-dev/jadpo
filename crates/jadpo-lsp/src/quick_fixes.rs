//! Quick fixes from an error's own edit (area 14, "Error messages": an
//! obvious repair carries its edit, so the editor and agents can apply
//! it): every error at the cursor whose next step carries edits is
//! offered as a quick fix named by that step, the edits `jadpo check
//! --json` gives an agent.
//!
//! Promotion (`layout.promotion_required`, area 1) is one of them, made as
//! `jadpo fmt --layout` makes it: the move, then every file's imports
//! fixed, so a file that imported the moved type from its old folder
//! imports it from the new one. Never on save: only when chosen. The edit
//! is made in the server's own database and sent as one workspace edit,
//! each changed file replaced whole, as "Make this a setting" is; the
//! server then puts its texts back. An editor that resolves code actions
//! gets the edit only once the action is chosen.

use std::collections::BTreeMap;

use jadpo_check::setting_move::FileChange;
use jadpo_diag::{Diagnostic, rule};
use lsp_types::{CodeAction, CodeActionKind, Uri};
use serde_json::json;

use crate::convert;
use crate::server::Server;

/// The `data` an unresolved quick fix carries, to tell it from the others.
pub(crate) const DATA: &str = "fix";

/// Where a next step's alternative for the command line starts, which the
/// editor's title leaves out: "..., or run `jadpo fmt --layout`, ...".
const OR_RUN: &str = ", or run `";

impl Server {
    /// The quick fixes for the errors at `start..end` of `path`.
    pub(crate) fn fix_actions(
        &mut self,
        uri: &Uri,
        path: &str,
        start: u32,
        end: u32,
    ) -> Vec<CodeAction> {
        let Some(loaded) = self.loaded() else {
            return Vec::new();
        };
        let problems: Vec<Diagnostic> = jadpo_check::check(&loaded.db, loaded.project)
            .into_iter()
            .filter(|problem| {
                problem.primary.as_ref().is_some_and(|label| {
                    label.span.path == path && label.span.start <= end && start <= label.span.end
                }) && problem
                    .next
                    .as_ref()
                    .is_some_and(|next| !next.edits.is_empty())
            })
            .collect();
        let resolves = self.resolves_edits;
        problems
            .iter()
            .filter_map(|problem| {
                let action = self.fix(problem, !resolves)?;
                let at = problem.primary.as_ref()?.span.start;
                Some(CodeAction {
                    data: Some(json!({
                        "uri": uri.as_str(),
                        "offset": at,
                        "action": DATA,
                        "rule": problem.rule.id,
                    })),
                    ..action
                })
            })
            .collect()
    }

    /// The quick fix chosen, found again by its error's rule and place.
    pub(crate) fn resolve_fix(
        &mut self,
        path: &str,
        offset: u32,
        rule: &str,
    ) -> Option<CodeAction> {
        let loaded = self.loaded()?;
        let problem = jadpo_check::check(&loaded.db, loaded.project)
            .into_iter()
            .find(|problem| {
                problem.rule.id == rule
                    && problem
                        .primary
                        .as_ref()
                        .is_some_and(|label| label.span.path == path && label.span.start == offset)
            })?;
        self.fix(&problem, true)
    }

    /// The quick fix for `problem`, with its edit when `edit` is set.
    fn fix(&mut self, problem: &Diagnostic, edit: bool) -> Option<CodeAction> {
        let next = problem.next.as_ref()?;
        let title = next
            .text
            .split_once(OR_RUN)
            .map_or(next.text.as_str(), |(first, _)| first)
            .to_string();
        let loaded = self.loaded()?;
        let label = problem.primary.as_ref()?;
        let texts = jadpo_db::source_texts(&loaded.db, loaded.project);
        let text = texts.get(&label.span.path)?;
        let source_of = |path: &str| texts.get(path).cloned();
        let shown = convert::diagnostic(
            problem,
            text,
            self.uri_root(loaded),
            &source_of,
            self.encoding(),
        );
        let mut action = CodeAction {
            title,
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![shown]),
            is_preferred: Some(true),
            ..CodeAction::default()
        };
        if edit {
            let files = self.applied(problem)?;
            action.edit = Some(self.workspace_edit(&files)?);
        }
        Some(action)
    }

    /// Each file `problem`'s edits change, with its text before and after;
    /// the server's texts are put back.
    fn applied(&mut self, problem: &Diagnostic) -> Option<Vec<FileChange>> {
        let next = problem.next.as_ref()?;
        let promotion = problem.rule.id == rule::LAYOUT_PROMOTION_REQUIRED.id;
        let loaded = self.loaded_mut()?;
        let project = loaded.project;
        let before: BTreeMap<String, String> = jadpo_db::source_texts(&loaded.db, project);
        let mut by_file: BTreeMap<String, Vec<(u32, u32, String)>> = BTreeMap::new();
        for edit in &next.edits {
            by_file.entry(edit.span.path.clone()).or_default().push((
                edit.span.start,
                edit.span.end,
                edit.replacement.clone(),
            ));
        }
        for (path, edits) in by_file {
            let text = before.get(&path).cloned().unwrap_or_default();
            loaded.set_text(&path, jadpo_check::apply_edits(&text, edits));
        }
        if promotion {
            // As `jadpo fmt --layout`: then every file's imports, then the
            // layout of each file the move touched that was laid out.
            let paths: Vec<String> = project
                .files(&loaded.db)
                .iter()
                .map(|file| file.path(&loaded.db).clone())
                .collect();
            for path in &paths {
                jadpo_check::fix_imports(loaded, path);
            }
            let in_project = project.layout(&loaded.db) == jadpo_db::Layout::Project;
            for path in &paths {
                let Some(now) = loaded.file(path).map(|file| file.text(&loaded.db).clone()) else {
                    continue;
                };
                let was = before.get(path);
                if was == Some(&now) {
                    continue;
                }
                let laid_out = was.is_none_or(|was| {
                    jadpo_check::formatting::layout(path, was, in_project)
                        .ok()
                        .as_ref()
                        == Some(was)
                });
                if laid_out
                    && let Ok(text) = jadpo_check::formatting::layout(path, &now, in_project)
                {
                    loaded.set_text(path, text);
                }
            }
        }
        let after: BTreeMap<String, String> = jadpo_db::source_texts(&loaded.db, project);
        let mut files = Vec::new();
        for (path, now) in &after {
            let was = before.get(path);
            if was == Some(now) {
                continue;
            }
            files.push(FileChange {
                path: path.clone(),
                before: was.cloned(),
                after: Some(now.clone()),
                rows: Vec::new(),
            });
        }
        // The editor makes the change; the server's texts are as they were.
        for change in &files {
            match &change.before {
                Some(text) => loaded.set_text(&change.path, text.clone()),
                None => loaded.remove(&change.path),
            }
        }
        Some(files)
    }
}
