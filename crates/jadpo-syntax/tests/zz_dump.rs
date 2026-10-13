#[test]
fn dump() {
    let text = std::fs::read_to_string(std::env::var("DUMP").unwrap()).unwrap();
    let parse = jadpo_syntax::parse_file(&text);
    println!("{}", jadpo_syntax::debug_tree(&parse.syntax()));
    println!("{:#?}", parse.errors());
}
