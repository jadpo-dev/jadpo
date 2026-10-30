//! What `jadpo build` carries so it builds outside this checkout (roadmap
//! chunk 9.2c; docs/compiler-runtime.md §2.8, "Toolchain pinning"): the
//! source of the crates a deployable compiles against, the shared and
//! runtime layers of the root `Cargo.toml`, packed into the binary
//! (`runtime.deflate`, read by `src/runtime_source.rs`) with a workspace
//! manifest of their own, and the target this `jadpo` runs on, whose
//! toolchain it fetches (`JADPO_HOST`).
//!
//! Each crate is its `Cargo.toml` and its `src/`: a deployable builds a
//! crate's library and nothing else, so its manifest is carried without
//! its dev-dependencies, tests, examples and benches. The workspace
//! manifest keeps the root's package fields, dependencies and lints, the
//! crates' paths beside it, and none of the other crates.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("cargo's"));
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .expect("the workspace")
        .to_path_buf();
    let target = std::env::var("TARGET").expect("cargo's target");
    println!("cargo::rustc-env=JADPO_HOST={target}");
    for file in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"] {
        println!("cargo::rerun-if-changed={}", root.join(file).display());
    }
    let workspace = std::fs::read_to_string(root.join("Cargo.toml")).expect("the root manifest");
    let crates: Vec<String> = ["shared", "runtime"]
        .iter()
        .flat_map(|layer| layer_members(&workspace, layer))
        .filter(|name| root.join("crates").join(name).join("Cargo.toml").exists())
        .collect();
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    files.insert(
        "Cargo.toml".to_string(),
        workspace_manifest(&workspace, &crates).into_bytes(),
    );
    for name in &crates {
        let folder = root.join("crates").join(name);
        println!(
            "cargo::rerun-if-changed={}",
            folder.join("Cargo.toml").display()
        );
        println!("cargo::rerun-if-changed={}", folder.join("src").display());
        let text = std::fs::read_to_string(folder.join("Cargo.toml")).expect("a crate's manifest");
        files.insert(
            format!("{name}/Cargo.toml"),
            crate_manifest(&text).into_bytes(),
        );
        let mut sources = Vec::new();
        walk(&folder.join("src"), &mut sources);
        for path in sources {
            let inside = path
                .strip_prefix(&folder)
                .expect("inside the crate")
                .to_string_lossy()
                .replace('\\', "/");
            files.insert(
                format!("{name}/{inside}"),
                std::fs::read(&path).expect("a source file"),
            );
        }
    }
    let mut packed = Vec::new();
    for (path, bytes) in &files {
        packed.extend_from_slice(&(path.len() as u32).to_le_bytes());
        packed.extend_from_slice(path.as_bytes());
        packed.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        packed.extend_from_slice(bytes);
    }
    use sha2::Digest;
    let digest: String = sha2::Sha256::digest(&packed)
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    println!("cargo::rustc-env=JADPO_RUNTIME_DIGEST={digest}");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo's"));
    std::fs::write(
        out.join("runtime.deflate"),
        miniz_oxide::deflate::compress_to_vec(&packed, 6),
    )
    .expect("the packed runtime");
    scaffold(&manifest.join("scaffold"), &out);
    error_pages(&root.join("docs").join("errors"), &out);
}

/// Every rule's page, `docs/errors/<rule>.md`, for `jadpo help <rule>`
/// offline (`src/help.rs`): each rule's id and page, deflated as one
/// `errors.deflate`, the id and the page each a length then its bytes.
fn error_pages(folder: &Path, out: &Path) {
    println!("cargo::rerun-if-changed={}", folder.display());
    let mut pages = Vec::new();
    walk(folder, &mut pages);
    pages.sort();
    let mut packed = Vec::new();
    for page in pages {
        let Some(id) = page
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(".md"))
        else {
            continue;
        };
        println!("cargo::rerun-if-changed={}", page.display());
        let text = std::fs::read(&page).expect("an error page");
        packed.extend_from_slice(&(id.len() as u32).to_le_bytes());
        packed.extend_from_slice(id.as_bytes());
        packed.extend_from_slice(&(text.len() as u32).to_le_bytes());
        packed.extend_from_slice(&text);
    }
    std::fs::write(
        out.join("errors.deflate"),
        miniz_oxide::deflate::compress_to_vec(&packed, 9),
    )
    .expect("the packed error pages");
}

