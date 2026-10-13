//! Prints the tree and errors for a file: `cargo run -p jadpo-syntax --example dump -- <file> [snippet]`.
fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("a file");
    let text = std::fs::read_to_string(&path).expect("readable");
    let parse = if args.next().as_deref() == Some("snippet") {
        jadpo_syntax::parse_snippet(&text)
    } else {
        jadpo_syntax::parse_file(&text)
    };
    print!("{}", jadpo_syntax::debug_tree(&parse.syntax()));
    for error in parse.errors() {
        println!(
            "{} {}..{} {} | {} | {} | {}",
            error.rule.id,
            error.start,
            error.end,
            error.message,
            error.label,
            error.reason,
            error.next
        );
    }
}
