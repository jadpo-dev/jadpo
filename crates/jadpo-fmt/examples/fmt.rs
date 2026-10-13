//! Formats a snippet file to stdout: `cargo run -p jadpo-fmt --example fmt -- <file>`.
fn main() {
    let path = std::env::args().nth(1).expect("a file");
    let text = std::fs::read_to_string(path).expect("readable");
    match jadpo_fmt::format_snippet(&text) {
        Ok(out) => print!("{out}"),
        Err(errors) => println!("{errors:#?}"),
    }
}
