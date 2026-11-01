//! `jadpo flags remove <flag> --keep <value>` (area 16, "Removing a flag";
//! chunk B10): the source rewritten as if the flag had always held the
//! kept value.
//!
//! - A read that is an `if`'s whole condition, alone, behind `!`, or
//!   compared with `==` or `!=` to one of the flag's values, decides that
//!   `if`: the branch that runs stays, unwrapped, and the others go. When
//!   what stays ends by returning, what came after it in the same block
//!   can't run and goes too.
//! - Every other read becomes the kept value, `true` or `Type.variant`.
//! - The declaration goes. `flags { }` stays when its last flag goes, so
//!   the flags' tables and their log stay.
//! - A test that gave the flag another value, or left it at another
//!   `default:`, and reached a read tested a branch that has gone, so it
//!   goes; any other test loses its `flags:` line for the flag.
//!
//! Then imports left unused go, each file is formatted as it was, and the
//! result is checked: a removal that wouldn't check is undone.

use std::collections::{BTreeMap, BTreeSet};

use jadpo_db::{Loaded, Project};
use jadpo_diag::Diagnostic;
use jadpo_syntax::ast::{self, AstNode, Expr};
use jadpo_syntax::{SyntaxKind, SyntaxNode};

use crate::flags::{FLAGS, Flag};
use crate::setting_move::{FileChange, Mark, Row, Stopped, canonical, remove_node, restore_texts};

/// What [`apply`] did: every file it changed, in path order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Removed {
    pub files: Vec<FileChange>,
    /// The tests it deleted, by file and sentence.
    pub tests: Vec<(String, String)>,
}

/// Why a flag can't be removed before anything is rewritten.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// A route's `run:` passes the flag on, and a route passes no literal.
    RouteReads { route: String, path: String },
}

