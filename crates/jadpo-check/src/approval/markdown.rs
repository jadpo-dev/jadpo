//! The approval report as Markdown, `build/review/approval.md`: read by
//! the person who decides, on the git host or in an editor, so it says
//! first what needs them, each decision a heading of its own with the
//! path that makes it true, then what else changed, then the source.
//! Records are shown as `diff` blocks of their entries, which a git host
//! colours; places are written `file:line`, which an editor opens and an
//! agent searches for.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::{Acceptance, Mark, Report, Section, Step, plural};

fn count(n: usize, one: &str) -> String {
    format!("{n} {}", plural(one, n))
}

fn capital(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn place(file: &str, line: i64) -> String {
    format!("`{file}:{line}`")
}

/// A path as one line: the actor in words, each declaration as code.
pub fn path_line(path: &[Step]) -> String {
    path.iter()
        .map(|step| {
            if step.kind == "actor" {
                step.name.clone()
            } else {
                format!("`{}`", step.name)
            }
        })
        .collect::<Vec<_>>()
        .join(" → ")
}

pub fn render(report: &Report) -> String {
    let mut out = String::new();
    let decisions = report.decisions.len();
    let accepted = report
        .decisions
        .iter()
        .filter(|d| d.acceptance.decided())
        .count();
    let _ = writeln!(out, "# Approval\n");
    let _ = writeln!(
        out,
        "This change compared with {}: what needs a person first, then what else it does, then its source. Written by `jadpo approval` from the two builds' manifests; `jadpo check` fails until each decision has its acceptance.\n",
        report.base.described
    );
    let mut facts = vec![if decisions == 0 {
        "No decision needs a person".to_string()
    } else {
        format!(
            "**{} {} a person**, {accepted} accepted",
            count(decisions, "decision"),
            if decisions == 1 { "needs" } else { "need" }
        )
    }];
    facts.push(format!(
        "{} newly reachable",
        count(report.reachable.len(), "declaration")
    ));
    facts.push(if report.source.is_empty() {
        "no declaration changed".to_string()
    } else {
        format!(
            "{} changed in {}",
            count(report.source.len(), "declaration"),
            count(report.source_files(), "file")
        )
    });
    let _ = writeln!(out, "{}\n", facts.join(" · "));
    if report.base_errors > 0 {
        let _ = writeln!(
            out,
            "> The before side has {} with this compiler, so its graph leaves out what couldn't be resolved: newly reachable code and the source compare with what it could build.\n",
            count(report.base_errors as usize, "error")
        );
    }

    let _ = writeln!(out, "## Decisions that need a person\n");
    for path in &report.not_judged {
        let _ = writeln!(
            out,
            "> `{path}` is new since {}, so the changes in it aren't judged.\n",
            report.base.described
        );
    }
    if report.decisions.is_empty() {
        let _ = writeln!(
            out,
            "None: nothing this change does needs a person's acceptance.\n"
        );
    } else {
        let _ = writeln!(
            out,
            "Each change that needs a person to accept it, with the path that makes it true. `jadpo acceptance new <purpose>` drafts the acceptance, and a person writes why on a `///` line above `acceptance`.\n"
        );
    }
    for decision in &report.decisions {
        let need = &decision.need;
        let _ = writeln!(out, "### {}\n", capital(&need.said));
        let entry = format!("`{}: {}`", need.subject, need.change);
        match &decision.acceptance {
            Acceptance::Missing => {
                let _ = writeln!(
                    out,
                    "- **Acceptance:** missing. No acceptance this change adds lists {entry}."
                );
            }
            Acceptance::NoReason { file } => {
                let _ = writeln!(
                    out,
                    "- **Acceptance:** `{file}` lists {entry}, and has no reason yet."
                );
            }
            Acceptance::Accepted { file, reason } => {
                let _ = writeln!(out, "- **Acceptance:** accepted in `{file}`.");
                let _ = writeln!(out, "- **Reason:** {reason}");
            }
        }
        let (record, line) = &decision.record;
        match (line, &need.heading) {
            (Some(line), Some(heading)) => {
                let _ = writeln!(out, "- **Shows in:** {}, `{heading}`", place(record, *line));
            }
            _ => {
                let _ = writeln!(out, "- **Shows in:** `{record}`");
            }
        }
        let paths = &decision.paths;
        match paths.shown.as_slice() {
            [] if paths.sought => {
                let _ = writeln!(
                    out,
                    "- **Path:** no route, schedule or subscriber reaches it yet."
                );
            }
            [] => {}
            [one] if paths.more == 0 => {
                let _ = writeln!(out, "- **Path:** {}", path_line(one));
            }
            shown => {
                let _ = writeln!(out, "- **Paths:**");
                for path in shown {
                    let _ = writeln!(out, "  - {}", path_line(path));
                }
                if paths.more > 0 {
                    let _ = writeln!(out, "  - and {} more", count(paths.more, "path"));
                }
            }
        }
        let mut places: Vec<String> = Vec::new();
        for step in paths.shown.iter().flatten() {
            if let Some((file, line)) = &step.place {
                let shown = place(file, *line);
                if !places.contains(&shown) {
                    places.push(shown);
                }
            }
        }
        if !places.is_empty() {
            let _ = writeln!(out, "- **Where:** {}", places.join(", "));
        }
        out.push('\n');
    }

    let _ = writeln!(out, "## Newly reachable code\n");
    if report.reachable.is_empty() {
        let _ = writeln!(
            out,
            "None: nothing this change left alone is reached from anywhere new.\n"
        );
    } else {
        let _ = writeln!(
            out,
            "Declarations this change didn't edit that are now reached from somewhere new, which a line diff never shows.\n"
        );
        let _ = writeln!(out, "| Declaration | Now reached from | Where |");
        let _ = writeln!(out, "|---|---|---|");
        for reachable in &report.reachable {
            let from: Vec<String> = reachable.from.iter().map(|f| format!("`{f}`")).collect();
            let at = reachable
                .place
                .as_ref()
                .map_or_else(String::new, |(file, line)| place(file, *line));
            let _ = writeln!(
                out,
                "| {} `{}` | {} | {at} |",
                reachable.kind,
                reachable.name,
                from.join(", ")
            );
        }
        out.push('\n');
    }

    let _ = writeln!(out, "## Other behaviour\n");
    for section in &report.other {
        other(&mut out, section);
    }

    let _ = writeln!(out, "## Source by declaration\n");
    if report.source.is_empty() {
        let _ = writeln!(out, "No declaration changed.");
        return out;
    }
    let _ = writeln!(
        out,
        "{} in {}, by domain.\n",
        count(report.source.len(), "declaration"),
        count(report.source_files(), "file")
    );
    let mut by_domain: BTreeMap<Option<&str>, Vec<&super::SourceChange>> = BTreeMap::new();
    for change in &report.source {
        by_domain
            .entry(change.domain.as_deref())
            .or_default()
            .push(change);
    }
    let groups = by_domain.len();
    // The domains first, then what is outside them.
    let mut by_domain: Vec<(Option<&str>, Vec<&super::SourceChange>)> =
        by_domain.into_iter().collect();
    by_domain.sort_by_key(|(domain, _)| domain.is_none());
    for (domain, changes) in by_domain {
        if groups > 1 || domain.is_some() {
            let _ = match domain {
                Some(domain) => writeln!(out, "### `{domain}`\n"),
                None => writeln!(out, "### Outside the domains\n"),
            };
        }
        let _ = writeln!(out, "| Declaration | Change | Where |");
        let _ = writeln!(out, "|---|---|---|");
        for change in changes {
            let mark = match &change.renamed_from {
                Some(was) => format!("renamed from `{was}`"),
                None => change.mark.word().to_string(),
            };
            let lines = if change.lines.0 == change.lines.1 {
                change.lines.0.to_string()
            } else {
                format!("{}-{}", change.lines.0, change.lines.1)
            };
            let _ = writeln!(
                out,
                "| {} `{}` | {mark} | `{}:{lines}` |",
                change.kind.replace('_', " "),
                change.name,
                change.file
            );
        }
        out.push('\n');
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

fn other(out: &mut String, section: &Section) {
    let _ = writeln!(out, "### {}\n", section.title);
    if section.is_empty() {
        let _ = writeln!(out, "{}\n", section.note.as_deref().unwrap_or("No change."));
        return;
    }
    let mut records: BTreeMap<&str, Vec<&super::EntryDiff>> = BTreeMap::new();
    for entry in &section.entries {
        records
            .entry(entry.record.as_str())
            .or_default()
            .push(entry);
    }
    for (record, entries) in records {
        let mut counts: BTreeMap<Mark, usize> = BTreeMap::new();
        for entry in &entries {
            *counts.entry(entry.mark).or_default() += 1;
        }
        let said: Vec<String> = counts
            .iter()
            .map(|(mark, n)| format!("{} {}", count(*n, "entry"), mark.word()))
            .collect();
        let _ = writeln!(out, "`{record}`: {}.\n", said.join(", "));
        let _ = writeln!(out, "```diff");
        for (i, entry) in entries.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let sign = match entry.mark {
                Mark::Added => '+',
                Mark::Removed => '-',
                Mark::Changed => ' ',
            };
            let _ = writeln!(out, "{sign} {}", entry.heading);
            for (mark, line) in &entry.lines {
                let sign = if *mark == Mark::Added { '+' } else { '-' };
                let _ = writeln!(out, "{sign} {line}");
            }
        }
        let _ = writeln!(out, "```\n");
    }
    for line in &section.lines {
        let at = line
            .place
            .as_ref()
            .map_or_else(String::new, |(file, at)| format!(", {}", place(file, *at)));
        let _ = writeln!(out, "- {}{at}", capital(&line.said));
    }
    if !section.lines.is_empty() {
        out.push('\n');
    }
}
