//! Fuzzed function bodies (roadmap chunk 9.3; docs/compiler-runtime.md
//! §2.5, "Differential test"): what the generator makes checks without a
//! diagnostic, from every seed, and a seed makes the same project each
//! time. The differential run (`jadpo-cli`'s `compiled::`) runs them both
//! ways.

use std::path::{Path, PathBuf};

/// The project `files` written into a folder of its own under the build's
/// temporary folder.
fn written(name: &str, files: &[(String, String)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("jadpo-fuzz-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("folders");
        std::fs::write(path, text).expect("write");
    }
    root
}

/// Every diagnostic the project at `root` gives, each with the code
/// around it.
fn diagnostics(root: &Path) -> Vec<String> {
    let loaded = jadpo_db::load(root);
    jadpo_check::check(&loaded.db, loaded.project)
        .iter()
        .map(|diagnostic| {
            let around = diagnostic
                .primary
                .as_ref()
                .map_or_else(String::new, |label| {
                    let text =
                        std::fs::read_to_string(root.join(&label.span.path)).unwrap_or_default();
                    let start = (label.span.start as usize).saturating_sub(120);
                    let end = (label.span.end as usize + 40).min(text.len());
                    text.get(start..end).unwrap_or_default().to_string()
                });
            format!(
                "{}: {}
{around}
",
                diagnostic.rule.id, diagnostic.message
            )
        })
        .collect()
}

/// How many seeds the check below tries: more with `JADPO_FUZZ_BODIES`.
fn seeds() -> u64 {
    let (_, bodies) = jadpo_test::fuzz::asked();
    (bodies as u64 / 2).max(20)
}

#[test]
fn every_fuzzed_body_checks() {
    let mut problems = Vec::new();
    for seed in 0..seeds() {
        let files = jadpo_test::fuzz::project(seed, 8);
        let root = written(&format!("seed-{seed}"), &files);
        let found = diagnostics(&root);
        if !found.is_empty() {
            problems.push(format!("seed {seed}:\n{}", found.join("\n")));
        }
        let _ = std::fs::remove_dir_all(&root);
        if problems.len() >= 3 {
            break;
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n\n"));
}

#[test]
fn a_seed_makes_the_same_bodies_each_time() {
    assert_eq!(
        jadpo_test::fuzz::project(7, 5),
        jadpo_test::fuzz::project(7, 5)
    );
    assert_ne!(
        jadpo_test::fuzz::project(7, 5),
        jadpo_test::fuzz::project(8, 5)
    );
    let projects = jadpo_test::fuzz::made(3, 90);
    let names: Vec<&str> = projects.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "fuzzed bodies, seed 3, 40 bodies",
            "fuzzed bodies, seed 4, 40 bodies",
            "fuzzed bodies, seed 5, 10 bodies"
        ]
    );
}
