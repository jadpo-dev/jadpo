use jadpo_diagnostics::{diagnostic_help_url, Diagnostic, Severity};
use std::env;
use std::fs;
use std::io::{self, IsTerminal};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutChoice {
    Auto,
    Human,
    Plain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Presentation {
    pub layout: LayoutChoice,
    pub color: ColorChoice,
}

impl Default for Presentation {
    fn default() -> Self {
        Self {
            layout: LayoutChoice::Auto,
            color: ColorChoice::Auto,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderStyle {
    rich: bool,
    color: bool,
    unicode: bool,
    width: usize,
}

impl RenderStyle {
    #[cfg(test)]
    pub const fn plain() -> Self {
        Self {
            rich: false,
            color: false,
            unicode: false,
            width: 100,
        }
    }

    #[cfg(test)]
    pub const fn rich_for_test(color: bool, unicode: bool, width: usize) -> Self {
        Self {
            rich: true,
            color,
            unicode,
            width,
        }
    }
}

static PRESENTATION: OnceLock<Presentation> = OnceLock::new();

pub fn configure(presentation: Presentation) {
    let _ = PRESENTATION.set(presentation);
}

pub fn stderr_style() -> RenderStyle {
    style_for(io::stderr().is_terminal())
}

pub fn stdout_style() -> RenderStyle {
    style_for(io::stdout().is_terminal())
}

fn style_for(terminal: bool) -> RenderStyle {
    let presentation = PRESENTATION.get().copied().unwrap_or_default();
    let capable_terminal = terminal && env::var("TERM").map_or(true, |term| term != "dumb");
    let rich = match presentation.layout {
        LayoutChoice::Auto => capable_terminal || presentation.color == ColorChoice::Always,
        LayoutChoice::Human => true,
        LayoutChoice::Plain => false,
    };
    let color = if env::var_os("NO_COLOR").is_some() {
        false
    } else {
        match presentation.color {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => capable_terminal,
        }
    };
    let unicode = rich && env::var_os("JADPO_ASCII").is_none();
    let width = env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(100)
        .clamp(60, 140);
    RenderStyle {
        rich,
        color,
        unicode,
        width,
    }
}

pub fn render_diagnostic(diagnostic: &Diagnostic, style: RenderStyle) -> String {
    if !style.rich {
        return render_plain_diagnostic(diagnostic);
    }
    render_rich_diagnostic(diagnostic, style)
}

pub fn render_check_success(
    source_files: usize,
    declarations: usize,
    semantic_nodes: usize,
    style: RenderStyle,
) -> String {
    if !style.rich {
        return format!(
            "semantic check passed: {source_files} source file(s), {declarations} declaration(s), {semantic_nodes} semantic node(s)\n"
        );
    }
    let mark = if style.unicode { "✓" } else { "+" };
    let separator = separator(style);
    format!(
        "{} {}\n  {}{separator}{}{separator}{}\n",
        paint(mark, "1;32", style.color),
        paint("Check passed", "1;32", style.color),
        plural(source_files, "source", "sources"),
        plural(declarations, "declaration", "declarations"),
        plural(semantic_nodes, "semantic node", "semantic nodes"),
    )
}

pub fn render_build_success(artifact_count: usize, output: &str, style: RenderStyle) -> String {
    if !style.rich {
        return format!("built {artifact_count} derived and target file(s) beneath {output}\n");
    }
    let mark = if style.unicode { "✓" } else { "+" };
    let arrow = if style.unicode { "→" } else { "->" };
    format!(
        "{} {}\n  {} {arrow} {}\n",
        paint(mark, "1;32", style.color),
        paint("Build complete", "1;32", style.color),
        plural(artifact_count, "generated file", "generated files"),
        paint(output, "36", style.color),
    )
}

pub fn render_success(plain: &str, title: &str, detail: &str, style: RenderStyle) -> String {
    if !style.rich {
        return format!("{plain}\n");
    }
    let mark = if style.unicode { "✓" } else { "+" };
    let mut output = format!(
        "{} {}\n",
        paint(mark, "1;32", style.color),
        paint(title, "1;32", style.color),
    );
    if !detail.is_empty() {
        let detail = if style.unicode {
            detail.to_owned()
        } else {
            detail.replace(" · ", " | ").replace(" → ", " -> ")
        };
        for line in wrap(&detail, style.width.saturating_sub(4).max(30)) {
            output.push_str(&format!("  {line}\n"));
        }
    }
    output
}

pub fn render_lifecycle(
    command: &str,
    event: &str,
    revision: u64,
    status: &str,
    stale: bool,
    style: RenderStyle,
) -> String {
    if !style.rich {
        return format!(
            "{command}[{event}]: revision {revision}, status {status}, stale {stale}\n"
        );
    }
    let (glyph, color) = match status {
        "passed" | "succeeded" | "ready" | "running" | "built" => {
            (if style.unicode { "●" } else { "+" }, "32")
        }
        "failed" | "stopped" => (if style.unicode { "×" } else { "x" }, "31"),
        _ => (if style.unicode { "◆" } else { ">" }, "36"),
    };
    let separator = separator(style);
    let stale_suffix = if stale {
        format!("{separator}stale")
    } else {
        String::new()
    };
    format!(
        "{} {}  {}\n",
        paint(glyph, &format!("1;{color}"), style.color),
        paint(&format!("{command}{separator}{event}"), "1", style.color,),
        paint(
            &format!("revision {revision}{separator}{status}{stale_suffix}"),
            "2",
            style.color,
        ),
    )
}

pub fn render_help(style: RenderStyle) -> String {
    let heading = |value: &str| paint(value, "1;36", style.color && style.rich);
    let command = |value: &str| paint(value, "1", style.color && style.rich);
    let product = if style.rich {
        format!(
            "{}  {}\n{}\n",
            paint("Jadpo", "1;32", style.color),
            paint(
                "a programming environment for coding agents",
                "2",
                style.color
            ),
            paint("https://jadpo.dev/", "4;36", style.color),
        )
    } else {
        "Jadpo - a programming environment for coding agents\nhttps://jadpo.dev/\n".to_owned()
    };

    format!(
        "{product}\n{}\n  {}\n\n{}\n  {:11} Create a deterministic project scaffold\n  {:11} Validate syntax and semantics\n  {:11} Emit the checked semantic manifest\n  {:11} Write deterministic derived artifacts\n  {:11} Inspect before/after behavioral review JSON\n  {:17} Inspect an exact compiler-refused protected change\n  {:11} Generate the checked Bun target\n  {:11} Build and run authored tests\n  {:11} Format authored .jadpo files\n\n{}\n  {:11} Rebuild after authored-input changes\n  {:11} Rebuild, run Bun, and restart when ready\n  {:11} Enrich a secret-safe runtime event locally\n  {:11} Run the compiler-backed language server\n\n{}\n  {}\n  {}\n\n{}\n  {}\n  {}\n  {}\n  {}\n\n{}\n  {:30} Rich terminal, stable plain text, or diagnostic JSON\n  {:30} Automatic, forced, or disabled ANSI colour\n  {:30} Disable colour using the shared convention\n  {:30} Use ASCII decoration in rich output\n\n{}\n  jadpo build --target native|wasm\n  Build the bounded shared-Rust fixture from a repository checkout (no Bun).\n  Ordinary project builds still default to Bun; --target rust aliases native.\n\n{}\n  Diagnostic JSON is available for check, watch, and dev. Other machine-oriented\n  commands emit JSON directly. Pipes default to ANSI-free plain text.\n",
        heading("USAGE"),
        command("jadpo <command> <project> [options]"),
        heading("CORE COMMANDS"),
        command("new"),
        command("check"),
        command("inspect"),
        command("artifacts"),
        command("approval"),
        command("protected-attempt"),
        command("build"),
        command("test"),
        command("fmt"),
        heading("DEVELOPMENT"),
        command("watch"),
        command("dev"),
        command("incident"),
        command("lsp"),
        heading("CONFIGURATION"),
        command("jadpo config set <field>"),
        command("jadpo config check"),
        heading("SCHEMA"),
        command("jadpo schema <init|check|add|rename> <project> ..."),
        command("jadpo schema <snapshot|diff> <project> ..."),
        command("jadpo schema <decision-template|decision-check> <project> ..."),
        command("jadpo schema <plan|sql|index-recommend|index-accept> <project> ..."),
        heading("PRESENTATION"),
        command("--diagnostic-format=human|plain|json"),
        command("--color=auto|always|never"),
        command("NO_COLOR=1"),
        command("JADPO_ASCII=1"),
        heading("EXPERIMENTAL BUILDS"),
        heading("AUTOMATION"),
    )
}

fn render_plain_diagnostic(diagnostic: &Diagnostic) -> String {
    let mut output = format!("{}: {}\n", diagnostic.severity, diagnostic.message);
    if let Some(primary) = &diagnostic.primary {
        match fs::read_to_string(&primary.source)
            .ok()
            .and_then(|source| source_excerpt(&source, primary.start, primary.end))
        {
            Some(excerpt) => {
                output.push_str(&format!(
                    " --> {}:{}:{}\n",
                    primary.source, excerpt.line, excerpt.column
                ));
                let gutter = excerpt.line.to_string().len();
                output.push_str(&format!("{} |\n", " ".repeat(gutter)));
                output.push_str(&format!("{} | {}\n", excerpt.line, excerpt.text));
                output.push_str(&format!(
                    "{} | {}{}\n",
                    " ".repeat(gutter),
                    " ".repeat(excerpt.column.saturating_sub(1)),
                    "^".repeat(excerpt.width)
                ));
            }
            None => output.push_str(&format!(
                "  at {}:{}..{}\n",
                primary.source, primary.start, primary.end
            )),
        }
    }
    output.push_str(&format!("  reason: {}\n", diagnostic.reason));
    let next_label = match diagnostic.recommended_next_step.kind.as_str() {
        "automatic_fix" => "fix",
        "human_decision" => "decision",
        _ => "next",
    };
    output.push_str(&format!(
        "  {next_label}: {} ({}, owner: {})\n",
        diagnostic.recommended_next_step.title,
        diagnostic.recommended_next_step.kind.as_str(),
        diagnostic.decision_owner.as_str()
    ));
    for edit in &diagnostic.recommended_next_step.edits {
        output.push_str(&format!(
            "    edit: {}:{}..{} -> `{}`\n",
            edit.source,
            edit.start,
            edit.end,
            visible_replacement(&edit.replacement)
        ));
    }
    for alternative in &diagnostic.alternatives {
        output.push_str(&format!(
            "  alternative: {} ({}, owner: {})\n    reason: {}\n    behavioral impact: {}\n    public contract impact: {}\n",
            alternative.title,
            alternative.kind.as_str(),
            alternative.decision_owner.as_str(),
            alternative.reason,
            alternative.behavioral_effect,
            alternative.public_contract_effect,
        ));
        for edit in &alternative.edits {
            output.push_str(&format!(
                "    edit: {}:{}..{} -> `{}`\n",
                edit.source,
                edit.start,
                edit.end,
                visible_replacement(&edit.replacement)
            ));
        }
    }
    output.push_str(&format!(
        "  behavioral impact: {}\n  public contract impact: {}\n",
        diagnostic.impact.behavioral, diagnostic.impact.public_contract
    ));
    if !diagnostic.impact.affected.is_empty() {
        output.push_str(&format!(
            "  affected: {}\n",
            diagnostic.impact.affected.join(", ")
        ));
    }
    output.push_str(&format!(
        "  rule: {}\n  help: {}\n",
        diagnostic.rule_id,
        diagnostic_help_url(&diagnostic.help_id)
    ));
    for note in &diagnostic.notes {
        output.push_str(&format!("  note: {note}\n"));
    }
    output
}

fn render_rich_diagnostic(diagnostic: &Diagnostic, style: RenderStyle) -> String {
    let (glyph, severity_color) = match diagnostic.severity {
        Severity::Error => (if style.unicode { "×" } else { "x" }, "31"),
        Severity::Warning => (if style.unicode { "▲" } else { "!" }, "33"),
        Severity::Note => (if style.unicode { "●" } else { "i" }, "36"),
    };
    let mut output = format!(
        "{} {}  {}\n",
        paint(glyph, &format!("1;{severity_color}"), style.color),
        paint(
            &diagnostic.severity.to_string(),
            &format!("1;{severity_color}"),
            style.color,
        ),
        paint(&diagnostic.message, "1", style.color),
    );

    if let Some(primary) = &diagnostic.primary {
        let excerpt = fs::read_to_string(&primary.source)
            .ok()
            .and_then(|source| source_excerpt(&source, primary.start, primary.end));
        if let Some(excerpt) = excerpt {
            render_source_frame(
                &mut output,
                &primary.source,
                &excerpt,
                severity_color,
                style,
            );
        } else {
            labeled(
                &mut output,
                "At",
                &format!("{}:{}..{}", primary.source, primary.start, primary.end),
                style,
            );
        }
    }

    labeled(&mut output, "Why", &diagnostic.reason, style);
    let next_label = match diagnostic.recommended_next_step.kind.as_str() {
        "automatic_fix" => "Fix",
        "human_decision" => "Decision",
        _ => "Next",
    };
    labeled(
        &mut output,
        next_label,
        &diagnostic.recommended_next_step.title,
        style,
    );
    let repair_meta = format!(
        "{}{}{}-owned",
        diagnostic.recommended_next_step.kind.as_str(),
        separator(style),
        diagnostic.decision_owner.as_str()
    );
    continuation(&mut output, &repair_meta, style, true);

    for edit in &diagnostic.recommended_next_step.edits {
        let replacement = visible_replacement(&edit.replacement);
        continuation(
            &mut output,
            &format!(
                "edit {}:{}..{} {} `{replacement}`",
                edit.source,
                edit.start,
                edit.end,
                if style.unicode { "→" } else { "->" }
            ),
            style,
            false,
        );
    }

    if !diagnostic.alternatives.is_empty() {
        labeled(
            &mut output,
            if diagnostic.recommended_next_step.kind.as_str() == "human_decision" {
                "Choices"
            } else {
                "Other"
            },
            "Valid alternatives",
            style,
        );
        for (index, alternative) in diagnostic.alternatives.iter().enumerate() {
            continuation(
                &mut output,
                &format!(
                    "{}. {} {} {}",
                    index + 1,
                    alternative.title,
                    if style.unicode { "—" } else { "-" },
                    alternative.reason
                ),
                style,
                false,
            );
            continuation(
                &mut output,
                &format!(
                    "Behavior: {} Public contract: {}",
                    alternative.behavioral_effect, alternative.public_contract_effect
                ),
                style,
                true,
            );
            for edit in &alternative.edits {
                continuation(
                    &mut output,
                    &format!(
                        "edit {}:{}..{} {} `{}`",
                        edit.source,
                        edit.start,
                        edit.end,
                        if style.unicode { "→" } else { "->" },
                        visible_replacement(&edit.replacement)
                    ),
                    style,
                    false,
                );
            }
        }
    }

    if !diagnostic.context.is_empty() {
        let context = diagnostic
            .context
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(separator(style));
        labeled(&mut output, "Context", &context, style);
    }
    labeled(
        &mut output,
        "Impact",
        &format!(
            "{} Public contract: {}",
            diagnostic.impact.behavioral, diagnostic.impact.public_contract
        ),
        style,
    );
    if !diagnostic.impact.affected.is_empty() {
        continuation(
            &mut output,
            &format!("Affected: {}", diagnostic.impact.affected.join(", ")),
            style,
            true,
        );
    }
    for related in &diagnostic.related {
        labeled(
            &mut output,
            "Related",
            &format!("{}:{}..{}", related.source, related.start, related.end),
            style,
        );
    }
    for note in &diagnostic.notes {
        labeled(&mut output, "Note", note, style);
    }

    metadata(
        &mut output,
        "Rule",
        &format!(
            "{}{}{}",
            diagnostic.rule_id,
            separator(style),
            diagnostic_help_url(&diagnostic.help_id)
        ),
        style,
    );
    output.push('\n');
    output
}

fn render_source_frame(
    output: &mut String,
    source: &str,
    excerpt: &SourceExcerpt,
    severity_color: &str,
    style: RenderStyle,
) {
    let (top, vertical, bottom) = if style.unicode {
        ("╭─", "│", "╰─")
    } else {
        ("+-", "|", "+-")
    };
    output.push_str(&format!(
        "  {} {}\n",
        paint(top, "2", style.color),
        paint(
            &format!("{source}:{}:{}", excerpt.line, excerpt.column),
            "36",
            style.color,
        )
    ));
    let gutter = excerpt.line.to_string().len();
    output.push_str(&format!(
        "  {} {}\n",
        " ".repeat(gutter),
        paint(vertical, "2", style.color)
    ));
    output.push_str(&format!(
        "  {} {} {}\n",
        paint(&excerpt.line.to_string(), "2", style.color),
        paint(vertical, "2", style.color),
        excerpt.text
    ));
    output.push_str(&format!(
        "  {} {} {}{}\n",
        " ".repeat(gutter),
        paint(vertical, "2", style.color),
        " ".repeat(excerpt.column.saturating_sub(1)),
        paint(
            &"^".repeat(excerpt.width),
            &format!("1;{severity_color}"),
            style.color,
        )
    ));
    output.push_str(&format!(
        "  {} {}\n",
        " ".repeat(gutter),
        paint(bottom, "2", style.color)
    ));
}

fn labeled(output: &mut String, label: &str, value: &str, style: RenderStyle) {
    let label_width = 9usize;
    let available = style.width.saturating_sub(label_width + 3).max(30);
    let lines = wrap(value, available);
    for (index, line) in lines.iter().enumerate() {
        if index == 0 {
            let padded_label = format!("{label:label_width$}");
            output.push_str(&format!(
                "  {} {}\n",
                paint(&padded_label, "1;36", style.color),
                line,
            ));
        } else {
            output.push_str(&format!("  {:label_width$} {}\n", "", line));
        }
    }
}

fn continuation(output: &mut String, value: &str, style: RenderStyle, dim: bool) {
    for line in wrap(value, style.width.saturating_sub(14).max(30)) {
        let rendered = if dim {
            paint(&line, "2", style.color)
        } else {
            line
        };
        output.push_str(&format!("  {:9}   {rendered}\n", ""));
    }
}

fn metadata(output: &mut String, label: &str, value: &str, style: RenderStyle) {
    let label_width = 9usize;
    let available = style.width.saturating_sub(label_width + 3).max(30);
    for (index, line) in wrap(value, available).iter().enumerate() {
        let raw = if index == 0 {
            format!("  {label:label_width$} {line}\n")
        } else {
            format!("  {:label_width$} {line}\n", "")
        };
        output.push_str(&paint(&raw, "2", style.color));
    }
}

fn wrap(value: &str, width: usize) -> Vec<String> {
    if value.is_empty() {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in value.split_whitespace() {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > width {
            lines.push(current);
            current = String::new();
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn visible_replacement(value: &str) -> String {
    if value.is_empty() {
        return "<delete>".to_owned();
    }
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
        .replace('`', "\\`")
}

fn paint(value: &str, code: &str, enabled: bool) -> String {
    if enabled {
        format!("\u{1b}[{code}m{value}\u{1b}[0m")
    } else {
        value.to_owned()
    }
}

pub fn plural(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

fn separator(style: RenderStyle) -> &'static str {
    if style.unicode {
        " · "
    } else {
        " | "
    }
}

struct SourceExcerpt {
    line: usize,
    column: usize,
    width: usize,
    text: String,
}

fn source_excerpt(source: &str, start: usize, end: usize) -> Option<SourceExcerpt> {
    if start > source.len() || !source.is_char_boundary(start) {
        return None;
    }
    let end = end.min(source.len());
    if !source.is_char_boundary(end) {
        return None;
    }
    let line_start = source[..start].rfind('\n').map_or(0, |offset| offset + 1);
    let line_end = source[start..]
        .find('\n')
        .map_or(source.len(), |offset| start + offset);
    let marker_end = end
        .max(start + usize::from(start < source.len()))
        .min(line_end);
    let width = if marker_end > start {
        source[start..marker_end].chars().count().max(1)
    } else {
        1
    };
    Some(SourceExcerpt {
        line: source[..line_start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1,
        column: source[line_start..start].chars().count() + 1,
        width,
        text: source[line_start..line_end]
            .strip_suffix('\r')
            .unwrap_or(&source[line_start..line_end])
            .to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        render_diagnostic, render_help, render_lifecycle, render_success, ColorChoice,
        LayoutChoice, Presentation, RenderStyle,
    };
    use jadpo_core::analyze_project;
    use jadpo_diagnostics::{Diagnostic, SourceSpan, TextEdit, CATALOGUE_CODES};

    #[test]
    fn plain_output_is_deterministic_and_escape_free() {
        let diagnostic = Diagnostic::error("FAIL_ATTEMPT_REQUIRED");
        let output = render_diagnostic(&diagnostic, RenderStyle::plain());

        assert!(output.starts_with("error: This operation can fail and requires `attempt`\n"));
        assert!(!output.contains('\u{1b}'));
        assert!(!output.contains('╭'));
    }

    #[test]
    fn rich_output_has_hierarchy_source_and_repair_without_changing_source() {
        let root =
            std::env::temp_dir().join(format!("jadpo-terminal-renderer-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("fixture directory");
        let source = root.join("app.jadpo");
        std::fs::write(&source, "return child()\n").expect("fixture source");
        let mut diagnostic = Diagnostic::error("FAIL_ATTEMPT_REQUIRED").with_edit(TextEdit {
            source: source.to_string_lossy().into_owned(),
            start: 7,
            end: 7,
            replacement: "attempt ".to_owned(),
        });
        diagnostic.primary = Some(SourceSpan {
            source: source.to_string_lossy().into_owned(),
            start: 7,
            end: 12,
        });

        let output = render_diagnostic(&diagnostic, RenderStyle::rich_for_test(false, true, 88));

        assert!(output.contains("× error  This operation can fail and requires `attempt`"));
        assert!(output.contains("╭─"));
        assert!(output.contains("return child()"));
        assert!(output.contains("^^^^^"));
        assert!(output.contains("Fix"));
        assert!(output.contains("`attempt `"));
        assert!(!output.contains('\u{1b}'));
        assert!(output.ends_with("\n\n"));
        assert_eq!(
            std::fs::read_to_string(&source).unwrap(),
            "return child()\n"
        );
        std::fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn presentation_defaults_protect_noninteractive_consumers() {
        assert_eq!(Presentation::default().layout, LayoutChoice::Auto);
        assert_eq!(Presentation::default().color, ColorChoice::Auto);
    }

    #[test]
    fn rich_colour_is_opt_in_and_ascii_mode_has_no_unicode_framing() {
        let diagnostic = Diagnostic::error("FAIL_ATTEMPT_REQUIRED");
        let output = render_diagnostic(&diagnostic, RenderStyle::rich_for_test(true, false, 80));
        let lifecycle = render_lifecycle(
            "watch",
            "build",
            7,
            "passed",
            false,
            RenderStyle::rich_for_test(true, false, 80),
        );

        assert!(output.contains("\u{1b}[1;31m"));
        assert!(output.starts_with("\u{1b}[1;31mx"));
        assert!(!output.contains('×'));
        assert!(!output.contains('╭'));
        assert!(!lifecycle.contains('·'));
        assert!(lifecycle.contains("watch | build"));
    }

    #[test]
    fn the_whole_human_surface_has_a_plain_escape_free_fallback() {
        let style = RenderStyle::plain();
        let help = render_help(style);
        let success = render_success("done", "Done", "one detail", style);
        let lifecycle = render_lifecycle("watch", "build", 7, "passed", false, style);

        assert!(help.contains("CORE COMMANDS"));
        assert!(help.contains("a programming environment for coding agents"));
        assert!(!help.contains("one language for the whole web"));
        assert!(help.contains("Diagnostic JSON is available for check, watch, and dev"));
        assert_eq!(success, "done\n");
        assert_eq!(
            lifecycle,
            "watch[build]: revision 7, status passed, stale false\n"
        );
        assert!(!format!("{help}{success}{lifecycle}").contains('\u{1b}'));
    }

    #[test]
    fn every_catalogue_code_has_complete_plain_and_rich_terminal_projections() {
        for code in CATALOGUE_CODES {
            let diagnostic = Diagnostic::error(code);
            let plain = render_diagnostic(&diagnostic, RenderStyle::plain());
            let rich = render_diagnostic(&diagnostic, RenderStyle::rich_for_test(false, true, 92));

            for (name, output) in [("plain", &plain), ("rich", &rich)] {
                let normalised_output = normalise_words(output);
                for required in [
                    diagnostic.message.as_str(),
                    diagnostic.reason.as_str(),
                    diagnostic.recommended_next_step.title.as_str(),
                    diagnostic.impact.behavioral.as_str(),
                    diagnostic.impact.public_contract.as_str(),
                    diagnostic.rule_id.as_str(),
                    diagnostic.help_id.as_str(),
                ] {
                    assert!(
                        normalised_output.contains(&normalise_words(required)),
                        "{code}: {name} projection omitted `{required}`\n{output}"
                    );
                }
                for alternative in &diagnostic.alternatives {
                    assert!(
                        normalised_output.contains(&normalise_words(&alternative.title)),
                        "{code}: {name}"
                    );
                    assert!(
                        normalised_output.contains(&normalise_words(&alternative.reason)),
                        "{code}: {name}"
                    );
                    assert!(
                        normalised_output
                            .contains(&normalise_words(&alternative.behavioral_effect)),
                        "{code}: {name}"
                    );
                    assert!(
                        normalised_output
                            .contains(&normalise_words(&alternative.public_contract_effect)),
                        "{code}: {name}"
                    );
                }
                assert!(!output.contains('\u{1b}'), "{code}: {name}");
            }
            assert!(!plain.contains('╭'), "{code}");
            assert!(rich.ends_with("\n\n"), "{code}");
        }
    }

    #[test]
    fn operational_diagnostic_emitter_contracts_are_human_readable() {
        const OPERATIONAL_CODES: &[&str] = &[
            "CLI_CHECK_ARGUMENTS",
            "CLI_DEV_ARGUMENTS",
            "CLI_DEV_BUN_START_FAILED",
            "CLI_DEV_ROLLBACK_CLEANUP_FAILED",
            "CLI_DEV_ROLLBACK_PREPARE_FAILED",
            "CLI_DEV_ROLLBACK_RESTORE_FAILED",
            "CLI_DEV_READINESS_TIMEOUT",
            "CLI_DEV_RUNTIME_EXITED",
            "CLI_DEV_RUNTIME_STATUS_FAILED",
            "CLI_DEV_TARGET_MISSING",
            "CLI_FMT_ARGUMENTS",
            "CLI_INCIDENT_ARGUMENTS",
            "CLI_INCIDENT_INVALID",
            "CLI_INCIDENT_MANIFEST_INVALID",
            "CLI_INCIDENT_MANIFEST_MISSING",
            "CLI_INCIDENT_MANIFEST_STALE",
            "CLI_INCIDENT_OPERATION_UNKNOWN",
            "CLI_INCIDENT_READ_FAILED",
            "CLI_INCIDENT_REVISION_MISMATCH",
            "CLI_LSP_ARGUMENTS",
            "CLI_PRESENTATION_ARGUMENTS",
            "CLI_PROJECT_REQUIRED",
            "CLI_SIGNAL_HANDLER_FAILED",
            "CLI_SCHEMA_COMMAND_REQUIRED",
            "CLI_SCHEMA_DECISION_ARGUMENTS",
            "CLI_SCHEMA_DIFF_ARGUMENTS",
            "CLI_SCHEMA_INDEX_ACCEPT_ARGUMENTS",
            "CLI_SCHEMA_PLAN_ARGUMENTS",
            "CLI_SCHEMA_RENAME_ARGUMENTS",
            "CLI_SCHEMA_SNAPSHOT_ARGUMENTS",
            "CLI_SCHEMA_SQL_ARGUMENTS",
            "CLI_UNKNOWN_COMMAND",
            "CLI_UNKNOWN_SCHEMA_COMMAND",
            "CLI_WATCH_ARGUMENTS",
            "CLI_WATCH_INPUT_READ_FAILED",
            "CLI_WATCH_OUTPUT_FAILED",
            "FMT_CHANGES_REQUIRED",
            "FMT_WRITE_FAILED",
            "INDEX_ACCEPT_CHECK_FAILED",
            "INDEX_ACCEPT_IDENTITY_COUNT",
            "INDEX_ACCEPT_RANGE_INVALID",
            "INDEX_ACCEPT_REGISTRY_MISSING",
            "INDEX_ACCEPT_ROLLBACK_FAILED",
            "INDEX_ACCEPT_SOURCE_READ_FAILED",
            "INDEX_ACCEPT_SOURCE_WRITE_FAILED",
            "INDEX_RECOMMENDATION_AVAILABLE",
            "INDEX_RECOMMENDATION_FIELD_MISSING",
            "JADPO_ARTIFACT_CLEANUP_FAILED",
            "JADPO_ARTIFACT_PROMOTE_FAILED",
            "JADPO_ARTIFACT_STAGE_FAILED",
            "JADPO_NO_SOURCES",
            "JADPO_PROJECT_NOT_FOUND",
            "JADPO_PROJECT_READ_FAILED",
            "JADPO_SCAFFOLD_DESTINATION_EXISTS",
            "JADPO_SCAFFOLD_NAME_INVALID",
            "JADPO_SCAFFOLD_READ_FAILED",
            "JADPO_SCAFFOLD_WRITE_FAILED",
            "JADPO_SOURCE_READ_FAILED",
            "JADPO_TARGET_AUTH_NOT_IMPLEMENTED",
            "LSP_CONTENT_LENGTH_MISSING",
            "LSP_JSON_INVALID",
            "LSP_JSON_WRITE_FAILED",
            "LSP_READ_FAILED",
            "LSP_ROOT_MISSING",
            "LSP_WRITE_FAILED",
            "MIG_DECISION_ARTIFACT_INVALID",
            "MIG_DECISION_ARTIFACT_READ_FAILED",
            "MIG_DECISION_ARTIFACT_WRITE_FAILED",
            "MIG_DECISION_DUPLICATE",
            "MIG_DECISION_EVIDENCE_DUPLICATE",
            "MIG_DECISION_EVIDENCE_EMPTY",
            "MIG_DECISION_EVIDENCE_UNEXPECTED",
            "MIG_DECISION_MISSING",
            "MIG_DECISION_STRATEGY_INVALID",
            "MIG_DECISION_UNEXPECTED",
            "MIG_IDENTITY_DUPLICATE_ID",
            "MIG_IDENTITY_DUPLICATE_PATH",
            "MIG_IDENTITY_PHYSICAL_NAME",
            "MIG_IDENTITY_REGISTRY_INVALID",
            "MIG_IDENTITY_REGISTRY_MISSING",
            "MIG_IDENTITY_REGISTRY_NOT_CANONICAL",
            "MIG_IDENTITY_REGISTRY_OWNER",
            "MIG_IDENTITY_REGISTRY_READ_FAILED",
            "MIG_IDENTITY_REGISTRY_WRITE_FAILED",
            "MIG_IDENTITY_RENAME_KIND",
            "MIG_IDENTITY_RENAME_SOURCE_UNKNOWN",
            "MIG_IDENTITY_RENAME_TARGET_EXISTS",
            "MIG_IDENTITY_RENAME_TARGET_UNKNOWN",
            "MIG_IDENTITY_SNAPSHOT_INVALID",
            "MIG_IDENTITY_SNAPSHOT_MISSING",
            "MIG_IDENTITY_SNAPSHOT_NOT_CANONICAL",
            "MIG_IDENTITY_SNAPSHOT_SHAPE_MISSING",
            "MIG_IDENTITY_SNAPSHOT_WRITE_FAILED",
            "MIG_PLAN_ADAPTER_INVALID",
            "MIG_PLAN_DECISION_REJECTS_CHANGE",
            "MIG_PLAN_EXISTS",
            "MIG_PLAN_WRITE_FAILED",
            "MIG_SQL_CHANGE_UNSUPPORTED",
            "MIG_SQL_EXPRESSION_UNSUPPORTED",
            "MIG_SQL_FIELD_OWNER_MISSING",
            "MIG_SQL_FIELD_SHAPE_INVALID",
            "MIG_SQL_LITERAL_INVALID",
            "MIG_SQL_REVIEW_EXISTS",
            "MIG_SQL_REVIEW_WRITE_FAILED",
            "MIG_SQL_SQLITE_CONSTRAINT_MISSING",
            "MIG_SQL_SQLITE_ENTITY_MISSING",
            "MIG_SQL_SQLITE_FIELD_IDENTITY_MISSING",
            "MIG_SQL_SQLITE_FIELD_MISSING",
            "MIG_SQL_SQLITE_INDEX_MISSING",
            "MIG_SQL_SQLITE_REBUILD_REQUIRED",
            "MIG_SQL_SQLITE_REBUILD_UNSUPPORTED",
            "MIG_SQL_SQLITE_REFERENCE_MISSING",
            "MIG_SQL_SQLITE_RENAMED_TABLE_UNSUPPORTED",
            "MIG_SQL_STRATEGY_UNSUPPORTED",
            "MIG_SQL_TYPE_UNSUPPORTED",
        ];

        for code in OPERATIONAL_CODES {
            let diagnostic = Diagnostic::error(code);
            let rendered = render_diagnostic(&diagnostic, RenderStyle::plain());
            assert!(rendered.contains(&diagnostic.message), "{code}");
            assert!(rendered.contains(&diagnostic.reason), "{code}");
            assert!(
                rendered.contains(&diagnostic.recommended_next_step.title),
                "{code}"
            );
            assert!(
                rendered.contains("https://jadpo.dev/docs/diagnostics/"),
                "{code}"
            );
            assert!(!rendered.contains("compiler-enforced"), "{code}");
            assert!(!rendered.contains(code), "{code}");
        }
    }

    #[test]
    fn invalid_auth_value_terminal_projection_is_a_security_decision_not_a_token_error() {
        let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("CLI crate should be inside the repository");
        let fixture = repository.join("tests/compile/fail/71_invalid_route_auth_value.jadpo");
        let project = analyze_project(&fixture).expect("fixture should analyze");
        let diagnostics = project.syntax.diagnostics().collect::<Vec<_>>();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        let diagnostic = diagnostics[0];

        let rich = render_diagnostic(diagnostic, RenderStyle::rich_for_test(false, true, 100));
        let plain = render_diagnostic(diagnostic, RenderStyle::plain());
        for output in [&rich, &plain] {
            let output = normalise_words(output);
            assert!(output.contains("Route authentication value `nonke` is not valid"));
            assert!(output.contains("Keep authentication required for this route"));
            assert!(output.contains("Make this route explicitly unauthenticated"));
            assert!(output.contains("Target generation may still be blocked until authentication runtime support is configured"));
            assert!(output.contains("callable without authentication"));
            assert!(output.contains("human"));
            assert!(!output.contains("Unexpected token"));
            assert!(!output.contains("SYN_UNEXPECTED_TOKEN"));
        }
        assert!(rich.contains("Why"));
        assert!(rich.contains("Impact"));
        assert!(plain.contains("behavioral impact:"));
        assert!(!plain.contains('\u{1b}'));
    }

    fn normalise_words(value: &str) -> String {
        value.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}
