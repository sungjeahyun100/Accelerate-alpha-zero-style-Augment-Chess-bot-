//! Embed source provenance so a stale executable cannot be attributed to HEAD.
//! Paths in the resulting receipt are repository-relative group names only.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const SKIP_DIRECTORIES: &[&str] = &[
    "node_modules",
    "dist",
    "dist-server",
    "build",
    "coverage",
    "models",
    "checkpoints",
    ".cache",
    "target",
];

fn collect(
    directory: &Path,
    relative: &str,
    extensions: &[&str],
    files: &mut Vec<(PathBuf, String)>,
) {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("source directory could not be read: {error}"))
        .collect::<Result<Vec<_>, _>>()
        .expect("source directory entries");
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = entry
            .file_name()
            .into_string()
            .expect("UTF-8 source file name");
        let kind = entry.file_type().expect("source file type");
        let lower = name.to_ascii_lowercase();
        if name.starts_with(".env")
            || [
                "credential",
                "service-account",
                "service_account",
                "api-key",
                "api_key",
                "apikey",
                "ssh-key",
                "ssh_key",
                "sshkey",
            ]
            .iter()
            .any(|pattern| lower.contains(pattern))
        {
            continue;
        }
        let next = if relative.is_empty() {
            name.clone()
        } else {
            format!("{relative}/{name}")
        };
        if kind.is_dir() && !SKIP_DIRECTORIES.contains(&name.as_str()) {
            collect(&entry.path(), &next, extensions, files);
        } else if kind.is_file() && extensions.iter().any(|extension| name.ends_with(extension)) {
            files.push((entry.path(), next));
        } else if kind.is_symlink() {
            panic!("source fingerprint refuses symlink {next}");
        }
    }
}

fn source_digest(directory: &Path, extensions: &[&str]) -> Value {
    println!("cargo:rerun-if-changed={}", directory.display());
    let mut files = Vec::new();
    collect(directory, "", extensions, &mut files);
    let mut hash = Sha256::new();
    for (file, name) in &files {
        println!("cargo:rerun-if-changed={}", file.display());
        hash.update(name.as_bytes());
        hash.update([0]);
        hash.update(fs::read(file).expect("source bytes"));
        hash.update([0]);
    }
    json!({"sha256": format!("{:x}", hash.finalize()), "files": files.len()})
}

fn main() {
    let manifest =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let root = manifest.ancestors().nth(4).expect("repository root");
    let sources = json!({
        "engineSource": source_digest(&root.join("projects/augment-chess/engine"), &[".rs", ".toml"]),
        "adapterRuntimeSource": source_digest(&root.join("packages/adapter-runtime"), &[".rs", ".toml"]),
        "gameContractSource": source_digest(&root.join("projects/augment-chess/contracts"), &[".rs", ".toml", ".json", ".js", ".cjs"]),
        "perfNativeSource": source_digest(&manifest, &[".rs", ".toml", ".lock"]),
    });
    let rustc = std::env::var_os("RUSTC").expect("Rust compiler");
    let version = Command::new(rustc)
        .arg("--version")
        .output()
        .expect("Rust compiler version");
    assert!(
        version.status.success(),
        "Rust compiler version request failed"
    );
    println!("cargo:rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
    let flags = std::env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    let provenance = json!({
        "sources": sources,
        "compiler": String::from_utf8(version.stdout).expect("UTF-8 compiler version").trim(),
        "target": std::env::var("TARGET").expect("compilation target"),
        "profile": std::env::var("PROFILE").expect("build profile"),
        "optimizationLevel": std::env::var("OPT_LEVEL").expect("optimization level"),
        "debug": std::env::var("DEBUG").expect("debug information setting"),
        "rustFlagsSha256": format!("{:x}", Sha256::digest(flags.as_bytes())),
        "allocationProbe": std::env::var_os("CARGO_FEATURE_ALLOCATION_PROBE").is_some(),
    });
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo output directory"));
    fs::write(
        output.join("build-provenance.json"),
        serde_json::to_vec(&provenance).expect("provenance JSON"),
    )
    .expect("writing build provenance");
}
