//! Seeded random edits of Jadpo source, shared by the parser's fuzz test
//! and the formatter's: the same seed gives the same inputs on every
//! platform, so a failure repeats.

#![allow(dead_code)]

/// xorshift64*: small, fast and the same on every platform.
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

/// Pieces of Jadpo and of what models get wrong, to insert at random.
pub const PIECES: &[&str] = &[
    "{",
    "}",
    "(",
    ")",
    "[",
    "]",
    "<",
    ">",
    "\"",
    ":",
    ",",
    ".",
    "...",
    "->",
    "=>",
    "=",
    "==",
    "?",
    "/",
    "//",
    "///",
    "\n",
    " ",
    "$",
    ";",
    "&&",
    "!",
    "é",
    "_",
    "{{",
    "}}",
    "\\",
    "15m",
    "1.5",
    "type ",
    "function ",
    "action ",
    "query ",
    "entity ",
    "routes ",
    "public ",
    "if ",
    "else ",
    "for ",
    "in ",
    "match ",
    "return ",
    "var ",
    "var mut ",
    "attempt ",
    "reject ",
    "fails ",
    "GET /",
    "/{id: Item.id}",
    "prefix: /",
    "test \"t\" ",
    "seed ",
    "config ",
    "x: ",
    "Name ",
];

fn char_boundary(text: &str, mut at: usize) -> usize {
    at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

pub fn mutate(text: &str, rng: &mut Rng) -> String {
    let mut out = text.to_string();
    for _ in 0..=rng.below(3) {
        let len = out.len();
        let at = char_boundary(&out, rng.below(len + 1));
        match rng.below(5) {
            0 => {
                let end = char_boundary(&out, at + 1 + rng.below(12));
                out.replace_range(at..end, "");
            }
            1 | 2 => {
                let piece = PIECES[rng.below(PIECES.len())];
                out.insert_str(at, piece);
            }
            3 => {
                let end = char_boundary(&out, at + rng.below(40));
                let copy = out[at..end].to_string();
                let to = char_boundary(&out, rng.below(len + 1));
                out.insert_str(to, &copy);
            }
            _ => {
                let mut lines: Vec<&str> = out.split('\n').collect();
                if lines.len() > 1 {
                    let a = rng.below(lines.len());
                    let b = rng.below(lines.len());
                    lines.swap(a, b);
                }
                out = lines.join("\n");
            }
        }
    }
    out
}
