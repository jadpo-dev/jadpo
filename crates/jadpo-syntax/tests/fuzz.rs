//! The parser never panics and never loses text (docs/compiler-runtime.md
//! §2.2): every example, mutated at random many times, still parses into a
//! tree that holds its text exactly. The mutations are seeded, so a failure
//! repeats; the failing input is printed.

#[path = "support/examples.rs"]
mod examples;

use std::panic::{AssertUnwindSafe, catch_unwind};

use jadpo_syntax::{parse_file, parse_snippet, parse_template};

/// xorshift64*: small, fast and the same on every platform.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

/// Pieces of Jadpo and of what models get wrong, to insert at random.
const PIECES: &[&str] = &[
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

fn mutate(text: &str, rng: &mut Rng) -> String {
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

fn check(text: &str) {
    let result = catch_unwind(AssertUnwindSafe(|| {
        for parse in [parse_file(text), parse_snippet(text), parse_template(text)] {
            assert_eq!(parse.syntax().to_string(), text, "the tree lost text");
            for error in parse.errors() {
                assert!(
                    error.start <= error.end && error.end as usize <= text.len(),
                    "an error points outside the text: {error:?}"
                );
            }
        }
    }));
    if result.is_err() {
        panic!("the parser failed on this input:\n{text:?}");
    }
}

#[test]
fn mutated_examples_never_break_the_parser() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let examples = examples::all();
    assert!(examples.len() > 500);
    for example in &examples {
        check(&example.source);
        for _ in 0..6 {
            check(&mutate(&example.source, &mut rng));
        }
    }
}

#[test]
fn hostile_input_is_survived() {
    let mut inputs = vec![
        "(".repeat(20_000),
        "{".repeat(20_000),
        "[".repeat(20_000),
        "<".repeat(5_000),
        format!(
            "function f() -> T {{\n    return {}\n}}\n",
            "(".repeat(5_000)
        ),
        format!("type A = Object {{\n{}\n}}\n", "    x: List<".repeat(2_000)),
        "\"".repeat(1_000),
        "}}}}{{{{".repeat(1_000),
        "/{a}/{b}".repeat(500),
        String::from_utf8_lossy(&(0u8..=255).collect::<Vec<u8>>()).into_owned(),
        "\u{feff}type A = Int {}\r\n".to_string(),
        String::new(),
    ];
    inputs.push("not not not not ".repeat(5_000));
    inputs.push("- ".repeat(20_000));
    for input in inputs {
        check(&input);
    }
}
