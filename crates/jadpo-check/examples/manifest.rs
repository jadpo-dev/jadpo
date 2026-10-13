//! Prints the manifest of the project at the path given, for looking at
//! what the graph holds: `cargo run -p jadpo-check --example manifest -- <path>`.

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let loaded = jadpo_db::load(std::path::Path::new(&path));
    let errors = jadpo_check::check(&loaded.db, loaded.project).len();
    let graph = jadpo_check::graph::graph(&loaded.db, loaded.project);
    let stamp = jadpo_graph::manifest::Stamp {
        compiler: "dev".into(),
        errors,
    };
    print!(
        "{}",
        jadpo_graph::manifest::manifest(graph, &stamp).to_document()
    );
}
