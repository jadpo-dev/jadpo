//! Output-mode, colour, symbol and theme detection, as the CLI design language
//! decides it (docs/cli-design-language.md, "Output modes").

use jadpo_term::{ColorChoice, Depth, Environment, Mode, Stream, Style, Symbols, Theme};

fn terminal() -> Environment {
    Environment::empty()
        .with_terminals(true, true, true)
        .with_var("TERM", "xterm")
}

fn stdout(env: &Environment) -> Style {
    Style::detect(env, false, ColorChoice::Auto, Stream::Stdout)
}

#[test]
fn a_terminal_gets_rich_output_with_sixteen_colours() {
    let style = stdout(&terminal());
    assert_eq!(style.mode, Mode::Rich);
    assert_eq!(style.color, Some(Depth::Ansi16));
    assert_eq!(style.symbols, Symbols::Unicode);
    assert_eq!(style.theme, None);
    assert!(style.animate);
    assert_eq!(style.width, 80);
}

#[test]
fn piped_dumb_and_jadpo_plain_get_plain_output() {
    let piped = terminal().with_terminals(true, false, true);
    assert_eq!(stdout(&piped).mode, Mode::Plain);
    assert_eq!(stdout(&piped).color, None);
    assert!(!stdout(&piped).bold);
    assert_eq!(
        stdout(&terminal().with_var("TERM", "dumb")).mode,
        Mode::Plain
    );
    assert_eq!(
        stdout(&terminal().with_var("JADPO_PLAIN", "1")).mode,
        Mode::Plain
    );
    assert_eq!(
        stdout(&terminal().with_var("JADPO_PLAIN", "0")).mode,
        Mode::Rich
    );
}

#[test]
fn json_wins_over_everything() {
    let style = Style::detect(&terminal(), true, ColorChoice::Always, Stream::Stdout);
    assert_eq!(style.mode, Mode::Json);
    assert_eq!(style.color, None);
}

#[test]
fn each_stream_is_checked_separately() {
    let env = terminal().with_terminals(true, false, true);
    assert_eq!(
        Style::detect(&env, false, ColorChoice::Auto, Stream::Stdout).mode,
        Mode::Plain
    );
    assert_eq!(
        Style::detect(&env, false, ColorChoice::Auto, Stream::Stderr).mode,
        Mode::Rich
    );
}

#[test]
fn no_color_turns_colour_off_but_keeps_symbols() {
    let style = stdout(&terminal().with_var("NO_COLOR", "1"));
    assert_eq!(style.mode, Mode::Rich);
    assert_eq!(style.color, None);
    assert_eq!(style.symbols, Symbols::Unicode);
    assert!(style.bold);
    assert_eq!(
        stdout(&terminal().with_var("NO_COLOR", "")).color,
        Some(Depth::Ansi16)
    );
}

#[test]
fn the_color_flag_comes_first() {
    let env = terminal().with_var("NO_COLOR", "1");
    assert_eq!(
        Style::detect(&env, false, ColorChoice::Always, Stream::Stdout).color,
        Some(Depth::Ansi16)
    );
    let env = terminal().with_var("FORCE_COLOR", "3");
    assert_eq!(
        Style::detect(&env, false, ColorChoice::Never, Stream::Stdout).color,
        None
    );
}

#[test]
fn force_color_sets_on_off_and_depth() {
    assert_eq!(stdout(&terminal().with_var("FORCE_COLOR", "0")).color, None);
    assert_eq!(
        stdout(&terminal().with_var("FORCE_COLOR", "1")).color,
        Some(Depth::Ansi16)
    );
    assert_eq!(
        stdout(&terminal().with_var("FORCE_COLOR", "2")).color,
        Some(Depth::Ansi256)
    );
    assert_eq!(
        stdout(&terminal().with_var("FORCE_COLOR", "3")).color,
        Some(Depth::TrueColor)
    );
    assert_eq!(
        stdout(
            &terminal()
                .with_var("NO_COLOR", "1")
                .with_var("FORCE_COLOR", "1")
        )
        .color,
        None
    );
}

