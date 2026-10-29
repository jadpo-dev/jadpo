//! What each fuzz target holds the compiler and the runtime to, as plain
//! functions: a target in `fuzz_targets/` hands its input to one, and a
//! crash it finds is reproduced by running that target on the crash file
//! (docs/security.md, "Fuzzing"). Every panic is a bug; so is a broken
//! guarantee, which these assert.

pub mod check;
pub mod parse;
pub mod request;

/// The repository's root, where the teaching examples are.
pub fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the fuzz crate is in the repository")
        .to_path_buf()
}
