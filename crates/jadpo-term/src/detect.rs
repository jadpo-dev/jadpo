//! Which output mode, colour depth, palette and symbol set a stream gets
//! (CLI design language, "Output modes").

use std::collections::BTreeMap;
use std::io::IsTerminal;

/// The lines every rich screen fits.
pub const MAX_WIDTH: usize = 80;

/// What Jadpo knows about where it is running. Built from the process in
/// `main`, and by hand in tests, so detection is a pure function.
#[derive(Clone, Debug, Default)]
pub struct Environment {
    vars: BTreeMap<String, String>,
    pub stdin_terminal: bool,
    pub stdout_terminal: bool,
    pub stderr_terminal: bool,
    pub windows: bool,
}

impl Environment {
    /// Nothing set and no terminals: what a pipe in a bare environment sees.
    pub fn empty() -> Environment {
        Environment::default()
    }

    pub fn from_process() -> Environment {
        Environment {
            vars: std::env::vars_os()
                .filter_map(|(key, value)| {
                    Some((key.into_string().ok()?, value.into_string().ok()?))
                })
                .collect(),
            stdin_terminal: std::io::stdin().is_terminal(),
            stdout_terminal: std::io::stdout().is_terminal(),
            stderr_terminal: std::io::stderr().is_terminal(),
            windows: cfg!(windows),
        }
    }

    pub fn with_var(mut self, key: &str, value: &str) -> Environment {
        self.vars.insert(key.to_string(), value.to_string());
        self
    }

    pub fn with_terminals(mut self, stdin: bool, stdout: bool, stderr: bool) -> Environment {
        self.stdin_terminal = stdin;
        self.stdout_terminal = stdout;
        self.stderr_terminal = stderr;
        self
    }

    pub fn on_windows(mut self) -> Environment {
        self.windows = true;
        self
    }

    pub fn var(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(String::as_str)
    }

    /// Set to something other than empty or `0`.
    fn flag(&self, key: &str) -> bool {
        matches!(self.var(key), Some(value) if !value.is_empty() && value != "0")
    }
}

/// `--color=auto|always|never`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    pub fn parse(value: &str) -> Option<ColorChoice> {
        match value {
            "auto" => Some(ColorChoice::Auto),
            "always" => Some(ColorChoice::Always),
            "never" => Some(ColorChoice::Never),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Colour, symbols, alignment, live lines.
    Rich,
    /// Area 14's `error:` / `-->` form: no colour, symbols or cursor movement.
    Plain,
    /// One JSON document on stdout, nothing else.
    Json,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    Ansi16,
    Ansi256,
    TrueColor,
}

/// The terminal's background, when Jadpo knows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Symbols {
    Unicode,
    Ascii,
}

/// Everything rendering needs to know about one output stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    pub mode: Mode,
    /// `None` when colour is off.
    pub color: Option<Depth>,
    /// Bold survives `NO_COLOR`; plain output has none.
    pub bold: bool,
    /// `None` uses the terminal's own 16 colours, so its theme decides.
    pub theme: Option<Theme>,
    pub symbols: Symbols,
    /// Spinners and redraws; append-only lines otherwise.
    pub animate: bool,
    pub width: usize,
}

impl Style {
    /// A style for `mode` with no colour, Unicode symbols and 80 columns.
    pub fn new(mode: Mode) -> Style {
        Style {
            mode,
            color: None,
            bold: mode == Mode::Rich,
            theme: None,
            symbols: Symbols::Unicode,
            animate: false,
            width: MAX_WIDTH,
        }
    }

    /// Decides the style for one stream. First match wins at each step, in
    /// the order the design language gives.
    pub fn detect(env: &Environment, json: bool, choice: ColorChoice, stream: Stream) -> Style {
        if json {
            return Style::new(Mode::Json);
        }
        let terminal = match stream {
            Stream::Stdout => env.stdout_terminal,
            Stream::Stderr => env.stderr_terminal,
        };
        let forced_depth = match env.var("FORCE_COLOR") {
            Some("2") => Some(Depth::Ansi256),
            Some("3") => Some(Depth::TrueColor),
            _ => None,
        };
        let colour = match choice {
            ColorChoice::Always => Some(true),
            ColorChoice::Never => Some(false),
            ColorChoice::Auto => {
                if env.flag("NO_COLOR") {
                    Some(false)
                } else if let Some(value) = env.var("FORCE_COLOR") {
                    Some(!matches!(value, "0" | "false"))
                } else if env.flag("CLICOLOR_FORCE") {
                    Some(true)
                } else {
                    None
                }
            }
        };
        let term = env.var("TERM").unwrap_or("");
        let mode = if env.flag("JADPO_PLAIN") {
            Mode::Plain
        } else if (terminal && term != "dumb") || colour == Some(true) {
            Mode::Rich
        } else {
            Mode::Plain
        };
        let rich = mode == Mode::Rich;
        let depth = forced_depth.unwrap_or_else(|| {
            let colorterm = env.var("COLORTERM").unwrap_or("");
            if colorterm == "truecolor" || colorterm == "24bit" || env.var("WT_SESSION").is_some() {
                Depth::TrueColor
            } else if term.ends_with("-256color") {
                Depth::Ansi256
            } else {
                Depth::Ansi16
            }
        });
        let unicode_console = !env.windows
            || env.var("WT_SESSION").is_some()
            || env.var("TERM_PROGRAM") == Some("vscode");
        let symbols = if env.flag("JADPO_ASCII") || term == "linux" || !unicode_console {
            Symbols::Ascii
        } else {
            Symbols::Unicode
        };
        let width = match env
            .var("COLUMNS")
            .and_then(|value| value.parse::<usize>().ok())
        {
            Some(columns) if rich && terminal && columns > 0 => columns.min(MAX_WIDTH),
            _ => MAX_WIDTH,
        };
        Style {
            mode,
            color: if rich && colour != Some(false) {
                Some(depth)
            } else {
                None
            },
            bold: rich,
            theme: theme(env),
            symbols,
            animate: rich && env.stderr_terminal && env.var("CI").is_none(),
            width,
        }
    }
}

/// `JADPO_THEME`, else `COLORFGBG`. The OSC 11 query the design language
/// places between them needs a terminal round trip and comes with the first
/// interactive screen.
fn theme(env: &Environment) -> Option<Theme> {
    match env.var("JADPO_THEME") {
        Some("dark") => return Some(Theme::Dark),
        Some("light") => return Some(Theme::Light),
        _ => {}
    }
    let background = env
        .var("COLORFGBG")?
        .rsplit(';')
        .next()?
        .parse::<u8>()
        .ok()?;
    match background {
        0..=6 | 8 => Some(Theme::Dark),
        7 | 9..=15 => Some(Theme::Light),
        _ => None,
    }
}