#[test]
fn forced_colour_on_a_pipe_gives_rich_output_without_animation() {
    let env = Environment::empty().with_var("FORCE_COLOR", "1");
    let style = stdout(&env);
    assert_eq!(style.mode, Mode::Rich);
    assert_eq!(style.color, Some(Depth::Ansi16));
    assert!(!style.animate);
    let env = Environment::empty().with_var("CLICOLOR_FORCE", "1");
    assert_eq!(stdout(&env).color, Some(Depth::Ansi16));
    let env = Environment::empty()
        .with_var("FORCE_COLOR", "1")
        .with_var("JADPO_PLAIN", "1");
    assert_eq!(stdout(&env).mode, Mode::Plain);
}

#[test]
fn depth_comes_from_colorterm_wt_session_and_term() {
    assert_eq!(
        stdout(&terminal().with_var("COLORTERM", "truecolor")).color,
        Some(Depth::TrueColor)
    );
    assert_eq!(
        stdout(&terminal().with_var("COLORTERM", "24bit")).color,
        Some(Depth::TrueColor)
    );
    assert_eq!(
        stdout(&terminal().with_var("WT_SESSION", "x")).color,
        Some(Depth::TrueColor)
    );
    assert_eq!(
        stdout(&terminal().with_var("TERM", "xterm-256color")).color,
        Some(Depth::Ansi256)
    );
}

#[test]
fn ascii_symbols_when_asked_or_when_the_console_cannot_show_unicode() {
    assert_eq!(
        stdout(&terminal().with_var("JADPO_ASCII", "1")).symbols,
        Symbols::Ascii
    );
    assert_eq!(
        stdout(&terminal().with_var("TERM", "linux")).symbols,
        Symbols::Ascii
    );
    let windows = terminal().on_windows();
    assert_eq!(stdout(&windows).symbols, Symbols::Ascii);
    assert_eq!(
        stdout(&windows.clone().with_var("WT_SESSION", "1")).symbols,
        Symbols::Unicode
    );
    assert_eq!(
        stdout(&windows.with_var("TERM_PROGRAM", "vscode")).symbols,
        Symbols::Unicode
    );
}

#[test]
fn animation_needs_a_terminal_on_stderr_and_no_ci() {
    assert!(!stdout(&terminal().with_var("CI", "true")).animate);
    assert!(!stdout(&terminal().with_terminals(true, true, false)).animate);
}

#[test]
fn theme_comes_from_jadpo_theme_then_colorfgbg() {
    assert_eq!(
        stdout(&terminal().with_var("JADPO_THEME", "dark")).theme,
        Some(Theme::Dark)
    );
    assert_eq!(
        stdout(&terminal().with_var("JADPO_THEME", "light")).theme,
        Some(Theme::Light)
    );
    assert_eq!(
        stdout(&terminal().with_var("COLORFGBG", "15;0")).theme,
        Some(Theme::Dark)
    );
    assert_eq!(
        stdout(&terminal().with_var("COLORFGBG", "0;default;15")).theme,
        Some(Theme::Light)
    );
    assert_eq!(
        stdout(&terminal().with_var("COLORFGBG", "default;default")).theme,
        None
    );
    let both = terminal()
        .with_var("JADPO_THEME", "light")
        .with_var("COLORFGBG", "15;0");
    assert_eq!(stdout(&both).theme, Some(Theme::Light));
}

#[test]
fn width_is_eighty_unless_the_terminal_is_narrower() {
    assert_eq!(stdout(&terminal().with_var("COLUMNS", "60")).width, 60);
    assert_eq!(stdout(&terminal().with_var("COLUMNS", "200")).width, 80);
    assert_eq!(stdout(&terminal().with_var("COLUMNS", "junk")).width, 80);
    let piped = terminal()
        .with_terminals(true, false, true)
        .with_var("COLUMNS", "60");
    assert_eq!(stdout(&piped).width, 80);
}
