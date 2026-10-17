//! Colour tokens and symbols (CLI design language, "Colour tokens" and
//! "Symbols"). Each token means one thing; every coloured thing also has a
//! symbol or word, so output reads the same without colour.

use crate::detect::{Depth, Symbols, Theme};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Token {
    /// Wrong, and it stops you: errors, failed tests and runs, faults.
    Failure,
    /// Works, but should change: alerts, retries.
    Warning,
    /// Done and correct: passed, succeeded, wrote, added.
    Success,
    /// Now and next: what is running, the selection, the prompt, the command to run next.
    Accent,
    /// Needs a person: `(human)` owners, outcome unknown, a choice to write down.
    Decision,
    /// Secondary: labels, gutters, times, counts, paths in lists.
    Muted,
}

struct Values {
    ansi16: &'static str,
    dark: (u8, u8, u8),
    dark256: u8,
    light: (u8, u8, u8),
    light256: u8,
}

impl Token {
    fn values(self) -> Values {
        let (ansi16, dark, dark256, light, light256) = match self {
            Token::Failure => ("31", (0xff, 0x7b, 0x70), 209, (0xc3, 0x26, 0x2f), 124),
            Token::Warning => ("33", (0xeb, 0xb5, 0x4a), 179, (0x8b, 0x54, 0x00), 94),
            Token::Success => ("32", (0x4f, 0xd6, 0x9c), 79, (0x0f, 0x7a, 0x55), 23),
            Token::Accent => ("36", (0x5c, 0xc3, 0xf0), 74, (0x0b, 0x64, 0x9f), 24),
            Token::Decision => ("35", (0xd9, 0xa1, 0xf7), 183, (0x8b, 0x3d, 0xb8), 91),
            Token::Muted => ("2", (0x96, 0x9d, 0xa8), 247, (0x5d, 0x64, 0x70), 241),
        };
        Values {
            ansi16,
            dark,
            dark256,
            light,
            light256,
        }
    }

    /// The SGR parameters for this token. The exact values are used only
    /// when the background is known; otherwise the terminal's own 16
    /// colours, with muted as dim.
    pub fn sgr(self, depth: Depth, theme: Option<Theme>) -> String {
        let values = self.values();
        match (theme, depth) {
            (None, _) | (_, Depth::Ansi16) => values.ansi16.to_string(),
            (Some(Theme::Dark), Depth::Ansi256) => format!("38;5;{}", values.dark256),
            (Some(Theme::Light), Depth::Ansi256) => format!("38;5;{}", values.light256),
            (Some(theme), Depth::TrueColor) => {
                let (r, g, b) = if theme == Theme::Dark {
                    values.dark
                } else {
                    values.light
                };
                format!("38;2;{r};{g};{b}")
            }
        }
    }
}

/// The closed symbol set. Single-width; no emoji or Nerd Font glyphs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Symbol {
    Success,
    Failure,
    Warning,
    Decision,
    Running,
    Pending,
    Paused,
    Skipped,
    Prompt,
    Next,
    Selected,
    Separator,
    Gutter,
    Cut,
    /// The ends of a watch rule.
    Rule,
}

impl Symbol {
    pub fn text(self, symbols: Symbols) -> &'static str {
        let (unicode, ascii) = match self {
            Symbol::Success => ("✓", "+"),
            Symbol::Failure => ("✗", "x"),
            Symbol::Warning => ("▲", "!"),
            Symbol::Decision => ("◆", "@"),
            Symbol::Running => ("●", "*"),
            Symbol::Pending => ("○", "o"),
            Symbol::Paused => ("‖", "="),
            Symbol::Skipped => ("–", "-"),
            Symbol::Prompt => ("?", "?"),
            Symbol::Next => ("→", "->"),
            Symbol::Selected => ("›", ">"),
            Symbol::Separator => ("·", ","),
            Symbol::Gutter => ("│", "|"),
            Symbol::Cut => ("…", "..."),
            Symbol::Rule => ("──", "--"),
        };
        match symbols {
            Symbols::Unicode => unicode,
            Symbols::Ascii => ascii,
        }
    }

    pub fn token(self) -> Option<Token> {
        match self {
            Symbol::Success => Some(Token::Success),
            Symbol::Failure => Some(Token::Failure),
            Symbol::Warning => Some(Token::Warning),
            Symbol::Decision => Some(Token::Decision),
            Symbol::Running | Symbol::Prompt | Symbol::Next | Symbol::Selected => {
                Some(Token::Accent)
            }
            Symbol::Pending
            | Symbol::Paused
            | Symbol::Skipped
            | Symbol::Separator
            | Symbol::Gutter
            | Symbol::Cut
            | Symbol::Rule => Some(Token::Muted),
        }
    }
}
