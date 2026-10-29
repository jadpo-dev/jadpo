use std::{env, fs, path::PathBuf};
fn main() {
    let project = PathBuf::from(env::var("GOLDEN_PROJECT").expect("GOLDEN_PROJECT"));
    let loaded = jadpo_db::load(&project);
    let prepared =
        jadpo_interp::Prepared::load(&loaded.db, loaded.project).expect("application checks");
    let generated = jadpo_codegen::module(prepared.plan(), 0).expect("compiled application");
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    generated.write(&out).expect("generated source");
    fs::write(
        out.join("module.rs"),
        format!("#[path = {:?}] mod golden;", out.join("src/p0/mod.rs")),
    )
    .expect("module");
    println!("cargo:rerun-if-env-changed=GOLDEN_PROJECT");
    println!("cargo:rerun-if-changed={}", project.display());
}
