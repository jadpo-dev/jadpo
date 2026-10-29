fn main() {
    let args: Vec<String> = std::env::args().collect();
    let bound = jadpo_codegen::workers::bind(
        std::path::Path::new(&args[1]),
        "golden",
        jadpo_codegen::workers::Names::Kept,
    )
    .expect("bound Wasm");
    std::fs::create_dir_all(&args[2]).expect("folder");
    std::fs::write(format!("{}/golden.js", args[2]), bound.js).expect("JS");
    std::fs::write(format!("{}/golden.wasm", args[2]), bound.wasm).expect("Wasm");
}
