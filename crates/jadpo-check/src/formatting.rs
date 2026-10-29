//! What `jadpo fmt` writes for one file (area 14, "Formatting"), shared
//! by the command and the editor's formatting, so a file formatted on save
//! is the file `jadpo fmt --check` passes.
//!
//! Inside a project the file's imports are fixed first, which needs every
//! file to parse (a name declared in a file that doesn't would read as
//! missing), then the layout is the formatter's, the file's declarations
//! in definition-before-use order with the one it is named after last.

use jadpo_db::Loaded;
use jadpo_syntax::SyntaxError;
use jadpo_syntax::ast::{self, AstNode};

/// `text` laid out as `jadpo fmt` lays out a file at `path`: with its
/// declarations in definition-before-use order when `project` holds it.
pub fn layout(path: &str, text: &str, project: bool) -> Result<String, Vec<SyntaxError>> {
    let primary = project.then(|| primary_name(path, text)).flatten();
    jadpo_fmt::format_file_in_order(text, primary.as_deref())
}

/// The name of the declaration a project file is named after, which
/// `jadpo fmt` writes last (area 1, "Files and names"); none for a test,
/// root or loose file.
pub fn primary_name(path: &str, text: &str) -> Option<String> {
    let parse = jadpo_syntax::parse_file(text);
    let source = ast::SourceFile::cast(parse.syntax())?;
    let items = jadpo_db::items_of(&source);
    let location = jadpo_db::locate(path, jadpo_db::Layout::Project);
    jadpo_db::primary(&location, &items).map(|item| item.name.clone())
}

/// The text `jadpo fmt` writes for the file at `path` of `loaded`, from
/// the text `loaded` holds (an editor's unsaved text included): imports
/// fixed when the project's every file parses, then laid out. `loaded`
/// is left as it was.
pub fn formatted(loaded: &mut Loaded, path: &str) -> Result<String, Vec<SyntaxError>> {
    let project = loaded.project;
    let Some(file) = loaded.file(path) else {
        return Ok(String::new());
    };
    let text = file.text(&loaded.db).clone();
    let in_project = project.layout(&loaded.db) == jadpo_db::Layout::Project;
    let parses = loaded.unreadable.is_empty()
        && project
            .files(&loaded.db)
            .iter()
            .all(|file| !jadpo_db::parse(&loaded.db, *file).has_errors());
    let input = if in_project && parses {
        match crate::fix_imports(loaded, path) {
            Some(fixed) => {
                loaded.set_text(path, text);
                fixed
            }
            None => text,
        }
    } else {
        text
    };
    layout(path, &input, in_project)
}