/// What `jadpo new` writes (`src/new.rs`): each file of `scaffold/` as the
/// project path it is written to and its text, in `scaffold.rs`. A
/// template is named after its path with `.tmpl` added, so no tool reads
/// one as a project's source; `gitignore` and `gitattributes` at the top
/// are the project's dot files, which inside this repository would apply
/// to it.
fn scaffold(folder: &Path, out: &Path) {
    println!("cargo::rerun-if-changed={}", folder.display());
    let mut templates = Vec::new();
    walk(folder, &mut templates);
    templates.sort();
    let mut code = String::from("pub const SCAFFOLD: &[(&str, &str)] = &[\n");
    for template in templates {
        println!("cargo::rerun-if-changed={}", template.display());
        let inside = template
            .strip_prefix(folder)
            .expect("inside the scaffold")
            .to_string_lossy()
            .replace('\\', "/");
        let Some(path) = inside.strip_suffix(".tmpl") else {
            panic!("scaffold/{inside} is a template, so its name ends in `.tmpl`");
        };
        let path = match path {
            "gitignore" | "gitattributes" => format!(".{path}"),
            other => other.to_string(),
        };
        code.push_str(&format!(
            "    ({path:?}, include_str!({:?})),\n",
            template.display().to_string()
        ));
    }
    code.push_str("];\n");
    std::fs::write(out.join("scaffold.rs"), code).expect("the scaffold's list");
}

/// Every file under `folder`, in no particular order.
fn walk(folder: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(folder)
        .expect("a source folder")
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// The crates `[workspace.metadata.layers]` lists in `layer`.
fn layer_members(workspace: &str, layer: &str) -> Vec<String> {
    let section = sections(workspace)
        .into_iter()
        .find(|(header, _)| header == "[workspace.metadata.layers]")
        .map(|(_, body)| body)
        .unwrap_or_default();
    let Some(start) = section.find(&format!("\n{layer} = [")) else {
        return Vec::new();
    };
    let rest = &section[start..];
    let list = &rest[rest.find('[').expect("a list") + 1..rest.find(']').expect("a list")];
    list.split(',')
        .map(|name| name.trim().trim_matches('"').to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// A manifest's sections in order: each header line, and the lines up to
/// the next header. What comes before the first header has an empty one.
fn sections(text: &str) -> Vec<(String, String)> {
    let mut out = vec![(String::new(), String::from("\n"))];
    for line in text.lines() {
        if line.starts_with('[') {
            out.push((line.trim().to_string(), String::from("\n")));
        } else if let Some((_, body)) = out.last_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    out
}

/// A crate's manifest as a deployable reads it: without the sections only
/// its own tests, examples and benches read.
fn crate_manifest(text: &str) -> String {
    let mut out = String::new();
    for (header, body) in sections(text) {
        let only_its_own = header.ends_with("dev-dependencies]")
            || ["[[test]]", "[[example]]", "[[bench]]"].contains(&header.as_str());
        if only_its_own {
            continue;
        }
        out.push_str(&header);
        out.push_str(&body);
    }
    out.trim_start().to_string()
}

/// The carried crates' workspace: the root's package fields, its
/// dependencies with the carried crates' paths beside the manifest and
/// none of the others, and its lints, without the root's comments.
fn workspace_manifest(workspace: &str, crates: &[String]) -> String {
    let members: Vec<String> = crates.iter().map(|name| format!("\"{name}\"")).collect();
    let mut out = format!(
        "# The runtime's crates as this Jadpo carries them, written by `jadpo build` (docs/compiler-runtime.md §2.8); don't edit them.\n\
         [workspace]\n\
         members = [{}]\n\
         resolver = \"3\"\n",
        members.join(", ")
    );
    for (header, body) in sections(workspace) {
        let kept = header == "[workspace.package]"
            || header == "[workspace.dependencies]"
            || header.starts_with("[workspace.lints");
        if !kept {
            continue;
        }
        out.push('\n');
        out.push_str(&header);
        // The root's comments are about the whole workspace.
        for line in body
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
        {
            match line.split_once(" = { path = \"crates/") {
                Some((name, _)) if !crates.iter().any(|carried| carried == name) => {}
                Some(_) => {
                    out.push('\n');
                    out.push_str(&line.replacen("path = \"crates/", "path = \"", 1));
                }
                None => {
                    out.push('\n');
                    out.push_str(line);
                }
            }
        }
        out.push('\n');
    }
    out
}