/// Rewrites `loaded` as if `flag` had always held the value at `keep`,
/// checking the result: on success the new texts stay in `loaded`; on
/// failure they are undone and the errors returned.
pub fn apply(
    loaded: &mut Loaded,
    flag: &Flag,
    keep: u32,
) -> Result<Result<Removed, Stopped>, Refused> {
    let project = loaded.project;
    if let Some((key, _, span)) = crate::routes::flag_arguments(&loaded.db, project)
        .into_iter()
        .find(|(_, name, _)| *name == flag.name)
    {
        return Err(Refused::RouteReads {
            route: key.name().to_string(),
            path: span.path,
        });
    }
    let reach = crate::flags::tests_reaching(&loaded.db, project);
    let paths: Vec<String> = jadpo_db::source_texts(&loaded.db, project)
        .keys()
        .cloned()
        .collect();
    let mut before: BTreeMap<String, Option<String>> = BTreeMap::new();
    let text_of =
        |loaded: &Loaded, path: &str| loaded.file(path).map(|f| f.text(&loaded.db).clone());
    for path in &paths {
        before.insert(path.clone(), text_of(loaded, path));
    }
    let mut rows: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    let mut deleted_tests = Vec::new();

    // 1. The tests: those that tested a branch that goes, whole; the
    //    others' `flags:` line for the flag.
    // A test reaches a read when it calls what holds it and, where the read
    // sits in a branch another flag decides, its values take that branch.
    let declared = crate::flags::flags(&loaded.db, project).clone();
    let mut doomed: BTreeSet<(String, u32)> = BTreeSet::new();
    for test in &reach {
        let value = |name: &str| -> Option<u32> {
            test.given
                .iter()
                .find(|(given, _)| given == name)
                .map(|(_, at)| *at)
                .or_else(|| declared.flag(name).map(|flag| flag.default))
        };
        if !test.flags.contains(&flag.name) || value(&flag.name) == Some(keep) {
            continue;
        }
        let reaches = test.reads.iter().any(|(path, start)| {
            let Some(text) = before.get(path).cloned().flatten() else {
                return false;
            };
            let parse = jadpo_syntax::parse_file(&text);
            let Some(read) = parse
                .syntax()
                .descendants()
                .filter_map(ast::FieldExpr::cast)
                .find(|field| u32::from(field.syntax().text_range().start()) == *start)
            else {
                // Another flag's read in a route's `run:`; this flag's
                // are refused above.
                return false;
            };
            is_read(read.syntax(), &flag.name) && guards_hold(read.syntax(), &declared, &value)
        });
        if reaches {
            doomed.insert((test.path.clone(), test.index));
        }
    }
    for path in &paths {
        let Some(text) = text_of(loaded, path) else {
            continue;
        };
        let parse = jadpo_syntax::parse_file(&text);
        let Some(source) = ast::SourceFile::cast(parse.syntax()) else {
            continue;
        };
        let mut edits: Vec<(u32, u32, String)> = Vec::new();
        for (index, decl) in source.decls().enumerate() {
            let ast::Decl::TestDecl(test) = decl else {
                continue;
            };
            let sentence = test_sentence(&test);
            if doomed.contains(&(path.clone(), index as u32)) {
                edits.push(whole(&text, test.syntax()));
                deleted_tests.push((path.clone(), sentence.clone()));
                rows.entry(path.clone()).or_default().push(Row {
                    mark: Mark::Removed,
                    subject: format!("test \"{sentence}\""),
                    note: "tested a branch that goes".to_string(),
                });
                continue;
            }
            if let Some(edit) = override_edit(&text, &test, &flag.name) {
                edits.push(edit);
                rows.entry(path.clone()).or_default().push(Row {
                    mark: Mark::Removed,
                    subject: format!("{}: {}", flag.name, flag.written(keep)),
                    note: format!("in test \"{sentence}\""),
                });
            }
        }
        if !edits.is_empty() {
            loaded.set_text(path, crate::apply_edits(&text, edits));
        }
    }

    // 2. Each read, one at a time, the file read again after each, so a
    //    read inside a branch another read decided is seen as it now is.
    let kept = flag.written(keep);
    for path in &paths {
        let mut reads = 0u32;
        let mut branches = 0u32;
        for _ in 0..10_000 {
            let Some(text) = text_of(loaded, path) else {
                break;
            };
            let parse = jadpo_syntax::parse_file(&text);
            let root = parse.syntax();
            let Some(read) = root
                .descendants()
                .filter_map(ast::FieldExpr::cast)
                .find(|field| is_read(field.syntax(), &flag.name))
            else {
                break;
            };
            reads += 1;
            let edits = match decide(&text, read.syntax(), flag, keep) {
                Some(edits) => {
                    branches += 1;
                    edits
                }
                None => {
                    let range = read.syntax().text_range();
                    vec![(range.start().into(), range.end().into(), kept.clone())]
                }
            };
            loaded.set_text(path, crate::apply_edits(&text, edits));
        }
        if reads > 0 {
            let mut note = format!("{} read{}", reads, if reads == 1 { "" } else { "s" });
            if branches > 0 {
                note.push_str(&format!(", {branches} `if` decided by {kept}",));
            } else {
                note.push_str(&format!(" now {kept}"));
            }
            rows.entry(path.clone()).or_default().push(Row {
                mark: Mark::Changed,
                subject: format!("{FLAGS}.{}", flag.name),
                note,
            });
        }
    }

    // 3. The declaration.
    for path in &paths {
        let Some(text) = text_of(loaded, path) else {
            continue;
        };
        let parse = jadpo_syntax::parse_file(&text);
        let Some(source) = ast::SourceFile::cast(parse.syntax()) else {
            continue;
        };
        for decl in source.decls() {
            let syntax = decl.syntax();
            if !is_flags_block(syntax) {
                continue;
            }
            let members: Vec<ast::Member> = syntax
                .children()
                .find_map(ast::MemberList::cast)
                .map(|list| {
                    list.member_items()
                        .filter_map(|item| match item {
                            ast::MemberItem::Member(member) => Some(member),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default();
            let Some(own) = members.iter().find(|member| {
                member
                    .member_key()
                    .is_some_and(|key| crate::tree::written(key.syntax()) == flag.name)
            }) else {
                continue;
            };
            // The last flag leaves `flags { }` empty, so the flags' tables
            // and the log of their changes stay: dropping them is a
            // migration of its own.
            let note = if members.len() == 1 {
                "flag, the last: `flags { }` stays, keeping its tables"
            } else {
                "flag"
            };
            let after = remove_node(&text, own.syntax());
            rows.entry(path.clone()).or_default().push(Row {
                mark: Mark::Removed,
                subject: flag.name.clone(),
                note: note.to_string(),
            });
            loaded.set_text(path, after);
            break;
        }
    }

    // 4. Imports left unused, and each file formatted as it was.
    let touched: Vec<String> = paths
        .iter()
        .filter(|path| text_of(loaded, path) != *before.get(*path).unwrap_or(&None))
        .cloned()
        .collect();
    for path in &touched {
        if loaded.file(path).is_some() {
            crate::fix_imports(loaded, path);
        }
    }
    let is_project = loaded.project.layout(&loaded.db) == jadpo_db::Layout::Project;
    for path in &touched {
        let Some(after) = text_of(loaded, path) else {
            continue;
        };
        let was = before.get(path).cloned().flatten();
        let formatted = match &was {
            Some(was) => canonical(path, was, is_project).as_deref() == Some(was.as_str()),
            None => true,
        };
        if formatted && let Some(text) = canonical(path, &after, is_project) {
            loaded.set_text(path, text);
        }
    }

    // 5. An enum only the flag used, which nothing uses now.
    let mut problems: Vec<Diagnostic> = crate::check(&loaded.db, project);
    let mut touched = touched;
    if let Some(own) = &flag.enum_type {
        let short = own.rsplit('.').next().unwrap_or(own).to_string();
        let unused = problems.iter().any(|problem| {
            problem.rule.id == jadpo_diag::rule::LAYOUT_UNUSED_DECLARATION.id
                && problem.message == format!("`{short}` isn't used")
        });
        if unused && let Some(path) = remove_type(loaded, &paths, &short) {
            rows.entry(path.clone()).or_default().push(Row {
                mark: Mark::Removed,
                subject: short.clone(),
                note: "enum, used only by the flag".to_string(),
            });
            if !touched.contains(&path) {
                touched.push(path);
            }
            problems = crate::check(&loaded.db, project);
        }
    }

    // 6. Checked: a removal never leaves a project that doesn't.
    if !problems.is_empty() {
        let sources = jadpo_db::source_texts(&loaded.db, project);
        restore_texts(loaded, &before);
        return Ok(Err(Stopped { problems, sources }));
    }
    let mut files = Vec::new();
    for path in touched {
        let was = before.get(&path).cloned().flatten();
        let now = text_of(loaded, &path);
        if was == now {
            continue;
        }
        files.push(FileChange {
            rows: rows.remove(&path).unwrap_or_default(),
            path,
            before: was,
            after: now,
        });
    }
    Ok(Ok(Removed {
        files,
        tests: deleted_tests,
    }))
}

/// Removes the type declaration called `name` from the file among
/// `paths` that holds it, and the file when nothing else is left: the
/// file's path.
fn remove_type(loaded: &mut Loaded, paths: &[String], name: &str) -> Option<String> {
    for path in paths {
        let Some(text) = loaded.file(path).map(|file| file.text(&loaded.db).clone()) else {
            continue;
        };
        let parse = jadpo_syntax::parse_file(&text);
        let Some(source) = ast::SourceFile::cast(parse.syntax()) else {
            continue;
        };
        let decls: Vec<ast::Decl> = source.decls().collect();
        let Some(found) = decls.iter().find_map(|decl| match decl {
            ast::Decl::TypeDecl(ty)
                if ty
                    .name()
                    .and_then(|n| crate::tree::name_text(&n))
                    .as_deref()
                    == Some(name) =>
            {
                Some(ty.clone())
            }
            _ => None,
        }) else {
            continue;
        };
        if decls.len() == 1 {
            loaded.remove(path);
        } else {
            loaded.set_text(path, remove_node(&text, found.syntax()));
            crate::fix_imports(loaded, path);
        }
        return Some(path.clone());
    }
    None
}

/// Whether a test whose flags hold `value` takes every branch that leads
/// to `read`: each enclosing `if` whose condition a flag decides is true
/// for a read in its block and false for one in its `else`. A condition
/// no flag decides, or one inside an `else if` chain, holds.
fn guards_hold(
    read: &SyntaxNode,
    declared: &crate::flags::Flags,
    value: &dyn Fn(&str) -> Option<u32>,
) -> bool {
    let decided = |condition: Option<ast::Condition>| -> Option<bool> {
        let expr = condition?.expr()?;
        let field = expr
            .syntax()
            .descendants()
            .filter_map(ast::FieldExpr::cast)
            .find(|field| {
                let text = crate::tree::written(field.syntax());
                text.starts_with(&format!("{FLAGS}."))
            })?;
        let text: String = crate::tree::written(field.syntax())
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        let name = text.strip_prefix(&format!("{FLAGS}."))?;
        let flag = declared.flag(name)?;
        condition_value(&expr, field.syntax(), flag, value(name)?)
    };
    for block in read.ancestors().filter_map(ast::Block::cast) {
        let Some(parent) = block.syntax().parent() else {
            continue;
        };
        if let Some(stmt) = ast::IfStmt::cast(parent.clone()) {
            if decided(stmt.condition()) == Some(false) {
                return false;
            }
        } else if let Some(branch) = ast::ElseBranch::cast(parent)
            && let Some(stmt) = branch.syntax().parent().and_then(ast::IfStmt::cast)
            && stmt.else_ifs().next().is_none()
            && decided(stmt.condition()) == Some(true)
        {
            return false;
        }
    }
    true
}

/// Whether `node`, a field expression, is `flags.<flag>`.
fn is_read(node: &SyntaxNode, flag: &str) -> bool {
    let text: String = crate::tree::written(node)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    text == format!("{FLAGS}.{flag}")
}

/// Whether `node` is `flags { }`.
fn is_flags_block(node: &SyntaxNode) -> bool {
    node.children_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| !token.kind().is_trivia())
        .is_some_and(|token| token.text() == FLAGS)
}

/// A test's sentence, as its `test "..."` writes it.
fn test_sentence(test: &ast::TestDecl) -> String {
    let text = crate::tree::written(test.syntax());
    text.split_once('"')
        .and_then(|(_, rest)| rest.split_once('"'))
        .map_or_else(String::new, |(sentence, _)| sentence.to_string())
}

/// The edit that removes `node` whole, as [`remove_node`] does: from the
/// start of the line its first comment or code is on to the end of the
/// line its last code is on, leaving one blank line where there were two.
fn whole(text: &str, node: &SyntaxNode) -> (u32, u32, String) {
    let first = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .find(|token| token.kind() != SyntaxKind::WHITESPACE)
        .map_or_else(
            || u32::from(node.text_range().start()),
            |token| u32::from(token.text_range().start()),
        ) as usize;
    let start = text[..first].rfind('\n').map_or(0, |at| at + 1);
    let last = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .last()
        .map_or_else(
            || u32::from(node.text_range().end()),
            |token| u32::from(token.text_range().end()),
        ) as usize;
    let rest = &text[last..];
    let mut end = last + rest.find('\n').map_or(rest.len(), |at| at + 1);
    let (head, tail) = (&text[..start], &text[end..]);
    if head.ends_with("\n\n") && tail.starts_with('\n') {
        end += 1;
    } else if head.ends_with("{\n") && tail.starts_with('\n') {
        end += tail.len() - tail.trim_start_matches('\n').len();
    }
    (start as u32, end as u32, String::new())
}

/// The edit that drops `flag`'s line from a test's `flags:`, or the whole
/// `flags:` when it is the last.
fn override_edit(text: &str, test: &ast::TestDecl, flag: &str) -> Option<(u32, u32, String)> {
    let block = test.block()?;
    for item in block.block_items() {
        let ast::BlockItem::Member(member) = item else {
            continue;
        };
        let Some(key) = member.member_key() else {
            continue;
        };
        if crate::tree::written(key.syntax()) != FLAGS {
            continue;
        }
        let entries: Vec<(ast::Name, ast::Member)> =
            crate::tree::members(member.block()).into_iter().collect();
        let own = entries
            .iter()
            .find(|(name, _)| crate::tree::name_text(name).as_deref() == Some(flag))?;
        let target = if entries.len() == 1 {
            member.syntax().clone()
        } else {
            own.1.syntax().clone()
        };
        return Some(whole(text, &target));
    }
    None
}

/// The value a read's condition comes to once the flag holds `keep`:
/// the read alone, `!` it, or it compared with `==` or `!=` to one of
/// the flag's values; `None` for anything else.
fn condition_value(condition: &Expr, read: &SyntaxNode, flag: &Flag, keep: u32) -> Option<bool> {
    let bool_value = |at: u32| flag.enum_type.is_none().then_some(at == 1);
    match condition {
        Expr::FieldExpr(field) if field.syntax() == read => bool_value(keep),
        Expr::ParenExpr(paren) => condition_value(&paren.expr()?, read, flag, keep),
        Expr::PrefixExpr(prefix) if prefix.op()?.text() == "!" => {
            condition_value(&prefix.expr()?, read, flag, keep).map(|value| !value)
        }
        Expr::BinExpr(bin) => {
            let op = bin.op()?.text().to_string();
            if op != "==" && op != "!=" {
                return None;
            }
            let (lhs, rhs) = (bin.lhs()?, bin.rhs()?);
            let other = if lhs.syntax() == read {
                rhs
            } else if rhs.syntax() == read {
                lhs
            } else {
                return None;
            };
            let written: String = crate::tree::written(other.syntax())
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            let at = (0..flag.values.len() as u32).find(|at| flag.written(*at) == written)?;
            Some((at == keep) == (op == "=="))
        }
        _ => None,
    }
}

/// The edits that decide the `if` whose condition `read` decides, or
/// `None` when it decides none.
fn decide(
    text: &str,
    read: &SyntaxNode,
    flag: &Flag,
    keep: u32,
) -> Option<Vec<(u32, u32, String)>> {
    let condition = read.ancestors().find_map(ast::Condition::cast)?;
    let expr = condition.expr()?;
    // The read must be the condition itself, not inside a call in it.
    if !expr.syntax().descendants().any(|node| node == *read) {
        return None;
    }
    let value = condition_value(&expr, read, flag, keep)?;
    let owner = condition.syntax().parent()?;
    let start = |node: &SyntaxNode| u32::from(node.text_range().start());
    let end = |node: &SyntaxNode| u32::from(node.text_range().end());
    if let Some(stmt) = ast::IfStmt::cast(owner.clone()) {
        let else_ifs: Vec<ast::ElseIf> = stmt.else_ifs().collect();
        if value {
            let block = stmt.block()?;
            return Some(unwrap(text, stmt.syntax(), &block));
        }
        if let Some(first) = else_ifs.first() {
            // `if (c) { } else if (d) { }` becomes `if (d) { }`.
            let from_if = first.if_kw_token()?;
            return Some(vec![(
                start(stmt.syntax()),
                u32::from(from_if.text_range().start()),
                String::new(),
            )]);
        }
        return Some(match stmt.else_branch().and_then(|branch| branch.block()) {
            Some(block) => unwrap(text, stmt.syntax(), &block),
            None => vec![whole(text, stmt.syntax())],
        });
    }
    let else_if = ast::ElseIf::cast(owner)?;
    let stmt = ast::IfStmt::cast(else_if.syntax().parent()?)?;
    if value {
        // What runs here now runs whenever the branches before don't.
        let block = else_if.block()?;
        return Some(vec![(
            start(else_if.syntax()),
            end(stmt.syntax()),
            format!("else {}", crate::tree::written(block.syntax())),
        )]);
    }
    Some(vec![(
        start(else_if.syntax()),
        end(else_if.syntax()),
        String::new(),
    )])
}

/// The edits that put `block`'s statements in place of `stmt`, and drop
/// what follows it in the same block when they end by returning.
fn unwrap(text: &str, stmt: &SyntaxNode, block: &ast::Block) -> Vec<(u32, u32, String)> {
    let inner = {
        let (open, close) = (block.l_brace_token(), block.r_brace_token());
        match (open, close) {
            (Some(open), Some(close)) => {
                let from = u32::from(open.text_range().end()) as usize;
                let to = u32::from(close.text_range().start()) as usize;
                text[from..to].trim_matches('\n').to_string()
            }
            _ => String::new(),
        }
    };
    let returns = block
        .block_items()
        .last()
        .is_some_and(|item| item.syntax().kind() == SyntaxKind::RETURN_STMT);
    let range = stmt.text_range();
    let mut end: u32 = range.end().into();
    if returns
        && let Some(parent) = stmt.parent()
        && let Some(outer) = ast::Block::cast(parent)
        && let Some(close) = outer.r_brace_token()
    {
        // Up to the last statement before the enclosing `}`.
        let last = outer
            .block_items()
            .last()
            .map_or(end, |item| u32::from(item.syntax().text_range().end()));
        let close: u32 = close.text_range().start().into();
        end = last.max(end).min(close);
    }
    let dedented: Vec<String> = inner
        .lines()
        .map(|line| line.strip_prefix("    ").unwrap_or(line).to_string())
        .collect();
    let replacement = dedented.join("\n");
    vec![(
        range.start().into(),
        end,
        replacement.trim_start().to_string(),
    )]
}

/// Whether the project `project` declares flag `name`, for the CLI.
pub fn find(db: &dyn jadpo_db::Db, project: Project, name: &str) -> Option<Flag> {
    crate::flags::flags(db, project).flag(name).cloned()
}
